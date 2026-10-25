//! Transport Linux : bus de session D-Bus, nom `org.openheadset.Control`,
//! interface `org.openheadset.Control1` sur `/org/openheadset/Control`.

use momentum_core::i18n::tr;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use std::collections::HashMap;

use momentum_client::{BUS_NAME, Command, OBJECT_PATH, runner};
use momentum_core::paths;
use momentum_service::worker::Job;
use zbus::object_server::SignalEmitter;
use zbus::{OwnedVariant, Variant};

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
        let mut shared = self.shared.lock().map_err(|_| {
            zbus::fdo::Error::Failed(tr("état du service inaccessible").to_string())
        })?;
        service::submit(command, session, &mut shared, &self.commands)
            .map_err(zbus::fdo::Error::Failed)
    }

    /// État courant en JSON (`momentum_client::Snapshot`).
    fn snapshot(&self) -> zbus::fdo::Result<String> {
        let shared = self.shared.lock().map_err(|_| {
            zbus::fdo::Error::Failed(tr("état du service inaccessible").to_string())
        })?;
        serde_json::to_string(&shared.snapshot).map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
    }

    /// Scènes de `scenes.txt` et celles du panneau rapide (`panneau.txt`),
    /// en JSON : `{"names": [...], "panel": [...]}` (widget Plasma).
    fn scenes(&self) -> zbus::fdo::Result<String> {
        let (list, _) = paths::load_scenes();
        let names: Vec<String> = list.into_iter().map(|s| s.name).collect();
        let panel = paths::load_panel_prefs().shown_scenes(&names);
        serde_json::to_string(&serde_json::json!({ "names": names, "panel": panel }))
            .map_err(|e| zbus::fdo::Error::Failed(e.to_string()))
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

/// Objet de la recherche du bureau (KRunner, `org.kde.krunner1`), déclaré
/// par `assets/linux/openheadset-control-krunner.desktop`.
const RUNNER_PATH: &str = "/runner";

/// Résultat KRunner : identifiant, texte, icône, catégorie, pertinence,
/// propriétés.
type RemoteMatch = (
    String,
    String,
    String,
    u32,
    f64,
    HashMap<String, OwnedVariant>,
);

/// Pertinence de catégorie de KRunner (`KRunner::QueryMatch`).
const CATEGORY_HIGHEST: u32 = 100;
const CATEGORY_MODERATE: u32 = 50;

/// Recherche du bureau : les résultats viennent de `momentum_client::runner`
/// et chaque choix part par [`service::submit`], comme une commande d'une
/// interface (mêmes refus).
struct Runner {
    commands: Sender<Job>,
    shared: Arc<Mutex<Shared>>,
}

#[zbus::interface(name = "org.kde.krunner1")]
impl Runner {
    /// Aucune action secondaire.
    fn actions(&self) -> Vec<(String, String, String)> {
        Vec::new()
    }

    #[zbus(name = "Match")]
    fn find(&self, query: String) -> Vec<RemoteMatch> {
        let (list, _) = paths::load_scenes();
        let names: Vec<String> = list.into_iter().map(|s| s.name).collect();
        let battery = self
            .shared
            .lock()
            .ok()
            .and_then(|s| s.snapshot.state.map(|st| st.battery));
        runner::matches(&query, &names, battery)
            .into_iter()
            .map(|m| {
                let mut properties = HashMap::new();
                if let Ok(subtext) = OwnedVariant::try_from(Variant::from(m.subtext)) {
                    properties.insert("subtext".to_string(), subtext);
                }
                let category = if m.exact {
                    CATEGORY_HIGHEST
                } else {
                    CATEGORY_MODERATE
                };
                (
                    m.id,
                    m.text,
                    "org.openheadset.control".to_string(),
                    category,
                    m.relevance,
                    properties,
                )
            })
            .collect()
    }

    fn run(&self, match_id: String, _action_id: String) {
        let Some(command) = runner::command_for(&match_id) else {
            return;
        };
        let result = match self.shared.lock() {
            Ok(mut shared) => service::submit(command, 0, &mut shared, &self.commands).map(|id| {
                shared.runner_requests.push(id);
            }),
            Err(_) => Err(tr("état du service inaccessible").to_string()),
        };
        if let Err(reason) = result {
            momentum_service::desktop::show_toast(
                "OpenHeadset Control",
                &format!("refusé : {reason}"),
            );
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    let (commands, commands_rx) = mpsc::channel::<Job>();
    let shared = Arc::new(Mutex::new(Shared {
        battery_file: momentum_core::paths::battery_file(),
        journal_file: momentum_core::paths::journal_file(),
        commands: Some(commands.clone()),
        ..Shared::default()
    }));
    // zbus 6 n'a plus d'API bloquante : la connexion est construite dans
    // `block_on`, puis servie par zbus entre deux appels.
    let runner = Runner {
        commands: commands.clone(),
        shared: Arc::clone(&shared),
    };
    let control = Control {
        commands,
        shared: Arc::clone(&shared),
    };
    // Dans le Flatpak, le fichier d'activation porte l'identifiant de
    // l'application (seul nom que Flatpak exporte) : le service le prend
    // aussi, sinon le bus considérerait l'activation comme échouée.
    let flatpak_id = std::env::var("FLATPAK_ID").ok().filter(|id| id != BUS_NAME);
    let (connection, iface) = zbus::block_on(async {
        let mut builder = zbus::connection::Builder::session().name(BUS_NAME);
        if let Some(id) = &flatpak_id {
            builder = builder.name(id.as_str());
        }
        let connection = builder
            .serve_at(OBJECT_PATH, control)
            .serve_at(RUNNER_PATH, runner)
            .build()
            .await
            .context("service déjà lancé, ou bus de session indisponible")?;
        let iface = connection
            .object_server()
            .interface::<_, Control>(OBJECT_PATH)
            .await
            .context("interface D-Bus")?;
        anyhow::Ok((connection, iface))
    })?;
    tracing::info!(nom = BUS_NAME, "service OpenHeadset Control prêt");

    let events = service::spawn_worker(commands_rx, &shared)?;
    service::spawn_scheduler(Arc::clone(&shared))?;
    service::pump(events, &shared, |api_event| {
        let json = serde_json::to_string(api_event);
        zbus::block_on(async {
            let emitter = iface.signal_emitter();
            match json {
                Ok(json) => {
                    if let Err(e) = Control::event(emitter, &json).await {
                        tracing::warn!(%e, "signal D-Bus");
                    }
                }
                Err(e) => tracing::warn!(%e, "événement non sérialisable"),
            }
            let control = iface.get().await;
            let _ = control.connected_changed(emitter).await;
            let _ = control.battery_changed(emitter).await;
            let _ = control.name_changed(emitter).await;
        });
    });
    drop(connection);
    Ok(())
}
