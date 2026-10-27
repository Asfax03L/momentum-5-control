//! Couche « casque » : lecture de l'état et réglages de haut niveau.
//!
//! Chaque réglage reproduit à l'octet près la séquence envoyée par Smart
//! Control Plus (capture HCI du 08/10/2026, voir docs/MOMENTUM5.md), puis
//! l'appelant peut relire l'état pour vérifier qu'il a bien été appliqué.

use crate::i18n::{tr, trf};
use std::time::Duration;

use thiserror::Error;
use tracing::{debug, warn};

use crate::client::{ClientError, GaiaClient};
use crate::profile::{ProfileBlock, ProfileExport};
use crate::protocol::command::{REGISTER_NOTIFICATION, core_set, feature, get, set};
use crate::protocol::messages::{
    M5NoiseMode, NoiseControlMode, m5_noise_mode, parse_battery, parse_codec, parse_core_features,
    parse_eq_config, parse_eq_gain, parse_firmware_version, parse_model_name,
    parse_noise_control_modes, parse_on_head_state, parse_paired_device, parse_sample_rate,
    parse_switch,
};
pub use crate::protocol::messages::{OnHeadState, PairedDevice};
use crate::protocol::{
    CommandId, Frame, PacketKind, ProtocolError, VENDOR_QUALCOMM, VENDOR_SENNHEISER,
};
use crate::transport::Transport;

/// Valeur maximale du curseur ANC ↔ transparence.
pub const MAX_LEVEL: u8 = 100;

/// Durées d'extinction automatique proposées par l'app, en secondes.
pub const AUTO_OFF_CHOICES: [u16; 4] = [0, 15 * 60, 30 * 60, 60 * 60];
/// Identifiant du minuteur d'extinction automatique.
const TIMER_AUTO_OFF: u8 = 0;

/// Modes d'invites relevés dans la capture (1 = audio uniquement,
/// 2 = audio et vocales). « ARRÊT » n'a pas été capturé : jamais envoyé.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PromptMode {
    #[default]
    Unknown,
    TonesOnly,
    TonesAndVoice,
}

impl PromptMode {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => PromptMode::TonesOnly,
            2 => PromptMode::TonesAndVoice,
            _ => PromptMode::Unknown,
        }
    }
}

/// Modes audio (`0803`/`0804`, deuxième octet) relevés dans la capture.
const AUDIO_MODE_EQUALIZER: u8 = 1;
const AUDIO_MODE_SPEECH_CLARITY: u8 = 2;
const AUDIO_MODE_PERSONALIZATION: u8 = 3;

/// Mode audio lu en `0804` ou notifié en `0884` (`[00, mode]`).
///
/// 1 égaliseur, 2 Speech Clarity, 3 profil de personnalisation du son.
/// Le mode 3 sélectionne le profil déjà enregistré, sans le réécrire
/// (capture HCI du 09/10/2026, validée depuis le PC) ; il n'est écrit que
/// par `select_sound_profile`, qui vérifie d'abord qu'un profil existe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AudioMode {
    Equalizer,
    SpeechClarity,
    Personalization,
    Unknown(u8),
}

impl Default for AudioMode {
    fn default() -> Self {
        AudioMode::Unknown(0)
    }
}

impl AudioMode {
    pub fn from_byte(b: u8) -> Self {
        match b {
            AUDIO_MODE_EQUALIZER => AudioMode::Equalizer,
            AUDIO_MODE_SPEECH_CLARITY => AudioMode::SpeechClarity,
            AUDIO_MODE_PERSONALIZATION => AudioMode::Personalization,
            other => AudioMode::Unknown(other),
        }
    }

    pub fn raw(self) -> u8 {
        match self {
            AudioMode::Equalizer => AUDIO_MODE_EQUALIZER,
            AudioMode::SpeechClarity => AUDIO_MODE_SPEECH_CLARITY,
            AudioMode::Personalization => AUDIO_MODE_PERSONALIZATION,
            AudioMode::Unknown(b) => b,
        }
    }

    pub fn label(self) -> String {
        match self {
            AudioMode::Equalizer => tr("égaliseur graphique").into(),
            AudioMode::SpeechClarity => tr("Speech Clarity").into(),
            AudioMode::Personalization => {
                tr("profil personnalisé (personnalisation du son)").into()
            }
            AudioMode::Unknown(b) => trf("inconnu (valeur {})", &[&b]),
        }
    }
}

/// État de la personnalisation du son (`2001`, notification `2081`).
///
/// Indépendant du mode audio : « profil reçu » ne dit pas que le profil
/// est sélectionné, ni qu'il survit à un redémarrage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SoundProfileStatus {
    /// 0 : aucun profil (lu avant le premier test).
    NoProfile,
    /// 1 : pendant le test auditif.
    TestRunning,
    /// 2 : profil reçu par le casque.
    Received,
    Unknown(u8),
}

impl SoundProfileStatus {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => SoundProfileStatus::NoProfile,
            1 => SoundProfileStatus::TestRunning,
            2 => SoundProfileStatus::Received,
            other => SoundProfileStatus::Unknown(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            SoundProfileStatus::NoProfile => tr("aucun profil").into(),
            SoundProfileStatus::TestRunning => tr("test en cours").into(),
            SoundProfileStatus::Received => tr("profil reçu par le casque").into(),
            SoundProfileStatus::Unknown(b) => trf("inconnu (valeur {})", &[&b]),
        }
    }
}

/// Issue d'une sélection du profil de personnalisation du son.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileSelection {
    /// `0803 [00 03]` envoyé et acquitté par le casque.
    WriteAcknowledged,
    /// Une lecture fraîche `0804`, faite dans la même opération, montrait
    /// déjà le profil, sans notification reçue entre-temps qui la contredise :
    /// aucune écriture n'a été envoyée. Ce n'est ni un acquittement, ni une
    /// garantie pour la suite (Android peut changer le mode à tout moment).
    AlreadySelectedAtRead,
}

/// Sens du code d'erreur d'une sélection de mode audio (`0803` → `0983
/// [code]`), d'après le décodeur de Smart Control Plus 1.6.3 (analyse
/// statique). Propre à cette commande : le même code a un autre sens pour
/// d'autres commandes. Seul `80` a été observé sur le casque.
pub fn audio_mode_error_meaning(code: u8) -> Option<&'static str> {
    match code {
        0x80 => Some(tr("mode déjà sélectionné")),
        0x81 => Some(tr("mode non configuré")),
        0x82 => Some(tr("mode non pris en charge")),
        0x83 => Some(tr("erreur générique")),
        _ => None,
    }
}

fn describe_audio_mode_error(payload: &[u8]) -> String {
    match payload.first() {
        None => tr("sans code").to_string(),
        Some(&code) => match audio_mode_error_meaning(code) {
            Some(meaning) => trf(
                "code {} : {}, d'après l'app officielle",
                &[&(format!("{:02X}", code)), &meaning],
            ),
            None => trf("code {} inconnu", &[&(format!("{:02X}", code))]),
        },
    }
}

/// Modes de la paire `[mode, état]` utilisés en écriture.
const PAIR_ANTI_WIND: u8 = 1;
const PAIR_ADAPTIVE: u8 = 3;

#[derive(Debug, Error)]
pub enum DeviceError {
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("{}", trf("niveau {} hors limites (0 à {})", &[&.0, &MAX_LEVEL]))]
    InvalidLevel(u8),
    #[error(
        "{}",
        tr("le casque n'a pas répondu les paires attendues pour le contrôle du bruit")
    )]
    MissingPair,
    #[error("{}", trf("bande {} inexistante (l'égaliseur en a {})", &[&.band, &.count]))]
    InvalidBand { band: u8, count: usize },
    #[error("{}", trf("gain {} hors limites ({} à {} dixièmes de dB)", &[&.gain, &.min, &.max]))]
    InvalidGain { gain: i8, min: i8, max: i8 },
    #[error("{}", trf("durée d'extinction {} s non proposée par l'app (0, 900, 1800 ou 3600)", &[&.0]))]
    InvalidAutoOff(u16),
    #[error("{}", tr("mode d'invites non validé sur le matériel"))]
    InvalidPromptMode,
    #[error("{}", trf("volume des invites {} hors de l'échelle de l'app (−20 à +6)", &[&.0]))]
    InvalidPromptVolume(i8),
    #[error("{}", trf("nom refusé : {}", &[&.0]))]
    InvalidName(&'static str),
    #[error("{}", tr("impossible de déconnecter l'appareil qui pilote le casque"))]
    OwnConnection,
    #[error(
        "{}",
        tr("le suivi de tête n'est réglable qu'avec l'audio spatial activé")
    )]
    SpatialAudioOff,
    #[error("{}", trf("aucun profil de personnalisation du son reçu par le casque (état {})", &[&format!("{:?}", .0)]))]
    NoSoundProfile(Option<u8>),
    /// Réponse d'erreur `0983` à `0803`, payload brut conservé.
    #[error("{}", trf("sélection du mode audio refusée par le casque (0983, {})", &[&describe_audio_mode_error(.0)]))]
    AudioModeRefused(Vec<u8>),
    #[error("{}", trf("{} : lecture seule (les écritures n'ont été vérifiées que sur le MOMENTUM 5, micrologiciel {})", &[&.0, &VALIDATED_FIRMWARE]))]
    WritesBlocked(String),
}

/// État du contrôle du bruit, lu en trois GET.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NoiseState {
    pub anc_enabled: bool,
    pub adaptive: bool,
    pub anti_wind: bool,
    pub level: u8,
}

impl NoiseState {
    pub fn mode(&self) -> M5NoiseMode {
        m5_noise_mode(self.anc_enabled, self.adaptive, self.level)
    }
}

/// Features obligatoires (`0495:0007 [feature]`, comme l'app).
/// La feature du port est ajoutée séparément, en abonnement facultatif.
pub const NOTIFICATION_FEATURES: [u8; 6] = [
    feature::BATTERY,
    feature::ANC,
    feature::SPATIAL_AUDIO,
    feature::USER_EQ,
    // Mode audio (Speech Clarity, notification 0884).
    feature::GENERIC_AUDIO,
    // Commandes tactiles (notification 1687).
    feature::MMI,
];

/// Nombre maximal de bandes acceptées (garde-fou contre une réponse folle).
const MAX_EQ_BANDS: u8 = 16;

/// État de l'égaliseur graphique (gains en dixièmes de dB).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EqState {
    pub min_gain: i8,
    pub max_gain: i8,
    pub gains: Vec<i8>,
    pub bass_boost: bool,
}

impl EqState {
    /// Notifications relevées dans la capture : `1082` (tous les gains, un
    /// octet par bande) et `1089` (Bass Boost). Renvoie `true` si changement.
    pub fn apply_notification(&mut self, frame: &Frame) -> bool {
        if frame.vendor != VENDOR_SENNHEISER || frame.kind() != PacketKind::Notification {
            return false;
        }
        let before = self.clone();
        let request = frame.command.request();
        if request == get::EQ_ALL_GAINS_NOTIFICATION.request() {
            let gains: Vec<i8> = frame.payload.iter().map(|&g| g as i8).collect();
            // Longueur et bornes vérifiées : une notification incohérente ne
            // remplace pas le dernier état valide.
            if gains.len() == self.gains.len() && gains.iter().all(|&g| self.check_gain(g).is_ok())
            {
                self.gains = gains;
            } else {
                warn!(payload = ?frame.payload, "notification d'égaliseur incohérente ignorée");
            }
        } else if request == get::BASS_BOOST
            && let Ok(on) = parse_switch(request, &frame.payload)
        {
            self.bass_boost = on;
        }
        *self != before
    }

