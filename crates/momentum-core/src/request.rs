//! Résultat d'une demande faite au service : chaque commande reçoit un
//! numéro, et le service publie un seul résultat pour ce numéro. Un accusé
//! du casque n'est pas une confirmation : « appliquée » veut dire relue avec
//! la valeur demandée.

use serde::{Deserialize, Serialize};

/// Issue d'une demande.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    /// Envoyée, acquittée, puis relue avec la valeur demandée.
    Applied,
    /// Rien à envoyer : le casque était déjà dans l'état demandé.
    AlreadySatisfied,
    /// Acquittée par le casque ; l'effet n'est pas relu tout de suite (il
    /// arrive par les notifications, comme la connexion d'un appareil).
    Acknowledged,
    /// Refusée (par le service ou par le casque) : rien n'a changé.
    Refused(String),
    /// Acquittée, mais la relecture ne montre pas la valeur demandée.
    NotApplied(String),
    /// Abandonnée sans rien envoyer (remplacée par une demande plus récente,
    /// connexion changée, casque absent).
    Cancelled(String),
    /// Connexion perdue après l'envoi : le casque a pu l'appliquer ou non.
    /// Jamais renvoyée automatiquement.
    Unknown(String),
}

impl Outcome {
    /// Le casque est dans l'état demandé (ou l'a acquitté).
    pub fn is_success(&self) -> bool {
        matches!(
            self,
            Outcome::Applied | Outcome::AlreadySatisfied | Outcome::Acknowledged
        )
    }

    /// Phrase pour l'utilisateur.
    pub fn describe(&self) -> String {
        match self {
            Outcome::Applied => "appliqué et relu".to_string(),
            Outcome::AlreadySatisfied => "déjà dans cet état".to_string(),
            Outcome::Acknowledged => "accepté par le casque".to_string(),
            Outcome::Refused(r) => format!("refusé : {r}"),
            Outcome::NotApplied(r) => format!("non appliqué : {r}"),
            Outcome::Cancelled(r) => format!("annulé sans envoi : {r}"),
            Outcome::Unknown(r) => format!("résultat inconnu : {r}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_confirmed_outcomes_are_successes() {
        assert!(Outcome::Applied.is_success());
        assert!(Outcome::AlreadySatisfied.is_success());
        assert!(Outcome::Acknowledged.is_success());
        for failure in [
            Outcome::Refused("x".into()),
            Outcome::NotApplied("x".into()),
            Outcome::Cancelled("x".into()),
            Outcome::Unknown("x".into()),
        ] {
            assert!(!failure.is_success(), "{failure:?}");
        }
        assert_eq!(
            Outcome::Unknown("connexion perdue".into()).describe(),
            "résultat inconnu : connexion perdue"
        );
    }
}
