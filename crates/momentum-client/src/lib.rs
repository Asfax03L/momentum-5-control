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
use momentum_core::i18n::{tr, trf};
pub use momentum_core::request::Outcome;
use momentum_core::scenes::SceneReport;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "linux")]
pub mod dbus;
pub mod demo;
pub mod edits;
pub mod essais;
pub mod menu;
#[cfg(windows)]
pub mod pipe;
pub mod runner;
pub mod tracker;
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
    /// Reconnexion par le Bluetooth du système ; aucune commande GAIA.
    ReconnectHeadset,
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
    /// Appareil de la liste [`Snapshot::connections`]. `device` :
    /// identifiant opaque ([`ConnectionInfo::id`]), qui désigne cet
    /// appareil-là même si la liste a changé ; sans lui (interface
    /// ancienne), la position et le nom, refusés si ce nom n'est pas unique.
    ToggleConnection {
        index: usize,
        name: String,
        connect: bool,
        #[serde(default)]
        device: Option<String>,
    },
    /// Scène du fichier `scenes.txt`, par son nom.
    ApplyScene(String),
    /// Sauvegarde brute du profil de personnalisation dans ce fichier.
    ExportProfile(std::path::PathBuf),
    /// Efface l'historique de la batterie (`batterie.csv`), tenu par le
    /// service ; ne touche pas au casque.
    ClearBatteryHistory,
    /// Efface le journal des événements (`journal.csv`), sans casque.
    ClearJournal,
}

/// Appareil appairé au casque (sans son adresse Bluetooth, qui reste dans
/// le service).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Identifiant opaque et stable (empreinte de l'adresse, qui reste dans
    /// le service) ; vide avec un service ancien.
    #[serde(default)]
    pub id: String,
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
    /// Recherche terminée sans casque (raison) ; le service réessaie.
    NotFound(String),
    Connected {
        name: String,
        /// Empreinte de l'identifiant choisi par le service, jamais l'adresse.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        headset_id: Option<String>,
        /// Raison de la lecture seule (modèle ou micrologiciel non validé).
        read_only: Option<String>,
        /// Numéro de cette connexion (croissant ; 0 avec un service ancien).
        #[serde(default)]
        session: u64,
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
    /// Résultat de la demande `id` rendue par `submit` : un seul par
    /// demande. `details` : compte rendu d'une scène, fichier d'un export.
    Done {
        id: u64,
        outcome: Outcome,
        #[serde(default)]
        details: Option<String>,
    },
}

/// État courant, pour une interface qui arrive.  Les données d'une connexion
/// ne survivent pas à sa fin : rien d'un ancien casque n'est présenté comme
/// actuel.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Correspondance audio locale ; absente avec un service ancien.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headset_id: Option<String>,
    /// Nom annoncé à la connexion ; `None` : recherche du casque.
    pub device_name: Option<String>,
    /// Raison de la lecture seule (modèle ou micrologiciel non validé).
    pub read_only: Option<String>,
    /// Numéro de la connexion courante, à joindre aux demandes
    /// (`submit`) ; 0 sans casque ou avec un service ancien.
    #[serde(default)]
    pub session: u64,
    /// `None` tant que l'état n'a pas été lu.
    pub state: Option<HeadsetState>,
    pub equalizer: Option<EqState>,
    pub info: Option<DeviceInfo>,
    pub connections: Vec<ConnectionInfo>,
    /// Dernier incident (connexion perdue, réglage refusé).
    pub problem: Option<String>,
}

impl Snapshot {
    /// Fréquences documentées du M5 qualifié, pas des fréquences lues par GAIA.
    pub fn eq_topology(&self) -> Option<momentum_core::presets::EqTopology> {
        if self.state.is_none() || self.read_only.is_some() {
            return None;
        }
        self.equalizer
            .as_ref()
            .and_then(momentum_core::presets::EqTopology::momentum5)
    }
    /// Met l'état à jour d'après un événement (même règle dans le service
    /// et dans une interface qui suit les événements).
    pub fn apply(&mut self, event: &Event) {
        match event {
            Event::Searching => {
                self.device_name = None;
                self.read_only = None;
                self.forget_headset();
            }
            Event::Connected {
                name,
                headset_id,
                read_only,
                session,
            } => {
                self.device_name = Some(name.clone());
                self.read_only = read_only.clone();
                self.session = *session;
                self.forget_headset();
                self.headset_id = headset_id.clone();
                self.problem = None;
            }
            Event::State(s) => self.state = Some(*s),
            Event::Equalizer(eq) => self.equalizer = Some(eq.clone()),
            Event::Connections(list) => self.connections = list.clone(),
            Event::Info(info) => self.info = Some(info.clone()),
            Event::NotFound(reason) => {
                self.problem = Some(format!("Casque introuvable : {reason}"));
            }
            Event::Lost(reason) => {
                self.forget_headset();
                self.problem = Some(format!("Connexion perdue : {reason}"));
            }
            Event::Refused(reason) => self.problem = Some(format!("Réglage refusé : {reason}")),
            Event::Exported(_) | Event::SceneApplied(_) | Event::Done { .. } => {}
        }
    }

