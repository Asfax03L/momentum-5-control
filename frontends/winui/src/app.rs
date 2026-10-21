//! Composant racine : état du service, messages et vues.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use momentum_client::{Client, ClientError, Command, Event, Snapshot};
use momentum_core::battery_log::{self, BatteryLog};
use momentum_core::device::{
    AUTO_OFF_CHOICES, NoiseModeRequest, PromptMode, check_name, prompt_volume_from_percent,
};
use momentum_core::diagnostic::{self, DiagnosticInput};
use momentum_core::hotkeys::{self, HotkeyAction};
use momentum_core::paths;
use momentum_core::presets::{self, AbCompare, Preset};
use momentum_core::scenes::Scene;
use momentum_core::tray_menu::{self, MenuAction, MenuContext, MenuEntry};
use momentum_core::ui::{IconKind, band_label, icon_rgba, tooltip};
use momentum_core::viewmodel::{self, ConnectionRow};
use momentum_service::desktop::HotkeyThread;
use momentum_service::system::{Autostart, Instance, acquire_named_instance};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_reactor::*;

use crate::desktop;

/// Pause avant de recontacter un service injoignable.
const RETRY: Duration = Duration::from_secs(3);
/// Un curseur n'envoie sa valeur qu'après ce délai sans mouvement.
const SETTLE: Duration = Duration::from_millis(350);
/// Option de démarrage caché (ouverture de session) : l'icône seule.
const HIDDEN_FLAG: &str = "--cache";
/// Nom de l'entrée de lancement au démarrage (clé `Run`).
const AUTOSTART_VALUE: &str = "OpenHeadset Control";
/// Taille de l'icône de la zone de notification.
const ICON_SIZE: u32 = 32;
/// Écart minimal entre deux lancements du service par cette interface.
const DAEMON_RELAUNCH: Duration = Duration::from_secs(30);
/// Zone de la courbe de la batterie, en DIP.
const CHART: (f64, f64) = (720.0, 180.0);
/// Points au plus sur la courbe (un segment WinUI par paire).
const CHART_POINTS: usize = 240;
/// Incidents gardés pour le diagnostic.
const MAX_INCIDENTS: usize = 20;

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Historique de la batterie tenu par le service.
fn load_battery_log(now: u64) -> BatteryLog {
    paths::battery_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| BatteryLog::parse(&text, now))
        .unwrap_or_default()
}

/// Page Batterie : vue commune et courbe à dessiner.
struct BatteryPage {
    view: viewmodel::BatteryView,
    lines: Vec<Vec<(f64, f64)>>,
}

/// Demandes venues du bureau (icône, menu, raccourcis), relayées au fil de
/// l'interface.
#[derive(Debug)]
enum Desk {
    Menu(MenuId),
    Hotkey(HotkeyAction),
    /// Clic gauche sur l'icône : panneau rapide (ou sa fermeture).
    Panel,
}

/// Canal des demandes du bureau : envoi depuis tout fil, réception par une
/// tâche de fond de l'interface.
type DeskChannel = (Sender<Desk>, Arc<Mutex<Receiver<Desk>>>);

static DESK: OnceLock<DeskChannel> = OnceLock::new();

fn desk() -> &'static DeskChannel {
    DESK.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        (tx, Arc::new(Mutex::new(rx)))
    })
}

pub fn run() {
    // Une seule fenêtre : un deuxième lancement montre la première.
    match acquire_named_instance("openheadset-control-show", "openheadset-control-winui") {
        Ok(Instance::AlreadyRunning) => return,
        Ok(Instance::First(request)) => {
            std::thread::spawn(move || {
                while request.wait() {
                    desktop::show();
                }
            });
        }
        Err(e) => eprintln!("instance unique : {e}"),
    }
    if let Err(e) = App::run_component::<Root>(()) {
        eprintln!("WinUI : {e}");
    }
}

/// Lance `momentum-daemon.exe` posé à côté de cette interface, s'il existe
/// (le service se ferme seul s'il est déjà lancé).
fn start_daemon() {
    let Some(exe) = std::env::current_exe()
        .ok()
        .map(|p| p.with_file_name("momentum-daemon.exe"))
        .filter(|p| p.exists())
    else {
        return;
    };
    if let Err(e) = std::process::Command::new(exe).spawn() {
        eprintln!("lancement du service : {e}");
    }
}

fn tray_icon_for(kind: IconKind) -> Option<Icon> {
    Icon::from_rgba(icon_rgba(kind, ICON_SIZE), ICON_SIZE, ICON_SIZE).ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Home,
    Sound,
    Scenes,
    Battery,
    Settings,
    About,
}

impl Page {
    /// Page, étiquette, titre et glyphe Segoe Fluent Icons.
    const ALL: [(Page, &'static str, &'static str, &'static str); 6] = [
        (Page::Home, "accueil", "Accueil", "\u{E80F}"),
        (Page::Sound, "son", "Égaliseur", "\u{E9E9}"),
        (Page::Scenes, "scenes", "Scènes", "\u{E8FD}"),
        (Page::Battery, "batterie", "Batterie", "\u{E83F}"),
        (Page::Settings, "reglages", "Paramètres", "\u{E713}"),
        (Page::About, "apropos", "À propos", "\u{E946}"),
    ];

    fn from_tag(tag: &str) -> Option<Page> {
        Self::ALL
            .iter()
            .find(|(_, t, _, _)| *t == tag)
            .map(|(p, ..)| *p)
    }
}

/// Ce qui arrive du service (thread d'écoute).
pub(crate) enum Incoming {
    Snapshot(Snapshot),
    Event(Event),
    Error(String),
}

/// Action à confirmer avant l'envoi.
#[derive(Clone, Debug, PartialEq)]
enum Confirm {
    MultipointOff,
    Disconnect { index: usize, name: String },
}

#[derive(Clone, Debug)]
enum Msg {
    /// Message du service, puis attente du suivant.
    Service(Arc<Mutex<Option<Incoming>>>),
    Navigate(Option<String>),
    NoiseMode(Option<usize>),
    Level(f64),
    PromptVolume(f64),
    NewName(String),
    Rename,
    AntiWind(bool),
    Spatial(bool),
    HeadTracking(bool),
    Touch(bool),
    Multipoint(bool),
    SmartPause(bool),
    AutoAnswer(bool),
    Comfort(bool),
    BatteryProtection(bool),
    AutoOff(Option<usize>),
    PromptMode(Option<usize>),
    AudioMode(Option<usize>),
    Band(usize, f64),
    Bass(bool),
    Flat,
    Preset(Option<usize>),
    NewPresetName(String),
    SavePreset,
    DeletePreset,
    GainsText(String),
    ApplyGainsText,
    ExportCurve,
    ImportCurve,
    RememberA,
    ToggleAb,
    ExportProfile,
    HotkeyText(usize, String),
    SaveHotkeys,
    ToggleConnection(usize),
    Confirmed,
    Cancelled,
    /// Fin du délai des curseurs (numéro de la dernière modification).
    Settled(u64),
    /// Recherche de la fenêtre Win32 (essai numéro n).
    Attach(u32),
    /// Demande du bureau, puis attente de la suivante.
    Desk(Arc<Mutex<Option<Desk>>>),
    Autostart(bool),
    ApplyScene(usize),
    ReloadScenes,
    EditScenes,
    BatterySpan(Option<usize>),
    CopyDiagnostic,
}

// Le contenu de `Incoming` ne passe qu'une fois d'un thread à l'autre.
impl std::fmt::Debug for Incoming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Incoming::Snapshot(_) => "Snapshot",
            Incoming::Event(_) => "Event",
            Incoming::Error(_) => "Error",
        })
    }
}

struct Root {
    page: Page,
    snapshot: Snapshot,
    service_error: Option<String>,
    busy: bool,
    confirm: Option<Confirm>,
    commands: Sender<Command>,
    incoming: Arc<Mutex<Receiver<Incoming>>>,
    user_presets: Vec<Preset>,
    /// Saisies de la page Égaliseur et résultat de la dernière action.
    new_preset_name: String,
    gains_text: String,
    eq_notice: Option<String>,
    ab: AbCompare,
    /// Curseurs déplacés, pas encore envoyés.
    pending_level: Option<u8>,
    pending_bands: Vec<(u8, i8)>,
    /// Volume des invites (valeur du casque), pas encore envoyé.
    pending_prompt_volume: Option<i8>,
    edits: u64,
    /// Nom saisi, et pourquoi il est refusé avant tout envoi.
    new_name: String,
    name_problem: Option<String>,
    /// Icône de la zone de notification et son menu.
    tray: Option<TrayIcon>,
    tray_kind: Option<IconKind>,
    tray_tooltip: String,
    /// Menu affiché (reconstruit seulement s'il change) et action de
    /// chaque entrée cliquable.
    tray_entries: Vec<MenuEntry>,
    tray_actions: Vec<(MenuId, MenuAction)>,
    hotkeys: Option<HotkeyThread>,
    /// Éditeur des raccourcis : action et touches saisies, et résultat du
    /// dernier enregistrement.
    hotkey_rows: Vec<(HotkeyAction, String)>,
    hotkey_notice: Option<String>,
    /// Raccourcis invalides ou déjà pris par une autre application.
    hotkey_problem: Option<String>,
    start_hidden: bool,
    autostart: bool,
    scenes: Vec<Scene>,
    scene_problems: Vec<String>,
    /// Compte rendu de la dernière scène et réussite complète.
    scene_result: Option<(String, bool)>,
    battery_span: usize,
    battery: Option<BatteryPage>,
    /// Incidents récents : (heure Unix, message).
    incidents: Vec<(u64, String)>,
    diagnostic_copied: Option<bool>,
}