    /// Vérifie qu'un gain est dans les limites annoncées par le casque.
    pub fn check_gain(&self, gain: i8) -> Result<(), DeviceError> {
        if gain < self.min_gain || gain > self.max_gain {
            return Err(DeviceError::InvalidGain {
                gain,
                min: self.min_gain,
                max: self.max_gain,
            });
        }
        Ok(())
    }
}

/// Échelle du volume des invites (`081C`/`081D`), relevée dans l'app le
/// 09/10/2026 : 0 % → −20, 77 % → 0, 99-100 % → +6.
pub const PROMPT_VOLUME_MIN: i8 = -20;
pub const PROMPT_VOLUME_MAX: i8 = 6;

/// Pourcentage du curseur de l'app → valeur du casque :
/// `arrondi(26 × % / 100 − 20)` (15 points relevés, tous conformes).
pub fn prompt_volume_from_percent(percent: u8) -> i8 {
    let p = f32::from(percent.min(100));
    // Résultat entre −20 et +6 : la conversion ne peut pas déborder.
    (p * 26.0 / 100.0 - 20.0).round() as i8
}

/// Valeur du casque → pourcentage approché pour l'affichage (0 → 77 %).
pub fn prompt_volume_percent(value: i8) -> u8 {
    let v = f32::from(value.clamp(PROMPT_VOLUME_MIN, PROMPT_VOLUME_MAX));
    ((v + 20.0) * 100.0 / 26.0).round() as u8
}

/// Appareil appairé, vu par « Gestion des connexions » (`140D`/`140E`).
/// L'adresse Bluetooth sert uniquement aux commandes : elle n'est ni
/// affichée, ni journalisée (voir `Debug`), ni exportée.
#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
    address: [u8; 6],
    pub name: String,
    pub connected: bool,
    /// État brut de la fiche (`140E`) : 1, 2 et 3 observés ; voir
    /// `audio_check::describe_link_state`.
    pub state: u8,
    /// L'appareil qui pose la question (ce PC), d'après `1413`.
    pub this_device: bool,
}

impl Connection {
    /// Même appareil (même adresse), quels que soient son nom et son état :
    /// deux appareils du même nom restent distincts.
    pub fn same_device(&self, other: &Connection) -> bool {
        self.address == other.address
    }

    /// Identifiant opaque et stable de l'appareil pour les interfaces
    /// (empreinte de son adresse, qui ne sort jamais du service) : une
    /// action vise cet appareil-là, même si la liste change d'ordre ou si
    /// deux appareils portent le même nom (F05 de la revue).
    pub fn opaque_id(&self) -> String {
        let address = self
            .address
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":");
        crate::transport::choice::fingerprint(&address)
    }

    /// Appareil fictif pour les tests d'autres modules : adresse
    /// `00:00:00:00:00:<id>`, jamais envoyée à un casque réel.
    #[doc(hidden)]
    pub fn synthetic(id: u8, name: &str, connected: bool) -> Self {
        Self {
            address: [0, 0, 0, 0, 0, id],
            name: name.to_string(),
            connected,
            state: if connected { 2 } else { 1 },
            this_device: false,
        }
    }

    /// Appareil fictif pour les tests (adresse nulle, jamais envoyée).
    #[cfg(test)]
    pub(crate) fn sample(name: &str, connected: bool, this_device: bool) -> Self {
        Self {
            address: [0; 6],
            name: name.to_string(),
            connected,
            state: if connected { 2 } else { 1 },
            this_device,
        }
    }
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("name", &self.name)
            .field("connected", &self.connected)
            .field("state", &self.state)
            .field("this_device", &self.this_device)
            .finish_non_exhaustive()
    }
}

fn address(bytes: &[u8]) -> Option<[u8; 6]> {
    bytes.get(..6)?.try_into().ok()
}

/// Fiche `140E` : `[adresse, état, 3 octets inconnus, nom…\0]`.
fn parse_connection(payload: &[u8], own: Option<[u8; 6]>) -> Result<Connection, ProtocolError> {
    let bad = |reason| ProtocolError::BadPayload {
        command: get::CONNECTION_INFO.response(),
        reason,
    };
    let addr = address(payload).ok_or_else(|| bad("moins de 6 octets"))?;
    let (state, name) = match &payload[6..] {
        [state, _, _, _, name @ ..] => (*state, name),
        _ => return Err(bad("fiche trop courte")),
    };
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    Ok(Connection {
        address: addr,
        name: String::from_utf8_lossy(&name[..end]).into_owned(),
        // 2 = connecté ; 3 = connecté, vraisemblablement en lecture audio
        // (relevé du 09/10/2026, l'app affiche deux fois « connecté »).
        connected: state >= 2,
        state,
        this_device: own == Some(addr),
    })
}

/// Longueur maximale d'un nom, en octets. Limite prudente choisie par le
/// projet : celle de l'app n'a pas été relevée.
pub const NAME_MAX_LEN: usize = 32;

/// Vérifie un nom avant l'écriture `2801`. Seuls les caractères ASCII
/// imprimables sont acceptés : les accents n'ont pas été essayés sur le
/// casque.
pub fn check_name(name: &str) -> Result<(), DeviceError> {
    if name.trim().is_empty() {
        return Err(DeviceError::InvalidName("nom vide"));
    }
    if name.len() > NAME_MAX_LEN {
        return Err(DeviceError::InvalidName("plus de 32 caractères"));
    }
    if !name.bytes().all(|b| (0x20..0x7F).contains(&b)) {
        return Err(DeviceError::InvalidName(
            "seuls les caractères ASCII sans accent sont acceptés",
        ));
    }
    if name.trim() != name {
        return Err(DeviceError::InvalidName("espace au début ou à la fin"));
    }
    Ok(())
}

/// Nom lu en `2802` : UTF-8, zéros de fin ignorés.
fn parse_device_name(payload: &[u8]) -> String {
    let end = payload.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    String::from_utf8_lossy(&payload[..end]).into_owned()
}

/// Identité du casque et réglages lus à part de l'état : nom, invites,
/// priorité du mode audio. Tout est facultatif (un refus du casque donne
/// `None`).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceInfo {
    /// Nom donné au casque (`2802`), « MOMENTUM 5 Exemple » par exemple.
    pub name: Option<String>,
    pub model: Option<String>,
    /// « 6.21.5 ».
    pub firmware: Option<String>,
    /// Révision matérielle (`1200`, deux premiers octets).
    pub hardware_revision: Option<u16>,
    /// Langue des invites (`0807`) : 2 = français, seule valeur confirmée.
    pub prompt_language: Option<u8>,
    /// Volume des invites (`081D`), de −20 à +6 (0 = 77 % dans l'app).
    pub prompt_volume: Option<i8>,
    /// Priorité du mode audio : (`0406`, `0408`).
    pub audio_priority: Option<(u8, u8)>,
    /// Multipoint actif (`1409` ≥ 2 connexions ; notification `001D:0E80`).
    pub multipoint: Option<bool>,
}

impl DeviceInfo {
    /// Notifications `089D` (volume des invites), `0486` et `0488`
    /// (priorité). Renvoie `true` si quelque chose change.
    pub fn apply_notification(&mut self, frame: &Frame) -> bool {
        // Multipoint : notification du core Qualcomm `0E80 [état]`.
        if frame.vendor == VENDOR_QUALCOMM
            && frame.kind() == PacketKind::Notification
            && frame.command.request() == core_set::MULTIPOINT
            && let Some(&state) = frame.payload.first()
        {
            let before = self.multipoint;
            self.multipoint = Some(state != 0);
            return before != self.multipoint;
        }
        if frame.vendor != VENDOR_SENNHEISER || frame.kind() != PacketKind::Notification {
            return false;
        }
        let before = self.clone();
        let request = frame.command.request();
        let Some(&v) = frame.payload.first() else {
            return false;
        };
        if request == get::PROMPT_VOLUME {
            self.prompt_volume = Some(v as i8);
        } else if request == get::BT_COMPATIBILITY {
            self.audio_priority = self.audio_priority.map(|(_, l)| (v, l));
        } else if request == get::LOSSLESS {
            self.audio_priority = self.audio_priority.map(|(h, _)| (h, v));
        }
        *self != before
    }
}

/// État complet affiché par l'interface.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
///
/// Batterie et contrôle du bruit sont indispensables. Les autres réglages
/// sont facultatifs : `None` veut dire « non lu » (commande refusée ou
/// réponse illisible), ce qui n'empêche pas d'afficher le reste.
pub struct HeadsetState {
    pub battery: u8,
    pub noise: NoiseState,
    pub spatial_audio: Option<bool>,
    /// Suivi de tête de Dolby Atmos (`3603`, notification `3683`).
    pub head_tracking: Option<bool>,
    pub audio_mode: Option<AudioMode>,
    pub sound_profile: Option<SoundProfileStatus>,
    pub battery_protection: Option<bool>,
    /// Commandes tactiles actives (le casque stocke l'inverse : un verrou).
    pub touch_controls: Option<bool>,
    pub smart_pause: Option<bool>,
    pub auto_answer: Option<bool>,
    pub comfort_call: Option<bool>,
    /// Extinction automatique en secondes (0 = jamais).
    pub auto_off_seconds: Option<u16>,
    pub prompt_mode: Option<PromptMode>,
    /// Codec du flux audio vers le casque (`0800`, notification `0880` ;
    /// `0xFF` relevé à l'arrêt de la lecture).
    pub codec: Option<u8>,
    /// Fréquence d'échantillonnage, connue seulement par notification `089A`.
    pub sample_rate_hz: Option<u32>,
    /// État physique (`0402` / `0482`) ; `None` si la lecture est indisponible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_head_state: Option<OnHeadState>,
}

