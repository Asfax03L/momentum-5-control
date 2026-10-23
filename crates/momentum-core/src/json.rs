//! Sorties JSON de `ctl`, version 1, sans adresse ni numéro de série.
//!
//! [`state_json`] conserve octet pour octet le format historique de `etat`.
//! Les autres lectures ont une clé dédiée (`egaliseur`, `connexions`, `nom`,
//! `scenes`). [`result_json`] ajoute `ok`, `erreur` en cas d'échec, `etat`
//! (valeurs effectivement relues) et `details` (rapport ou fichier exporté).
//! Une valeur non lue vaut `null` ; une demande acquittée n'est pas décrite
//! comme relue : `resultat` conserve la description de son issue.

use crate::device::{AudioMode, EqState, HeadsetState, NoiseState, PromptMode, SoundProfileStatus};
use crate::diagnostic::mask_addresses;
use crate::protocol::messages::M5NoiseMode;
use crate::viewmodel::stream_label;
use serde_json::{Value, json};

/// Version du format JSON de l'état.
pub const JSON_VERSION: u32 = 1;

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_opt<T>(value: Option<T>, text: impl Fn(T) -> String) -> String {
    value.map_or("null".to_string(), text)
}

/// État du casque en JSON (version [`JSON_VERSION`]) : `null` pour une
/// valeur non lue, `"connecte": false` sans casque. Ni nom ni adresse.
pub fn state_json(state: Option<&HeadsetState>) -> String {
    let Some(s) = state else {
        return format!("{{\"version\":{JSON_VERSION},\"connecte\":false}}");
    };
    let (mode, level) = match s.noise.mode() {
        M5NoiseMode::Off => ("arret", None),
        M5NoiseMode::Adaptive => ("adaptatif", None),
        M5NoiseMode::Custom { level } => ("personnalise", Some(level)),
    };
    let audio_mode = |m: AudioMode| {
        json_string(match m {
            AudioMode::Equalizer => "egaliseur",
            AudioMode::SpeechClarity => "voix",
            AudioMode::Personalization => "profil",
            AudioMode::Unknown(_) => "inconnu",
        })
    };
    let profile = |p: SoundProfileStatus| {
        json_string(match p {
            SoundProfileStatus::NoProfile => "aucun",
            SoundProfileStatus::TestRunning => "test_en_cours",
            SoundProfileStatus::Received => "recu",
            SoundProfileStatus::Unknown(_) => "inconnu",
        })
    };
    let boolean = |b: bool| b.to_string();
    let stream = stream_label(s.codec, s.sample_rate_hz);
    [
        format!("\"version\":{JSON_VERSION}"),
        "\"connecte\":true".to_string(),
        format!("\"batterie\":{}", s.battery),
        format!(
            "\"bruit\":{{\"mode\":{},\"niveau\":{},\"anti_vent\":{}}}",
            json_string(mode),
            json_opt(level, |l| l.to_string()),
            s.noise.anti_wind
        ),
        format!("\"audio_spatial\":{}", json_opt(s.spatial_audio, boolean)),
        format!("\"suivi_tete\":{}", json_opt(s.head_tracking, boolean)),
        format!("\"mode_audio\":{}", json_opt(s.audio_mode, audio_mode)),
        format!("\"profil\":{}", json_opt(s.sound_profile, profile)),
        format!(
            "\"commandes_tactiles\":{}",
            json_opt(s.touch_controls, boolean)
        ),
        format!("\"codec\":{}", json_opt(s.codec, |c| c.to_string())),
        format!(
            "\"frequence_hz\":{}",
            json_opt(s.sample_rate_hz, |f| f.to_string())
        ),
        format!(
            "\"flux\":{}",
            if stream.is_empty() {
                "null".to_string()
            } else {
                json_string(&stream)
            }
        ),
    ]
    .into_iter()
    .fold(String::from("{"), |mut out, field| {
        if out.len() > 1 {
            out.push(',');
        }
        out.push_str(&field);
        out
    }) + "}"
}

/// Connexion exportable : ni adresse, ni empreinte opaque du service.
#[derive(Clone, Debug)]
pub struct ConnectionSummary {
    pub name: String,
    pub connected: bool,
    pub this_device: bool,
}

/// Valeurs déjà lues par la commande. Ne provoque aucun échange matériel.
/// Les champs isolés servent aux commandes qui ne relisent pas tout l'état.
#[derive(Default)]
pub struct Readback {
    pub state: Option<HeadsetState>,
    pub noise: Option<NoiseState>,
    pub equalizer: Option<EqState>,
    pub connections: Option<Vec<ConnectionSummary>>,
    pub name: Option<String>,
    pub spatial_audio: Option<bool>,
    pub head_tracking: Option<bool>,
    pub multipoint: Option<bool>,
    pub prompt_volume: Option<i8>,
}

fn noise_value(noise: Option<&NoiseState>) -> Value {
    noise.map_or(Value::Null, |n| {
        let (mode, level) = match n.mode() {
            M5NoiseMode::Off => ("arret", None),
            M5NoiseMode::Adaptive => ("adaptatif", None),
            M5NoiseMode::Custom { level } => ("personnalise", Some(level)),
        };
        json!({"mode": mode, "niveau": level, "anti_vent": n.anti_wind})
    })
}

fn equalizer_value(eq: Option<&EqState>) -> Value {
    eq.map_or(Value::Null, |eq| {
        json!({
            "gains_dixiemes_db": eq.gains,
            "min_dixiemes_db": eq.min_gain,
            "max_dixiemes_db": eq.max_gain,
            "basses": eq.bass_boost,
        })
    })
}