/// Suit le service dans un fil dédié (reconnexion toutes les 3 s) ; avec
/// `launch_daemon`, lance le service s'il manque (au plus toutes les 30 s).
pub(crate) fn spawn_listener(launch_daemon: bool) -> Arc<Mutex<Receiver<Incoming>>> {
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    std::thread::spawn(move || {
        let mut launched: Option<Instant> = None;
        loop {
            match follow_service(&in_tx) {
                Ok(()) => {}
                Err(ClientError::NoService)
                    if launch_daemon && launched.is_none_or(|t| t.elapsed() > DAEMON_RELAUNCH) =>
                {
                    launched = Some(Instant::now());
                    start_daemon();
                    if in_tx
                        .send(Incoming::Error("démarrage du service…".into()))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(e) => {
                    if in_tx.send(Incoming::Error(e.to_string())).is_err() {
                        return;
                    }
                }
            }
            std::thread::sleep(RETRY);
        }
    });
    Arc::new(Mutex::new(in_rx))
}

/// Fil d'envoi des commandes au service. Une erreur d'envoi se voit :
/// l'état n'est pas confirmé et l'écoute signale le service injoignable.
pub(crate) fn spawn_sender() -> Sender<Command> {
    let (tx, rx) = mpsc::channel::<Command>();
    std::thread::spawn(move || {
        for command in rx {
            let _ = Client.send(&command);
        }
    });
    tx
}

/// Lit l'état du service puis relaie ses événements jusqu'à une erreur.
fn follow_service(tx: &Sender<Incoming>) -> Result<(), ClientError> {
    let client = Client::connect()?;
    let events = client.events()?;
    let snapshot = client.snapshot()?;
    if tx.send(Incoming::Snapshot(snapshot)).is_err() {
        return Ok(());
    }
    for event in events {
        if tx.send(Incoming::Event(event)).is_err() {
            break;
        }
    }
    Ok(())
}

impl Root {
    fn note_incident(&mut self, message: String) {
        self.incidents.push((unix_now(), message));
        let excess = self.incidents.len().saturating_sub(MAX_INCIDENTS);
        self.incidents.drain(..excess);
    }

    /// Change de page et relit ce qu'elle affiche depuis les fichiers.
    fn open_page(&mut self, page: Page) {
        self.page = page;
        match page {
            Page::Scenes => self.reload_scenes(),
            Page::Battery => self.reload_battery(),
            Page::About => self.diagnostic_copied = None,
            _ => {}
        }
    }

    fn reload_scenes(&mut self) {
        let (scenes, errors) = paths::load_scenes();
        self.scenes = scenes;
        self.scene_problems = errors;
        self.reload_hotkey_rows();
    }

    /// Remplit l'éditeur des raccourcis depuis `raccourcis.txt`.
    fn reload_hotkey_rows(&mut self) {
        let names: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
        self.hotkey_rows = hotkeys::editor_rows(&paths::load_hotkey_config(), &names);
    }

    /// Écrit `raccourcis.txt` puis réenregistre les raccourcis.
    fn save_hotkeys(&mut self) {
        let text = match hotkeys::build_config(&self.hotkey_rows) {
            Ok(text) => text,
            Err(reason) => {
                self.hotkey_notice = Some(format!("Raccourcis non enregistrés : {reason}."));
                return;
            }
        };
        let written = paths::hotkeys_file()
            .ok_or_else(|| "dossier des données introuvable".to_string())
            .and_then(|path| paths::write_user_file(&path, &text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            self.hotkey_notice = Some(format!("Raccourcis non enregistrés : {e}."));
            return;
        }
        if let Some(thread) = self.hotkeys.take() {
            thread.stop();
        }
        let (thread, errors) = register_hotkeys(&text);
        self.hotkeys = thread;
        self.hotkey_problem =
            (!errors.is_empty()).then(|| format!("Raccourcis clavier : {}", errors.join(" ; ")));
        self.hotkey_notice = Some(if errors.is_empty() {
            "Raccourcis enregistrés et actifs.".to_string()
        } else {
            format!("Raccourcis enregistrés ; refusés : {}.", errors.join(" ; "))
        });
        self.reload_hotkey_rows();
    }

    fn reload_battery(&mut self) {
        let now = unix_now();
        let log = load_battery_log(now);
        let span =
            viewmodel::BATTERY_SPANS[self.battery_span.min(viewmodel::BATTERY_SPANS.len() - 1)].1;
        self.battery = Some(BatteryPage {
            view: viewmodel::battery_view(
                &log,
                now,
                self.battery_span,
                self.snapshot.state.map(|s| s.battery),
            ),
            lines: battery_log::chart_polylines(&log, now, span, CHART_POINTS),
        });
    }

    /// Diagnostic expurgé (adresses, noms d'appareils, dossier personnel).
    fn diagnostic(&self) -> String {
        let s = &self.snapshot;
        let now = unix_now();
        let incidents: Vec<(u64, String)> = self
            .incidents
            .iter()
            .map(|(time, message)| (now.saturating_sub(*time), message.clone()))
            .collect();
        let profile = std::env::var("USERPROFILE").unwrap_or_default();
        let user = std::env::var("USERNAME").unwrap_or_default();
        let system = momentum_service::system::os_version();
        let mut secrets: Vec<&str> = vec![profile.as_str(), user.as_str()];
        secrets.extend(s.device_name.as_deref());
        secrets.extend(s.info.as_ref().and_then(|i| i.name.as_deref()));
        secrets.extend(s.connections.iter().map(|d| d.name.as_str()));
        diagnostic::build(&DiagnosticInput {
            app_version: env!("CARGO_PKG_VERSION"),
            os_version: system.as_deref(),
            connected: s.state.is_some(),
            read_only: s.read_only.as_deref(),
            info: s.info.as_ref(),
            state: s.state.as_ref(),
            eq_bands: s.equalizer.as_ref().map(|e| e.gains.len()),
            paired_devices: s.state.is_some().then_some(s.connections.len()),
            battery_entries: load_battery_log(now).entries().len(),
            incidents: &incidents,
            secrets: &secrets,
        })
    }

    /// Attend la prochaine demande du bureau en tâche de fond.
    fn listen_desk(&self, context: &ComponentContext<Self>) {
        let rx = Arc::clone(&desk().1);
        context.spawn_background(move |_| {
            let next = rx.lock().ok().and_then(|rx| rx.recv().ok());
            Msg::Desk(Arc::new(Mutex::new(next)))
        });
    }

    /// Recopie l'état dans l'icône et son menu (seulement ce qui change).
    fn update_tray(&mut self) {
        let Some(tray) = &self.tray else { return };
        let state = self.snapshot.state;
        let kind = IconKind::from_state(state.as_ref());
        if self.tray_kind != Some(kind) {
            if let Some(icon) = tray_icon_for(kind) {
                let _ = tray.set_icon(Some(icon));
            }
            self.tray_kind = Some(kind);
        }
        let v = self.main_view();
        let problem = self
            .service_error
            .as_deref()
            .or(self.snapshot.problem.as_deref());
        let text = tooltip(self.name().as_deref(), state.as_ref(), problem);
        if text != self.tray_tooltip {
            let _ = tray.set_tooltip(Some(&text));
            self.tray_tooltip = text;
        }
        let name = self.name();
        let scenes: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
        let entries = tray_menu::tray_menu(
            state.as_ref(),
            &MenuContext {
                name: name.as_deref(),
                problem,
                writable: v.writable,
                scenes: &scenes,
                autostart: self.autostart,
                autostart_label: "Lancer au démarrage de Windows",
            },
        );
        if entries != self.tray_entries {
            let menu = Menu::new();
            let mut actions = Vec::new();
            for entry in &entries {
                if let Some(item) = menu_item(entry, &mut actions) {
                    let _ = menu.append(item.as_ref());
                }
            }
            tray.set_menu(Some(Box::new(menu)));
            self.tray_entries = entries;
            self.tray_actions = actions;
        }
    }

    /// Choix du menu de l'icône.
    fn on_menu(&mut self, action: MenuAction) {
        match action {
            MenuAction::OpenWindow => desktop::show(),
            MenuAction::OpenEqualizer => {
                self.open_page(Page::Sound);
                desktop::show();
            }
            MenuAction::Quit => desktop::quit(),
            MenuAction::ReloadScenes => self.reload_scenes(),
            MenuAction::Autostart => {
                let entry = Autostart::named(AUTOSTART_VALUE);
                if let Err(e) = entry.set_with_args(!self.autostart, HIDDEN_FLAG) {
                    eprintln!("lancement au démarrage : {e}");
                }
                self.autostart = entry.is_enabled();
            }
            MenuAction::Scene(name) => {
                if let Some(command) =
                    self.snapshot
                        .state
                        .filter(|_| self.writable())
                        .and_then(|s| {
                            momentum_client::menu::command_for(&MenuAction::Scene(name.clone()), &s)
                        })
                {
                    self.scene_result = Some((format!("Scène « {name} » en cours…"), true));
                    self.send(command);
                }
            }
            action => {
                if let Some(command) = self
                    .snapshot
                    .state
                    .filter(|_| self.writable())
                    .and_then(|s| momentum_client::menu::command_for(&action, &s))
                {
                    self.send(command);
                }
            }
        }
    }

    /// Raccourci global ou choix du menu de l'icône.
    fn on_desk(&mut self, request: Desk, context: &ComponentContext<Self>) {
        let state = self.snapshot.state.filter(|_| self.writable());
        match request {
            Desk::Panel => {
                if !context.open_window(View::component::<crate::panel::Panel>(())) {
                    desktop::show();
                }
            }
            Desk::Menu(id) => {
                if let Some(action) = self
                    .tray_actions
                    .iter()
                    .find(|(item, _)| *item == id)
                    .map(|(_, action)| action.clone())
                {
                    self.on_menu(action);
                }
            }
            Desk::Hotkey(HotkeyAction::OpenWindow) => desktop::show(),
            Desk::Hotkey(HotkeyAction::CycleNoise) => {
                if let Some(s) = state {
                    self.send(Command::SetMode(hotkeys::next_noise_mode(s.noise.mode())));
                }
            }
            Desk::Hotkey(HotkeyAction::ToggleProfile) => {
                if let Some(command) = state
                    .and_then(|s| hotkeys::profile_toggle(&s))
                    .map(Command::SetSoundProfile)
                {
                    self.send(command);
                }
            }
            Desk::Hotkey(HotkeyAction::Scene(name)) => self.send(Command::ApplyScene(name)),
        }
    }

    /// Attend le prochain message du service en tâche de fond.
    fn listen(&self, context: &ComponentContext<Self>) {
        let incoming = Arc::clone(&self.incoming);
        context.spawn_background(move |_| {
            let next = incoming
                .lock()
                .ok()
                .and_then(|rx| rx.recv().ok())
                .unwrap_or_else(|| Incoming::Error("écoute du service arrêtée".into()));
            Msg::Service(Arc::new(Mutex::new(Some(next))))
        });
    }

    fn writable(&self) -> bool {
        self.service_error.is_none()
            && self.snapshot.state.is_some()
            && self.snapshot.read_only.is_none()
    }

    fn send(&mut self, command: Command) {
        if !self.writable() {
            return;
        }
        if self.commands.send(command).is_ok() {
            self.busy = true;
            self.snapshot.problem = None;
        }
    }

    fn all_presets(&self) -> Vec<Preset> {
        let bands = self
            .snapshot
            .equalizer
            .as_ref()
            .map_or(0, |e| e.gains.len());
        let mut all = presets::builtin_presets(bands);
        all.extend(
            self.user_presets
                .iter()
                .filter(|p| p.gains.len() == bands)
                .cloned(),
        );
        all
    }

    /// Préréglage personnel actuellement appliqué (supprimable).
    fn current_user_preset(&self) -> Option<Preset> {
        let presets = self.all_presets();
        let gains = self.snapshot.equalizer.as_ref().map(|e| e.gains.clone());
        let sv = viewmodel::sound_view(
            self.snapshot.state.as_ref(),
            gains.as_deref(),
            &presets,
            true,
        );
        usize::try_from(sv.current_preset)
            .ok()
            .and_then(|i| presets.get(i))
            .filter(|p| !p.builtin)
            .cloned()
    }

    fn rows(&self, writable: bool) -> Vec<ConnectionRow> {
        if self.snapshot.state.is_none() {
            return Vec::new();
        }
        self.snapshot
            .connections
            .iter()
            .map(|d| viewmodel::connection_row(&d.name, d.connected, d.this_device, writable))
            .collect()
    }

    /// Relance le délai des curseurs.
    fn settle_later(&mut self, context: &ComponentContext<Self>) {
        self.edits += 1;
        let edit = self.edits;
        context.spawn_background(move |_| {
            std::thread::sleep(SETTLE);
            Msg::Settled(edit)
        });
    }
}

impl Component for Root {
    type Input = ();
    type Message = Msg;

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let in_rx = spawn_listener(true);
        let tx = spawn_sender();

        // `--page son` : page ouverte au lancement (accueil par défaut).
        let page = std::env::args()
            .skip_while(|a| a != "--page")
            .nth(1)
            .and_then(|tag| Page::from_tag(&tag))
            .unwrap_or(Page::Home);
        // Icône de la zone de notification : clic gauche pour le panneau
        // rapide, menu au clic droit.
        // Menu complet posé par `update_tray` (contenu de `tray_menu`).
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(Menu::new()))
            .with_menu_on_left_click(false)
            .with_tooltip("OpenHeadset Control")
            .with_icon(
                tray_icon_for(IconKind::Disconnected)
                    .unwrap_or_else(|| Icon::from_rgba(vec![0; 4], 1, 1).expect("icône vide")),
            )
            .build()
            .map_err(|e| eprintln!("icône de la zone de notification : {e}"))
            .ok();
        TrayIconEvent::set_event_handler(Some(|event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if desktop::panel_open() {
                    desktop::close_panel();
                } else {
                    let _ = desk().0.send(Desk::Panel);
                }
            }
        }));
        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            let _ = desk().0.send(Desk::Menu(event.id));
        }));

        // Raccourcis globaux : `raccourcis.txt` (page Paramètres).
        let (hotkey_thread, errors) = register_hotkeys(&paths::load_hotkey_config());

        let start_hidden = std::env::args().any(|a| a == HIDDEN_FLAG) && tray.is_some();
        let root = Self {
            page,
            snapshot: Snapshot::default(),
            service_error: None,
            busy: false,
            confirm: None,
            commands: tx,
            incoming: in_rx,
            user_presets: paths::load_user_presets(),
            new_preset_name: String::new(),
            gains_text: String::new(),
            eq_notice: None,
            ab: AbCompare::default(),
            pending_level: None,
            pending_bands: Vec::new(),
            pending_prompt_volume: None,
            edits: 0,
            new_name: String::new(),
            name_problem: None,
            tray,
            tray_kind: None,
            tray_tooltip: String::new(),
            tray_entries: Vec::new(),
            tray_actions: Vec::new(),
            hotkeys: hotkey_thread,
            hotkey_rows: Vec::new(),
            hotkey_notice: None,
            hotkey_problem: (!errors.is_empty())
                .then(|| format!("Raccourcis clavier : {}", errors.join(" ; "))),
            start_hidden,
            autostart: Autostart::named(AUTOSTART_VALUE).is_enabled(),
            scenes: Vec::new(),
            scene_problems: Vec::new(),
            scene_result: None,
            battery_span: 0,
            battery: None,
            incidents: Vec::new(),
            diagnostic_copied: None,
        };
        let mut root = root;
        // Scènes du menu de l'icône.
        root.reload_scenes();
        root.open_page(page);
        root.listen(context);
        root.listen_desk(context);
        context.spawn_background(|_| Msg::Attach(0));
        root
    }

    fn update(&mut self, message: Msg, context: &ComponentContext<Self>) {
        let state = self.snapshot.state;
        match message {
            Msg::Service(cell) => {
                let incoming = cell.lock().ok().and_then(|mut c| c.take());
                match incoming {
                    Some(Incoming::Snapshot(s)) => {
                        self.snapshot = s;
                        self.service_error = None;
                        self.busy = false;
                    }
                    Some(Incoming::Event(e)) => {
                        if !matches!(e, Event::Searching | Event::Connected { .. }) {
                            self.busy = false;
                        }
                        match &e {
                            Event::Lost(r) => self.note_incident(format!("connexion perdue : {r}")),
                            Event::Refused(r) => {
                                self.note_incident(format!("réglage refusé : {r}"))
                            }
                            Event::SceneApplied(report) => {
                                self.scene_result = Some((report.details(), report.is_complete()));
                            }
                            Event::Exported(result) => {
                                self.eq_notice = Some(match result {
                                    Ok(path) => format!(
                                        "Profil exporté dans {} (données auditives personnelles : ne pas le publier).",
                                        path.display()
                                    ),
                                    Err(reason) => {
                                        format!("Export du profil impossible : {reason}.")
                                    }
                                });
                            }
                            // Nouvelle mesure : la courbe suit, page ouverte.
                            Event::State(_) if self.page == Page::Battery => self.reload_battery(),
                            _ => {}
                        }
                        self.snapshot.apply(&e);
                        self.service_error = None;
                    }
                    Some(Incoming::Error(reason)) => {
                        if self.service_error.as_ref() != Some(&reason) {
                            self.note_incident(format!("service : {reason}"));
                        }
                        self.snapshot = Snapshot::default();
                        self.service_error = Some(reason);
                        self.busy = false;
                    }
                    None => {}
                }
                self.listen(context);
            }
            Msg::Navigate(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.open_page(page);
                }
            }
            Msg::ApplyScene(i) => {
                if let Some(name) = self.scenes.get(i).map(|s| s.name.clone()) {
                    self.scene_result = Some((format!("Scène « {name} » en cours…"), true));
                    self.send(Command::ApplyScene(name));
                }
            }
            Msg::ReloadScenes => self.reload_scenes(),
            Msg::EditScenes => {
                // Éditeur associé aux .txt (fichier créé s'il manque).
                self.reload_scenes();
                if let Some(path) = paths::scenes_file()
                    && let Err(e) = std::process::Command::new("explorer.exe").arg(path).spawn()
                {
                    eprintln!("ouverture de scenes.txt : {e}");
                }
            }
            Msg::BatterySpan(Some(i)) => {
                self.battery_span = i;
                self.reload_battery();
            }
            Msg::BatterySpan(None) => {}
            Msg::CopyDiagnostic => {
                self.diagnostic_copied = Some(desktop::copy_text(&self.diagnostic()));
            }
            Msg::NoiseMode(Some(i)) => {
                let current = viewmodel::main_view(None, state.as_ref(), &[], None, None)
                    .controls
                    .noise_mode;
                if i32::try_from(i).ok() != Some(current) {
                    self.send(Command::SetMode(match i {
                        0 => NoiseModeRequest::Adaptive,
                        1 => NoiseModeRequest::Custom,
                        _ => NoiseModeRequest::Off,
                    }));
                }
            }
            Msg::Level(v) => {
                let level = v.round().clamp(0.0, 100.0) as u8;
                let current = viewmodel::main_view(None, state.as_ref(), &[], None, None)
                    .controls
                    .level;
                if i32::from(level) != current || self.pending_level.is_some() {
                    self.pending_level = Some(level);
                    self.settle_later(context);
                }
            }
            Msg::Band(i, db) => {
                let tenths = (db * 10.0).round().clamp(-128.0, 127.0) as i8;
                let current = self
                    .snapshot
                    .equalizer
                    .as_ref()
                    .and_then(|e| e.gains.get(i).copied());
                let band = u8::try_from(i).unwrap_or(u8::MAX);
                let pending = self.pending_bands.iter().any(|(b, _)| *b == band);
                if current.is_some() && (current != Some(tenths) || pending) {
                    self.pending_bands.retain(|(b, _)| *b != band);
                    self.pending_bands.push((band, tenths));
                    self.settle_later(context);
                }
            }
            Msg::PromptVolume(pct) => {
                // Échelle de l'app : plusieurs % donnent la même valeur du
                // casque ; on compare donc les valeurs du casque.
                let value = prompt_volume_from_percent(pct.round().clamp(0.0, 100.0) as u8);
                let current = self.snapshot.info.as_ref().and_then(|i| i.prompt_volume);
                if current.is_some()
                    && (current != Some(value) || self.pending_prompt_volume.is_some())
                {
                    self.pending_prompt_volume = Some(value);
                    self.settle_later(context);
                }
            }
            Msg::NewName(text) => {
                self.new_name = text;
                self.name_problem = None;
            }
            Msg::Rename => {
                let name = self.new_name.trim().to_string();
                let current = self.snapshot.info.as_ref().and_then(|i| i.name.clone());
                match check_name(&name) {
                    Err(e) => self.name_problem = Some(e.to_string()),
                    Ok(()) if current.as_deref() == Some(name.as_str()) => {
                        self.name_problem =
                            Some("nom refusé : c'est déjà le nom du casque".to_string());
                    }
                    Ok(()) => {
                        self.send(Command::SetName(name));
                        self.new_name.clear();
                    }
                }
            }
            Msg::Settled(edit) => {
                if edit == self.edits {
                    if let Some(level) = self.pending_level.take() {
                        self.send(Command::SetLevel(level));
                    }
                    if let Some(volume) = self.pending_prompt_volume.take() {
                        self.send(Command::SetPromptVolume(volume));
                    }
                    for (band, gain) in std::mem::take(&mut self.pending_bands) {
                        self.send(Command::SetEqBand(band, gain));
                    }
                }
            }
            Msg::AntiWind(on) => {
                if state.is_some_and(|s| s.noise.anti_wind != on) {
                    self.send(Command::SetAntiWind(on));
                }
            }
            Msg::Spatial(on) => {
                let c = viewmodel::main_view(None, state.as_ref(), &[], None, None).controls;
                if c.spatial_on != on {
                    self.send(Command::SetSpatial(on));
                }
            }
            Msg::HeadTracking(on) => {
                let c = viewmodel::main_view(None, state.as_ref(), &[], None, None).controls;
                if c.head_tracking_on != on {
                    self.send(Command::SetHeadTracking(on));
                }
            }
            Msg::Touch(on) => {
                let c = viewmodel::main_view(None, state.as_ref(), &[], None, None).controls;
                if c.touch_on != on {
                    self.send(Command::SetTouchControls(on));
                }
            }
            Msg::Multipoint(on) => {
                let current = self.snapshot.info.as_ref().and_then(|i| i.multipoint);
                if current.is_some() && current != Some(on) {
                    if on {
                        self.send(Command::SetMultipoint(true));
                    } else {
                        self.confirm = Some(Confirm::MultipointOff);
                    }
                }
            }
            Msg::SmartPause(on) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if st.smart_pause.on != on {
                    self.send(Command::SetSmartPause(on));
                }
            }
            Msg::AutoAnswer(on) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if st.auto_answer.on != on {
                    self.send(Command::SetAutoAnswer(on));
                }
            }
            Msg::Comfort(on) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if st.comfort_call.on != on {
                    self.send(Command::SetComfortCall(on));
                }
            }
            Msg::BatteryProtection(on) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if st.battery_protection.on != on {
                    self.send(Command::SetBatteryProtection(on));
                }
            }
            Msg::AutoOff(Some(i)) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if i32::try_from(i).ok() != Some(st.auto_off)
                    && let Some(&seconds) = AUTO_OFF_CHOICES.get(i)
                {
                    self.send(Command::SetAutoOff(seconds));
                }
            }
            Msg::PromptMode(Some(i)) => {
                let st =
                    viewmodel::settings_view(state.as_ref(), self.snapshot.info.as_ref(), true);
                if i32::try_from(i).ok() != Some(st.prompt_mode) {
                    self.send(Command::SetPromptMode(if i == 0 {
                        PromptMode::TonesOnly
                    } else {
                        PromptMode::TonesAndVoice
                    }));
                }
            }
            Msg::AudioMode(Some(i)) => {
                let presets = self.all_presets();
                let sv = viewmodel::sound_view(state.as_ref(), None, &presets, true);
                if i32::try_from(i).ok() != Some(sv.audio_mode) {
                    self.send(match i {
                        1 => Command::SetSpeechClarity(true),
                        2 => Command::SetSoundProfile(true),
                        _ => Command::SetSpeechClarity(false),
                    });
                }
            }
            Msg::Bass(on) => {
                if self
                    .snapshot
                    .equalizer
                    .as_ref()
                    .is_some_and(|e| e.bass_boost != on)
                {
                    self.send(Command::SetBassBoost(on));
                }
            }
            Msg::Flat => self.send(Command::SetEqFlat),
            Msg::Preset(Some(i)) => {
                let presets = self.all_presets();
                let gains = self.snapshot.equalizer.as_ref().map(|e| e.gains.clone());
                let sv = viewmodel::sound_view(state.as_ref(), gains.as_deref(), &presets, true);
                if i32::try_from(i).ok() != Some(sv.current_preset)
                    && let Some(p) = presets.get(i)
                {
                    self.send(Command::SetEqGains(p.gains.clone()));
                }
            }
            Msg::NewPresetName(text) => self.new_preset_name = text,
            Msg::SavePreset => {
                if let Some(gains) = self.snapshot.equalizer.as_ref().map(|e| e.gains.clone()) {
                    let name = self.new_preset_name.trim().to_string();
                    self.eq_notice = Some(
                        match presets::upsert(&mut self.user_presets, &name, &gains) {
                            Ok(()) => match paths::store_user_presets(&self.user_presets) {
                                Ok(()) => {
                                    self.new_preset_name.clear();
                                    format!("Préréglage « {name} » enregistré sur ce PC.")
                                }
                                Err(e) => format!("Préréglage non enregistré : {e}."),
                            },
                            Err(reason) => format!("Préréglage non enregistré : {reason}."),
                        },
                    );
                }
            }
            Msg::DeletePreset => {
                if let Some(preset) = self.current_user_preset()
                    && presets::remove(&mut self.user_presets, &preset.name)
                {
                    self.eq_notice = Some(match paths::store_user_presets(&self.user_presets) {
                        Ok(()) => format!("Préréglage « {} » supprimé.", preset.name),
                        Err(e) => format!("Suppression non enregistrée : {e}."),
                    });
                }
            }
            Msg::GainsText(text) => self.gains_text = text,
            Msg::ApplyGainsText => {
                if let Some(eq) = self.snapshot.equalizer.clone() {
                    match presets::parse_gains_db(
                        &self.gains_text,
                        eq.gains.len(),
                        eq.min_gain,
                        eq.max_gain,
                    ) {
                        Ok(gains) if gains == eq.gains => {
                            self.eq_notice = Some("Ce sont déjà les gains du casque.".to_string());
                        }
                        Ok(gains) => {
                            self.send(Command::SetEqGains(gains));
                            self.gains_text.clear();
                            self.eq_notice = None;
                        }
                        Err(reason) => {
                            self.eq_notice = Some(format!("Gains non appliqués : {reason}."));
                        }
                    }
                }
            }
            Msg::ExportCurve => {
                if let Some(eq) = self.snapshot.equalizer.as_ref() {
                    let model = self.snapshot.info.as_ref().and_then(|i| i.model.as_deref());
                    let text = presets::format_curve(model, &eq.gains, eq.bass_boost);
                    self.eq_notice = Some(
                        match paths::export_file("momentum-egaliseur", "txt", unix_now()) {
                            Some(path) => match std::fs::write(&path, text) {
                                Ok(()) => format!("Courbe exportée dans {}.", path.display()),
                                Err(e) => format!("Export de la courbe impossible : {e}."),
                            },
                            None => {
                                "Export impossible : dossier Documents introuvable.".to_string()
                            }
                        },
                    );
                }
            }
            Msg::ImportCurve => {
                if let Some(eq) = self.snapshot.equalizer.clone()
                    && let Some(path) = desktop::pick_curve_file()
                {
                    let model = self.snapshot.info.as_ref().and_then(|i| i.model.clone());
                    let parsed = std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|text| {
                            presets::parse_curve(
                                &text,
                                model.as_deref(),
                                eq.gains.len(),
                                eq.min_gain,
                                eq.max_gain,
                            )
                        });
                    self.eq_notice = Some(match parsed {
                        Ok(curve) => {
                            if curve.gains != eq.gains {
                                self.send(Command::SetEqGains(curve.gains));
                            }
                            if let Some(on) = curve.bass_boost.filter(|&on| on != eq.bass_boost) {
                                self.send(Command::SetBassBoost(on));
                            }
                            format!("Courbe importée depuis {}.", path.display())
                        }
                        Err(reason) => format!("Courbe non importée : {reason}."),
                    });
                }
            }
            Msg::HotkeyText(i, text) => {
                if let Some(row) = self.hotkey_rows.get_mut(i) {
                    row.1 = text;
                }
            }
            Msg::SaveHotkeys => self.save_hotkeys(),
            Msg::ExportProfile => match paths::export_file("momentum-profil", "json", unix_now()) {
                Some(path) => self.send(Command::ExportProfile(path)),
                None => {
                    self.eq_notice =
                        Some("Export impossible : dossier Documents introuvable.".to_string());
                }
            },
            Msg::RememberA => {
                if let Some(eq) = self.snapshot.equalizer.as_ref() {
                    self.ab.remember(&eq.gains);
                    self.eq_notice = Some(
                        "Courbe A mémorisée : modifiez l'égaliseur (B), puis comparez.".to_string(),
                    );
                }
            }
            Msg::ToggleAb => {
                if let Some(eq) = self.snapshot.equalizer.clone()
                    && let Some(gains) = self.ab.toggle(&eq.gains)
                    && gains != eq.gains
                {
                    self.send(Command::SetEqGains(gains));
                }
            }
            Msg::ToggleConnection(index) => {
                let writable = self.writable();
                if let Some(row) = self.rows(writable).get(index).filter(|r| r.can_act) {
                    if row.action == "Déconnecter" {
                        self.confirm = Some(Confirm::Disconnect {
                            index,
                            name: row.name.clone(),
                        });
                    } else {
                        let name = row.name.clone();
                        self.send(Command::ToggleConnection {
                            index,
                            name,
                            connect: true,
                        });
                    }
                }
            }
            Msg::Confirmed => match self.confirm.take() {
                Some(Confirm::MultipointOff) => self.send(Command::SetMultipoint(false)),
                Some(Confirm::Disconnect { index, name }) => self.send(Command::ToggleConnection {
                    index,
                    name,
                    connect: false,
                }),
                None => {}
            },
            Msg::Cancelled => self.confirm = None,
            Msg::Attach(tries) => {
                if desktop::attach() {
                    if self.start_hidden {
                        desktop::hide();
                    }
                    // `--panneau` : panneau rapide ouvert au lancement (essais).
                    if std::env::args().any(|a| a == "--panneau") {
                        let _ = desk().0.send(Desk::Panel);
                    }
                } else if tries < 50 {
                    // La fenêtre n'existe qu'après la première publication.
                    context.spawn_background(move |_| {
                        std::thread::sleep(Duration::from_millis(100));
                        Msg::Attach(tries + 1)
                    });
                }
            }
            Msg::Desk(cell) => {
                if let Some(request) = cell.lock().ok().and_then(|mut c| c.take()) {
                    self.on_desk(request, context);
                }
                self.listen_desk(context);
            }
            Msg::Autostart(on) => {
                if on != self.autostart {
                    let entry = Autostart::named(AUTOSTART_VALUE);
                    if let Err(e) = entry.set_with_args(on, HIDDEN_FLAG) {
                        eprintln!("lancement au démarrage : {e}");
                    }
                    self.autostart = entry.is_enabled();
                }
            }
            Msg::NoiseMode(None)
            | Msg::AutoOff(None)
            | Msg::PromptMode(None)
            | Msg::AudioMode(None)
            | Msg::Preset(None) => {}
        }
        self.update_tray();
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title("OpenHeadset Control");
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Mica)
                .client_size(1100.0, 780.0)
                .constraints(WindowConstraints {
                    min_width: Some(420.0),
                    min_height: Some(480.0),
                    ..WindowConstraints::default()
                }),
        );

        let page = match self.page {
            Page::Home => self.home(context),
            Page::Sound => self.sound(context),
            Page::Scenes => self.scenes_page(context),
            Page::Battery => self.battery_page(context),
            Page::Settings => self.settings(context),
            Page::About => self.about(context),
        };
        let items = Page::ALL.iter().map(|(p, tag, label, glyph)| {
            KeyedView::new(
                *tag,
                NavigationViewItem::new()
                    .tag(*tag)
                    .is_selected(*p == self.page)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, *label),
                        SlotView::new(NavigationViewItemSlot::Icon, FontIcon::new().glyph(*glyph)),
                    ]),
            )
        });
        NavigationView::new()
            .pane_title("OpenHeadset Control")
            // Volet toujours ouvert à gauche (le mode automatique le pose
            // par-dessus le contenu dans une fenêtre moyenne).
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .open_pane_length(220.0)
            .is_settings_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .on_selected_tag_changed(context.callback(Msg::Navigate))
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, items),
                SlotView::new(
                    NavigationViewSlot::Content,
                    ScrollViewer::new().content(
                        StackPanel::new()
                            .spacing(12.0)
                            .margin(Thickness::new(32.0, 24.0, 32.0, 32.0))
                            .max_width(860.0)
                            .children((self.header(context), page)),
                    ),
                ),
            ])
    }
}

