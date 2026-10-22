//! Client D-Bus (bus de session) du service `momentum-daemon`.
//!
//! Interface `org.openheadset.Control1` sur `/org/openheadset/Control` :
//! - `Command(s json)` : envoie une [`Command`] (sans suivi) ;
//! - `Submit(s json, t session) -> t id` : commande suivie, résultat dans
//!   l'événement `Done { id }` ;
//! - `Snapshot() -> s json` : état courant ([`Snapshot`]) ;
//! - signal `Event(s json)` : chaque [`Event`] du worker ;
//! - propriétés `Connected`, `Battery`, `Name` (pour un widget ou un script).

use zbus::blocking::Connection;
use zbus::proxy;

// `Event` est aussi le nom du type de signal généré par le proxy.
use crate::{BUS_NAME, ClientError, Command, Event as ApiEvent, OBJECT_PATH, Snapshot};

#[proxy(
    interface = "org.openheadset.Control1",
    default_service = "org.openheadset.Control",
    default_path = "/org/openheadset/Control"
)]
trait Control {
    fn command(&self, json: &str) -> zbus::Result<()>;
    fn submit(&self, json: &str, session: u64) -> zbus::Result<u64>;
    fn snapshot(&self) -> zbus::Result<String>;
    #[zbus(signal)]
    fn event(&self, json: String) -> zbus::Result<()>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn battery(&self) -> zbus::Result<i32>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
}

fn service_error(e: zbus::Error) -> ClientError {
    match &e {
        zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.ServiceUnknown"
                || name.as_str() == "org.freedesktop.DBus.Error.NameHasNoOwner" =>
        {
            ClientError::NoService
        }
        zbus::Error::FDO(fdo) if matches!(**fdo, zbus::fdo::Error::ServiceUnknown(_)) => {
            ClientError::NoService
        }
        // Refus du service : sa raison seule, sans le nom de l'erreur D-Bus.
        zbus::Error::MethodError(_, Some(reason), _) => ClientError::Service(reason.clone()),
        zbus::Error::FDO(fdo) => match &**fdo {
            zbus::fdo::Error::Failed(reason) | zbus::fdo::Error::InvalidArgs(reason) => {
                ClientError::Service(reason.clone())
            }
            _ => ClientError::Service(e.to_string()),
        },
        _ => ClientError::Service(e.to_string()),
    }
}

/// Connexion au service sur le bus de session.
pub struct Client {
    proxy: ControlProxyBlocking<'static>,
}

impl Client {
    /// Ouvre le bus de session et vérifie que le service répond.
    pub fn connect() -> Result<Self, ClientError> {
        let connection = Connection::session().map_err(service_error)?;
        let proxy = ControlProxyBlocking::builder(&connection)
            .destination(BUS_NAME)
            .map_err(service_error)?
            .path(OBJECT_PATH)
            .map_err(service_error)?
            .build()
            .map_err(service_error)?;
        let client = Self { proxy };
        client.snapshot()?;
        Ok(client)
    }

    /// État courant du casque vu par le service.
    pub fn snapshot(&self) -> Result<Snapshot, ClientError> {
        let json = self.proxy.snapshot().map_err(service_error)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Envoie une commande ; la réponse du casque arrive en [`ApiEvent`].
    pub fn send(&self, command: &Command) -> Result<(), ClientError> {
        let json = serde_json::to_string(command)?;
        self.proxy.command(&json).map_err(service_error)
    }

    /// Envoie une commande suivie ; son résultat arrive en
    /// `ApiEvent::Done` avec le numéro rendu. `Err` : refusée tout de suite.
    pub fn submit(&self, command: &Command, session: u64) -> Result<u64, ClientError> {
        let json = serde_json::to_string(command)?;
        self.proxy.submit(&json, session).map_err(service_error)
    }

    /// Événements du service ([`ApiEvent`]), dans l'ordre (bloquant). Un message illisible
    /// est ignoré.
    pub fn events(&self) -> Result<impl Iterator<Item = ApiEvent> + use<>, ClientError> {
        let stream = self.proxy.receive_event().map_err(service_error)?;
        Ok(stream.filter_map(|signal| {
            let args = signal.args().ok()?;
            serde_json::from_str::<ApiEvent>(&args.json).ok()
        }))
    }
}
