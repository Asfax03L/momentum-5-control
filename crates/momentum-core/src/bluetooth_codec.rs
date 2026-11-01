//! Codec Bluetooth du PC vers le casque, côté PipeWire (Linux).
//!
//! Lecture : `pactl --format=json list cards`. Choix : `pactl set-card-profile`,
//! puis relecture. Ce réglage appartient au PC : aucune trame n'est envoyée au
//! casque. Seuls les profils A2DP annoncés par la carte de la connexion courante
//! sont proposés ou acceptés ; le choix est toujours demandé par l'utilisateur.

use serde_json::Value;

use crate::transport::choice::fingerprint;

/// Taille maximale acceptée de la liste des cartes.
pub const MAX_CARDS_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodecChoice {
    /// Profil PipeWire, par exemple `a2dp-sink-aac`.
    pub profile: String,
    /// Nom du codec annoncé par PipeWire, par exemple `AAC`.
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodecCard {
    /// Nom de la carte : contient l'adresse, à ne jamais afficher.
    pub card: String,
    pub choices: Vec<CodecChoice>,
}

impl CodecCard {
    /// Le choix désigné par son codec (sans casse) ou son profil.
    pub fn find(&self, wanted: &str) -> Option<&CodecChoice> {
        let wanted = wanted.trim();
        self.choices
            .iter()
            .find(|c| c.profile == wanted || c.label.eq_ignore_ascii_case(wanted))
    }

    pub fn active(&self) -> Option<&CodecChoice> {
        self.choices.iter().find(|c| c.active)
    }
}

fn label(description: &str) -> Option<String> {
    let (_, rest) = description.split_once("codec ")?;
    let label = rest.split(')').next()?.trim();
    (!label.is_empty()).then(|| label.to_string())
}

/// Carte de la connexion courante, repérée par l'empreinte de son adresse.
/// `None` : pas de carte pour cette connexion, ou liste illisible.
pub fn parse_cards(text: &str, headset_id: &str) -> Option<CodecCard> {
    if text.len() > MAX_CARDS_BYTES {
        return None;
    }
    let cards: Value = serde_json::from_str(text).ok()?;
    let card = cards.as_array()?.iter().find(|card| {
        card["properties"]["api.bluez5.address"]
            .as_str()
            .is_some_and(|address| fingerprint(address) == headset_id)
    })?;
    let name = card["name"].as_str()?;
    if !name.starts_with("bluez_card.") {
        return None;
    }
    let active = card["active_profile"].as_str();
    let mut choices: Vec<CodecChoice> = card["profiles"]
        .as_object()?
        .iter()
        .filter(|(profile, value)| {
            profile.starts_with("a2dp-sink") && value["available"].as_bool() == Some(true)
        })
        .filter_map(|(profile, value)| {
            Some(CodecChoice {
                profile: profile.clone(),
                label: label(value["description"].as_str()?)?,
                active: active == Some(profile.as_str()),
            })
        })
        .collect();
    choices.sort_by(|a, b| a.label.cmp(&b.label));
    Some(CodecCard {
        card: name.to_string(),
        choices,
    })
}

#[cfg(target_os = "linux")]
mod system {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::{CodecCard, parse_cards};
    use crate::i18n::{tr, trf};

    fn pactl() -> Command {
        let mut command = Command::new("pactl");
        command.env("LC_ALL", "C").stdin(Stdio::null());
        command
    }

    /// La carte de la connexion courante, si `pactl` la donne.
    pub fn read(headset_id: &str) -> Option<CodecCard> {
        let output = pactl()
            .args(["--format=json", "list", "cards"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_cards(std::str::from_utf8(&output.stdout).ok()?, headset_id)
    }

    /// Applique un profil annoncé par la carte, puis le relit. Le son peut
    /// s'interrompre un instant.
    pub fn select(headset_id: &str, wanted: &str) -> Result<CodecCard, String> {
        let card = read(headset_id).ok_or_else(|| tr("codec : carte du casque introuvable"))?;
        let choice = card
            .find(wanted)
            .ok_or_else(|| trf("codec non proposé par PipeWire : {}", &[&wanted]))?;
        let profile = choice.profile.clone();
        let status = pactl()
            .args(["set-card-profile", &card.card, &profile])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| tr("codec : pactl est absent").to_string())?;
        if !status.success() {
            return Err(tr("codec : changement refusé par PipeWire").to_string());
        }
        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(300));
            if let Some(read_back) = read(headset_id)
                && read_back.active().is_some_and(|c| c.profile == profile)
            {
                return Ok(read_back);
            }
        }
        Err(tr("codec : le changement n'a pas été relu").to_string())
    }
}

#[cfg(target_os = "linux")]
pub use system::{read, select};

#[cfg(test)]
mod tests {
    use super::*;

    const ADDRESS: &str = "02:00:00:00:00:01";

    fn cards() -> String {
        serde_json::json!([
            {
                "name": "alsa_card.pci",
                "active_profile": "off",
                "properties": {},
                "profiles": {}
            },
            {
                "name": "bluez_card.02_00_00_00_00_01",
                "active_profile": "a2dp-sink",
                "properties": { "api.bluez5.address": ADDRESS },
                "profiles": {
                    "off": { "description": "(null)", "available": true },
                    "a2dp-sink-aac": {
                        "description": "High Fidelity Playback (A2DP Sink, codec AAC)",
                        "available": true
                    },
                    "a2dp-sink": {
                        "description": "High Fidelity Playback (A2DP Sink, codec aptX HD)",
                        "available": true
                    },
                    "a2dp-sink-sbc": {
                        "description": "High Fidelity Playback (A2DP Sink, codec SBC)",
                        "available": false
                    },
                    "headset-head-unit": {
                        "description": "Headset Head Unit (HSP/HFP, codec mSBC)",
                        "available": true
                    }
                }
            }
        ])
        .to_string()
    }

    #[test]
    fn only_available_a2dp_profiles_of_this_headset_are_offered() {
        let card = parse_cards(&cards(), &fingerprint(ADDRESS)).unwrap();
        let labels: Vec<_> = card.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["AAC", "aptX HD"]);
        assert_eq!(card.active().unwrap().profile, "a2dp-sink");
    }

    #[test]
    fn a_choice_is_found_by_codec_or_profile_and_nothing_else() {
        let card = parse_cards(&cards(), &fingerprint(ADDRESS)).unwrap();
        assert_eq!(card.find("aac").unwrap().profile, "a2dp-sink-aac");
        assert_eq!(card.find("a2dp-sink").unwrap().label, "aptX HD");
        assert!(card.find("headset-head-unit").is_none());
        assert!(card.find("off").is_none());
    }

    #[test]
    fn another_headset_or_an_invalid_list_gives_nothing() {
        assert!(parse_cards(&cards(), &fingerprint("02:00:00:00:00:02")).is_none());
        assert!(parse_cards("pas du json", &fingerprint(ADDRESS)).is_none());
        assert!(parse_cards(&" ".repeat(MAX_CARDS_BYTES + 1), &fingerprint(ADDRESS)).is_none());
    }
}
