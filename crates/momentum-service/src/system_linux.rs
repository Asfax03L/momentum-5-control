//! Intégration au système Linux (session de bureau XDG) : lancement à
//! l'ouverture de session, instance unique, heure locale. Même interface
//! que la version Windows (`system.rs`).

use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

/// Nom du fichier `.desktop` de lancement automatique.
const AUTOSTART_FILE: &str = "momentum-ctl-win.desktop";
/// Socket de l'instance unique, dans `$XDG_RUNTIME_DIR`.
const INSTANCE_SOCKET: &str = "momentum-ctl-win-tray.sock";

/// Lancement à l'ouverture de session : fichier
/// `~/.config/autostart/momentum-ctl-win.desktop` (spécification XDG,
/// suivie par KDE et GNOME).
pub struct Autostart {
    path: Option<PathBuf>,
}

impl Default for Autostart {
    fn default() -> Self {
        Self::new()
    }
}

impl Autostart {
    pub fn new() -> Self {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
        Self {
            path: config.map(|c| c.join("autostart").join(AUTOSTART_FILE)),
        }
    }

    /// Ligne `Exec=` : chemin de l'exécutable courant, entre guillemets.
    pub fn command_line() -> io::Result<String> {
        Ok(format!("\"{}\"", std::env::current_exe()?.display()))
    }

    /// Contenu du fichier `.desktop`, s'il existe.
    pub fn registered(&self) -> io::Result<Option<String>> {
        let Some(path) = &self.path else {
            return Ok(None);
        };
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Some(text)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub fn is_enabled(&self) -> bool {
        matches!(self.registered(), Ok(Some(_)))
    }

    pub fn set(&self, on: bool) -> io::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| io::Error::other("ni XDG_CONFIG_HOME ni HOME"))?;
        if on {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(path, desktop_entry(&Self::command_line()?))
        } else {
            match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e),
            }
        }
    }
}

/// Fichier `.desktop` de lancement automatique.
fn desktop_entry(exec: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=MOMENTUM 5 (non officiel)\n\
         Comment=Icône de réglage du casque Sennheiser MOMENTUM 5\n\
         Exec={exec}\n\
         Icon=audio-headphones\n\
         Terminal=false\n\
         X-GNOME-Autostart-enabled=true\n"
    )
}

/// Résultat de la prise du verrou d'instance unique.
pub enum Instance {
    /// Première instance : attend les demandes des suivantes.
    First(ShowRequest),
    /// Une instance tourne déjà ; elle a été priée d'ouvrir sa fenêtre.
    AlreadyRunning,
}

/// Demandes d'ouverture envoyées par une deuxième instance.
pub struct ShowRequest(UnixListener);

impl ShowRequest {
    /// Bloque jusqu'à la prochaine demande ; `false` si le socket est
    /// inutilisable.
    pub fn wait(&self) -> bool {
        match self.0.accept() {
            Ok((mut stream, _)) => {
                let mut byte = [0u8; 1];
                let _ = stream.read(&mut byte);
                true
            }
            Err(_) => false,
        }
    }
}

fn instance_socket() -> io::Result<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR absent (hors session ?)"))?;
    Ok(PathBuf::from(dir).join(INSTANCE_SOCKET))
}

/// Instance unique par socket Unix dans `$XDG_RUNTIME_DIR` (propre à
/// l'utilisateur, vidé à la fermeture de session).
pub fn acquire_instance() -> io::Result<Instance> {
    let path = instance_socket()?;
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let _ = stream.write_all(b"s");
        return Ok(Instance::AlreadyRunning);
    }
    // Personne n'écoute : un fichier resté d'un arrêt brutal est remplacé.
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(Instance::First(ShowRequest(UnixListener::bind(&path)?)))
}

/// Système (« Bazzite 44 »), d'après `/etc/os-release`, pour le
/// diagnostic.
pub fn os_version() -> Option<String> {
    momentum_core::diagnostic::os_release_name(&std::fs::read_to_string("/etc/os-release").ok()?)
}

/// Heure locale « HH:MM:SS ».
pub fn clock() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let t = libc::time_t::try_from(now).unwrap_or(0);
    // SAFETY: `localtime_r` remplit la structure fournie, réentrant.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&t, &mut tm) }.is_null() {
        return "--:--:--".to_string();
    }
    format!("{:02}:{:02}:{:02}", tm.tm_hour, tm.tm_min, tm.tm_sec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_entry_runs_the_quoted_executable() {
        let entry = desktop_entry("\"/home/a b/momentum-tray\"");
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("\nExec=\"/home/a b/momentum-tray\"\n"));
        assert!(entry.contains("\nType=Application\n"));
    }

    #[test]
    fn autostart_round_trip_in_a_temporary_config() {
        let dir = std::env::temp_dir().join(format!("momentum-autostart-{}", std::process::id()));
        let autostart = Autostart {
            path: Some(dir.join("autostart").join(AUTOSTART_FILE)),
        };
        assert!(!autostart.is_enabled());
        autostart.set(true).unwrap();
        assert!(autostart.is_enabled());
        autostart.set(false).unwrap();
        assert!(!autostart.is_enabled());
        // Désactiver deux fois n'est pas une erreur.
        autostart.set(false).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn clock_has_the_hh_mm_ss_shape() {
        let text = clock();
        assert_eq!(text.len(), 8);
        assert_eq!(&text[2..3], ":");
        assert_eq!(&text[5..6], ":");
    }
}
