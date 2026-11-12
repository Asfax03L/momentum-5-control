//! Fenêtre libadwaita et fils vers le service (ou le casque simulé).

use momentum_core::i18n::{tr, trf};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use momentum_client::demo::{self, Demo};
use momentum_client::sound_check::{Action as GuidedAction, GuidedEq};
use momentum_client::tracker::Tracker;
use momentum_client::{Client, ClientError, Command, Event, Outcome, Snapshot};
use momentum_core::device::NoiseModeRequest;
use momentum_core::viewmodel::{self, NOISE_MODES};

use crate::pages::{Extras, Guided, GuidedSave, Notify, Send};
use crate::panel::Panel;
use crate::shortcuts::{Pressed, Report};

const APP_ID: &str = "org.openheadset.ControlGnome";
/// Pause avant de recontacter un service injoignable.
const RETRY: Duration = Duration::from_secs(3);
/// Le curseur n'envoie qu'après ce délai sans mouvement (comme WinUI).
pub(crate) const SETTLE: Duration = Duration::from_millis(350);
/// Option de l'essai de l'interface (CI, avec `--demo`).
const SELF_TEST: &str = "--essai-interface";
/// Ouvre le panneau rapide (dans l'instance déjà lancée, s'il y en a une).
const PANEL: &str = "--panneau";
/// Lancement à l'ouverture de session : aucune fenêtre, le client reste
/// en arrière-plan pour les raccourcis globaux.
const BACKGROUND: &str = "--cache";

fn has_argument(name: &str) -> bool {
    std::env::args().any(|a| a == name)
}

/// Ce qui arrive du service (ou du casque simulé).
enum Incoming {
    Snapshot(Snapshot),
    Event(Event),
    Submitted(u64),
    Rejected(String),
    ServiceError(String),
}

/// État de la fenêtre : celui du service, suivi événement par événement.
#[derive(Default)]
struct State {
    slider_edits: momentum_client::edits::SliderEdits,
    snapshot: Snapshot,
    service_error: Option<String>,
    requests: Tracker,
    guided: GuidedEq,
}

/// Contrôles mis à jour à chaque changement d'état.
#[derive(Clone)]
struct Widgets {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    spinner: gtk::Spinner,
    waiting: adw::StatusPage,
    pages: gtk::Box,
    battery: adw::ActionRow,
    battery_bar: gtk::LevelBar,
    status: adw::ActionRow,
    noise: Vec<gtk::ToggleButton>,
    level: gtk::Scale,
    level_row: adw::ActionRow,
    anti_wind: adw::SwitchRow,
    spatial: adw::SwitchRow,
    reconnect: gtk::Button,
    extras: Rc<Extras>,
    panel: Panel,
}

pub fn run() -> i32 {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "momentum_core=info,momentum_client=info,momentum_gtk=info".into());
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .init();
    tracing::info!("démarrage du client GTK");

    let app = adw::Application::builder().application_id(APP_ID).build();
    // Instance déjà lancée : `--panneau` lui demande seulement le panneau.
    if has_argument(PANEL)
        && app.register(None::<&gtk::gio::Cancellable>).is_ok()
        && app.is_remote()
    {
        app.activate_action("panneau", None);
        return 0;
    }
    let exit = Rc::new(Cell::new(0));
    let code = exit.clone();
    app.connect_activate(move |app| build(app, code.clone()));
    // Les options (`--demo`…) sont lues par ce client, pas par GTK.
    let program = std::env::args().next().unwrap_or_default();
    let status = app.run_with_args(&[program]);
    if status != glib::ExitCode::SUCCESS {
        return 1;
    }
    exit.get()
}

