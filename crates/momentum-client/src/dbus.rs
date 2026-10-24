//! Client D-Bus (bus de session) du service `momentum-daemon`.
//!
//! Interface `org.openheadset.Control1` sur `/org/openheadset/Control` :
//! - `Command(s json)` : envoie une [`Command`] (sans suivi) ;
//! - `Submit(s json, t session) -> t id` : commande suivie, résultat dans
//!   l'événement `Done { id }` ;
//! - `Snapshot() -> s json` : état courant ([`Snapshot`]) ;
//! - `Scenes() -> s json` : scènes et celles du panneau rapide (widget) ;
//! - signal `Event(s json)` : chaque [`Event`] du worker ;
//! - propriétés `Connected`, `Battery`, `Name` (pour un widget ou un script).
//!
//! zbus 6 n'a plus d'API bloquante : chaque appel attend sa réponse avec
//! `zbus::block_on`, la connexion continue d'être servie entre deux appels.

use std::pin::Pin;

use futures_core::Stream;
use zbus::proxy;
use zbus::{Connection, block_on};

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

/// Flatpak n'exporte un fichier d'activation D-Bus que sous l'identifiant
/// de l'application : dans le Flatpak, le service y est déclaré sous ce nom
/// (`$FLATPAK_ID`) et porte aussi `org.openheadset.Control`. `true` :
/// service démarré par ce nom.
fn start_flatpak_service(proxy: &ControlProxy<'static>) -> bool {
    let Some(app_id) = std::env::var("FLATPAK_ID").ok().filter(|id| id != BUS_NAME) else {
        return false;
    };
    block_on(async {
        let bus = zbus::fdo::DBusProxy::new(proxy.inner().connection()).await?;
        let name = zbus::names::WellKnownName::try_from(app_id.as_str())?;
        bus.start_service_by_name(name, 0).await?;
        zbus::Result::Ok(())
    })
    .is_ok()
}

/// Connexion au service sur le bus de session.
pub struct Client {
    proxy: ControlProxy<'static>,
}

impl Client {
    /// Ouvre le bus de session et vérifie que le service répond.
    pub fn connect() -> Result<Self, ClientError> {
        let proxy = block_on(async {
            let connection = Connection::session().await?;
            ControlProxy::builder(&connection)
                .destination(BUS_NAME)
                .path(OBJECT_PATH)
                .build()
                .await
        })
        .map_err(service_error)?;
        let client = Self { proxy };
        match client.snapshot() {
            Err(ClientError::NoService) if start_flatpak_service(&client.proxy) => {
                client.snapshot()?;
            }
            result => {
                result?;
            }
        }
        Ok(client)
    }

    /// État courant du casque vu par le service.
    pub fn snapshot(&self) -> Result<Snapshot, ClientError> {
        let json = block_on(self.proxy.snapshot()).map_err(service_error)?;
        Ok(serde_json::from_str(&json)?)
    }

    /// Envoie une commande ; la réponse du casque arrive en [`ApiEvent`].
    pub fn send(&self, command: &Command) -> Result<(), ClientError> {
        let json = serde_json::to_string(command)?;
        block_on(self.proxy.command(&json)).map_err(service_error)
    }

    /// Envoie une commande suivie ; son résultat arrive en
    /// `ApiEvent::Done` avec le numéro rendu. `Err` : refusée tout de suite.
    pub fn submit(&self, command: &Command, session: u64) -> Result<u64, ClientError> {
        let json = serde_json::to_string(command)?;
        block_on(self.proxy.submit(&json, session)).map_err(service_error)
    }

    /// Événements du service ([`ApiEvent`]), dans l'ordre (bloquant). Un message illisible
    /// est ignoré.
    pub fn events(&self) -> Result<impl Iterator<Item = ApiEvent> + use<>, ClientError> {
        let mut stream = block_on(self.proxy.receive_event()).map_err(service_error)?;
        let signals = std::iter::from_fn(move || {
            block_on(std::future::poll_fn(|cx| {
                Pin::new(&mut stream).poll_next(cx)
            }))
        });
        Ok(signals.filter_map(|signal| {
            let args = signal.args().ok()?;
            serde_json::from_str::<ApiEvent>(&args.json).ok()
        }))
    }
}
