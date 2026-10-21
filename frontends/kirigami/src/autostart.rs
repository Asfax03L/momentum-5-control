//! Lancement à l'ouverture de session : fichier
//! `~/.config/autostart/momentum-kirigami.desktop` (spécification XDG,
//! suivie par KDE). Le client démarre caché (`--cache`) : seule l'icône de la
//! zone de notification apparaît.

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
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=OpenHeadset Control\n\
         Comment=Réglages du casque Sennheiser MOMENTUM 5 (non officiel)\n\
         Exec=\"{exe}\" {HIDDEN_FLAG}\n\
         Icon=org.openheadset.control\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

pub fn is_enabled() -> bool {
    path().is_some_and(|p| p.exists())
}

pub fn set(on: bool) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::other("ni XDG_CONFIG_HOME ni HOME"))?;
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
