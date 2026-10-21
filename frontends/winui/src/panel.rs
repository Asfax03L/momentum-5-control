//! Panneau rapide : petite fenêtre ouverte par un clic sur l'icône de la
//! zone de notification : batterie, puis ce que l'utilisateur a choisi dans
//! `panneau.txt` (page Paramètres) : contrôle du bruit, curseur, profil
//! personnalisé, quatre scènes au plus ; anti-vent toujours. Client du
//! service comme la fenêtre principale, avec les mêmes règles : rien n'est
//! envoyé si la valeur est déjà celle du casque, le curseur attend 350 ms
//! sans mouvement. Il se ferme quand il perd le focus.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use momentum_client::{Command, Event, Snapshot};
use momentum_core::device::NoiseModeRequest;
use momentum_core::quick_panel::PanelPrefs;
use momentum_core::{hotkeys, paths, viewmodel};
use windows_reactor::*;

use crate::app::{Incoming, spawn_listener, spawn_sender};
use crate::desktop;

const SETTLE: Duration = Duration::from_millis(350);

#[derive(Clone, Debug)]
pub enum PanelMsg {
    Service(Arc<Mutex<Option<Incoming>>>),
    /// Placement de la fenêtre (essai numéro n).
    Place(u32),
    NoiseMode(Option<usize>),
    Level(f64),
    Settled(u64),
    AntiWind(bool),
    Profile(bool),
    Scene(String),
    OpenWindow,
}

pub struct Panel {
    snapshot: Snapshot,
    service_error: Option<String>,
    busy: bool,
    incoming: Arc<Mutex<Receiver<Incoming>>>,
    commands: Sender<Command>,
    pending_level: Option<u8>,
    edits: u64,
    /// Contenu choisi (`panneau.txt`) et scènes montrées.
    prefs: PanelPrefs,
    scenes: Vec<String>,
}

impl Panel {
    fn listen(&self, context: &ComponentContext<Self>) {
        let incoming = Arc::clone(&self.incoming);
        context.spawn_background(move |_| {
            let next = incoming
                .lock()
                .ok()
                .and_then(|rx| rx.recv().ok())
                .unwrap_or_else(|| Incoming::Error("écoute du service arrêtée".into()));
            PanelMsg::Service(Arc::new(Mutex::new(Some(next))))
        });
    }

    fn view_model(&self) -> viewmodel::MainView {
        let name = self
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.name.clone())
            .or_else(|| self.snapshot.device_name.clone());
        let read_only = self.snapshot.read_only.as_ref().map(|_| "lecture seule");
        viewmodel::main_view(
            name.as_deref(),
            self.snapshot.state.as_ref(),
            &[],
            self.service_error.as_deref(),
            read_only,
        )
    }

    fn send(&mut self, command: Command) {
        if self.view_model().writable && self.commands.send(command).is_ok() {
            self.busy = true;
        }
    }
}

