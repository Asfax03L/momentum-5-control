//! Panneau rapide : petite fenêtre ouverte par un clic sur l'icône de la
//! zone de notification (batterie, contrôle du bruit, anti-vent). Client du
//! service comme la fenêtre principale, avec les mêmes règles : rien n'est
//! envoyé si la valeur est déjà celle du casque, le curseur attend 350 ms
//! sans mouvement. Il se ferme quand il perd le focus.

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use momentum_client::{Command, Event, Snapshot};
use momentum_core::device::NoiseModeRequest;
use momentum_core::viewmodel;
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
        };
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
            PanelMsg::OpenWindow => {
                desktop::show();
                desktop::close_panel();
            }
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title(desktop::PANEL_TITLE);
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Acrylic)
                .client_size(460.0, 350.0),
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
        StackPanel::new().spacing(10.0).margin(16.0).children((
            TextBlock::new()
                .text(v.title.clone())
                .font_size(18.0)
                .font_weight(FontWeight::SEMI_BOLD),
            TextBlock::new().text(status).opacity(0.8),
            ProgressBar::new()
                .minimum(0.0)
                .maximum(100.0)
                .value(battery)
                .opacity(if v.connected { 1.0 } else { 0.0 }),
            RadioButtons::new()
                .items_source(viewmodel::NOISE_MODES)
                .max_columns(3)
                .selected_index(usize::try_from(c.noise_mode).ok())
                .on_selection_changed(context.callback(PanelMsg::NoiseMode)),
            TextBlock::new().text(format!("ANC ↔ transparence : {} %", c.level)),
            Slider::new()
                .minimum(0.0)
                .maximum(100.0)
                // Pas de 1 : une valeur lue ne peut pas être arrondie.
                .step_frequency(1.0)
                .value(f64::from(c.level))
                .is_enabled(v.writable)
                .on_value_changed(context.callback(PanelMsg::Level)),
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Auto])
                .children((
                    TextBlock::new()
                        .text("Anti-vent")
                        .vertical_alignment(VerticalAlignment::Center),
                    ToggleSwitch::new()
                        .is_on(c.anti_wind)
                        .is_enabled(v.writable)
                        .on_toggled(context.callback(PanelMsg::AntiWind))
                        .grid_column(1),
                )),
            Button::new()
                .on_click(context.message(PanelMsg::OpenWindow))
                .content("Ouvrir OpenHeadset Control"),
        ))
    }
}
