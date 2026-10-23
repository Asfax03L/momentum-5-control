//! Action du système, indépendante de la session RFCOMM et des écritures GAIA.
use momentum_core::request::Outcome;
use momentum_core::transport::reconnect::ReconnectError;

pub const LINUX_MESSAGE: &str = "Demande Bluetooth exécutée par BlueZ. Attendre la connexion du service ; la connexion audio reste à vérifier.";
pub const WINDOWS_MESSAGE: &str = "Paramètres Bluetooth ouverts : sélectionner le casque et cliquer sur Connecter. Windows ne fournit pas ici de reconnexion audio automatique fiable ; aucune connexion n'est confirmée.";

/// Point d'entrée du service ; aucun accès système sans casque mémorisé.
pub fn request(remembered: Option<&str>) -> Outcome {
    request_with(remembered, platform)
}

fn request_with(
    remembered: Option<&str>,
    action: impl FnOnce(&str) -> Result<&'static str, ReconnectError>,
) -> Outcome {
    let Some(remembered) = remembered else {
        return Outcome::Refused(ReconnectError::NotRemembered.to_string());
    };
    match action(remembered) {
        Ok(message) => Outcome::SystemRequested(message.into()),
        Err(error @ ReconnectError::Failed) => Outcome::Unknown(error.to_string()),
        Err(error) => Outcome::Refused(error.to_string()),
    }
}

#[cfg(target_os = "linux")]
fn platform(remembered: &str) -> Result<&'static str, ReconnectError> {
    momentum_core::transport::bluez_rfcomm::reconnect_remembered(remembered)?;
    Ok(LINUX_MESSAGE)
}

#[cfg(windows)]
fn platform(_: &str) -> Result<&'static str, ReconnectError> {
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::w;
    // SAFETY: chaînes UTF-16 statiques, aucun argument ou chemin utilisateur.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            w!("ms-settings:bluetooth"),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err(ReconnectError::Unavailable);
    }
    Ok(WINDOWS_MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_memory_refuses_without_executing_the_action() {
        assert!(matches!(
            request_with(None, |_| panic!("aucun appel système")),
            Outcome::Refused(_)
        ));
    }

    #[test]
    fn system_results_never_claim_gaia_or_audio_confirmation() {
        for message in [LINUX_MESSAGE, WINDOWS_MESSAGE] {
            let outcome = request_with(Some("empreinte fictive"), |_| Ok(message));
            assert_eq!(outcome, Outcome::SystemRequested(message.into()));
            assert!(outcome.is_success());
            assert_eq!(outcome.describe(), message);
        }
        assert!(matches!(
            request_with(Some("fictif"), |_| Err(ReconnectError::Failed)),
            Outcome::Unknown(_)
        ));
        assert!(matches!(
            request_with(Some("fictif"), |_| Err(ReconnectError::Missing)),
            Outcome::Refused(_)
        ));
    }
}
