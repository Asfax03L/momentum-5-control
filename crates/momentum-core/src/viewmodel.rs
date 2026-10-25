//! Modèle de vue de la fenêtre principale, en Rust pur (testable partout).
//!
//! Toute la logique d'affichage est ici : textes en français, valeurs « non
//! lu », bandeau d'avertissement. Les interfaces (WinUI, client KDE) se
//! contentent d'afficher ces champs ; elle ne décide de rien.

use crate::audio_check::{
    ControlLink, Level, WindowsAudio, describe_link_state, describe_output, findings, format_mix,
    format_rate,
};
use crate::battery_log::{BatteryLog, Entry, RETENTION_SECONDS, chart_path};
use crate::device::NoiseModeRequest;
use crate::device::{
    AUTO_OFF_CHOICES, AudioMode, Connection, DeviceInfo, HeadsetState, PromptMode,
    SoundProfileStatus, prompt_volume_percent,
};
use crate::i18n::{tr, trf};
use crate::presets::{Preset, matching};
use crate::protocol::messages::{Codec, M5NoiseMode};
use crate::scenes::{Scene, SceneSound, check_scene};
use crate::ui::{audio_mode_label, eq_mode_note};

/// Pages de la navigation latérale (même ordre que dans la fenêtre).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Home = 0,
    Sound = 1,
    Settings = 2,
    Battery = 3,
    Audio = 4,
    Scenes = 5,
    About = 6,
}

/// Périodes proposées pour la courbe de la batterie.
pub const BATTERY_SPANS: [(&str, u64); 3] = [
    ("24 heures", 24 * 3600),
    ("7 jours", 7 * 24 * 3600),
    ("30 jours", RETENTION_SECONDS),
];

/// Page « Batterie ».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatteryView {
    /// Tracé SVG (repère 1000 × 100), vide sans mesure sur la période.
    pub path: String,
    /// « 70 % » ou « non connecté ».
    pub current: String,
    /// Estimation du temps restant, ou pourquoi il n'y en a pas.
    pub estimate: String,
    /// Bord gauche de la courbe : « il y a 24 heures »…
    pub axis_start: String,
    pub details: Vec<Row>,
}

/// « il y a 5 min », « il y a 3 h », « il y a 2 j ».
fn ago(seconds: u64) -> String {
    match seconds {
        s if s < 3600 => trf("il y a {} min", &[&(s / 60)]),
        s if s < 48 * 3600 => trf("il y a {} h", &[&(s / 3600)]),
        s => trf("il y a {} j", &[&(s / 86_400)]),
    }
}

pub fn battery_view(
    log: &BatteryLog,
    now: u64,
    span_index: usize,
    current: Option<u8>,
) -> BatteryView {
    let (label, span) = BATTERY_SPANS[span_index.min(BATTERY_SPANS.len() - 1)];
    let label = tr(label);
    let since = now.saturating_sub(span);
    let measures = log
        .entries()
        .iter()
        .filter(|e| matches!(e, Entry::Level { time, .. } if *time >= since))
        .count();
    let last = log.entries().iter().rev().find_map(|e| match *e {
        Entry::Level { time, .. } => Some(time),
        Entry::Gap { .. } => None,
    });
    let estimate = match log.estimate(now) {
        Some((remaining, observed)) => trf("Estimation : environ {} h restantes, d'après les {} dernières heures de décharge. Le rythme dépend de l'ANC, du volume et du codec.", &[&(format!("{:.0}", remaining)), &(format!("{observed:.1}").replace('.', ","))]),
        None => {
            tr("Pas d'estimation : il faut au moins une heure de décharge continue et 5 points perdus.")
                .to_string()
        }
    };
    BatteryView {
        path: chart_path(log, now, span),
        current: current.map_or(tr("non connecté").to_string(), |b| format!("{b} %")),
        estimate,
        axis_start: trf("il y a {}", &[&label]),
        details: vec![
            row(tr("Mesures sur la période"), measures.to_string()),
            row(
                tr("Dernière mesure"),
                last.map_or(tr("aucune").to_string(), |t| ago(now.saturating_sub(t))),
            ),
            row(
                tr("Conservation"),
                tr("30 jours sur ce PC (batterie.csv), rien n'est envoyé"),
            ),
            row(
                tr("Non mesuré"),
                tr(
                    "état de charge, santé et cycles de la batterie : le casque ne les donne pas au projet",
                ),
            ),
        ],
    }
}

/// Une ligne « libellé : valeur ».
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub label: String,
    pub value: String,
}

fn row(label: &str, value: impl Into<String>) -> Row {
    Row {
        label: label.to_string(),
        value: value.into(),
    }
}

/// Tout ce qu'affiche la fenêtre principale.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MainView {
    /// Nom du casque (ou générique).
    pub title: String,
    /// « Connecté », « Connexion… » ou « Recherche du casque… ».
    pub connection: String,
    pub connected: bool,
    /// Réglages modifiables : connecté et modèle validé.
    pub writable: bool,
    /// Batterie en %, absente si inconnue.
    pub battery: Option<u8>,
    pub battery_text: String,
    /// Avertissement à afficher en haut (vide sinon), par priorité :
    /// incident, lecture seule, puis mode audio autre que l'égaliseur.
    pub banner: String,
    /// Vrai pour un incident ou la lecture seule (affiché en rouge) ; faux
    /// pour une simple information (mode audio).
    pub banner_is_warning: bool,
    /// Message affiché à la place des pages tant qu'aucun état n'est lu.
    pub waiting: String,
    pub home: Vec<Row>,
    /// Commandes de la page Accueil.
    pub controls: HomeControls,
    /// Flux audio : « aptX · 44,1 kHz », « Pas de lecture », ou vide.
    pub stream: String,
    /// Appareils appairés au casque, avec l'action possible sur chacun.
    pub connections: Vec<ConnectionRow>,
}

/// Une ligne de « Appareils appairés au casque ».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionRow {
    pub name: String,
    /// « Connecté », « Non connecté » ou « Connecté (ce PC) ».
    pub status: String,
    /// « Connecter » ou « Déconnecter ».
    pub action: String,
    /// Faux pour ce PC (le déconnecter couperait le pilotage) ou en lecture
    /// seule.
    pub can_act: bool,
}

