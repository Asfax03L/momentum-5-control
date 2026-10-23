//! Suivi des demandes d'une interface : envoyées, puis terminées par le
//! résultat qui porte leur numéro (`Event::Done`). Un autre événement ne
//! les termine pas (F01 de la revue), et un refus immédiat du service se
//! voit au lieu de laisser l'interface en attente (F03).

use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crate::{Event, Outcome};

/// Demandes en cours d'une interface.
#[derive(Debug, Default)]
pub struct Tracker {
    /// Envoyées, numéro pas encore reçu.
    in_flight: usize,
    /// Acceptées, résultat attendu.
    pending: Vec<u64>,
    /// Dernier résultat système appartenant à cette interface.
    notice: Option<String>,
    /// Résultats arrivés avant leur numéro (le casque peut répondre avant
    /// que le fil d'envoi ne transmette le numéro).
    early: Vec<(u64, Outcome)>,
}

/// Résultats d'avance gardés au plus (ceux des autres interfaces passent
/// aussi par là tant qu'une demande attend son numéro).
const EARLY_MAX: usize = 32;

impl Tracker {
    /// Une demande vient de partir.
    pub fn sent(&mut self) {
        self.in_flight += 1;
    }

    /// Une demande attend encore son numéro ou son résultat.
    pub fn busy(&self) -> bool {
        self.in_flight > 0 || !self.pending.is_empty()
    }

    /// Numéro rendu par le service ; `Some` : message d'échec si le
    /// résultat était déjà arrivé et n'est pas un succès.
    pub fn submitted(&mut self, id: u64) -> Option<String> {
        self.in_flight = self.in_flight.saturating_sub(1);
        if let Some(i) = self.early.iter().position(|(e, _)| *e == id) {
            let (_, outcome) = self.early.remove(i);
            return self.result(&outcome);
        }
        self.pending.push(id);
        None
    }

    /// Demande refusée tout de suite par le service : message à montrer.
    pub fn rejected(&mut self, reason: &str) -> String {
        self.in_flight = self.in_flight.saturating_sub(1);
        format!("refusé : {reason}")
    }

    /// Résultat publié par le service ; `Some` : message d'échec quand la
    /// demande est à cette interface et n'a pas abouti.
    pub fn done(&mut self, id: u64, outcome: &Outcome) -> Option<String> {
        if let Some(i) = self.pending.iter().position(|p| *p == id) {
            self.pending.remove(i);
            return self.result(outcome);
        }
        if self.in_flight > 0 {
            self.early.push((id, outcome.clone()));
            let excess = self.early.len().saturating_sub(EARLY_MAX);
            self.early.drain(..excess);
        }
        None
    }

    fn result(&mut self, outcome: &Outcome) -> Option<String> {
        if let Outcome::SystemRequested(message) = outcome {
            self.notice = Some(message.clone());
        }
        failure(outcome)
    }

    /// Message système réussi, uniquement pour une demande de cette interface.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Service perdu : les résultats attendus n'arriveront pas.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Attend le résultat de la demande `id` parmi les événements du service
/// (abonnement pris avant l'envoi) : tout autre événement est ignoré, même
/// un `Done` d'une autre demande. `None` : rien dans le délai (résultat
/// inconnu) ou service perdu.
pub fn wait_for(
    events: &Receiver<Event>,
    id: u64,
    timeout: Duration,
) -> Option<(Outcome, Option<String>)> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left).ok()? {
            Event::Done {
                id: done,
                outcome,
                details,
            } if done == id => return Some((outcome, details)),
            _ => {}
        }
    }
}

/// Code de sortie d'un outil en ligne de commande pour ce résultat :
/// 0 réussite confirmée, 1 refus ou annulation (rien n'a changé), 2 non
/// appliqué (relu autrement), 3 résultat inconnu (à relire).
pub fn exit_code(outcome: Option<&Outcome>) -> i32 {
    match outcome {
        Some(
            Outcome::Applied
            | Outcome::AlreadySatisfied
            | Outcome::Acknowledged
            | Outcome::SystemRequested(_),
        ) => 0,
        Some(Outcome::Refused(_) | Outcome::Cancelled(_)) => 1,
        Some(Outcome::NotApplied(_)) => 2,
        Some(Outcome::Unknown(_)) | None => 3,
    }
}

