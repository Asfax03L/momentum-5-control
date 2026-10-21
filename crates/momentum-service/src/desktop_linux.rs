//! Intégration au bureau Linux : notifications par D-Bus
//! (`org.freedesktop.Notifications`, affichées par KDE ou GNOME). Même
//! interface que la version Windows (`desktop.rs`).
//!
//! Raccourcis clavier globaux : pas encore pris en charge (sous Wayland,
//! ils passent par le portail `GlobalShortcuts`, à faire).

use std::time::Duration;

use dbus::blocking::Connection;
use momentum_core::hotkeys::{Hotkey, HotkeyAction};
use tracing::{info, warn};

/// Nom de l'application dans les notifications.
const APP_NAME: &str = "MOMENTUM 5 (non officiel)";
/// Délai maximal de l'appel D-Bus.
const DBUS_TIMEOUT: Duration = Duration::from_secs(2);

/// Thread des raccourcis (aucun sous Linux pour l'instant).
pub struct HotkeyThread;

impl HotkeyThread {
    pub fn stop(self) {}
}

/// Raccourcis globaux non pris en charge : rien n'est enregistré, et ce
/// n'est pas une erreur de l'utilisateur (une ligne d'information suffit).
pub fn spawn_hotkeys(
    keys: Vec<(HotkeyAction, Hotkey)>,
    _on_hotkey: impl Fn(HotkeyAction) + Send + 'static,
) -> (Option<HotkeyThread>, Vec<String>) {
    if !keys.is_empty() {
        info!(
            nombre = keys.len(),
            "raccourcis globaux pas encore pris en charge sous Linux : ignorés"
        );
    }
    (None, Vec::new())
}

/// Rien à déclarer : le nom de l'application accompagne chaque notification.
pub fn register_notifications() {}

/// Affiche une notification du bureau ; un échec est seulement journalisé.
pub fn show_toast(title: &str, body: &str) {
    let result = (|| -> Result<u32, dbus::Error> {
        let connection = Connection::new_session()?;
        let proxy = connection.with_proxy(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            DBUS_TIMEOUT,
        );
        let actions: Vec<&str> = Vec::new();
        let hints: dbus::arg::PropMap = dbus::arg::PropMap::new();
        let (id,): (u32,) = proxy.method_call(
            "org.freedesktop.Notifications",
            "Notify",
            (
                APP_NAME,
                0u32,
                "audio-headphones",
                title,
                body,
                actions,
                hints,
                -1i32,
            ),
        )?;
        Ok(id)
    })();
    if let Err(e) = result {
        warn!(%e, "notification du bureau");
    }
}
