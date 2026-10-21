//! Serveur du canal local : reçoit les requêtes de `ctl` et les transmet au
//! worker, seul propriétaire de la connexion GAIA.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

use momentum_core::ipc::{Reply, Request, parse_request, state_json};
use momentum_core::paths;
use momentum_core::pipe::PipeServer;
use momentum_core::scenes;
use momentum_core::ui::describe_state;
use tracing::{debug, info, warn};

use crate::worker::WorkerCommand;

/// Attente de l'état courant (le worker répond entre deux échanges).
const STATUS_TIMEOUT: Duration = Duration::from_secs(10);
/// Attente d'une scène : quelques écritures et relectures.
const SCENE_TIMEOUT: Duration = Duration::from_secs(60);

/// Crée le pipe et le sert dans un thread. En cas d'échec (nom déjà pris,
/// par exemple), l'icône fonctionne sans canal.
pub fn spawn(commands: Sender<WorkerCommand>) {
    let server = match PipeServer::create() {
        Ok(s) => s,
        Err(e) => {
            warn!(%e, "canal local indisponible : ctl devra ouvrir sa propre connexion");
            return;
        }
    };
    let started = std::thread::Builder::new()
        .name("canal-local".into())
        .spawn(move || {
            info!("canal local prêt");
            loop {
                if let Err(e) = server.serve_one(|line| handle(line, &commands).encode()) {
                    debug!(%e, "client du canal local abandonné");
                }
            }
        });
    if let Err(e) = started {
        warn!(%e, "lancement du canal local");
    }
}

fn handle(line: &str, commands: &Sender<WorkerCommand>) -> Reply {
    let request = match parse_request(line) {
        Ok(r) => r,
        Err(e) => return Reply::Error(e),
    };
    debug!(?request, "requête du canal local");
    match request.clone() {
        Request::Status | Request::StatusText => {
            let (tx, rx) = mpsc::channel();
            if commands.send(WorkerCommand::Query(tx)).is_err() {
                return Reply::Error("l'icône s'arrête".to_string());
            }
            let state = match rx.recv_timeout(STATUS_TIMEOUT) {
                Ok(state) => Some(state),
                // Hors session, le worker abandonne la demande.
                Err(RecvTimeoutError::Disconnected) => None,
                Err(RecvTimeoutError::Timeout) => {
                    return Reply::Error(
                        "l'icône n'a pas répondu (connexion au casque en cours ?)".to_string(),
                    );
                }
            };
            Reply::Ok(match (request, state) {
                (Request::Status, state) => state_json(state.as_ref()),
                (_, Some(state)) => describe_state(&state),
                (_, None) => "Casque non connecté (l'icône le recherche).".to_string(),
            })
        }
        Request::ListScenes => {
            let (list, errors) = paths::load_scenes();
            let mut body: Vec<String> = list.into_iter().map(|s| s.name).collect();
            body.extend(errors.into_iter().map(|e| format!("# {e}")));
            Reply::Ok(body.join("\n") + "\n")
        }
        Request::ApplyScene(name) => {
            let (list, _) = paths::load_scenes();
            let Some(scene) = scenes::find(&list, &name).cloned() else {
                return Reply::Error(format!("scène « {name} » introuvable (voir ctl scenes)"));
            };
            let (tx, rx) = mpsc::channel();
            let command = WorkerCommand::ApplyScene {
                scene,
                presets: paths::scene_presets(),
                reply: Some(tx),
            };
            if commands.send(command).is_err() {
                return Reply::Error("l'icône s'arrête".to_string());
            }
            match rx.recv_timeout(SCENE_TIMEOUT) {
                Ok(report) => Reply::Ok(format!(
                    "{}\n{}\n",
                    if report.is_complete() {
                        "complet"
                    } else {
                        "partiel"
                    },
                    report.details()
                )),
                Err(RecvTimeoutError::Disconnected) => {
                    Reply::Error("casque non connecté".to_string())
                }
                Err(RecvTimeoutError::Timeout) => Reply::Error(
                    "pas de rapport de l'icône dans le délai ; relire l'état avant de réessayer"
                        .to_string(),
                ),
            }
        }
    }
}
