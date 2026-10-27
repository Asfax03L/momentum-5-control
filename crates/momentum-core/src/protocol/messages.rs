//! Interprétation des payloads de réponse connus.

use super::command::{CommandId, PacketKind, VENDOR_QUALCOMM, VENDOR_SENNHEISER, get};
use super::{Frame, ProtocolError};

fn bad(command: CommandId, reason: &'static str) -> ProtocolError {
    ProtocolError::BadPayload { command, reason }
}

/// État physique du port (`0502` / `0482`), distinct de l'activation du capteur.
/// `02` / `03` observés sur M5 ; toute autre valeur est conservée sans interprétation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OnHeadState {
    NotWorn,
    Worn,
    Unknown(u8),
}

/// Contrat de lecture strict : un seul octet, sans rabattre l'inconnu sur un booléen.
pub fn parse_on_head_state(payload: &[u8]) -> Result<OnHeadState, ProtocolError> {
    match payload {
        [2] => Ok(OnHeadState::NotWorn),
        [3] => Ok(OnHeadState::Worn),
        [value] => Ok(OnHeadState::Unknown(*value)),
        _ => Err(bad(get::ON_HEAD_STATE.response(), "un octet attendu")),
    }
}

/// Niveau de sidetone (`0906` / `0886`) : exactement un octet, gardé tel
/// quel (`00` arrêt, `01`, `02` observés ; l'app prévoit jusqu'à `04`).
pub fn parse_sidetone(payload: &[u8]) -> Result<u8, ProtocolError> {
    match payload {
        [value] => Ok(*value),
        _ => Err(bad(get::SIDETONE.response(), "un octet attendu")),
    }
}

/// Batterie en pourcentage (capture : `2A` = 42 %).
pub fn parse_battery(payload: &[u8]) -> Result<u8, ProtocolError> {
    payload
        .first()
        .copied()
        .ok_or_else(|| bad(get::BATTERY.response(), "payload vide"))
}

/// Codec Bluetooth actif.
///
/// Table relevée dans le client d'Oein (issue de l'APK Smart Control Plus) ;
/// seule la valeur 5 (aptX HD) a une capture vérifiée.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Codec(pub u8);

impl Codec {
    pub fn name(self) -> Option<&'static str> {
        Some(match self.0 {
            0 => "SBC",
            1 => "AAC",
            2 => "aptX",
            3 => "aptX Low Latency",
            4 => "MP3",
            5 => "aptX HD",
            6 => "FastStream",
            7 => "LHDC",
            8 => "aptX Adaptive",
            9 => "aptX Lossless",
            10 => "LC3",
            _ => return None,
        })
    }
}

pub fn parse_codec(payload: &[u8]) -> Result<Codec, ProtocolError> {
    payload
        .first()
        .map(|&c| Codec(c))
        .ok_or_else(|| bad(get::CODEC.response(), "payload vide"))
}

/// Appareil appairé au casque (`1401 [index]` → `[index, priorité, état,
/// nom…\0]`). Relevé sur M5 : 3 appareils dont 2 connectés ; seul « état
/// non nul = connecté » est établi.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PairedDevice {
    pub index: u8,
    pub name: String,
    pub connected: bool,
}

pub fn parse_paired_device(payload: &[u8]) -> Result<PairedDevice, ProtocolError> {
    match payload {
        [index, _priority, status, name @ ..] => {
            let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
            Ok(PairedDevice {
                index: *index,
                name: String::from_utf8_lossy(&name[..end]).into_owned(),
                connected: *status != 0,
            })
        }
        _ => Err(bad(get::PAIRED_DEVICE_INFO.response(), "moins de 3 octets")),
    }
}