impl Component for Panel {
    type Input = ();
    type Message = PanelMsg;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let panel = Self {
            snapshot: Snapshot::default(),
            service_error: None,
            busy: false,
            incoming: spawn_listener(false),
            commands: spawn_sender(),
            pending_level: None,
            edits: 0,
            prefs: PanelPrefs::default(),
            scenes: Vec::new(),
        };
        let mut panel = panel;
        let all: Vec<String> = paths::load_scenes().0.into_iter().map(|s| s.name).collect();
        panel.prefs = paths::load_panel_prefs();
        panel.scenes = panel.prefs.shown_scenes(&all);
        panel.listen(context);
        context.spawn_background(|_| PanelMsg::Place(0));
        panel
    }

    fn update(&mut self, message: PanelMsg, context: &ComponentContext<Self>) {
        let controls = self.view_model().controls;
        match message {
            PanelMsg::Service(cell) => {
                match cell.lock().ok().and_then(|mut c| c.take()) {
                    Some(Incoming::Snapshot(s)) => {
                        self.snapshot = s;
                        self.service_error = None;
                    }
                    Some(Incoming::Event(e)) => {
                        if !matches!(e, Event::Searching | Event::Connected { .. }) {
                            self.busy = false;
                        }
                        self.snapshot.apply(&e);
                        self.service_error = None;
                    }
                    Some(Incoming::Error(reason)) => {
                        self.snapshot = Snapshot::default();
                        self.service_error = Some(reason);
                    }
                    None => {}
                }
                self.listen(context);
            }
            PanelMsg::Place(tries) => {
                if !desktop::place_panel() && tries < 50 {
                    context.spawn_background(move |_| {
                        std::thread::sleep(Duration::from_millis(50));
                        PanelMsg::Place(tries + 1)
                    });
                }
            }
            PanelMsg::NoiseMode(Some(i)) => {
                if i32::try_from(i).ok() != Some(controls.noise_mode) {
                    self.send(Command::SetMode(match i {
                        0 => NoiseModeRequest::Adaptive,
                        1 => NoiseModeRequest::Custom,
                        _ => NoiseModeRequest::Off,
                    }));
                }
            }
            PanelMsg::NoiseMode(None) => {}
            PanelMsg::Level(v) => {
                let level = v.round().clamp(0.0, 100.0) as u8;
                if i32::from(level) != controls.level || self.pending_level.is_some() {
                    self.pending_level = Some(level);
                    self.edits += 1;
                    let edit = self.edits;
                    context.spawn_background(move |_| {
                        std::thread::sleep(SETTLE);
                        PanelMsg::Settled(edit)
                    });
                }
            }
            PanelMsg::Settled(edit) => {
                if edit == self.edits
                    && let Some(level) = self.pending_level.take()
                {
                    self.send(Command::SetLevel(level));
                }
            }
            PanelMsg::AntiWind(on) => {
                if controls.anti_wind != on && self.snapshot.state.is_some() {
                    self.send(Command::SetAntiWind(on));
                }
            }
            PanelMsg::Profile(on) => {
                let current = self
                    .snapshot
                    .state
                    .and_then(|s| hotkeys::profile_toggle(&s));
                // `profile_toggle` donne la valeur à envoyer : l'inverse de
                // l'état actuel.
                if current == Some(on) {
                    self.send(Command::SetSoundProfile(on));
                }
            }
            PanelMsg::Scene(name) => self.send(Command::ApplyScene(name)),
            PanelMsg::OpenWindow => {
                desktop::show();
                desktop::close_panel();
            }
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title(desktop::PANEL_TITLE);
        let p = &self.prefs;
        // Hauteur selon le contenu choisi.
        let height = 230.0
            + if p.noise { 50.0 } else { 0.0 }
            + if p.level { 70.0 } else { 0.0 }
            + if p.profile { 45.0 } else { 0.0 }
            + if self.scenes.is_empty() { 0.0 } else { 50.0 };
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Acrylic)
                .theme(crate::app::window_theme(momentum_core::appearance::load()))
                .client_size(460.0, height),
        );
        let v = self.view_model();
        let c = &v.controls;
        let battery = self.snapshot.state.map_or(0.0, |s| f64::from(s.battery));
        let status = if self.busy {
            "Envoi au casque…".to_string()
        } else if v.connected {
            v.battery_text.clone()
        } else {
            v.connection.clone()
        };
        let state = self.snapshot.state;
        let profile_on = state.is_some_and(|s| hotkeys::profile_toggle(&s) == Some(false));
        let profile_enabled =
            v.writable && state.is_some_and(|s| hotkeys::profile_toggle(&s).is_some());
        let switch = |label: &str, on: bool, enabled: bool, view: View| -> View {
            Grid::new()
                .opacity(if enabled || on { 1.0 } else { 0.6 })
                .columns([GridLength::Star(1.0), GridLength::Auto])
                .children((
                    TextBlock::new()
                        .text(label)
                        .vertical_alignment(VerticalAlignment::Center),
                    Border::new().grid_column(1).content(view),
                ))
        };
        let mut items: Vec<View> = vec![
            TextBlock::new()
                .text(v.title.clone())
                .font_size(18.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .into(),
            TextBlock::new().text(status).opacity(0.8).into(),
            ProgressBar::new()
                .minimum(0.0)
                .maximum(100.0)
                .value(battery)
                .opacity(if v.connected { 1.0 } else { 0.0 })
                .into(),
        ];
        if p.noise {
            items.push(
                RadioButtons::new()
                    .items_source(viewmodel::NOISE_MODES)
                    .max_columns(3)
                    .selected_index(usize::try_from(c.noise_mode).ok())
                    .on_selection_changed(context.callback(PanelMsg::NoiseMode))
                    .into(),
            );
        }
        if p.level {
            items.push(
                TextBlock::new()
                    .text(format!("ANC ↔ transparence : {} %", c.level))
                    .into(),
            );
            items.push(
                Slider::new()
                    .minimum(0.0)
                    .maximum(100.0)
                    // Pas de 1 : une valeur lue ne peut pas être arrondie.
                    .step_frequency(1.0)
                    .value(f64::from(c.level))
                    .is_enabled(v.writable)
                    .on_value_changed(context.callback(PanelMsg::Level))
                    .into(),
            );
        }
        items.push(switch(
            "Anti-vent",
            c.anti_wind,
            v.writable,
            ToggleSwitch::new()
                .is_on(c.anti_wind)
                .is_enabled(v.writable)
                .on_toggled(context.callback(PanelMsg::AntiWind))
                .into(),
        ));
        if p.profile {
            items.push(switch(
                "Profil personnalisé",
                profile_on,
                profile_enabled,
                ToggleSwitch::new()
                    .is_on(profile_on)
                    .is_enabled(profile_enabled)
                    .on_toggled(context.callback(PanelMsg::Profile))
                    .into(),
            ));
        }
        if !self.scenes.is_empty() {
            let buttons = self.scenes.iter().map(|name| {
                let n = name.clone();
                KeyedView::new(
                    format!("scene-{name}"),
                    Button::new()
                        .is_enabled(v.writable && !self.busy)
                        .on_click(context.message(PanelMsg::Scene(n)))
                        .content(name.clone()),
                )
            });
            items.push(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .keyed_children(buttons),
            );
        }
        items.push(
            Button::new()
                .on_click(context.message(PanelMsg::OpenWindow))
                .content("Ouvrir OpenHeadset Control"),
        );
        StackPanel::new().spacing(10.0).margin(16.0).keyed_children(
            items
                .into_iter()
                .enumerate()
                .map(|(i, v)| KeyedView::new(format!("element-{i}"), v)),
        )
    }
}
