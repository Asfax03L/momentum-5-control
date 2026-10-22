//! Transport Linux : bus de session D-Bus, nom `org.openheadset.Control`,
//! interface `org.openheadset.Control1` sur `/org/openheadset/Control`.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use momentum_client::{BUS_NAME, Command, OBJECT_PATH};
use momentum_service::worker::Job;
use zbus::object_server::SignalEmitter;

use crate::service::{self, Shared};

struct Control {
    commands: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
}

#[zbus::interface(name = "org.openheadset.Control1")]
impl Control {
    /// Commande JSON de l'API (`momentum_client::Command`), sans suivi.
    fn command(&self, json: String) -> zbus::fdo::Result<()> {
        self.submit(json, 0).map(|_| ())
    }

    /// Commande suivie : rend son numéro ; le résultat arrive dans
    /// l'événement `Done`. `session` : connexion vue par l'interface (0 :
    /// la connexion courante). Refusée tout de suite si le casque est absent,
    /// en lecture seule ou si la connexion a changé.
    fn submit(&self, json: String, session: u64) -> zbus::fdo::Result<u64> {
        let command: Command = serde_json::from_str(&json)
            .map_err(|e| zbus::fdo::Error::InvalidArgs(format!("commande illisible : {e}")))?;
        let mut shared = self
            .shared
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("état du service inaccessible".to_string()))?;
        service::submit(command, session, &mut shared, &self.commands)
            .map_err(zbus::fdo::Error::Failed)
    }

    /// État courant en JSON (`momentum_client::Snapshot`).
    fn snapshot(&self) -> zbus::fdo::Result<String> {
        let shared = self
            .shared
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("état du service inaccessible".to_string()))?;
        serde_json::to_string(&shared.snapshot).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Chaque événement du worker, en JSON (`momentum_client::Event`).
    #[zbus(signal)]
    async fn event(emitter: &SignalEmitter<'_>, json: &str) -> zbus::Result<()>;

    /// Casque connecté et état lu.
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.shared.lock().is_ok_and(|s| s.snapshot.state.is_some())
    }

    /// Batterie en pourcentage (-1 : inconnue).
    #[zbus(property)]
    fn battery(&self) -> i32 {
        self.shared
            .lock()
            .ok()
            .and_then(|s| s.snapshot.state.map(|st| i32::from(st.battery)))
            .unwrap_or(-1)
    }

    /// Nom du casque (vide : recherche).
    #[zbus(property)]
    fn name(&self) -> String {
        self.shared
            .lock()
            .ok()
            .and_then(|s| {
                s.snapshot
                    .info
                    .as_ref()
                    .and_then(|i| i.name.clone())
                    .or_else(|| s.snapshot.device_name.clone())
            })
            .unwrap_or_default()
    }
}

pub fn run() -> anyhow::Result<()> {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (commands, commands_rx) = mpsc::channel::<Job>();
    let connection = zbus::blocking::connection::Builder::session()
        .context("bus de session D-Bus")?
        .name(BUS_NAME)
        .context("nom D-Bus")?
        .serve_at(
            OBJECT_PATH,
            Control {
                commands,
                shared: Arc::clone(&shared),
            },
        )
        .context("objet D-Bus")?
        .build()
        .context("service déjà lancé, ou bus de session indisponible")?;
    let iface = connection
        .object_server()
        .interface::<_, Control>(OBJECT_PATH)
        .context("interface D-Bus")?;
    tracing::info!(nom = BUS_NAME, "service OpenHeadset Control prêt");

    let events = service::spawn_worker(commands_rx, &shared)?;
    service::pump(events, &shared, |api_event| {
        let emitter = iface.signal_emitter();
        match serde_json::to_string(api_event) {
            Ok(json) => {
                if let Err(e) = zbus::block_on(Control::event(emitter, &json)) {
                    tracing::warn!(%e, "signal D-Bus");
                }
            }
            Err(e) => tracing::warn!(%e, "événement non sérialisable"),
        }
        let control = iface.get();
        let _ = zbus::block_on(control.connected_changed(emitter));
        let _ = zbus::block_on(control.battery_changed(emitter));
        let _ = zbus::block_on(control.name_changed(emitter));
    });
    drop(connection);
    Ok(())
}