fn build(app: &adw::Application, exit: Rc<Cell<i32>>) {
    if let Some(window) = app
        .windows()
        .into_iter()
        .find(|w| w.is::<adw::ApplicationWindow>())
    {
        window.present();
        return;
    }
    let widgets = layout(app);
    let background = has_argument(BACKGROUND) && !demo::requested();
    if background {
        // Fermer la fenêtre la cache : les raccourcis restent actifs ; on
        // quitte par le menu ou Ctrl+Q.
        widgets.window.set_hide_on_close(true);
        std::mem::forget(app.hold());
    }
    app_actions(app, &widgets);
    let state = Rc::new(RefCell::new(State::default()));
    // Vrai pendant que l'état est recopié dans les contrôles : leurs
    // signaux ne doivent alors rien envoyer.
    let updating = Rc::new(Cell::new(false));
    let (commands, incoming) = spawn_service();

    // Envoi sans contrôle de l'assistant EQ ; `false` si le fil est fermé.
    let send_raw = {
        let state = state.clone();
        let widgets = widgets.clone();
        let commands = commands.clone();
        Rc::new(move |command: Command| -> bool {
            let session = if command == Command::ReconnectHeadset {
                0
            } else {
                state.borrow().snapshot.session
            };
            if commands.send((command.clone(), session)).is_err() {
                return false;
            }
            let mut state = state.borrow_mut();
            if matches!(command, Command::SetMode(_)) {
                state
                    .slider_edits
                    .discard(momentum_client::edits::SliderKey::Level);
            }
            state.slider_edits.submitted(&command);
            state.requests.sent();
            widgets.spinner.set_spinning(true);
            true
        })
    };
    let send = {
        let state = state.clone();
        let widgets = widgets.clone();
        let send_raw = send_raw.clone();
        Rc::new(move |command: Command| {
            // Pendant l'assistant EQ, seuls ses propres essais partent.
            if state.borrow().guided.active() {
                widgets.toasts.add_toast(adw::Toast::new(tr(
                    "Terminez ou restaurez l'assistant EQ en cours.",
                )));
                widgets.extras.stack.set_visible_child_name("son");
                return;
            }
            send_raw(command);
        })
    };

    for (index, button) in widgets
        .noise
        .iter()
        .chain(widgets.panel.noise.iter())
        .enumerate()
        .map(|(i, b)| (i % NOISE_MODES.len(), b))
    {
        let send = send.clone();
        let updating = updating.clone();
        button.connect_toggled(move |b| {
            if updating.get() || !b.is_active() {
                return;
            }
            send(Command::SetMode(match index {
                0 => NoiseModeRequest::Adaptive,
                1 => NoiseModeRequest::Custom,
                _ => NoiseModeRequest::Off,
            }));
        });
    }
    for level in [&widgets.level, &widgets.panel.level] {
        let send = send.clone();
        let updating = updating.clone();
        level.connect_value_changed(move |scale| {
            if updating.get() {
                return;
            }
            let value = scale.value().round();
            let scale = scale.clone();
            let send = send.clone();
            glib::timeout_add_local_once(SETTLE, move || {
                // Envoyé seulement si le curseur n'a plus bougé.
                if scale.value().round() == value {
                    send(Command::SetLevel(value.clamp(0.0, 100.0) as u8));
                }
            });
        });
    }
    {
        let send = send.clone();
        let updating = updating.clone();
        widgets.anti_wind.connect_active_notify(move |row| {
            if !updating.get() {
                send(Command::SetAntiWind(row.is_active()));
            }
        });
    }
    {
        let send = send.clone();
        let updating = updating.clone();
        widgets.spatial.connect_active_notify(move |row| {
            if !updating.get() {
                send(Command::SetSpatial(row.is_active()));
            }
        });
    }

    {
        let send = send.clone();
        widgets
            .reconnect
            .connect_clicked(move |_| send(Command::ReconnectHeadset));
    }
    {
        let window = widgets.window.clone();
        let panel = widgets.panel.window.clone();
        widgets.panel.open.connect_clicked(move |_| {
            panel.set_visible(false);
            window.present();
        });
    }
    // Contenu du panneau rapide : relu à chaque ouverture et à chaque choix
    // fait dans la page Scènes.
    let apply_panel: Rc<dyn Fn()> = {
        let widgets = widgets.clone();
        let state = state.clone();
        let on_scene: Rc<dyn Fn(String)> = {
            let send = send.clone();
            Rc::new(move |name: String| send(Command::ApplyScene(name)))
        };
        Rc::new(move || {
            let prefs = momentum_core::paths::load_panel_prefs();
            let (scenes, _) = momentum_core::paths::load_scenes();
            let all: Vec<String> = scenes.into_iter().map(|s| s.name).collect();
            let snapshot = &state.borrow().snapshot;
            let writable = snapshot.state.is_some() && snapshot.read_only.is_none();
            widgets.panel.apply_prefs(&prefs, &all, writable, &on_scene);
        })
    };
    apply_panel();
    widgets.extras.on_panel_changed(apply_panel.clone());
    {
        let apply_panel = apply_panel.clone();
        widgets.panel.window.connect_show(move |_| apply_panel());
    }
    {
        let send = send.clone();
        let state = state.clone();
        let updating = updating.clone();
        widgets.panel.profile.connect_active_notify(move |row| {
            if updating.get() {
                return;
            }
            // `profile_toggle` donne la valeur à envoyer : l'inverse de l'état.
            let wanted = {
                let snapshot = &state.borrow().snapshot;
                snapshot
                    .state
                    .filter(|_| snapshot.read_only.is_none())
                    .and_then(|s| momentum_core::hotkeys::profile_toggle(&s))
            };
            if wanted == Some(row.is_active()) {
                send(Command::SetSoundProfile(row.is_active()));
            }
        });
    }
    let pressed = spawn_shortcuts();
    let notify: Notify = {
        let toasts = widgets.toasts.clone();
        Rc::new(move |text: String| toasts.add_toast(adw::Toast::new(&text)))
    };
    let shared: Send = send.clone();
    widgets.extras.connect(&shared, &notify, &updating);
    let guided: Guided = {
        let state = state.clone();
        let widgets = widgets.clone();
        let updating = updating.clone();
        let notify = notify.clone();
        Rc::new(move |action: GuidedAction| {
            let result = {
                let mut st = state.borrow_mut();
                let snapshot = st.snapshot.clone();
                let busy = st.requests.busy();
                let drafts = !st.slider_edits.is_empty();
                if action == GuidedAction::Start
                    && st.guided.view(&snapshot, busy, drafts).can_start
                {
                    let Some(eq) = snapshot.equalizer.as_ref() else {
                        return;
                    };
                    if let Err(error) = momentum_core::paths::save_guided_reference(eq) {
                        drop(st);
                        notify(trf(
                            "Copie initiale impossible : {}. Assistant non démarré.",
                            &[&error],
                        ));
                        return;
                    }
                }
                st.guided.action(action, &snapshot, busy, drafts)
            };
            match result {
                Ok(Some(command)) => {
                    if !send_raw(command) {
                        state.borrow_mut().guided.rejected();
                    }
                }
                Ok(None) => {}
                Err(reason) => notify(reason.to_string()),
            }
            refresh(&widgets, &state.borrow(), &updating);
        })
    };
    let guided_save: GuidedSave = {
        let state = state.clone();
        let notify = notify.clone();
        Rc::new(move |name: &str| {
            let result = {
                let st = state.borrow();
                st.guided
                    .curve_to_save(&st.snapshot, st.requests.busy())
                    .map_err(str::to_string)
                    .and_then(|(gains, topology)| {
                        momentum_core::paths::create_user_preset_for(name, &gains, &topology)
                    })
            };
            match result {
                Ok(list) => {
                    state.borrow_mut().guided.saved();
                    Some(list)
                }
                Err(reason) => {
                    notify(reason);
                    None
                }
            }
        })
    };
    widgets
        .extras
        .connect_guided(&guided, &guided_save, &notify);
    {
        // Fermer pendant l'assistant laisserait une courbe d'essai sur le casque.
        let state = state.clone();
        let extras = widgets.extras.clone();
        widgets.window.connect_close_request(move |_| {
            if state.borrow().guided.active() {
                extras.stack.set_visible_child_name("son");
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
    }
    refresh(&widgets, &state.borrow(), &updating);
    {
        let widgets = widgets.clone();
        let state = state.clone();
        let updating = updating.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            let mut changed = false;
            while let Ok(message) = incoming.try_recv() {
                handle(&widgets, &mut state.borrow_mut(), message);
                changed = true;
            }
            while let Ok(report) = pressed.try_recv() {
                match report {
                    Report::Pressed(action) => {
                        // L'état est relâché avant l'envoi, qui le modifie.
                        let command = on_shortcut(&widgets, &state.borrow(), action);
                        if let Some(command) = command {
                            send(command);
                        }
                    }
                    Report::Ready(session) => widgets.extras.set_shortcut_session(session),
                    Report::Bound(list) => widgets.extras.set_shortcuts(list),
                }
            }
            if changed {
                refresh(&widgets, &state.borrow(), &updating);
            }
            widgets.extras.poll(&notify, &updating);
            glib::ControlFlow::Continue
        });
    }
    if demo::requested() && std::env::args().any(|a| a == SELF_TEST) {
        self_test(app, &widgets, &state, exit);
    }
    // `--page nom` : page d'ouverture (captures d'écran, essais).
    let arguments: Vec<String> = std::env::args().collect();
    if let Some(page) = arguments
        .iter()
        .position(|a| a == "--page")
        .and_then(|i| arguments.get(i + 1))
    {
        widgets.extras.stack.set_visible_child_name(page);
    }
    if has_argument(PANEL) {
        widgets.panel.window.present();
    } else if !background {
        widgets.window.present();
    }
}

/// Actions de l'application : panneau rapide (aussi depuis une autre
/// instance ou l'entrée de menu) et Quitter (Ctrl+Q).
fn app_actions(app: &adw::Application, widgets: &Widgets) {
    let panel = gtk::gio::SimpleAction::new("panneau", None);
    {
        let window = widgets.panel.window.clone();
        panel.connect_activate(move |_, _| window.present());
    }
    app.add_action(&panel);
    let quit = gtk::gio::SimpleAction::new("quitter", None);
    {
        let app = app.clone();
        quit.connect_activate(move |_, _| app.quit());
    }
    app.add_action(&quit);
    app.set_accels_for_action("app.quitter", &["<Control>q"]);
}

/// Raccourcis globaux (portail XDG) dans un fil dédié ; rien en
/// démonstration. Les appuis arrivent par le canal rendu.
fn spawn_shortcuts() -> Receiver<Report> {
    let (tx, rx) = mpsc::channel();
    if demo::requested() {
        return rx;
    }
    let (scenes, _) = momentum_core::paths::load_scenes();
    let names: Vec<String> = scenes.into_iter().map(|s| s.name).collect();
    let spawned = std::thread::Builder::new()
        .name("raccourcis".into())
        .spawn(move || {
            if let Err(e) = crate::shortcuts::run(APP_ID, &names, |report| {
                let _ = tx.send(report);
            }) {
                tracing::warn!(%e, "raccourcis globaux indisponibles");
            }
        });
    if let Err(e) = spawned {
        tracing::error!(%e, "lancement du fil des raccourcis");
    }
    rx
}

/// Appui sur un raccourci : mêmes règles que le client KDE. Rend la
/// demande à envoyer, s'il y en a une.
fn on_shortcut(widgets: &Widgets, state: &State, action: Pressed) -> Option<Command> {
    use momentum_core::hotkeys::{self, HotkeyAction};
    let snapshot = &state.snapshot;
    let headset = snapshot.state.filter(|_| snapshot.read_only.is_none());
    match (action, headset) {
        (Pressed::Panel, _) => {
            widgets.panel.window.present();
            None
        }
        (Pressed::Action(HotkeyAction::OpenWindow), _) => {
            widgets.window.present();
            None
        }
        (Pressed::Action(HotkeyAction::CycleNoise), Some(s)) => {
            Some(Command::SetMode(hotkeys::next_noise_mode(s.noise.mode())))
        }
        (Pressed::Action(HotkeyAction::ToggleProfile), Some(s)) => {
            hotkeys::profile_toggle(&s).map(Command::SetSoundProfile)
        }
        (Pressed::Action(HotkeyAction::Scene(name)), Some(_)) => Some(Command::ApplyScene(name)),
        _ => None,
    }
}

/// Fenêtre : barre de titre, bandeau, attente du casque ou page Accueil.
fn layout(app: &adw::Application) -> Widgets {
    let header = adw::HeaderBar::new();
    let spinner = gtk::Spinner::new();
    header.pack_end(&spinner);
    let reconnect = gtk::Button::with_label(tr("Reconnecter le casque"));
    reconnect.set_halign(gtk::Align::Center);

    let banner = adw::Banner::new("");
    let waiting = adw::StatusPage::builder()
        .icon_name("network-bluetooth-symbolic")
        .title(tr("En attente du casque"))
        .child(&reconnect)
        .build();

    let battery_bar = gtk::LevelBar::for_interval(0.0, 100.0);
    battery_bar.set_hexpand(true);
    battery_bar.set_valign(gtk::Align::Center);
    battery_bar.set_width_request(160);
    let battery = adw::ActionRow::builder().title("Batterie").build();
    battery.add_suffix(&battery_bar);
    // Charge et port, lus sur le casque (comme l'accueil de WinUI).
    let status = adw::ActionRow::builder()
        .title(tr("État du casque"))
        .build();
    let headset = adw::PreferencesGroup::builder().title("Casque").build();
    headset.add(&battery);
    headset.add(&status);

    let modes = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    modes.add_css_class("linked");
    modes.set_valign(gtk::Align::Center);
    let mut noise: Vec<gtk::ToggleButton> = Vec::new();
    for label in NOISE_MODES.map(tr) {
        let button = gtk::ToggleButton::with_label(label);
        if let Some(first) = noise.first() {
            button.set_group(Some(first));
        }
        modes.append(&button);
        noise.push(button);
    }
    let noise_row = adw::ActionRow::builder().title("Mode").build();
    noise_row.add_suffix(&modes);
    let level = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    level.set_hexpand(true);
    level.set_draw_value(true);
    level.set_width_request(200);
    let level_row = adw::ActionRow::builder()
        .title(tr("ANC ↔ transparence"))
        .subtitle(tr("0 = ANC maximal, 100 = transparence maximale"))
        .build();
    level_row.add_suffix(&level);
    let anti_wind = adw::SwitchRow::builder()
        .title("Anti-vent")
        .subtitle(tr("Réduit le bruit du vent sur les micros"))
        .build();
    let noise_group = adw::PreferencesGroup::builder()
        .title(tr("Réduction de bruit"))
        .build();
    noise_group.add(&noise_row);
    noise_group.add(&level_row);
    noise_group.add(&anti_wind);

    let spatial = adw::SwitchRow::builder().title(tr("Audio spatial")).build();
    let features = adw::PreferencesGroup::builder()
        .title(tr("Fonctionnalités"))
        .build();
    features.add(&spatial);

    let pages = gtk::Box::new(gtk::Orientation::Vertical, 24);
    pages.append(&headset);
    pages.append(&noise_group);
    pages.append(&features);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    column.set_margin_top(12);
    column.set_margin_bottom(24);
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.append(&waiting);
    column.append(&pages);
    let clamp = adw::Clamp::builder()
        .maximum_size(640)
        .child(&column)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let extras = Extras::new(&scrolled, &pages);
    let switcher = adw::ViewSwitcherBar::builder()
        .stack(&extras.stack)
        .reveal(true)
        .build();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&banner);
    extras.stack.set_vexpand(true);
    body.append(&extras.stack);
    body.append(&switcher);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&body));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toasts));

    // Menu principal : panneau rapide et Quitter.
    let menu = gtk::gio::Menu::new();
    menu.append(Some(tr("Panneau rapide")), Some("app.panneau"));
    menu.append(Some(tr("Quitter")), Some("app.quitter"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(tr("Menu principal"))
        .build();
    header.pack_end(&menu_button);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(tr("OpenHeadset Control"))
        .icon_name("org.openheadset.control")
        .default_width(560)
        .default_height(720)
        .content(&view)
        .build();
    Widgets {
        window,
        toasts,
        banner,
        spinner,
        waiting,
        pages,
        battery,
        battery_bar,
        status,
        noise,
        level,
        level_row,
        anti_wind,
        spatial,
        reconnect,
        extras,
        panel: Panel::new(app),
    }
}

/// Message du service : état, événement, ou suivi d'une demande.
fn handle(widgets: &Widgets, state: &mut State, message: Incoming) {
    let toast = |text: String| widgets.toasts.add_toast(adw::Toast::new(&text));
    match message {
        Incoming::Snapshot(snapshot) => {
            state.snapshot = snapshot;
            state.service_error = None;
        }
        Incoming::Event(event) => {
            if let Event::Done { id, outcome, .. } = &event {
                // Seuls les échecs des demandes de cette fenêtre s'affichent ;
                // une réussite se voit à l'état relu.
                state.guided.done(*id, outcome);
                if let Some(failure) = state.requests.done(*id, outcome) {
                    toast(failure);
                }
                if let Some(notice) = state.requests.take_notice() {
                    toast(notice);
                }
            }
            if let Event::Exported(result) = &event {
                toast(match result {
                    Ok(path) => trf(
                        "Profil exporté dans {} (données personnelles, fichier privé).",
                        &[&path.display()],
                    ),
                    Err(reason) => trf("Export du profil impossible : {}.", &[&reason]),
                });
            }
            state.snapshot.apply(&event);
        }
        Incoming::Submitted(id) => {
            state.guided.submitted(id);
            if let Some(failure) = state.requests.submitted(id) {
                toast(failure);
            }
        }
        Incoming::Rejected(reason) => {
            state.guided.rejected();
            toast(state.requests.rejected(&reason));
        }
        Incoming::ServiceError(reason) => {
            tracing::warn!(%reason, "service momentum-daemon");
            state.snapshot = Snapshot::default();
            state.service_error = Some(reason);
            state.requests.reset();
        }
    }
    let snapshot = state.snapshot.clone();
    state.guided.sync(&snapshot);
    state.slider_edits.sync(&state.snapshot);
    state.slider_edits.finish_if_idle(state.requests.busy());
}

/// Recopie l'état dans les contrôles (sans renvoyer de commande).
fn refresh(widgets: &Widgets, state: &State, updating: &Cell<bool>) {
    let s = &state.snapshot;
    let name = s
        .info
        .as_ref()
        .and_then(|i| i.name.clone())
        .or_else(|| s.device_name.clone());
    let read_only = s
        .read_only
        .as_ref()
        .map(|reason| trf("Lecture seule : {}", &[&reason]));
    let problem = state
        .service_error
        .as_ref()
        .map(|e| trf("Service OpenHeadset Control injoignable : {}", &[&e]))
        .or_else(|| s.problem.clone());
    let v = viewmodel::main_view(
        name.as_deref(),
        s.state.as_ref(),
        &[],
        problem.as_deref(),
        read_only.as_deref(),
    )
    .with_system_battery(s.system_battery);
    let c = &v.controls;
    updating.set(true);
    widgets.window.set_title(Some(&v.title));
    let banner = if v.banner.is_empty() && demo::requested() {
        tr("Démonstration : données simulées (aucun casque, aucun service)").to_string()
    } else {
        v.banner.clone()
    };
    widgets.banner.set_title(&banner);
    widgets.banner.set_revealed(!banner.is_empty());
    widgets.spinner.set_spinning(state.requests.busy());
    widgets.waiting.set_visible(!v.connected);
    widgets.waiting.set_description(Some(&v.waiting));
    widgets.pages.set_visible(v.connected);
    widgets.battery.set_subtitle(&v.battery_text);
    let summary = momentum_core::home_presentation::headset_summary(s.state.as_ref());
    widgets
        .status
        .set_subtitle(&format!("{} · {}", summary.charge, summary.wearing));
    widgets
        .battery_bar
        .set_value(f64::from(v.battery.unwrap_or(0)));
    for (index, button) in widgets.noise.iter().enumerate() {
        button.set_active(usize::try_from(c.noise_mode).ok() == Some(index));
        button.set_sensitive(v.writable);
    }
    widgets.level.set_value(f64::from(
        state
            .slider_edits
            .value(momentum_client::edits::SliderKey::Level)
            .unwrap_or(c.level as i16),
    ));
    let mut notice = state.slider_edits.notice(s);
    if notice.is_empty() {
        notice = trf("Niveau confirmé : {} %.", &[&c.level]);
    }
    widgets.level_row.set_subtitle(&notice);
    widgets.level.set_sensitive(v.writable);
    widgets.level_row.set_sensitive(v.writable);
    widgets.anti_wind.set_active(c.anti_wind);
    widgets.anti_wind.set_sensitive(v.writable);
    widgets.spatial.set_active(c.spatial_on);
    widgets.spatial.set_sensitive(c.spatial_enabled);
    widgets
        .reconnect
        .set_sensitive(state.service_error.is_none());
    let panel = &widgets.panel;
    panel.waiting.set_text(&v.waiting);
    panel.waiting.set_visible(!v.connected);
    panel.controls.set_visible(v.connected);
    panel.battery.set_subtitle(&v.battery_text);
    panel
        .battery_bar
        .set_value(f64::from(v.battery.unwrap_or(0)));
    for (index, button) in panel.noise.iter().enumerate() {
        button.set_active(usize::try_from(c.noise_mode).ok() == Some(index));
        button.set_sensitive(v.writable);
    }
    panel.level.set_value(widgets.level.value());
    panel.level.set_sensitive(v.writable);
    // `profile_toggle` donne la valeur à envoyer : `false` quand le profil
    // est actif.
    let toggle = s
        .state
        .and_then(|st| momentum_core::hotkeys::profile_toggle(&st));
    panel.profile.set_active(toggle == Some(false));
    panel.profile.set_sensitive(v.writable && toggle.is_some());
    for button in panel.scene_buttons.borrow().iter() {
        button.set_sensitive(v.writable);
    }
    widgets.extras.refresh(s, c, v.writable);
    widgets.extras.refresh_guided(&state.guided.view(
        s,
        state.requests.busy(),
        !state.slider_edits.is_empty(),
    ));
    updating.set(false);
}

/// Fils vers le service : l'un suit ses événements, l'autre transmet les
/// demandes. En démonstration, un seul fil simule le casque.
fn spawn_service() -> (Sender<(Command, u64)>, Receiver<Incoming>) {
    let (commands, requests) = mpsc::channel::<(Command, u64)>();
    let (to_window, incoming) = mpsc::channel::<Incoming>();
    if demo::requested() {
        let mut simulated = Demo::new();
        let _ = to_window.send(Incoming::Snapshot(simulated.snapshot()));
        let spawned = std::thread::Builder::new()
            .name("demo".into())
            .spawn(move || {
                for (id, (command, _)) in (1u64..).zip(requests) {
                    let _ = to_window.send(Incoming::Submitted(id));
                    // Délai d'un vrai casque : l'interface montre l'envoi.
                    std::thread::sleep(Duration::from_millis(450));
                    let mut events = simulated.apply(command);
                    events.push(Event::Done {
                        id,
                        outcome: Outcome::Applied,
                        details: None,
                    });
                    for event in events {
                        let _ = to_window.send(Incoming::Event(event));
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!(%e, "lancement du casque simulé");
        }
        return (commands, incoming);
    }

    let events = to_window.clone();
    let listener = std::thread::Builder::new()
        .name("service-evenements".into())
        .spawn(move || {
            loop {
                if let Err(e) = follow_service(&events)
                    && events.send(Incoming::ServiceError(e.to_string())).is_err()
                {
                    return;
                }
                std::thread::sleep(RETRY);
            }
        });
    if let Err(e) = listener {
        tracing::error!(%e, "lancement du fil des événements");
    }
    let sender = std::thread::Builder::new()
        .name("service-commandes".into())
        .spawn(move || {
            let mut client: Option<Client> = None;
            for (command, session) in requests {
                if client.is_none() {
                    client = Client::connect().ok();
                }
                let result = match &client {
                    Some(c) => c.submit(&command, session),
                    None => Err(ClientError::NoService),
                };
                let message = match result {
                    Ok(id) => Incoming::Submitted(id),
                    // Service joint, demande refusée : pas une panne.
                    Err(ClientError::Service(reason)) => Incoming::Rejected(reason),
                    Err(e) => {
                        client = None;
                        Incoming::ServiceError(e.to_string())
                    }
                };
                if to_window.send(message).is_err() {
                    return;
                }
            }
        });
    if let Err(e) = sender {
        tracing::error!(%e, "lancement du fil des demandes");
    }
    (commands, incoming)
}

/// Lit l'état du service puis relaie ses événements ; ne revient qu'en cas
/// d'erreur ou de fermeture de la fenêtre.
fn follow_service(to_window: &Sender<Incoming>) -> Result<(), ClientError> {
    let client = Client::connect()?;
    // Abonnement avant la lecture de l'état : aucun événement perdu.
    let events = client.events()?;
    let snapshot = client.snapshot()?;
    tracing::info!("connecté au service momentum-daemon");
    if to_window.send(Incoming::Snapshot(snapshot)).is_err() {
        return Ok(());
    }
    for event in events {
        if to_window.send(Incoming::Event(event)).is_err() {
            return Ok(());
        }
    }
    Err(ClientError::Service(
        tr("événements du service interrompus").into(),
    ))
}

/// Gains saisis par l'essai de l'interface, et leur valeur en dixièmes de dB.
const GAINS_TEXT: &str = "1 ; -0,5 ; 0 ; 0 ; 0 ; 0 ; 0 ; -1";
const GAINS_WANTED: [i8; 8] = [10, -5, 0, 0, 0, 0, 0, -10];

/// Essai de l'interface (CI) : l'anti-vent simulé est basculé comme par un
/// clic, puis relu à l'affichage. « ESSAI INTERFACE RÉUSSI » et code 0,
/// sinon code 1.
fn self_test(
    app: &adw::Application,
    widgets: &Widgets,
    state: &Rc<RefCell<State>>,
    exit: Rc<Cell<i32>>,
) {
    let app = app.clone();
    let widgets = widgets.clone();
    let state = state.clone();
    let step = Rc::new(Cell::new(0));
    let before = Rc::new(Cell::new(false));
    glib::timeout_add_local(Duration::from_millis(1500), move || {
        let fail = |message: &str| {
            eprintln!("ESSAI INTERFACE ÉCHEC : {message}");
            exit.set(1);
            app.quit();
            glib::ControlFlow::Break
        };
        match step.get() {
            0 => {
                if !widgets.pages.is_visible() {
                    return fail(tr("page Accueil non affichée"));
                }
                eprintln!("essai interface : page Accueil affichée");
                before.set(widgets.anti_wind.is_active());
                widgets.anti_wind.set_active(!before.get());
            }
            n if n < 6 => {
                let relu = state
                    .borrow()
                    .snapshot
                    .state
                    .is_some_and(|s| s.noise.anti_wind != before.get());
                if relu && widgets.anti_wind.is_active() != before.get() {
                    eprintln!(
                        "essai interface : anti-vent {} → {}",
                        before.get(),
                        !before.get()
                    );
                    for name in [
                        "son",
                        "scenes",
                        "parametres",
                        "batterie",
                        "audio",
                        "apropos",
                    ] {
                        widgets.extras.stack.set_visible_child_name(name);
                        eprintln!("essai interface : page {name} affichée");
                    }
                    widgets.extras.sidetone.set_selected(1);
                    step.set(6);
                    return glib::ControlFlow::Continue;
                }
            }
            6..=10 => {
                if state
                    .borrow()
                    .snapshot
                    .state
                    .is_some_and(|s| s.sidetone == Some(1))
                    && widgets.extras.sidetone.selected() == 1
                {
                    eprintln!("essai interface : sidetone relu à l'affichage");
                    widgets.extras.press_guided_start();
                    step.set(11);
                    return glib::ControlFlow::Continue;
                }
                if step.get() == 10 {
                    return fail(tr("sidetone simulé non relu à l'affichage"));
                }
            }
            11..=15 => {
                if state.borrow().guided.active() {
                    eprintln!("essai interface : assistant EQ démarré");
                    widgets.extras.press_guided_restore();
                    step.set(16);
                    return glib::ControlFlow::Continue;
                }
                if step.get() == 15 {
                    return fail(tr("assistant EQ non démarré"));
                }
            }
            16..=22 => {
                if !state.borrow().guided.active() {
                    eprintln!("essai interface : assistant EQ restauré");
                    widgets.extras.enter_gains(GAINS_TEXT);
                    step.set(23);
                    return glib::ControlFlow::Continue;
                }
                if step.get() == 22 {
                    return fail(tr("restauration non terminée"));
                }
            }
            23..=28 => {
                let relus = state
                    .borrow()
                    .snapshot
                    .equalizer
                    .as_ref()
                    .is_some_and(|eq| eq.gains == GAINS_WANTED);
                if relus {
                    eprintln!("essai interface : gains saisis en texte relus");
                    // Choix d'une notification : enregistré, puis remis.
                    let load = || momentum_core::alerts::NotificationPrefs::load().idle;
                    let before = load();
                    let toggled = widgets.extras.toggle_idle_notification();
                    let stored = load();
                    let restored = widgets.extras.toggle_idle_notification();
                    if toggled == before || stored != toggled || restored != before {
                        return fail(tr("choix de notification non enregistré"));
                    }
                    eprintln!("essai interface : choix de notification enregistré puis remis");
                    // Sauvegarde : écrite dans un fichier temporaire, puis
                    // examinée ; rien n'est importé.
                    let file = std::env::temp_dir()
                        .join(format!("openheadset-essai-{}.json", std::process::id()));
                    let _ = std::fs::remove_file(&file);
                    widgets.extras.export_settings(&file);
                    widgets.extras.preview_settings(&file);
                    let ready = widgets.extras.import_ready();
                    widgets.extras.cancel_settings();
                    let _ = std::fs::remove_file(&file);
                    if !ready {
                        return fail(tr("sauvegarde des réglages non relue"));
                    }
                    eprintln!("essai interface : sauvegarde des réglages écrite puis examinée");
                    // Contenu du panneau : le curseur retiré, puis remis.
                    let shown = widgets.panel.level_box.get_visible();
                    widgets.extras.set_panel_level_shown(!shown);
                    let changed = widgets.panel.level_box.get_visible();
                    widgets.extras.set_panel_level_shown(shown);
                    if changed == shown || widgets.panel.level_box.get_visible() != shown {
                        return fail(tr("contenu du panneau non appliqué"));
                    }
                    eprintln!("essai interface : contenu du panneau appliqué puis remis");
                    eprintln!("ESSAI INTERFACE RÉUSSI");
                    app.quit();
                    return glib::ControlFlow::Break;
                }
                if step.get() == 28 {
                    return fail(tr("gains saisis en texte non relus"));
                }
            }
            _ => return fail(tr("anti-vent simulé non relu à l'affichage")),
        }
        step.set(step.get() + 1);
        glib::ControlFlow::Continue
    });
}
