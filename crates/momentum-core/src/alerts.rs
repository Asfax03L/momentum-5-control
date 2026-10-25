//! Quand prévenir l'utilisateur par une notification Windows, en Rust pur.
//!
//! Règles, pour ne jamais inonder l'écran :
//! - batterie faible : une seule fois sous 15 %, réarmée au-dessus de 20 % ;
//! - connexion perdue : seulement si le casque était connecté ;
//! - casque retrouvé : seulement après une perte (pas au démarrage).

use crate::i18n::{tr, trf};
/// Seuil d'alerte de batterie, en %.
pub const LOW_BATTERY: u8 = 15;
/// Seuil de réarmement de l'alerte, en %.
const BATTERY_REARM: u8 = 20;

/// Notification à afficher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct AlertTracker {
    connected: bool,
    lost: bool,
    battery_warned: bool,
}

impl AlertTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Casque connecté (nom affiché).
    pub fn on_connected(&mut self, name: &str) -> Option<Alert> {
        self.connected = true;
        std::mem::take(&mut self.lost).then(|| Alert {
            title: tr("Casque reconnecté").to_string(),
            body: trf("« {} » est de nouveau connecté.", &[&name]),
        })
    }

    /// Connexion perdue.
    pub fn on_lost(&mut self) -> Option<Alert> {
        if !std::mem::take(&mut self.connected) {
            return None;
        }
        self.lost = true;
        Some(Alert {
            title: tr("Casque déconnecté").to_string(),
            body: tr("La connexion au casque est perdue ; nouvel essai toutes les 5 s.")
                .to_string(),
        })
    }

    /// Niveau de batterie lu ou notifié.
    pub fn on_battery(&mut self, percent: u8) -> Option<Alert> {
        if percent >= BATTERY_REARM {
            self.battery_warned = false;
            return None;
        }
        if percent > LOW_BATTERY || self.battery_warned {
            return None;
        }
        self.battery_warned = true;
        Some(Alert {
            title: tr("Batterie faible").to_string(),
            body: trf("Il reste {} % de batterie au casque.", &[&percent]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_alerts_only_on_changes() {
        let mut t = AlertTracker::new();
        // Premier démarrage : pas d'alerte, ni de « perte » sans connexion.
        assert!(t.on_lost().is_none());
        assert!(t.on_connected("M5").is_none());
        assert!(t.on_lost().is_some());
        // Tentatives répétées pendant la perte : une seule alerte.
        assert!(t.on_lost().is_none());
        let back = t.on_connected("M5").unwrap();
        assert!(back.body.contains("M5"));
        assert!(t.on_connected("M5").is_none());
    }

    #[test]
    fn battery_alert_once_then_rearmed() {
        let mut t = AlertTracker::new();
        assert!(t.on_battery(50).is_none());
        assert!(t.on_battery(15).is_some());
        assert!(t.on_battery(14).is_none());
        assert!(t.on_battery(18).is_none());
        // Rechargé au-dessus de 20 % : l'alerte est réarmée.
        assert!(t.on_battery(25).is_none());
        assert_eq!(
            t.on_battery(10).unwrap().body,
            "Il reste 10 % de batterie au casque."
        );
    }
}
