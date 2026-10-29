//! Composant racine : état du service, messages et vues.

use momentum_core::home_presentation::{self, Availability, availability, headset_summary};
use momentum_core::i18n::{tr, trf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use momentum_client::demo::{self, Demo};
use momentum_client::edits::{SliderEdits, SliderKey};
use momentum_client::tracker::Tracker;
use momentum_client::{Client, ClientError, Command, Event, Outcome, Snapshot};
use momentum_core::alerts::NotificationPrefs;
use momentum_core::assistant::{self, Platform, StepAction};
use momentum_core::audio_check::WindowsAudio;
use momentum_core::battery_log::{self, BatteryLog};
use momentum_core::device::{
    AUTO_OFF_CHOICES, NoiseModeRequest, PromptMode, check_name, prompt_volume_from_percent,
};
use momentum_core::diagnostic::{self, DiagnosticInput};
use momentum_core::hotkeys::{self, HotkeyAction};
use momentum_core::journal;
use momentum_core::paths;
use momentum_core::presets::{self, AbCompare, Preset};
use momentum_core::quick_panel::PanelPrefs;
use momentum_core::scene_editor::{self, SceneEditor};
use momentum_core::scenes::Scene;
use momentum_core::tray_menu::{self, MenuAction, MenuContext, MenuEntry};
use momentum_core::ui::{IconKind, band_label, icon_rgba, tooltip};
use momentum_core::update_check;
use momentum_core::viewmodel::{self, ConnectionRow};
use momentum_service::desktop::HotkeyThread;
use momentum_service::system::{Autostart, Instance, acquire_named_instance};
use tray_icon::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_reactor::*;

use crate::{desktop, taskbar};

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
    /// Autonomie mesurée (bilan, puis une ligne par mode de bruit).
    autonomy: Vec<String>,
    /// Derniers événements du journal, du plus récent au plus ancien.
    journal: Vec<String>,
}

/// Demandes venues du bureau (icône, menu, raccourcis), relayées au fil de
/// l'interface.
#[derive(Debug)]
enum Desk {
    Menu(MenuId),
    Hotkey(HotkeyAction),
    /// Clic gauche sur l'icône : panneau rapide (ou sa fermeture).
    Panel,
    /// Changement de disposition uniquement au franchissement du seuil.
    Layout(bool),
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
    // Entrée de la liste de raccourcis : la scène part au service, sans
    // fenêtre ni deuxième instance.
    if let Some(name) = taskbar::scene_argument() {
        std::process::exit(taskbar::apply_scene(&name, start_daemon));
    }
    // La démonstration reste indépendante de l'application réelle : elle
    // peut être vérifiée sans réveiller ni fermer la fenêtre de l'utilisateur.
    let (show, instance) = if demo::requested() {
        (
            "openheadset-control-demo-show",
            "openheadset-control-demo-winui",
        )
    } else {
        ("openheadset-control-show", "openheadset-control-winui")
    };
    // Une seule fenêtre par mode : un deuxième lancement montre la première.
    match acquire_named_instance(show, instance) {
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
    Audio,
    Settings,
    About,
}

impl Page {
    /// Page, étiquette, titre et glyphe Segoe Fluent Icons.
    const ALL: [(Page, &'static str, &'static str, &'static str); 7] = [
        (Page::Home, "accueil", "Accueil", "\u{E80F}"),
        (Page::Sound, "son", "Égaliseur", "\u{E9E9}"),
        (Page::Scenes, "scenes", "Scènes", "\u{E8FD}"),
        (Page::Battery, "batterie", "Batterie", "\u{E83F}"),
        (Page::Audio, "audio", "Connexion et audio", "\u{E7F5}"),
        (Page::Settings, "reglages", "Paramètres", "\u{E713}"),
        (Page::About, "apropos", "À propos", "\u{E946}"),
    ];

    fn from_tag(tag: &str) -> Option<Page> {
        Self::ALL
            .iter()
            .find(|(_, t, _, _)| *t == tag)
            .map(|(p, ..)| *p)
    }
    /// Repère de la page (`--page`, essais UIA).
    fn tag(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(p, ..)| *p == self)
            .map_or("accueil", |(_, t, ..)| *t)
    }
}

/// Ce qui arrive du service (thread d'écoute).
pub(crate) enum Incoming {
    Snapshot(Snapshot),
    Event(Event),
    Error(String),
    /// Demande acceptée par le service sous ce numéro (résultat à venir dans
    /// `Event::Done`).
    Submitted(u64),
    /// Demande refusée tout de suite par le service (raison).
    Rejected(String),
}

/// Demandes d'une fenêtre, suivies jusqu'au résultat qui porte leur numéro.
#[derive(Default)]
pub(crate) struct Requests(Tracker);

impl Requests {
    pub(crate) fn sent(&mut self) {
        self.0.sent();
    }

    pub(crate) fn busy(&self) -> bool {
        self.0.busy()
    }

    /// Suit un message du service ; rend le message d'échec à montrer quand
    /// une demande de cette fenêtre se termine mal.
    pub(crate) fn on_incoming(&mut self, incoming: &Incoming) -> Option<String> {
        match incoming {
            Incoming::Submitted(id) => self.0.submitted(*id),
            Incoming::Rejected(reason) => Some(self.0.rejected(reason)),
            Incoming::Event(Event::Done { id, outcome, .. }) => self.0.done(*id, outcome),
            Incoming::Error(_) => {
                self.0.reset();
                None
            }
            Incoming::Snapshot(_) | Incoming::Event(_) => None,
        }
    }
}

/// Liste déroulante du formulaire de scène.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SceneField {
    Sound,
    Spatial,
    Noise,
    AntiWind,
    Touch,
    Equalizer,
    Bass,
    Auto,
}

/// Action à confirmer avant l'envoi.
#[derive(Clone, Debug, PartialEq)]
enum Confirm {
    MultipointOff,
    ClearBattery,
    ClearJournal,
    Disconnect {
        index: usize,
        name: String,
        device: String,
    },
}

#[derive(Clone, Debug)]
enum Msg {
    SettingsBattery(bool),
    ExportSettings,
    PreviewSettings,
    ImportSettings,
    CancelSettings,
    /// Message du service, puis attente du suivant.
    Service(Arc<Mutex<Option<Incoming>>>),
    Navigate(Option<String>),
    PaneOpen(bool),
    ReconnectHeadset,
    NoiseMode(Option<usize>),
    Level(f64, u64),
    PromptVolume(f64, u64),
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
    Band(usize, f64, u64),
    Bass(bool),
    Flat,
    Preset(Option<usize>),
    NewPresetName(String),
    SavePreset,
    BindLegacyPresets,
    DeletePreset,
    GainsText(String),
    ApplyGainsText,
    ExportCurve,
    ImportCurve,
    RememberA,
    ToggleAb,
    ExportProfile,
    HotkeyText(usize, String),
    /// Contenu du panneau rapide : bruit, curseur, profil.
    PanelNoise(bool),
    PanelLevel(bool),
    PanelProfile(bool),
    PanelScene(String, bool),
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
    CaptureSceneName(String),
    CaptureSceneReplace(bool),
    CaptureScene,
    /// Éditeur de scènes.
    SceneSelect(Option<usize>),
    SceneNew,
    SceneDuplicate,
    SceneDelete,
    SceneMove(isize),
    SceneName(String),
    SceneLevel(String),
    SceneAutoTime(String),
    /// Liste déroulante du formulaire (champ, choix).
    SceneChoice(SceneField, Option<usize>),
    SceneSave,
    SceneDiscard,
    EditScenes,
    BatterySpan(Option<usize>),
    ReadWindowsAudio,
    /// Thème : 0 clair, 1 sombre.
    Theme(Option<usize>),
    /// Langue choisie : 0 système, 1 français, 2 anglais.
    Language(Option<usize>),
    ExportBattery,
    ClearBattery,
    ReloadBattery,
    CopyDiagnostic,
    /// Rapport de bogue : aperçu du diagnostic, puis formulaire GitHub.
    ReportPreview,
    ReportOpen,
    ReportCancel,
    /// Assistant de premier lancement.
    AssistantOpen,
    AssistantPrevious,
    AssistantNext,
    AssistantClose,
    AssistantBluetooth,
    OpenGuide,
    /// Avis de nouvelle version (désactivé par défaut).
    UpdateNotice(bool),
    /// Notifications au choix (batterie faible, connexion, sans son).
    Notification(NotificationKind, bool),
    ExportJournal,
    ClearJournal,
    ExportApo,
    UpdateCheck,
    UpdateResult(update_check::Outcome),
    OpenRelease,
}

// Le contenu de `Incoming` ne passe qu'une fois d'un thread à l'autre.
impl std::fmt::Debug for Incoming {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Incoming::Snapshot(_) => "Snapshot",
            Incoming::Event(_) => "Event",
            Incoming::Error(_) => "Error",
            Incoming::Submitted(_) => "Submitted",
            Incoming::Rejected(_) => "Rejected",
        })
    }
}

struct Root {
    settings_battery: bool,
    settings_notice: String,
    settings_plan: Option<momentum_core::settings_transfer::ImportPlan>,
    page: Page,
    wide_home: bool,
    nav_open: bool,
    snapshot: Snapshot,
    service_error: Option<String>,
    reconnect_notice: Option<String>,
    busy: bool,
    requests: Requests,
    confirm: Option<Confirm>,
    commands: Sender<(Command, u64)>,
    incoming: Arc<Mutex<Receiver<Incoming>>>,
    user_presets: Vec<Preset>,
    /// Saisies de la page Égaliseur et résultat de la dernière action.
    new_preset_name: String,
    gains_text: String,
    eq_notice: Option<String>,
    ab: AbCompare,
    /// Curseurs déplacés, pas encore envoyés.
    slider_edits: SliderEdits,
    /// Volume des invites (valeur du casque), pas encore envoyé.
    edits: u64,
    /// Nom saisi, et pourquoi il est refusé avant tout envoi.
    new_name: String,
    name_problem: Option<String>,
    /// Icône de la zone de notification et son menu.
    tray: Option<TrayIcon>,
    tray_kind: Option<IconKind>,
    tray_tooltip: String,
    /// Pastille de la barre des tâches posée (batterie, `None` : aucune) ;
    /// `None` au départ : pas encore posée.
    taskbar_badge: Option<Option<u8>>,
    /// Menu affiché (reconstruit seulement s'il change) et action de
    /// chaque entrée cliquable.
    tray_entries: Vec<MenuEntry>,
    tray_actions: Vec<(MenuId, MenuAction)>,
    hotkeys: Option<HotkeyThread>,
    /// Éditeur des raccourcis : action et touches saisies, et résultat du
    /// dernier enregistrement.
    hotkey_rows: Vec<(HotkeyAction, String)>,
    /// Texte de `raccourcis.txt` à la lecture (F14 : pas d'écrasement d'une
    /// modification faite ailleurs).
    hotkeys_loaded: Option<String>,
    hotkey_notice: Option<String>,
    /// Contenu du panneau rapide (`panneau.txt`) et résultat du dernier
    /// enregistrement.
    panel_prefs: PanelPrefs,
    /// Texte de `panneau.txt` à la lecture.
    panel_loaded: Option<String>,
    panel_notice: Option<String>,
    /// Thème choisi (`apparence.txt`) ; `None` : celui du système.
    dark: Option<bool>,
    /// Langue enregistrée (0 système, 1 français, 2 anglais) et message.
    language: usize,
    language_notice: Option<String>,
    /// Raccourcis invalides ou déjà pris par une autre application.
    hotkey_problem: Option<String>,
    start_hidden: bool,
    autostart: bool,
    scenes: Vec<Scene>,
    scene_problems: Vec<String>,
    capture_scene_name: String,
    capture_scene_replace: bool,
    /// Éditeur : brouillon de `scenes.txt`, formulaire affiché (gardé tel
    /// que saisi, même invalide) et résultat du dernier enregistrement.
    scene_editor: SceneEditor,
    scene_form: viewmodel::SceneForm,
    scene_notice: Option<String>,
    /// Compte rendu de la dernière scène et réussite complète.
    scene_result: Option<(String, bool)>,
    battery_span: usize,
    battery: Option<BatteryPage>,
    /// Résultat du dernier export ou effacement de l'historique.
    battery_notice: Option<String>,
    /// Page « Connexion et audio » : heure de chaque lecture, et sorties
    /// audio de Windows (lecture seule).
    control_since: Option<String>,
    connections_at: Option<String>,
    codec_since: Option<String>,
    rate_since: Option<String>,
    windows_audio: Option<Result<WindowsAudio, String>>,
    windows_at: Option<String>,
    /// Incidents récents : (heure Unix, message).
    incidents: Vec<(u64, String)>,
    diagnostic_copied: Option<bool>,
    /// Diagnostic montré avant d'ouvrir le formulaire de bogue : c'est ce
    /// texte-là, et lui seul, qui part dans l'adresse.
    report: Option<String>,
    report_failed: bool,
    /// Étape affichée de l'assistant de premier lancement.
    assistant: Option<usize>,
    /// Avis de nouvelle version : choix, dernière vérification, en cours.
    update_enabled: bool,
    update: Option<update_check::Outcome>,
    update_busy: bool,
    notifications: NotificationPrefs,
}

/// Interrupteur de notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotificationKind {
    LowBattery,
    Connection,
    Idle,
}

