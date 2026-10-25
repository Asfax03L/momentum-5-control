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

/// Délai sans lecture avant l'avis « casque allumé sans son ».
pub const IDLE_SECONDS: u64 = 30 * 60;

/// Notifications choisies par l'utilisateur (`notifications.txt`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NotificationPrefs {
    pub low_battery: bool,
    pub connection: bool,
    /// Casque connecté sans lecture depuis [`IDLE_SECONDS`] ; désactivé par
    /// défaut.
    pub idle: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            low_battery: true,
            connection: true,
            idle: false,
        }
    }
}

impl NotificationPrefs {
    /// Lit `cle = oui|non` ; valeur par défaut pour une clé absente.
    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let on = match value.trim() {
                "oui" => true,
                "non" => false,
                _ => continue,
            };
            match key.trim() {
                "batterie-faible" => prefs.low_battery = on,
                "connexion" => prefs.connection = on,
                "sans-son" => prefs.idle = on,
                _ => {}
            }
        }
        prefs
    }

    pub fn to_text(&self) -> String {
        let yes = |b: bool| if b { "oui" } else { "non" };
        format!(
            "# Notifications d'OpenHeadset Control : oui ou non\n\
             batterie-faible = {}\n\
             connexion = {}\n\
             sans-son = {}\n",
            yes(self.low_battery),
            yes(self.connection),
            yes(self.idle)
        )
    }

    fn file() -> Option<std::path::PathBuf> {
        Some(crate::paths::data_dir()?.join("notifications.txt"))
    }

    /// Choix enregistrés (valeurs par défaut sans fichier).
    pub fn load() -> Self {
        Self::file()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Self::default, |t| Self::parse(&t))
    }

    pub fn store(&self) -> std::io::Result<()> {
        let path = Self::file()
            .ok_or_else(|| std::io::Error::other(tr("dossier des données introuvable")))?;
        crate::paths::write_user_file(&path, &self.to_text())
    }
}

/// Suivi de l'absence de lecture : `0xFF` est le codec relevé à l'arrêt de
/// la lecture (voir `HeadsetState::codec`).
#[derive(Debug, Default)]
pub struct IdleTracker {
    since: Option<u64>,
    warned: bool,
}

impl IdleTracker {
    /// Codec lu à `now` ; avis une seule fois par période sans lecture.
    pub fn on_codec(&mut self, now: u64, codec: Option<u8>) -> Option<Alert> {
        if codec != Some(0xFF) {
            self.since = None;
            self.warned = false;
            return None;
        }
        let since = *self.since.get_or_insert(now);
        if self.warned || now.saturating_sub(since) < IDLE_SECONDS {
            return None;
        }
        self.warned = true;
        Some(Alert {
            title: tr("Casque allumé sans son").to_string(),
            body: tr("Le casque ne signale aucune lecture depuis 30 minutes : penser à l'éteindre pour garder la batterie.")
                .to_string(),
        })
    }

    /// Connexion perdue : la période sans lecture recommence.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_prefs_defaults_and_round_trip() {
        let d = NotificationPrefs::parse("");
        assert_eq!(d, NotificationPrefs::default());
        assert!(d.low_battery && d.connection && !d.idle);
        let p = NotificationPrefs {
            low_battery: false,
            connection: true,
            idle: true,
        };
        assert_eq!(NotificationPrefs::parse(&p.to_text()), p);
        assert_eq!(NotificationPrefs::parse("sans-son = peut-être\n"), d);
    }

    #[test]
    fn idle_alert_once_per_silent_period() {
        let mut t = IdleTracker::default();
        assert!(t.on_codec(0, Some(0xFF)).is_none());
        assert!(t.on_codec(IDLE_SECONDS - 1, Some(0xFF)).is_none());
        assert!(t.on_codec(IDLE_SECONDS, Some(0xFF)).is_some());
        assert!(t.on_codec(IDLE_SECONDS * 2, Some(0xFF)).is_none());
        // Lecture reprise, puis nouveau silence : nouvel avis possible.
        assert!(t.on_codec(IDLE_SECONDS * 2 + 1, Some(2)).is_none());
        assert!(t.on_codec(IDLE_SECONDS * 3, Some(0xFF)).is_none());
        assert!(t.on_codec(IDLE_SECONDS * 4, Some(0xFF)).is_some());
        // Codec inconnu : jamais d'avis.
        let mut t = IdleTracker::default();
        assert!(t.on_codec(0, None).is_none());
        assert!(t.on_codec(IDLE_SECONDS * 9, None).is_none());
    }

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
