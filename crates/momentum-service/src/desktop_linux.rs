//! Intégration au bureau Linux : notifications du service par D-Bus
//! (`org.freedesktop.Notifications`, affichées par KDE ou GNOME), comme
//! `show_toast` de la version Windows (`desktop.rs`). Les raccourcis globaux
//! sont enregistrés par le client KDE (KGlobalAccel).

use std::time::Duration;

use dbus::blocking::Connection;
use tracing::warn;

/// Nom de l'application dans les notifications.
const APP_NAME: &str = "MOMENTUM 5 (non officiel)";
/// Délai maximal de l'appel D-Bus.
const DBUS_TIMEOUT: Duration = Duration::from_secs(2);

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