impl HeadsetState {
    /// Applique une notification du casque. Renvoie `true` si l'état change.
    ///
    /// Notifications relevées dans la capture de Smart Control Plus :
    /// `0683` batterie, `1A85` ANC, `1A81` paires, `1A83` curseur,
    /// `3685` audio spatial, `0482` état physique du port. Les autres sont ignorées.
    pub fn apply_notification(&mut self, frame: &Frame) -> bool {
        if frame.vendor != VENDOR_SENNHEISER || frame.kind() != PacketKind::Notification {
            return false;
        }
        let before = *self;
        let request = frame.command.request();
        let p = &frame.payload;
        if request == get::ON_HEAD_STATE {
            if let Ok(state) = parse_on_head_state(p) {
                self.on_head_state = Some(state);
            }
        } else if request == get::BATTERY {
            if let Ok(v) = parse_battery(p) {
                self.battery = v;
            }
        } else if request == get::ANC_ENABLED {
            if let Ok(on) = parse_switch(request, p) {
                self.noise.anc_enabled = on;
            }
        } else if request == get::NOISE_CONTROL_MODES {
            if let Ok(pairs) = parse_noise_control_modes(p) {
                for (mode, state) in pairs {
                    match mode {
                        NoiseControlMode::Adaptive => self.noise.adaptive = state == 1,
                        NoiseControlMode::AntiWind => self.noise.anti_wind = state == 1,
                        _ => {}
                    }
                }
            }
        } else if request == get::TRANSPARENCY_LEVEL {
            match p.first() {
                Some(&level) if level <= MAX_LEVEL => self.noise.level = level,
                _ => warn!(payload = ?p, "notification de curseur incohérente ignorée"),
            }
        } else if request == get::SPATIAL_AUDIO {
            if let Ok(on) = parse_switch(request, p) {
                self.spatial_audio = Some(on);
            }
        } else if request == get::HEAD_TRACKING {
            if let Ok(on) = parse_switch(request, p) {
                self.head_tracking = Some(on);
            }
        } else if request == get::AUDIO_MODE {
            // Notification 0884 : [00, mode].
            if let Some(&mode) = p.get(1) {
                self.audio_mode = Some(AudioMode::from_byte(mode));
            }
        } else if request == get::SOUND_PERSONALIZATION {
            if let Some(&status) = p.first() {
                self.sound_profile = Some(SoundProfileStatus::from_byte(status));
            }
        } else if request == get::TOUCH_LOCK {
            if let Ok(locked) = parse_switch(request, p) {
                self.touch_controls = Some(!locked);
            }
        } else if request == get::PROMPT_MODE
            && let Some(&mode) = p.first()
        {
            // Notification 0882 relevée dans la capture.
            self.prompt_mode = Some(PromptMode::from_byte(mode));
        } else if request == get::CODEC {
            if let Some(&codec) = p.first() {
                self.codec = Some(codec);
            }
        } else if request == get::SAMPLE_RATE_NOTIFICATION.request()
            && let Some(hz) = parse_sample_rate(p)
        {
            self.sample_rate_hz = Some(hz);
        }
        *self != before
    }
}

/// Mode demandé par l'utilisateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NoiseModeRequest {
    Off,
    Adaptive,
    Custom,
}

/// Préfixe du nom de modèle (`1206`) du seul casque sur lequel les
/// écritures ont été capturées puis validées : le MOMENTUM 5 (« M5AEBT »).
pub const VALIDATED_MODEL_PREFIX: &str = "M5AEBT";

/// Micrologiciel (`1201`) sur lequel ces écritures ont été validées. Un
/// autre micrologiciel peut changer le sens d'une commande : lecture seule
/// tant qu'il n'a pas été vérifié à son tour.
pub const VALIDATED_FIRMWARE: &str = "6.21.5";

/// Raison de refuser les écritures pour ce modèle et ce micrologiciel, ou
/// `None` s'ils sont validés. Un micrologiciel illisible n'est pas validé.
pub fn write_block(model: &str, firmware: Option<&str>) -> Option<String> {
    if !model.starts_with(VALIDATED_MODEL_PREFIX) {
        return Some(format!("modèle « {model} » non validé"));
    }
    match firmware {
        Some(VALIDATED_FIRMWARE) => None,
        Some(other) => Some(format!("micrologiciel {other} non validé")),
        None => Some("micrologiciel illisible".to_string()),
    }
}

pub struct Headset<T: Transport> {
    client: GaiaClient<T>,
    timeout: Duration,
    /// Raison donnée par `identify` quand le modèle ou le micrologiciel
    /// n'est pas validé : les écritures sont alors refusées sans rien
    /// envoyer.
    write_block: Option<String>,
}

/// Notification qui rend caduque une lecture « profil déjà sélectionné » :
/// changement de mode audio (`0884`), d'état ou d'usage du profil (`2081`,
/// `208A`), ou audio spatial réactivé (`3685 [01]`). Vendor Sennheiser
/// seulement ; les autres notifications sont gardées sans effet ici.
fn contradicts_profile_reading(frame: &Frame) -> bool {
    if frame.vendor != VENDOR_SENNHEISER || frame.kind() != PacketKind::Notification {
        return false;
    }
    let request = frame.command.request();
    request == get::AUDIO_MODE
        || request == get::SOUND_PERSONALIZATION
        || request == get::SOUND_PROFILE_USAGE_NOTIFICATION.request()
        || (request == get::SPATIAL_AUDIO && frame.payload.first() != Some(&0))
}

/// Raison donnée tant que `identify` n'a pas qualifié le casque.
pub const NOT_IDENTIFIED: &str = "casque non identifié";

impl<T: Transport> Headset<T> {
    /// Casque connecté, pas encore identifié : lectures permises, écritures
    /// refusées jusqu'à ce que [`Headset::identify`] ait validé le modèle et
    /// le micrologiciel.
    pub fn new(transport: T, timeout: Duration) -> Self {
        Self {
            client: GaiaClient::new(transport),
            timeout,
            write_block: Some(NOT_IDENTIFIED.to_string()),
        }
    }

    /// Casque tenu pour validé, sans échange d'identification : réservé aux
    /// tests (fonctionnalité `essais`, jamais activée par les binaires).
    #[cfg(any(test, feature = "essais"))]
    pub fn validated_for_tests(transport: T, timeout: Duration) -> Self {
        Self {
            write_block: None,
            ..Self::new(transport, timeout)
        }
    }

    /// Lit le nom de modèle puis le micrologiciel, et n'autorise les
    /// écritures que sur le couple validé ([`write_block`]). À appeler juste
    /// après la connexion : sans cet appel, les écritures restent refusées.
    /// Un modèle illisible compte comme non validé.
    /// Renvoie le nom lu (ou « inconnu »).
    pub fn identify(&mut self) -> String {
        let model = self
            .optional(|h| Ok(parse_model_name(&h.get(get::MODEL_NAME)?)?))
            .ok()
            .flatten()
            .unwrap_or_else(|| "inconnu".to_string());
        // Autre modèle : inutile de lire son micrologiciel.
        let firmware = if model.starts_with(VALIDATED_MODEL_PREFIX) {
            self.optional(|h| {
                let (a, b, c) = parse_firmware_version(&h.get(get::FIRMWARE_VERSION)?)?;
                Ok(format!("{a}.{b}.{c}"))
            })
            .ok()
            .flatten()
        } else {
            None
        };
        self.write_block = write_block(&model, firmware.as_deref());
        model
    }

    /// Faux si `identify` a trouvé un modèle ou un micrologiciel non validé.
    pub fn writes_allowed(&self) -> bool {
        self.write_block.is_none()
    }

    /// Raison de la lecture seule (« micrologiciel 6.22.0 non validé »…).
    pub fn write_block_reason(&self) -> Option<&str> {
        self.write_block.as_deref()
    }

    pub fn client(&mut self) -> &mut GaiaClient<T> {
        &mut self.client
    }

    fn get(&mut self, command: CommandId) -> Result<Vec<u8>, DeviceError> {
        let frame = self
            .client
            .request(&Frame::sennheiser_request(command), self.timeout)?;
        Ok(frame.payload)
    }

    /// Envoie une écriture de réglage et attend l'accusé (réponse vide sur
    /// M5). Refusée sans rien envoyer si le modèle n'est pas validé.
    fn write(&mut self, command: CommandId, payload: Vec<u8>) -> Result<(), DeviceError> {
        if let Some(reason) = &self.write_block {
            return Err(DeviceError::WritesBlocked(reason.clone()));
        }
        self.ack(command, payload)
    }

    /// Écriture du core Qualcomm (vendor 0x001D), mêmes garde-fous.
    fn write_core(&mut self, command: CommandId, payload: Vec<u8>) -> Result<(), DeviceError> {
        if let Some(reason) = &self.write_block {
            return Err(DeviceError::WritesBlocked(reason.clone()));
        }
        self.ack_vendor(VENDOR_QUALCOMM, command, payload)
    }

    /// Envoie une requête qui ne change aucun réglage et attend l'accusé.
    fn ack(&mut self, command: CommandId, payload: Vec<u8>) -> Result<(), DeviceError> {
        self.ack_vendor(VENDOR_SENNHEISER, command, payload)
    }

    fn ack_vendor(
        &mut self,
        vendor: u16,
        command: CommandId,
        payload: Vec<u8>,
    ) -> Result<(), DeviceError> {
        debug!(vendor, %command, payload = %crate::hex::loggable(&payload), "écriture");
        let ack = self
            .client
            .request(&Frame::new(vendor, command, payload), self.timeout)?;
        if !ack.payload.is_empty() {
            warn!(%command, payload = ?ack.payload, "accusé non vide (inattendu sur M5)");
        }
        Ok(())
    }

    pub fn battery(&mut self) -> Result<u8, DeviceError> {
        let p = self.get(get::BATTERY)?;
        Ok(parse_battery(&p)?)
    }

    pub fn read_noise(&mut self) -> Result<NoiseState, DeviceError> {
        let anc_enabled = parse_switch(get::ANC_ENABLED, &self.get(get::ANC_ENABLED)?)?;
        let pairs = parse_noise_control_modes(&self.get(get::NOISE_CONTROL_MODES)?)?;
        let state_of = |wanted: NoiseControlMode| {
            pairs
                .iter()
                .find(|(m, _)| *m == wanted)
                .map(|(_, s)| *s == 1)
                .ok_or(DeviceError::MissingPair)
        };
        let adaptive = state_of(NoiseControlMode::Adaptive)?;
        let anti_wind = state_of(NoiseControlMode::AntiWind)?;
        let level =
            *self
                .get(get::TRANSPARENCY_LEVEL)?
                .first()
                .ok_or(ProtocolError::BadPayload {
                    command: get::TRANSPARENCY_LEVEL.response(),
                    reason: "payload vide",
                })?;
        Ok(NoiseState {
            anc_enabled,
            adaptive,
            anti_wind,
            level,
        })
    }

    /// Curseur ANC ↔ transparence (mode Personnalisé). App : `1A02 [niveau]`.
    pub fn set_level(&mut self, level: u8) -> Result<(), DeviceError> {
        if level > MAX_LEVEL {
            return Err(DeviceError::InvalidLevel(level));
        }
        self.write(set::TRANSPARENCY_LEVEL, vec![level])
    }

    /// Change de mode en reproduisant l'ordre des trames de l'app.
    pub fn set_mode(&mut self, mode: NoiseModeRequest) -> Result<(), DeviceError> {
        match mode {
            // App : 1A04 [01] puis 1A00 [03 01].
            NoiseModeRequest::Adaptive => {
                self.write(set::ANC_ENABLED, vec![1])?;
                self.write(set::NOISE_CONTROL_MODE, vec![PAIR_ADAPTIVE, 1])
            }
            // App : 1A00 [03 00] puis 1A04 [01].
            NoiseModeRequest::Custom => {
                self.write(set::NOISE_CONTROL_MODE, vec![PAIR_ADAPTIVE, 0])?;
                self.write(set::ANC_ENABLED, vec![1])
            }
            // Non capturé dans l'app : 1A04 [00] (capture ACCENTUM de
            // PROTOCOL.md). Validé sur M5 : l'app affiche bien « ARRÊT ».
            NoiseModeRequest::Off => self.write(set::ANC_ENABLED, vec![0]),
        }
    }

