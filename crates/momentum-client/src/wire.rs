//! Protocole du canal nommé Windows entre une interface et le service.
//!
//! Une requête par connexion : une ligne JSON ([`Request`]), puis une
//! réponse JSON ([`Reply`]). Les événements sont numérotés ; une interface
//! demande ceux qui suivent le dernier numéro reçu ([`Request::Events`]).
//! Le service en garde [`EVENT_LOG_CAPACITY`] : une interface trop en retard
//! reçoit `reset` et relit l'état complet.
//!
//! Le canal est propre à l'utilisateur (SID dans le nom, accès réservé,
//! clients distants refusés : voir `momentum_core::pipe`).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::{Command, Event, Snapshot};

/// Événements gardés pour les interfaces qui les relèvent.
pub const EVENT_LOG_CAPACITY: usize = 512;

/// Nom du canal du service pour un utilisateur (SID textuel).
pub fn pipe_name(user_sid: &str) -> String {
    format!(r"\\.\pipe\openheadset-control-{user_sid}")
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Request {
    /// État courant et numéro du dernier événement qu'il contient.
    Snapshot,
    /// Événements après `after` ; `None` : seulement le numéro courant
    /// (point de départ d'un abonnement).
    Events {
        after: Option<u64>,
    },
    Command(Command),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Reply {
    Snapshot {
        seq: u64,
        snapshot: Box<Snapshot>,
    },
    Events {
        seq: u64,
        events: Vec<Event>,
        /// Événements perdus (interface trop en retard) : relire l'état.
        reset: bool,
    },
    Ok,
    Error(String),
}

/// Derniers événements du service, numérotés à partir de 1.
#[derive(Debug, Default)]
pub struct EventLog {
    seq: u64,
    events: VecDeque<(u64, Event)>,
}

impl EventLog {
    pub fn push(&mut self, event: Event) {
        self.seq += 1;
        self.events.push_back((self.seq, event));
        if self.events.len() > EVENT_LOG_CAPACITY {
            self.events.pop_front();
        }
    }

    /// Numéro du dernier événement.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Réponse à [`Request::Events`].
    pub fn since(&self, after: Option<u64>) -> Reply {
        let Some(after) = after else {
            return Reply::Events {
                seq: self.seq,
                events: Vec::new(),
                reset: false,
            };
        };
        let oldest = self.events.front().map_or(self.seq + 1, |(n, _)| *n);
        // Un numéro du futur vient d'un service précédent (redémarré).
        let reset = after > self.seq || after + 1 < oldest;
        let events = if reset {
            Vec::new()
        } else {
            self.events
                .iter()
                .filter(|(n, _)| *n > after)
                .map(|(_, e)| e.clone())
                .collect()
        };
        Reply::Events {
            seq: self.seq,
            events,
            reset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn events(reply: Reply) -> (u64, usize, bool) {
        match reply {
            Reply::Events { seq, events, reset } => (seq, events.len(), reset),
            other => panic!("réponse inattendue : {other:?}"),
        }
    }

    #[test]
    fn events_after_a_number() {
        let mut log = EventLog::default();
        assert_eq!(events(log.since(None)), (0, 0, false));
        log.push(Event::Searching);
        log.push(Event::Lost("délai".into()));
        assert_eq!(events(log.since(Some(0))), (2, 2, false));
        assert_eq!(events(log.since(Some(1))), (2, 1, false));
        assert_eq!(events(log.since(Some(2))), (2, 0, false));
        // Service redémarré : numéro inconnu.
        assert_eq!(events(log.since(Some(9))), (2, 0, true));
    }

    #[test]
    fn lagging_client_is_reset() {
        let mut log = EventLog::default();
        for _ in 0..EVENT_LOG_CAPACITY + 10 {
            log.push(Event::Searching);
        }
        assert!(events(log.since(Some(3))).2);
        let recent = log.seq() - 5;
        assert_eq!(events(log.since(Some(recent))), (log.seq(), 5, false));
    }

    #[test]
    fn requests_round_trip_through_json() {
        for request in [
            Request::Snapshot,
            Request::Events { after: Some(4) },
            Request::Command(Command::SetAntiWind(true)),
        ] {
            let json = serde_json::to_string(&request).unwrap();
            assert!(!json.contains('\n'));
            assert_eq!(serde_json::from_str::<Request>(&json).unwrap(), request);
        }
    }
}