/// Fréquence d'échantillonnage (notification `089A`) : u32 big-endian, Hz.
pub fn parse_sample_rate(payload: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = payload.get(..4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

/// Nom du modèle : ASCII terminé par `\0` (on coupe au premier `\0`).
pub fn parse_model_name(payload: &[u8]) -> Result<String, ProtocolError> {
    let end = payload
        .iter()
        .position(|&b| b == 0)
        .unwrap_or(payload.len());
    let text = &payload[..end];
    if !text.is_ascii() {
        return Err(bad(get::MODEL_NAME.response(), "chaîne non ASCII"));
    }
    Ok(String::from_utf8_lossy(text).into_owned())
}

/// Version firmware : 3 × u16 big-endian d'après la doc. À valider sur le M5.
pub fn parse_firmware_version(payload: &[u8]) -> Result<(u16, u16, u16), ProtocolError> {
    match payload {
        [a, b, c, d, e, f, ..] => Ok((
            u16::from_be_bytes([*a, *b]),
            u16::from_be_bytes([*c, *d]),
            u16::from_be_bytes([*e, *f]),
        )),
        _ => Err(bad(get::FIRMWARE_VERSION.response(), "moins de 6 octets")),
    }
}

/// Mode de contrôle du bruit (encodage combiné du Momentum 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseControlMode {
    AntiWind,
    Transparency,
    Adaptive,
    Unknown(u8),
}

impl NoiseControlMode {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Self::AntiWind,
            2 => Self::Transparency,
            3 => Self::Adaptive,
            other => Self::Unknown(other),
        }
    }

    pub fn label_fr(self) -> String {
        match self {
            Self::AntiWind => "anti-vent".into(),
            Self::Transparency => "transparence".into(),
            Self::Adaptive => "adaptatif".into(),
            Self::Unknown(b) => format!("mode inconnu {b}"),
        }
    }
}

/// Libellé de l'état d'une paire `[mode, état]` : 0 désactivé, 1 activé
/// (« max » pour l'anti-vent), 2 automatique (client d'Oein, à valider sur M5).
pub fn noise_state_label(state: u8) -> String {
    match state {
        0 => "désactivé".into(),
        1 => "activé".into(),
        2 => "auto".into(),
        other => format!("état {other}"),
    }
}

/// Paires `[mode, état]` (voir [`noise_state_label`]).
pub fn parse_noise_control_modes(
    payload: &[u8],
) -> Result<Vec<(NoiseControlMode, u8)>, ProtocolError> {
    let (pairs, rest) = payload.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(bad(
            get::NOISE_CONTROL_MODES.response(),
            "longueur impaire (paires attendues)",
        ));
    }
    Ok(pairs
        .iter()
        .map(|&[mode, state]| (NoiseControlMode::from_byte(mode), state))
        .collect())
}

/// Configuration de l'égaliseur graphique (gains en dixièmes de dB).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EqConfig {
    pub bands: u8,
    pub min_gain: i8,
    pub max_gain: i8,
}

/// `[bandes, gain min i8, gain max i8, …]`. M5 : `08 C4 3C 00 00`, soit
/// 8 bandes de −6,0 à +6,0 dB (les 2 derniers octets restent inconnus).
pub fn parse_eq_config(payload: &[u8]) -> Result<EqConfig, ProtocolError> {
    match payload {
        [bands, min, max, ..] => Ok(EqConfig {
            bands: *bands,
            min_gain: *min as i8,
            max_gain: *max as i8,
        }),
        _ => Err(bad(get::EQ_CONFIG.response(), "moins de 3 octets")),
    }
}

/// Réponse à GET bande (`[bande]` en requête) : `[gain i8]` en dixièmes de dB.
/// Observé sur M5 : un seul octet, la bande n'est pas répétée.
pub fn parse_eq_gain(payload: &[u8]) -> Result<i8, ProtocolError> {
    payload
        .first()
        .map(|&g| g as i8)
        .ok_or_else(|| bad(get::EQ_BAND.response(), "payload vide"))
}