/// Carte aux couleurs du thème (fond et contour des cartes Fluent).
/// Entrée de `tray_menu` pour `tray-icon` ; note l'action de chaque
/// entrée cliquable.
fn menu_item(
    entry: &MenuEntry,
    actions: &mut Vec<(MenuId, MenuAction)>,
) -> Option<Box<dyn IsMenuItem>> {
    Some(match entry {
        MenuEntry::Separator => Box::new(PredefinedMenuItem::separator()),
        MenuEntry::Info(label) => Box::new(MenuItem::new(label, false, None)),
        MenuEntry::Item {
            label,
            action,
            enabled,
        } => {
            let item = MenuItem::new(label, *enabled, None);
            actions.push((item.id().clone(), action.clone()));
            Box::new(item)
        }
        MenuEntry::Check {
            label,
            action,
            checked,
            enabled,
        } => {
            let item = CheckMenuItem::new(label, *enabled, *checked, None);
            actions.push((item.id().clone(), action.clone()));
            Box::new(item)
        }
        MenuEntry::Submenu {
            label,
            enabled,
            entries,
        } => {
            let sub = Submenu::new(label, *enabled);
            for e in entries {
                if let Some(item) = menu_item(e, actions) {
                    let _ = sub.append(item.as_ref());
                }
            }
            Box::new(sub)
        }
    })
}