fn failure(outcome: &Outcome) -> Option<String> {
    (!outcome.is_success()).then(|| outcome.describe())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_notice_is_matched_even_when_done_precedes_the_number() {
        let outcome = Outcome::SystemRequested("paramètres Bluetooth ouverts (simulation)".into());
        for early in [false, true] {
            let mut tracker = Tracker::default();
            tracker.sent();
            tracker.done(99, &outcome);
            assert_eq!(tracker.take_notice(), None);
            if early {
                tracker.done(7, &outcome);
            }
            assert_eq!(tracker.submitted(7), None);
            if !early {
                tracker.done(7, &outcome);
            }
            assert_eq!(tracker.take_notice(), Some(outcome.describe()));
            assert_eq!(tracker.take_notice(), None);
            assert!(!tracker.busy());
            assert_eq!(exit_code(Some(&outcome)), 0);
        }
    }

    #[test]
    fn only_its_own_result_ends_a_request() {
        let mut t = Tracker::default();
        t.sent();
        assert!(t.busy());
        assert_eq!(t.submitted(7), None);
        // Résultat d'une autre interface : sans effet.
        assert_eq!(t.done(3, &Outcome::Refused("x".into())), None);
        assert!(t.busy());
        assert_eq!(t.done(7, &Outcome::Applied), None);
        assert!(!t.busy());
    }

    #[test]
    fn failures_and_refusals_are_reported() {
        let mut t = Tracker::default();
        t.sent();
        assert_eq!(t.rejected("casque absent"), "refusé : casque absent");
        assert!(!t.busy());
        t.sent();
        t.submitted(1);
        assert_eq!(
            t.done(1, &Outcome::Unknown("connexion perdue".into()))
                .as_deref(),
            Some("résultat inconnu : connexion perdue")
        );
        assert!(!t.busy());
    }

    #[test]
    fn a_result_before_its_number_is_kept() {
        let mut t = Tracker::default();
        t.sent();
        assert_eq!(
            t.done(4, &Outcome::NotApplied("relu autrement".into())),
            None
        );
        assert_eq!(
            t.submitted(4).as_deref(),
            Some("non appliqué : relu autrement")
        );
        assert!(!t.busy());
    }

    #[test]
    fn a_command_line_waits_for_its_own_result() {
        // F01/F13 : notification de batterie, résultat d'une autre demande,
        // puis refus tardif de la sienne : c'est ce refus qui compte.
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Event::State(momentum_core::device::HeadsetState::default()))
            .unwrap();
        tx.send(Event::Done {
            id: 8,
            outcome: Outcome::Applied,
            details: None,
        })
        .unwrap();
        tx.send(Event::Done {
            id: 9,
            outcome: Outcome::Refused("casque absent".into()),
            details: None,
        })
        .unwrap();
        let (outcome, _) = wait_for(&rx, 9, Duration::from_secs(1)).unwrap();
        assert_eq!(outcome, Outcome::Refused("casque absent".into()));
        assert_eq!(exit_code(Some(&outcome)), 1);
        // Rien pour la demande 10 : résultat inconnu, code 3.
        assert_eq!(wait_for(&rx, 10, Duration::from_millis(20)), None);
        assert_eq!(exit_code(None), 3);
        assert_eq!(exit_code(Some(&Outcome::Applied)), 0);
        assert_eq!(exit_code(Some(&Outcome::NotApplied("x".into()))), 2);
    }

    #[test]
    fn losing_the_service_ends_the_wait() {
        let mut t = Tracker::default();
        t.sent();
        t.submitted(2);
        t.reset();
        assert!(!t.busy());
    }
}