/// Ligne d'un appareil appairé (aussi pour un client du service, qui n'a
/// pas l'adresse des appareils). `writable` : casque validé, pas en lecture
/// seule.
pub fn connection_row(
    name: &str,
    connected: bool,
    this_device: bool,
    writable: bool,
) -> ConnectionRow {
    // Ce PC pilote le casque : il est connecté, même si l'état lu (lien
    // audio) varie d'une lecture à l'autre.
    let connected = connected || this_device;
    ConnectionRow {
        name: name.to_string(),
        status: match (connected, this_device) {
            (_, true) => tr("Connecté (ce PC)"),
            (true, false) => tr("Connecté"),
            (false, _) => tr("Non connecté"),
        }
        .to_string(),
        action: if connected {
            tr("Déconnecter")
        } else {
            tr("Connecter")
        }
        .to_string(),
        can_act: writable && !this_device,
    }
}

impl MainView {
    /// Information ponctuelle (export réussi…) : affichée dans le bandeau
    /// sauf si un incident ou la lecture seule l'occupe déjà.
    pub fn with_notice(mut self, notice: Option<&str>) -> Self {
        if let Some(n) = notice
            && !self.banner_is_warning
        {
            self.banner = n.to_string();
        }
        self
    }
}

/// Modes audio, dans l'ordre des boutons de la page « Mon son ».
pub const AUDIO_MODES: [&str; 3] = [
    "Égaliseur graphique",
    "Speech Clarity",
    "Profil personnalisé",
];

/// Page « Mon son » : choix du mode audio et préréglages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoundView {
    /// Indice dans `AUDIO_MODES`, -1 si inconnu ou non lu.
    pub audio_mode: i32,
    pub modes_enabled: bool,
    /// Un profil de personnalisation existe sur le casque (`2001` = 2).
    pub profile_available: bool,
    /// Noms des préréglages, dans l'ordre (app puis personnels).
    pub preset_names: Vec<String>,
    /// Préréglage correspondant à la courbe du casque, sinon -1.
    pub current_preset: i32,
    /// Le préréglage sélectionné peut être supprimé (personnel).
    pub removable: Vec<bool>,
    /// « Préréglage : Audiophile » ou « Courbe personnalisée, non enregistrée ».
    pub preset_status: String,
}

pub fn sound_view(
    state: Option<&HeadsetState>,
    gains: Option<&[i8]>,
    presets: &[Preset],
    writable: bool,
) -> SoundView {
    let audio_mode = match state.and_then(|s| s.audio_mode) {
        Some(AudioMode::Equalizer) => 0,
        Some(AudioMode::SpeechClarity) => 1,
        Some(AudioMode::Personalization) => 2,
        _ => -1,
    };
    let current = gains.and_then(|g| matching(presets, g));
    SoundView {
        audio_mode,
        modes_enabled: writable && audio_mode >= 0,
        profile_available: writable
            && state.is_some_and(|s| s.sound_profile == Some(SoundProfileStatus::Received)),
        preset_names: presets.iter().map(|p| p.name.clone()).collect(),
        current_preset: current
            .and_then(|c| presets.iter().position(|p| p.name == c.name))
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1),
        removable: presets.iter().map(|p| !p.builtin).collect(),
        preset_status: match (gains, current) {
            (None, _) => String::new(),
            (Some(_), Some(p)) => trf("Préréglage : {}", &[&p.name]),
            (Some(_), None) => tr("Courbe personnalisée, non enregistrée").to_string(),
        },
    }
}

/// Mode de contrôle du bruit, dans l'ordre des boutons de la page Accueil.
pub const NOISE_MODES: [&str; 3] = ["Adaptatif", "Personnalisé", "Arrêt"];

/// État des commandes de la page Accueil (ce que la fenêtre affiche et
/// autorise ; les clics repartent en commandes vers le casque).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HomeControls {
    /// Indice dans `NOISE_MODES`, -1 si inconnu.
    pub noise_mode: i32,
    /// Curseur ANC (0) ↔ transparence (100).
    pub level: i32,
    pub anti_wind: bool,
    pub spatial_on: bool,
    /// Réglage lu et modifiable. Dolby Atmos reste grisé tant que le profil
    /// personnalisé est sélectionné (comme dans l'app).
    pub spatial_enabled: bool,
    pub touch_on: bool,
    pub touch_enabled: bool,
    pub head_tracking_on: bool,
    /// Réglable seulement avec l'audio spatial actif (comme dans l'app).
    pub head_tracking_enabled: bool,
}

fn home_controls(s: &HeadsetState, writable: bool) -> HomeControls {
    let (noise_mode, level) = match s.noise.mode() {
        M5NoiseMode::Adaptive => (0, i32::from(s.noise.level)),
        M5NoiseMode::Custom { level } => (1, i32::from(level)),
        M5NoiseMode::Off => (2, i32::from(s.noise.level)),
    };
    let profile_selected = s.audio_mode == Some(AudioMode::Personalization);
    HomeControls {
        noise_mode,
        level,
        anti_wind: s.noise.anti_wind,
        spatial_on: s.spatial_audio == Some(true),
        spatial_enabled: writable
            && s.spatial_audio.is_some()
            && !(profile_selected && s.spatial_audio == Some(false)),
        touch_on: s.touch_controls == Some(true),
        touch_enabled: writable && s.touch_controls.is_some(),
        head_tracking_on: s.head_tracking == Some(true),
        head_tracking_enabled: writable
            && s.spatial_audio == Some(true)
            && s.head_tracking.is_some(),
    }
}

/// « aptX · 44,1 kHz » ; `0xFF` (relevé à l'arrêt de la lecture) donne
/// « Pas de lecture ».
pub fn stream_label(codec: Option<u8>, sample_rate_hz: Option<u32>) -> String {
    let codec = match codec {
        None => return String::new(),
        Some(0xFF) => return tr("Pas de lecture").to_string(),
        Some(c) => Codec(c)
            .name()
            .map_or(trf("codec {}", &[&c]), str::to_string),
    };
    match sample_rate_hz {
        Some(hz) if hz > 0 => {
            let khz = f64::from(hz) / 1000.0;
            let text = if hz % 1000 == 0 {
                format!("{khz:.0}")
            } else {
                format!("{khz:.1}").replace('.', ",")
            };
            trf("{} · {} kHz", &[&codec, &text])
        }
        _ => codec,
    }
}

/// Valeur d'un réglage facultatif : « non lu » si le casque l'a refusé.
fn opt<T>(value: Option<T>, text: impl Fn(T) -> String) -> String {
    value.map_or_else(|| tr("non lu").to_string(), text)
}