/// Suit le service dans un fil dédié (reconnexion toutes les 3 s) ; avec
/// `launch_daemon`, lance le service s'il manque (au plus toutes les 30 s).
/// Mode démonstration : casque simulé partagé par la fenêtre et le panneau,
/// et leurs fils d'écoute.
type DemoHub = Mutex<(Demo, Vec<Sender<Incoming>>)>;

fn demo_hub() -> &'static DemoHub {
    static HUB: OnceLock<DemoHub> = OnceLock::new();
    HUB.get_or_init(|| Mutex::new((Demo::new(), Vec::new())))
}

/// Récepteur des messages du service, partagé avec le fil d'écoute.
pub(crate) type IncomingRx = Arc<Mutex<Receiver<Incoming>>>;

fn spawn_listener(launch_daemon: bool) -> (Sender<Incoming>, IncomingRx) {
    let (in_tx, in_rx) = mpsc::channel::<Incoming>();
    let feedback = in_tx.clone();
    if demo::requested() {
        if let Ok(mut hub) = demo_hub().lock() {
            let _ = in_tx.send(Incoming::Snapshot(hub.0.snapshot()));
            hub.1.push(in_tx);
        }
        return (feedback, Arc::new(Mutex::new(in_rx)));
    }
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
                        .send(Incoming::Error(tr("démarrage du service…").into()))
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
    (feedback, Arc::new(Mutex::new(in_rx)))
}

/// Écoute du service et fil d'envoi des demandes, reliés : le numéro de
/// chaque demande (ou son refus immédiat) revient par le même récepteur que
/// les événements. Une demande porte la connexion vue par l'interface.
pub(crate) fn spawn_service(launch_daemon: bool) -> (IncomingRx, Sender<(Command, u64)>) {
    let (feedback, incoming) = spawn_listener(launch_daemon);
    (incoming, spawn_sender(feedback))
}

