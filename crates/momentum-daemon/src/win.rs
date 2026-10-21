//! Transport Windows : canal nommé de l'utilisateur
//! (`\\.\pipe\openheadset-control-<SID>`, protocole `momentum_client::wire`).
//! Une requête par connexion ; les interfaces relèvent les événements
//! numérotés (voir `wire::EventLog`).

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use momentum_client::wire::{self, EventLog, Reply, Request};
use momentum_core::pipe::PipeServer;
use momentum_service::worker::WorkerCommand;

use crate::service::{self, Shared};

/// Réponse à une requête d'une interface.
fn handle(
    line: &str,
    shared: &Mutex<Shared>,
    log: &Mutex<EventLog>,
    commands: &Sender<WorkerCommand>,
) -> Reply {
    let request: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return Reply::Error(format!("requête illisible : {e}")),
    };
    let unavailable = || Reply::Error("état du service inaccessible".to_string());
    match request {
        Request::Snapshot => {
            // Numéro lu avant l'état : un événement de l'intervalle sera
            // redonné, jamais perdu.
            let Ok(seq) = log.lock().map(|l| l.seq()) else {
                return unavailable();
            };
            match shared.lock() {
                Ok(s) => Reply::Snapshot {
                    seq,
                    snapshot: Box::new(s.snapshot.clone()),
                },
                Err(_) => unavailable(),
            }
        }
        Request::Events { after } => match log.lock() {
            Ok(l) => l.since(after),
            Err(_) => unavailable(),
        },
        Request::Command(command) => {
            let worker_command = match shared.lock() {
                Ok(s) => service::to_worker(command, &s),
                Err(_) => return unavailable(),
            };
            match worker_command {
                Ok(c) => match commands.send(c) {
                    Ok(()) => Reply::Ok,
                    Err(_) => Reply::Error("le worker s'est arrêté".to_string()),
                },
                Err(reason) => Reply::Error(reason),
            }
        }
    }
}

pub fn run() -> anyhow::Result<()> {
    // Première instance exigée : un service déjà lancé garde le canal.
    let server = PipeServer::create_named(wire::pipe_name)
        .context("canal du service (service déjà lancé ?)")?;
    momentum_service::desktop::register_notifications();

    let shared = Arc::new(Mutex::new(Shared::default()));
    let log = Arc::new(Mutex::new(EventLog::default()));
    let (commands, commands_rx) = mpsc::channel::<WorkerCommand>();
    {
        let shared = Arc::clone(&shared);
        let log = Arc::clone(&log);
        std::thread::Builder::new()
            .name("canal".into())
            .spawn(move || {
                loop {
                    let result = server.serve_one(|line| {
                        let reply = handle(line, &shared, &log, &commands);
                        serde_json::to_string(&reply).unwrap_or_else(|e| {
                            format!("{{\"Error\":\"réponse non sérialisable : {e}\"}}")
                        })
                    });
                    if let Err(e) = result {
                        tracing::debug!(%e, "client du canal");
                    }
                }
            })
            .context("lancement du thread du canal")?;
    }
    tracing::info!("service OpenHeadset Control prêt (canal nommé)");

    let events = service::spawn_worker(commands_rx)?;
    service::pump(events, &shared, |api_event| {
        if let Ok(mut l) = log.lock() {
            l.push(api_event.clone());
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use momentum_client::{Command, Event};

    #[test]
    fn requests_are_answered_from_the_shared_state() {
        let shared = Mutex::new(Shared::default());
        let log = Mutex::new(EventLog::default());
        let (commands, received) = mpsc::channel();
        log.lock().unwrap().push(Event::Searching);

        let snapshot = serde_json::to_string(&Request::Snapshot).unwrap();
        assert!(matches!(
            handle(&snapshot, &shared, &log, &commands),
            Reply::Snapshot { seq: 1, .. }
        ));
        // Sans casque, la commande est refusée et rien ne part au worker.
        let command = serde_json::to_string(&Request::Command(Command::SetAntiWind(true))).unwrap();
        assert!(matches!(
            handle(&command, &shared, &log, &commands),
            Reply::Error(_)
        ));
        assert!(received.try_recv().is_err());
        assert!(matches!(
            handle("n'importe quoi", &shared, &log, &commands),
            Reply::Error(_)
        ));
    }
}