fn connections_value(list: Option<&[ConnectionSummary]>) -> Value {
    list.map_or(Value::Null, |list| {
        Value::Array(list.iter().map(|c| json!({
        "nom": mask_addresses(&c.name), "connecte": c.connected, "ce_pc": c.this_device,
    })).collect())
    })
}

impl Readback {
    /// État éventuellement partiel : aucun champ n'est rempli avec la
    /// valeur demandée, seulement avec une lecture déjà effectuée.
    pub fn value(&self) -> Value {
        let s = self.state.as_ref();
        json!({
            "batterie": s.map(|s| s.battery),
            "bruit": noise_value(self.noise.as_ref().or(s.map(|s| &s.noise))),
            "audio_spatial": self.spatial_audio.or(s.and_then(|s| s.spatial_audio)),
            "suivi_tete": self.head_tracking.or(s.and_then(|s| s.head_tracking)),
            "mode_audio": s.and_then(|s| s.audio_mode).map(|m| match m {
                AudioMode::Equalizer => "egaliseur", AudioMode::SpeechClarity => "voix",
                AudioMode::Personalization => "profil", AudioMode::Unknown(_) => "inconnu",
            }),
            "profil": s.and_then(|s| s.sound_profile).map(|p| match p {
                SoundProfileStatus::NoProfile => "aucun", SoundProfileStatus::TestRunning => "test_en_cours",
                SoundProfileStatus::Received => "recu", SoundProfileStatus::Unknown(_) => "inconnu",
            }),
            "protection_batterie": s.and_then(|s| s.battery_protection),
            "commandes_tactiles": s.and_then(|s| s.touch_controls),
            "smart_pause": s.and_then(|s| s.smart_pause),
            "reponse_auto": s.and_then(|s| s.auto_answer),
            "appels_confort": s.and_then(|s| s.comfort_call),
            "extinction_secondes": s.and_then(|s| s.auto_off_seconds),
            "invites": s.and_then(|s| s.prompt_mode).map(|p| match p {
                PromptMode::TonesOnly => "tonalites", PromptMode::TonesAndVoice => "voix",
                PromptMode::Unknown => "inconnu",
            }),
            "codec": s.and_then(|s| s.codec),
            "frequence_hz": s.and_then(|s| s.sample_rate_hz),
            "egaliseur": equalizer_value(self.equalizer.as_ref()),
            "connexions": connections_value(self.connections.as_deref()),
            "nom": self.name.as_deref().map(mask_addresses),
            "multipoint": self.multipoint,
            "volume_invites": self.prompt_volume.map(crate::device::prompt_volume_percent),
        })
    }
}

/// Lecture de l'égaliseur ; gains et bornes en dixièmes de dB.
pub fn equalizer_json(eq: Option<&EqState>) -> String {
    json!({"version": JSON_VERSION, "egaliseur": equalizer_value(eq)}).to_string()
}

/// Lecture des connexions : `null` si la liste n'a pas été lue.
pub fn connections_json(list: Option<&[ConnectionSummary]>) -> String {
    json!({"version": JSON_VERSION, "connexions": connections_value(list)}).to_string()
}

/// Lecture du nom, sans les autres données d'identification du casque.
pub fn name_json(name: Option<&str>) -> String {
    json!({"version": JSON_VERSION, "nom": name.map(mask_addresses)}).to_string()
}

/// Liste des noms des scènes valides et avertissements du fichier local.
pub fn scenes_json(names: &[&str], errors: &[String]) -> String {
    let names: Vec<_> = names.iter().map(|s| mask_addresses(s)).collect();
    let errors: Vec<_> = errors.iter().map(|s| mask_addresses(s)).collect();
    json!({"version": JSON_VERSION, "scenes": names, "avertissements": errors}).to_string()
}

/// Documentation générée, encapsulée dans un seul objet avec `--json`.
pub fn documentation_json(kind: &str, text: &str) -> String {
    json!({"version": JSON_VERSION, "type": kind, "contenu": text}).to_string()
}

/// Résultat d'un réglage ou d'un export ; `etat` peut rester partiel.
/// En échec, les valeurs relues avant l'erreur sont conservées.
pub fn result_json(
    readback: &Readback,
    error: Option<&str>,
    outcome: Option<&str>,
    details: Option<&str>,
) -> String {
    let mut value = json!({
        "version": JSON_VERSION, "ok": error.is_none(), "etat": readback.value(),
        "resultat": outcome.map(mask_addresses), "details": details.map(mask_addresses),
    });
    if let Some(error) = error {
        value["erreur"] = json!(mask_addresses(error));
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::NoiseState;

    #[test]
    fn state_json_uses_null_for_unread_values() {
        assert_eq!(state_json(None), r#"{"version":1,"connecte":false}"#);
        let state = HeadsetState {
            battery: 70,
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: true,
                level: 0,
            },
            audio_mode: Some(AudioMode::Personalization),
            spatial_audio: Some(false),
            ..HeadsetState::default()
        };
        let json = state_json(Some(&state));
        assert!(json.starts_with(r#"{"version":1,"connecte":true,"batterie":70,"bruit":{"mode":"personnalise","niveau":0,"anti_vent":true}"#), "{json}");
        assert!(json.contains(r#""audio_spatial":false,"suivi_tete":null,"mode_audio":"profil","profil":null,"commandes_tactiles":null,"codec":null,"frequence_hz":null,"flux":null}"#), "{json}");
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\nd\u{1}"), r#""a\"b\\c\nd\u0001""#);
    }
}
