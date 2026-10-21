//! API locale d'OpenHeadset Control : ce que les interfaces envoient au
//! service `momentum-daemon` et ce qu'elles en reçoivent.
//!
//! Le service est le seul propriétaire de la connexion au casque. Une
//! interface lui envoie des [`Command`] (liste fermée : uniquement les
//! réglages validés que le worker sait appliquer) et reçoit des [`Event`] ;
//! [`Snapshot`] donne l'état courant à la connexion d'une interface.
//!
//! Transport : D-Bus sous Linux (bus de session, [`dbus`]) ; canal nommé sous
//! Windows ([`wire`], `pipe`). Les messages circulent en JSON.

use momentum_core::device::{DeviceInfo, EqState, HeadsetState, NoiseModeRequest, PromptMode};
use momentum_core::scenes::SceneReport;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub mod dbus;
pub mod demo;
pub mod menu;
#[cfg(windows)]
pub mod pipe;
pub mod wire;

/// Client du service sur ce système : D-Bus sous Linux, canal nommé sous
/// Windows (même interface).
#[cfg(target_os = "linux")]
pub use dbus::Client;
#[cfg(windows)]
pub use pipe::Client;

/// Nom D-Bus du service.
pub const BUS_NAME: &str = "org.openheadset.Control";
/// Chemin de l'objet D-Bus.
pub const OBJECT_PATH: &str = "/org/openheadset/Control";
/// Interface D-Bus (version 1).
pub const INTERFACE: &str = "org.openheadset.Control1";

/// Commande d'une interface. Le service la transmet au worker, qui relit
/// l'état du casque après chaque écriture.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    SetMode(NoiseModeRequest),
    /// 0 = ANC, 100 = transparence (passe en mode Personnalisé).
    SetLevel(u8),
    SetAntiWind(bool),
    SetSpatial(bool),
    SetHeadTracking(bool),
    SetMultipoint(bool),
    /// Bande et gain en dixièmes de dB.
    SetEqBand(u8, i8),
    SetEqFlat,
    SetEqGains(Vec<i8>),
    SetBassBoost(bool),
    SetSpeechClarity(bool),
    SetSoundProfile(bool),
    SetTouchControls(bool),
    SetBatteryProtection(bool),
    SetSmartPause(bool),
    SetAutoAnswer(bool),
    SetComfortCall(bool),
    SetAutoOff(u16),
    SetPromptMode(PromptMode),
    /// Volume des invites, valeur du casque (−20 à +6).
    SetPromptVolume(i8),
    SetName(String),
    /// Appareil de la liste [`Snapshot::connections`], par sa position ;
    /// le service vérifie le nom et retrouve l'adresse lui-même.
    ToggleConnection {
        index: usize,
        name: String,
        connect: bool,
    },
    /// Scène du fichier `scenes.txt`, par son nom.
    ApplyScene(String),
    /// Sauvegarde brute du profil de personnalisation dans ce fichier.
    ExportProfile(std::path::PathBuf),
    /// Efface l'historique de la batterie (`batterie.csv`), tenu par le
    /// service ; ne touche pas au casque.
    ClearBatteryHistory,
}

/// Appareil appairé au casque (sans son adresse Bluetooth, qui reste dans
/// le service).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub name: String,
    pub connected: bool,
    pub this_device: bool,
    /// État brut de la fiche (`140E`), pour la page « Connexion et audio » ;
    /// 0 avec un service plus ancien.
    #[serde(default)]
    pub state: u8,
}

impl ConnectionInfo {
    /// Fiche du cœur pour `viewmodel::audio_view` (adresse fictive, jamais
    /// utilisée pour une commande).
    pub fn to_connection(&self, index: usize) -> momentum_core::device::Connection {
        let mut c = momentum_core::device::Connection::synthetic(
            u8::try_from(index).unwrap_or(u8::MAX),
            &self.name,
            self.connected,
        );
        c.state = self.state;
        c.this_device = self.this_device;
        c
    }
}

/// Événement du service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Event {
    Searching,
    Connected {
        name: String,
        /// Modèle non validé : lecture seule.
        read_only: Option<String>,
    },
    State(HeadsetState),
    Equalizer(EqState),
    Connections(Vec<ConnectionInfo>),
    Info(DeviceInfo),
    Exported(Result<std::path::PathBuf, String>),
    Lost(String),
    /// Réglage refusé par le casque (ou commande refusée par le service).
    Refused(String),
    SceneApplied(SceneReport),
}

/// État courant, pour une interface qui arrive.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Nom annoncé à la connexion ; `None` : recherche du casque.
    pub device_name: Option<String>,
    pub read_only: Option<String>,
    /// `None` tant que l'état n'a pas été lu.
    pub state: Option<HeadsetState>,
    pub equalizer: Option<EqState>,
    pub info: Option<DeviceInfo>,
    pub connections: Vec<ConnectionInfo>,
    /// Dernier incident (connexion perdue, réglage refusé).
    pub problem: Option<String>,
}

impl Snapshot {
    /// Met l'état à jour d'après un événement (même règle dans le service
    /// et dans une interface qui suit les événements).
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Searching => {
                self.device_name = None;
                self.state = None;
            }
            Event::Connected { name, read_only } => {
                self.device_name = Some(name.clone());
                self.read_only = read_only.clone();
                self.state = None;
                self.problem = None;
            }
            Event::State(s) => self.state = Some(*s),
            Event::Equalizer(eq) => self.equalizer = Some(eq.clone()),
            Event::Connections(list) => self.connections = list.clone(),
            Event::Info(info) => self.info = Some(info.clone()),
            Event::Lost(reason) => {
                self.state = None;
                self.problem = Some(format!("Connexion perdue : {reason}"));
            }
            Event::Refused(reason) => self.problem = Some(format!("Réglage refusé : {reason}")),
            Event::Exported(_) | Event::SceneApplied(_) => {}
        }
    }
}

/// Erreur d'un client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("service OpenHeadset Control absent (momentum-daemon lancé ?)")]
    NoService,
    #[error("service : {0}")]
    Service(String),
    #[error("message illisible : {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_events_round_trip_through_json() {
        for command in [
            Command::SetMode(NoiseModeRequest::Custom),
            Command::SetEqGains(vec![10, -20, 0]),
            Command::ToggleConnection {
                index: 1,
                name: "Téléphone".into(),
                connect: false,
            },
            Command::ApplyScene("Travail".into()),
        ] {
            let json = serde_json::to_string(&command).unwrap();
            assert_eq!(serde_json::from_str::<Command>(&json).unwrap(), command);
        }
        let event = Event::State(HeadsetState {
            battery: 80,
            ..HeadsetState::default()
        });
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), event);
    }

    #[test]
    fn snapshot_follows_events() {
        let mut s = Snapshot::default();
        s.apply(&Event::Connected {
            name: "MOMENTUM 5".into(),
            read_only: None,
        });
        s.apply(&Event::State(HeadsetState {
            battery: 55,
            ..HeadsetState::default()
        }));
        assert_eq!(s.state.map(|x| x.battery), Some(55));
        s.apply(&Event::Lost("délai".into()));
        assert!(s.state.is_none());
        assert_eq!(s.problem.as_deref(), Some("Connexion perdue : délai"));
        // Une reconnexion efface l'incident.
        s.apply(&Event::Connected {
            name: "MOMENTUM 5".into(),
            read_only: None,
        });
        assert!(s.problem.is_none());
    }
}
