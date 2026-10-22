//! Couche transport : un flux d'octets bidirectionnel vers le casque.
//!
//! Le trait ne connaît pas les trames GAIA : c'est `client::GaiaClient` qui
//! découpe le flux. On peut ainsi brancher RFCOMM, un futur transport GATT ou
//! un `MockTransport` sans toucher au reste.

#[cfg(target_os = "linux")]
pub mod bluez_rfcomm;
pub mod choice;
pub mod mock;
#[cfg(windows)]
pub mod windows_rfcomm;

pub use mock::MockTransport;

use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TransportError {
    #[error("délai dépassé ({0:?}) sans données du casque")]
    Timeout(Duration),
    #[error("connexion fermée par le casque")]
    Closed,
    /// Une opération Windows (recherche, connexion, envoi) n'a pas abouti à
    /// temps : on abandonne plutôt que de bloquer le programme.
    #[error("{context} : pas de réponse de Windows après {timeout:?}")]
    Deadline {
        context: &'static str,
        timeout: Duration,
    },
    /// Erreur Windows avec son HRESULT, pour le diagnostic.
    #[error("{context} : HRESULT {hresult:#010X} — {message}")]
    Os {
        context: &'static str,
        hresult: u32,
        message: String,
    },
    /// Erreur de BlueZ (Linux) ou du socket RFCOMM, message d'origine conservé.
    #[error("{context} : {message}")]
    Bluetooth {
        context: &'static str,
        message: String,
    },
    #[error("trame inattendue envoyée au mock : {0}")]
    UnexpectedRequest(String),
}

pub trait Transport {
    /// Envoie tous les octets.
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError>;

    /// Attend au plus `timeout` et renvoie au moins un octet reçu.
    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError>;
}
