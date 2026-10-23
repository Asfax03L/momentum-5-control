//! État PipeWire en lecture seule. Les noms, adresses et identifiants du
//! graphe servent uniquement à la correspondance et ne sortent pas de l'analyse.
//!
//! Sources : docs.pipewire.org/page_man_pw-dump_1.html et group__spa__keys.html
//! (api.bluez5.address/profile/codec), page_module_metadata.html (default.audio.sink).

use serde_json::Value;

use crate::transport::choice::fingerprint;

pub const MAX_DUMP_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AudioReadError {
    #[error(
        "pw-dump est absent : installer les outils PipeWire de la distribution ; dans un Flatpak, vérifier le contenu du paquet"
    )]
    Missing,
    #[error("PipeWire n'a pas répondu dans le délai prévu")]
    Timeout,
    #[error("PipeWire est inaccessible : vérifier la session audio et les permissions du paquet")]
    Unavailable,
    #[error("le résultat de pw-dump n'est pas un graphe JSON valide")]
    Invalid,
    #[error("le graphe PipeWire dépasse la limite de 16 Mio")]
    TooLarge,
    #[error(
        "l'identité du casque connecté n'est pas disponible : un service récent et un casque piloté sont nécessaires"
    )]
    NoIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    A2dp,
    Hfp,
    Hsp,
    LeAudio,
}
impl Profile {
    pub fn label(self) -> &'static str {
        match self {
            Self::A2dp => "A2DP (musique)",
            Self::Hfp => "HFP (appel)",
            Self::Hsp => "HSP (appel)",
            Self::LeAudio => "LE Audio",
        }
    }
    pub fn call(self) -> bool {
        matches!(self, Self::Hfp | Self::Hsp)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub profile: Option<Profile>,
    pub codec: Option<&'static str>,
    pub default: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PipeWireAudio {
    /// Sorties correspondant uniquement à l'empreinte de la connexion courante.
    pub outputs: Vec<Output>,
    /// `None` : métadonnée ou identité de la sortie par défaut non connue.
    pub default_headset: Option<bool>,
}

fn profile(value: &str) -> Option<Profile> {
    let value = value.replace('_', "-");
    match value.as_str() {
        "a2dp-sink" | "a2dp-source" => Some(Profile::A2dp),
        "headset-head-unit"
        | "handsfree-head-unit"
        | "headset-audio-gateway"
        | "handsfree-audio-gateway"
        | "hfp-hf"
        | "hfp-ag" => Some(Profile::Hfp),
        "hsp-hs" | "hsp-ag" => Some(Profile::Hsp),
        "bap-sink" | "bap-source" => Some(Profile::LeAudio),
        _ => None,
    }
}

fn codec(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "sbc" => Some("SBC"),
        "sbc_xq" => Some("SBC XQ"),
        "aac" => Some("AAC"),
        "aptx" => Some("aptX"),
        "aptx_hd" => Some("aptX HD"),
        "aptx_ll" => Some("aptX Low Latency"),
        "ldac" => Some("LDAC"),
        "lc3" => Some("LC3"),
        "lc3plus_h3" => Some("LC3plus"),
        "cvsd" => Some("CVSD"),
        "msbc" => Some("mSBC"),
        "faststream" => Some("FastStream"),
        "opus_05" => Some("Opus"),
        _ => None,
    }
}

fn props(object: &Value) -> &Value {
    &object["info"]["props"]
}
fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

/// La forme normalisée est exactement celle utilisée par BlueZ dans le service.
fn address_id(value: &str) -> Option<String> {
    let bytes: Vec<_> = value.split(':').collect();
    if bytes.len() != 6
        || bytes
            .iter()
            .any(|b| b.len() != 2 || !b.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    Some(fingerprint(value))
}

fn device<'a>(node: &Value, objects: &'a [Value]) -> Option<&'a Value> {
    let id = number(&props(node)["device.id"])?;
    let mut found = objects
        .iter()
        .filter(|o| o["type"] == "PipeWire:Interface:Device" && number(&o["id"]) == Some(id));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

fn node_id(node: &Value, objects: &[Value]) -> Option<String> {
    if let Some(address) = props(node)["api.bluez5.address"].as_str() {
        return address_id(address);
    }
    device(node, objects)
        .and_then(|d| props(d)["api.bluez5.address"].as_str())
        .and_then(address_id)
}

fn node_profile(node: &Value, objects: &[Value]) -> Option<Profile> {
    if let Some(value) = props(node)["api.bluez5.profile"].as_str() {
        return profile(value);
    }
    let active = device(node, objects)?["info"]["params"]["Profile"].as_array()?;
    if active.len() != 1 {
        return None;
    }
    active[0]["name"].as_str().and_then(profile)
}

/// Analyse pure d'un seul instantané, jamais d'un flux de `--monitor`.
/// Une empreinte absente ne déclenche aucune recherche par nom de périphérique.
pub fn parse_dump(text: &str, headset_id: Option<&str>) -> Result<PipeWireAudio, AudioReadError> {
    let headset_id = headset_id
        .filter(|s| s.len() == 16 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or(AudioReadError::NoIdentity)?;
    if text.len() > MAX_DUMP_BYTES {
        return Err(AudioReadError::TooLarge);
    }
    let value: Value = serde_json::from_str(text).map_err(|_| AudioReadError::Invalid)?;
    let objects = value.as_array().ok_or(AudioReadError::Invalid)?;
    let nodes: Vec<_> = objects
        .iter()
        .filter(|o| {
            o["type"] == "PipeWire:Interface:Node" && props(o)["media.class"] == "Audio/Sink"
        })
        .collect();
    let mut defaults = Vec::new();
    for object in objects.iter().filter(|o| {
        o["type"] == "PipeWire:Interface:Metadata" && o["props"]["metadata.name"] == "default"
    }) {
        for entry in object["metadata"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|e| number(&e["subject"]) == Some(0) && e["key"] == "default.audio.sink")
        {
            let value = match entry["value"].as_str() {
                Some(s) => serde_json::from_str(s).unwrap_or(Value::Null),
                None => entry["value"].clone(),
            };
            if let Some(name) = value["name"].as_str() {
                defaults.push(name.to_string());
            }
        }
    }
    defaults.sort();
    defaults.dedup();
    let default = if defaults.len() == 1 {
        let mut matches = nodes
            .iter()
            .filter(|n| props(n)["node.name"].as_str() == Some(defaults[0].as_str()));
        let first = matches.next().copied();
        if matches.next().is_none() {
            first
        } else {
            None
        }
    } else {
        None
    };
    let default_headset = default.and_then(|node| {
        node_id(node, objects)
            .map(|id| id == headset_id)
            .or_else(|| {
                matches!(props(node)["device.api"].as_str(), Some("alsa" | "jack")).then_some(false)
            })
    });
    let outputs = nodes
        .iter()
        .filter(|node| node_id(node, objects).as_deref() == Some(headset_id))
        .map(|node| Output {
            profile: node_profile(node, objects),
            codec: props(node)["api.bluez5.codec"].as_str().and_then(codec),
            default: default.and_then(|d| {
                number(&node["id"])
                    .zip(number(&d["id"]))
                    .map(|(a, b)| a == b)
            }),
        })
        .collect();
    Ok(PipeWireAudio {
        outputs,
        default_headset,
    })
}
