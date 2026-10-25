//! Lancement à l'ouverture de session : fichier
//! `~/.config/autostart/momentum-kirigami.desktop` (spécification XDG,
//! suivie par KDE). Le client démarre caché (`--cache`) : seule l'icône de la
//! zone de notification apparaît.
//!
//! Dans le Flatpak, ce fichier reste dans le bac à sable (il garde le choix)
//! et la demande passe par le portail `org.freedesktop.portal.Background`,
//! qui crée l'entrée de démarrage du bureau (`flatpak run …`).

use momentum_core::i18n::{tr, trf};
use std::io;
use std::path::PathBuf;

/// Option de la ligne de commande : démarrer sans montrer la fenêtre.
pub const HIDDEN_FLAG: &str = "--cache";

fn path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("autostart").join("momentum-kirigami.desktop"))
}

/// Fichier `.desktop` de lancement automatique.
fn desktop_entry(exe: &str) -> String {
    trf(
        "[Desktop Entry]\nType=Application\nName=OpenHeadset Control\nComment=Réglages du casque Sennheiser MOMENTUM 5 (non officiel)\nExec=\"{}\" {}\nIcon=org.openheadset.control\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        &[&exe, &HIDDEN_FLAG],
    )
}

/// Lancé dans le bac à sable Flatpak.
pub fn in_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// Demande au portail de lancer (ou non) le client à l'ouverture de
/// session, caché.
fn request_background(autostart: bool) -> zbus::Result<()> {
    use std::collections::HashMap;
    use zbus::wire::Variant as Value;

    let mut options: HashMap<&str, Value> = HashMap::new();
    options.insert(
        "reason",
        Value::from(tr(
            "Lancer OpenHeadset Control à l'ouverture de session (icône seule)",
        )),
    );
    options.insert("autostart", Value::from(autostart));
    options.insert(
        "commandline",
        Value::from(vec!["momentum-kirigami", HIDDEN_FLAG]),
    );
    zbus::block_on(async {
        let connection = zbus::Connection::session().await?;
        connection
            .call_method(
                Some("org.freedesktop.portal.Desktop"),
                "/org/freedesktop/portal/desktop",
                Some("org.freedesktop.portal.Background"),
                "RequestBackground",
                &("", options),
            )
            .await?;
        Ok(())
    })
}

pub fn is_enabled() -> bool {
    path().is_some_and(|p| p.exists())
}

pub fn set(on: bool) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::other(tr("ni XDG_CONFIG_HOME ni HOME")))?;
    if in_flatpak() {
        request_background(on)
            .map_err(|e| io::Error::other(trf("portail Background : {}", &[&e])))?;
    }
    if on {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let exe = std::env::current_exe()?;
        std::fs::write(path, desktop_entry(&exe.display().to_string()))
    } else {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_starts_hidden() {
        let entry = desktop_entry("/home/x/.local/bin/momentum-kirigami");
        assert!(entry.contains("Exec=\"/home/x/.local/bin/momentum-kirigami\" --cache\n"));
        assert!(entry.contains("Icon=org.openheadset.control\n"));
    }
}