/// Enregistre les raccourcis de `config` ; renvoie le fil qui les écoute
/// et les erreurs (ligne invalide, combinaison déjà prise).
fn register_hotkeys(config: &str) -> (Option<HotkeyThread>, Vec<String>) {
    let (keys, mut errors) = hotkeys::parse_config(config);
    let (thread, refused) = momentum_service::desktop::spawn_hotkeys(keys, |action| match action {
        HotkeyAction::OpenWindow => desktop::show(),
        other => {
            let _ = desk().0.send(Desk::Hotkey(other));
        }
    });
    errors.extend(refused);
    (thread, errors)
}

fn card(title: &str, content: impl Into<View>) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .corner_radius(8.0)
        .padding(16.0)
        .content(
            StackPanel::new().spacing(10.0).children((
                TextBlock::new()
                    .text(title)
                    .font_size(18.0)
                    .font_weight(FontWeight::SEMI_BOLD),
                content.into(),
            )),
        )
}

/// Ligne « libellé, description — interrupteur ».
fn switch_row(
    label: &str,
    description: &str,
    on: bool,
    enabled: bool,
    toggled: Callback<bool>,
) -> View {
    Grid::new()
        .columns([GridLength::Star(1.0), GridLength::Auto])
        .column_spacing(12.0)
        .children((
            StackPanel::new()
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new().text(label),
                    TextBlock::new()
                        .text(description)
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ToggleSwitch::new()
                .is_on(on)
                .is_enabled(enabled)
                .on_toggled(toggled)
                .grid_column(1),
        ))
}