/// Table `(bande, valeur u16)` en triplets, telle que la notification 0x108B
/// (fréquences centrales en Hz) la pousse à l'app.
///
/// Capture M5 : 90, 325, 1500, 2500, 4500, 8000, 13000, 16000 Hz.
pub fn parse_band_table(payload: &[u8]) -> Result<Vec<(u8, u16)>, ProtocolError> {
    let (triplets, rest) = payload.as_chunks::<3>();
    if !rest.is_empty() {
        return Err(bad(
            CommandId::new(0x108B),
            "longueur non multiple de 3 (triplets attendus)",
        ));
    }
    Ok(triplets
        .iter()
        .map(|&[band, hi, lo]| (band, u16::from_be_bytes([hi, lo])))
        .collect())
}

/// Flottants 32 bits big-endian (blocs du profil de personnalisation du
/// son), suivis des octets restants en hexadécimal.
pub fn f32_block(payload: &[u8]) -> (Vec<f32>, &[u8]) {
    let (words, rest) = payload.as_chunks::<4>();
    (words.iter().map(|w| f32::from_be_bytes(*w)).collect(), rest)
}

fn describe_f32_block(payload: &[u8]) -> String {
    let (values, rest) = f32_block(payload);
    let list: Vec<String> = values.iter().map(|v| format!("{v:.3}")).collect();
    let mut text = format!("profil [{}]", list.join(", "));
    if !rest.is_empty() {
        text.push_str(&format!(" + octets {rest:02X?}"));
    }
    text
}

fn tenths_db(value: i8) -> f32 {
    f32::from(value) / 10.0
}

/// Features du core Qualcomm (réponse à 001D:0001) ; même mise en page
/// pour le catalogue Sennheiser (0495:0001), dans un autre espace de noms.
///
/// Hypothèse de lecture, cohérente avec la capture M5 : un premier octet
/// (0 = liste complète, sans suite) puis des paires `(feature, version)`.
pub fn parse_core_features(payload: &[u8]) -> Result<(u8, Vec<(u8, u8)>), ProtocolError> {
    let command = super::command::core_get::SUPPORTED_FEATURES.response();
    let (&more, rest) = payload
        .split_first()
        .ok_or_else(|| bad(command, "payload vide"))?;
    let (pairs, odd) = rest.as_chunks::<2>();
    if !odd.is_empty() {
        return Err(bad(command, "longueur impaire après le premier octet"));
    }
    Ok((more, pairs.iter().map(|&[f, v]| (f, v)).collect()))
}

/// Mode de contrôle du bruit tel que l'affiche Smart Control Plus sur le M5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum M5NoiseMode {
    /// « ARRÊT » : ANC coupé.
    Off,
    /// « Adaptatif ».
    Adaptive,
    /// « Personnalisé » : curseur bipolaire, 0 = ANC maximal, 50 = neutre,
    /// 100 = transparence maximale.
    Custom { level: u8 },
}

/// Déduit le mode du M5 à partir de trois GET.
///
/// Relevé croisé du 08/10/2026 (appuis dans l'app via ADB, lecture GAIA sur
/// le PC) :
/// - ARRÊT        → `1A05` = 0 ;
/// - Adaptatif    → `1A05` = 1 et paire `(3, 1)` dans `1A01` ;
/// - Personnalisé → `1A05` = 1, paire `(3, 0)`, curseur = `1A03` (0 à 100).
///
/// L'interrupteur de transparence `1805` (ACCENTUM) reste toujours à 0 : il
/// n'intervient pas.
pub fn m5_noise_mode(anc_enabled: bool, adaptive: bool, level: u8) -> M5NoiseMode {
    match (anc_enabled, adaptive) {
        (false, _) => M5NoiseMode::Off,
        (true, true) => M5NoiseMode::Adaptive,
        (true, false) => M5NoiseMode::Custom { level },
    }
}

/// Interrupteur on/off (ANC, transparence…) : payload `[0|1]`.
pub fn parse_switch(command: CommandId, payload: &[u8]) -> Result<bool, ProtocolError> {
    match payload.first() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        Some(_) => Err(bad(command.response(), "valeur différente de 0 ou 1")),
        None => Err(bad(command.response(), "payload vide")),
    }
}

fn on_off(on: bool, feminine: bool) -> &'static str {
    match (on, feminine) {
        (true, false) => "activé",
        (false, false) => "désactivé",
        (true, true) => "activée",
        (false, true) => "désactivée",
    }
}