/// Première lettre en capitale (« profil personnalisé » → « Profil… »).
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Construit la vue à partir de l'état connu du casque.
pub fn main_view(
    name: Option<&str>,
    state: Option<&HeadsetState>,
    connections: &[Connection],
    problem: Option<&str>,
    read_only: Option<&str>,
) -> MainView {
    let connected = state.is_some();
    let banner = problem
        .or(read_only)
        .map(|p| format!("⚠ {p}"))
        .or_else(|| state.and_then(|s| s.audio_mode).and_then(eq_mode_note))
        .unwrap_or_default();
    let waiting = match (name, state) {
        (_, Some(_)) => String::new(),
        (Some(n), None) => trf("Connexion à « {} »…", &[&n]),
        (None, None) => {
            tr("Recherche du casque… Allumez-le et connectez-le à ce PC en Bluetooth.").to_string()
        }
    };
    MainView {
        title: name.unwrap_or(tr("Casque Sennheiser")).to_string(),
        connection: match (name, state) {
            (_, Some(_)) => tr("Connecté").to_string(),
            (Some(_), None) => tr("Connexion…").to_string(),
            (None, None) => tr("Recherche du casque…").to_string(),
        },
        connected,
        writable: connected && read_only.is_none(),
        battery: state.map(|s| s.battery),
        battery_text: state.map_or("—".to_string(), |s| format!("{} %", s.battery)),
        banner_is_warning: problem.or(read_only).is_some(),
        banner,
        waiting,
        home: state.map(home_rows).unwrap_or_default(),
        controls: state
            .map(|s| home_controls(s, connected && read_only.is_none()))
            .unwrap_or(HomeControls {
                noise_mode: -1,
                ..HomeControls::default()
            }),
        stream: state.map_or(String::new(), |s| stream_label(s.codec, s.sample_rate_hz)),
        connections: if connected {
            connections
                .iter()
                .map(|d| connection_row(&d.name, d.connected, d.this_device, read_only.is_none()))
                .collect()
        } else {
            Vec::new()
        },
    }
}

/// Lignes d'information de l'Accueil (les réglages modifiables sont dans
/// `HomeControls`).
fn home_rows(s: &HeadsetState) -> Vec<Row> {
    vec![
        row(
            tr("Mode audio"),
            opt(s.audio_mode, |m| {
                capitalize(audio_mode_label(m).trim_start_matches("Mode audio : "))
            }),
        ),
        row(
            tr("Profil personnalisé"),
            opt(s.sound_profile, |p| match p {
                SoundProfileStatus::NoProfile => tr("Aucun").to_string(),
                SoundProfileStatus::TestRunning => tr("Test en cours").to_string(),
                SoundProfileStatus::Received
                    if s.audio_mode == Some(AudioMode::Personalization) =>
                {
                    tr("Reçu par le casque, sélectionné").to_string()
                }
                SoundProfileStatus::Received => {
                    tr("Reçu par le casque, non sélectionné").to_string()
                }
                SoundProfileStatus::Unknown(b) => trf("État inconnu ({})", &[&b]),
            }),
        ),
    ]
}

/// Choix d'extinction automatique, dans l'ordre de `AUTO_OFF_CHOICES`.
pub const AUTO_OFF_LABELS: [&str; 4] = ["Jamais", "Après 15 min", "Après 30 min", "Après 60 min"];
/// Modes d'invites, dans l'ordre des boutons.
pub const PROMPT_MODES: [&str; 2] = ["Tonalités seules", "Tonalités et voix"];

/// Interrupteur : état et disponibilité (lu et modifiable).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggle {
    pub on: bool,
    pub enabled: bool,
}

fn toggle(value: Option<bool>, writable: bool) -> Toggle {
    Toggle {
        on: value == Some(true),
        enabled: writable && value.is_some(),
    }
}

/// Page « Réglages ».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsView {
    pub smart_pause: Toggle,
    pub auto_answer: Toggle,
    pub comfort_call: Toggle,
    pub battery_protection: Toggle,
    /// Indice dans `AUTO_OFF_LABELS`, -1 si non lu ou durée inconnue.
    pub auto_off: i32,
    pub auto_off_enabled: bool,
    /// Indice dans `PROMPT_MODES`, -1 si non lu ou autre.
    pub prompt_mode: i32,
    pub prompts_enabled: bool,
    /// Nom donné au casque (`2802`), vide si non lu.
    pub name: String,
    /// Renommage possible : casque connecté, modèle validé, nom lu.
    pub rename_enabled: bool,
    /// Volume des invites en % (échelle de l'app), -1 si non lu.
    pub prompt_volume: i32,
    pub prompt_volume_enabled: bool,
    /// Lignes en lecture seule : langue et volume des invites, priorité audio.
    pub info: Vec<Row>,
}

pub fn settings_view(
    state: Option<&HeadsetState>,
    info: Option<&DeviceInfo>,
    writable: bool,
) -> SettingsView {
    let Some(s) = state else {
        return SettingsView {
            auto_off: -1,
            prompt_mode: -1,
            name: String::new(),
            rename_enabled: false,
            prompt_volume: -1,
            ..SettingsView::default()
        };
    };
    let auto_off = s
        .auto_off_seconds
        .and_then(|secs| AUTO_OFF_CHOICES.iter().position(|&c| c == secs))
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1);
    let prompt_mode = match s.prompt_mode {
        Some(PromptMode::TonesOnly) => 0,
        Some(PromptMode::TonesAndVoice) => 1,
        _ => -1,
    };
    SettingsView {
        smart_pause: toggle(s.smart_pause, writable),
        auto_answer: toggle(s.auto_answer, writable),
        comfort_call: toggle(s.comfort_call, writable),
        battery_protection: toggle(s.battery_protection, writable),
        auto_off,
        auto_off_enabled: writable && s.auto_off_seconds.is_some(),
        prompt_mode,
        prompts_enabled: writable && s.prompt_mode.is_some(),
        name: info.and_then(|i| i.name.clone()).unwrap_or_default(),
        rename_enabled: writable && info.is_some_and(|i| i.name.is_some()),
        prompt_volume: info
            .and_then(|i| i.prompt_volume)
            .map_or(-1, |v| i32::from(prompt_volume_percent(v))),
        prompt_volume_enabled: writable && info.is_some_and(|i| i.prompt_volume.is_some()),
        info: info_rows(info),
    }
}

