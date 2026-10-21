//! Client du service `momentum-daemon` par le canal nommé Windows
//! (protocole [`crate::wire`]). Même interface que le client D-Bus de Linux.

use std::time::Duration;

use momentum_core::pipe::{ClientOutcome, request_named};

use crate::wire::{self, Reply, Request};
use crate::{ClientError, Command, Event, Snapshot};

/// Délai de réponse du service à une requête.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Intervalle de relève des événements.
const POLL: Duration = Duration::from_millis(150);

fn call(request: &Request) -> Result<Reply, ClientError> {
    let line = serde_json::to_string(request)? + "\n";
    match request_named(wire::pipe_name, &line, REQUEST_TIMEOUT).map_err(ClientError::Service)? {
        ClientOutcome::NoServer => Err(ClientError::NoService),
        ClientOutcome::Reply(text) => match serde_json::from_str(&text)? {
            Reply::Error(reason) => Err(ClientError::Service(reason)),
            reply => Ok(reply),
        },
    }
}

fn unexpected(reply: Reply) -> ClientError {
    ClientError::Service(format!("réponse inattendue du service : {reply:?}"))
}

/// Connexion au service (sans état : une requête par appel).
pub struct Client;

impl Client {
    /// Vérifie que le service répond.
    pub fn connect() -> Result<Self, ClientError> {
        let client = Self;
        client.snapshot()?;
        Ok(client)
    }

    /// État courant du casque vu par le service.
    pub fn snapshot(&self) -> Result<Snapshot, ClientError> {
        match call(&Request::Snapshot)? {
            Reply::Snapshot { snapshot, .. } => Ok(*snapshot),
            other => Err(unexpected(other)),
        }
    }

    /// Envoie une commande ; la réponse du casque arrive en [`Event`].
    pub fn send(&self, command: &Command) -> Result<(), ClientError> {
        match call(&Request::Command(command.clone()))? {
            Reply::Ok => Ok(()),
            other => Err(unexpected(other)),
        }
    }

    /// Événements du service, dans l'ordre (bloquant). L'itération s'arrête
    /// si le service disparaît ou si des événements ont été perdus : relire
    /// alors l'état avec [`Client::snapshot`].
    pub fn events(&self) -> Result<impl Iterator<Item = Event> + use<>, ClientError> {
        let mut seq = match call(&Request::Events { after: None })? {
            Reply::Events { seq, .. } => seq,
            other => return Err(unexpected(other)),
        };
        let mut pending = std::collections::VecDeque::new();
        Ok(std::iter::from_fn(move || {
            loop {
                if let Some(event) = pending.pop_front() {
                    return Some(event);
                }
                std::thread::sleep(POLL);
                match call(&Request::Events { after: Some(seq) }) {
                    Ok(Reply::Events {
                        reset: false,
                        seq: last,
                        events,
                    }) => {
                        seq = last;
                        pending.extend(events);
                    }
                    _ => return None,
                }
            }
        }))
    }
}