    /// Oublie tout ce qui a été lu sur le casque (F07 de la revue).
    fn forget_headset(&mut self) {
        self.headset_id = None;
        self.state = None;
        self.equalizer = None;
        self.info = None;
        self.connections.clear();
    }
}

/// Erreur d'un client.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error(
        "{}",
        tr("service OpenHeadset Control absent (momentum-daemon lancé ?)")
    )]
    NoService,
    #[error("{}", trf("service : {}", &[&.0]))]
    Service(String),
    #[error("{}", trf("message illisible : {}", &[&.0]))]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_and_events_round_trip_through_json() {
        for command in [
            Command::ReconnectHeadset,
            Command::SetMode(NoiseModeRequest::Custom),
            Command::SetEqGains(vec![10, -20, 0]),
            Command::ToggleConnection {
                index: 1,
                name: "Téléphone".into(),
                connect: false,
                device: Some("0123456789abcdef".into()),
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

    /// Commandes écrites telles quelles par le widget Plasma
    /// (`plasma/…/main.qml`) : elles doivent rester lisibles par le service.
    #[test]
    fn plasma_widget_commands_are_valid() {
        for (json, command) in [
            (r#""ReconnectHeadset""#, Command::ReconnectHeadset),
            (
                r#"{"SetMode":"Adaptive"}"#,
                Command::SetMode(NoiseModeRequest::Adaptive),
            ),
            (
                r#"{"SetMode":"Custom"}"#,
                Command::SetMode(NoiseModeRequest::Custom),
            ),
            (
                r#"{"SetMode":"Off"}"#,
                Command::SetMode(NoiseModeRequest::Off),
            ),
            (r#"{"SetLevel":35}"#, Command::SetLevel(35)),
            (r#"{"SetAntiWind":true}"#, Command::SetAntiWind(true)),
            (r#"{"SetSpatial":false}"#, Command::SetSpatial(false)),
        ] {
            assert_eq!(
                serde_json::from_str::<Command>(json).unwrap(),
                command,
                "{json}"
            );
        }
    }

    #[test]
    fn snapshot_follows_events() {
        let mut s = Snapshot::default();
        s.apply(&Event::Connected {
            headset_id: None,
            name: "MOMENTUM 5".into(),
            read_only: None,
            session: 1,
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
            headset_id: None,
            name: "MOMENTUM 5".into(),
            read_only: None,
            session: 1,
        });
        assert!(s.problem.is_none());
    }

    #[test]
    fn charge_state_survives_ipc_but_not_a_connection_change() {
        use momentum_core::device::{ChargeReading, ChargerStatus};

        let state = ChargeReading {
            primary_status: ChargerStatus::Unknown(2),
            secondary_raw: Some(0),
        };
        let event = Event::State(HeadsetState {
            charge_state: Some(state),
            ..HeadsetState::default()
        });
        let event: Event = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        let mut snapshot = Snapshot::default();
        snapshot.apply(&event);
        assert_eq!(snapshot.state.unwrap().charge_state, Some(state));
        snapshot.apply(&Event::Lost("perte simulée".into()));
        assert!(snapshot.state.is_none());
        snapshot.apply(&event);
        snapshot.apply(&Event::Connected {
            headset_id: None,
            name: "Casque démo".into(),
            read_only: None,
            session: 2,
        });
        assert!(snapshot.state.is_none());
    }

    #[test]
    fn wearing_state_survives_ipc_but_not_a_connection_change() {
        use momentum_core::device::OnHeadState;

        for state in [
            OnHeadState::NotWorn,
            OnHeadState::Worn,
            OnHeadState::Unknown(1),
        ] {
            let event = Event::State(HeadsetState {
                on_head_state: Some(state),
                ..HeadsetState::default()
            });
            let json = serde_json::to_string(&event).unwrap();
            let event: Event = serde_json::from_str(&json).unwrap();
            let mut snapshot = Snapshot::default();
            snapshot.apply(&event);
            assert_eq!(snapshot.state.unwrap().on_head_state, Some(state));
            snapshot.apply(&Event::Lost("perte simulée".into()));
            assert!(snapshot.state.is_none());
            snapshot.apply(&event);
            snapshot.apply(&Event::Connected {
                headset_id: None,
                name: "Casque démo".into(),
                read_only: None,
                session: 2,
            });
            assert!(snapshot.state.is_none());
        }
        // Un service ancien omet ce champ ; absence et code inconnu sont distincts.
        let json = serde_json::to_value(HeadsetState::default()).unwrap();
        assert!(json.get("on_head_state").is_none());
        let old: HeadsetState = serde_json::from_value(json).unwrap();
        assert_eq!(old.on_head_state, None);
    }

    #[test]
    fn current_identity_is_opaque_optional_and_forgotten_after_disconnect() {
        let id = momentum_core::transport::choice::fingerprint("02:00:00:00:00:01");
        let event = Event::Connected {
            name: "Casque démo".into(),
            headset_id: Some(id.clone()),
            read_only: None,
            session: 1,
        };
        let mut snapshot = Snapshot::default();
        snapshot.apply(&event);
        assert_eq!(snapshot.headset_id.as_ref(), Some(&id));
        let text = serde_json::to_string(&snapshot).unwrap();
        assert!(!text.contains("02:00:"));
        snapshot.apply(&Event::Lost("perte fictive".into()));
        assert_eq!(snapshot.headset_id, None);
        snapshot.apply(&event);
        snapshot.apply(&Event::Searching);
        assert_eq!(snapshot.headset_id, None);
        snapshot.apply(&event);
        let old: Event =
            serde_json::from_str(r#"{"Connected":{"name":"Casque démo","read_only":null}}"#)
                .unwrap();
        snapshot.apply(&old);
        assert_eq!(snapshot.headset_id, None);
    }

    #[test]
    fn a_failed_search_is_shown_until_a_connection() {
        let mut s = Snapshot::default();
        s.apply(&Event::Searching);
        s.apply(&Event::NotFound(
            "aucun casque connecté avec service GAIA".into(),
        ));
        assert_eq!(
            s.problem.as_deref(),
            Some("Casque introuvable : aucun casque connecté avec service GAIA")
        );
        // Nouvel essai : l'incident reste affiché jusqu'à la connexion.
        s.apply(&Event::Searching);
        assert!(s.problem.is_some());
        s.apply(&Event::Connected {
            headset_id: None,
            name: "MOMENTUM 5".into(),
            read_only: None,
            session: 1,
        });
        assert!(s.problem.is_none());
    }

    #[test]
    fn a_new_connection_forgets_the_old_headset() {
        // F07 : rien de la connexion précédente n'est présenté comme actuel.
        let mut s = Snapshot::default();
        s.apply(&Event::Connected {
            headset_id: None,
            name: "Casque A".into(),
            read_only: None,
            session: 1,
        });
        s.apply(&Event::Equalizer(EqState {
            min_gain: -60,
            max_gain: 60,
            gains: vec![30; 8],
            bass_boost: true,
        }));
        s.apply(&Event::Info(DeviceInfo {
            firmware: Some("6.21.5".into()),
            ..DeviceInfo::default()
        }));
        s.apply(&Event::Connections(vec![ConnectionInfo::default()]));
        s.apply(&Event::Lost("hors de portée".into()));
        assert!(s.equalizer.is_none() && s.info.is_none() && s.connections.is_empty());
        s.apply(&Event::Equalizer(EqState::default()));
        s.apply(&Event::Connected {
            headset_id: None,
            name: "Casque B".into(),
            read_only: None,
            session: 2,
        });
        assert_eq!(s.session, 2);
        assert!(s.equalizer.is_none() && s.state.is_none());
    }

    /// Format JSON de l'API (contrat avec les interfaces et le widget).
    #[test]
    fn api_json_contract() {
        let done = Event::Done {
            id: 4,
            outcome: Outcome::Refused("casque absent".into()),
            details: None,
        };
        assert_eq!(
            serde_json::to_string(&done).unwrap(),
            r#"{"Done":{"id":4,"outcome":{"Refused":"casque absent"},"details":null}}"#
        );
        // Un service ancien n'envoie ni `session` ni `details`.
        let old: Event =
            serde_json::from_str(r#"{"Connected":{"name":"M5","read_only":null}}"#).unwrap();
        assert!(matches!(old, Event::Connected { session: 0, .. }));
        let old: Event = serde_json::from_str(r#"{"Done":{"id":1,"outcome":"Applied"}}"#).unwrap();
        assert!(matches!(old, Event::Done { details: None, .. }));
        let snapshot: Snapshot = serde_json::from_str(
            r#"{"device_name":null,"read_only":null,"state":null,"equalizer":null,"info":null,"connections":[],"problem":null}"#,
        )
        .unwrap();
        assert_eq!(snapshot.session, 0);
    }
}
