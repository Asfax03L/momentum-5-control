//! Choix strict du casque mémorisé pour une reconnexion système.
//! Aucun nom, balayage radio ou choix de secours ; aucune trame GAIA.

use super::choice::fingerprint;
use crate::i18n::tr;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReconnectError {
    #[error(
        "{}",
        tr(
            "aucun casque mémorisé : le connecter une première fois depuis les paramètres Bluetooth"
        )
    )]
    NotRemembered,
    #[error(
        "{}",
        tr("casque mémorisé absent des appareils appairés ; vérifier les paramètres Bluetooth")
    )]
    Missing,
    #[error("{}", tr("identité mémorisée ambiguë : reconnexion refusée"))]
    Ambiguous,
    #[error("{}", tr("Bluetooth système indisponible ou accès refusé"))]
    Unavailable,
    #[error(
        "{}",
        tr(
            "la demande Bluetooth a échoué ou dépassé le délai ; vérifier les paramètres Bluetooth avant de réessayer"
        )
    )]
    Failed,
}

/// Demande acceptée par le système.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconnected {
    /// Connexion demandée ; son effet reste à constater.
    Requested,
    /// Le système le voit déjà connecté : rien n'a été demandé.
    AlreadyConnected,
}

/// Adaptateur système : identifiants privés des appareils déjà appairés.
/// Les erreurs restent expurgées, même si le système fournit une adresse.
pub trait BluetoothSystem {
    fn paired_devices(&mut self) -> Result<Vec<String>, ReconnectError>;
    fn connect(&mut self, key: &str) -> Result<Reconnected, ReconnectError>;
}

/// Un seul appel Connect, exclusivement vers l'identité mémorisée.
pub fn reconnect(
    remembered: Option<&str>,
    system: &mut impl BluetoothSystem,
) -> Result<Reconnected, ReconnectError> {
    let remembered = remembered.ok_or(ReconnectError::NotRemembered)?;
    let devices = system.paired_devices()?;
    let mut matches = devices.iter().filter(|key| fingerprint(key) == remembered);
    let key = matches.next().ok_or(ReconnectError::Missing)?;
    if matches.next().is_some() {
        return Err(ReconnectError::Ambiguous);
    }
    system.connect(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        devices: Vec<String>,
        calls: Vec<String>,
        enumerations: usize,
        failure: bool,
    }
    impl BluetoothSystem for Fake {
        fn paired_devices(&mut self) -> Result<Vec<String>, ReconnectError> {
            self.enumerations += 1;
            Ok(self.devices.clone())
        }
        fn connect(&mut self, key: &str) -> Result<Reconnected, ReconnectError> {
            self.calls.push(key.into());
            if self.failure {
                Err(ReconnectError::Failed)
            } else {
                Ok(Reconnected::Requested)
            }
        }
    }

    #[test]
    fn no_memory_never_consults_bluetooth() {
        let mut fake = Fake::default();
        assert_eq!(
            reconnect(None, &mut fake),
            Err(ReconnectError::NotRemembered)
        );
        assert_eq!(fake.enumerations, 0);
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn only_the_remembered_paired_device_is_connected_once() {
        let key = "02:00:00:00:00:01";
        let mut fake = Fake {
            devices: vec!["02:00:00:00:00:02".into(), key.into()],
            ..Fake::default()
        };
        assert_eq!(
            reconnect(Some(&fingerprint(key)), &mut fake),
            Ok(Reconnected::Requested)
        );
        assert_eq!(fake.calls, [key]);
        assert_eq!(fake.enumerations, 1);
    }

    #[test]
    fn absent_or_ambiguous_never_falls_back_to_another_device() {
        let key = "02:00:00:00:00:01";
        let mut fake = Fake {
            devices: vec!["02:00:00:00:00:02".into()],
            ..Fake::default()
        };
        assert_eq!(
            reconnect(Some(&fingerprint(key)), &mut fake),
            Err(ReconnectError::Missing)
        );
        fake.devices = vec![key.into(), key.into()];
        assert_eq!(
            reconnect(Some(&fingerprint(key)), &mut fake),
            Err(ReconnectError::Ambiguous)
        );
        assert!(fake.calls.is_empty());
    }

    #[test]
    fn failure_is_not_retried_or_reported_as_connected() {
        let key = "02:00:00:00:00:01";
        let mut fake = Fake {
            devices: vec![key.into()],
            failure: true,
            ..Fake::default()
        };
        assert_eq!(
            reconnect(Some(&fingerprint(key)), &mut fake),
            Err(ReconnectError::Failed)
        );
        assert_eq!(fake.calls.len(), 1);
        assert!(!ReconnectError::Failed.to_string().contains(key));
    }
}
