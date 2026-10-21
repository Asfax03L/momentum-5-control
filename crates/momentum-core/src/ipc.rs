//! Canal local entre `ctl` et l'icône : protocole texte, en Rust pur.
//!
//! L'icône garde seule la connexion GAIA ; `ctl` lui envoie une requête par
//! connexion au named pipe (voir `docs/PLAN-SCENES-IPC.md`). Requête : une
//! ligne `v1 <commande>` ; réponse : `OK` ou `ERREUR <raison>` sur la
//! première ligne, puis le contenu. La liste des requêtes est fermée : aucune
//! trame brute ni commande arbitraire ne passe par le canal.

use crate::device::{AudioMode, HeadsetState, SoundProfileStatus};
use crate::protocol::messages::M5NoiseMode;
use crate::viewmodel::stream_label;

/// Version du protocole du canal (et du JSON d'état).
pub const PROTOCOL_VERSION: u32 = 1;

/// Taille maximale d'une requête, en octets.
pub const MAX_REQUEST_LEN: usize = 1024;

/// Nom du pipe pour un utilisateur (SID textuel, « S-1-5-21-… »).
pub fn pipe_name(user_sid: &str) -> String {
    format!(r"\\.\pipe\momentum-ctl-win-{user_sid}")
}

/// Requêtes acceptées.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// État du casque en JSON.
    Status,
    /// État du casque en texte, une ligne par réglage.
    StatusText,
    /// Noms des scènes, un par ligne.
    ListScenes,
    /// Applique la scène nommée ; réponse : rapport détaillé.
    ApplyScene(String),
}

impl Request {
    /// Ligne envoyée par `ctl`.
    pub fn to_line(&self) -> String {
        let command = match self {
            Request::Status => "etat".to_string(),
            Request::StatusText => "etat-texte".to_string(),
            Request::ListScenes => "scenes".to_string(),
            Request::ApplyScene(name) => format!("scene {name}"),
        };
        format!("v{PROTOCOL_VERSION} {command}\n")
    }
}

/// Analyse une requête reçue par l'icône.
pub fn parse_request(line: &str) -> Result<Request, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (version, command) = line
        .split_once(' ')
        .ok_or_else(|| "requête attendue : v1 <commande>".to_string())?;
    if version != format!("v{PROTOCOL_VERSION}") {
        return Err(format!(
            "version « {version} » non prise en charge (attendue : v{PROTOCOL_VERSION})"
        ));
    }
    let (name, argument) = match command.split_once(' ') {
        Some((n, a)) => (n, Some(a.trim())),
        None => (command, None),
    };
    match (name, argument) {
        ("etat", None) => Ok(Request::Status),
        ("etat-texte", None) => Ok(Request::StatusText),
        ("scenes", None) => Ok(Request::ListScenes),
        ("scene", Some(scene)) if !scene.is_empty() => Ok(Request::ApplyScene(scene.to_string())),
        ("scene", _) => Err("nom de scène manquant".to_string()),
        _ => Err(format!(
            "requête inconnue « {command} » (possibles : etat, etat-texte, scenes, scene <nom>)"
        )),
    }
}

/// Réponse de l'icône.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reply {
    Ok(String),
    Error(String),
}

impl Reply {
    pub fn encode(&self) -> String {
        match self {
            Reply::Ok(body) => format!("OK\n{body}"),
            Reply::Error(reason) => format!("ERREUR {}\n", reason.replace('\n', " ")),
        }
    }

    pub fn decode(text: &str) -> Reply {
        let (first, body) = text.split_once('\n').unwrap_or((text, ""));
        match first.trim_end_matches('\r') {
            "OK" => Reply::Ok(body.to_string()),
            other => Reply::Error(
                other
                    .strip_prefix("ERREUR ")
                    .unwrap_or("réponse illisible de l'icône")
                    .to_string(),
            ),
        }
    }
}

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

/// État du casque en JSON (version [`PROTOCOL_VERSION`]) : `null` pour une
/// valeur non lue, `"connecte": false` sans casque. Ni nom ni adresse.
pub fn state_json(state: Option<&HeadsetState>) -> String {
    let Some(s) = state else {
        return format!("{{\"version\":{PROTOCOL_VERSION},\"connecte\":false}}");
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
        format!("\"version\":{PROTOCOL_VERSION}"),
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
    fn requests_round_trip_and_unknown_ones_are_refused() {
        for request in [
            Request::Status,
            Request::StatusText,
            Request::ListScenes,
            Request::ApplyScene("Transport".to_string()),
        ] {
            assert_eq!(parse_request(&request.to_line()), Ok(request));
        }
        assert_eq!(
            parse_request("v1 scene Mon trajet\r\n"),
            Ok(Request::ApplyScene("Mon trajet".to_string()))
        );
        for bad in [
            "etat",
            "v2 etat",
            "v1 mode arret",
            "v1 scene",
            "v1 scene  ",
            "v1 etat x",
        ] {
            assert!(parse_request(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn replies_round_trip() {
        let ok = Reply::Ok("ligne 1\nligne 2\n".to_string());
        assert_eq!(Reply::decode(&ok.encode()), ok);
        let err = Reply::Error("casque non connecté".to_string());
        assert_eq!(Reply::decode(&err.encode()), err);
        assert_eq!(
            Reply::decode("n'importe quoi"),
            Reply::Error("réponse illisible de l'icône".to_string())
        );
    }

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
