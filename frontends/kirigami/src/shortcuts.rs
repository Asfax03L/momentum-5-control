//! Raccourcis clavier globaux de KDE (service `kglobalaccel`, D-Bus).
//!
//! Les actions sont déclarées avec leur raccourci par défaut (les mêmes que
//! sous Windows), plus une action sans raccourci par scène ; l'utilisateur
//! les change ou les retire dans Configuration du système > Raccourcis,
//! rubrique « OpenHeadset Control » (bouton de la page Paramètres). KDE
//! garde ce choix d'une session à l'autre.
//!
//! Interface relevée par introspection sur Plasma 6 (`org.kde.KGlobalAccel`
//! sur `/kglobalaccel`, `org.kde.kglobalaccel.Component` sur l'objet du
//! composant) : `doRegister(as)`, `setShortcutKeys(as, a(ai), u) -> a(ai)`,
//! `getComponent(s) -> o`, signal `globalShortcutPressed(s, s, x)`.

use momentum_core::hotkeys::HotkeyAction;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::OwnedObjectPath;

const SERVICE: &str = "org.kde.kglobalaccel";
/// Composant. Pas de nom en `.desktop` : KDE traiterait alors les
/// raccourcis comme des lancements d'application (action `_launch`) et
/// n'enverrait pas le signal `globalShortcutPressed`.
const COMPONENT: &str = "openheadset-control";
const COMPONENT_NAME: &str = "OpenHeadset Control";

/// Drapeaux de `setShortcutKeys` (KGlobalAccel) : raccourci par défaut ;
/// raccourci actif, remplacé par celui que l'utilisateur a enregistré.
const IS_DEFAULT: u32 = 1;
const SET_PRESENT: u32 = 2;

/// Modificateurs et touches de Qt (`Qt::KeyboardModifier`, `Qt::Key`).
const CTRL: i32 = 0x0400_0000;
const ALT: i32 = 0x0800_0000;

/// Actions déclarées et raccourci par défaut (Ctrl+Alt+lettre).
const ACTIONS: [(HotkeyAction, char); 3] = [
    (HotkeyAction::CycleNoise, 'N'),
    (HotkeyAction::ToggleProfile, 'P'),
    (HotkeyAction::OpenWindow, 'M'),
];

/// Séquence Qt d'un raccourci Ctrl+Alt+lettre (`Qt::Key_A` = 'A').
fn ctrl_alt(letter: char) -> i32 {
    CTRL | ALT | letter as i32
}

/// Identifiant KGlobalAccel d'une action : composant, action, et leurs noms
/// affichés.
fn action_id(action: &HotkeyAction) -> Vec<String> {
    vec![
        COMPONENT.to_string(),
        action.key(),
        COMPONENT_NAME.to_string(),
        action.label(),
    ]
}

/// Déclare les actions (et une par scène de `scenes`) puis appelle
/// `on_pressed` à chaque appui, jusqu'à la fin de la session D-Bus. À
/// lancer dans un thread dédié.
pub fn run(scenes: &[String], mut on_pressed: impl FnMut(HotkeyAction)) -> zbus::Result<()> {
    let connection = Connection::session()?;
    let accel = Proxy::new(
        &connection,
        SERVICE,
        "/kglobalaccel",
        "org.kde.KGlobalAccel",
    )?;
    for (action, letter) in &ACTIONS {
        let id = action_id(action);
        let keys = vec![(vec![ctrl_alt(*letter)],)];
        accel.call_method("doRegister", &(&id,))?;
        let _: Vec<(Vec<i32>,)> = accel.call("setShortcutKeys", &(&id, &keys, IS_DEFAULT))?;
        let active: Vec<(Vec<i32>,)> = accel.call("setShortcutKeys", &(&id, &keys, SET_PRESENT))?;
        if active.is_empty() {
            tracing::info!(action = %action.key(), "raccourci KDE sans touche (déjà pris ou retiré)");
        }
    }
    // Scènes : sans raccourci par défaut, à choisir dans KDE.
    for name in scenes {
        accel.call_method(
            "doRegister",
            &(&action_id(&HotkeyAction::Scene(name.clone())),),
        )?;
    }
    let path: OwnedObjectPath = accel.call("getComponent", &(COMPONENT,))?;
    let component = Proxy::new(&connection, SERVICE, path, "org.kde.kglobalaccel.Component")?;
    tracing::info!("raccourcis KDE déclarés");
    for message in component.receive_signal("globalShortcutPressed")? {
        let Ok((component, name, _timestamp)) =
            message.body().deserialize::<(String, String, i64)>()
        else {
            continue;
        };
        if component != COMPONENT {
            continue;
        }
        tracing::info!(action = %name, "raccourci KDE");
        if let Some(action) = HotkeyAction::from_key(&name) {
            on_pressed(action);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_key_codes() {
        // Qt::CTRL | Qt::ALT | Qt::Key_N
        assert_eq!(ctrl_alt('N'), 0x0C00_004E);
    }

    #[test]
    fn action_ids() {
        let id = action_id(&HotkeyAction::OpenWindow);
        assert_eq!(id[0], "openheadset-control");
        assert_eq!(id[1], "fenetre");
    }
}
