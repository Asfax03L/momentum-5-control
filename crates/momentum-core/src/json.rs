//! État du casque en JSON, pour `ctl etat --json` : format stable et
//! versionné, sans nom ni adresse de l'appareil.

use crate::device::{AudioMode, HeadsetState, SoundProfileStatus};
use crate::protocol::messages::M5NoiseMode;
use crate::viewmodel::stream_label;

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
