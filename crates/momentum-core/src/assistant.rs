//! Assistant de premier lancement (Rust pur) : Bluetooth, appairage,
//! Windows App Runtime, Multipoint. Montré une fois (`assistant.txt` dans le
//! dossier des données), puis rouvert depuis « À propos ». Jamais
//! automatiquement en mode démonstration (essais d'interface).
//!
//! L'appairage reste celui du système : l'assistant l'explique, il ne le
//! fait pas.

use crate::i18n::tr;

/// Système de l'interface : quelques étapes en dépendent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
}

/// Action proposée par une étape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepAction {
    /// Ouvrir les paramètres Bluetooth du système.
    BluetoothSettings,
}

/// Une étape : titre, texte (déjà traduits), action éventuelle, et si l'état
/// de connexion du casque y est montré.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub title: &'static str,
    pub text: &'static str,
    pub action: Option<StepAction>,
    pub shows_connection: bool,
}

/// Site du guide d'utilisation et de la FAQ.
pub const GUIDE_URL: &str = "https://asfax03l.github.io/openheadset-control/";

/// Étapes dans l'ordre, pour ce système.
pub fn steps(platform: Platform) -> Vec<Step> {
    let step = |title, text| Step {
        title,
        text,
        action: None,
        shows_connection: false,
    };
    let mut list = vec![
        step(
            tr("Bienvenue"),
            tr(
                "OpenHeadset Control règle le Sennheiser MOMENTUM 5 depuis ce PC : réduction de bruit, égaliseur, scènes, batterie. Projet non officiel, sans lien avec Sennheiser ni Sonova. Seuls les réglages documentés sont modifiés ; le reste est en lecture seule.",
            ),
        ),
        Step {
            action: Some(StepAction::BluetoothSettings),
            ..step(
                tr("Bluetooth"),
                match platform {
                    Platform::Windows => tr(
                        "Le Bluetooth du PC doit être activé : Paramètres > Bluetooth et appareils.",
                    ),
                    Platform::Linux => tr(
                        "Le Bluetooth du PC doit être activé : Configuration du système > Bluetooth.",
                    ),
                },
            )
        },
        Step {
            shows_connection: true,
            ..step(
                tr("Appairage"),
                tr(
                    "Appairez le casque avec ce PC dans les paramètres Bluetooth du système (mode appairage : voir la notice du casque). OpenHeadset Control n'appaire rien lui-même : il trouve ensuite le casque tout seul.",
                ),
            )
        },
    ];
    if platform == Platform::Windows {
        list.push(step(
            tr("Windows App Runtime"),
            tr("Cette interface utilise Windows App Runtime 2 : il est installé, puisque cette fenêtre s'affiche. L'installateur le vérifie ; après une réinstallation de Windows : winget install Microsoft.WindowsAppRuntime.2."),
        ));
    }
    list.push(step(
        tr("Multipoint"),
        tr("Avec le Multipoint, le casque reste connecté à deux appareils à la fois, par exemple deux PC ou un PC et un téléphone. OpenHeadset Control peut tourner sur chaque PC. Couper le Multipoint déconnecte l'autre appareil."),
    ));
    list.push(step(
        tr("C'est prêt"),
        tr("Cet assistant reste accessible depuis « À propos ». Guide d'utilisation et questions fréquentes : asfax03l.github.io/openheadset-control."),
    ));
    list
}

/// Texte d'état du casque pour l'étape d'appairage.
pub fn connection_text(connected: bool) -> &'static str {
    if connected {
        tr("Casque connecté : tout est prêt.")
    } else {
        tr("Casque pas encore détecté : l'appairer, l'allumer, puis patienter quelques secondes.")
    }
}

fn marker_file() -> Option<std::path::PathBuf> {
    Some(crate::paths::data_dir()?.join("assistant.txt"))
}

/// Montrer l'assistant au démarrage : jamais vu, hors démonstration.
pub fn should_show(demo: bool) -> bool {
    !demo && marker_file().is_some_and(|path| !path.exists())
}

/// Note que l'assistant a été vu (terminé ou passé).
pub fn mark_seen() -> std::io::Result<()> {
    let path =
        marker_file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    crate::paths::write_user_file(
        &path,
        "# Assistant de premier lancement déjà vu (supprimer ce fichier pour le revoir)\n",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_per_platform() {
        let windows = steps(Platform::Windows);
        let linux = steps(Platform::Linux);
        assert_eq!(windows.len(), linux.len() + 1);
        assert!(windows.iter().any(|s| s.title == "Windows App Runtime"));
        assert!(!linux.iter().any(|s| s.title == "Windows App Runtime"));
        for list in [&windows, &linux] {
            assert_eq!(list.iter().filter(|s| s.shows_connection).count(), 1);
            assert_eq!(
                list.iter()
                    .filter(|s| s.action == Some(StepAction::BluetoothSettings))
                    .count(),
                1
            );
            assert!(
                list.iter()
                    .all(|s| !s.title.is_empty() && !s.text.is_empty())
            );
        }
    }

    #[test]
    fn never_shown_in_demo() {
        assert!(!should_show(true));
    }
}