fn index(value: i32) -> Option<usize> {
    usize::try_from(value).ok()
}

impl Root {
    fn name(&self) -> Option<String> {
        self.snapshot
            .info
            .as_ref()
            .and_then(|i| i.name.clone())
            .or_else(|| self.snapshot.device_name.clone())
    }

    fn main_view(&self) -> viewmodel::MainView {
        let read_only = self
            .snapshot
            .read_only
            .as_ref()
            .map(|model| format!("Modèle « {model} » non validé : lecture seule"));
        let problem = self
            .service_error
            .as_ref()
            .map(|e| format!("Service OpenHeadset Control injoignable : {e}"))
            .or_else(|| self.snapshot.problem.clone())
            .or_else(|| self.hotkey_problem.clone());
        viewmodel::main_view(
            self.name().as_deref(),
            self.snapshot.state.as_ref(),
            &[],
            problem.as_deref(),
            read_only.as_deref(),
        )
    }

    /// En-tête commun : nom, état, avertissement, confirmation en attente.
    fn header(&self, context: &mut ViewContext<Self>) -> View {
        let v = self.main_view();
        let status = if self.busy {
            "Envoi au casque…".to_string()
        } else {
            v.connection.clone()
        };
        let confirm = self.confirm.as_ref().map(|c| {
            let (title, text, action) = match c {
                Confirm::MultipointOff => (
                    "Couper le Multipoint ?",
                    "Sans Multipoint, le casque ne garde qu'un appareil : l'autre est déconnecté."
                        .to_string(),
                    "Couper le Multipoint",
                ),
                Confirm::Disconnect { name, .. } => (
                    "Déconnecter cet appareil ?",
                    format!(
                        "Le casque coupe sa liaison avec « {name} ». Il reste appairé : « Connecter » la rétablit."
                    ),
                    "Déconnecter",
                ),
            };
            Border::new()
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::Accent)
                .border_thickness(1.0)
                .corner_radius(8.0)
                .padding(16.0)
                .content(
                    StackPanel::new().spacing(8.0).children((
                        TextBlock::new().text(title).font_weight(FontWeight::SEMI_BOLD),
                        TextBlock::new().text(text).text_wrapping(TextWrapping::Wrap),
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .children((
                                Button::new()
                                    .style(ButtonStyle::Accent)
                                    .on_click(context.message(Msg::Confirmed))
                                    .content(action),
                                Button::new()
                                    .on_click(context.message(Msg::Cancelled))
                                    .content("Annuler"),
                            )),
                    )),
                )
        });
        StackPanel::new().spacing(8.0).children((
            TextBlock::new()
                .text(v.title.clone())
                .font_size(28.0)
                .font_weight(FontWeight::SEMI_BOLD),
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(8.0)
                .children((
                    ProgressRing::new()
                        .is_active(self.busy || !v.connected)
                        .width(16.0)
                        .height(16.0),
                    TextBlock::new().text(status).opacity(0.8),
                )),
            InfoBar::new()
                .is_open(!v.banner.is_empty())
                .is_closable(false)
                .severity(if v.banner_is_warning {
                    InfoBarSeverity::Warning
                } else {
                    InfoBarSeverity::Informational
                })
                .message(v.banner.clone()),
            confirm.map_or_else(View::empty, View::from),
        ))
    }

    fn home(&self, context: &mut ViewContext<Self>) -> View {
        let v = self.main_view();
        if !v.connected {
            return TextBlock::new()
                .text(v.waiting.clone())
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.8)
                .into();
        }
        let c = &v.controls;
        let battery = self.snapshot.state.map_or(0.0, |s| f64::from(s.battery));
        let multipoint = self.snapshot.info.as_ref().and_then(|i| i.multipoint);
        let rows = self.rows(v.writable);
        let devices = rows.iter().enumerate().map(|(i, row)| {
            // Pas de bouton pour ce PC ni en lecture seule.
            let button = Border::new().grid_column(1).content(if row.can_act {
                Button::new()
                    .on_click(context.message(Msg::ToggleConnection(i)))
                    .content(row.action.clone())
            } else {
                View::empty()
            });
            KeyedView::new(
                format!("{i}-{}", row.name),
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .children((
                        StackPanel::new().children((
                            TextBlock::new().text(row.name.clone()),
                            TextBlock::new()
                                .text(row.status.clone())
                                .font_size(12.0)
                                .opacity(0.7),
                        )),
                        button,
                    )),
            )
        });
        StackPanel::new().spacing(12.0).children((
            card(
                "Casque",
                StackPanel::new().spacing(6.0).children((
                    TextBlock::new().text(v.battery_text.clone()),
                    ProgressBar::new()
                        .minimum(0.0)
                        .maximum(100.0)
                        .value(battery),
                    TextBlock::new().text(v.stream.clone()).opacity(0.7),
                )),
            ),
            card(
                "Réduction de bruit",
                StackPanel::new().spacing(10.0).children((
                    RadioButtons::new()
                        .items_source(viewmodel::NOISE_MODES)
                        .max_columns(3)
                        .selected_index(index(c.noise_mode))
                        .on_selection_changed(context.callback(Msg::NoiseMode)),
                    TextBlock::new().text(format!("ANC ↔ transparence : {} %", c.level)),
                    Slider::new()
                        .minimum(0.0)
                        .maximum(100.0)
                        // Pas de 1 : une valeur lue ne peut pas être arrondie (et renvoyée).
                        .step_frequency(1.0)
                        .value(f64::from(c.level))
                        .is_enabled(v.writable)
                        .on_value_changed(context.callback(Msg::Level)),
                    switch_row(
                        "Anti-vent",
                        "Réduit le bruit du vent sur les micros",
                        c.anti_wind,
                        v.writable,
                        context.callback(Msg::AntiWind),
                    ),
                )),
            ),
            card(
                "Fonctionnalités rapides",
                StackPanel::new().spacing(8.0).children((
                    switch_row(
                        "Multipoint",
                        "Deux appareils à la fois",
                        multipoint == Some(true),
                        v.writable && multipoint.is_some(),
                        context.callback(Msg::Multipoint),
                    ),
                    switch_row(
                        "Audio spatial",
                        "Dolby Atmos",
                        c.spatial_on,
                        c.spatial_enabled,
                        context.callback(Msg::Spatial),
                    ),
                    switch_row(
                        "Suivi de tête",
                        "Avec l'audio spatial",
                        c.head_tracking_on,
                        c.head_tracking_enabled,
                        context.callback(Msg::HeadTracking),
                    ),
                    switch_row(
                        "Commandes tactiles",
                        "Gestes sur l'écouteur droit",
                        c.touch_on,
                        c.touch_enabled,
                        context.callback(Msg::Touch),
                    ),
                    switch_row(
                        "Smart Pause",
                        "Pause au retrait du casque",
                        viewmodel::settings_view(
                            self.snapshot.state.as_ref(),
                            self.snapshot.info.as_ref(),
                            v.writable,
                        )
                        .smart_pause
                        .on,
                        v.writable,
                        context.callback(Msg::SmartPause),
                    ),
                )),
            ),
            card(
                "Appareils appairés au casque",
                StackPanel::new().spacing(8.0).keyed_children(devices),
            ),
        ))
    }

    fn sound(&self, context: &mut ViewContext<Self>) -> View {
        let v = self.main_view();
        let Some(eq) = self.snapshot.equalizer.as_ref().filter(|_| v.connected) else {
            return TextBlock::new()
                .text(v.waiting.clone())
                .text_wrapping(TextWrapping::Wrap)
                .opacity(0.8)
                .into();
        };
        let presets = self.all_presets();
        let sv = viewmodel::sound_view(
            self.snapshot.state.as_ref(),
            Some(eq.gains.as_slice()),
            &presets,
            v.writable,
        );
        let count = eq.gains.len();
        let (min, max) = (f64::from(eq.min_gain) / 10.0, f64::from(eq.max_gain) / 10.0);
        let bands = eq.gains.iter().enumerate().map(|(i, &gain)| {
            let db = f64::from(gain) / 10.0;
            KeyedView::new(
                format!("bande-{i}"),
                StackPanel::new().spacing(4.0).width(72.0).children((
                    TextBlock::new()
                        .text(format!("{db:+.1}").replace('.', ","))
                        .horizontal_alignment(HorizontalAlignment::Center),
                    Slider::new()
                        .orientation(Orientation::Vertical)
                        .minimum(min)
                        .maximum(max)
                        // Pas du casque (0,1 dB) : aucune valeur lue n'est arrondie.
                        .step_frequency(0.1)
                        .value(db)
                        .height(180.0)
                        .is_enabled(v.writable)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .on_value_changed(context.callback(move |value| Msg::Band(i, value))),
                    TextBlock::new()
                        .text(band_label(i, count))
                        .font_size(12.0)
                        .text_wrapping(TextWrapping::Wrap)
                        .horizontal_alignment(HorizontalAlignment::Center),
                )),
            )
        });
        StackPanel::new().spacing(12.0).children((
            card(
                "Mode audio",
                RadioButtons::new()
                    .items_source(viewmodel::AUDIO_MODES)
                    .max_columns(3)
                    .selected_index(index(sv.audio_mode))
                    .on_selection_changed(context.callback(Msg::AudioMode)),
            ),
            card(
                &format!("Égaliseur graphique — {count} bandes"),
                StackPanel::new().spacing(10.0).children((
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .keyed_children(bands),
                    switch_row(
                        "Amplification des basses",
                        "",
                        eq.bass_boost,
                        v.writable,
                        context.callback(Msg::Bass),
                    ),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            ComboBox::new()
                                .items_source(sv.preset_names.clone())
                                .selected_index(index(sv.current_preset))
                                .placeholder_text("Préréglage")
                                .is_enabled(v.writable)
                                .on_selection_changed(context.callback(Msg::Preset)),
                            Button::new()
                                .is_enabled(v.writable)
                                .on_click(context.message(Msg::Flat))
                                .content("Réinitialiser"),
                        )),
                )),
            ),
            self.curves_card(v.writable, context),
        ))
    }

    /// Préréglages personnels, saisie des gains, fichiers de courbe et
    /// comparaison A/B.
    fn curves_card(&self, writable: bool, context: &mut ViewContext<Self>) -> View {
        let user = self.current_user_preset();
        let presets = self.all_presets();
        let gains = self.snapshot.equalizer.as_ref().map(|e| e.gains.clone());
        let profile_available = viewmodel::sound_view(
            self.snapshot.state.as_ref(),
            gains.as_deref(),
            &presets,
            writable,
        )
        .profile_available;
        let small = |text: String| {
            TextBlock::new()
                .text(text)
                .font_size(12.0)
                .opacity(0.7)
                .text_wrapping(TextWrapping::Wrap)
        };
        let field_row = |field: View, button: View| {
            Grid::new()
                .columns([GridLength::Star(1.0), GridLength::Auto])
                .column_spacing(8.0)
                .children((field, Border::new().grid_column(1).content(button)))
        };
        card(
            "Préréglages et courbes",
            StackPanel::new().spacing(10.0).children((
                field_row(
                    TextBox::new()
                        .text(self.new_preset_name.clone())
                        .placeholder_text("Nom du préréglage")
                        .is_enabled(writable)
                        .on_text_changed(context.callback(Msg::NewPresetName))
                        .into(),
                    Button::new()
                        .is_enabled(writable && !self.new_preset_name.trim().is_empty())
                        .on_click(context.message(Msg::SavePreset))
                        .content("Enregistrer la courbe"),
                ),
                Button::new()
                    .is_enabled(user.is_some())
                    .on_click(context.message(Msg::DeletePreset))
                    .content(user.map_or_else(
                        || "Supprimer le préréglage (choisir un préréglage personnel)".to_string(),
                        |p| format!("Supprimer « {} »", p.name),
                    )),
                field_row(
                    TextBox::new()
                        .text(self.gains_text.clone())
                        .placeholder_text("Gains en dB : -3,5 ; -1 ; 0 ; …")
                        .is_enabled(writable)
                        .on_text_changed(context.callback(Msg::GainsText))
                        .into(),
                    Button::new()
                        .is_enabled(writable && !self.gains_text.trim().is_empty())
                        .on_click(context.message(Msg::ApplyGainsText))
                        .content("Appliquer"),
                ),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .on_click(context.message(Msg::ExportCurve))
                            .content("Exporter la courbe"),
                        Button::new()
                            .is_enabled(writable)
                            .on_click(context.message(Msg::ImportCurve))
                            .content("Importer une courbe…"),
                    )),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .is_enabled(writable)
                            .on_click(context.message(Msg::RememberA))
                            .content("Mémoriser A"),
                        Button::new()
                            .is_enabled(writable && self.ab.ready())
                            .on_click(context.message(Msg::ToggleAb))
                            .content(self.ab.label()),
                    )),
                Button::new()
                    .is_enabled(profile_available)
                    .on_click(context.message(Msg::ExportProfile))
                    .content("Exporter le profil auditif"),
                small(self.eq_notice.clone().unwrap_or_else(|| {
                    "Préréglages enregistrés sur ce PC (prereglages.txt). Courbe exportée dans \
                     Documents ; une courbe d'un autre modèle ou d'un autre nombre de bandes \
                     est refusée. Le profil auditif (lecture seule, aucune restauration) est \
                     une donnée personnelle : ne pas le publier."
                        .to_string()
                })),
            )),
        )
    }

    fn settings(&self, context: &mut ViewContext<Self>) -> View {
        let app = card(
            "Application",
            StackPanel::new().spacing(8.0).children((switch_row(
                "Lancer à l'ouverture de session",
                "L'icône apparaît près de l'horloge, la fenêtre reste fermée",
                self.autostart,
                true,
                context.callback(Msg::Autostart),
            ),)),
        );
        let hotkey_rows = self
            .hotkey_rows
            .iter()
            .enumerate()
            .map(|(i, (action, spec))| {
                KeyedView::new(
                    format!("raccourci-{}", action.key()),
                    Grid::new()
                        .columns([GridLength::Star(1.0), GridLength::Pixel(220.0)])
                        .column_spacing(12.0)
                        .children((
                            TextBlock::new()
                                .text(action.label())
                                .vertical_alignment(VerticalAlignment::Center),
                            Border::new().grid_column(1).content(
                                TextBox::new()
                                    .text(spec.clone())
                                    .placeholder_text("aucun")
                                    .on_text_changed(
                                        context.callback(move |t| Msg::HotkeyText(i, t)),
                                    ),
                            ),
                        )),
                )
            });
        let shortcuts = card(
            "Raccourcis clavier",
            StackPanel::new().spacing(8.0).children((
                StackPanel::new().spacing(6.0).keyed_children(hotkey_rows),
                Button::new()
                    .on_click(context.message(Msg::SaveHotkeys))
                    .content("Enregistrer les raccourcis"),
                TextBlock::new()
                    .text(self.hotkey_notice.clone().unwrap_or_else(|| {
                        "Ctrl, Alt, Shift ou Win, plus une lettre, un chiffre ou F1 à F24 \
                         (« Ctrl+Alt+N »). Laisser vide pour désactiver. Une combinaison \
                         déjà prise par une autre application est signalée."
                            .to_string()
                    }))
                    .font_size(12.0)
                    .opacity(0.7)
                    .text_wrapping(TextWrapping::Wrap),
            )),
        );
        let v = self.main_view();
        if !v.connected {
            return StackPanel::new().spacing(12.0).children((
                TextBlock::new()
                    .text(v.waiting.clone())
                    .text_wrapping(TextWrapping::Wrap)
                    .opacity(0.8),
                app,
                shortcuts,
            ));
        }
        let st = viewmodel::settings_view(
            self.snapshot.state.as_ref(),
            self.snapshot.info.as_ref(),
            v.writable,
        );
        StackPanel::new().spacing(12.0).children((
            card(
                "Appels",
                StackPanel::new().spacing(8.0).children((
                    switch_row(
                        "Réponse automatique",
                        "Décrocher en mettant le casque",
                        st.auto_answer.on,
                        st.auto_answer.enabled,
                        context.callback(Msg::AutoAnswer),
                    ),
                    switch_row(
                        "Appels confortables",
                        "Votre voix dans le casque pendant un appel",
                        st.comfort_call.on,
                        st.comfort_call.enabled,
                        context.callback(Msg::Comfort),
                    ),
                )),
            ),
            card(
                "Alimentation",
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new().text("Extinction automatique"),
                    ComboBox::new()
                        .items_source(viewmodel::AUTO_OFF_LABELS)
                        .selected_index(index(st.auto_off))
                        .is_enabled(st.auto_off_enabled)
                        .on_selection_changed(context.callback(Msg::AutoOff)),
                    switch_row(
                        "Protection de la batterie",
                        "Limite la charge pour ménager la batterie",
                        st.battery_protection.on,
                        st.battery_protection.enabled,
                        context.callback(Msg::BatteryProtection),
                    ),
                )),
            ),
            card(
                "Invites",
                StackPanel::new().spacing(8.0).children((
                    ComboBox::new()
                        .items_source(viewmodel::PROMPT_MODES)
                        .selected_index(index(st.prompt_mode))
                        .is_enabled(st.prompts_enabled)
                        .on_selection_changed(context.callback(Msg::PromptMode)),
                    TextBlock::new().text(if st.prompt_volume < 0 {
                        "Volume des invites : non lu".to_string()
                    } else {
                        format!("Volume des invites : {} %", st.prompt_volume)
                    }),
                    Slider::new()
                        .minimum(0.0)
                        .maximum(100.0)
                        .step_frequency(1.0)
                        .value(f64::from(st.prompt_volume.max(0)))
                        .is_enabled(st.prompt_volume_enabled)
                        .on_value_changed(context.callback(Msg::PromptVolume)),
                )),
            ),
            card(
                "Nom du casque",
                StackPanel::new().spacing(8.0).children((
                    Grid::new()
                        .columns([GridLength::Star(1.0), GridLength::Auto])
                        .column_spacing(8.0)
                        .children((
                            TextBox::new()
                                .text(self.new_name.clone())
                                .placeholder_text(if st.name.is_empty() {
                                    "non lu".to_string()
                                } else {
                                    st.name.clone()
                                })
                                .is_enabled(st.rename_enabled)
                                .on_text_changed(context.callback(Msg::NewName)),
                            Border::new().grid_column(1).content(
                                Button::new()
                                    .is_enabled(
                                        st.rename_enabled && !self.new_name.trim().is_empty(),
                                    )
                                    .on_click(context.message(Msg::Rename))
                                    .content("Renommer"),
                            ),
                        )),
                    TextBlock::new()
                        .text(self.name_problem.clone().unwrap_or_else(|| {
                            "Lettres sans accent, chiffres et espaces, 32 caractères au plus. \
                             Le système et le téléphone peuvent garder l'ancien nom jusqu'à \
                             la prochaine connexion."
                                .to_string()
                        }))
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            app,
            shortcuts,
        ))
    }

    fn scenes_page(&self, context: &mut ViewContext<Self>) -> View {
        let writable = self.writable() && !self.busy;
        let rows = self.scenes.iter().enumerate().map(|(i, scene)| {
            KeyedView::new(
                format!("scene-{i}-{}", scene.name),
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .column_spacing(12.0)
                    .children((
                        StackPanel::new().children((
                            TextBlock::new()
                                .text(scene.name.clone())
                                .font_weight(FontWeight::SEMI_BOLD),
                            TextBlock::new()
                                .text(viewmodel::scene_preview(scene))
                                .font_size(12.0)
                                .opacity(0.7)
                                .text_wrapping(TextWrapping::Wrap),
                        )),
                        Border::new().grid_column(1).content(
                            Button::new()
                                .is_enabled(writable)
                                .on_click(context.message(Msg::ApplyScene(i)))
                                .content("Appliquer"),
                        ),
                    )),
            )
        });
        let (result, ok) = self.scene_result.clone().unwrap_or_default();
        StackPanel::new().spacing(12.0).children((
            InfoBar::new()
                .is_open(!result.is_empty())
                .is_closable(false)
                .severity(if ok {
                    InfoBarSeverity::Success
                } else {
                    InfoBarSeverity::Warning
                })
                .message(result),
            InfoBar::new()
                .is_open(!self.scene_problems.is_empty())
                .is_closable(false)
                .severity(InfoBarSeverity::Warning)
                .title("scenes.txt")
                .message(self.scene_problems.join("\n")),
            card(
                "Scènes",
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new()
                        .text(
                            "Chaque réglage d'une scène est appliqué puis relu sur le casque, un \
                             par un ; une étape refusée arrête la scène.",
                        )
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                    StackPanel::new().spacing(10.0).keyed_children(rows),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Msg::EditScenes))
                                .content("Modifier scenes.txt"),
                            Button::new()
                                .on_click(context.message(Msg::ReloadScenes))
                                .content("Relire"),
                        )),
                )),
            ),
        ))
    }

    fn battery_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(page) = &self.battery else {
            return TextBlock::new().text("Lecture de l'historique…").into();
        };
        let (width, height) = CHART;
        let segments = page.lines.iter().enumerate().flat_map(|(n, line)| {
            line.windows(2).enumerate().map(move |(k, pair)| {
                let [(x1, y1), (x2, y2)] = [pair[0], pair[1]];
                KeyedView::new(
                    format!("{n}-{k}"),
                    Line::new()
                        .x1(x1 * width)
                        .y1(y1 * height)
                        .x2(x2 * width)
                        .y2(y2 * height)
                        .stroke(ThemeBrush::Accent)
                        .stroke_thickness(2.0),
                )
            })
        });
        // Repères discrets à 100, 50 et 0 %, sous la courbe.
        let guides = [0.0, 0.5, 1.0].into_iter().map(|y| {
            KeyedView::new(
                format!("repere-{y}"),
                Line::new()
                    .x1(0.0)
                    .y1(y * height)
                    .x2(width)
                    .y2(y * height)
                    .stroke(ThemeBrush::CardStroke)
                    .stroke_thickness(1.0),
            )
        });
        let segments = guides.chain(segments);
        let empty = page.lines.iter().all(|l| l.len() < 2);
        let details = page.view.details.iter().enumerate().map(|(i, row)| {
            KeyedView::new(
                i as u64,
                TextBlock::new()
                    .text(format!("{} : {}", row.label, row.value))
                    .font_size(12.0)
                    .opacity(0.7)
                    .text_wrapping(TextWrapping::Wrap),
            )
        });
        StackPanel::new().spacing(12.0).children((
            card(
                &format!("Batterie : {}", page.view.current),
                StackPanel::new().spacing(10.0).children((
                    RadioButtons::new()
                        .items_source(viewmodel::BATTERY_SPANS.iter().map(|(label, _)| *label))
                        .max_columns(3)
                        .selected_index(Some(self.battery_span))
                        .on_selection_changed(context.callback(Msg::BatterySpan)),
                    Border::new()
                        .background(ThemeBrush::SolidBackground)
                        .corner_radius(4.0)
                        .padding(8.0)
                        .content(
                            Canvas::new()
                                .width(width)
                                .height(height)
                                .keyed_children(segments),
                        ),
                    TextBlock::new()
                        .text(if empty {
                            "Aucune mesure sur la période.".to_string()
                        } else {
                            format!(
                                "De {} (à gauche) à maintenant (à droite) ; repères à 100, 50 et 0 %.",
                                page.view.axis_start
                            )
                        })
                        .font_size(12.0)
                        .opacity(0.7),
                    TextBlock::new()
                        .text(page.view.estimate.clone())
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            card("Détails", StackPanel::new().spacing(4.0).keyed_children(details)),
        ))
    }

    fn about(&self, context: &mut ViewContext<Self>) -> View {
        let info = self.snapshot.info.as_ref();
        let firmware = info
            .and_then(|i| i.firmware.clone())
            .unwrap_or_else(|| "non lu".into());
        let model = info.and_then(|i| i.model.clone()).unwrap_or_default();
        let copied = match self.diagnostic_copied {
            Some(true) => "Diagnostic copié dans le presse-papiers.",
            Some(false) => "Copie impossible (presse-papiers occupé) : réessayer.",
            None => "",
        };
        StackPanel::new().spacing(12.0).children((
            card(
                "OpenHeadset Control",
                StackPanel::new().spacing(6.0).children((
                    TextBlock::new().text(format!(
                        "Client WinUI 3, version {}",
                        env!("CARGO_PKG_VERSION")
                    )),
                    TextBlock::new().text(format!("Micrologiciel du casque : {firmware} {model}")),
                    TextBlock::new()
                        .text("Projet non officiel, sans lien avec Sennheiser ni Sonova.")
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text("Licence : MIT OU Apache-2.0")
                        .opacity(0.7),
                )),
            ),
            card(
                "Diagnostic",
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text(
                            "Texte à joindre à un rapport : ni adresse Bluetooth, ni numéro de \
                         série, ni noms d'appareils, ni dossier personnel.",
                        )
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                    Button::new()
                        .on_click(context.message(Msg::CopyDiagnostic))
                        .content("Copier le diagnostic"),
                    TextBlock::new().text(copied),
                )),
            ),
        ))
    }
}