/// Réglages lus mais non modifiables depuis le PC (langue des invites,
/// priorité du mode audio) : affichés tels quels.
fn info_rows(info: Option<&DeviceInfo>) -> Vec<Row> {
    let info = info.cloned().unwrap_or_default();
    vec![
        row(
            tr("Langue des invites"),
            opt(info.prompt_language, |l| match l {
                2 => tr("Français").to_string(),
                other => trf("Valeur {}", &[&other]),
            }),
        ),
        row(
            tr("Priorité du mode audio"),
            opt(info.audio_priority, |p| match p {
                (1, 0) => tr("Qualité audio avancée").to_string(),
                (0, 0) => tr("Haute résolution 24 bits/96 kHz").to_string(),
                (0, 1) => tr("Sans perte 16 bits/44,1 kHz").to_string(),
                (h, l) => trf("Combinaison non observée ({}, {})", &[&h, &l]),
            }),
        ),
    ]
}

/// Lignes de la page « À propos » (casque, puis application).
pub fn about_rows(app_version: &str, info: Option<&DeviceInfo>) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(info) = info {
        rows.push(row(tr("Modèle"), opt(info.model.clone(), |m| m)));
        rows.push(row(tr("Micrologiciel"), opt(info.firmware.clone(), |f| f)));
        rows.push(row(
            tr("Révision matérielle"),
            opt(info.hardware_revision, |r| r.to_string()),
        ));
    }
    rows.extend([
        row(
            tr("Application"),
            trf("momentum-ctl-win {}", &[&app_version]),
        ),
        row(
            tr("Statut"),
            tr("Projet non officiel, sans lien avec Sennheiser ni Sonova"),
        ),
        row(tr("Licence"), tr("MIT OU Apache-2.0")),
        row(
            tr("Écritures"),
            tr("Uniquement celles capturées dans l'app puis validées sur le MOMENTUM 5"),
        ),
    ]);
    rows
}

/// Identité du casque pour la carte de présentation : modèle,
/// micrologiciel, révision matérielle. Seulement ce qui a été lu sur le
/// casque ; le numéro de série n'y figure jamais (donnée personnelle).
pub fn device_rows(info: Option<&DeviceInfo>) -> Vec<Row> {
    let Some(info) = info else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Some(model) = &info.model {
        rows.push(row(tr("Modèle"), model.clone()));
    }
    if let Some(firmware) = &info.firmware {
        rows.push(row(tr("Micrologiciel"), firmware.clone()));
    }
    if let Some(revision) = info.hardware_revision {
        rows.push(row(tr("Révision matérielle"), revision.to_string()));
    }
    rows
}

/// Ligne d'information avec sa provenance et l'heure de sa lecture.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InfoRow {
    pub label: String,
    pub value: String,
    /// « casque (GAIA), lu à 14:05:32 », « Windows, lu à 14:05:40 ».
    pub source: String,
}

/// Ce que connaît l'application, avec l'heure de chaque lecture.
#[derive(Default)]
pub struct AudioInputs<'a> {
    /// Nom du casque lu par le pilotage.
    pub device_name: Option<&'a str>,
    pub state: Option<&'a HeadsetState>,
    pub connections: &'a [Connection],
    /// Heure de connexion du pilotage.
    pub control_since: Option<&'a str>,
    /// Heure de la dernière liste des connexions.
    pub connections_at: Option<&'a str>,
    /// Heure depuis laquelle le codec, puis la fréquence, ont leur valeur.
    pub codec_since: Option<&'a str>,
    pub rate_since: Option<&'a str>,
    /// Lecture de Windows : résultat et heure.
    pub windows: Option<&'a Result<WindowsAudio, String>>,
    pub windows_at: Option<&'a str>,
}

/// Page « Connexion et audio ».
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioView {
    /// Constats : (cause probable, texte).
    pub findings: Vec<(bool, String)>,
    pub control: Vec<InfoRow>,
    pub windows: Vec<InfoRow>,
    pub stream: Vec<InfoRow>,
}

fn sourced(label: &str, value: impl Into<String>, source: impl Into<String>) -> InfoRow {
    InfoRow {
        label: label.to_string(),
        value: value.into(),
        source: source.into(),
    }
}

fn at(origin: &str, time: Option<&str>) -> String {
    time.map_or_else(
        || trf("{}, heure inconnue", &[&origin]),
        |t| trf("{}, lu à {}", &[&origin, &t]),
    )
}

/// Construit la page : aucune information n'est déduite d'une autre source
/// que la sienne, et une valeur absente reste « inconnue ».
pub fn audio_view(i: &AudioInputs) -> AudioView {
    let connected = i.state.is_some();
    let this_pc = i.connections.iter().find(|c| c.this_device);
    let others_connected = i
        .connections
        .iter()
        .filter(|c| !c.this_device && c.connected)
        .count();
    let control_link = ControlLink {
        connected,
        this_pc_state: this_pc.map(|c| c.state),
        others_connected,
    };

    let gaia = tr("casque (GAIA)");
    let mut control = vec![sourced(
        tr("Pilotage par ce PC"),
        if connected {
            tr("connecté (commande les réglages ; ne prouve pas que le son passe)")
        } else {
            tr("non connecté")
        },
        if connected {
            i.control_since.map_or_else(
                || tr("application").to_string(),
                |t| trf("application, depuis {}", &[&t]),
            )
        } else {
            tr("application").to_string()
        },
    )];
    if connected {
        control.push(sourced(
            tr("Ce PC dans la liste du casque"),
            this_pc.map_or(tr("absent de la liste lue").to_string(), |c| {
                describe_link_state(c.state)
            }),
            at(gaia, i.connections_at),
        ));
        control.push(sourced(
            tr("Autres appareils connectés"),
            others_connected.to_string(),
            at(gaia, i.connections_at),
        ));
    }

    let windows_origin = tr("Windows (sortie par défaut)");
    let windows = match i.windows {
        None => vec![sourced(
            tr("Sortie audio"),
            tr("pas encore lue"),
            tr("Windows"),
        )],
        Some(Err(e)) => vec![sourced(
            tr("Sortie audio"),
            trf("illisible : {}", &[&e]),
            at(tr("Windows"), i.windows_at),
        )],
        Some(Ok(w)) => match &w.default_output {
            None => vec![sourced(
                tr("Sortie par défaut"),
                tr("aucune"),
                at(windows_origin, i.windows_at),
            )],
            Some(o) => {
                let src = at(windows_origin, i.windows_at);
                let mut rows = vec![
                    sourced(tr("Sortie par défaut"), o.name.clone(), src.clone()),
                    sourced(
                        tr("C'est le casque"),
                        if o.headset { tr("oui") } else { tr("non") },
                        src.clone(),
                    ),
                    sourced(tr("Liaison"), describe_output(o), src.clone()),
                    sourced(
                        tr("Format de mixage (traitement de Windows, pas le codec Bluetooth)"),
                        o.mix.as_ref().map_or(tr("inconnu").to_string(), format_mix),
                        src.clone(),
                    ),
                ];
                rows.push(sourced(
                    tr("Volume"),
                    match (o.muted, o.volume) {
                        (Some(true), _) => tr("muet").to_string(),
                        (_, Some(v)) => format!("{v} %"),
                        _ => tr("inconnu").to_string(),
                    },
                    src.clone(),
                ));
                rows.push(sourced(
                    tr("Sorties actives du casque"),
                    if w.headset_outputs.is_empty() {
                        tr("aucune").to_string()
                    } else {
                        w.headset_outputs
                            .iter()
                            .map(describe_output)
                            .collect::<Vec<_>>()
                            .join(" ; ")
                    },
                    at(tr("Windows"), i.windows_at),
                ));
                rows
            }
        },
    };

    let mut stream = Vec::new();
    if let Some(s) = i.state {
        stream.push(sourced(
            tr("Codec"),
            match s.codec {
                None => tr("inconnu").to_string(),
                Some(0xFF) => tr("pas de lecture").to_string(),
                Some(c) => Codec(c)
                    .name()
                    .map_or(trf("codec {}", &[&c]), str::to_string),
            },
            i.codec_since.map_or(gaia.to_string(), |t| {
                trf("{}, valeur depuis {}", &[&gaia, &t])
            }),
        ));
        stream.push(sourced(
            tr("Fréquence d'échantillonnage"),
            s.sample_rate_hz.filter(|&hz| hz > 0).map_or(
                tr("inconnue (notifiée seulement au début d'une lecture)").to_string(),
                format_rate,
            ),
            match (s.sample_rate_hz.filter(|&hz| hz > 0), i.rate_since) {
                (Some(_), Some(t)) => trf("{} (notification), valeur depuis {}", &[&gaia, &t]),
                _ => trf("{} (notification)", &[&gaia]),
            },
        ));
    }

    let findings = findings(
        &control_link,
        i.windows.and_then(|w| w.as_ref().ok()),
        i.state.and_then(|s| s.codec),
    )
    .into_iter()
    .map(|f| (f.level == Level::Problem, f.text))
    .collect();
    AudioView {
        findings,
        control,
        windows,
        stream,
    }
}

