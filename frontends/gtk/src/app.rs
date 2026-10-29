//! Fenêtre libadwaita et fils vers le service (ou le casque simulé).

use momentum_core::i18n::{tr, trf};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use adw::prelude::*;
use gtk::glib;
use momentum_client::demo::{self, Demo};
use momentum_client::tracker::Tracker;
use momentum_client::{Client, ClientError, Command, Event, Outcome, Snapshot};
use momentum_core::device::NoiseModeRequest;
use momentum_core::viewmodel::{self, NOISE_MODES};

const APP_ID: &str = "org.openheadset.ControlGnome";
/// Pause avant de recontacter un service injoignable.
const RETRY: Duration = Duration::from_secs(3);
/// Le curseur n'envoie qu'après ce délai sans mouvement (comme WinUI).
const SETTLE: Duration = Duration::from_millis(350);
/// Option de l'essai de l'interface (CI, avec `--demo`).
const SELF_TEST: &str = "--essai-interface";

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
    noise: Vec<gtk::ToggleButton>,
    level: gtk::Scale,
    level_row: adw::ActionRow,
    anti_wind: adw::SwitchRow,
    spatial: adw::SwitchRow,
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
    if let Some(window) = app.active_window() {
        window.present();
        return;
    }
    let widgets = layout(app);
    let state = Rc::new(RefCell::new(State::default()));
    // Vrai pendant que l'état est recopié dans les contrôles : leurs
    // signaux ne doivent alors rien envoyer.
    let updating = Rc::new(Cell::new(false));
    let (commands, incoming) = spawn_service();

    let send = {
        let state = state.clone();
        let widgets = widgets.clone();
        let commands = commands.clone();
        Rc::new(move |command: Command| {
            let session = state.borrow().snapshot.session;
            if commands.send((command.clone(), session)).is_ok() {
                let mut state = state.borrow_mut();
                if matches!(command, Command::SetMode(_)) {
                    state
                        .slider_edits
                        .discard(momentum_client::edits::SliderKey::Level);
                }
                state.slider_edits.submitted(&command);
                state.requests.sent();
                widgets.spinner.set_spinning(true);
            }
        })
    };

    for (index, button) in widgets.noise.iter().enumerate() {
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
    {
        let send = send.clone();
        let updating = updating.clone();
        widgets.level.connect_value_changed(move |scale| {
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
            if changed {
                refresh(&widgets, &state.borrow(), &updating);
            }
            glib::ControlFlow::Continue
        });
    }
    if demo::requested() && std::env::args().any(|a| a == SELF_TEST) {
        self_test(app, &widgets, &state, exit);
    }
    widgets.window.present();
}

/// Fenêtre : barre de titre, bandeau, attente du casque ou page Accueil.
fn layout(app: &adw::Application) -> Widgets {
    let header = adw::HeaderBar::new();
    let spinner = gtk::Spinner::new();
    header.pack_end(&spinner);

    let banner = adw::Banner::new("");
    let waiting = adw::StatusPage::builder()
        .icon_name("network-bluetooth-symbolic")
        .title(tr("En attente du casque"))
        .build();

    let battery_bar = gtk::LevelBar::for_interval(0.0, 100.0);
    battery_bar.set_hexpand(true);
    battery_bar.set_valign(gtk::Align::Center);
    battery_bar.set_width_request(160);
    let battery = adw::ActionRow::builder().title("Batterie").build();
    battery.add_suffix(&battery_bar);
    let headset = adw::PreferencesGroup::builder().title("Casque").build();
    headset.add(&battery);

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
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&banner);
    body.append(&scrolled);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&body));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&header);
    view.set_content(Some(&toasts));

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
        noise,
        level,
        level_row,
        anti_wind,
        spatial,
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
                if let Some(failure) = state.requests.done(*id, outcome) {
                    toast(failure);
                }
                if let Some(notice) = state.requests.take_notice() {
                    toast(notice);
                }
            }
            state.snapshot.apply(&event);
        }
        Incoming::Submitted(id) => {
            if let Some(failure) = state.requests.submitted(id) {
                toast(failure);
            }
        }
        Incoming::Rejected(reason) => toast(state.requests.rejected(&reason)),
        Incoming::ServiceError(reason) => {
            tracing::warn!(%reason, "service momentum-daemon");
            state.snapshot = Snapshot::default();
            state.service_error = Some(reason);
            state.requests.reset();
        }
    }
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
    );
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
                    eprintln!("ESSAI INTERFACE RÉUSSI");
                    app.quit();
                    return glib::ControlFlow::Break;
                }
            }
            _ => return fail(tr("anti-vent simulé non relu à l'affichage")),
        }
        step.set(step.get() + 1);
        glib::ControlFlow::Continue
    });
}
