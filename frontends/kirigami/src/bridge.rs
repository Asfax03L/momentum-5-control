//! Pont Rust ↔ QML : l'objet `Headset` vu par l'interface Kirigami.
//!
//! Client du service `momentum-daemon` (API D-Bus `org.openheadset.Control1`,
//! [`momentum_client`]), seul propriétaire de la connexion au casque : la
//! fenêtre peut s'ouvrir et se fermer sans couper le casque, et `ctl` ou une
//! autre interface restent utilisables en même temps. Le service démarre
//! tout seul au premier appel (activation D-Bus).
//!
//! L'objet recopie dans ses propriétés ce que calculent les modèles de vue
//! de `momentum-core` : toutes les interfaces affichent les mêmes valeurs,
//! avec les mêmes règles. Une valeur n'est présentée qu'une fois relue sur
//! le casque ; `busy` indique qu'une commande attend sa confirmation.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    unsafe extern "C++" {
        include!("momentum-kirigami/src/identity.h");
        /// Nom, fichier .desktop et icône de l'application (identity.cpp).
        fn openheadset_set_identity();
        /// Thème demandé à Qt : 0 système, 1 clair, 2 sombre (identity.cpp).
        fn openheadset_set_color_scheme(scheme: i32);
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        // État de la connexion.
        #[qproperty(bool, connected)]
        #[qproperty(bool, writable)]
        #[qproperty(bool, busy)]
        #[qproperty(QString, title)]
        #[qproperty(QString, connection)]
        #[qproperty(QString, waiting)]
        #[qproperty(QString, banner)]
        #[qproperty(bool, banner_warning)]
        #[qproperty(i32, battery)]
        #[qproperty(QString, battery_text)]
        #[qproperty(QString, stream)]
        #[qproperty(QString, model)]
        #[qproperty(QString, firmware)]
        // Contrôle du bruit.
        #[qproperty(QStringList, noise_modes)]
        #[qproperty(i32, noise_mode)]
        #[qproperty(i32, level)]
        #[qproperty(bool, anti_wind)]
        // Interrupteurs : valeur et disponibilité (lu et modifiable).
        #[qproperty(bool, spatial)]
        #[qproperty(bool, spatial_enabled)]
        #[qproperty(bool, head_tracking)]
        #[qproperty(bool, head_tracking_enabled)]
        #[qproperty(bool, touch)]
        #[qproperty(bool, touch_enabled)]
        #[qproperty(bool, multipoint)]
        #[qproperty(bool, multipoint_enabled)]
        #[qproperty(bool, smart_pause)]
        #[qproperty(bool, smart_pause_enabled)]
        #[qproperty(bool, auto_answer)]
        #[qproperty(bool, auto_answer_enabled)]
        #[qproperty(bool, comfort)]
        #[qproperty(bool, comfort_enabled)]
        #[qproperty(bool, battery_protection)]
        #[qproperty(bool, battery_protection_enabled)]
        #[qproperty(QStringList, auto_off_labels)]
        #[qproperty(i32, auto_off)]
        #[qproperty(bool, auto_off_enabled)]
        #[qproperty(QStringList, prompt_modes)]
        #[qproperty(i32, prompt_mode)]
        #[qproperty(bool, prompts_enabled)]
        #[qproperty(i32, prompt_volume)]
        #[qproperty(bool, prompt_volume_enabled)]
        #[qproperty(QString, headset_name)]
        #[qproperty(bool, rename_enabled)]
        // Son.
        #[qproperty(QStringList, audio_modes)]
        #[qproperty(i32, audio_mode)]
        #[qproperty(bool, modes_enabled)]
        #[qproperty(bool, profile_available)]
        #[qproperty(QStringList, preset_names)]
        #[qproperty(i32, preset_index)]
        /// Préréglage personnel appliqué (supprimable), vide sinon.
        #[qproperty(QString, user_preset)]
        /// Résultat de la dernière action sur les préréglages et courbes.
        #[qproperty(QString, eq_notice)]
        #[qproperty(bool, ab_ready)]
        #[qproperty(QString, ab_label)]
        #[qproperty(bool, bass_boost)]
        #[qproperty(i32, band_count)]
        #[qproperty(f64, min_gain)]
        #[qproperty(f64, max_gain)]
        // Change à chaque relecture de l'égaliseur (rafraîchit les curseurs).
        #[qproperty(i32, eq_revision)]
        // Icône de la zone de notification présente : fermer la fenêtre la
        // cache au lieu de quitter.
        #[qproperty(bool, tray_available)]
        // Démarré par `--cache` (ouverture de session) : fenêtre cachée.
        #[qproperty(bool, start_hidden)]
        // Page ouverte au démarrage (`--page son`…, noms de WinUI), pour les essais.
        #[qproperty(QString, start_page)]
        // Appareils appairés au casque (lus par `connection_*`).
        #[qproperty(i32, connection_count)]
        #[qproperty(i32, connection_revision)]
        // Scènes (`scenes.txt`) et résultat de la dernière appliquée.
        #[qproperty(QStringList, scene_names)]
        #[qproperty(QStringList, scene_previews)]
        #[qproperty(QString, scene_problems)]
        // Éditeur de scènes (brouillon de scenes.txt).
        #[qproperty(QStringList, scene_edit_names)]
        #[qproperty(i32, scene_edit_selected)]
        #[qproperty(bool, scene_edit_dirty)]
        #[qproperty(bool, scene_edit_can_save)]
        #[qproperty(QString, scene_edit_preview)]
        #[qproperty(QString, scene_edit_notice)]
        #[qproperty(QStringList, scene_eq_choices)]
        #[qproperty(QStringList, scene_sound_choices)]
        #[qproperty(QStringList, scene_switch_choices)]
        #[qproperty(QStringList, scene_noise_choices)]
        #[qproperty(QString, scene_form_name)]
        #[qproperty(i32, scene_form_sound)]
        #[qproperty(i32, scene_form_spatial)]
        #[qproperty(i32, scene_form_noise)]
        #[qproperty(QString, scene_form_level)]
        #[qproperty(i32, scene_form_anti_wind)]
        #[qproperty(i32, scene_form_touch)]
        #[qproperty(i32, scene_form_equalizer)]
        #[qproperty(i32, scene_form_bass)]
        #[qproperty(QString, scene_result)]
        #[qproperty(bool, scene_result_ok)]
        #[qproperty(QString, scenes_url)]
        // Historique de la batterie (fichier tenu par le service).
        #[qproperty(QStringList, battery_spans)]
        #[qproperty(i32, battery_span)]
        #[qproperty(QString, battery_current)]
        #[qproperty(QString, battery_estimate)]
        #[qproperty(QString, battery_axis)]
        #[qproperty(QStringList, battery_details)]
        #[qproperty(i32, battery_revision)]
        // Page « Connexion et audio » : change à chaque mise à jour.
        #[qproperty(i32, audio_revision)]
        // Thème choisi : -1 aucun (système), 0 clair, 1 sombre.
        #[qproperty(i32, theme_choice)]
        // Lancement à l'ouverture de session.
        #[qproperty(bool, autostart)]
        // Lancé dans le bac à sable Flatpak (Configuration du système hors
        // d'atteinte).
        #[qproperty(bool, sandboxed)]
        type Headset = super::HeadsetRust;

        /// Clic sur l'icône de la zone de notification : montrer la fenêtre.
        #[qsignal]
        fn show_requested(self: Pin<&mut Headset>);
        /// « Quitter » dans le menu de l'icône.
        #[qsignal]
        fn quit_requested(self: Pin<&mut Headset>);
        /// Demande d'ouvrir la fenêtre sur la page Égaliseur (menu).
        #[qsignal]
        fn equalizer_requested(self: Pin<&mut Headset>);

        /// Gain de la bande `index`, en dB.
        #[qinvokable]
        fn band_gain(self: &Headset, index: i32) -> f64;
        /// Fréquence de la bande `index` (« 325 Hz »).
        #[qinvokable]
        fn band_label(self: &Headset, index: i32) -> QString;

        #[qinvokable]
        fn set_noise_mode_index(self: Pin<&mut Headset>, index: i32);
        #[qinvokable]
        fn send_level(self: Pin<&mut Headset>, level: i32);
        #[qinvokable]
        fn send_anti_wind(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_spatial(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_head_tracking(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_touch(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_multipoint(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_smart_pause(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_auto_answer(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_comfort(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_battery_protection(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_auto_off_index(self: Pin<&mut Headset>, index: i32);
        #[qinvokable]
        fn send_prompt_mode_index(self: Pin<&mut Headset>, index: i32);
        /// Volume des invites en % (échelle de l'app).
        #[qinvokable]
        fn send_prompt_volume(self: Pin<&mut Headset>, percent: i32);
        /// Renomme le casque ; renvoie pourquoi le nom est refusé (vide si
        /// envoyé).
        #[qinvokable]
        fn rename(self: Pin<&mut Headset>, name: QString) -> QString;
        #[qinvokable]
        fn send_audio_mode_index(self: Pin<&mut Headset>, index: i32);
        #[qinvokable]
        fn send_band(self: Pin<&mut Headset>, index: i32, db: f64);
        #[qinvokable]
        fn send_bass_boost(self: Pin<&mut Headset>, on: bool);
        #[qinvokable]
        fn send_flat(self: Pin<&mut Headset>);
        #[qinvokable]
        fn apply_preset_index(self: Pin<&mut Headset>, index: i32);
        /// Enregistre la courbe du casque comme préréglage personnel ;
        /// `true` si c'est fait.
        #[qinvokable]
        fn save_preset(self: Pin<&mut Headset>, name: QString) -> bool;
        #[qinvokable]
        fn delete_user_preset(self: Pin<&mut Headset>);
        /// Gains saisis en dB (« -3,5 ; -1 ; … ») ; `true` si envoyés.
        #[qinvokable]
        fn apply_gains_text(self: Pin<&mut Headset>, text: QString) -> bool;
        /// Exporte la courbe dans Documents.
        #[qinvokable]
        fn export_curve(self: Pin<&mut Headset>);
        /// Importe une courbe (URL `file://` de la boîte de dialogue).
        #[qinvokable]
        fn import_curve(self: Pin<&mut Headset>, url: QString);
        /// Exporte le profil auditif dans Documents (par le service).
        #[qinvokable]
        fn export_profile(self: Pin<&mut Headset>);
        #[qinvokable]
        fn remember_a(self: Pin<&mut Headset>);
        #[qinvokable]
        fn toggle_ab(self: Pin<&mut Headset>);

        #[qinvokable]
        fn connection_name(self: &Headset, index: i32) -> QString;
        /// « Connecté », « Non connecté », « Connecté (ce PC) ».
        #[qinvokable]
        fn connection_status(self: &Headset, index: i32) -> QString;
        /// « Connecter » ou « Déconnecter ».
        #[qinvokable]
        fn connection_action(self: &Headset, index: i32) -> QString;
        /// Faux pour ce PC et en lecture seule.
        #[qinvokable]
        fn connection_can_act(self: &Headset, index: i32) -> bool;
        #[qinvokable]
        fn toggle_connection(self: Pin<&mut Headset>, index: i32);

        /// Relit `scenes.txt` (créé avec des exemples s'il manque).
        #[qinvokable]
        fn reload_scenes(self: Pin<&mut Headset>);
        #[qinvokable]
        fn apply_scene(self: Pin<&mut Headset>, index: i32);
        #[qinvokable]
        fn scene_edit_select(self: Pin<&mut Headset>, index: i32);
        /// « nouvelle », « dupliquer », « supprimer », « monter »,
        /// « descendre », « enregistrer », « annuler ».
        #[qinvokable]
        fn scene_edit_action(self: Pin<&mut Headset>, action: QString);
        /// Formulaire saisi : nom, niveau, puis les choix des listes
        /// (mode audio, spatial, bruit, anti-vent, tactile, égaliseur,
        /// basses) séparés par des virgules.
        #[qinvokable]
        fn set_scene_form(self: Pin<&mut Headset>, name: QString, level: QString, choices: QString);

        /// Relit l'historique de la batterie sur la période choisie.
        #[qinvokable]
        fn reload_battery(self: Pin<&mut Headset>, span: i32);
        /// Lignes d'une partie de la page « Connexion et audio » :
        /// `constats` (« ⚠ texte »), `pilotage`, `systeme`, `flux`
        /// (« libellé : valeur » puis la source après une tabulation).
        #[qinvokable]
        fn audio_rows(self: &Headset, section: QString) -> QStringList;
        /// Copie `batterie.csv` dans Documents ; message à afficher.
        #[qinvokable]
        fn export_battery(self: &Headset) -> QString;
        /// Efface l'historique (fait par le service) puis le relit.
        #[qinvokable]
        fn clear_battery(self: Pin<&mut Headset>) -> QString;
        /// Courbe de la batterie en tracé SVG, à la taille donnée.
        #[qinvokable]
        fn battery_chart(self: &Headset, width: f64, height: f64) -> QString;

        #[qinvokable]
        fn set_autostart_enabled(self: Pin<&mut Headset>, on: bool);
        /// Ouvre la page Raccourcis de la Configuration du système.
        #[qinvokable]
        fn open_shortcut_settings(self: &Headset);
        /// Enregistre et applique le thème (0 clair, 1 sombre).
        #[qinvokable]
        fn choose_theme(self: Pin<&mut Headset>, choice: i32);
        /// Diagnostic expurgé à copier dans un rapport.
        #[qinvokable]
        fn diagnostic(self: &Headset) -> QString;
    }

    impl cxx_qt::Threading for Headset {}
    impl cxx_qt::Initialize for Headset {}
}

use std::pin::Pin;
use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QString, QStringList};
use momentum_client::dbus::Client;
use momentum_client::demo::{self, Demo};
use momentum_client::tracker::Tracker;
use momentum_client::{Command, Event, Outcome, Snapshot};
use momentum_core::audio_check::LINUX_OUTPUTS_UNREAD;
use momentum_core::battery_log::BatteryLog;
use momentum_core::device::{
    AUTO_OFF_CHOICES, NoiseModeRequest, PromptMode, check_name, prompt_volume_from_percent,
};
use momentum_core::diagnostic::{self, DiagnosticInput};
use momentum_core::hotkeys::{self, HotkeyAction};
use momentum_core::paths;
use momentum_core::presets::{self, AbCompare, Preset};
use momentum_core::scene_editor::{self, SceneEditor};
use momentum_core::scenes::Scene;
use momentum_core::tray_menu::{self, MenuAction, MenuContext};
use momentum_core::ui::{IconKind, band_label, tooltip};
use momentum_core::viewmodel;

use crate::autostart;
use crate::tray::{Tray, TrayView};

/// Pause avant de recontacter un service injoignable.
const RETRY: Duration = Duration::from_secs(3);
/// Incidents gardés pour le diagnostic.
const MAX_INCIDENTS: usize = 20;

/// Heure locale « 14:05:32 ».
fn clock() -> String {
    let t = cxx_qt_lib::QTime::current_time();
    format!("{:02}:{:02}:{:02}", t.hour(), t.minute(), t.second())
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Copie `batterie.csv` dans Documents ; renvoie le message à afficher.
fn export_battery_history() -> String {
    let Some(source) = paths::battery_file() else {
        return "Export impossible : dossier des données introuvable.".to_string();
    };
    let Some(target) = paths::export_file("momentum-batterie", "csv", unix_now()) else {
        return "Export impossible : dossier Documents introuvable.".to_string();
    };
    match std::fs::copy(&source, &target) {
        Ok(_) => format!("Historique exporté dans {}.", target.display()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            "Aucun historique à exporter pour l'instant.".to_string()
        }
        Err(e) => format!("Export de l'historique impossible : {e}."),
    }
}

/// Remet un tracé du repère 1000 × 100 (`battery_log::chart_path`) à la
/// taille de la zone de dessin.
fn scale_path(path: &str, width: f64, height: f64) -> String {
    let mut out = Vec::new();
    let mut is_x = true;
    for token in path.split_whitespace() {
        if let Ok(value) = token.parse::<f64>() {
            let scaled = if is_x {
                value * width / 1000.0
            } else {
                value * height / 100.0
            };
            out.push(format!("{scaled:.1}"));
            is_x = !is_x;
        } else {
            out.push(token.to_string());
            is_x = true;
        }
    }
    out.join(" ")
}

/// Données Rust de l'objet `Headset` : propriétés exposées à QML, puis
/// état interne (non exposé).
pub struct HeadsetRust {
    connected: bool,
    writable: bool,
    busy: bool,
    title: QString,
    connection: QString,
    waiting: QString,
    banner: QString,
    banner_warning: bool,
    battery: i32,
    battery_text: QString,
    stream: QString,
    model: QString,
    firmware: QString,
    noise_modes: QStringList,
    noise_mode: i32,
    level: i32,
    anti_wind: bool,
    spatial: bool,
    spatial_enabled: bool,
    head_tracking: bool,
    head_tracking_enabled: bool,
    touch: bool,
    touch_enabled: bool,
    multipoint: bool,
    multipoint_enabled: bool,
    smart_pause: bool,
    smart_pause_enabled: bool,
    auto_answer: bool,
    auto_answer_enabled: bool,
    comfort: bool,
    comfort_enabled: bool,
    battery_protection: bool,
    battery_protection_enabled: bool,
    auto_off_labels: QStringList,
    auto_off: i32,
    auto_off_enabled: bool,
    prompt_modes: QStringList,
    prompt_mode: i32,
    prompts_enabled: bool,
    prompt_volume: i32,
    prompt_volume_enabled: bool,
    headset_name: QString,
    rename_enabled: bool,
    audio_modes: QStringList,
    audio_mode: i32,
    modes_enabled: bool,
    profile_available: bool,
    preset_names: QStringList,
    preset_index: i32,
    user_preset: QString,
    eq_notice: QString,
    ab_ready: bool,
    ab_label: QString,
    bass_boost: bool,
    band_count: i32,
    min_gain: f64,
    max_gain: f64,
    eq_revision: i32,
    tray_available: bool,
    start_hidden: bool,
    start_page: QString,
    connection_count: i32,
    connection_revision: i32,
    scene_names: QStringList,
    scene_previews: QStringList,
    scene_problems: QString,
    scene_edit_names: QStringList,
    scene_edit_selected: i32,
    scene_edit_dirty: bool,
    scene_edit_can_save: bool,
    scene_edit_preview: QString,
    scene_edit_notice: QString,
    scene_eq_choices: QStringList,
    scene_sound_choices: QStringList,
    scene_switch_choices: QStringList,
    scene_noise_choices: QStringList,
    scene_form_name: QString,
    scene_form_sound: i32,
    scene_form_spatial: i32,
    scene_form_noise: i32,
    scene_form_level: QString,
    scene_form_anti_wind: i32,
    scene_form_touch: i32,
    scene_form_equalizer: i32,
    scene_form_bass: i32,
    scene_editor: SceneEditor,
    /// Formulaire tel que saisi (même invalide).
    scene_form: viewmodel::SceneForm,
    scene_result: QString,
    scene_result_ok: bool,
    scenes_url: QString,
    battery_spans: QStringList,
    battery_span: i32,
    battery_current: QString,
    battery_estimate: QString,
    battery_axis: QString,
    battery_details: QStringList,
    battery_revision: i32,
    audio_revision: i32,
    theme_choice: i32,
    /// Heure de chaque lecture (page « Connexion et audio »).
    control_since: Option<String>,
    connections_at: Option<String>,
    codec_since: Option<String>,
    rate_since: Option<String>,
    autostart: bool,
    sandboxed: bool,

    commands: Option<Sender<(Command, u64)>>,
    tray: Option<Tray>,
    /// État du casque vu par le service, suivi événement par événement.
    snapshot: Snapshot,
    /// Service injoignable (affiché à la place de la recherche du casque).
    service_error: Option<String>,
    connection_rows: Vec<viewmodel::ConnectionRow>,
    scenes: Vec<Scene>,
    /// Tracé de la batterie dans le repère 1000 × 100 de `chart_path`.
    battery_path: String,
    /// Incidents récents pour le diagnostic : (heure Unix, message).
    incidents: Vec<(u64, String)>,
    user_presets: Vec<Preset>,
    ab: AbCompare,
    /// Numéro de la dernière commande envoyée (efface l'attente périmée).
    /// Demandes envoyées, suivies jusqu'à leur résultat (`Event::Done`).
    requests: Tracker,
}

fn string_list<'a>(items: impl IntoIterator<Item = &'a str>) -> QStringList {
    let mut list = QStringList::default();
    for item in items {
        list.append(QString::from(item));
    }
    list
}

impl Default for HeadsetRust {
    fn default() -> Self {
        Self {
            connected: false,
            writable: false,
            busy: false,
            title: QString::from("MOMENTUM 5"),
            connection: QString::from("Recherche du casque…"),
            waiting: QString::default(),
            banner: QString::default(),
            banner_warning: false,
            battery: -1,
            battery_text: QString::default(),
            stream: QString::default(),
            model: QString::default(),
            firmware: QString::default(),
            noise_modes: string_list(viewmodel::NOISE_MODES),
            noise_mode: -1,
            level: 0,
            anti_wind: false,
            spatial: false,
            spatial_enabled: false,
            head_tracking: false,
            head_tracking_enabled: false,
            touch: false,
            touch_enabled: false,
            multipoint: false,
            multipoint_enabled: false,
            smart_pause: false,
            smart_pause_enabled: false,
            auto_answer: false,
            auto_answer_enabled: false,
            comfort: false,
            comfort_enabled: false,
            battery_protection: false,
            battery_protection_enabled: false,
            auto_off_labels: string_list(viewmodel::AUTO_OFF_LABELS),
            auto_off: -1,
            auto_off_enabled: false,
            prompt_modes: string_list(viewmodel::PROMPT_MODES),
            prompt_mode: -1,
            prompts_enabled: false,
            prompt_volume: -1,
            prompt_volume_enabled: false,
            headset_name: QString::default(),
            rename_enabled: false,
            audio_modes: string_list(viewmodel::AUDIO_MODES),
            audio_mode: -1,
            modes_enabled: false,
            profile_available: false,
            preset_names: QStringList::default(),
            preset_index: -1,
            user_preset: QString::default(),
            eq_notice: QString::default(),
            ab_ready: false,
            ab_label: QString::from("Écouter A"),
            bass_boost: false,
            band_count: 0,
            min_gain: -6.0,
            max_gain: 6.0,
            eq_revision: 0,
            tray_available: false,
            start_hidden: std::env::args().any(|a| a == autostart::HIDDEN_FLAG),
            start_page: std::env::args()
                .skip_while(|a| a != "--page")
                .nth(1)
                .map_or_else(QString::default, |p| QString::from(p.as_str())),
            connection_count: 0,
            connection_revision: 0,
            scene_names: QStringList::default(),
            scene_previews: QStringList::default(),
            scene_problems: QString::default(),
            scene_edit_names: QStringList::default(),
            scene_edit_selected: 0,
            scene_edit_dirty: false,
            scene_edit_can_save: false,
            scene_edit_preview: QString::default(),
            scene_edit_notice: QString::default(),
            scene_eq_choices: QStringList::default(),
            scene_sound_choices: string_list(viewmodel::SCENE_SOUND_CHOICES.iter().copied()),
            scene_switch_choices: string_list(viewmodel::SCENE_SWITCH_CHOICES.iter().copied()),
            scene_noise_choices: string_list(viewmodel::SCENE_NOISE_CHOICES.iter().copied()),
            scene_form_name: QString::default(),
            scene_form_sound: 0,
            scene_form_spatial: 0,
            scene_form_noise: 0,
            scene_form_level: QString::default(),
            scene_form_anti_wind: 0,
            scene_form_touch: 0,
            scene_form_equalizer: 0,
            scene_form_bass: 0,
            scene_editor: SceneEditor::default(),
            scene_form: viewmodel::SceneForm::default(),
            scene_result: QString::default(),
            scene_result_ok: true,
            scenes_url: paths::scenes_file()
                .map(|p| QString::from(format!("file://{}", p.display()).as_str()))
                .unwrap_or_default(),
            battery_spans: string_list(viewmodel::BATTERY_SPANS.iter().map(|(label, _)| *label)),
            battery_span: 0,
            battery_current: QString::default(),
            battery_estimate: QString::default(),
            battery_axis: QString::default(),
            battery_details: QStringList::default(),
            battery_revision: 0,
            audio_revision: 0,
            theme_choice: momentum_core::appearance::load().map_or(-1, i32::from),
            control_since: None,
            connections_at: None,
            codec_since: None,
            rate_since: None,
            autostart: autostart::is_enabled(),
            sandboxed: autostart::in_flatpak(),
            commands: None,
            tray: None,
            snapshot: Snapshot::default(),
            service_error: None,
            connection_rows: Vec::new(),
            scenes: Vec::new(),
            battery_path: String::new(),
            incidents: Vec::new(),
            user_presets: paths::load_user_presets(),
            ab: AbCompare::default(),
            requests: Tracker::default(),
        }
    }
}

impl cxx_qt::Initialize for qobject::Headset {
    /// Ouvre deux threads vers le service : l'un suit ses événements,
    /// l'autre lui transmet les commandes. Tout revient sur le thread Qt.
    fn initialize(mut self: Pin<&mut Self>) {
        let (tx, rx) = mpsc::channel::<(Command, u64)>();
        self.as_mut().rust_mut().commands = Some(tx.clone());
        crate::instance::attach(self.qt_thread());
        self.as_mut().reload_scenes();

        // Icône KDE : absente si aucune zone de notification n'écoute
        // (autre bureau) ; la fenêtre reste alors la seule interface.
        match Tray::spawn(self.qt_thread()) {
            Ok(handle) => {
                self.as_mut().rust_mut().tray = Some(handle);
                self.as_mut().set_tray_available(true);
            }
            Err(e) => tracing::warn!(%e, "icône de la zone de notification indisponible"),
        }

        // Raccourcis globaux KDE (absents hors de Plasma) ; une action par
        // scène connue au démarrage.
        let qt_thread = self.qt_thread();
        let scene_names: Vec<String> = self.rust().scenes.iter().map(|s| s.name.clone()).collect();
        let shortcuts = std::thread::Builder::new()
            .name("raccourcis-kde".into())
            .spawn(move || {
                let result = crate::shortcuts::run(&scene_names, |action| {
                    let _ = qt_thread.queue(move |headset| headset.on_shortcut(action));
                });
                if let Err(e) = result {
                    tracing::warn!(%e, "raccourcis KDE indisponibles");
                }
            });
        if let Err(e) = shortcuts {
            tracing::error!(%e, "lancement du thread des raccourcis");
        }

        // Mode démonstration : casque simulé, le service n'est pas joint.
        if demo::requested() {
            let mut simulated = Demo::new();
            let snapshot = simulated.snapshot();
            let qt_thread = self.qt_thread();
            let _ = qt_thread.queue(move |headset| headset.on_snapshot(snapshot));
            let spawned = std::thread::Builder::new()
                .name("demo".into())
                .spawn(move || {
                    for (last_id, (command, _)) in (1u64..).zip(rx) {
                        let _ = qt_thread.queue(move |headset| headset.on_submitted(last_id));
                        // Délai d'un vrai casque : l'interface montre son envoi.
                        std::thread::sleep(Duration::from_millis(450));
                        let mut events = simulated.apply(command);
                        events.push(Event::Done {
                            id: last_id,
                            outcome: Outcome::Applied,
                            details: None,
                        });
                        for event in events {
                            let _ = qt_thread.queue(move |headset| headset.on_event(event));
                        }
                    }
                });
            if let Err(e) = spawned {
                tracing::error!(%e, "lancement du casque simulé");
            }
            return;
        }

        let qt_thread = self.qt_thread();
        let listener = std::thread::Builder::new()
            .name("service-evenements".into())
            .spawn(move || {
                loop {
                    if let Err(e) = follow_service(&qt_thread) {
                        let reason = e.to_string();
                        let queued =
                            qt_thread.queue(move |headset| headset.on_service_error(reason));
                        if queued.is_err() {
                            return;
                        }
                    }
                    std::thread::sleep(RETRY);
                }
            });
        if let Err(e) = listener {
            tracing::error!(%e, "lancement du thread des événements");
        }

        let qt_thread = self.qt_thread();
        let sender = std::thread::Builder::new()
            .name("service-commandes".into())
            .spawn(move || {
                let mut client: Option<Client> = None;
                for (command, session) in rx {
                    if client.is_none() {
                        client = Client::connect().ok();
                    }
                    let result = match &client {
                        Some(c) => c.submit(&command, session),
                        None => Err(momentum_client::ClientError::NoService),
                    };
                    let _ = match result {
                        Ok(id) => qt_thread.queue(move |headset| headset.on_submitted(id)),
                        // Service joint, demande refusée : ce n'est pas une
                        // panne du service (F03 de la revue).
                        Err(momentum_client::ClientError::Service(reason)) => {
                            qt_thread.queue(move |headset| headset.on_rejected(reason))
                        }
                        Err(e) => {
                            client = None;
                            let reason = e.to_string();
                            qt_thread.queue(move |headset| headset.on_service_error(reason))
                        }
                    };
                }
            });
        if let Err(e) = sender {
            tracing::error!(%e, "lancement du thread des commandes");
        }
    }
}

/// Lit l'état du service puis relaie ses événements ; ne revient qu'en cas
/// d'erreur ou de fermeture de l'interface.
fn follow_service(
    qt_thread: &cxx_qt::CxxQtThread<qobject::Headset>,
) -> Result<(), momentum_client::ClientError> {
    let client = Client::connect()?;
    // Abonnement avant la lecture de l'état : aucun événement perdu entre
    // les deux.
    let events = client.events()?;
    let snapshot = client.snapshot()?;
    tracing::info!("connecté au service momentum-daemon");
    if qt_thread
        .queue(move |headset| headset.on_snapshot(snapshot))
        .is_err()
    {
        return Ok(());
    }
    for event in events {
        if qt_thread
            .queue(move |headset| headset.on_event(event))
            .is_err()
        {
            break;
        }
    }
    Ok(())
}

impl HeadsetRust {
    fn note_incident(&mut self, message: String) {
        self.incidents.push((unix_now(), message));
        let excess = self.incidents.len().saturating_sub(MAX_INCIDENTS);
        self.incidents.drain(..excess);
    }

    /// Préréglages de l'app (selon l'égaliseur lu) puis personnels.
    /// Heure de chaque lecture, pour la page « Connexion et audio ».
    fn note_reading_times(&mut self, event: &Event) {
        match event {
            Event::Searching | Event::Lost(_) => self.control_since = None,
            Event::Connected { .. } => self.control_since = Some(clock()),
            Event::Connections(_) => self.connections_at = Some(clock()),
            Event::State(s) => {
                let before = self.snapshot.state;
                if before.is_none_or(|b| b.codec != s.codec) {
                    self.codec_since = Some(clock());
                }
                if before.is_none_or(|b| b.sample_rate_hz != s.sample_rate_hz) {
                    self.rate_since = Some(clock());
                }
            }
            _ => {}
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
}

impl qobject::Headset {
    /// Envoie une demande et affiche l'attente de son résultat.
    fn send(mut self: Pin<&mut Self>, command: Command) {
        let Some(tx) = self.rust().commands.clone() else {
            return;
        };
        let session = self.rust().snapshot.session;
        if tx.send((command, session)).is_err() {
            return;
        }
        self.as_mut().rust_mut().requests.sent();
        self.as_mut().rust_mut().snapshot.problem = None;
        self.as_mut().set_busy(true);
        self.refresh();
    }

    /// Numéro de la demande rendu par le service.
    fn on_submitted(mut self: Pin<&mut Self>, id: u64) {
        let failure = self.as_mut().rust_mut().requests.submitted(id);
        self.as_mut().finish_request(failure);
    }

    /// Demande refusée tout de suite par le service.
    fn on_rejected(mut self: Pin<&mut Self>, reason: String) {
        let failure = self.as_mut().rust_mut().requests.rejected(&reason);
        self.as_mut().finish_request(Some(failure));
    }

    /// Montre l'échec d'une demande et met à jour l'attente.
    fn finish_request(mut self: Pin<&mut Self>, failure: Option<String>) {
        if let Some(message) = failure {
            tracing::warn!(%message, "demande sans succès");
            self.as_mut().rust_mut().note_incident(message.clone());
            self.as_mut().rust_mut().snapshot.problem = Some(message);
        }
        let busy = self.rust().requests.busy();
        self.as_mut().set_busy(busy);
        self.refresh();
    }

    /// État complet lu à la connexion au service.
    fn on_snapshot(mut self: Pin<&mut Self>, snapshot: Snapshot) {
        self.as_mut().rust_mut().snapshot = snapshot;
        self.as_mut().rust_mut().service_error = None;
        let busy = self.rust().requests.busy();
        self.as_mut().set_busy(busy);
        self.refresh();
    }

    fn on_event(mut self: Pin<&mut Self>, event: Event) {
        if let Event::Done { id, outcome, .. } = &event {
            let failure = self.as_mut().rust_mut().requests.done(*id, outcome);
            self.as_mut().finish_request(failure);
        }
        self.as_mut().rust_mut().note_reading_times(&event);
        match &event {
            Event::Lost(reason) => {
                tracing::warn!(%reason, "connexion perdue");
                self.as_mut()
                    .rust_mut()
                    .note_incident(format!("connexion perdue : {reason}"));
            }
            Event::Refused(reason) => {
                tracing::warn!(%reason, "réglage refusé");
                self.as_mut()
                    .rust_mut()
                    .note_incident(format!("réglage refusé : {reason}"));
            }
            Event::SceneApplied(report) => {
                self.as_mut()
                    .set_scene_result(QString::from(report.details().as_str()));
                self.as_mut().set_scene_result_ok(report.is_complete());
            }
            Event::Exported(result) => {
                let text = match result {
                    Ok(path) => format!(
                        "Profil exporté dans {} (données auditives personnelles : ne pas le publier).",
                        path.display()
                    ),
                    Err(reason) => format!("Export du profil impossible : {reason}."),
                };
                self.as_mut().set_eq_notice(QString::from(text.as_str()));
            }
            _ => {}
        }
        self.as_mut().rust_mut().snapshot.apply(&event);
        self.as_mut().rust_mut().service_error = None;
        self.refresh();
    }

    /// Rouvre l'éditeur sur `scenes.txt` (modifications abandonnées).
    fn reload_scene_editor(mut self: Pin<&mut Self>) {
        let text = paths::scenes_file().and_then(|p| std::fs::read_to_string(p).ok());
        let selected = self.rust().scene_editor.selected();
        let mut editor = SceneEditor::load(text, paths::scene_presets());
        editor.select(selected.min(editor.names().len().saturating_sub(1)));
        let form = editor.form();
        self.as_mut().rust_mut().scene_editor = editor;
        self.as_mut().rust_mut().scene_form = form;
    }

    /// Recopie l'éditeur et le formulaire dans les propriétés QML.
    fn push_scene_editor(mut self: Pin<&mut Self>, notice: Option<String>) {
        let e = self.rust().scene_editor.clone();
        let f = self.rust().scene_form.clone();
        let names = e.names();
        self.as_mut()
            .set_scene_edit_names(string_list(names.iter().map(String::as_str)));
        self.as_mut()
            .set_scene_edit_selected(i32::try_from(e.selected()).unwrap_or(0));
        self.as_mut().set_scene_edit_dirty(e.dirty());
        self.as_mut().set_scene_edit_can_save(e.can_save());
        self.as_mut()
            .set_scene_edit_preview(QString::from(e.preview().as_str()));
        self.as_mut()
            .set_scene_eq_choices(string_list(e.eq_choices().iter().map(String::as_str)));
        let text = e
            .problem()
            .map(|p| format!("⚠ {p}"))
            .or(notice)
            .unwrap_or_else(|| {
                "Le brouillon n'est écrit qu'avec « Enregistrer » ; rien n'est envoyé au casque. \
             Le panneau et les raccourcis suivent les renommages."
                    .to_string()
            });
        self.as_mut()
            .set_scene_edit_notice(QString::from(text.as_str()));
        self.as_mut()
            .set_scene_form_name(QString::from(f.name.as_str()));
        self.as_mut().set_scene_form_sound(f.sound);
        self.as_mut().set_scene_form_spatial(f.spatial);
        self.as_mut().set_scene_form_noise(f.noise);
        self.as_mut()
            .set_scene_form_level(QString::from(f.level.as_str()));
        self.as_mut().set_scene_form_anti_wind(f.anti_wind);
        self.as_mut().set_scene_form_touch(f.touch);
        self.as_mut().set_scene_form_equalizer(f.equalizer);
        self.as_mut().set_scene_form_bass(f.bass_boost);
    }

    /// Choix du menu de l'icône (mêmes actions que WinUI).
    pub fn on_menu_action(mut self: Pin<&mut Self>, action: MenuAction) {
        match action {
            MenuAction::OpenWindow => self.as_mut().show_requested(),
            MenuAction::OpenEqualizer => self.as_mut().equalizer_requested(),
            MenuAction::Quit => self.as_mut().quit_requested(),
            MenuAction::ReloadScenes => {
                self.as_mut().reload_scenes();
                self.refresh();
            }
            MenuAction::Autostart => {
                let on = !self.autostart();
                self.as_mut().set_autostart_enabled(on);
                self.refresh();
            }
            action => {
                let snapshot = &self.rust().snapshot;
                let command = snapshot
                    .state
                    .filter(|_| snapshot.read_only.is_none())
                    .and_then(|s| momentum_client::menu::command_for(&action, &s));
                if let Some(command) = command {
                    self.send(command);
                }
            }
        }
    }

    /// Raccourci global : mêmes règles que WinUI sous Windows.
    fn on_shortcut(mut self: Pin<&mut Self>, action: HotkeyAction) {
        let snapshot = &self.rust().snapshot;
        let state = snapshot.state.filter(|_| snapshot.read_only.is_none());
        let command = match (action, state) {
            (HotkeyAction::OpenWindow, _) => {
                self.as_mut().show_requested();
                None
            }
            (HotkeyAction::CycleNoise, Some(s)) => {
                Some(Command::SetMode(hotkeys::next_noise_mode(s.noise.mode())))
            }
            (HotkeyAction::ToggleProfile, Some(s)) => {
                hotkeys::profile_toggle(&s).map(Command::SetSoundProfile)
            }
            (HotkeyAction::Scene(name), Some(_)) => Some(Command::ApplyScene(name)),
            _ => None,
        };
        if let Some(command) = command {
            self.send(command);
        }
    }

    /// Service injoignable : l'état affiché n'est plus garanti.
    fn on_service_error(mut self: Pin<&mut Self>, reason: String) {
        tracing::warn!(%reason, "service momentum-daemon");
        self.as_mut()
            .rust_mut()
            .note_incident(format!("service injoignable : {reason}"));
        self.as_mut().rust_mut().snapshot = Snapshot::default();
        self.as_mut().rust_mut().service_error = Some(reason);
        self.as_mut().rust_mut().requests.reset();
        self.as_mut().set_busy(false);
        self.refresh();
    }

    /// Recopie l'état courant dans les propriétés QML.
    fn refresh(mut self: Pin<&mut Self>) {
        let r = self.rust();
        let s = &r.snapshot;
        let name = s
            .info
            .as_ref()
            .and_then(|i| i.name.clone())
            .or_else(|| s.device_name.clone());
        let read_only = s
            .read_only
            .as_ref()
            .map(|reason| format!("Lecture seule : {reason}"));
        let problem = r
            .service_error
            .as_ref()
            .map(|e| format!("Service OpenHeadset Control injoignable : {e}"))
            .or_else(|| s.problem.clone());
        // La liste des appareils n'est pas affichée par ce client.
        let v = viewmodel::main_view(
            name.as_deref(),
            s.state.as_ref(),
            &[],
            problem.as_deref(),
            read_only.as_deref(),
        );
        let st = viewmodel::settings_view(s.state.as_ref(), s.info.as_ref(), v.writable);
        let presets = r.all_presets();
        let ab_ready = r.ab.ready();
        let ab_label = r.ab.label();
        let sv = viewmodel::sound_view(
            s.state.as_ref(),
            s.equalizer.as_ref().map(|e| e.gains.as_slice()),
            &presets,
            v.writable,
        );
        let battery = s.state.map_or(-1, |st| i32::from(st.battery));
        let multipoint = s.info.as_ref().and_then(|i| i.multipoint);
        let multipoint_enabled = s.state.is_some() && s.read_only.is_none() && multipoint.is_some();
        let model = s
            .info
            .as_ref()
            .and_then(|i| i.model.clone())
            .unwrap_or_default();
        let firmware = s
            .info
            .as_ref()
            .and_then(|i| i.firmware.clone())
            .unwrap_or_default();
        let eq = s.equalizer.clone();
        let c = v.controls.clone();
        let rows: Vec<viewmodel::ConnectionRow> = if s.state.is_some() {
            s.connections
                .iter()
                .map(|d| viewmodel::connection_row(&d.name, d.connected, d.this_device, v.writable))
                .collect()
        } else {
            Vec::new()
        };
        let scene_names: Vec<String> = r.scenes.iter().map(|sc| sc.name.clone()).collect();
        let tray_view = TrayView {
            kind: IconKind::from_state(s.state.as_ref()),
            tooltip: tooltip(name.as_deref(), s.state.as_ref(), problem.as_deref()),
            entries: tray_menu::tray_menu(
                s.state.as_ref(),
                &MenuContext {
                    name: name.as_deref(),
                    problem: problem.as_deref(),
                    writable: v.writable,
                    scenes: &scene_names,
                    autostart: r.autostart,
                    autostart_label: "Lancer à l'ouverture de session",
                },
            ),
        };
        if let Some(tray) = &r.tray {
            tray.update(tray_view);
        }
        let audio_revision = r.audio_revision.wrapping_add(1);
        if rows != r.connection_rows {
            let count = i32::try_from(rows.len()).unwrap_or(0);
            let revision = r.connection_revision.wrapping_add(1);
            self.as_mut().rust_mut().connection_rows = rows;
            self.as_mut().set_connection_count(count);
            self.as_mut().set_connection_revision(revision);
        }

        let mut h = self.as_mut();
        h.as_mut().set_audio_revision(audio_revision);
        h.as_mut().set_title(QString::from(v.title.as_str()));
        h.as_mut()
            .set_connection(QString::from(v.connection.as_str()));
        h.as_mut().set_connected(v.connected);
        h.as_mut().set_writable(v.writable);
        h.as_mut().set_waiting(QString::from(v.waiting.as_str()));
        let banner = if v.banner.is_empty() && demo::requested() {
            demo::BANNER.to_string()
        } else {
            v.banner.clone()
        };
        h.as_mut().set_banner(QString::from(banner.as_str()));
        h.as_mut().set_banner_warning(v.banner_is_warning);
        h.as_mut().set_battery(battery);
        h.as_mut()
            .set_battery_text(QString::from(v.battery_text.as_str()));
        h.as_mut().set_stream(QString::from(v.stream.as_str()));
        h.as_mut().set_model(QString::from(model.as_str()));
        h.as_mut().set_firmware(QString::from(firmware.as_str()));
        h.as_mut().set_noise_mode(c.noise_mode);
        h.as_mut().set_level(c.level);
        h.as_mut().set_anti_wind(c.anti_wind);
        h.as_mut().set_spatial(c.spatial_on);
        h.as_mut().set_spatial_enabled(c.spatial_enabled);
        h.as_mut().set_head_tracking(c.head_tracking_on);
        h.as_mut()
            .set_head_tracking_enabled(c.head_tracking_enabled);
        h.as_mut().set_touch(c.touch_on);
        h.as_mut().set_touch_enabled(c.touch_enabled);
        h.as_mut().set_multipoint(multipoint == Some(true));
        h.as_mut().set_multipoint_enabled(multipoint_enabled);
        h.as_mut().set_smart_pause(st.smart_pause.on);
        h.as_mut().set_smart_pause_enabled(st.smart_pause.enabled);
        h.as_mut().set_auto_answer(st.auto_answer.on);
        h.as_mut().set_auto_answer_enabled(st.auto_answer.enabled);
        h.as_mut().set_comfort(st.comfort_call.on);
        h.as_mut().set_comfort_enabled(st.comfort_call.enabled);
        h.as_mut().set_battery_protection(st.battery_protection.on);
        h.as_mut()
            .set_battery_protection_enabled(st.battery_protection.enabled);
        h.as_mut().set_auto_off(st.auto_off);
        h.as_mut().set_auto_off_enabled(st.auto_off_enabled);
        h.as_mut().set_prompt_mode(st.prompt_mode);
        h.as_mut().set_prompts_enabled(st.prompts_enabled);
        h.as_mut().set_prompt_volume(st.prompt_volume);
        h.as_mut()
            .set_prompt_volume_enabled(st.prompt_volume_enabled);
        h.as_mut().set_headset_name(QString::from(st.name.as_str()));
        h.as_mut().set_rename_enabled(st.rename_enabled);
        h.as_mut().set_audio_mode(sv.audio_mode);
        h.as_mut().set_modes_enabled(sv.modes_enabled);
        h.as_mut().set_profile_available(sv.profile_available);
        h.as_mut()
            .set_preset_names(string_list(sv.preset_names.iter().map(String::as_str)));
        h.as_mut().set_preset_index(sv.current_preset);
        let user_preset = usize::try_from(sv.current_preset)
            .ok()
            .and_then(|i| presets.get(i))
            .filter(|p| !p.builtin)
            .map_or_else(QString::default, |p| QString::from(p.name.as_str()));
        h.as_mut().set_user_preset(user_preset);
        h.as_mut().set_ab_ready(ab_ready);
        h.as_mut().set_ab_label(QString::from(ab_label));
        if let Some(eq) = eq {
            h.as_mut().set_bass_boost(eq.bass_boost);
            h.as_mut()
                .set_band_count(i32::try_from(eq.gains.len()).unwrap_or(0));
            h.as_mut().set_min_gain(f64::from(eq.min_gain) / 10.0);
            h.as_mut().set_max_gain(f64::from(eq.max_gain) / 10.0);
            let revision = h.rust().eq_revision.wrapping_add(1);
            h.as_mut().set_eq_revision(revision);
        }
    }

    fn band_gain(&self, index: i32) -> f64 {
        usize::try_from(index)
            .ok()
            .and_then(|i| {
                self.rust()
                    .snapshot
                    .equalizer
                    .as_ref()?
                    .gains
                    .get(i)
                    .copied()
            })
            .map_or(0.0, |g| f64::from(g) / 10.0)
    }

    fn band_label(&self, index: i32) -> QString {
        let count = self
            .rust()
            .snapshot
            .equalizer
            .as_ref()
            .map_or(0, |e| e.gains.len());
        match usize::try_from(index) {
            Ok(i) if i < count => QString::from(band_label(i, count).as_str()),
            _ => QString::default(),
        }
    }

    fn set_noise_mode_index(self: Pin<&mut Self>, index: i32) {
        let mode = match index {
            0 => NoiseModeRequest::Adaptive,
            1 => NoiseModeRequest::Custom,
            _ => NoiseModeRequest::Off,
        };
        self.send(Command::SetMode(mode));
    }

    fn send_level(self: Pin<&mut Self>, level: i32) {
        self.send(Command::SetLevel(level.clamp(0, 100) as u8));
    }

    fn send_anti_wind(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetAntiWind(on));
    }

    fn send_spatial(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetSpatial(on));
    }

    fn send_head_tracking(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetHeadTracking(on));
    }

    fn send_touch(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetTouchControls(on));
    }

    fn send_multipoint(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetMultipoint(on));
    }

    fn send_smart_pause(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetSmartPause(on));
    }

    fn send_auto_answer(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetAutoAnswer(on));
    }

    fn send_comfort(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetComfortCall(on));
    }

    fn send_battery_protection(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetBatteryProtection(on));
    }

    fn send_auto_off_index(self: Pin<&mut Self>, index: i32) {
        if let Some(&seconds) = usize::try_from(index)
            .ok()
            .and_then(|i| AUTO_OFF_CHOICES.get(i))
        {
            self.send(Command::SetAutoOff(seconds));
        }
    }

    fn send_prompt_mode_index(self: Pin<&mut Self>, index: i32) {
        self.send(Command::SetPromptMode(if index == 0 {
            PromptMode::TonesOnly
        } else {
            PromptMode::TonesAndVoice
        }));
    }

    fn send_prompt_volume(self: Pin<&mut Self>, percent: i32) {
        // Plusieurs % donnent la même valeur du casque : on compare celles-ci.
        let value = prompt_volume_from_percent(percent.clamp(0, 100) as u8);
        let current = self
            .rust()
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.prompt_volume);
        if current.is_some_and(|c| c != value) {
            self.send(Command::SetPromptVolume(value));
        }
    }

    fn rename(self: Pin<&mut Self>, name: QString) -> QString {
        let name = name.to_string().trim().to_string();
        let current = self
            .rust()
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.name.clone());
        if let Err(e) = check_name(&name) {
            return QString::from(e.to_string().as_str());
        }
        if current.as_deref() == Some(name.as_str()) {
            return QString::from("nom refusé : c'est déjà le nom du casque");
        }
        self.send(Command::SetName(name));
        QString::default()
    }

    fn send_audio_mode_index(self: Pin<&mut Self>, index: i32) {
        self.send(match index {
            1 => Command::SetSpeechClarity(true),
            2 => Command::SetSoundProfile(true),
            _ => Command::SetSpeechClarity(false),
        });
    }

    fn send_band(self: Pin<&mut Self>, index: i32, db: f64) {
        let tenths = (db * 10.0)
            .round()
            .clamp(f64::from(i8::MIN), f64::from(i8::MAX)) as i8;
        if let Ok(band) = u8::try_from(index) {
            self.send(Command::SetEqBand(band, tenths));
        }
    }

    fn send_bass_boost(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetBassBoost(on));
    }

    fn send_flat(self: Pin<&mut Self>) {
        self.send(Command::SetEqFlat);
    }

    fn paired_row(&self, index: i32) -> Option<&viewmodel::ConnectionRow> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.rust().connection_rows.get(i))
    }

    fn connection_name(&self, index: i32) -> QString {
        self.paired_row(index)
            .map(|r| QString::from(r.name.as_str()))
            .unwrap_or_default()
    }

    fn connection_status(&self, index: i32) -> QString {
        self.paired_row(index)
            .map(|r| QString::from(r.status.as_str()))
            .unwrap_or_default()
    }

    fn connection_action(&self, index: i32) -> QString {
        self.paired_row(index)
            .map(|r| QString::from(r.action.as_str()))
            .unwrap_or_default()
    }

    fn connection_can_act(&self, index: i32) -> bool {
        self.paired_row(index).is_some_and(|r| r.can_act)
    }

    fn toggle_connection(self: Pin<&mut Self>, index: i32) {
        let Some(row) = self.paired_row(index).filter(|r| r.can_act).cloned() else {
            return;
        };
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let device = self
            .rust()
            .snapshot
            .connections
            .get(index)
            .map(|c| c.id.clone())
            .filter(|id| !id.is_empty());
        self.send(Command::ToggleConnection {
            index,
            name: row.name,
            connect: row.action == "Connecter",
            device,
        });
    }

    fn scene_edit_select(mut self: Pin<&mut Self>, index: i32) {
        if let Ok(i) = usize::try_from(index)
            && i != self.rust().scene_editor.selected()
        {
            self.as_mut().rust_mut().scene_editor.select(i);
            let form = self.rust().scene_editor.form();
            self.as_mut().rust_mut().scene_form = form;
            self.as_mut().push_scene_editor(None);
        }
    }

    fn scene_edit_action(mut self: Pin<&mut Self>, action: QString) {
        let notice = {
            let editor = &mut self.as_mut().rust_mut().scene_editor;
            match action.to_string().as_str() {
                "nouvelle" => editor.new_scene(),
                "dupliquer" => editor.duplicate(),
                "supprimer" => editor.delete(),
                "monter" => editor.move_by(-1),
                "descendre" => editor.move_by(1),
                "enregistrer" => {
                    let result = scene_editor::save(editor);
                    let notice = match result {
                        Ok((message, _)) => message,
                        Err(reason) => format!("Non enregistré : {reason}"),
                    };
                    // Les raccourcis KDE des scènes suivent au prochain
                    // démarrage du client.
                    self.as_mut().reload_scene_editor();
                    self.as_mut().reload_scenes();
                    self.as_mut().push_scene_editor(Some(notice));
                    return;
                }
                "annuler" => {
                    self.as_mut().reload_scene_editor();
                    self.as_mut()
                        .push_scene_editor(Some("Modifications abandonnées.".to_string()));
                    return;
                }
                _ => return,
            }
            None
        };
        let form = self.rust().scene_editor.form();
        self.as_mut().rust_mut().scene_form = form;
        self.as_mut().push_scene_editor(notice);
    }

    fn set_scene_form(mut self: Pin<&mut Self>, name: QString, level: QString, choices: QString) {
        let c: Vec<i32> = choices
            .to_string()
            .split(',')
            .map(|v| v.trim().parse().unwrap_or(0))
            .collect();
        let get = |i: usize| c.get(i).copied().unwrap_or(0);
        let (sound, spatial, noise, anti_wind, touch, equalizer, bass) =
            (get(0), get(1), get(2), get(3), get(4), get(5), get(6));
        let form = viewmodel::SceneForm {
            name: name.to_string(),
            sound,
            spatial,
            noise,
            level: level.to_string(),
            anti_wind,
            touch,
            equalizer,
            bass_boost: bass,
        };
        if form == self.rust().scene_form {
            return;
        }
        self.as_mut().rust_mut().scene_editor.set_form(&form);
        self.as_mut().rust_mut().scene_form = form;
        self.as_mut().push_scene_editor(None);
    }

    fn reload_scenes(mut self: Pin<&mut Self>) {
        // Une modification en cours dans l'éditeur n'est pas perdue.
        if !self.rust().scene_editor.dirty() {
            self.as_mut().reload_scene_editor();
            self.as_mut().push_scene_editor(None);
        }
        let (scenes, errors) = paths::load_scenes();
        let names = string_list(scenes.iter().map(|s| s.name.as_str()));
        let previews: Vec<String> = scenes.iter().map(viewmodel::scene_preview).collect();
        self.as_mut().rust_mut().scenes = scenes;
        self.as_mut().set_scene_names(names);
        self.as_mut()
            .set_scene_previews(string_list(previews.iter().map(String::as_str)));
        self.as_mut()
            .set_scene_problems(QString::from(errors.join("\n").as_str()));
    }

    fn apply_scene(mut self: Pin<&mut Self>, index: i32) {
        let Some(name) = usize::try_from(index)
            .ok()
            .and_then(|i| self.rust().scenes.get(i).map(|s| s.name.clone()))
        else {
            return;
        };
        self.as_mut().set_scene_result(QString::from(
            format!("Scène « {name} » en cours…").as_str(),
        ));
        self.as_mut().set_scene_result_ok(true);
        self.send(Command::ApplyScene(name));
    }

    fn reload_battery(mut self: Pin<&mut Self>, span: i32) {
        let now = unix_now();
        let log = paths::battery_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| BatteryLog::parse(&text, now))
            .unwrap_or_default();
        let current = self.rust().snapshot.state.map(|s| s.battery);
        let span_index = usize::try_from(span).unwrap_or(0);
        let view = viewmodel::battery_view(&log, now, span_index, current);
        let details: Vec<String> = view
            .details
            .iter()
            .map(|r| format!("{} : {}", r.label, r.value))
            .collect();
        let revision = self.rust().battery_revision.wrapping_add(1);
        self.as_mut().rust_mut().battery_path = view.path;
        self.as_mut().set_battery_span(span);
        self.as_mut()
            .set_battery_current(QString::from(view.current.as_str()));
        self.as_mut()
            .set_battery_estimate(QString::from(view.estimate.as_str()));
        self.as_mut()
            .set_battery_axis(QString::from(view.axis_start.as_str()));
        self.as_mut()
            .set_battery_details(string_list(details.iter().map(String::as_str)));
        self.as_mut().set_battery_revision(revision);
    }

    fn battery_chart(&self, width: f64, height: f64) -> QString {
        QString::from(scale_path(&self.rust().battery_path, width, height).as_str())
    }

    fn audio_rows(&self, section: QString) -> QStringList {
        let r = self.rust();
        let connections: Vec<_> = r
            .snapshot
            .connections
            .iter()
            .enumerate()
            .map(|(i, c)| c.to_connection(i))
            .collect();
        let name = r
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.name.clone())
            .or_else(|| r.snapshot.device_name.clone());
        let unread = Err(LINUX_OUTPUTS_UNREAD.to_string());
        let v = viewmodel::audio_view(&viewmodel::AudioInputs {
            device_name: name.as_deref(),
            state: r.snapshot.state.as_ref(),
            connections: &connections,
            control_since: r.control_since.as_deref(),
            connections_at: r.connections_at.as_deref(),
            codec_since: r.codec_since.as_deref(),
            rate_since: r.rate_since.as_deref(),
            windows: Some(&unread),
            windows_at: None,
        });
        let rows = |rows: Vec<viewmodel::InfoRow>| -> Vec<String> {
            rows.into_iter()
                .map(|row| format!("{} : {}\t{}", row.label, row.value, row.source))
                .collect()
        };
        let lines = match section.to_string().as_str() {
            "constats" => v
                .findings
                .into_iter()
                .map(|(problem, text)| format!("{} {text}", if problem { "⚠" } else { "•" }))
                .collect(),
            "pilotage" => rows(v.control),
            "systeme" => rows(v.windows),
            "flux" => rows(v.stream),
            _ => Vec::new(),
        };
        string_list(lines.iter().map(String::as_str))
    }

    fn export_battery(&self) -> QString {
        QString::from(export_battery_history().as_str())
    }

    fn clear_battery(mut self: Pin<&mut Self>) -> QString {
        let Some(tx) = self.rust().commands.clone() else {
            return QString::from("Effacement impossible : service injoignable.");
        };
        // Sans casque aussi : connexion 0 (indifférente). Un refus du service
        // s'affiche comme pour toute demande.
        if tx.send((Command::ClearBatteryHistory, 0)).is_err() {
            return QString::from("Effacement impossible : service injoignable.");
        }
        self.as_mut().rust_mut().requests.sent();
        self.as_mut().set_busy(true);
        // Le service réécrit le fichier : relecture un peu plus tard.
        let qt_thread = self.qt_thread();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            let _ = qt_thread.queue(|mut headset| {
                let span = *headset.battery_span();
                headset.as_mut().reload_battery(span);
            });
        });
        let span = *self.battery_span();
        self.as_mut().reload_battery(span);
        QString::from("Effacement de l'historique demandé au service.")
    }

    fn choose_theme(mut self: Pin<&mut Self>, choice: i32) {
        let dark = choice == 1;
        if let Err(e) = momentum_core::appearance::store(dark) {
            tracing::warn!(%e, "apparence.txt");
        }
        qobject::openheadset_set_color_scheme(if dark { 2 } else { 1 });
        self.as_mut().set_theme_choice(i32::from(dark));
    }

    fn open_shortcut_settings(&self) {
        if let Err(e) = std::process::Command::new("systemsettings")
            .arg("kcm_keys")
            .spawn()
        {
            tracing::warn!(%e, "ouverture des raccourcis KDE");
        }
    }

    fn set_autostart_enabled(mut self: Pin<&mut Self>, on: bool) {
        if let Err(e) = autostart::set(on) {
            tracing::warn!(%e, "lancement à l'ouverture de session");
        }
        self.as_mut().set_autostart(autostart::is_enabled());
    }

    fn diagnostic(&self) -> QString {
        let r = self.rust();
        let s = &r.snapshot;
        let now = unix_now();
        let incidents: Vec<(u64, String)> = r
            .incidents
            .iter()
            .map(|(time, message)| (now.saturating_sub(*time), message.clone()))
            .collect();
        // Dossier personnel, nom d'utilisateur et noms d'appareils masqués.
        let home = std::env::var("HOME").unwrap_or_default();
        let user = std::env::var("USER").unwrap_or_default();
        let system = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|text| diagnostic::os_release_name(&text));
        let battery_entries = paths::battery_file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or(0, |text| BatteryLog::parse(&text, now).entries().len());
        let mut secrets: Vec<&str> = vec![home.as_str(), user.as_str()];
        secrets.extend(s.device_name.as_deref());
        secrets.extend(s.info.as_ref().and_then(|i| i.name.as_deref()));
        secrets.extend(s.connections.iter().map(|d| d.name.as_str()));
        let text = diagnostic::build(&DiagnosticInput {
            app_version: env!("CARGO_PKG_VERSION"),
            os_version: system.as_deref(),
            connected: s.state.is_some(),
            read_only: s.read_only.as_deref(),
            info: s.info.as_ref(),
            state: s.state.as_ref(),
            eq_bands: s.equalizer.as_ref().map(|e| e.gains.len()),
            paired_devices: s.state.is_some().then_some(s.connections.len()),
            battery_entries,
            incidents: &incidents,
            secrets: &secrets,
        });
        QString::from(text.as_str())
    }

    fn notice(mut self: Pin<&mut Self>, text: String) {
        self.as_mut().set_eq_notice(QString::from(text.as_str()));
    }

    fn save_preset(mut self: Pin<&mut Self>, name: QString) -> bool {
        let Some(gains) = self
            .rust()
            .snapshot
            .equalizer
            .as_ref()
            .map(|e| e.gains.clone())
        else {
            return false;
        };
        let name = name.to_string().trim().to_string();
        // Relu puis modifié : rien d'enregistré par une autre interface n'est perdu.
        match paths::upsert_user_preset(&name, &gains) {
            Ok(list) => {
                self.as_mut().rust_mut().user_presets = list;
                self.as_mut()
                    .notice(format!("Préréglage « {name} » enregistré sur ce PC."));
                self.refresh();
                true
            }
            Err(reason) => {
                self.notice(format!("Préréglage non enregistré : {reason}."));
                false
            }
        }
    }

    fn delete_user_preset(mut self: Pin<&mut Self>) {
        let name = self.user_preset().to_string();
        if name.is_empty() {
            return;
        }
        let text = match paths::remove_user_preset(&name) {
            Ok(list) => {
                self.as_mut().rust_mut().user_presets = list;
                format!("Préréglage « {name} » supprimé.")
            }
            Err(e) => format!("Suppression non enregistrée : {e}."),
        };
        self.as_mut().notice(text);
        self.refresh();
    }

    fn apply_gains_text(self: Pin<&mut Self>, text: QString) -> bool {
        let Some(eq) = self.rust().snapshot.equalizer.clone() else {
            return false;
        };
        match presets::parse_gains_db(&text.to_string(), eq.gains.len(), eq.min_gain, eq.max_gain) {
            Ok(gains) if gains == eq.gains => {
                self.notice("Ce sont déjà les gains du casque.".to_string());
                false
            }
            Ok(gains) => {
                self.send(Command::SetEqGains(gains));
                true
            }
            Err(reason) => {
                self.notice(format!("Gains non appliqués : {reason}."));
                false
            }
        }
    }

    fn export_curve(self: Pin<&mut Self>) {
        let r = self.rust();
        let Some(eq) = r.snapshot.equalizer.as_ref() else {
            return;
        };
        let model = r.snapshot.info.as_ref().and_then(|i| i.model.as_deref());
        let text = presets::format_curve(model, &eq.gains, eq.bass_boost);
        let notice = match paths::export_file("momentum-egaliseur", "txt", unix_now()) {
            Some(path) => match std::fs::write(&path, text) {
                Ok(()) => format!("Courbe exportée dans {}.", path.display()),
                Err(e) => format!("Export de la courbe impossible : {e}."),
            },
            None => "Export impossible : dossier personnel introuvable.".to_string(),
        };
        self.notice(notice);
    }

    fn import_curve(mut self: Pin<&mut Self>, url: QString) {
        let Some(eq) = self.rust().snapshot.equalizer.clone() else {
            return;
        };
        let url = url.to_string();
        let path = std::path::PathBuf::from(url.strip_prefix("file://").unwrap_or(&url));
        let model = self
            .rust()
            .snapshot
            .info
            .as_ref()
            .and_then(|i| i.model.clone());
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
        let notice = match parsed {
            Ok(curve) => {
                if curve.gains != eq.gains {
                    self.as_mut().send(Command::SetEqGains(curve.gains));
                }
                if let Some(on) = curve.bass_boost.filter(|&on| on != eq.bass_boost) {
                    self.as_mut().send(Command::SetBassBoost(on));
                }
                format!("Courbe importée depuis {}.", path.display())
            }
            Err(reason) => format!("Courbe non importée : {reason}."),
        };
        self.notice(notice);
    }

    fn export_profile(self: Pin<&mut Self>) {
        match paths::export_file("momentum-profil", "json", unix_now()) {
            Some(path) => self.send(Command::ExportProfile(path)),
            None => self.notice("Export impossible : dossier personnel introuvable.".to_string()),
        }
    }

    fn remember_a(mut self: Pin<&mut Self>) {
        let Some(gains) = self
            .rust()
            .snapshot
            .equalizer
            .as_ref()
            .map(|e| e.gains.clone())
        else {
            return;
        };
        self.as_mut().rust_mut().ab.remember(&gains);
        self.as_mut()
            .notice("Courbe A mémorisée : modifiez l'égaliseur (B), puis comparez.".to_string());
        self.refresh();
    }

    fn toggle_ab(mut self: Pin<&mut Self>) {
        let Some(current) = self
            .rust()
            .snapshot
            .equalizer
            .as_ref()
            .map(|e| e.gains.clone())
        else {
            return;
        };
        let target = self.as_mut().rust_mut().ab.toggle(&current);
        if let Some(gains) = target.filter(|g| *g != current) {
            self.as_mut().send(Command::SetEqGains(gains));
        }
        self.refresh();
    }

    fn apply_preset_index(self: Pin<&mut Self>, index: i32) {
        let gains = usize::try_from(index)
            .ok()
            .and_then(|i| self.rust().all_presets().get(i).map(|p| p.gains.clone()));
        if let Some(gains) = gains {
            self.send(Command::SetEqGains(gains));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::scale_path;

    #[test]
    fn chart_path_is_scaled_to_the_view() {
        assert_eq!(
            scale_path("M 0.0 30.0 L 500.0 40.0 M 1000.0 100.0", 200.0, 50.0),
            "M 0.0 15.0 L 100.0 20.0 M 200.0 50.0"
        );
    }
}