/// Même page sous Linux ; le flux GAIA et la lecture PipeWire restent séparés.
pub fn pipewire_audio_view(
    i: &AudioInputs,
    result: Option<
        &Result<crate::pipewire_audio::PipeWireAudio, crate::pipewire_audio::AudioReadError>,
    >,
    read_at: Option<&str>,
) -> AudioView {
    use crate::pipewire_audio::Profile;
    let mut view = audio_view(i);
    let source = at(tr("PipeWire (pw-dump, lecture seule)"), read_at);
    view.findings.clear();
    if i.state.is_none() {
        view.findings.push((
            false,
            tr("Le casque n'est pas piloté par ce PC ; le son peut fonctionner sans pilotage.")
                .into(),
        ));
    }
    match result {
        None => {
            view.windows = vec![sourced(
                tr("Sorties du système"),
                tr("pas encore lues"),
                source,
            )]
        }
        Some(Err(error)) => {
            view.windows = vec![sourced(tr("Sorties du système"), error.to_string(), source)];
            view.findings.push((false, error.to_string()));
        }
        Some(Ok(audio)) => {
            view.windows = vec![sourced(
                tr("Le casque est la sortie par défaut"),
                match audio.default_headset {
                    Some(true) => tr("oui"),
                    Some(false) => tr("non"),
                    None => tr("inconnu (métadonnée ou sortie non identifiée)"),
                },
                source.clone(),
            )];
            if audio.outputs.is_empty() {
                view.findings.push((true, tr("Aucune sortie Bluetooth du casque piloté n'est visible dans PipeWire. Vérifier la connexion audio dans les paramètres du système ; le pilotage seul ne prouve pas une sortie audio.").into()));
            }
            if audio.default_headset == Some(false) {
                view.findings.push((true, tr("Le casque n'est pas la sortie audio par défaut. Le choisir dans les paramètres audio du système si c'est la sortie souhaitée.").into()));
            }
            for (index, output) in audio.outputs.iter().enumerate() {
                view.windows.extend([
                    sourced(
                        &trf("Sortie Bluetooth {} : profil", &[&(index + 1)]),
                        output.profile.map_or(tr("inconnu"), Profile::label),
                        source.clone(),
                    ),
                    sourced(
                        tr("Codec de cette sortie PipeWire"),
                        output.codec.unwrap_or(tr("inconnu ou non reconnu")),
                        source.clone(),
                    ),
                    sourced(
                        tr("Cette sortie est celle par défaut"),
                        output
                            .default
                            .map_or(tr("inconnu"), |d| if d { tr("oui") } else { tr("non") }),
                        source.clone(),
                    ),
                ]);
                if output.profile.is_some_and(Profile::call) {
                    view.findings.push((true, tr("Profil appel : qualité réduite (HFP/HSP). Hors appel, choisir le profil musique A2DP dans les paramètres audio si disponible.").into()));
                }
            }
            view.findings.push((false, tr("Le codec PipeWire décrit la sortie de ce PC ; il ne prouve ni un son en cours ni une transmission sans perte. Le flux rapporté par le casque peut venir de l'autre appareil en Multipoint.").into()));
        }
    }
    if i.connections.iter().any(|c| !c.this_device && c.connected) {
        view.findings.push((
            false,
            tr("Un autre appareil est connecté (Multipoint) et peut prendre la main sur l'audio.")
                .into(),
        ));
    }
    view
}

/// Choix « inchangé / activé / désactivé » des réglages on/off d'une scène.
pub const SCENE_SWITCH_CHOICES: [&str; 3] = ["inchangé", "activé", "désactivé"];
/// Choix du mode audio d'une scène.
pub const SCENE_SOUND_CHOICES: [&str; 4] = [
    "inchangé",
    "égaliseur",
    "voix (Speech Clarity)",
    "profil personnalisé",
];
/// Choix du contrôle du bruit d'une scène.
pub const SCENE_NOISE_CHOICES: [&str; 4] = ["inchangé", "adaptatif", "personnalisé", "arrêt"];

/// Formulaire de l'éditeur de scènes : indices dans les listes de choix
/// (0 = inchangé) ; niveau saisi en texte (vide = inchangé).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneForm {
    pub name: String,
    pub sound: i32,
    pub spatial: i32,
    pub noise: i32,
    pub level: String,
    pub anti_wind: i32,
    pub touch: i32,
    /// Indice dans `equalizer_choices` (0 = inchangé).
    pub equalizer: i32,
    pub bass_boost: i32,
}