/// Description lisible d'une trame, pour le probe et les journaux.
///
/// Ne lève jamais d'erreur : une trame inconnue est décrite par ses champs.
pub fn describe(frame: &Frame) -> String {
    let vendor = match frame.vendor {
        VENDOR_SENNHEISER => "Sennheiser".to_string(),
        VENDOR_QUALCOMM => "Qualcomm".to_string(),
        other => format!("vendor {other:#06X}"),
    };
    let head = format!(
        "{vendor} {} (feature {}, {}, cmd {})",
        frame.command,
        frame.command.feature(),
        kind_fr(frame.kind()),
        frame.command.number()
    );

    if frame.kind() == PacketKind::Error {
        return format!("{head} → ERREUR, payload brut {:02X?}", frame.payload);
    }
    if frame.kind() == PacketKind::Command {
        return format!("{head} → non interprété");
    }
    if frame.vendor == VENDOR_QUALCOMM
        && frame.command.request() == super::command::core_get::SUPPORTED_FEATURES
        && let Ok((more, features)) = parse_core_features(&frame.payload)
    {
        let list: Vec<String> = features
            .iter()
            .map(|(f, v)| format!("feature {f} v{v}"))
            .collect();
        return format!("{head} → [{}] (premier octet {more})", list.join(", "));
    }
    if frame.vendor == VENDOR_SENNHEISER
        && frame.command.request() == get::FEATURE_CATALOG
        && let Ok((first, features)) = parse_core_features(&frame.payload)
    {
        let list: Vec<String> = features
            .iter()
            .map(|(f, v)| format!("feature {f} v{v}"))
            .collect();
        return format!(
            "{head} → features Sennheiser [{}] (premier octet {first})",
            list.join(", ")
        );
    }
    if frame.vendor != VENDOR_SENNHEISER {
        return format!("{head} → payload {:02X?}", frame.payload);
    }

    let request = frame.command.request();
    let p = &frame.payload;
    let detail = if request == get::BATTERY {
        parse_battery(p).map(|v| format!("batterie {v} %"))
    } else if request == get::CODEC {
        parse_codec(p).map(|c| match c.name() {
            Some(n) => format!("codec {} ({n})", c.0),
            None => format!("codec {} (valeur non documentée)", c.0),
        })
    } else if request == get::MODEL_NAME {
        parse_model_name(p).map(|n| format!("modèle « {n} »"))
    } else if request == get::FIRMWARE_VERSION {
        parse_firmware_version(p).map(|(a, b, c)| format!("firmware {a}.{b}.{c}"))
    } else if request == get::NOISE_CONTROL_MODES {
        parse_noise_control_modes(p).map(|modes| {
            let list: Vec<String> = modes
                .iter()
                .map(|(m, s)| format!("{} {}", m.label_fr(), noise_state_label(*s)))
                .collect();
            format!("contrôle du bruit [{}]", list.join(", "))
        })
    } else if request == get::ANC_ENABLED {
        parse_switch(request, p).map(|on| format!("ANC {}", on_off(on, false)))
    } else if request == get::TRANSPARENCY_ENABLED {
        parse_switch(request, p).map(|on| format!("transparence {}", on_off(on, true)))
    } else if request == get::TRANSPARENCY_LEVEL {
        p.first()
            .map(|lvl| {
                format!("niveau de transparence {lvl} (M5 : 0 = ANC max, 50 = neutre, 100 = transparence max)")
            })
            .ok_or_else(|| bad(request.response(), "payload vide"))
    } else if let Some(text) = describe_extended(request, p) {
        text
    } else {
        return format!("{head} → non interprété, payload {:02X?}", p);
    };

    match detail {
        Ok(d) => format!("{head} → {d}"),
        Err(e) => format!("{head} → {e}, payload {:02X?}", p),
    }
}

fn kind_fr(kind: PacketKind) -> &'static str {
    match kind {
        PacketKind::Command => "requête",
        PacketKind::Notification => "notification",
        PacketKind::Response => "réponse",
        PacketKind::Error => "erreur",
    }
}