    /// Anti-vent. App : `1A00 [01 état]`.
    pub fn set_anti_wind(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::NOISE_CONTROL_MODE, vec![PAIR_ANTI_WIND, u8::from(on)])
    }

    /// Suivi de tête (`3603`).
    pub fn head_tracking(&mut self) -> Result<bool, DeviceError> {
        let p = self.get(get::HEAD_TRACKING)?;
        Ok(parse_switch(get::HEAD_TRACKING, &p)?)
    }

    /// Suivi de tête. App : `3602 [0|1]`, proposé seulement avec l'audio
    /// spatial actif : refusé ici sinon, sans rien envoyer.
    pub fn set_head_tracking(&mut self, on: bool) -> Result<(), DeviceError> {
        if !self.spatial_audio()? {
            return Err(DeviceError::SpatialAudioOff);
        }
        self.write(set::HEAD_TRACKING, vec![u8::from(on)])
    }

    /// Nombre de connexions simultanées (`1409`) : 1 sans Multipoint, 2 avec.
    pub fn connection_slots(&mut self) -> Result<u8, DeviceError> {
        self.get(get::CONNECTION_SLOTS)?
            .first()
            .copied()
            .ok_or_else(|| {
                ProtocolError::BadPayload {
                    command: get::CONNECTION_SLOTS.response(),
                    reason: "payload vide",
                }
                .into()
            })
    }

    /// Multipoint. App : `001D:0E00 [0|1]`. Couper le Multipoint ne garde
    /// qu'un appareil connecté (dans l'app, le téléphone qui l'a coupé a été
    /// déconnecté).
    pub fn set_multipoint(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write_core(core_set::MULTIPOINT, vec![u8::from(on)])
    }

    pub fn spatial_audio(&mut self) -> Result<bool, DeviceError> {
        let p = self.get(get::SPATIAL_AUDIO)?;
        Ok(parse_switch(get::SPATIAL_AUDIO, &p)?)
    }

    /// Audio spatial. App : `3604 [0|1]`.
    pub fn set_spatial_audio(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::SPATIAL_AUDIO, vec![u8::from(on)])
    }

    /// Lit tout l'état affiché par l'interface.
    ///
    /// Batterie et contrôle du bruit doivent répondre ; un réglage refusé
    /// par le casque donne `None` sans faire échouer toute la lecture.
    pub fn read_state(&mut self) -> Result<HeadsetState, DeviceError> {
        let switch = |h: &mut Self, command: CommandId| -> Result<bool, DeviceError> {
            Ok(parse_switch(command, &h.get(command)?)?)
        };
        Ok(HeadsetState {
            battery: self.battery()?,
            noise: self.read_noise()?,
            spatial_audio: self.optional(|h| h.spatial_audio())?,
            head_tracking: self.optional(|h| h.head_tracking())?,
            audio_mode: self.optional(|h| h.audio_mode())?,
            sound_profile: self
                .optional(|h| h.get(get::SOUND_PERSONALIZATION))?
                .and_then(|p| p.first().map(|&b| SoundProfileStatus::from_byte(b))),
            battery_protection: self.optional(|h| switch(h, get::BATTERY_PROTECTION))?,
            touch_controls: self
                .optional(|h| switch(h, get::TOUCH_LOCK))?
                .map(|locked| !locked),
            smart_pause: self.optional(|h| switch(h, get::SMART_PAUSE))?,
            auto_answer: self.optional(|h| switch(h, get::AUTO_ANSWER))?,
            comfort_call: self.optional(|h| switch(h, get::COMFORT_CALL))?,
            auto_off_seconds: self.optional(|h| h.auto_off_seconds())?,
            prompt_mode: self
                .optional(|h| h.get(get::PROMPT_MODE))?
                .and_then(|p| p.first().map(|&b| PromptMode::from_byte(b))),
            codec: self
                .optional(|h| Ok(parse_codec(&h.get(get::CODEC)?)?))?
                .map(|c| c.0),
            // Pas de lecture connue : seulement par notification.
            sample_rate_hz: None,
            on_head_state: self.optional(|h| h.on_head_state())?,
        })
    }

    /// Identité et réglages en lecture seule ; chaque lecture refusée donne
    /// `None` (seule une erreur de transport fait échouer).
    pub fn read_device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        let first = |h: &mut Self, command: CommandId| -> Result<u8, DeviceError> {
            h.get(command)?.first().copied().ok_or_else(|| {
                ProtocolError::BadPayload {
                    command: command.response(),
                    reason: "payload vide",
                }
                .into()
            })
        };
        let name = self.optional(|h| Ok(parse_device_name(&h.get(get::DEVICE_NAME)?)))?;
        let model = self.optional(|h| Ok(parse_model_name(&h.get(get::MODEL_NAME)?)?))?;
        let firmware = self.optional(|h| {
            let (a, b, c) = parse_firmware_version(&h.get(get::FIRMWARE_VERSION)?)?;
            Ok(format!("{a}.{b}.{c}"))
        })?;
        let hardware_revision = self.optional(|h| match h.get(get::HARDWARE_REVISION)?[..] {
            [hi, lo, ..] => Ok(u16::from_be_bytes([hi, lo])),
            _ => Err(ProtocolError::BadPayload {
                command: get::HARDWARE_REVISION.response(),
                reason: "moins de 2 octets",
            }
            .into()),
        })?;
        let prompt_language = self.optional(|h| first(h, get::PROMPT_LANGUAGE))?;
        let prompt_volume = self
            .optional(|h| first(h, get::PROMPT_VOLUME))?
            .map(|v| v as i8);
        let hi_res = self.optional(|h| first(h, get::BT_COMPATIBILITY))?;
        let lossless = self.optional(|h| first(h, get::LOSSLESS))?;
        Ok(DeviceInfo {
            name,
            model,
            firmware,
            hardware_revision,
            prompt_language,
            prompt_volume,
            audio_priority: hi_res.zip(lossless),
            multipoint: self.optional(|h| h.connection_slots())?.map(|n| n >= 2),
        })
    }

    /// Appareils appairés au casque (lecture seule : `1400` puis `1401 [i]`).
    /// Au plus 8 appareils lus, garde-fou contre une réponse folle.
    pub fn read_paired_devices(&mut self) -> Result<Vec<PairedDevice>, DeviceError> {
        let count = self
            .get(get::PAIRED_LIST_SIZE)?
            .last()
            .copied()
            .unwrap_or(0)
            .min(8);
        let mut devices = Vec::new();
        for index in 0..count {
            let frame = self.client.request(
                &Frame::new(VENDOR_SENNHEISER, get::PAIRED_DEVICE_INFO, vec![index]),
                self.timeout,
            )?;
            devices.push(parse_paired_device(&frame.payload)?);
        }
        Ok(devices)
    }

    /// Appareils appairés avec leur état de connexion, comme « Gestion des
    /// connexions » dans l'app : `140D`, puis `140E [adresse]` pour chacun,
    /// et `1413` pour reconnaître ce PC. Au plus 8 appareils.
    pub fn read_connections(&mut self) -> Result<Vec<Connection>, DeviceError> {
        let own = self.optional(|h| {
            address(&h.get(get::OWN_ADDRESS)?).ok_or_else(|| {
                ProtocolError::BadPayload {
                    command: get::OWN_ADDRESS.response(),
                    reason: "moins de 6 octets",
                }
                .into()
            })
        })?;
        let list = self.get(get::CONNECTION_LIST)?;
        let count = usize::from(list.first().copied().unwrap_or(0)).min(8);
        let mut devices = Vec::new();
        let (addresses, _) = list.get(1..).unwrap_or(&[]).as_chunks::<6>();
        for chunk in addresses.iter().take(count) {
            let frame = self.client.request(
                &Frame::new(VENDOR_SENNHEISER, get::CONNECTION_INFO, chunk.to_vec()),
                self.timeout,
            )?;
            devices.push(parse_connection(&frame.payload, own)?);
        }
        Ok(devices)
    }

    /// Connecte (`140F`) ou déconnecte (`1410`) un appareil appairé. La
    /// déconnexion de ce PC est refusée : elle couperait le pilotage.
    pub fn set_connection(
        &mut self,
        device: &Connection,
        connect: bool,
    ) -> Result<(), DeviceError> {
        if !connect && device.this_device {
            return Err(DeviceError::OwnConnection);
        }
        let command = if connect {
            set::CONNECT_DEVICE
        } else {
            set::DISCONNECT_DEVICE
        };
        self.write(command, device.address.to_vec())
    }

    /// Extinction automatique en secondes (`0601 [00]` → `[00, u16]`).
    pub fn auto_off_seconds(&mut self) -> Result<u16, DeviceError> {
        let frame = self.client.request(
            &Frame::new(VENDOR_SENNHEISER, get::TIMER, vec![TIMER_AUTO_OFF]),
            self.timeout,
        )?;
        match frame.payload.as_slice() {
            [_, hi, lo, ..] => Ok(u16::from_be_bytes([*hi, *lo])),
            _ => Err(ProtocolError::BadPayload {
                command: get::TIMER.response(),
                reason: "moins de 3 octets",
            }
            .into()),
        }
    }

    /// Smart Pause. App : `080C [0|1]`.
    pub fn set_smart_pause(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::SMART_PAUSE, vec![u8::from(on)])
    }

    /// Réponse automatique aux appels. App : `080A [0|1]`.
    pub fn set_auto_answer(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::AUTO_ANSWER, vec![u8::from(on)])
    }

    /// Appels confortables. App : `0814 [0|1]`.
    pub fn set_comfort_call(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::COMFORT_CALL, vec![u8::from(on)])
    }

    /// Extinction automatique, limitée aux durées proposées par l'app.
    /// App : `0600 [00, secondes u16]`.
    pub fn set_auto_off(&mut self, seconds: u16) -> Result<(), DeviceError> {
        if !AUTO_OFF_CHOICES.contains(&seconds) {
            return Err(DeviceError::InvalidAutoOff(seconds));
        }
        let [hi, lo] = seconds.to_be_bytes();
        self.write(set::TIMER, vec![TIMER_AUTO_OFF, hi, lo])
    }

    /// Invites : seuls les deux modes capturés sont autorisés.
    /// App : `0801 [1]` (audio uniquement) / `0801 [2]` (audio et vocales).
    /// Volume des invites, borné à l'échelle de l'app. App : `081C [i8]`
    /// (notification `089D`).
    pub fn set_prompt_volume(&mut self, value: i8) -> Result<(), DeviceError> {
        if !(PROMPT_VOLUME_MIN..=PROMPT_VOLUME_MAX).contains(&value) {
            return Err(DeviceError::InvalidPromptVolume(value));
        }
        self.write(set::PROMPT_VOLUME, vec![value as u8])
    }

    /// Nom du casque, vérifié par [`check_name`]. App : `2801 [nom]`, sans
    /// notification. Windows et le téléphone peuvent garder l'ancien nom
    /// jusqu'à la prochaine connexion.
    pub fn set_name(&mut self, name: &str) -> Result<(), DeviceError> {
        check_name(name)?;
        self.write(set::DEVICE_NAME, name.as_bytes().to_vec())
    }

    pub fn set_prompt_mode(&mut self, mode: PromptMode) -> Result<(), DeviceError> {
        let byte = match mode {
            PromptMode::TonesOnly => 1,
            PromptMode::TonesAndVoice => 2,
            PromptMode::Unknown => return Err(DeviceError::InvalidPromptMode),
        };
        self.write(set::PROMPT_MODE, vec![byte])
    }

    /// Mode audio (`0804` → `[00, mode]`), valeurs inconnues conservées.
    pub fn audio_mode(&mut self) -> Result<AudioMode, DeviceError> {
        let p = self.get(get::AUDIO_MODE)?;
        let mode = *p.get(1).ok_or(ProtocolError::BadPayload {
            command: get::AUDIO_MODE.response(),
            reason: "moins de 2 octets",
        })?;
        Ok(AudioMode::from_byte(mode))
    }

    /// Lecture facultative : une commande refusée par le casque (trame
    /// d'erreur GAIA) ou une réponse illisible donne `None` au lieu de faire
    /// échouer toute la lecture. Les erreurs de transport, elles, remontent.
    fn optional<R>(
        &mut self,
        read: impl FnOnce(&mut Self) -> Result<R, DeviceError>,
    ) -> Result<Option<R>, DeviceError> {
        match read(self) {
            Ok(v) => Ok(Some(v)),
            Err(
                e @ (DeviceError::Client(ClientError::Device { .. }) | DeviceError::Protocol(_)),
            ) => {
                debug!(%e, "lecture facultative indisponible");
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// Lit le profil de personnalisation du son et son contexte, sans rien
    /// écrire. Les trois blocs sont obligatoires (pas d'export partiel) ;
    /// modèle, firmware, version de feature, état et mode sont facultatifs.
    pub fn read_sound_profile(&mut self, unix_time: u64) -> Result<ProfileExport, DeviceError> {
        let model = self.optional(|h| Ok(parse_model_name(&h.get(get::MODEL_NAME)?)?))?;
        let firmware = self.optional(|h| {
            let (a, b, c) = parse_firmware_version(&h.get(get::FIRMWARE_VERSION)?)?;
            Ok(format!("{a}.{b}.{c}"))
        })?;
        let feature_version = self
            .optional(|h| Ok(parse_core_features(&h.get(get::FEATURE_CATALOG)?)?))?
            .and_then(|(_, list)| {
                list.into_iter()
                    .find(|&(f, _)| f == feature::SOUND_PERSONALIZATION)
                    .map(|(_, v)| v)
            });
        let status_raw = self.optional(|h| h.get(get::SOUND_PERSONALIZATION))?;
        let audio_mode_raw = self.optional(|h| h.get(get::AUDIO_MODE))?;
        let mut blocks = Vec::new();
        for command in [
            get::SOUND_PROFILE_1,
            get::SOUND_PROFILE_2,
            get::SOUND_PROFILE_3,
        ] {
            blocks.push(ProfileBlock {
                command,
                payload: self.get(command)?,
            });
        }
        Ok(ProfileExport {
            unix_time,
            model,
            firmware,
            feature_version,
            status_raw,
            audio_mode_raw,
            blocks,
        })
    }

    /// Écrit le mode audio (`0803 [00, mode]`). Un refus du casque garde
    /// son payload et son sens propre à cette commande ; il ne devient
    /// jamais un succès, quel que soit le code.
    fn write_audio_mode(&mut self, mode: u8) -> Result<(), DeviceError> {
        match self.write(set::AUDIO_MODE, vec![0, mode]) {
            Err(DeviceError::Client(ClientError::Device { command, payload }))
                if command == set::AUDIO_MODE.error() =>
            {
                Err(DeviceError::AudioModeRefused(payload))
            }
            other => other,
        }
    }

    /// Speech Clarity (sinon retour à l'égaliseur graphique).
    /// App : `0803 [00, 02]` / `0803 [00, 01]`.
    pub fn set_speech_clarity(&mut self, on: bool) -> Result<(), DeviceError> {
        let mode = if on {
            AUDIO_MODE_SPEECH_CLARITY
        } else {
            AUDIO_MODE_EQUALIZER
        };
        self.write_audio_mode(mode)
    }

    /// Sélectionne le profil de personnalisation du son déjà enregistré,
    /// comme Smart Control Plus (capture HCI du 09/10/2026), sans jamais
    /// réécrire ses blocs :
    /// 1. refus si le casque n'a pas de profil (`2001` ≠ `[02]`), rien
    ///    n'est écrit ;
    /// 2. Dolby Atmos et profil s'excluent : si l'audio spatial est actif,
    ///    il est coupé d'abord (`3604 [00]`), comme dans l'app ;
    /// 3. garde : lecture fraîche du mode (`0804`). Si le profil est déjà
    ///    sélectionné et qu'aucune notification reçue pendant ou juste après
    ///    cette lecture ne la contredit, rien n'est écrit
    ///    ([`ProfileSelection::AlreadySelectedAtRead`]) : le casque refuse un
    ///    `0803` redondant (`0983 [80]`). Sinon (autre mode, valeur
    ///    inconnue, lecture contredite), `0803 [00 03]` part comme avant.
    ///
    /// Le cache de l'application n'intervient pas. Les notifications reçues
    /// restent dans le client pour le propriétaire de la connexion, y
    /// compris en cas d'erreur.
    pub fn select_sound_profile(&mut self) -> Result<ProfileSelection, DeviceError> {
        let status = self.get(get::SOUND_PERSONALIZATION)?;
        match status.as_slice() {
            [2] => {}
            [other] => return Err(DeviceError::NoSoundProfile(Some(*other))),
            _ => {
                return Err(ProtocolError::BadPayload {
                    command: get::SOUND_PERSONALIZATION.response(),
                    reason: "un octet d'état attendu",
                }
                .into());
            }
        }
        let spatial = self.get(get::SPATIAL_AUDIO)?;
        match spatial.as_slice() {
            [0] => {}
            [1] => self.set_spatial_audio(false)?,
            _ => {
                return Err(ProtocolError::BadPayload {
                    command: get::SPATIAL_AUDIO.response(),
                    reason: "un octet 0 ou 1 attendu",
                }
                .into());
            }
        }
        // Garde : seules comptent les notifications arrivées pendant ou
        // juste après la lecture ; les plus anciennes sont antérieures à elle.
        let earlier = self.client.pending_notifications().len();
        let mode = self.get(get::AUDIO_MODE)?;
        let contradicted = self.client.pending_notifications()[earlier..]
            .iter()
            .any(contradicts_profile_reading);
        match mode.as_slice() {
            [0, AUDIO_MODE_PERSONALIZATION] if !contradicted => {
                debug!("profil déjà sélectionné à la lecture : 0803 non envoyé");
                return Ok(ProfileSelection::AlreadySelectedAtRead);
            }
            [0, _] => {
                if contradicted {
                    debug!("lecture du mode contredite par une notification : écriture normale");
                }
            }
            _ => {
                return Err(ProtocolError::BadPayload {
                    command: get::AUDIO_MODE.response(),
                    reason: "[00, mode] attendu",
                }
                .into());
            }
        }
        self.write_audio_mode(AUDIO_MODE_PERSONALIZATION)?;
        Ok(ProfileSelection::WriteAcknowledged)
    }

    /// Protection de la batterie. App : `060A [0|1]`.
    pub fn set_battery_protection(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::BATTERY_PROTECTION, vec![u8::from(on)])
    }

    /// Commandes tactiles. App : `1606 [01]` pour les désactiver.
    pub fn set_touch_controls(&mut self, enabled: bool) -> Result<(), DeviceError> {
        self.write(set::TOUCH_LOCK, vec![u8::from(!enabled)])
    }

    /// Règle le gain d'une bande (dixièmes de dB). App : `1001 [bande, gain]`.
    ///
    /// `eq` sert à vérifier la bande et les limites annoncées par le casque ;
    /// il est mis à jour quand le casque acquitte l'écriture, pour qu'une
    /// opération suivante ne se compare pas à un état périmé.
    pub fn set_eq_band(&mut self, eq: &mut EqState, band: u8, gain: i8) -> Result<(), DeviceError> {
        let count = eq.gains.len();
        if usize::from(band) >= count {
            return Err(DeviceError::InvalidBand { band, count });
        }
        eq.check_gain(gain)?;
        // Le gain est un i8 transmis tel quel (complément à deux).
        self.write(set::EQ_BAND, vec![band, gain as u8])?;
        eq.gains[usize::from(band)] = gain;
        Ok(())
    }

    /// Applique une courbe complète, bande par bande comme l'app, en
    /// n'écrivant que les bandes qui changent.
    ///
    /// Toute la courbe est vérifiée avant la première écriture. Une erreur
    /// du casque en cours de route laisse en revanche les premières bandes
    /// modifiées (le protocole n'a pas de transaction) : relire l'égaliseur.
    ///
    /// `eq` doit décrire l'état actuel du casque (relu, ou tenu à jour par
    /// les écritures précédentes) : les bandes égales à `eq` ne sont pas
    /// envoyées. Il suit chaque bande acquittée.
    pub fn set_eq_gains(&mut self, eq: &mut EqState, gains: &[i8]) -> Result<(), DeviceError> {
        if gains.len() != eq.gains.len() {
            return Err(DeviceError::InvalidBand {
                band: u8::try_from(gains.len()).unwrap_or(u8::MAX),
                count: eq.gains.len(),
            });
        }
        for &gain in gains {
            eq.check_gain(gain)?;
        }
        for (band, &new) in gains.iter().enumerate() {
            if new != eq.gains[band] {
                // Au plus 16 bandes (MAX_EQ_BANDS) : la conversion ne peut pas échouer.
                let band = u8::try_from(band).unwrap_or(u8::MAX);
                self.set_eq_band(eq, band, new)?;
            }
        }
        Ok(())
    }

    /// Amplification des basses. App : `1008 [0|1]`.
    pub fn set_bass_boost(&mut self, on: bool) -> Result<(), DeviceError> {
        self.write(set::BASS_BOOST, vec![u8::from(on)])
    }

    /// Lit l'égaliseur : configuration (`1000`), gain de chaque bande
    /// (`1002 [bande]`) et Bass Boost (`1009`).
    pub fn read_eq(&mut self) -> Result<EqState, DeviceError> {
        let config = parse_eq_config(&self.get(get::EQ_CONFIG)?)?;
        let mut gains = Vec::new();
        for band in 0..config.bands.min(MAX_EQ_BANDS) {
            let frame = self.client.request(
                &Frame::new(VENDOR_SENNHEISER, get::EQ_BAND, vec![band]),
                self.timeout,
            )?;
            gains.push(parse_eq_gain(&frame.payload)?);
        }
        let bass_boost = parse_switch(get::BASS_BOOST, &self.get(get::BASS_BOOST)?)?;
        Ok(EqState {
            min_gain: config.min_gain,
            max_gain: config.max_gain,
            gains,
            bass_boost,
        })
    }

    /// Lit l'état physique du port, sans modifier l'activation du capteur.
    pub fn on_head_state(&mut self) -> Result<OnHeadState, DeviceError> {
        let state = parse_on_head_state(&self.get(get::ON_HEAD_STATE)?)?;
        // La réponse remplace les états de port déjà reçus ; les notifications
        // suivantes et celles des autres fonctions restent disponibles.
        self.client
            .forget_queued_notification(VENDOR_SENNHEISER, get::ON_HEAD_STATE.notification());
        Ok(state)
    }

    /// S'abonne aux notifications des features utiles, comme l'app.
    pub fn register_notifications(&mut self) -> Result<(), DeviceError> {
        // L'abonnement ne modifie aucun réglage : permis en lecture seule.
        for feature in NOTIFICATION_FEATURES {
            self.ack(REGISTER_NOTIFICATION, vec![feature])?;
        }
        // Capture du 27/10/2026 : 0007 [02] / 0107 vide, records 40357/40360.
        // Facultatif sur les autres micrologiciels ; une perte de transport
        // reste une erreur et impose une nouvelle connexion.
        self.optional(|h| h.ack(REGISTER_NOTIFICATION, vec![feature::ON_HEAD]))?;
        Ok(())
    }

    /// Attend au plus `timeout` des notifications du casque.
    pub fn poll_notifications(&mut self, timeout: Duration) -> Result<Vec<Frame>, DeviceError> {
        Ok(self.client.poll(timeout)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::from_hex;
    use crate::transport::MockTransport;

    const T: Duration = Duration::from_millis(50);

    fn h(s: &str) -> Vec<u8> {
        from_hex(s).unwrap()
    }

    #[test]
    fn adaptive_sends_the_app_sequence() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 01 04 95 1A 04 01"),
                &h("FF 03 00 00 04 95 1B 04"),
            )
            .expect(
                &h("FF 03 00 02 04 95 1A 00 03 01"),
                &h("FF 03 00 00 04 95 1B 00"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_mode(NoiseModeRequest::Adaptive).unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn custom_sends_the_app_sequence() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 1A 00 03 00"),
                &h("FF 03 00 00 04 95 1B 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 1A 04 01"),
                &h("FF 03 00 00 04 95 1B 04"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_mode(NoiseModeRequest::Custom).unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn level_is_bounded_and_encoded() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 01 04 95 1A 02 32"),
            &h("FF 03 00 00 04 95 1B 02"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_level(50).unwrap();
        assert!(matches!(
            headset.set_level(101),
            Err(DeviceError::InvalidLevel(101))
        ));
    }

    #[test]
    fn anti_wind_and_spatial_frames() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 1A 00 01 00"),
                &h("FF 03 00 00 04 95 1B 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 36 04 01"),
                &h("FF 03 00 00 04 95 37 04"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_anti_wind(false).unwrap();
        headset.set_spatial_audio(true).unwrap();
    }

    #[test]
    fn read_noise_from_m5_answers() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 1A 05"),
                &h("FF 03 00 01 04 95 1B 05 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 1A 01"),
                &h("FF 03 00 06 04 95 1B 01 01 01 03 00 02 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 1A 03"),
                &h("FF 03 00 01 04 95 1B 03 32"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let state = headset.read_noise().unwrap();
        assert_eq!(
            state,
            NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 50
            }
        );
        assert_eq!(state.mode(), M5NoiseMode::Custom { level: 50 });
    }

    fn notif(hex: &str) -> Frame {
        Frame::decode(&h(hex)).unwrap().0
    }

    #[test]
    fn notifications_from_the_app_capture_update_the_state() {
        let mut state = HeadsetState {
            battery: 100,
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 0,
            },
            spatial_audio: Some(true),
            ..HeadsetState::default()
        };
        // Curseur à 50 % (le casque envoie FF 04 à l'app ; même contenu).
        assert!(state.apply_notification(&notif("FF 03 00 01 04 95 1A 83 32")));
        assert_eq!(state.noise.level, 50);
        // Adaptatif : paires (1,1) (3,1) (2,0).
        assert!(state.apply_notification(&notif("FF 03 00 06 04 95 1A 81 01 01 03 01 02 00")));
        assert!(state.noise.adaptive);
        // Audio spatial coupé.
        assert!(state.apply_notification(&notif("FF 03 00 01 04 95 36 85 00")));
        assert_eq!(state.spatial_audio, Some(false));
        // Batterie, puis la même valeur : pas de changement.
        assert!(state.apply_notification(&notif("FF 03 00 01 04 95 06 83 63")));
        assert!(!state.apply_notification(&notif("FF 03 00 01 04 95 06 83 63")));
        assert_eq!(state.battery, 99);
        // Une réponse (pas une notification) est ignorée.
        assert!(!state.apply_notification(&notif("FF 03 00 01 04 95 1B 03 64")));
    }

    #[test]
    fn extra_settings_match_the_app_capture() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 08 03 00 02"),
                &h("FF 03 00 00 04 95 09 03"),
            )
            .expect(
                &h("FF 03 00 02 04 95 08 03 00 01"),
                &h("FF 03 00 00 04 95 09 03"),
            )
            .expect(
                &h("FF 03 00 01 04 95 06 0A 01"),
                &h("FF 03 00 00 04 95 07 0A"),
            )
            .expect(
                &h("FF 03 00 01 04 95 16 06 01"),
                &h("FF 03 00 00 04 95 17 06"),
            )
            .expect(
                &h("FF 03 00 01 04 95 16 06 00"),
                &h("FF 03 00 00 04 95 17 06"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_speech_clarity(true).unwrap();
        headset.set_speech_clarity(false).unwrap();
        headset.set_battery_protection(true).unwrap();
        headset.set_touch_controls(false).unwrap();
        headset.set_touch_controls(true).unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn device_settings_match_the_app_capture() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 01 04 95 08 0C 00"),
                &h("FF 03 00 00 04 95 09 0C"),
            )
            .expect(
                &h("FF 03 00 01 04 95 08 0A 01"),
                &h("FF 03 00 00 04 95 09 0A"),
            )
            .expect(
                &h("FF 03 00 01 04 95 08 14 00"),
                &h("FF 03 00 00 04 95 09 14"),
            )
            .expect(
                &h("FF 03 00 03 04 95 06 00 00 07 08"),
                &h("FF 03 00 00 04 95 07 00"),
            )
            .expect(
                &h("FF 03 00 03 04 95 06 00 00 00 00"),
                &h("FF 03 00 00 04 95 07 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 08 01 01"),
                &h("FF 03 00 00 04 95 09 01"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_smart_pause(false).unwrap();
        headset.set_auto_answer(true).unwrap();
        headset.set_comfort_call(false).unwrap();
        headset.set_auto_off(1800).unwrap();
        headset.set_auto_off(0).unwrap();
        headset.set_prompt_mode(PromptMode::TonesOnly).unwrap();
        assert!(headset.client().transport().is_exhausted());
        // Valeurs hors de ce que l'app propose : refusées sans rien envoyer.
        assert!(matches!(
            headset.set_auto_off(42),
            Err(DeviceError::InvalidAutoOff(42))
        ));
        assert!(headset.set_prompt_mode(PromptMode::Unknown).is_err());
        assert_eq!(headset.client().transport().sent().len(), 6);
    }

    #[test]
    fn extra_notifications_update_the_state() {
        let mut state = HeadsetState {
            touch_controls: Some(true),
            ..HeadsetState::default()
        };
        assert!(state.apply_notification(&notif("FF 03 00 02 04 95 08 84 00 02")));
        assert_eq!(state.audio_mode, Some(AudioMode::SpeechClarity));
        // Suivi de tête : notification 3683 (essai E4.2).
        assert!(state.apply_notification(&notif("FF 04 00 01 04 95 36 83 00")));
        assert_eq!(state.head_tracking, Some(false));
        // Parcours de personnalisation (capture Android) : mode 3, puis
        // profil reçu (2081 [02]), puis retour à l'égaliseur (0884 [00 01]).
        assert!(state.apply_notification(&notif("FF 04 00 02 04 95 08 84 00 03")));
        assert_eq!(state.audio_mode, Some(AudioMode::Personalization));
        assert!(state.apply_notification(&notif("FF 04 00 01 04 95 20 81 02")));
        assert!(state.apply_notification(&notif("FF 04 00 02 04 95 08 84 00 01")));
        // L'état du profil ne bouge pas avec le mode : deux informations.
        assert_eq!(state.sound_profile, Some(SoundProfileStatus::Received));
        assert_eq!(state.audio_mode, Some(AudioMode::Equalizer));
        // Valeur jamais vue : conservée telle quelle, pas rabattue sur 1.
        assert!(state.apply_notification(&notif("FF 03 00 02 04 95 08 84 00 07")));
        assert_eq!(state.audio_mode, Some(AudioMode::Unknown(7)));
        assert_eq!(state.audio_mode.map(AudioMode::raw), Some(7));
        // Curseur hors bornes : l'état valide est conservé.
        let before = state;
        assert!(!state.apply_notification(&notif("FF 03 00 01 04 95 1A 83 C8")));
        assert_eq!(state, before);
        assert!(state.apply_notification(&notif("FF 03 00 01 04 95 16 87 01")));
        assert_eq!(state.touch_controls, Some(false));
    }

    #[test]
    fn a_refused_setting_does_not_break_the_snapshot() {
        // Audio spatial refusé (trame d'erreur 3785) : tout le reste est lu.
        let script = [
            ("FF 03 00 00 04 95 06 03", "FF 03 00 01 04 95 07 03 50"),
            ("FF 03 00 00 04 95 1A 05", "FF 03 00 01 04 95 1B 05 01"),
            (
                "FF 03 00 00 04 95 1A 01",
                "FF 03 00 04 04 95 1B 01 01 00 03 01",
            ),
            ("FF 03 00 00 04 95 1A 03", "FF 03 00 01 04 95 1B 03 32"),
            ("FF 03 00 00 04 95 36 05", "FF 03 00 01 04 95 37 85 01"),
            ("FF 03 00 00 04 95 36 03", "FF 03 00 01 04 95 37 03 01"),
            ("FF 03 00 00 04 95 08 04", "FF 03 00 02 04 95 09 04 00 01"),
            ("FF 03 00 00 04 95 20 01", "FF 03 00 01 04 95 21 01 02"),
            ("FF 03 00 00 04 95 06 0B", "FF 03 00 01 04 95 07 0B 00"),
            ("FF 03 00 00 04 95 16 07", "FF 03 00 01 04 95 17 07 00"),
            ("FF 03 00 00 04 95 08 0D", "FF 03 00 01 04 95 09 0D 01"),
            ("FF 03 00 00 04 95 08 0B", "FF 03 00 01 04 95 09 0B 00"),
            ("FF 03 00 00 04 95 08 15", "FF 03 00 01 04 95 09 15 01"),
            (
                "FF 03 00 01 04 95 06 01 00",
                "FF 03 00 03 04 95 07 01 00 07 08",
            ),
            ("FF 03 00 00 04 95 08 02", "FF 03 00 01 04 95 09 02 02"),
            ("FF 03 00 00 04 95 08 00", "FF 03 00 01 04 95 09 00 02"),
        ];
        for (port_reply, expected_port) in [
            ("FF 03 00 01 04 95 05 02 02", Some(OnHeadState::NotWorn)),
            ("FF 03 00 01 04 95 05 82 01", None),
            ("FF 03 00 00 04 95 05 02", None),
        ] {
            let mut mock = MockTransport::new();
            for (tx, rx) in script {
                mock = mock.expect(&h(tx), &h(rx));
            }
            mock = mock.expect(&h("FF 03 00 00 04 95 04 02"), &h(port_reply));
            let mut headset = Headset::validated_for_tests(mock, T);
            let state = headset.read_state().unwrap();
            assert!(headset.client().transport().is_exhausted());
            assert_eq!(state.battery, 80);
            assert_eq!(state.spatial_audio, None);
            assert_eq!(state.head_tracking, Some(true));
            assert_eq!(state.audio_mode, Some(AudioMode::Equalizer));
            assert_eq!(state.touch_controls, Some(true));
            assert_eq!(state.auto_off_seconds, Some(1800));
            assert_eq!(state.prompt_mode, Some(PromptMode::TonesAndVoice));
            assert_eq!(state.codec, Some(2));
            assert_eq!(state.on_head_state, expected_port);
        }
    }

    #[test]
    fn prompt_volume_scale_matches_the_app() {
        // Relevé du 09/10/2026 : % du curseur de l'app → valeur relue.
        for (percent, value) in [
            (0, -20),
            (10, -17),
            (20, -15),
            (30, -12),
            (40, -10),
            (50, -7),
            (57, -5),
            (62, -4),
            (67, -3),
            (72, -1),
            (77, 0),
            (82, 1),
            (87, 3),
            (92, 4),
            (99, 6),
        ] {
            assert_eq!(prompt_volume_from_percent(percent), value, "{percent} %");
        }
        assert_eq!(prompt_volume_from_percent(100), 6);
        assert_eq!(prompt_volume_percent(0), 77);
        assert_eq!(prompt_volume_percent(-20), 0);
        assert_eq!(prompt_volume_percent(6), 100);
    }

    #[test]
    fn head_tracking_and_multipoint_follow_the_captures() {
        let mock = MockTransport::new()
            // Audio spatial coupé : le suivi de tête est refusé sans envoi.
            .expect(
                &h("FF 03 00 00 04 95 36 05"),
                &h("FF 03 00 01 04 95 37 05 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 36 05"),
                &h("FF 03 00 01 04 95 37 05 01"),
            )
            .expect(
                &h("FF 03 00 01 04 95 36 02 00"),
                &h("FF 03 00 00 04 95 37 02"),
            )
            .expect(
                &h("FF 03 00 00 04 95 36 03"),
                &h("FF 03 00 01 04 95 37 03 00"),
            )
            .expect(
                &h("FF 03 00 01 00 1D 0E 00 01"),
                &h("FF 03 00 00 00 1D 0F 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 14 09"),
                &h("FF 03 00 01 04 95 15 09 02"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(matches!(
            headset.set_head_tracking(false),
            Err(DeviceError::SpatialAudioOff)
        ));
        headset.set_head_tracking(false).unwrap();
        assert!(!headset.head_tracking().unwrap());
        headset.set_multipoint(true).unwrap();
        assert_eq!(headset.connection_slots().unwrap(), 2);
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn connections_follow_the_capture() {
        // Adresses fictives ; structure de la capture F6.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 14 13"),
                &h("FF 03 00 06 04 95 15 13 11 11 11 11 11 11"),
            )
            .expect(
                &h("FF 03 00 00 04 95 14 0D"),
                &h("FF 03 00 0D 04 95 15 0D 02 11 11 11 11 11 11 22 22 22 22 22 22"),
            )
            .expect(
                &h("FF 03 00 06 04 95 14 0E 11 11 11 11 11 11"),
                &h("FF 03 00 0D 04 95 15 0E 11 11 11 11 11 11 03 00 00 03 50 43 00"),
            )
            .expect(
                &h("FF 03 00 06 04 95 14 0E 22 22 22 22 22 22"),
                &h("FF 03 00 0E 04 95 15 0E 22 22 22 22 22 22 01 00 00 00 54 56 32 00"),
            )
            .expect(
                &h("FF 03 00 06 04 95 14 0F 22 22 22 22 22 22"),
                &h("FF 03 00 00 04 95 15 0F"),
            )
            .expect(
                &h("FF 03 00 06 04 95 14 10 22 22 22 22 22 22"),
                &h("FF 03 00 00 04 95 15 10"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let list = headset.read_connections().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(
            (
                list[0].name.as_str(),
                list[0].connected,
                list[0].this_device
            ),
            ("PC", true, true)
        );
        assert_eq!(
            (
                list[1].name.as_str(),
                list[1].connected,
                list[1].this_device
            ),
            ("TV2", false, false)
        );
        // Jamais d'adresse dans le journal.
        assert!(!format!("{:?}", list[1]).contains("22"));
        headset.set_connection(&list[1], true).unwrap();
        headset.set_connection(&list[1], false).unwrap();
        assert!(matches!(
            headset.set_connection(&list[0], false),
            Err(DeviceError::OwnConnection)
        ));
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn name_write_matches_the_capture() {
        // Capture F3 : « M5 Essai » → FF 03 00 08 04 95 28 01 4D 35 20 45 73 73 61 69.
        let mock = MockTransport::new().expect(
            &h("FF 03 00 08 04 95 28 01 4D 35 20 45 73 73 61 69"),
            &h("FF 03 00 00 04 95 29 01"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_name("M5 Essai").unwrap();
        for bad in [
            "",
            "   ",
            " M5",
            "M5 ",
            "Écouteurs",
            "a\tb",
            &"x".repeat(33),
        ] {
            assert!(
                matches!(headset.set_name(bad), Err(DeviceError::InvalidName(_))),
                "{bad:?}"
            );
        }
        assert_eq!(headset.client().transport().sent().len(), 1);
        assert!(check_name(&"x".repeat(32)).is_ok());
    }

    #[test]
    fn device_name_ignores_trailing_zeros() {
        assert_eq!(
            parse_device_name(b"MOMENTUM 5 Exemple\0"),
            "MOMENTUM 5 Exemple"
        );
        assert_eq!(parse_device_name(b"M5 Essai"), "M5 Essai");
        assert_eq!(parse_device_name(b""), "");
    }

    #[test]
    fn prompt_volume_write_is_bounded() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 01 04 95 08 1C FB"),
            &h("FF 03 00 00 04 95 09 1C"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.set_prompt_volume(-5).unwrap();
        assert!(matches!(
            headset.set_prompt_volume(7),
            Err(DeviceError::InvalidPromptVolume(7))
        ));
        assert!(headset.set_prompt_volume(-21).is_err());
        assert_eq!(headset.client().transport().sent().len(), 1);
    }

    #[test]
    fn device_info_is_read_only_and_partial() {
        // Révision refusée par le casque : le reste est lu.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 28 02"),
                &h("FF 03 00 08 04 95 29 02 4D 35 20 45 73 73 61 69"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 02 04 95 13 06 4D 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 06 04 95 13 01 00 06 00 15 00 05"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 00"),
                &h("FF 03 00 01 04 95 13 80 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 07"),
                &h("FF 03 00 01 04 95 09 07 02"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 1D"),
                &h("FF 03 00 01 04 95 09 1D FB"),
            )
            .expect(
                &h("FF 03 00 00 04 95 04 06"),
                &h("FF 03 00 01 04 95 05 06 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 04 08"),
                &h("FF 03 00 01 04 95 05 08 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 14 09"),
                &h("FF 03 00 01 04 95 15 09 02"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let mut info = headset.read_device_info().unwrap();
        assert!(headset.client().transport().is_exhausted());
        assert_eq!(info.name.as_deref(), Some("M5 Essai"));
        assert_eq!(info.firmware.as_deref(), Some("6.21.5"));
        assert_eq!(info.hardware_revision, None);
        assert_eq!(info.prompt_language, Some(2));
        assert_eq!(info.prompt_volume, Some(-5));
        assert_eq!(info.audio_priority, Some((1, 0)));
        // Notifications relevées : volume des invites 089D, priorité 0488.
        assert!(info.apply_notification(&notif("FF 04 00 01 04 95 08 9D 00")));
        assert!(info.apply_notification(&notif("FF 04 00 01 04 95 04 88 01")));
        assert_eq!(
            (info.prompt_volume, info.audio_priority),
            (Some(0), Some((1, 1)))
        );
        // Multipoint : 1409 = 2, puis notification du core Qualcomm 0E80 [00].
        assert_eq!(info.multipoint, Some(true));
        assert!(info.apply_notification(&notif("FF 04 00 01 00 1D 0E 80 00")));
        assert_eq!(info.multipoint, Some(false));
    }

    #[test]
    fn paired_devices_and_stream_notifications() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 14 00"),
                &h("FF 03 00 02 04 95 15 00 00 02"),
            )
            .expect(
                &h("FF 03 00 01 04 95 14 01 00"),
                &h("FF 03 00 05 04 95 15 01 00 01 01 41 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 14 01 01"),
                &h("FF 03 00 05 04 95 15 01 01 02 00 42 00"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let devices = headset.read_paired_devices().unwrap();
        assert_eq!(
            devices,
            vec![
                PairedDevice {
                    index: 0,
                    name: "A".into(),
                    connected: true
                },
                PairedDevice {
                    index: 1,
                    name: "B".into(),
                    connected: false
                },
            ]
        );
        // Notifications relevées : codec 0880 et fréquence 089A (44,1 kHz).
        let mut state = HeadsetState::default();
        assert!(state.apply_notification(&notif("FF 04 00 01 04 95 08 80 02")));
        assert!(state.apply_notification(&notif("FF 04 00 04 04 95 08 9A 00 00 AC 44")));
        assert_eq!((state.codec, state.sample_rate_hz), (Some(2), Some(44_100)));
    }

    #[test]
    fn writes_are_refused_on_an_unvalidated_model() {
        // « MOMENTUM 4 » (nom fictif de test) : lecture et abonnement
        // permis, aucun réglage envoyé.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 05 04 95 13 06 4D 34 41 45 00"),
            )
            .expect(
                &h("FF 03 00 01 04 95 00 07 03"),
                &h("FF 03 00 00 04 95 01 07"),
            );
        let mut headset = Headset::new(mock, T);
        assert_eq!(headset.identify(), "M4AE");
        assert!(!headset.writes_allowed());
        assert!(matches!(
            headset.set_spatial_audio(true),
            Err(DeviceError::WritesBlocked(m)) if m == "modèle « M4AE » non validé"
        ));
        headset.ack(REGISTER_NOTIFICATION, vec![3]).unwrap();
        assert_eq!(headset.client().transport().sent().len(), 2);
    }

    #[test]
    fn the_validated_model_can_write() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 0D 04 95 13 06 4D 35 41 45 42 54 20 42 6C 61 63 6B 00"),
            )
            // Micrologiciel 6.21.5.
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 06 04 95 13 01 00 06 00 15 00 05"),
            )
            .expect(
                &h("FF 03 00 01 04 95 36 04 01"),
                &h("FF 03 00 00 04 95 37 04"),
            );
        let mut headset = Headset::new(mock, T);
        assert_eq!(headset.identify(), "M5AEBT Black");
        headset.set_spatial_audio(true).unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn another_firmware_of_the_validated_model_is_read_only() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 0D 04 95 13 06 4D 35 41 45 42 54 20 42 6C 61 63 6B 00"),
            )
            // Micrologiciel 6.22.0.
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 06 04 95 13 01 00 06 00 16 00 00"),
            );
        let mut headset = Headset::new(mock, T);
        headset.identify();
        assert_eq!(
            headset.write_block_reason(),
            Some("micrologiciel 6.22.0 non validé")
        );
        assert!(matches!(
            headset.set_spatial_audio(true),
            Err(DeviceError::WritesBlocked(_))
        ));
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn writes_are_refused_before_identification() {
        // Sans identify : aucune trame de réglage, quelle que soit la commande.
        let mut headset = Headset::new(MockTransport::new(), T);
        assert!(!headset.writes_allowed());
        assert!(matches!(
            headset.set_spatial_audio(true),
            Err(DeviceError::WritesBlocked(r)) if r == NOT_IDENTIFIED
        ));
        assert!(headset.client().transport().sent().is_empty());
    }

    #[test]
    fn write_block_names_the_reason() {
        assert_eq!(write_block("M5AEBT Black", Some(VALIDATED_FIRMWARE)), None);
        assert_eq!(
            write_block("M4AE", Some(VALIDATED_FIRMWARE)).as_deref(),
            Some("modèle « M4AE » non validé")
        );
        assert_eq!(
            write_block("M5AEBT White", None).as_deref(),
            Some("micrologiciel illisible")
        );
    }

    #[test]
    fn an_unreadable_model_is_not_validated() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 12 06"),
            &h("FF 03 00 01 04 95 13 86 01"),
        );
        let mut headset = Headset::new(mock, T);
        assert_eq!(headset.identify(), "inconnu");
        assert!(!headset.writes_allowed());
    }

    #[test]
    fn profile_selection_matches_the_app_capture() {
        // Dolby Atmos actif : l'app envoie 3604 [00] puis 0803 [00 03]. Le
        // PC lit en plus le mode (0804, garde) : égaliseur, donc il écrit.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 20 01"),
                &h("FF 03 00 01 04 95 21 01 02"),
            )
            .expect(
                &h("FF 03 00 00 04 95 36 05"),
                &h("FF 03 00 01 04 95 37 05 01"),
            )
            .expect(
                &h("FF 03 00 01 04 95 36 04 00"),
                &h("FF 03 00 00 04 95 37 04"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 04"),
                &h("FF 03 00 02 04 95 09 04 00 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 08 03 00 03"),
                &h("FF 03 00 00 04 95 09 03"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert_eq!(
            headset.select_sound_profile().unwrap(),
            ProfileSelection::WriteAcknowledged
        );
        assert!(headset.client().transport().is_exhausted());

        // Dolby Atmos coupé : 0803 [00 03] seul.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 20 01"),
                &h("FF 03 00 01 04 95 21 01 02"),
            )
            .expect(
                &h("FF 03 00 00 04 95 36 05"),
                &h("FF 03 00 01 04 95 37 05 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 04"),
                &h("FF 03 00 02 04 95 09 04 00 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 08 03 00 03"),
                &h("FF 03 00 00 04 95 09 03"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert_eq!(
            headset.select_sound_profile().unwrap(),
            ProfileSelection::WriteAcknowledged
        );
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn no_profile_means_nothing_is_written() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 20 01"),
            &h("FF 03 00 01 04 95 21 01 00"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(matches!(
            headset.select_sound_profile(),
            Err(DeviceError::NoSoundProfile(Some(0)))
        ));
        assert_eq!(headset.client().transport().sent().len(), 1);
    }

    #[test]
    fn a_lost_link_still_fails_the_snapshot() {
        // Pas de réponse à l'audio spatial : ce n'est pas un refus, la
        // lecture échoue (et la session est désynchronisée).
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 06 03"),
                &h("FF 03 00 01 04 95 07 03 50"),
            )
            .expect(
                &h("FF 03 00 00 04 95 1A 05"),
                &h("FF 03 00 01 04 95 1B 05 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 1A 01"),
                &h("FF 03 00 04 04 95 1B 01 01 00 03 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 1A 03"),
                &h("FF 03 00 01 04 95 1B 03 32"),
            )
            .expect_chunked(&h("FF 03 00 00 04 95 36 05"), &[]);
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(headset.read_state().is_err());
        assert!(headset.client().is_desynchronized());
    }

    #[test]
    fn registration_uses_the_sennheiser_vendor() {
        let mut mock = MockTransport::new();
        for f in ["03", "0D", "1B", "08", "04", "0B", "02"] {
            mock = mock.expect(
                &h(&format!("FF 03 00 01 04 95 00 07 {f}")),
                &h("FF 03 00 00 04 95 01 07"),
            );
        }
        let mut headset = Headset::validated_for_tests(mock, T);
        headset.register_notifications().unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn read_eq_from_m5_answers() {
        let mut mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 10 00"),
            &h("FF 03 00 05 04 95 11 00 03 C4 3C 00 00"),
        );
        for (band, gain) in [("00", "DD"), ("01", "F6"), ("02", "0A")] {
            mock = mock.expect(
                &h(&format!("FF 03 00 01 04 95 10 02 {band}")),
                &h(&format!("FF 03 00 01 04 95 11 02 {gain}")),
            );
        }
        mock = mock.expect(
            &h("FF 03 00 00 04 95 10 09"),
            &h("FF 03 00 01 04 95 11 09 00"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        let eq = headset.read_eq().unwrap();
        assert_eq!(
            eq,
            EqState {
                min_gain: -60,
                max_gain: 60,
                gains: vec![-35, -10, 10],
                bass_boost: false
            }
        );
    }

    fn audiophile() -> EqState {
        EqState {
            min_gain: -60,
            max_gain: 60,
            gains: vec![-35, -10, 10, 6, -10, -5, -5, 30],
            bass_boost: false,
        }
    }

    #[test]
    fn eq_writes_match_the_app_capture() {
        // Trames capturées : « Neutre » bande 0, puis « Audiophile » bande 0
        // (-3,5 dB = DD) et bande 7 (+3 dB = 1E), Bass Boost on.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 10 01 00 00"),
                &h("FF 03 00 00 04 95 11 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 10 01 00 DD"),
                &h("FF 03 00 00 04 95 11 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 10 01 07 1E"),
                &h("FF 03 00 00 04 95 11 01"),
            )
            .expect(
                &h("FF 03 00 01 04 95 10 08 01"),
                &h("FF 03 00 00 04 95 11 08"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let mut eq = audiophile();
        headset.set_eq_band(&mut eq, 0, 0).unwrap();
        headset.set_eq_band(&mut eq, 0, -35).unwrap();
        headset.set_eq_band(&mut eq, 7, 30).unwrap();
        headset.set_bass_boost(true).unwrap();
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn eq_curve_writes_only_changed_bands() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 10 01 00 00"),
                &h("FF 03 00 00 04 95 11 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 10 01 07 00"),
                &h("FF 03 00 00 04 95 11 01"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let target = [0, -10, 10, 6, -10, -5, -5, 0];
        let mut eq = audiophile();
        headset.set_eq_gains(&mut eq, &target).unwrap();
        assert!(headset.client().transport().is_exhausted());
        // Le cache suit les bandes acquittées.
        assert_eq!(eq.gains, target);
    }

    #[test]
    fn eq_cache_keeps_the_old_gain_of_a_refused_band() {
        // Bande 0 acquittée, bande 7 refusée (1181) : le cache ne retient
        // que ce que le casque a accepté.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 02 04 95 10 01 00 00"),
                &h("FF 03 00 00 04 95 11 01"),
            )
            .expect(
                &h("FF 03 00 02 04 95 10 01 07 00"),
                &h("FF 03 00 01 04 95 11 81 01"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let mut eq = audiophile();
        assert!(
            headset
                .set_eq_gains(&mut eq, &[0, -10, 10, 6, -10, -5, -5, 0])
                .is_err()
        );
        assert_eq!(eq.gains, [0, -10, 10, 6, -10, -5, -5, 30]);
    }

    #[test]
    fn eq_bounds_are_enforced_before_sending() {
        let mut headset = Headset::validated_for_tests(MockTransport::new(), T);
        let mut eq = audiophile();
        assert!(matches!(
            headset.set_eq_band(&mut eq, 8, 0),
            Err(DeviceError::InvalidBand { band: 8, count: 8 })
        ));
        assert!(matches!(
            headset.set_eq_band(&mut eq, 0, 61),
            Err(DeviceError::InvalidGain { gain: 61, .. })
        ));
        assert!(headset.set_eq_gains(&mut eq, &[0; 5]).is_err());
        // Rien n'a été envoyé.
        assert!(headset.client().transport().sent().is_empty());
    }

    #[test]
    fn eq_notifications_from_the_app_capture() {
        let mut eq = EqState {
            min_gain: -60,
            max_gain: 60,
            gains: vec![0; 8],
            bass_boost: false,
        };
        assert!(eq.apply_notification(&notif("FF 03 00 08 04 95 10 82 DD F6 0A 06 F6 FB FB 1E")));
        assert_eq!(eq.gains, vec![-35, -10, 10, 6, -10, -5, -5, 30]);
        assert!(eq.apply_notification(&notif("FF 03 00 01 04 95 10 89 01")));
        assert!(eq.bass_boost);
        // Mauvaise longueur : ignorée.
        assert!(!eq.apply_notification(&notif("FF 03 00 02 04 95 10 82 00 00")));
    }

    #[test]
    fn refused_optional_read_does_not_break_the_snapshot() {
        // Lecture du profil : le casque refuse le catalogue (trame d'erreur)
        // mais rend les trois blocs ; l'export reste complet et brut.
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 02 04 95 13 06 4D 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 06 04 95 13 01 00 06 00 15 00 05"),
            )
            .expect(
                &h("FF 03 00 00 04 95 00 01"),
                &h("FF 03 00 01 04 95 01 81 05"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 01"),
                &h("FF 03 00 01 04 95 21 01 02"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 04"),
                &h("FF 03 00 02 04 95 09 04 00 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 03"),
                &h("FF 03 00 01 04 95 21 03 AA"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 05"),
                &h("FF 03 00 01 04 95 21 05 BB"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 07"),
                &h("FF 03 00 02 04 95 21 07 05 05"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        let export = headset.read_sound_profile(0).unwrap();
        assert!(headset.client().transport().is_exhausted());
        assert_eq!(export.model.as_deref(), Some("M"));
        assert_eq!(export.firmware.as_deref(), Some("6.21.5"));
        assert_eq!(export.feature_version, None);
        assert_eq!(export.status_raw, Some(vec![2]));
        assert_eq!(export.audio_mode_raw, Some(vec![0, 1]));
        let payloads: Vec<&[u8]> = export.blocks.iter().map(|b| &b.payload[..]).collect();
        assert_eq!(payloads, [&[0xAA][..], &[0xBB], &[5, 5]]);
    }

    #[test]
    fn missing_profile_block_fails_the_export() {
        let mock = MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 01 04 95 13 86 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 01 04 95 13 81 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 00 01"),
                &h("FF 03 00 01 04 95 01 81 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 01"),
                &h("FF 03 00 01 04 95 21 81 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 08 04"),
                &h("FF 03 00 01 04 95 09 84 01"),
            )
            .expect(
                &h("FF 03 00 00 04 95 20 03"),
                &h("FF 03 00 01 04 95 21 83 01"),
            );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(matches!(
            headset.read_sound_profile(0),
            Err(DeviceError::Client(ClientError::Device { .. }))
        ));
    }

    #[test]
    fn poll_returns_notifications() {
        let mut mock = MockTransport::new();
        mock.push_incoming(&h("FF 03 00 01 04 95 1A 85 00"));
        let mut headset = Headset::validated_for_tests(mock, T);
        let frames = headset.poll_notifications(T).unwrap();
        assert_eq!(frames.len(), 1);
        assert!(headset.poll_notifications(T).unwrap().is_empty());
    }

    #[test]
    fn device_error_is_reported() {
        // Erreur GAIA : id | 0x0180.
        let mock = MockTransport::new().expect(
            &h("FF 03 00 01 04 95 1A 02 32"),
            &h("FF 03 00 01 04 95 1B 82 01"),
        );
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(matches!(
            headset.set_level(50),
            Err(DeviceError::Client(ClientError::Device { .. }))
        ));
    }
}