/// Préréglages proposés à une scène : « inchangé », ceux de l'app et les
/// personnels, et celui de la scène s'il n'existe plus (signalé).
pub fn equalizer_choices(scene: Option<&Scene>, presets: &[Preset]) -> Vec<String> {
    let mut out = vec![tr("inchangé").to_string()];
    out.extend(presets.iter().map(|p| p.name.clone()));
    if let Some(name) = scene.and_then(|s| s.equalizer.as_deref())
        && !presets.iter().any(|p| p.name.eq_ignore_ascii_case(name))
    {
        out.push(name.to_string());
    }
    out
}

fn switch_index(value: Option<bool>) -> i32 {
    match value {
        None => 0,
        Some(true) => 1,
        Some(false) => 2,
    }
}

fn switch_value(index: i32) -> Option<bool> {
    match index {
        1 => Some(true),
        2 => Some(false),
        _ => None,
    }
}

/// Formulaire d'une scène.
pub fn scene_form(scene: &Scene, equalizers: &[String]) -> SceneForm {
    SceneForm {
        name: scene.name.clone(),
        sound: match scene.sound {
            None => 0,
            Some(SceneSound::Equalizer) => 1,
            Some(SceneSound::SpeechClarity) => 2,
            Some(SceneSound::Profile) => 3,
        },
        spatial: switch_index(scene.spatial),
        noise: match scene.noise {
            None => 0,
            Some(NoiseModeRequest::Adaptive) => 1,
            Some(NoiseModeRequest::Custom) => 2,
            Some(NoiseModeRequest::Off) => 3,
        },
        level: scene.level.map_or(String::new(), |l| l.to_string()),
        anti_wind: switch_index(scene.anti_wind),
        touch: switch_index(scene.touch),
        equalizer: scene
            .equalizer
            .as_deref()
            .and_then(|name| {
                equalizers
                    .iter()
                    .skip(1)
                    .position(|e| e.eq_ignore_ascii_case(name))
            })
            .map_or(0, |i| i32::try_from(i + 1).unwrap_or(0)),
        bass_boost: switch_index(scene.bass_boost),
    }
}

/// Scène décrite par le formulaire, vérifiée comme à la lecture du fichier.
pub fn scene_from_form(form: &SceneForm, equalizers: &[String]) -> Result<Scene, String> {
    let level = match form.level.trim() {
        "" => None,
        text => match text.parse::<u8>() {
            Ok(l) if l <= 100 => Some(l),
            _ => return Err(tr("niveau : nombre entier de 0 à 100, ou vide").to_string()),
        },
    };
    let scene = Scene {
        name: form.name.trim().to_string(),
        sound: match form.sound {
            1 => Some(SceneSound::Equalizer),
            2 => Some(SceneSound::SpeechClarity),
            3 => Some(SceneSound::Profile),
            _ => None,
        },
        spatial: switch_value(form.spatial),
        noise: match form.noise {
            1 => Some(NoiseModeRequest::Adaptive),
            2 => Some(NoiseModeRequest::Custom),
            3 => Some(NoiseModeRequest::Off),
            _ => None,
        },
        level,
        anti_wind: switch_value(form.anti_wind),
        touch: switch_value(form.touch),
        equalizer: usize::try_from(form.equalizer)
            .ok()
            .filter(|&i| i > 0)
            .and_then(|i| equalizers.get(i).cloned()),
        bass_boost: switch_value(form.bass_boost),
    };
    check_scene(&scene)?;
    Ok(scene)
}