/// Interprétation du catalogue étendu (`probe --deep`). `None` si la commande
/// n'en fait pas partie.
fn describe_extended(request: CommandId, p: &[u8]) -> Option<Result<String, ProtocolError>> {
    let first = || {
        p.first()
            .copied()
            .ok_or_else(|| bad(request.response(), "payload vide"))
    };
    let switch = |label: &str, feminine: bool| {
        parse_switch(request, p).map(|on| format!("{label} {}", on_off(on, feminine)))
    };

    Some(if request == get::SMART_PAUSE {
        switch("Smart Pause", true)
    } else if request == get::ON_HEAD_DETECTION {
        switch("détection de port", true)
    } else if request == get::ON_HEAD_STATE {
        parse_on_head_state(p).map(|state| match state {
            OnHeadState::NotWorn => "casque non porté".to_string(),
            OnHeadState::Worn => "casque porté".to_string(),
            OnHeadState::Unknown(value) => format!("état de port inconnu ({value:02X})"),
        })
    } else if request == get::AUTO_ANSWER {
        switch("décroché automatique", false)
    } else if request == get::COMFORT_CALL {
        switch("Comfort Call", false)
    } else if request == get::LOW_LATENCY {
        switch("faible latence", true)
    } else if request == get::BASS_BOOST {
        switch("Bass Boost", false)
    } else if request == get::SIDETONE {
        first().map(|v| format!("sidetone niveau {v}"))
    } else if request == get::PROMPT_MODE {
        first().map(|v| match v {
            0 => "invites : aucune".to_string(),
            1 => "invites : tonalités".to_string(),
            2 => "invites : tonalités et voix".to_string(),
            other => format!("invites : valeur {other}"),
        })
    } else if request == get::PROMPT_LANGUAGE {
        first().map(|v| {
            let name = match v {
                0 => "anglais",
                1 => "allemand",
                2 => "français",
                3 => "espagnol",
                4 => "chinois",
                5 => "japonais",
                6 => "russe",
                7 => "coréen",
                _ => "inconnue",
            };
            format!("langue des invites {v} ({name})")
        })
    } else if request == get::BT_COMPATIBILITY {
        // Capture du 09/10/2026 : 1 = Qualité audio avancée, 0 = haute
        // résolution 24 bits/96 kHz activée (l'inverse de la lecture d'Oein).
        first().map(|v| match v {
            1 => "priorité du mode audio : Qualité audio avancée (haute résolution coupée)"
                .to_string(),
            0 => "priorité du mode audio : haute résolution 24 bits/96 kHz activée".to_string(),
            other => format!("priorité du mode audio : valeur {other}"),
        })
    } else if request == get::SOUND_PERSONALIZATION {
        first().map(|v| match v {
            0 => "personnalisation du son : aucun profil".to_string(),
            1 => "personnalisation du son : test en cours".to_string(),
            2 => "personnalisation du son : profil reçu par le casque (sélection non déduite)"
                .to_string(),
            other => format!("personnalisation du son : état {other}"),
        })
    } else if request == get::SAMPLE_RATE_NOTIFICATION.request() {
        parse_sample_rate(p)
            .map(|hz| format!("fréquence d'échantillonnage {hz} Hz"))
            .ok_or_else(|| bad(request.response(), "moins de 4 octets"))
    } else if request == get::SOUND_PROFILE_USAGE_NOTIFICATION.request() {
        first().map(|v| match v {
            0 => "profil personnalisé : non utilisé (mode égaliseur)".to_string(),
            1 => "profil personnalisé : sélectionné".to_string(),
            2 => "profil personnalisé : indisponible, Dolby Atmos activé (hypothèse)".to_string(),
            other => format!("profil personnalisé : utilisation {other}"),
        })
    } else if request == get::SOUND_PROFILE_1
        || request == get::SOUND_PROFILE_2
        || request == get::SOUND_PROFILE_3
    {
        Ok(describe_f32_block(p))
    } else if request == get::LOSSLESS {
        switch("mode sans perte 16 bits/44,1 kHz", false)
    } else if request == get::SPATIAL_AUDIO {
        switch("audio spatial", false)
    } else if request == get::PROMPT_VOLUME {
        first().map(|v| format!("volume des invites {} (0 = 77 % dans l'app)", v as i8))
    } else if request == get::BATTERY_PROTECTION {
        switch("protection de la batterie", true)
    } else if request == get::TOUCH_LOCK {
        parse_switch(request, p).map(|locked| {
            format!(
                "commandes tactiles {}",
                if locked { "désactivées" } else { "actives" }
            )
        })
    } else if request == get::AUDIO_MODE {
        p.last()
            .map(|&v| {
                let name = match v {
                    1 => " (égaliseur graphique)",
                    2 => " (Speech Clarity)",
                    3 => " (profil personnalisé)",
                    _ => "",
                };
                format!("mode audio {v}{name}, payload {p:02X?}")
            })
            .ok_or_else(|| bad(request.response(), "payload vide"))
    } else if request == get::TIMER {
        match p {
            [id, hi, lo, ..] => {
                // Valeur en secondes (capture : 30 min = 0x0708 = 1800).
                let seconds = u16::from_be_bytes([*hi, *lo]);
                let what = if *id == 0 {
                    "extinction auto"
                } else {
                    "minuteur"
                };
                if seconds == 0 {
                    Ok(format!("{what} (id {id}) = jamais"))
                } else {
                    Ok(format!(
                        "{what} (id {id}) = {} min ({seconds} s)",
                        seconds / 60
                    ))
                }
            }
            _ => Err(bad(request.response(), "moins de 3 octets")),
        }
    } else if request == get::HARDWARE_REVISION {
        match p {
            [hi, lo, ..] => Ok(format!(
                "révision matérielle {}",
                u16::from_be_bytes([*hi, *lo])
            )),
            _ => Err(bad(request.response(), "moins de 2 octets")),
        }
    } else if request == get::PAIRED_LIST_SIZE {
        p.last()
            .map(|n| format!("{n} appareil(s) appairé(s) au casque, payload {p:02X?}"))
            .ok_or_else(|| bad(request.response(), "payload vide"))
    } else if request == get::PAIRED_DEVICE_INFO {
        match p {
            [idx, prio, status, name @ ..] => {
                let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
                let name = String::from_utf8_lossy(&name[..end]);
                let state = if *status != 0 {
                    "connecté"
                } else {
                    "déconnecté"
                };
                Ok(format!(
                    "appareil n°{idx} « {name} », {state} (priorité {prio}, état brut {status})"
                ))
            }
            _ => Err(bad(request.response(), "moins de 3 octets")),
        }
    } else if request == get::EQ_CONFIG {
        parse_eq_config(p).map(|c| {
            format!(
                "égaliseur : {} bandes, gain de {:+.1} à {:+.1} dB, payload {p:02X?}",
                c.bands,
                tenths_db(c.min_gain),
                tenths_db(c.max_gain)
            )
        })
    } else if request == get::EQ_BAND {
        parse_eq_gain(p).map(|g| format!("gain {:+.1} dB", tenths_db(g)))
    } else {
        return None;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(command: CommandId, payload: &[u8]) -> String {
        describe(&Frame::new(
            VENDOR_SENNHEISER,
            command.response(),
            payload.to_vec(),
        ))
    }

    #[test]
    fn every_documented_reading_is_described() {
        // Payloads de la forme documentée pour chaque lecture ; seul le
        // texte est vérifié (aucune trame n'est envoyée).
        let cases: &[(CommandId, &[u8], &str)] = &[
            (get::BATTERY, &[42], "batterie 42 %"),
            (get::MODEL_NAME, b"M5\0", "modèle « M5 »"),
            (get::SMART_PAUSE, &[1], "Smart Pause activée"),
            (get::ON_HEAD_DETECTION, &[0], "détection de port désactivée"),
            (get::AUTO_ANSWER, &[1], "décroché automatique activé"),
            (get::COMFORT_CALL, &[0], "Comfort Call désactivé"),
            (get::LOW_LATENCY, &[1], "faible latence activée"),
            (get::BASS_BOOST, &[1], "Bass Boost activé"),
            (get::SIDETONE, &[3], "sidetone niveau 3"),
            (get::PROMPT_MODE, &[2], "tonalités et voix"),
            (get::PROMPT_MODE, &[9], "invites : valeur 9"),
            (
                get::PROMPT_LANGUAGE,
                &[2],
                "langue des invites 2 (français)",
            ),
            (get::PROMPT_LANGUAGE, &[42], "(inconnue)"),
            (get::BT_COMPATIBILITY, &[1], "Qualité audio avancée"),
            (
                get::BT_COMPATIBILITY,
                &[0],
                "haute résolution 24 bits/96 kHz",
            ),
            (
                get::SOUND_PERSONALIZATION,
                &[2],
                "profil reçu par le casque",
            ),
            (get::SOUND_PERSONALIZATION, &[0], "aucun profil"),
            (get::LOSSLESS, &[0], "mode sans perte"),
            (get::SPATIAL_AUDIO, &[1], "audio spatial activé"),
            (get::PROMPT_VOLUME, &[0], "volume des invites 0"),
            (
                get::BATTERY_PROTECTION,
                &[1],
                "protection de la batterie activée",
            ),
            (get::TOUCH_LOCK, &[1], "commandes tactiles désactivées"),
            (get::TOUCH_LOCK, &[0], "commandes tactiles actives"),
            (get::AUDIO_MODE, &[0, 2], "mode audio 2 (Speech Clarity)"),
            (
                get::TIMER,
                &[0, 0x07, 0x08],
                "extinction auto (id 0) = 30 min",
            ),
            (get::TIMER, &[0, 0, 0], "= jamais"),
            (get::HARDWARE_REVISION, &[0, 3], "révision matérielle 3"),
            (get::PAIRED_LIST_SIZE, &[2], "2 appareil(s) appairé(s)"),
            (
                get::PAIRED_DEVICE_INFO,
                b"\x01\x00\x01Casque demo\0",
                "appareil n°1 « Casque demo », connecté",
            ),
            (get::TRANSPARENCY_LEVEL, &[50], "niveau de transparence 50"),
        ];
        for (command, payload, expected) in cases {
            let text = text(*command, payload);
            assert!(text.contains(expected), "{command}: « {text} »");
            assert!(!text.contains("non interprété"), "{text}");
        }
    }

    #[test]
    fn short_payloads_are_reported_without_panicking() {
        for command in [
            get::BATTERY,
            get::SIDETONE,
            get::PROMPT_MODE,
            get::AUDIO_MODE,
            get::TIMER,
            get::HARDWARE_REVISION,
            get::PAIRED_LIST_SIZE,
            get::PAIRED_DEVICE_INFO,
            get::TRANSPARENCY_LEVEL,
        ] {
            let text = text(command, &[]);
            assert!(text.contains("payload"), "{command}: « {text} »");
        }
    }

    #[test]
    fn other_kinds_and_vendors_are_described_by_their_fields() {
        let error = describe(&Frame::new(
            VENDOR_SENNHEISER,
            get::BATTERY.error(),
            vec![0x80],
        ));
        assert!(error.contains("ERREUR") && error.contains("80"), "{error}");
        let request = describe(&Frame::new(VENDOR_SENNHEISER, get::BATTERY, vec![]));
        assert!(request.contains("requête") && request.contains("non interprété"));
        let other = describe(&Frame::new(0x1234, get::BATTERY.response(), vec![1, 2]));
        assert!(
            other.contains("vendor 0x1234") && other.contains("01, 02"),
            "{other}"
        );
        let unknown = describe(&Frame::new(
            VENDOR_SENNHEISER,
            CommandId::new(0x7F7F).response(),
            vec![7],
        ));
        assert!(unknown.contains("non interprété"), "{unknown}");
    }
}
