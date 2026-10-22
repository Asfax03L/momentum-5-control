//! Transport Windows : canal nommé de l'utilisateur
//! (`\\.\pipe\openheadset-control-<SID>`, protocole `momentum_client::wire`).
//! Une requête par connexion ; les interfaces relèvent les événements
//! numérotés (voir `wire::EventLog`).

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use momentum_client::wire::{self, EventLog, Reply, Request};
use momentum_core::pipe::PipeServer;
use momentum_service::worker::Job;

use crate::service::{self, Shared};

/// Réponse à une requête d'une interface.
fn handle(
    line: &str,
    shared: &Mutex<Shared>,
    log: &Mutex<EventLog>,
    commands: &Sender<Job>,
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
        Request::Command(command) => match shared.lock() {
            Ok(mut s) => match service::submit(command, 0, &mut s, commands) {
                Ok(_) => Reply::Ok,
                Err(reason) => Reply::Error(reason),
            },
            Err(_) => unavailable(),
        },
        Request::Submit { command, session } => match shared.lock() {
            Ok(mut s) => match service::submit(command, session, &mut s, commands) {
                Ok(id) => Reply::Submitted { id },
                Err(reason) => Reply::Error(reason),
            },
            Err(_) => unavailable(),
        },
    }
}

/// Réponse JSON à une requête du canal, refus de lecture compris (requête
/// trop longue ou non UTF-8) : le client lit toujours une `Reply`.
fn answer(
    line: Result<&str, &str>,
    shared: &Mutex<Shared>,
    log: &Mutex<EventLog>,
    commands: &Sender<Job>,
) -> String {
    let reply = match line {
        Ok(line) => handle(line, shared, log, commands),
        Err(reason) => Reply::Error(reason.to_string()),
    };
    serde_json::to_string(&reply)
        .unwrap_or_else(|e| format!("{{\"Error\":\"réponse non sérialisable : {e}\"}}"))
}

pub fn run() -> anyhow::Result<()> {
    // Première instance exigée : un service déjà lancé garde le canal.
    let server = PipeServer::create_named(wire::pipe_name)
        .context("canal du service (service déjà lancé ?)")?;
    momentum_service::desktop::register_notifications();

    let shared = Arc::new(Mutex::new(Shared::default()));
    let log = Arc::new(Mutex::new(EventLog::default()));
    let (commands, commands_rx) = mpsc::channel::<Job>();
    {
        let shared = Arc::clone(&shared);
        let log = Arc::clone(&log);
        std::thread::Builder::new()
            .name("canal".into())
            .spawn(move || {
                loop {
                    let result = server.serve_one(|line| answer(line, &shared, &log, &commands));
                    if let Err(e) = result {
                        tracing::debug!(%e, "client du canal");
                    }
                }
            })
            .context("lancement du thread du canal")?;
    }
    tracing::info!("service OpenHeadset Control prêt (canal nommé)");

    let events = service::spawn_worker(commands_rx, &shared)?;
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

    #[test]
    fn unreadable_requests_get_a_json_error() {
        let shared = Mutex::new(Shared::default());
        let log = Mutex::new(EventLog::default());
        let (commands, _received) = mpsc::channel();
        let json = answer(Err("requête trop longue"), &shared, &log, &commands);
        assert_eq!(
            serde_json::from_str::<Reply>(&json).unwrap(),
            Reply::Error("requête trop longue".to_string())
        );
    }
}