/// Fil d'envoi des demandes au service.
fn spawn_sender(feedback: Sender<Incoming>) -> Sender<(Command, u64)> {
    let (tx, rx) = mpsc::channel::<(Command, u64)>();
    if demo::requested() {
        std::thread::spawn(move || {
            let mut last_id = 0;
            for (command, _) in rx {
                last_id += 1;
                if feedback.send(Incoming::Submitted(last_id)).is_err() {
                    return;
                }
                // Délai d'un vrai casque : l'interface montre son envoi.
                std::thread::sleep(Duration::from_millis(450));
                if let Ok(mut hub) = demo_hub().lock() {
                    let outcome = if command == Command::ReconnectHeadset {
                        Outcome::SystemRequested(
                            tr("Démonstration : reconnexion simulée, aucun accès Bluetooth.")
                                .into(),
                        )
                    } else {
                        Outcome::Applied
                    };
                    let mut events = hub.0.apply(command);
                    events.push(Event::Done {
                        id: last_id,
                        outcome,
                        details: None,
                    });
                    hub.1.retain(|sub| {
                        events
                            .iter()
                            .all(|e| sub.send(Incoming::Event(e.clone())).is_ok())
                    });
                }
            }
        });
        return tx;
    }
    std::thread::spawn(move || {
        for (command, session) in rx {
            let reply = match Client.submit(&command, session) {
                Ok(id) => Incoming::Submitted(id),
                Err(e) => Incoming::Rejected(e.to_string()),
            };
            if feedback.send(reply).is_err() {
                return;
            }
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
            Page::Audio => self.read_windows_audio(),
            Page::About => {
                self.diagnostic_copied = None;
                self.report = None;
                self.report_failed = false;
            }
            _ => {}
        }
    }

    fn reload_scenes(&mut self) {
        let (scenes, errors) = paths::load_scenes();
        self.scenes = scenes;
        self.scene_problems = errors;
        let names: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
        if let Err(e) = taskbar::update_jump_list(&names) {
            eprintln!("liste de raccourcis : {e}");
        }
        self.reload_hotkey_rows();
        // Une modification en cours dans l'éditeur n'est pas perdue.
        if !self.scene_editor.dirty() {
            self.reload_scene_editor();
        }
    }

    /// Rouvre l'éditeur sur `scenes.txt` (modifications abandonnées).
    fn reload_scene_editor(&mut self) {
        let text = paths::scenes_file().and_then(|p| std::fs::read_to_string(p).ok());
        let selected = self.scene_editor.selected();
        self.scene_editor = SceneEditor::load(text, paths::scene_presets());
        self.scene_editor
            .select(selected.min(self.scene_editor.names().len().saturating_sub(1)));
        self.scene_form = self.scene_editor.form();
    }

    /// Le formulaire a changé : le brouillon suit s'il est valide.
    fn scene_form_changed(&mut self) {
        self.scene_editor.set_form(&self.scene_form);
        self.scene_notice = None;
    }

    /// La liste a changé : le formulaire montre la scène choisie.
    fn scene_list_changed(&mut self) {
        self.scene_form = self.scene_editor.form();
        self.scene_notice = None;
    }

    /// Enregistre le brouillon ; raccourcis et panneau suivent les
    /// renommages.
    fn save_scenes(&mut self) {
        match scene_editor::save(&self.scene_editor) {
            Ok((message, hotkeys_changed)) => {
                if hotkeys_changed {
                    if let Some(thread) = self.hotkeys.take() {
                        thread.stop();
                    }
                    let (thread, _) = register_hotkeys(&paths::load_hotkey_config());
                    self.hotkeys = thread;
                }
                self.panel_prefs = paths::load_panel_prefs();
                self.panel_loaded = paths::read_panel_text();
                self.reload_scene_editor();
                self.reload_scenes();
                self.scene_notice = Some(message);
            }
            Err(reason) => self.scene_notice = Some(trf("Non enregistré : {}", &[&reason])),
        }
    }

    /// Remplit l'éditeur des raccourcis depuis `raccourcis.txt`.
    fn reload_hotkey_rows(&mut self) {
        let names: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
        let text = paths::load_hotkey_config();
        self.hotkey_rows = hotkeys::editor_rows(&text, &names);
        self.hotkeys_loaded = Some(text);
    }

    /// Écrit `panneau.txt` (pris en compte à la prochaine ouverture du
    /// panneau).
    fn store_panel_prefs(&mut self) {
        let text = self.panel_prefs.to_text();
        let written = paths::panel_file()
            .ok_or_else(|| tr("dossier des données introuvable").to_string())
            .and_then(|path| paths::write_if_unchanged(&path, self.panel_loaded.as_deref(), &text));
        self.panel_notice = Some(match written {
            Ok(()) => {
                self.panel_loaded = Some(text);
                tr("Panneau enregistré : pris en compte à sa prochaine ouverture.").to_string()
            }
            Err(e) => {
                // Relu : l'écran montre le fichier tel qu'il est.
                self.panel_prefs = paths::load_panel_prefs();
                self.panel_loaded = paths::read_panel_text();
                trf("Panneau non enregistré : {}.", &[&e])
            }
        });
    }

    /// Écrit `raccourcis.txt` puis réenregistre les raccourcis.
    fn save_hotkeys(&mut self) {
        let text = match hotkeys::build_config(&self.hotkey_rows) {
            Ok(text) => text,
            Err(reason) => {
                self.hotkey_notice = Some(trf("Raccourcis non enregistrés : {}.", &[&reason]));
                return;
            }
        };
        let written = paths::hotkeys_file()
            .ok_or_else(|| tr("dossier des données introuvable").to_string())
            .and_then(|path| {
                paths::write_if_unchanged(&path, self.hotkeys_loaded.as_deref(), &text)
            });
        if let Err(e) = written {
            self.hotkey_notice = Some(trf("Raccourcis non enregistrés : {}.", &[&e]));
            self.reload_hotkey_rows();
            return;
        }
        if let Some(thread) = self.hotkeys.take() {
            thread.stop();
        }
        let (thread, errors) = register_hotkeys(&text);
        self.hotkeys = thread;
        self.hotkey_problem =
            (!errors.is_empty()).then(|| trf("Raccourcis clavier : {}", &[&(errors.join(" ; "))]));
        self.hotkey_notice = Some(if errors.is_empty() {
            tr("Raccourcis enregistrés et actifs.").to_string()
        } else {
            trf(
                "Raccourcis enregistrés ; refusés : {}.",
                &[&(errors.join(" ; "))],
            )
        });
        self.reload_hotkey_rows();
    }

    /// Heure de chaque lecture, pour la page « Connexion et audio ».
    fn note_reading_times(&mut self, event: &Event) {
        let now = momentum_service::system::clock;
        match event {
            Event::Searching | Event::Lost(_) => self.control_since = None,
            Event::Connected { .. } => self.control_since = Some(now()),
            Event::Connections(_) => self.connections_at = Some(now()),
            Event::State(s) => {
                let before = self.snapshot.state;
                if before.is_none_or(|b| b.codec != s.codec) {
                    self.codec_since = Some(now());
                }
                if before.is_none_or(|b| b.sample_rate_hz != s.sample_rate_hz) {
                    self.rate_since = Some(now());
                }
            }
            _ => {}
        }
    }

    /// Lit les sorties audio de Windows (lecture seule).
    fn read_windows_audio(&mut self) {
        self.windows_audio = Some(momentum_service::audio_output::read(self.name().as_deref()));
        self.windows_at = Some(momentum_service::system::clock());
    }

    fn reload_battery(&mut self) {
        let now = unix_now();
        let log = load_battery_log(now);
        let span =
            viewmodel::BATTERY_SPANS[self.battery_span.min(viewmodel::BATTERY_SPANS.len() - 1)].1;
        let events = journal::load(now);
        self.battery = Some(BatteryPage {
            view: viewmodel::battery_view(
                &log,
                now,
                self.battery_span,
                self.snapshot.state.map(|s| s.battery),
            ),
            lines: battery_log::chart_polylines(&log, now, span, CHART_POINTS),
            autonomy: battery_log::autonomy_lines(&log, &events),
            journal: journal::lines(&events, now, 30),
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
        // Pastille de la barre des tâches : batterie relue sur le casque.
        let badge = state.map(|s| s.battery);
        if self.taskbar_badge != Some(badge)
            && let Some(hwnd) = desktop::hwnd()
        {
            match taskbar::set_battery_badge(hwnd, badge) {
                Ok(()) => self.taskbar_badge = Some(badge),
                Err(e) => eprintln!("pastille de la barre des tâches : {e}"),
            }
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
                can_reconnect: self.service_error.is_none() && !self.busy,
                scenes: &scenes,
                autostart: self.autostart,
                autostart_label: tr("Lancer au démarrage de Windows"),
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
            MenuAction::ReconnectHeadset => self.send(Command::ReconnectHeadset),
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
                    self.scene_result = Some((trf("Scène « {} » en cours…", &[&name]), true));
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
            Desk::Layout(wide) => {
                self.wide_home = wide;
                self.nav_open = wide;
            }
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
                .unwrap_or_else(|| Incoming::Error(tr("écoute du service arrêtée").into()));
            Msg::Service(Arc::new(Mutex::new(Some(next))))
        });
    }

    fn writable(&self) -> bool {
        self.service_error.is_none()
            && self.snapshot.state.is_some()
            && self.snapshot.read_only.is_none()
    }

    fn send(&mut self, command: Command) {
        if matches!(command, Command::SetMode(_)) {
            self.slider_edits.discard(SliderKey::Level);
        }
        if matches!(command, Command::SetEqGains(_) | Command::SetEqFlat) {
            self.slider_edits.discard_bands();
        }
        let reconnect = command == Command::ReconnectHeadset;
        if reconnect {
            if self.service_error.is_some() || self.busy {
                return;
            }
            self.reconnect_notice = None;
        } else if !self.writable() {
            return;
        }
        let session = if reconnect { 0 } else { self.snapshot.session };
        if self.commands.send((command.clone(), session)).is_ok() {
            self.slider_edits.submitted(&command);
            self.requests.sent();
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
        let (in_rx, tx) = spawn_service(true);

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
            .with_tooltip(tr("OpenHeadset Control"))
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
            wide_home: true,
            nav_open: true,
            snapshot: Snapshot::default(),
            service_error: None,
            reconnect_notice: None,
            busy: false,
            requests: Requests::default(),
            confirm: None,
            commands: tx,
            incoming: in_rx,
            user_presets: paths::load_user_presets(),
            new_preset_name: String::new(),
            gains_text: String::new(),
            eq_notice: None,
            ab: AbCompare::default(),
            slider_edits: SliderEdits::default(),
            edits: 0,
            new_name: String::new(),
            name_problem: None,
            tray,
            tray_kind: None,
            tray_tooltip: String::new(),
            taskbar_badge: None,
            tray_entries: Vec::new(),
            tray_actions: Vec::new(),
            hotkeys: hotkey_thread,
            settings_battery: false,
            settings_notice: String::new(),
            settings_plan: None,
            hotkey_rows: Vec::new(),
            hotkeys_loaded: None,
            hotkey_notice: None,
            panel_prefs: paths::load_panel_prefs(),
            panel_loaded: paths::read_panel_text(),
            panel_notice: None,
            dark: momentum_core::appearance::load(),
            language: match momentum_core::i18n::load_pref() {
                None => 0,
                Some(momentum_core::i18n::Lang::Fr) => 1,
                Some(momentum_core::i18n::Lang::En) => 2,
            },
            language_notice: None,
            hotkey_problem: (!errors.is_empty())
                .then(|| trf("Raccourcis clavier : {}", &[&(errors.join(" ; "))])),
            start_hidden,
            autostart: Autostart::named(AUTOSTART_VALUE).is_enabled(),
            scenes: Vec::new(),
            scene_problems: Vec::new(),
            capture_scene_name: String::new(),
            capture_scene_replace: false,
            scene_editor: SceneEditor::default(),
            scene_form: viewmodel::SceneForm::default(),
            scene_notice: None,
            scene_result: None,
            battery_span: 0,
            battery: None,
            battery_notice: None,
            control_since: None,
            connections_at: None,
            codec_since: None,
            rate_since: None,
            windows_audio: None,
            windows_at: None,
            incidents: Vec::new(),
            diagnostic_copied: None,
            report: None,
            report_failed: false,
            // `--assistant` : le montrer quand même (essais en démonstration).
            assistant: (assistant::should_show(demo::requested())
                || std::env::args().any(|a| a == "--assistant"))
            .then_some(0),
            update_enabled: update_check::load_enabled(),
            update: None,
            update_busy: false,
            notifications: NotificationPrefs::load(),
        };
        let mut root = root;
        // Avis de nouvelle version : une lecture au lancement, si activé.
        if root.update_enabled && !demo::requested() {
            root.update_busy = true;
            context.spawn_background(|_| Msg::UpdateResult(check_update()));
        }
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
            Msg::PaneOpen(open) => self.nav_open = open,
            Msg::Service(cell) => {
                let incoming = cell.lock().ok().and_then(|mut c| c.take());
                if let Some(failure) = incoming.as_ref().and_then(|i| self.requests.on_incoming(i))
                {
                    self.ab = AbCompare::default();
                    self.note_incident(failure.clone());
                    self.snapshot.problem = Some(failure);
                }
                if let Some(notice) = self.requests.0.take_notice() {
                    self.reconnect_notice = Some(notice);
                }
                self.busy = self.requests.busy();
                match incoming {
                    Some(Incoming::Snapshot(s)) => {
                        self.snapshot = s;
                        self.service_error = None;
                    }
                    Some(Incoming::Submitted(_) | Incoming::Rejected(_)) => {}
                    Some(Incoming::Event(e)) => {
                        self.note_reading_times(&e);
                        match &e {
                            Event::Lost(r) => {
                                self.note_incident(trf("connexion perdue : {}", &[&r]))
                            }
                            Event::Refused(r) => {
                                self.note_incident(trf("réglage refusé : {}", &[&r]))
                            }
                            Event::SceneApplied(report) => {
                                self.scene_result = Some((report.details(), report.is_complete()));
                            }
                            Event::Exported(result) => {
                                self.eq_notice = Some(match result {
                                    Ok(path) => trf(
                                        "Profil exporté dans {} (données auditives personnelles : ne pas le publier).",
                                        &[&(path.display())],
                                    ),
                                    Err(reason) => {
                                        trf("Export du profil impossible : {}.", &[&reason])
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
                            self.note_incident(trf("service : {}", &[&reason]));
                        }
                        self.snapshot = Snapshot::default();
                        self.service_error = Some(reason);
                    }
                    None => {}
                }
                if self.slider_edits.sync(&self.snapshot) {
                    self.edits = self.edits.wrapping_add(1);
                    self.ab = AbCompare::default();
                }
                self.slider_edits.finish_if_idle(self.busy);
                self.listen(context);
            }
            Msg::ReconnectHeadset => self.send(Command::ReconnectHeadset),
            Msg::Navigate(tag) => {
                if let Some(page) = tag.as_deref().and_then(Page::from_tag) {
                    self.open_page(page);
                }
            }
            Msg::ApplyScene(i) => {
                if let Some(name) = self.scenes.get(i).map(|s| s.name.clone()) {
                    self.scene_result = Some((trf("Scène « {} » en cours…", &[&name]), true));
                    self.send(Command::ApplyScene(name));
                }
            }
            Msg::ReloadScenes => self.reload_scenes(),
            Msg::CaptureSceneName(name) => self.capture_scene_name = name,
            Msg::CaptureSceneReplace(replace) => self.capture_scene_replace = replace,
            Msg::CaptureScene => {
                let result = self
                    .snapshot
                    .state
                    .as_ref()
                    .ok_or_else(|| tr("Aucun état connu à enregistrer.").to_string())
                    .and_then(|state| {
                        momentum_core::scene_capture::save_current(
                            &self.capture_scene_name,
                            state,
                            self.snapshot.equalizer.as_ref(),
                            self.capture_scene_replace,
                        )
                        .map_err(|e| e.to_string())
                    });
                self.scene_result = Some(match result {
                    Ok(scene) => {
                        self.user_presets = paths::load_user_presets();
                        self.reload_scenes();
                        (
                            trf(
                                "Scène « {} » enregistrée (rien n'a été envoyé au casque).",
                                &[&scene.name],
                            ),
                            true,
                        )
                    }
                    Err(error) => (trf("Non enregistré : {}", &[&error]), false),
                });
            }
            Msg::SettingsBattery(on) => {
                self.settings_battery = on;
                self.settings_plan = None;
                self.settings_notice.clear();
            }
            Msg::CancelSettings => {
                self.settings_plan = None;
                self.settings_notice = tr("Import annulé.").into();
            }
            Msg::ExportSettings | Msg::PreviewSettings => {
                let save = matches!(message, Msg::ExportSettings);
                self.settings_plan = None;
                let result = desktop::settings_file(save)
                    .map_err(|e| e.to_string())
                    .and_then(|path| {
                        let Some(path) = path else {
                            return Ok(tr("Sélection annulée.").to_string());
                        };
                        let dir =
                            paths::data_dir().ok_or(tr("Dossier des données introuvable."))?;
                        if save {
                            momentum_core::settings_transfer::export(
                                &dir,
                                &path,
                                self.settings_battery,
                            )
                            .map_err(|e| e.to_string())
                        } else {
                            let plan = momentum_core::settings_transfer::prepare_import(
                                &dir,
                                &path,
                                self.settings_battery,
                            )
                            .map_err(|e| e.to_string())?;
                            let summary = plan.summary().to_string();
                            self.settings_plan = Some(plan);
                            Ok(summary)
                        }
                    });
                self.settings_notice =
                    result.unwrap_or_else(|e| trf("Opération refusée : {}", &[&e]));
            }
            Msg::ImportSettings => {
                if let Some(plan) = self.settings_plan.take() {
                    self.settings_notice = plan
                        .apply()
                        .unwrap_or_else(|e| trf("Import refusé : {}", &[&e]));
                }
            }
            Msg::SceneSelect(Some(i)) => {
                if i != self.scene_editor.selected() {
                    self.scene_editor.select(i);
                    self.scene_list_changed();
                }
            }
            Msg::SceneSelect(None) => {}
            Msg::SceneNew => {
                self.scene_editor.new_scene();
                self.scene_list_changed();
            }
            Msg::SceneDuplicate => {
                self.scene_editor.duplicate();
                self.scene_list_changed();
            }
            Msg::SceneDelete => {
                self.scene_editor.delete();
                self.scene_list_changed();
            }
            Msg::SceneMove(delta) => {
                self.scene_editor.move_by(delta);
                self.scene_list_changed();
            }
            Msg::SceneName(text) => {
                if text != self.scene_form.name {
                    self.scene_form.name = text;
                    self.scene_form_changed();
                }
            }
            Msg::SceneLevel(text) => {
                if text != self.scene_form.level {
                    self.scene_form.level = text;
                    self.scene_form_changed();
                }
            }
            Msg::SceneAutoTime(text) => {
                if text != self.scene_form.auto_time {
                    self.scene_form.auto_time = text;
                    self.scene_form_changed();
                }
            }
            Msg::SceneChoice(field, Some(i)) => {
                let value = i32::try_from(i).unwrap_or(0);
                let f = &mut self.scene_form;
                let slot = match field {
                    SceneField::Sound => &mut f.sound,
                    SceneField::Spatial => &mut f.spatial,
                    SceneField::Noise => &mut f.noise,
                    SceneField::AntiWind => &mut f.anti_wind,
                    SceneField::Touch => &mut f.touch,
                    SceneField::Equalizer => &mut f.equalizer,
                    SceneField::Bass => &mut f.bass_boost,
                    SceneField::Auto => &mut f.auto,
                };
                if *slot != value {
                    *slot = value;
                    self.scene_form_changed();
                }
            }
            Msg::SceneChoice(_, None) => {}
            Msg::SceneSave => self.save_scenes(),
            Msg::SceneDiscard => {
                self.reload_scene_editor();
                self.scene_notice = Some(tr("Modifications abandonnées.").to_string());
            }
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
            Msg::ReadWindowsAudio => self.read_windows_audio(),
            Msg::Theme(Some(i)) => {
                let dark = i == 1;
                if self.dark != Some(dark) {
                    if let Err(e) = momentum_core::appearance::store(dark) {
                        eprintln!("apparence.txt : {e}");
                    }
                    self.dark = Some(dark);
                }
            }
            Msg::Theme(None) => {}
            Msg::Language(Some(i)) if i != self.language => {
                use momentum_core::i18n::{Lang, store_pref};
                let choice = match i {
                    1 => Some(Lang::Fr),
                    2 => Some(Lang::En),
                    _ => None,
                };
                self.language_notice = Some(match store_pref(choice) {
                    Ok(()) => {
                        self.language = i;
                        tr("Langue enregistrée : appliquée au prochain lancement de l'interface.")
                            .to_string()
                    }
                    Err(e) => trf("Langue non enregistrée : {}.", &[&e]),
                });
            }
            Msg::Language(_) => {}
            Msg::ExportBattery => self.battery_notice = Some(export_battery_history()),
            Msg::ClearBattery => self.confirm = Some(Confirm::ClearBattery),
            Msg::ClearJournal => self.confirm = Some(Confirm::ClearJournal),
            Msg::ExportJournal => self.battery_notice = Some(journal::export(unix_now())),
            Msg::ExportApo => {
                if let Some(eq) = self.snapshot.equalizer.as_ref() {
                    self.eq_notice = Some(match presets::format_graphic_eq(&eq.gains) {
                        Err(e) => e,
                        Ok(text) => {
                            match paths::export_file("momentum-egaliseur-apo", "txt", unix_now()) {
                                Some(path) => match std::fs::write(&path, text) {
                                    Ok(()) => trf(
                                        "Courbe exportée pour Equalizer APO dans {}.",
                                        &[&(path.display())],
                                    ),
                                    Err(e) => trf("Export de la courbe impossible : {}.", &[&e]),
                                },
                                None => tr("Export impossible : dossier Documents introuvable.")
                                    .to_string(),
                            }
                        }
                    });
                }
            }
            Msg::Notification(kind, on) => {
                let mut prefs = self.notifications;
                match kind {
                    NotificationKind::LowBattery => prefs.low_battery = on,
                    NotificationKind::Connection => prefs.connection = on,
                    NotificationKind::Idle => prefs.idle = on,
                }
                match prefs.store() {
                    Ok(()) => self.notifications = prefs,
                    Err(e) => eprintln!("notifications : {e}"),
                }
            }
            Msg::ReloadBattery => self.reload_battery(),
            Msg::CopyDiagnostic => {
                self.diagnostic_copied = Some(desktop::copy_text(&self.diagnostic()));
            }
            Msg::ReportPreview => {
                self.report = Some(self.diagnostic());
                self.report_failed = false;
            }
            Msg::ReportOpen => {
                if let Some(text) = self.report.take() {
                    let system = momentum_service::system::os_version();
                    let url = diagnostic::issue_url(
                        env!("CARGO_PKG_VERSION"),
                        diagnostic::ISSUE_INTERFACE_WINUI,
                        system.as_deref(),
                        &text,
                    );
                    self.report_failed = !desktop::open_url(&url);
                }
            }
            Msg::ReportCancel => self.report = None,
            Msg::AssistantOpen => self.assistant = Some(0),
            Msg::AssistantPrevious => {
                self.assistant = self.assistant.map(|i| i.saturating_sub(1));
            }
            Msg::AssistantNext => {
                let count = assistant::steps(Platform::Windows).len();
                self.assistant = self.assistant.map(|i| (i + 1).min(count - 1));
            }
            Msg::AssistantClose => {
                self.assistant = None;
                if !demo::requested()
                    && let Err(e) = assistant::mark_seen()
                {
                    eprintln!("assistant : état non enregistré : {e}");
                }
            }
            Msg::AssistantBluetooth => {
                desktop::open_url("ms-settings:bluetooth");
            }
            Msg::OpenGuide => {
                desktop::open_url(assistant::GUIDE_URL);
            }
            Msg::UpdateNotice(on) => {
                if let Err(e) = update_check::store_enabled(on) {
                    eprintln!("avis de nouvelle version : {e}");
                }
                self.update_enabled = on;
                self.update = None;
                if on && !demo::requested() {
                    self.update_busy = true;
                    context.spawn_background(|_| Msg::UpdateResult(check_update()));
                }
            }
            Msg::UpdateCheck => {
                if !self.update_busy && !demo::requested() {
                    self.update_busy = true;
                    context.spawn_background(|_| Msg::UpdateResult(check_update()));
                }
            }
            Msg::UpdateResult(outcome) => {
                self.update_busy = false;
                self.update = Some(outcome);
            }
            Msg::OpenRelease => {
                // Page vérifiée par `parse_latest` : publication du dépôt.
                if let Some(update_check::Outcome::Available(r)) = &self.update {
                    desktop::open_url(&r.url);
                }
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
            Msg::Level(v, epoch) if epoch == self.slider_edits.epoch() => {
                let level = v.round().clamp(0.0, 100.0) as u8;
                let current = viewmodel::main_view(None, state.as_ref(), &[], None, None)
                    .controls
                    .level;
                if self
                    .slider_edits
                    .edit(SliderKey::Level, i16::from(level), current as i16)
                {
                    self.settle_later(context);
                }
            }
            Msg::Band(i, db, epoch) if epoch == self.slider_edits.epoch() => {
                let tenths = (db * 10.0).round().clamp(-128.0, 127.0) as i8;
                let current = self
                    .snapshot
                    .equalizer
                    .as_ref()
                    .and_then(|e| e.gains.get(i).copied());
                let band = u8::try_from(i).unwrap_or(u8::MAX);
                if let Some(read) = current
                    && self.slider_edits.edit(
                        SliderKey::Band(band),
                        i16::from(tenths),
                        i16::from(read),
                    )
                {
                    self.settle_later(context);
                }
            }
            Msg::PromptVolume(pct, epoch) if epoch == self.slider_edits.epoch() => {
                // Échelle de l'app : plusieurs % donnent la même valeur du
                // casque ; on compare donc les valeurs du casque.
                let value = prompt_volume_from_percent(pct.round().clamp(0.0, 100.0) as u8);
                let current = self.snapshot.info.as_ref().and_then(|i| i.prompt_volume);
                if let Some(read) = current
                    && self.slider_edits.edit(
                        SliderKey::PromptVolume,
                        i16::from(value),
                        i16::from(read),
                    )
                {
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
                            Some(tr("nom refusé : c'est déjà le nom du casque").to_string());
                    }
                    Ok(()) => {
                        self.send(Command::SetName(name));
                        self.new_name.clear();
                    }
                }
            }
            Msg::Settled(edit) => {
                if edit == self.edits {
                    for command in self.slider_edits.take_commands() {
                        self.send(command);
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
                    match self
                        .snapshot
                        .eq_topology()
                        .ok_or(tr("Topologie de l'égaliseur non disponible."))
                        .and_then(|t| p.check_topology(&t))
                    {
                        Ok(()) => self.send(Command::SetEqGains(p.gains.clone())),
                        Err(reason) => self.eq_notice = Some(reason.into()),
                    }
                }
            }
            Msg::NewPresetName(text) => self.new_preset_name = text,
            Msg::SavePreset => {
                if let Some(gains) = self.snapshot.equalizer.as_ref().map(|e| e.gains.clone())
                    && let Some(topology) = self.snapshot.eq_topology()
                {
                    let name = self.new_preset_name.trim().to_string();
                    // Relu puis modifié : rien d'enregistré par une autre
                    // interface n'est perdu.
                    self.eq_notice = Some(
                        match paths::upsert_user_preset_for(&name, &gains, &topology) {
                            Ok(list) => {
                                self.user_presets = list;
                                self.new_preset_name.clear();
                                trf("Préréglage « {} » enregistré sur ce PC.", &[&name])
                            }
                            Err(reason) => trf("Préréglage non enregistré : {}.", &[&reason]),
                        },
                    );
                }
            }
            Msg::DeletePreset => {
                if let Some(preset) = self.current_user_preset() {
                    self.eq_notice = Some(match paths::remove_user_preset(&preset.name) {
                        Ok(list) => {
                            self.user_presets = list;
                            trf("Préréglage « {} » supprimé.", &[&preset.name])
                        }
                        Err(e) => trf("Suppression non enregistrée : {}.", &[&e]),
                    });
                }
            }
            Msg::BindLegacyPresets => {
                if let Some(topology) = self.snapshot.eq_topology() {
                    self.eq_notice = Some(match paths::bind_legacy_presets(&topology) {
                        Ok(list) => {
                            self.user_presets = list;
                            tr("Association enregistrée sur ce PC, sans modifier le casque.").into()
                        }
                        Err(reason) => reason,
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
                            self.eq_notice =
                                Some(tr("Ce sont déjà les gains du casque.").to_string());
                        }
                        Ok(gains) => {
                            self.send(Command::SetEqGains(gains));
                            self.gains_text.clear();
                            self.eq_notice = None;
                        }
                        Err(reason) => {
                            self.eq_notice = Some(trf("Gains non appliqués : {}.", &[&reason]));
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
                                Ok(()) => trf("Courbe exportée dans {}.", &[&(path.display())]),
                                Err(e) => trf("Export de la courbe impossible : {}.", &[&e]),
                            },
                            None => {
                                tr("Export impossible : dossier Documents introuvable.").to_string()
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
                            trf("Courbe importée depuis {}.", &[&(path.display())])
                        }
                        Err(reason) => trf("Courbe non importée : {}.", &[&reason]),
                    });
                }
            }
            Msg::HotkeyText(i, text) => {
                if let Some(row) = self.hotkey_rows.get_mut(i) {
                    row.1 = text;
                }
            }
            Msg::SaveHotkeys => self.save_hotkeys(),
            Msg::PanelNoise(on) => {
                self.panel_prefs.noise = on;
                self.store_panel_prefs();
            }
            Msg::PanelLevel(on) => {
                self.panel_prefs.level = on;
                self.store_panel_prefs();
            }
            Msg::PanelProfile(on) => {
                self.panel_prefs.profile = on;
                self.store_panel_prefs();
            }
            Msg::PanelScene(name, shown) => {
                let all: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
                match self.panel_prefs.set_scene(&all, &name, shown) {
                    Ok(()) => self.store_panel_prefs(),
                    Err(reason) => self.panel_notice = Some(reason),
                }
            }
            Msg::ExportProfile => match paths::export_file("momentum-profil", "json", unix_now()) {
                Some(path) => self.send(Command::ExportProfile(path)),
                None => {
                    self.eq_notice =
                        Some(tr("Export impossible : dossier Documents introuvable.").to_string());
                }
            },
            Msg::RememberA => {
                if let Some(eq) = self.snapshot.equalizer.as_ref()
                    && let Some(topology) = self.snapshot.eq_topology()
                {
                    let _ = self.ab.remember_for(&topology, &eq.gains);
                    self.eq_notice = Some(
                        tr("Courbe A mémorisée : modifiez l'égaliseur (B), puis comparez.")
                            .to_string(),
                    );
                }
            }
            Msg::ToggleAb => {
                if let Some(eq) = self.snapshot.equalizer.clone()
                    && let Some(topology) = self.snapshot.eq_topology()
                {
                    match self.ab.toggle_for(&topology, &eq.gains) {
                        Ok(Some(gains)) if gains != eq.gains => {
                            self.send(Command::SetEqGains(gains))
                        }
                        Err(reason) => self.eq_notice = Some(reason.into()),
                        _ => {}
                    }
                }
            }
            Msg::ToggleConnection(index) => {
                let writable = self.writable();
                if let Some(row) = self.rows(writable).get(index).filter(|r| r.can_act) {
                    // Identifiant opaque : la confirmation reste liée à cet
                    // appareil, même si la liste change entre-temps.
                    let device = self
                        .snapshot
                        .connections
                        .get(index)
                        .map(|c| c.id.clone())
                        .filter(|id| !id.is_empty());
                    if row.action == tr("Déconnecter") {
                        self.confirm = Some(Confirm::Disconnect {
                            index,
                            name: row.name.clone(),
                            device: device.unwrap_or_default(),
                        });
                    } else {
                        let name = row.name.clone();
                        self.send(Command::ToggleConnection {
                            index,
                            name,
                            connect: true,
                            device,
                        });
                    }
                }
            }
            Msg::Confirmed => match self.confirm.take() {
                Some(Confirm::MultipointOff) => self.send(Command::SetMultipoint(false)),
                Some(Confirm::ClearJournal) => {
                    // Le service tient le journal : il l'efface, puis la
                    // page le relit.
                    let sent = self.commands.send((Command::ClearJournal, 0)).is_ok();
                    if sent {
                        self.requests.sent();
                        self.busy = true;
                    }
                    self.battery_notice = Some(if sent {
                        tr("Effacement du journal demandé au service.").to_string()
                    } else {
                        tr("Effacement impossible : service injoignable.").to_string()
                    });
                    context.spawn_background(|_| {
                        std::thread::sleep(Duration::from_millis(600));
                        Msg::ReloadBattery
                    });
                }
                Some(Confirm::ClearBattery) => {
                    // Le service tient l'historique : il l'efface et
                    // réécrit le fichier, relu ensuite par la page.
                    // Sans casque aussi : connexion 0 (indifférente). Un refus
                    // du service s'affiche comme pour toute demande.
                    let sent = self
                        .commands
                        .send((Command::ClearBatteryHistory, 0))
                        .is_ok();
                    if sent {
                        self.requests.sent();
                        self.busy = true;
                    }
                    self.battery_notice = Some(if sent {
                        tr("Effacement de l'historique demandé au service.").to_string()
                    } else {
                        tr("Effacement impossible : service injoignable.").to_string()
                    });
                    context.spawn_background(|_| {
                        std::thread::sleep(Duration::from_millis(600));
                        Msg::ReloadBattery
                    });
                }
                Some(Confirm::Disconnect {
                    index,
                    name,
                    device,
                }) => self.send(Command::ToggleConnection {
                    index,
                    name,
                    connect: false,
                    device: Some(device).filter(|id| !id.is_empty()),
                }),
                None => {}
            },
            Msg::Cancelled => self.confirm = None,
            Msg::Attach(tries) => {
                if desktop::attach() {
                    desktop::on_layout_changed(|wide| {
                        let _ = desk().0.send(Desk::Layout(wide));
                    });
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
            | Msg::Level(_, _)
            | Msg::Band(_, _, _)
            | Msg::PromptVolume(_, _)
            | Msg::AutoOff(None)
            | Msg::PromptMode(None)
            | Msg::AudioMode(None)
            | Msg::Preset(None) => {}
        }
        self.update_tray();
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title(tr("OpenHeadset Control"));
        context.window_visuals(
            WindowVisuals::new()
                .backdrop(WindowBackdrop::Mica)
                .theme(window_theme(self.dark))
                .client_size(1100.0, 780.0)
                .constraints(WindowConstraints {
                    min_width: Some(420.0),
                    min_height: Some(480.0),
                    ..WindowConstraints::default()
                }),
        );

        let page = match self.page {
            _ if self.assistant.is_some() => self.assistant_view(context),
            Page::Home => self.home(context),
            Page::Sound => self.sound(context),
            Page::Scenes => self.scenes_page(context),
            Page::Battery => self.battery_page(context),
            Page::Audio => self.audio_page(context),
            Page::Settings => self.settings(context),
            Page::About => self.about(context),
        };
        let items = Page::ALL.iter().map(|(p, tag, label, glyph)| {
            KeyedView::new(
                *tag,
                NavigationViewItem::new()
                    .tag(*tag)
                    // Repères stables pour l'accessibilité et les essais UIA.
                    .automation_id(format!("nav-{tag}"))
                    .is_selected(*p == self.page)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, tr(label)),
                        SlotView::new(NavigationViewItemSlot::Icon, FontIcon::new().glyph(*glyph)),
                    ]),
            )
        });
        NavigationView::new()
            .pane_title(tr("OpenHeadset Control"))
            // Mode natif compact/fermé quand la largeur diminue.
            .pane_display_mode(NavigationViewPaneDisplayMode::Auto)
            .is_pane_open(self.nav_open)
            .on_is_pane_open_changed(context.callback(Msg::PaneOpen))
            .open_pane_length(220.0)
            .is_settings_visible(false)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .on_selected_tag_changed(context.callback(Msg::Navigate))
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, items),
                SlotView::new(
                    NavigationViewSlot::Content,
                    // Repère de la page affichée (essais UIA) : un panneau
                    // n'apparaît pas dans l'arbre UI Automation, le
                    // ScrollViewer si.
                    ScrollViewer::new()
                        .automation_id(format!("page-{}", self.page.tag()))
                        .content(
                            StackPanel::new()
                                .spacing(12.0)
                                .margin(Thickness::new(20.0, 24.0, 20.0, 24.0))
                                .max_width(if self.page == Page::Home {
                                    None
                                } else {
                                    Some(860.0)
                                })
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

/// Copie `batterie.csv` dans Documents ; renvoie le message à afficher.
fn export_battery_history() -> String {
    let Some(source) = paths::battery_file() else {
        return tr("Export impossible : dossier des données introuvable.").to_string();
    };
    let Some(target) = paths::export_file("momentum-batterie", "csv", unix_now()) else {
        return tr("Export impossible : dossier Documents introuvable.").to_string();
    };
    match std::fs::copy(&source, &target) {
        Ok(_) => trf("Historique exporté dans {}.", &[&(target.display())]),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tr("Aucun historique à exporter pour l'instant.").to_string()
        }
        Err(e) => trf("Export de l'historique impossible : {}.", &[&e]),
    }
}

/// Enregistre les raccourcis de `config` ; renvoie le fil qui les écoute
/// et les erreurs (ligne invalide, combinaison déjà prise).
fn register_hotkeys(config: &str) -> (Option<HotkeyThread>, Vec<String>) {
    // La démonstration ne réserve aucune combinaison globale : les
    // raccourcis de l'application réelle doivent rester disponibles.
    if demo::requested() {
        return (None, Vec::new());
    }
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

/// Thème de fenêtre d'après le choix enregistré (`None` : le système).
pub(crate) fn window_theme(dark: Option<bool>) -> WindowTheme {
    match dark {
        Some(true) => WindowTheme::Dark,
        Some(false) => WindowTheme::Light,
        None => WindowTheme::System,
    }
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
                    .font_weight(FontWeight::SEMI_BOLD)
                    .automation_heading_level(AutomationHeadingLevel::Level2),
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
                // Nom lu par les lecteurs d'écran (sinon « Activé »).
                .automation_name(label)
                .grid_column(1),
        ))
}

fn availability_hint(status: Availability) -> View {
    if status.enabled() {
        View::empty()
    } else {
        small(status.reason())
    }
}

/// Cartes de l'accueil : hiérarchie compacte inspirée de la maquette,
/// avec les ressources de thème et les icônes natives de Windows.
fn home_card(title: &str, glyph: &str, content: impl Into<View>) -> View {
    Border::new()
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .corner_radius(8.0)
        .padding(20.0)
        .content(
            StackPanel::new().spacing(16.0).children((
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(10.0)
                    .children((
                        FontIcon::new()
                            .glyph(glyph)
                            .width(20.0)
                            .height(20.0)
                            .opacity(0.8),
                        TextBlock::new()
                            .text(title)
                            .font_size(16.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .automation_heading_level(AutomationHeadingLevel::Level2),
                    )),
                content.into(),
            )),
        )
}

/// Le TogglePattern conserve l'état accessible ; le libellé Activé /
/// Désactivé est masqué pour éviter de répéter l'interrupteur dans la carte.
fn home_switch(
    label: &str,
    description: &str,
    on: bool,
    status: Availability,
    toggled: Callback<bool>,
) -> View {
    if matches!(status, Availability::Unread | Availability::Disconnected) {
        return available_switch(label, description, on, status, toggled);
    }
    StackPanel::new().spacing(4.0).children((
        Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(14.0)
            .children((
                StackPanel::new().spacing(4.0).children((
                    TextBlock::new()
                        .text(label)
                        .text_wrapping(TextWrapping::Wrap),
                    small(description),
                )),
                ToggleSwitch::new()
                    .is_on(on)
                    .is_enabled(status.enabled())
                    .automation_name(label)
                    .on_toggled(toggled)
                    .min_width(0.0)
                    .width(48.0)
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .slots([
                        SlotView::new(ToggleSwitchSlot::OnContent, TextBlock::new().text("")),
                        SlotView::new(ToggleSwitchSlot::OffContent, TextBlock::new().text("")),
                    ]),
            )),
        availability_hint(status),
    ))
}

fn small(text: impl Into<String>) -> View {
    TextBlock::new()
        .text(text)
        .font_size(12.0)
        .opacity(0.8)
        .text_wrapping(TextWrapping::Wrap)
        .into()
}

/// Un réglage non lu ne devient pas un faux interrupteur « désactivé ».
fn available_switch(
    label: &str,
    description: &str,
    on: bool,
    status: Availability,
    toggled: Callback<bool>,
) -> View {
    if matches!(status, Availability::Unread | Availability::Disconnected) {
        return StackPanel::new().spacing(4.0).children((
            TextBlock::new().text(label),
            small(description),
            availability_hint(status),
        ));
    }
    StackPanel::new().spacing(4.0).children((
        switch_row(label, description, on, status.enabled(), toggled),
        availability_hint(status),
    ))
}

/// Lecture de la dernière publication (fil de fond).
fn check_update() -> update_check::Outcome {
    let body = desktop::fetch_latest_release();
    update_check::Outcome::from_response(
        env!("CARGO_PKG_VERSION"),
        body.as_deref().map_err(|e| e.as_str()),
    )
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
            .map(|reason| trf("Lecture seule : {}", &[&reason]));
        let problem = self
            .service_error
            .as_ref()
            .map(|e| trf("Service OpenHeadset Control injoignable : {}", &[&e]))
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
            tr("Demande en cours…").to_string()
        } else {
            v.connection.clone()
        };
        let confirm = self.confirm.as_ref().map(|c| {
            let (title, text, action) = match c {
                Confirm::MultipointOff => (
                    tr("Couper le Multipoint ?"),
                    tr("Sans Multipoint, le casque ne garde qu'un appareil : l'autre est déconnecté.")
                        .to_string(),
                    tr("Couper le Multipoint"),
                ),
                Confirm::ClearJournal => (
                    tr("Effacer le journal des événements ?"),
                    tr("Tous les événements notés sur ce PC sont supprimés. Exporter d'abord pour les garder.")
                        .to_string(),
                    tr("Effacer"),
                ),
                Confirm::ClearBattery => (
                    tr("Effacer l'historique de la batterie ?"),
                    tr("Toutes les mesures enregistrées sur ce PC sont supprimées. Exporter d'abord pour les garder.")
                        .to_string(),
                    tr("Effacer"),
                ),
                Confirm::Disconnect { name, .. } => (
                    tr("Déconnecter cet appareil ?"),
                    trf("Le casque coupe sa liaison avec « {} ». Il reste appairé : « Connecter » la rétablit.", &[&name]),
                    tr("Déconnecter"),
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
                                    .content(tr("Annuler")),
                            )),
                    )),
                )
        });
        StackPanel::new().spacing(8.0).children((
            TextBlock::new()
                .text(if self.page == Page::Home && self.assistant.is_none() {
                    tr("Accueil").to_string()
                } else {
                    v.title.clone()
                })
                .font_size(28.0)
                .text_wrapping(TextWrapping::Wrap)
                .automation_heading_level(AutomationHeadingLevel::Level1)
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
                .is_open(demo::requested())
                .is_closable(false)
                .severity(InfoBarSeverity::Informational)
                .message(tr(
                    "Démonstration : données simulées (aucun casque, aucun service)",
                )),
            // Hauteur réservée : un retour de lecture ne déplace pas le
            // curseur sous la souris ni le point de défilement.
            TextBlock::new()
                .text(
                    self.slider_edits
                        .notice(&self.snapshot)
                        .replace('\n', " · "),
                )
                .height(20.0)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .automation_id("intentions-curseurs")
                .automation_name(self.slider_edits.notice(&self.snapshot)),
            InfoBar::new()
                .is_open(!v.banner.is_empty())
                .is_closable(false)
                .severity(if v.banner_is_warning {
                    InfoBarSeverity::Warning
                } else {
                    InfoBarSeverity::Informational
                })
                .message(v.banner.clone()),
            InfoBar::new()
                .is_open(self.reconnect_notice.is_some())
                .is_closable(false)
                .severity(InfoBarSeverity::Informational)
                .message(self.reconnect_notice.clone().unwrap_or_default()),
            confirm.map_or_else(View::empty, View::from),
            match &self.update {
                Some(update_check::Outcome::Available(_)) => self.update_row(context),
                _ => View::empty(),
            },
        ))
    }

    /// État de l'avis de nouvelle version, avec « Voir la publication »
    /// quand une version plus récente existe.
    fn update_row(&self, context: &mut ViewContext<Self>) -> View {
        if !self.update_enabled {
            return View::empty();
        }
        let current = env!("CARGO_PKG_VERSION");
        let text = if self.update_busy {
            tr("Vérification en cours…").to_string()
        } else {
            self.update
                .as_ref()
                .map(|o| o.text(current))
                .unwrap_or_default()
        };
        let available = matches!(self.update, Some(update_check::Outcome::Available(_)));
        StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                TextBlock::new()
                    .text(text)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_wrapping(TextWrapping::Wrap),
                if available {
                    Button::new()
                        .on_click(context.message(Msg::OpenRelease))
                        .content(tr("Voir la publication"))
                } else {
                    Button::new()
                        .is_enabled(!self.update_busy && !demo::requested())
                        .on_click(context.message(Msg::UpdateCheck))
                        .content(tr("Vérifier maintenant"))
                },
            ))
    }

    fn home(&self, context: &mut ViewContext<Self>) -> View {
        let v = self.main_view();
        if !v.connected {
            return card(
                tr("Votre casque"),
                StackPanel::new().spacing(12.0).children((
                    TextBlock::new()
                        .text(v.waiting.clone())
                        .text_wrapping(TextWrapping::Wrap),
                    Button::new()
                        .is_enabled(self.service_error.is_none() && !self.busy)
                        .on_click(context.message(Msg::ReconnectHeadset))
                        .content(tr("Reconnecter le casque")),
                )),
            );
        }
        let c = &v.controls;
        let s = self.snapshot.state.as_ref();
        let a = home_presentation::home_availability(s, self.snapshot.info.as_ref(), v.writable);
        let summary = headset_summary(s);
        let multipoint = self.snapshot.info.as_ref().and_then(|i| i.multipoint);
        let st = viewmodel::settings_view(s, self.snapshot.info.as_ref(), v.writable);
        let rows = self.rows(v.writable);
        let is_connected = |i: usize| {
            self.snapshot
                .connections
                .get(i)
                .is_some_and(|d| d.connected || d.this_device)
        };
        let connected_count = (0..rows.len()).filter(|&i| is_connected(i)).count();
        let device_group = |connected: bool| {
            StackPanel::new().spacing(12.0).keyed_children(
                rows.iter()
                    .enumerate()
                    .filter(|(i, _)| is_connected(*i) == connected)
                    .map(|(i, row)| {
                        KeyedView::new(
                            format!("{i}-{}", row.name),
                            Grid::new()
                                .columns([GridLength::Star(1.0), GridLength::Auto])
                                .column_spacing(8.0)
                                .children((
                                    StackPanel::new().spacing(4.0).children((
                                        TextBlock::new()
                                            .text(row.name.clone())
                                            .text_wrapping(TextWrapping::Wrap),
                                        small(row.status.clone()),
                                    )),
                                    Border::new().grid_column(1).content(if row.can_act {
                                        Button::new()
                                            .on_click(context.message(Msg::ToggleConnection(i)))
                                            .content(row.action.clone())
                                    } else {
                                        View::empty()
                                    }),
                                )),
                        )
                    }),
            )
        };
        let noise_modes = Border::new()
            .background(ThemeBrush::SolidBackground)
            .corner_radius(6.0)
            .padding(4.0)
            .content(
                Grid::new()
                    .columns([GridLength::Star(1.0); 3])
                    .column_spacing(4.0)
                    .keyed_children(viewmodel::NOISE_MODES.iter().enumerate().map(|(i, label)| {
                        let selected = c.noise_mode == i as i32;
                        KeyedView::new(
                            i as u64,
                            Border::new().grid_column(i as i32).content(
                                Button::new()
                                    .style(if selected {
                                        ButtonStyle::Accent
                                    } else {
                                        ButtonStyle::Subtle
                                    })
                                    .is_enabled(a.noise.enabled())
                                    .horizontal_alignment(HorizontalAlignment::Stretch)
                                    .automation_id(format!("mode-bruit-{i}"))
                                    .automation_name(if selected {
                                        trf("{} (mode actuel)", &[&tr(label)])
                                    } else {
                                        tr(label).into()
                                    })
                                    .on_click(context.message(Msg::NoiseMode(Some(i))))
                                    .content(
                                        TextBlock::new()
                                            .text(tr(label))
                                            .font_size(12.0)
                                            .font_weight(if selected {
                                                FontWeight::SEMI_BOLD
                                            } else {
                                                FontWeight::NORMAL
                                            })
                                            .text_wrapping(TextWrapping::Wrap),
                                    ),
                            ),
                        )
                    })),
            );
        let noise = home_card(
            tr("Contrôle du bruit"),
            "\u{E7F6}",
            StackPanel::new().spacing(10.0).children((
                noise_modes,
                availability_hint(a.noise),
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .children((
                        TextBlock::new()
                            .text(tr("ANC / transparence"))
                            .vertical_alignment(VerticalAlignment::Center),
                        TextBlock::new()
                            .text(trf("{} %", &[&c.level]))
                            .font_size(22.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .foreground(ThemeBrush::Accent)
                            .grid_column(1),
                    )),
                Slider::new()
                    .minimum(0.0)
                    .maximum(100.0)
                    .step_frequency(1.0)
                    .value(f64::from(
                        self.slider_edits
                            .value(SliderKey::Level)
                            .unwrap_or(c.level as i16),
                    ))
                    .is_enabled(a.level.enabled())
                    .automation_name(tr("ANC ↔ transparence"))
                    .on_value_changed(context.callback({
                        let epoch = self.slider_edits.epoch();
                        move |v| Msg::Level(v, epoch)
                    })),
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Star(1.0)])
                    .children((
                        small(tr("ANC maximal")),
                        TextBlock::new()
                            .text(tr("Transparence maximale"))
                            .font_size(12.0)
                            .text_wrapping(TextWrapping::Wrap)
                            .horizontal_alignment(HorizontalAlignment::Right)
                            .grid_column(1),
                    )),
                availability_hint(a.level),
                home_switch(
                    tr("Anti-vent"),
                    tr("Réduit le bruit du vent sur les micros"),
                    c.anti_wind,
                    a.anti_wind,
                    context.callback(Msg::AntiWind),
                ),
            )),
        );
        let spatial = home_card(
            tr("Audio spatial"),
            "\u{E767}",
            StackPanel::new().spacing(14.0).children((
                home_switch(
                    tr("Audio spatial"),
                    tr("Une scène sonore plus enveloppante"),
                    c.spatial_on,
                    a.spatial,
                    context.callback(Msg::Spatial),
                ),
                home_switch(
                    tr("Suivi de tête"),
                    tr("Avec l'audio spatial"),
                    c.head_tracking_on,
                    a.tracking,
                    context.callback(Msg::HeadTracking),
                ),
            )),
        );
        let connections = home_card(
            tr("Appareils"),
            "\u{E702}",
            StackPanel::new().spacing(12.0).children((
                home_switch(
                    tr("Multipoint"),
                    tr("Deux appareils à la fois"),
                    multipoint == Some(true),
                    a.multipoint,
                    context.callback(Msg::Multipoint),
                ),
                if rows.is_empty() {
                    small(tr("Liste des appareils non disponible."))
                } else {
                    StackPanel::new().spacing(12.0).children((
                        small(trf("CONNECTÉS · {}", &[&connected_count])),
                        device_group(true),
                        if rows.len() > connected_count {
                            StackPanel::new().spacing(12.0).children((
                                small(tr("APPAIRÉS NON CONNECTÉS")),
                                device_group(false),
                            ))
                        } else {
                            View::empty()
                        },
                    ))
                },
                small(tr(
                    "La connexion Bluetooth n'indique pas quel appareil diffuse du son.",
                )),
                if !v.writable {
                    availability_hint(Availability::ReadOnly)
                } else {
                    View::empty()
                },
            )),
        );
        let quick = home_card(
            tr("Au quotidien"),
            "\u{E713}",
            StackPanel::new().spacing(14.0).children((
                home_switch(
                    tr("Commandes tactiles"),
                    tr("Gestes sur l'écouteur droit"),
                    c.touch_on,
                    a.touch,
                    context.callback(Msg::Touch),
                ),
                home_switch(
                    tr("Smart Pause"),
                    tr("Pause au retrait du casque"),
                    st.smart_pause.on,
                    a.smart_pause,
                    context.callback(Msg::SmartPause),
                ),
                Button::new()
                    .automation_id("accueil-egaliseur")
                    .on_click(context.message(Msg::Navigate(Some("son".into()))))
                    .content(
                        TextBlock::new()
                            .text(tr("Ouvrir l'égaliseur et la comparaison A/B"))
                            .text_wrapping(TextWrapping::Wrap),
                    ),
            )),
        );
        let position = |column: i32, row: i32, content: View| {
            Border::new()
                .grid_column(if self.wide_home { column } else { 0 })
                .grid_row(if self.wide_home {
                    row
                } else {
                    row * 2 + column
                })
                .content(content)
        };
        StackPanel::new().spacing(16.0).children((
            self.headset_card(&v, &summary),
            Grid::new()
                .columns(if self.wide_home {
                    vec![GridLength::Star(1.0), GridLength::Star(1.0)]
                } else {
                    vec![GridLength::Star(1.0)]
                })
                .rows(vec![GridLength::Auto; if self.wide_home { 2 } else { 4 }])
                .column_spacing(16.0)
                .row_spacing(16.0)
                .children((
                    position(0, 0, noise),
                    position(1, 0, connections),
                    position(0, 1, spatial),
                    position(1, 1, quick),
                )),
        ))
    }

    fn headset_card(
        &self,
        v: &viewmodel::MainView,
        summary: &home_presentation::HeadsetSummary,
    ) -> View {
        let model = self
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.model.clone())
            .unwrap_or_else(|| tr("Casque Sennheiser").into());
        let badge = |label: &str, value: &str, id: &str| {
            Border::new()
                .background(ThemeBrush::SolidBackground)
                .corner_radius(6.0)
                .padding(Thickness::new(10.0, 6.0, 10.0, 6.0))
                .content(
                    TextBlock::new()
                        .text(value)
                        .font_size(12.0)
                        .font_weight(FontWeight::SEMI_BOLD)
                        .text_wrapping(TextWrapping::Wrap)
                        .automation_id(id)
                        .automation_name(trf("{} : {}", &[&label, &value])),
                )
        };
        Border::new()
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .corner_radius(10.0)
            .padding(20.0)
            .content(
                Grid::new()
                    .columns([GridLength::Auto, GridLength::Star(1.0)])
                    .column_spacing(if self.wide_home { 28.0 } else { 16.0 })
                    .children((
                        Image::new()
                            .source_data(EncodedImage::from_static(include_bytes!(
                                "../assets/casque-illustration.png"
                            )))
                            .width(if self.wide_home { 148.0 } else { 76.0 })
                            .height(if self.wide_home { 148.0 } else { 100.0 })
                            .stretch(Stretch::Uniform)
                            .automation_name(tr("Illustration de casque"))
                            .vertical_alignment(VerticalAlignment::Center),
                        StackPanel::new().grid_column(1).spacing(10.0).children((
                            TextBlock::new()
                                .text(v.title.clone())
                                .font_size(if self.wide_home { 28.0 } else { 22.0 })
                                .font_weight(FontWeight::SEMI_BOLD)
                                .text_wrapping(TextWrapping::Wrap),
                            small(model),
                            Grid::new()
                                .columns(vec![
                                    GridLength::Star(1.0);
                                    if self.wide_home { 3 } else { 1 }
                                ])
                                .rows(vec![GridLength::Auto; if self.wide_home { 1 } else { 3 }])
                                .column_spacing(8.0)
                                .row_spacing(6.0)
                                .children((
                                    badge(tr("Batterie"), &v.battery_text, "accueil-batterie"),
                                    Border::new()
                                        .grid_column(if self.wide_home { 1 } else { 0 })
                                        .grid_row(if self.wide_home { 0 } else { 1 })
                                        .content(badge(
                                            tr("Flux audio"),
                                            &summary.stream,
                                            "accueil-flux",
                                        )),
                                    Border::new()
                                        .grid_column(if self.wide_home { 2 } else { 0 })
                                        .grid_row(if self.wide_home { 0 } else { 2 })
                                        .content(badge(
                                            tr("Port"),
                                            &summary.wearing,
                                            "accueil-port",
                                        )),
                                )),
                            TextBlock::new()
                                .text(summary.charge.clone())
                                .automation_id("accueil-charge")
                                .font_size(12.0)
                                .opacity(0.8)
                                .text_wrapping(TextWrapping::Wrap),
                        )),
                    )),
            )
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
                        .value(
                            self.slider_edits
                                .value(SliderKey::Band(i as u8))
                                .map_or(db, |v| f64::from(v) / 10.0),
                        )
                        .height(180.0)
                        .is_enabled(v.writable)
                        .automation_name(trf("Bande {}", &[&band_label(i, count)]))
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .on_value_changed(context.callback({
                            let epoch = self.slider_edits.epoch();
                            move |value| Msg::Band(i, value, epoch)
                        })),
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
                tr("Mode audio"),
                RadioButtons::new()
                    .items_source(viewmodel::AUDIO_MODES.map(tr))
                    .max_columns(3)
                    .selected_index(index(sv.audio_mode))
                    .on_selection_changed(context.callback(Msg::AudioMode)),
            ),
            card(
                &trf("Égaliseur graphique — {} bandes", &[&count]),
                StackPanel::new().spacing(10.0).children((
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .keyed_children(bands),
                    switch_row(
                        tr("Amplification des basses"),
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
                                .placeholder_text(tr("Préréglage"))
                                .automation_name(tr("Préréglage"))
                                .is_enabled(v.writable)
                                .on_selection_changed(context.callback(Msg::Preset)),
                            Button::new()
                                .is_enabled(v.writable)
                                .on_click(context.message(Msg::Flat))
                                .content(tr("Réinitialiser")),
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
            tr("Préréglages et courbes"),
            StackPanel::new().spacing(10.0).children((
                if self.user_presets.iter().any(|p| p.topology.is_none()) {
                    StackPanel::new().spacing(4.0).children((
                        small(tr("Les anciens préréglages sont conservés. Associez-les aux fréquences documentées du M5 avant de les appliquer.").into()),
                        Button::new().is_enabled(self.snapshot.eq_topology().is_some())
                            .on_click(context.message(Msg::BindLegacyPresets))
                            .content(tr("Associer les anciens préréglages à cet égaliseur")),
                    ))
                } else { View::empty() },
                field_row(
                    TextBox::new()
                        .text(self.new_preset_name.clone())
                        .placeholder_text(tr("Nom du préréglage"))
                        .is_enabled(writable)
                        .on_text_changed(context.callback(Msg::NewPresetName))
                        .into(),
                    Button::new()
                        .is_enabled(writable && !self.new_preset_name.trim().is_empty())
                        .on_click(context.message(Msg::SavePreset))
                        .content(tr("Enregistrer la courbe")),
                ),
                Button::new()
                    .is_enabled(user.is_some())
                    .on_click(context.message(Msg::DeletePreset))
                    .content(user.map_or_else(
                        || tr("Supprimer le préréglage (choisir un préréglage personnel)").to_string(),
                        |p| trf("Supprimer « {} »", &[&p.name]),
                    )),
                field_row(
                    TextBox::new()
                        .text(self.gains_text.clone())
                        .placeholder_text(tr("Gains en dB : -3,5 ; -1 ; 0 ; …"))
                        .is_enabled(writable)
                        .on_text_changed(context.callback(Msg::GainsText))
                        .into(),
                    Button::new()
                        .is_enabled(writable && !self.gains_text.trim().is_empty())
                        .on_click(context.message(Msg::ApplyGainsText))
                        .content(tr("Appliquer")),
                ),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .on_click(context.message(Msg::ExportCurve))
                            .content(tr("Exporter la courbe")),
                        Button::new()
                            .on_click(context.message(Msg::ExportApo))
                            .content(tr("Exporter pour Equalizer APO")),
                        Button::new()
                            .is_enabled(writable)
                            .on_click(context.message(Msg::ImportCurve))
                            .content(tr("Importer une courbe…")),
                    )),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .is_enabled(writable)
                            .on_click(context.message(Msg::RememberA))
                            .content(tr("Mémoriser A")),
                        Button::new()
                            .is_enabled(writable && self.ab.ready())
                            .on_click(context.message(Msg::ToggleAb))
                            .content(self.ab.label()),
                    )),
                Button::new()
                    .is_enabled(profile_available)
                    .on_click(context.message(Msg::ExportProfile))
                    .content(tr("Exporter le profil auditif")),
                small(self.eq_notice.clone().unwrap_or_else(|| {
                    tr("Préréglages enregistrés sur ce PC (prereglages.txt). Courbe exportée dans Documents ; une courbe d'un autre modèle ou d'un autre nombre de bandes est refusée. Le profil auditif (lecture seule, aucune restauration) est une donnée personnelle : ne pas le publier.")
                        .to_string()
                })),
            )),
        )
    }

    fn settings(&self, context: &mut ViewContext<Self>) -> View {
        let transfer = card(tr("Sauvegarder et retrouver mes réglages"), StackPanel::new().spacing(8.0).children((
            TextBlock::new().text(tr("Scènes, préréglages, raccourcis, panneau et apparence. Aucun profil auditif. Relancer l'interface après import pour recharger les réglages ; un brouillon non enregistré n'est pas inclus.")).text_wrapping(TextWrapping::Wrap),
            CheckBox::new().is_checked(self.settings_battery).on_is_checked_changed(context.callback(Msg::SettingsBattery)).content(tr("Inclure aussi l'historique de batterie (export et import)")),
            Button::new().on_click(context.message(Msg::ExportSettings)).content(tr("Exporter les réglages…")),
            Button::new().on_click(context.message(Msg::PreviewSettings)).content(tr("Choisir un fichier à importer…")),
            TextBlock::new().text(self.settings_notice.clone()).text_wrapping(TextWrapping::Wrap),
            Button::new().is_enabled(self.settings_plan.is_some()).on_click(context.message(Msg::ImportSettings)).content(tr("Confirmer cet import")),
            Button::new().is_enabled(self.settings_plan.is_some()).on_click(context.message(Msg::CancelSettings)).content(tr("Annuler l'import")),
        )));
        let app = card(
            tr("Application"),
            StackPanel::new().spacing(8.0).children((
                TextBlock::new().text(if self.dark.is_none() {
                    tr("Thème (pour l'instant celui de Windows) :")
                } else {
                    tr("Thème :")
                }),
                RadioButtons::new()
                    .items_source([tr("Clair"), tr("Sombre")])
                    .max_columns(2)
                    .selected_index(self.dark.map(usize::from))
                    .on_selection_changed(context.callback(Msg::Theme)),
                TextBlock::new().text(tr("Langue :")),
                RadioButtons::new()
                    // Chaque langue dans sa propre langue.
                    .items_source([tr("Langue du système"), "Français", "English"])
                    .max_columns(3)
                    .selected_index(Some(self.language))
                    .on_selection_changed(context.callback(Msg::Language)),
                TextBlock::new()
                    .text(self.language_notice.clone().unwrap_or_default())
                    .text_wrapping(TextWrapping::Wrap),
                switch_row(
                    tr("Lancer à l'ouverture de session"),
                    tr("L'icône apparaît près de l'horloge, la fenêtre reste fermée"),
                    self.autostart,
                    true,
                    context.callback(Msg::Autostart),
                ),
                switch_row(
                    tr("Avis de nouvelle version"),
                    tr("Au lancement, lit la dernière publication sur GitHub. Seules votre adresse IP et le nom de l'application parviennent à GitHub ; rien d'autre n'est envoyé."),
                    self.update_enabled,
                    !demo::requested(),
                    context.callback(Msg::UpdateNotice),
                ),
                self.update_row(context),
                TextBlock::new()
                    .text(tr("Notifications"))
                    .font_weight(FontWeight::SEMI_BOLD),
                switch_row(
                    tr("Batterie faible"),
                    tr("Une fois sous 15 %"),
                    self.notifications.low_battery,
                    true,
                    context.callback(|on| Msg::Notification(NotificationKind::LowBattery, on)),
                ),
                switch_row(
                    tr("Connexion perdue et retrouvée"),
                    tr("Casque déconnecté, puis reconnecté"),
                    self.notifications.connection,
                    true,
                    context.callback(|on| Msg::Notification(NotificationKind::Connection, on)),
                ),
                switch_row(
                    tr("Casque allumé sans son"),
                    tr("Aucune lecture signalée depuis 30 minutes (désactivé par défaut)"),
                    self.notifications.idle,
                    true,
                    context.callback(|on| Msg::Notification(NotificationKind::Idle, on)),
                ),
            )),
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
                                    .placeholder_text(tr("aucun"))
                                    .on_text_changed(
                                        context.callback(move |t| Msg::HotkeyText(i, t)),
                                    ),
                            ),
                        )),
                )
            });
        let all_scenes: Vec<String> = self.scenes.iter().map(|s| s.name.clone()).collect();
        let shown = self.panel_prefs.shown_scenes(&all_scenes);
        let scene_boxes = all_scenes.iter().map(|name| {
            let n = name.clone();
            KeyedView::new(
                format!("panneau-{name}"),
                CheckBox::new()
                    .is_checked(shown.contains(name))
                    .on_is_checked_changed(
                        context.callback(move |on| Msg::PanelScene(n.clone(), on)),
                    )
                    .content(name.clone()),
            )
        });
        let panel = card(
            tr("Panneau rapide (clic sur l'icône)"),
            StackPanel::new().spacing(8.0).children((
                switch_row(
                    tr("Contrôle du bruit"),
                    "",
                    self.panel_prefs.noise,
                    true,
                    context.callback(Msg::PanelNoise),
                ),
                switch_row(
                    tr("Curseur ANC ↔ transparence"),
                    "",
                    self.panel_prefs.level,
                    true,
                    context.callback(Msg::PanelLevel),
                ),
                switch_row(
                    tr("Profil personnalisé"),
                    "",
                    self.panel_prefs.profile,
                    true,
                    context.callback(Msg::PanelProfile),
                ),
                TextBlock::new().text(trf(
                    "Scènes montrées ({} au plus) :",
                    &[&(momentum_core::quick_panel::MAX_PANEL_SCENES)],
                )),
                StackPanel::new().spacing(4.0).keyed_children(scene_boxes),
                TextBlock::new()
                    .text(self.panel_notice.clone().unwrap_or_else(|| {
                        tr("Anti-vent, batterie et « Ouvrir » sont toujours affichés.").to_string()
                    }))
                    .font_size(12.0)
                    .opacity(0.7)
                    .text_wrapping(TextWrapping::Wrap),
            )),
        );
        let shortcuts = card(
            tr("Raccourcis clavier"),
            StackPanel::new().spacing(8.0).children((
                StackPanel::new().spacing(6.0).keyed_children(hotkey_rows),
                Button::new()
                    .on_click(context.message(Msg::SaveHotkeys))
                    .content(tr("Enregistrer les raccourcis")),
                TextBlock::new()
                    .text(self.hotkey_notice.clone().unwrap_or_else(|| {
                        tr("Ctrl, Alt, Shift ou Win, plus une lettre, un chiffre ou F1 à F24 (« Ctrl+Alt+N »). Laisser vide pour désactiver. Une combinaison déjà prise par une autre application est signalée.")
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
                transfer,
                app,
                panel,
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
                tr("Appels"),
                StackPanel::new().spacing(8.0).children((
                    available_switch(
                        tr("Réponse automatique"),
                        tr("Décrocher en mettant le casque"),
                        st.auto_answer.on,
                        availability(v.connected, v.writable, self.snapshot.state.is_some_and(|s| s.auto_answer.is_some())),
                        context.callback(Msg::AutoAnswer),
                    ),
                    available_switch(
                        tr("Appels confortables"),
                        tr("Votre voix dans le casque pendant un appel"),
                        st.comfort_call.on,
                        availability(v.connected, v.writable, self.snapshot.state.is_some_and(|s| s.comfort_call.is_some())),
                        context.callback(Msg::Comfort),
                    ),
                )),
            ),
            card(
                tr("Alimentation"),
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new().text(tr("Extinction automatique")),
                    availability_hint(availability(v.connected, v.writable, self.snapshot.state.is_some_and(|s| s.auto_off_seconds.is_some()))),
                    ComboBox::new()
                        .items_source(viewmodel::AUTO_OFF_LABELS.map(tr))
                        .selected_index(index(st.auto_off))
                        .is_enabled(st.auto_off_enabled)
                        .automation_name(tr("Extinction automatique"))
                        .on_selection_changed(context.callback(Msg::AutoOff)),
                    available_switch(
                        tr("Protection de la batterie"),
                        tr("Limite la charge pour ménager la batterie"),
                        st.battery_protection.on,
                        availability(v.connected, v.writable, self.snapshot.state.is_some_and(|s| s.battery_protection.is_some())),
                        context.callback(Msg::BatteryProtection),
                    ),
                )),
            ),
            card(
                tr("Invites"),
                StackPanel::new().spacing(8.0).children((
                    availability_hint(availability(v.connected, v.writable, self.snapshot.state.is_some_and(|s| s.prompt_mode.is_some()))),
                    ComboBox::new()
                        .items_source(viewmodel::PROMPT_MODES.map(tr))
                        .selected_index(index(st.prompt_mode))
                        .is_enabled(st.prompts_enabled)
                        .automation_name(tr("Invites"))
                        .on_selection_changed(context.callback(Msg::PromptMode)),
                    TextBlock::new().text(if st.prompt_volume < 0 {
                        tr("Volume des invites : non lu").to_string()
                    } else {
                        trf("Volume des invites : {} %", &[&st.prompt_volume])
                    }),
                    availability_hint(availability(v.connected, v.writable, self.snapshot.info.as_ref().is_some_and(|i| i.prompt_volume.is_some()))),
                    Slider::new()
                        .minimum(0.0)
                        .maximum(100.0)
                        .step_frequency(1.0)
                        .value(self.slider_edits.value(SliderKey::PromptVolume).map_or(f64::from(st.prompt_volume.max(0)), |v| f64::from(momentum_core::device::prompt_volume_percent(v as i8))))
                        .is_enabled(st.prompt_volume_enabled)
                        .automation_name(tr("Volume des invites"))
                        .on_value_changed(context.callback({ let epoch = self.slider_edits.epoch(); move |v| Msg::PromptVolume(v, epoch) })),
                )),
            ),
            card(
                tr("Nom du casque"),
                StackPanel::new().spacing(8.0).children((
                    availability_hint(availability(v.connected, v.writable, self.snapshot.info.as_ref().is_some_and(|i| i.name.is_some()))),
                    Grid::new()
                        .columns([GridLength::Star(1.0), GridLength::Auto])
                        .column_spacing(8.0)
                        .children((
                            TextBox::new()
                                .text(self.new_name.clone())
                                .placeholder_text(if st.name.is_empty() {
                                    tr("non lu").to_string()
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
                                    .content(tr("Renommer")),
                            ),
                        )),
                    TextBlock::new()
                        .text(self.name_problem.clone().unwrap_or_else(|| {
                            tr("Lettres sans accent, chiffres et espaces, 32 caractères au plus. Le système et le téléphone peuvent garder l'ancien nom jusqu'à la prochaine connexion.")
                                .to_string()
                        }))
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            transfer,
            app,
            panel,
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
                                .content(tr("Appliquer")),
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
                tr("Scènes"),
                StackPanel::new().spacing(10.0).children((
                    TextBlock::new()
                        .text(
                            tr("Chaque réglage d'une scène est appliqué puis relu sur le casque, un par un ; une étape refusée arrête la scène."),
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
                                .content(tr("Ouvrir scenes.txt")),
                            Button::new()
                                .on_click(context.message(Msg::ReloadScenes))
                                .content(tr("Relire")),
                        )),
                )),
            ),
            card(
                tr("Enregistrer l'état actuel comme scène"),
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text(tr("Les valeurs non lues restent absentes ; aucun réglage du casque ne change."))
                        .text_wrapping(TextWrapping::Wrap),
                    TextBox::new()
                        .text(self.capture_scene_name.clone())
                        .placeholder_text(tr("Nom de la scène"))
                        .on_text_changed(context.callback(Msg::CaptureSceneName)),
                    CheckBox::new()
                        .is_checked(self.capture_scene_replace)
                        .on_is_checked_changed(context.callback(Msg::CaptureSceneReplace))
                        .content(tr("Remplacer la scène et son préréglage existants")),
                    Button::new()
                        .is_enabled(self.snapshot.state.is_some() && !self.busy && !self.capture_scene_name.trim().is_empty())
                        .on_click(context.message(Msg::CaptureScene))
                        .content(tr("Enregistrer l'état actuel comme scène")),
                )),
            ),
            self.scene_editor_card(context),
        ))
    }

    /// Carte « Modifier les scènes » : liste, formulaire, enregistrement.
    fn scene_editor_card(&self, context: &mut ViewContext<Self>) -> View {
        let e = &self.scene_editor;
        let f = &self.scene_form;
        let choice = |label: &str, items: Vec<String>, value: i32, field: SceneField| {
            Grid::new()
                .columns([GridLength::Pixel(160.0), GridLength::Star(1.0)])
                .children((
                    TextBlock::new()
                        .text(label)
                        .vertical_alignment(VerticalAlignment::Center),
                    Border::new().grid_column(1).content(
                        ComboBox::new()
                            .items_source(items)
                            .selected_index(index(value))
                            .automation_name(label)
                            .on_selection_changed(
                                context.callback(move |i| Msg::SceneChoice(field, i)),
                            ),
                    ),
                ))
        };
        let strings = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        let switches = || strings(&viewmodel::SCENE_SWITCH_CHOICES.map(tr));
        let form: View = if e.is_empty() {
            TextBlock::new()
                .text(tr("Aucune scène : « Nouvelle » en crée une."))
                .opacity(0.7)
                .into()
        } else {
            StackPanel::new().spacing(6.0).children((
                TextBox::new()
                    .text(f.name.clone())
                    .placeholder_text(tr("Nom de la scène"))
                    .on_text_changed(context.callback(Msg::SceneName)),
                choice(
                    tr("Mode audio"),
                    strings(&viewmodel::SCENE_SOUND_CHOICES.map(tr)),
                    f.sound,
                    SceneField::Sound,
                ),
                choice(
                    tr("Audio spatial"),
                    switches(),
                    f.spatial,
                    SceneField::Spatial,
                ),
                choice(
                    tr("Contrôle du bruit"),
                    strings(&viewmodel::SCENE_NOISE_CHOICES.map(tr)),
                    f.noise,
                    SceneField::Noise,
                ),
                TextBox::new()
                    .text(f.level.clone())
                    .placeholder_text(tr("Niveau ANC ↔ transparence, 0 à 100 (vide = inchangé)"))
                    .on_text_changed(context.callback(Msg::SceneLevel)),
                choice(
                    tr("Anti-vent"),
                    switches(),
                    f.anti_wind,
                    SceneField::AntiWind,
                ),
                choice(
                    tr("Commandes tactiles"),
                    switches(),
                    f.touch,
                    SceneField::Touch,
                ),
                choice(
                    tr("Préréglage d'égaliseur"),
                    e.eq_choices().to_vec(),
                    f.equalizer,
                    SceneField::Equalizer,
                ),
                choice(
                    tr("Amplification des basses"),
                    switches(),
                    f.bass_boost,
                    SceneField::Bass,
                ),
                choice(
                    tr("Automatique"),
                    strings(&viewmodel::SCENE_AUTO_CHOICES.map(tr)),
                    f.auto,
                    SceneField::Auto,
                ),
                TextBox::new()
                    .text(f.auto_time.clone())
                    .placeholder_text(tr("Heure (HH:MM)"))
                    .automation_name(tr("Heure (HH:MM)"))
                    .is_enabled(f.auto == 2)
                    .on_text_changed(context.callback(Msg::SceneAutoTime)),
                TextBlock::new()
                    .text(e.preview())
                    .font_size(12.0)
                    .opacity(0.8)
                    .text_wrapping(TextWrapping::Wrap),
            ))
        };
        let notice = e
            .problem()
            .map(|p| format!("⚠ {p}"))
            .or_else(|| self.scene_notice.clone())
            .unwrap_or_else(|| {
                tr("Le brouillon n'est écrit qu'avec « Enregistrer » ; rien n'est envoyé au casque. Les raccourcis et le panneau suivent les renommages.")
                    .to_string()
            });
        card(
            tr("Modifier les scènes"),
            StackPanel::new().spacing(10.0).children((
                RadioButtons::new()
                    .items_source(e.names())
                    .max_columns(3)
                    .selected_index((!e.is_empty()).then_some(e.selected()))
                    .on_selection_changed(context.callback(Msg::SceneSelect)),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .on_click(context.message(Msg::SceneNew))
                            .content(tr("Nouvelle")),
                        Button::new()
                            .is_enabled(!e.is_empty())
                            .on_click(context.message(Msg::SceneDuplicate))
                            .content(tr("Dupliquer")),
                        Button::new()
                            .is_enabled(!e.is_empty())
                            .on_click(context.message(Msg::SceneDelete))
                            .content(tr("Supprimer")),
                        Button::new()
                            .is_enabled(e.selected() > 0)
                            .on_click(context.message(Msg::SceneMove(-1)))
                            .content(tr("Monter")),
                        Button::new()
                            .is_enabled(e.selected() + 1 < e.names().len())
                            .on_click(context.message(Msg::SceneMove(1)))
                            .content(tr("Descendre")),
                    )),
                form,
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .style(ButtonStyle::Accent)
                            .is_enabled(e.can_save())
                            .on_click(context.message(Msg::SceneSave))
                            .content(tr("Enregistrer")),
                        Button::new()
                            .is_enabled(e.dirty())
                            .on_click(context.message(Msg::SceneDiscard))
                            .content(tr("Annuler les modifications")),
                    )),
                TextBlock::new()
                    .text(notice)
                    .font_size(12.0)
                    .opacity(0.8)
                    .text_wrapping(TextWrapping::Wrap),
            )),
        )
    }

    fn battery_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(page) = &self.battery else {
            return TextBlock::new().text(tr("Lecture de l'historique…")).into();
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
                    .text(trf("{} : {}", &[&row.label, &row.value]))
                    .font_size(12.0)
                    .opacity(0.7)
                    .text_wrapping(TextWrapping::Wrap),
            )
        });
        let summary = headset_summary(self.snapshot.state.as_ref());
        StackPanel::new().spacing(12.0).children((
            card(tr("État actuel"), StackPanel::new().spacing(8.0).children((
                TextBlock::new().text(summary.charge).automation_id("batterie-charge"),
                TextBlock::new().text(summary.wearing).automation_id("batterie-port"),
                small(tr("Le niveau de batterie et l'état de charge sont deux informations distinctes.")),
            ))),
            card(
                &trf("Batterie : {}", &[&page.view.current]),
                StackPanel::new().spacing(10.0).children((
                    RadioButtons::new()
                        .items_source(viewmodel::BATTERY_SPANS.iter().map(|(label, _)| tr(label)))
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
                            tr("Aucune mesure sur la période.").to_string()
                        } else {
                            trf("De {} (à gauche) à maintenant (à droite) ; repères à 100, 50 et 0 %.", &[&page.view.axis_start])
                        })
                        .font_size(12.0)
                        .opacity(0.7),
                    TextBlock::new()
                        .text(page.view.estimate.clone())
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            card(tr("Détails"), StackPanel::new().spacing(4.0).keyed_children(details)),
            card(
                tr("Historique"),
                StackPanel::new().spacing(8.0).children((
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Msg::ExportBattery))
                                .content(tr("Exporter (CSV)")),
                            Button::new()
                                .on_click(context.message(Msg::ClearBattery))
                                .content(tr("Effacer l'historique…")),
                        )),
                    TextBlock::new()
                        .text(self.battery_notice.clone().unwrap_or_else(|| {
                            tr("Mesures gardées sur ce PC (batterie.csv, 30 jours) ; l'export va dans Documents.")
                                .to_string()
                        }))
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            card(
                tr("Autonomie mesurée"),
                StackPanel::new().spacing(4.0).keyed_children(
                    page.autonomy.iter().enumerate().map(|(i, line)| {
                        KeyedView::new(
                            i as u64,
                            TextBlock::new()
                                .text(line.clone())
                                .text_wrapping(TextWrapping::Wrap),
                        )
                    }),
                ),
            ),
            card(
                tr("Journal des événements"),
                StackPanel::new().spacing(8.0).children((
                    StackPanel::new().spacing(2.0).keyed_children(
                        page.journal.iter().enumerate().map(|(i, line)| {
                            KeyedView::new(
                                i as u64,
                                TextBlock::new()
                                    .text(line.clone())
                                    .font_size(12.0)
                                    .text_wrapping(TextWrapping::Wrap),
                            )
                        }),
                    ),
                    TextBlock::new()
                        .text(if page.journal.is_empty() {
                            tr("Aucun événement noté pour l'instant.")
                        } else {
                            ""
                        })
                        .opacity(0.7),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Msg::ExportJournal))
                                .content(tr("Exporter le journal (CSV)")),
                            Button::new()
                                .on_click(context.message(Msg::ClearJournal))
                                .content(tr("Effacer le journal…")),
                        )),
                    TextBlock::new()
                        .text(tr("Connexions, modes de réduction de bruit, refus et scènes, notés par le service sur ce PC (journal.csv, 30 jours). Jamais envoyé."))
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                )),
            ),
        ))
    }

    fn audio_page(&self, context: &mut ViewContext<Self>) -> View {
        let connections: Vec<_> = self
            .snapshot
            .connections
            .iter()
            .enumerate()
            .map(|(i, c)| c.to_connection(i))
            .collect();
        let name = self.name();
        let v = viewmodel::audio_view(&viewmodel::AudioInputs {
            device_name: name.as_deref(),
            state: self.snapshot.state.as_ref(),
            connections: &connections,
            control_since: self.control_since.as_deref(),
            connections_at: self.connections_at.as_deref(),
            codec_since: self.codec_since.as_deref(),
            rate_since: self.rate_since.as_deref(),
            windows: self.windows_audio.as_ref(),
            windows_at: self.windows_at.as_deref(),
        });
        let rows = |prefix: &'static str, rows: Vec<viewmodel::InfoRow>| {
            StackPanel::new()
                .spacing(8.0)
                .keyed_children(rows.into_iter().enumerate().map(move |(i, r)| {
                    KeyedView::new(
                        format!("{prefix}-{i}"),
                        StackPanel::new().children((
                            TextBlock::new()
                                .text(trf("{} : {}", &[&r.label, &r.value]))
                                .text_wrapping(TextWrapping::Wrap),
                            TextBlock::new().text(r.source).font_size(12.0).opacity(0.7),
                        )),
                    )
                }))
        };
        let findings = v
            .findings
            .into_iter()
            .enumerate()
            .map(|(i, (problem, text))| {
                KeyedView::new(
                    format!("constat-{i}"),
                    TextBlock::new()
                        .text(format!("{} {text}", if problem { "⚠" } else { "•" }))
                        .text_wrapping(TextWrapping::Wrap),
                )
            });
        StackPanel::new().spacing(12.0).children((
            card(
                tr("Constats"),
                StackPanel::new().spacing(6.0).keyed_children(findings),
            ),
            card(tr("Pilotage du casque"), rows("pilotage", v.control)),
            card(
                tr("Sortie audio de Windows"),
                StackPanel::new().spacing(8.0).children((
                    rows("windows", v.windows),
                    Button::new()
                        .on_click(context.message(Msg::ReadWindowsAudio))
                        .content(tr("Relire les sorties de Windows")),
                )),
            ),
            card(tr("Flux Bluetooth"), rows("flux", v.stream)),
        ))
    }

    fn about(&self, context: &mut ViewContext<Self>) -> View {
        let info = self.snapshot.info.as_ref();
        let firmware = info
            .and_then(|i| i.firmware.clone())
            .unwrap_or_else(|| tr("non lu").into());
        let model = info.and_then(|i| i.model.clone()).unwrap_or_default();
        let copied = match self.diagnostic_copied {
            Some(true) => tr("Diagnostic copié dans le presse-papiers."),
            Some(false) => tr("Copie impossible (presse-papiers occupé) : réessayer."),
            None => "",
        };
        StackPanel::new().spacing(12.0).children((
            card(
                tr("OpenHeadset Control"),
                StackPanel::new().spacing(6.0).children((
                    TextBlock::new().text(trf("Client WinUI 3, version {}", &[&(env!("CARGO_PKG_VERSION"))])),
                    TextBlock::new().text(trf("Micrologiciel du casque : {} {}", &[&firmware, &model])),
                    TextBlock::new()
                        .text(tr("Projet non officiel, sans lien avec Sennheiser ni Sonova."))
                        .text_wrapping(TextWrapping::Wrap),
                    TextBlock::new()
                        .text(tr("Licence : MIT OU Apache-2.0"))
                        .opacity(0.7),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Msg::OpenGuide))
                                .content(tr("Guide d'utilisation")),
                            Button::new()
                                .on_click(context.message(Msg::AssistantOpen))
                                .content(tr("Assistant de premier lancement")),
                        )),
                )),
            ),
            card(
                tr("Diagnostic"),
                StackPanel::new().spacing(8.0).children((
                    TextBlock::new()
                        .text(
                            tr("Texte à joindre à un rapport : ni adresse Bluetooth, ni numéro de série, ni noms d'appareils, ni dossier personnel."),
                        )
                        .font_size(12.0)
                        .opacity(0.7)
                        .text_wrapping(TextWrapping::Wrap),
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .on_click(context.message(Msg::CopyDiagnostic))
                                .content(tr("Copier le diagnostic")),
                            Button::new()
                                .on_click(context.message(Msg::ReportPreview))
                                .content(tr("Signaler un problème…")),
                        )),
                    TextBlock::new().text(copied),
                    TextBlock::new().text(if self.report_failed {
                        tr("Navigateur introuvable : copier le diagnostic et ouvrir github.com/Asfax03L/openheadset-control/issues.")
                    } else {
                        ""
                    })
                    .text_wrapping(TextWrapping::Wrap),
                )),
            ),
            self.report_card(context),
        ))
    }

    /// Assistant de premier lancement : une étape à la fois, à la place de
    /// la page.
    fn assistant_view(&self, context: &mut ViewContext<Self>) -> View {
        let steps = assistant::steps(Platform::Windows);
        let index = self.assistant.unwrap_or(0).min(steps.len() - 1);
        let step = &steps[index];
        let last = index + 1 == steps.len();
        let action: View = match step.action {
            Some(StepAction::BluetoothSettings) => Button::new()
                .on_click(context.message(Msg::AssistantBluetooth))
                .content(tr("Ouvrir les paramètres Bluetooth")),
            None => StackPanel::new().into(),
        };
        let connection = if step.shows_connection {
            assistant::connection_text(self.snapshot.state.is_some())
        } else {
            ""
        };
        card(
            step.title,
            StackPanel::new().spacing(10.0).children((
                TextBlock::new()
                    .text(trf(
                        "Premier lancement — étape {} sur {}",
                        &[&(index + 1), &steps.len()],
                    ))
                    .font_size(12.0)
                    .opacity(0.7),
                TextBlock::new()
                    .text(step.text)
                    .text_wrapping(TextWrapping::Wrap),
                action,
                TextBlock::new()
                    .text(connection)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .text_wrapping(TextWrapping::Wrap),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .is_enabled(index > 0)
                            .on_click(context.message(Msg::AssistantPrevious))
                            .content(tr("Précédent")),
                        Button::new()
                            .on_click(context.message(if last {
                                Msg::AssistantClose
                            } else {
                                Msg::AssistantNext
                            }))
                            .content(if last { tr("Terminer") } else { tr("Suivant") }),
                        Button::new()
                            .on_click(context.message(Msg::AssistantClose))
                            .content(tr("Passer l'assistant")),
                    )),
            )),
        )
    }

    /// Aperçu du rapport de bogue : le diagnostic exact qui sera prérempli,
    /// relu avant d'ouvrir le formulaire GitHub (rien n'est envoyé ici).
    fn report_card(&self, context: &mut ViewContext<Self>) -> View {
        let Some(text) = &self.report else {
            return StackPanel::new().into();
        };
        card(
            tr("Signaler un problème"),
            StackPanel::new().spacing(8.0).children((
                TextBlock::new()
                    .text(tr("Ce texte sera prérempli dans le formulaire « Bogue » de GitHub, ouvert dans le navigateur. Relisez-le : rien n'est envoyé tant que vous ne validez pas le formulaire sur GitHub (compte GitHub nécessaire)."))
                    .text_wrapping(TextWrapping::Wrap),
                Border::new()
                    .background(ThemeBrush::SolidBackground)
                    .corner_radius(4.0)
                    .padding(8.0)
                    .content(
                        TextBlock::new()
                            .text(text.clone())
                            .font_size(12.0)
                            .is_text_selection_enabled(true)
                            .text_wrapping(TextWrapping::Wrap)
                            .automation_name(tr("Diagnostic à envoyer")),
                    ),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((
                        Button::new()
                            .on_click(context.message(Msg::ReportOpen))
                            .content(tr("Ouvrir le formulaire sur GitHub")),
                        Button::new()
                            .on_click(context.message(Msg::ReportCancel))
                            .content(tr("Annuler")),
                    )),
            )),
        )
    }
}
