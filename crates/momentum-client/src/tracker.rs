//! Suivi des demandes d'une interface : envoyées, puis terminées par le
//! résultat qui porte leur numéro (`Event::Done`). Un autre événement ne
//! les termine pas (F01 de la revue), et un refus immédiat du service se
//! voit au lieu de laisser l'interface en attente (F03).

use crate::Outcome;

/// Demandes en cours d'une interface.
#[derive(Debug, Default)]
pub struct Tracker {
    /// Envoyées, numéro pas encore reçu.
    in_flight: usize,
    /// Acceptées, résultat attendu.
    pending: Vec<u64>,
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
            return failure(&outcome);
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
            return failure(outcome);
        }
        if self.in_flight > 0 {
            self.early.push((id, outcome.clone()));
            let excess = self.early.len().saturating_sub(EARLY_MAX);
            self.early.drain(..excess);
        }
        None
    }

    /// Service perdu : les résultats attendus n'arriveront pas.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

fn failure(outcome: &Outcome) -> Option<String> {
    (!outcome.is_success()).then(|| outcome.describe())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn losing_the_service_ends_the_wait() {
        let mut t = Tracker::default();
        t.sent();
        t.submitted(2);
        t.reset();
        assert!(!t.busy());
    }
}