/// Étapes prévues, dans l'ordre d'application : « bruit : adaptatif →
/// anti-vent désactivé ».
pub fn scene_preview(scene: &Scene) -> String {
    let steps: Vec<String> = scene.steps().iter().map(|s| s.label()).collect();
    if steps.is_empty() {
        tr("Aucun réglage : la scène est vide.").to_string()
    } else {
        trf("Étapes prévues : {}.", &[&(steps.join(" → "))])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_rows_show_only_what_was_read() {
        assert!(device_rows(None).is_empty());
        let info = DeviceInfo {
            model: Some("M5AEBT".into()),
            firmware: Some("6.21.5".into()),
            ..DeviceInfo::default()
        };
        let rows = device_rows(Some(&info));
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            ["Modèle", "Micrologiciel"]
        );
        assert_eq!(rows[1].value, "6.21.5");
    }

    #[test]
    fn head_tracking_needs_spatial_audio() {
        let mut s = HeadsetState {
            spatial_audio: Some(false),
            head_tracking: Some(true),
            ..HeadsetState::default()
        };
        let c = home_controls(&s, true);
        assert!(c.head_tracking_on && !c.head_tracking_enabled);
        s.spatial_audio = Some(true);
        assert!(home_controls(&s, true).head_tracking_enabled);
        assert!(!home_controls(&s, false).head_tracking_enabled);
    }

    #[test]
    fn battery_page_shows_curve_estimate_and_limits() {
        let mut log = BatteryLog::default();
        for i in 0..=10u64 {
            log.record(1_000_000 + i * 720, (90 - i) as u8);
        }
        let now = 1_000_000 + 10 * 720;
        let v = battery_view(&log, now, 0, Some(80));
        assert_eq!(v.current, "80 %");
        assert!(v.path.starts_with('M'), "{}", v.path);
        assert!(
            v.estimate.starts_with(
                "Estimation : environ 16 h restantes, d'après les 2,0 dernières heures"
            ),
            "{}",
            v.estimate
        );
        assert!(v.estimate.ends_with("du codec."), "{}", v.estimate);
        assert_eq!(value(&v.details, "Mesures sur la période"), "11");
        assert_eq!(value(&v.details, "Dernière mesure"), "il y a 0 min");
        assert_eq!(v.axis_start, "il y a 24 heures");
        let empty = battery_view(&BatteryLog::default(), now, 9, None);
        assert_eq!(empty.current, "non connecté");
        assert!(empty.path.is_empty());
        assert!(empty.estimate.starts_with("Pas d'estimation"));
        assert_eq!(empty.axis_start, "il y a 30 jours");
    }
    use crate::device::NoiseState;

    fn state() -> HeadsetState {
        HeadsetState {
            battery: 80,
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 0,
            },
            spatial_audio: Some(false),
            audio_mode: Some(AudioMode::Personalization),
            sound_profile: Some(SoundProfileStatus::Received),
            touch_controls: Some(true),
            smart_pause: None,
            auto_off_seconds: Some(1800),
            ..HeadsetState::default()
        }
    }

    fn value<'a>(rows: &'a [Row], label: &str) -> &'a str {
        &rows.iter().find(|r| r.label == label).unwrap().value
    }

    #[test]
    fn connected_view() {
        let v = main_view(Some("MOMENTUM 5 Exemple"), Some(&state()), &[], None, None);
        assert_eq!(v.title, "MOMENTUM 5 Exemple");
        assert_eq!(v.connection, "Connecté");
        assert!(v.connected && v.writable);
        assert_eq!(v.battery_text, "80 %");
        assert!(v.waiting.is_empty());
        assert_eq!(value(&v.home, "Mode audio"), "Profil personnalisé");
        assert_eq!(
            value(&v.home, "Profil personnalisé"),
            "Reçu par le casque, sélectionné"
        );
        // Profil sélectionné et Dolby Atmos coupé : Dolby Atmos grisé.
        assert_eq!(v.controls.noise_mode, 1);
        assert!(!v.controls.spatial_on && !v.controls.spatial_enabled);
        assert!(v.controls.touch_enabled);
        // Réglage refusé par le casque : « non lu », pas une valeur inventée.
        let sv = settings_view(Some(&state()), None, true);
        assert!(!sv.smart_pause.enabled && !sv.smart_pause.on);
        assert_eq!(sv.auto_off, 2);

        // Mode audio autre que l'égaliseur : bandeau d'information.
        assert!(v.banner.contains("profil personnalisé"));
        assert!(!v.banner_is_warning);
    }

    #[test]
    fn banner_priority_and_read_only() {
        let v = main_view(
            Some("M"),
            Some(&state()),
            &[],
            Some("Connexion perdue"),
            Some("lecture seule"),
        );
        assert_eq!(v.banner, "⚠ Connexion perdue");
        assert!(v.banner_is_warning);
        let v = main_view(
            Some("M"),
            Some(&state()),
            &[],
            None,
            Some("Modèle X non validé"),
        );
        assert_eq!(v.banner, "⚠ Modèle X non validé");
        assert!(v.connected && !v.writable);
    }

    #[test]
    fn stream_and_paired_devices() {
        assert_eq!(stream_label(Some(2), Some(44_100)), "aptX · 44,1 kHz");
        assert_eq!(stream_label(Some(1), Some(48_000)), "AAC · 48 kHz");
        assert_eq!(stream_label(Some(0xFF), Some(44_100)), "Pas de lecture");
        assert_eq!(stream_label(Some(42), None), "codec 42");
        assert_eq!(stream_label(None, None), "");
        let list = [
            Connection::sample("PC", true, true),
            Connection::sample("Téléphone", true, false),
            Connection::sample("TV", false, false),
        ];
        let v = main_view(Some("M"), Some(&state()), &list, None, None);
        let c = &v.connections;
        assert_eq!(
            (c[0].status.as_str(), c[0].can_act),
            ("Connecté (ce PC)", false)
        );
        // Ce PC, même lu « non connecté » (lien audio au repos), reste affiché
        // connecté et sans action.
        let pc_idle = [Connection::sample("PC", false, true)];
        let idle = main_view(Some("M"), Some(&state()), &pc_idle, None, None);
        assert_eq!(
            (
                idle.connections[0].status.as_str(),
                idle.connections[0].can_act
            ),
            ("Connecté (ce PC)", false)
        );
        assert_eq!((c[1].action.as_str(), c[1].can_act), ("Déconnecter", true));
        assert_eq!(
            (c[2].status.as_str(), c[2].action.as_str()),
            ("Non connecté", "Connecter")
        );
        // Lecture seule : aucune action.
        let ro = main_view(Some("M"), Some(&state()), &list, None, Some("modèle"));
        assert!(ro.connections.iter().all(|c| !c.can_act));
        // Déconnecté : rien n'est affiché (la liste pourrait être périmée).
        assert!(
            main_view(None, None, &list, None, None)
                .connections
                .is_empty()
        );
    }

    #[test]
    fn read_only_greys_out_controls() {
        let v = main_view(Some("M"), Some(&state()), &[], None, Some("lecture seule"));
        assert!(!v.controls.touch_enabled && !v.controls.spatial_enabled);
    }

    #[test]
    fn sound_page() {
        let presets = crate::presets::builtin_presets(8);
        let v = sound_view(Some(&state()), Some(&[0; 8]), &presets, true);
        assert_eq!(v.audio_mode, 2);
        assert!(v.modes_enabled && v.profile_available);
        assert_eq!(v.current_preset, 0);
        assert_eq!(v.preset_status, "Préréglage : Neutre");
        assert_eq!(v.removable, [false, false]);
        let v = sound_view(Some(&state()), Some(&[5; 8]), &presets, true);
        assert_eq!(v.current_preset, -1);
        assert_eq!(v.preset_status, "Courbe personnalisée, non enregistrée");
        // Sans profil reçu, pas de sélection possible.
        let no_profile = HeadsetState {
            sound_profile: Some(SoundProfileStatus::NoProfile),
            ..state()
        };
        assert!(!sound_view(Some(&no_profile), None, &presets, true).profile_available);
        assert!(!sound_view(Some(&state()), None, &presets, false).modes_enabled);
    }

    #[test]
    fn notice_never_hides_a_warning() {
        let v = main_view(Some("M"), Some(&state()), &[], None, None)
            .with_notice(Some("Profil exporté"));
        assert_eq!(v.banner, "Profil exporté");
        let v = main_view(
            Some("M"),
            Some(&state()),
            &[],
            Some("Connexion perdue"),
            None,
        )
        .with_notice(Some("Profil exporté"));
        assert_eq!(v.banner, "⚠ Connexion perdue");
    }

    #[test]
    fn settings_page_and_read_only_rows() {
        let info = DeviceInfo {
            name: Some("MOMENTUM 5 Exemple".into()),
            model: Some("M5AEBT Black".into()),
            firmware: Some("6.21.5".into()),
            hardware_revision: Some(4),
            prompt_language: Some(2),
            prompt_volume: Some(3),
            audio_priority: Some((1, 1)),
            multipoint: Some(true),
        };
        let sv = settings_view(Some(&state()), Some(&info), true);
        assert_eq!(value(&sv.info, "Langue des invites"), "Français");
        assert_eq!(sv.prompt_volume, 88);
        assert!(sv.prompt_volume_enabled);
        assert_eq!(sv.name, "MOMENTUM 5 Exemple");
        assert!(sv.rename_enabled);
        assert!(!settings_view(Some(&state()), Some(&info), false).rename_enabled);
        assert_eq!(
            value(&sv.info, "Priorité du mode audio"),
            "Combinaison non observée (1, 1)"
        );
        assert_eq!(sv.prompt_mode, -1);
        let about = about_rows("0.1.0", Some(&info));
        assert_eq!(value(&about, "Micrologiciel"), "6.21.5");
        assert_eq!(about_rows("0.1.0", None).len(), 4);
        // Déconnecté : rien de cliquable.
        let off = settings_view(None, Some(&info), true);
        assert!(!off.auto_off_enabled && off.auto_off == -1);
        assert!(!off.prompt_volume_enabled);
    }

    #[test]
    fn waiting_views() {
        let v = main_view(None, None, &[], None, None);
        assert_eq!(v.connection, "Recherche du casque…");
        assert!(!v.connected && !v.writable);
        assert_eq!(v.battery, None);
        assert!(v.waiting.starts_with("Recherche du casque"));
        assert!(v.home.is_empty());
        let v = main_view(Some("MOMENTUM 5"), None, &[], None, None);
        assert_eq!(v.waiting, "Connexion à « MOMENTUM 5 »…");
    }

    #[test]
    fn audio_page_keeps_sources_apart() {
        let state = HeadsetState {
            codec: Some(2),
            sample_rate_hz: Some(44_100),
            ..HeadsetState::default()
        };
        let mut pc = Connection::synthetic(1, "PC", true);
        pc.this_device = true;
        pc.state = 3;
        let phone = Connection::synthetic(2, "Téléphone", true);
        let output = crate::audio_check::AudioOutput {
            name: "Casque (MOMENTUM 5)".into(),
            headset: true,
            link: crate::audio_check::OutputLink::Bluetooth,
            hands_free: false,
            mix: Some(crate::audio_check::MixFormat {
                channels: 2,
                rate_hz: 48_000,
                bits: 32,
                float: true,
            }),
            muted: Some(false),
            volume: Some(30),
        };
        let windows = Ok(WindowsAudio {
            default_output: Some(output.clone()),
            headset_outputs: vec![output],
        });
        let v = audio_view(&AudioInputs {
            state: Some(&state),
            connections: &[pc, phone],
            control_since: Some("10:00:00"),
            connections_at: Some("10:00:05"),
            codec_since: Some("10:01:00"),
            rate_since: None,
            windows: Some(&windows),
            windows_at: Some("10:02:00"),
            ..AudioInputs::default()
        });
        let value = |rows: &[InfoRow], label: &str| {
            rows.iter()
                .find(|r| r.label.starts_with(label))
                .map(|r| (r.value.clone(), r.source.clone()))
                .unwrap()
        };
        assert!(value(&v.control, "Ce PC").0.contains("audio actif"));
        assert_eq!(value(&v.control, "Autres").0, "1");
        assert_eq!(
            value(&v.control, "Autres").1,
            "casque (GAIA), lu à 10:00:05"
        );
        assert_eq!(
            value(&v.windows, "Format de mixage").0,
            "2 canaux, 48 kHz, 32 bits virgule flottante"
        );
        assert_eq!(
            value(&v.windows, "Volume").1,
            "Windows (sortie par défaut), lu à 10:02:00"
        );
        assert_eq!(value(&v.stream, "Codec").0, "aptX");
        assert_eq!(
            value(&v.stream, "Codec").1,
            "casque (GAIA), valeur depuis 10:01:00"
        );
        assert_eq!(value(&v.stream, "Fréquence").0, "44,1 kHz");
        // Multipoint signalé, aucune cause probable.
        assert!(v.findings.iter().all(|(problem, _)| !problem));
        assert!(v.findings.iter().any(|(_, t)| t.contains("Multipoint")));
    }

    #[test]
    fn audio_page_without_headset_or_windows_reading() {
        let v = audio_view(&AudioInputs::default());
        assert_eq!(v.control[0].value, "non connecté");
        assert_eq!(v.control.len(), 1);
        assert_eq!(v.windows[0].value, "pas encore lue");
        assert!(v.stream.is_empty());
        let err = Err("accès refusé".to_string());
        let v = audio_view(&AudioInputs {
            windows: Some(&err),
            windows_at: Some("09:00:00"),
            ..AudioInputs::default()
        });
        assert_eq!(v.windows[0].value, "illisible : accès refusé");
    }

    #[test]
    fn scene_form_round_trip() {
        let presets = crate::presets::builtin_presets(8);
        let scene = Scene {
            name: "Musique".into(),
            sound: Some(SceneSound::Equalizer),
            noise: Some(NoiseModeRequest::Custom),
            level: Some(40),
            anti_wind: Some(false),
            equalizer: Some(presets[1].name.clone()),
            bass_boost: Some(true),
            ..Scene::default()
        };
        let choices = equalizer_choices(Some(&scene), &presets);
        assert_eq!(choices[0], "inchangé");
        let form = scene_form(&scene, &choices);
        assert_eq!((form.sound, form.noise, form.equalizer), (1, 2, 2));
        assert_eq!(form.level, "40");
        assert_eq!(scene_from_form(&form, &choices).unwrap(), scene);
        assert!(scene_preview(&scene).starts_with("Étapes prévues : mode audio : égaliseur → "));
    }

    #[test]
    fn scene_form_rejects_inconsistent_choices() {
        let choices = equalizer_choices(None, &[]);
        let mut form = SceneForm {
            name: "X".into(),
            noise: 1,
            level: "30".into(),
            ..SceneForm::default()
        };
        // Niveau avec le mode adaptatif : refusé comme dans le fichier.
        assert!(
            scene_from_form(&form, &choices)
                .unwrap_err()
                .contains("niveau")
        );
        form.level = "150".into();
        assert!(
            scene_from_form(&form, &choices)
                .unwrap_err()
                .contains("0 à 100")
        );
        form.level = String::new();
        assert!(scene_from_form(&form, &choices).is_ok());
        // Préréglage disparu : gardé dans la liste et sélectionné.
        let gone = Scene {
            name: "Ancien".into(),
            sound: Some(SceneSound::Equalizer),
            equalizer: Some("Supprimé".into()),
            ..Scene::default()
        };
        let choices = equalizer_choices(Some(&gone), &[]);
        assert_eq!(choices, ["inchangé", "Supprimé"]);
        assert_eq!(scene_form(&gone, &choices).equalizer, 1);
        assert_eq!(
            scene_preview(&Scene::default()),
            "Aucun réglage : la scène est vide."
        );
    }
}
