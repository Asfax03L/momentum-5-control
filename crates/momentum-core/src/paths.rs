//! Fichiers de l'utilisateur, dans `%LOCALAPPDATA%\momentum-ctl-win`
//! (Windows) ou `~/.local/share/momentum-ctl-win` (Linux) : préréglages et
//! scènes, partagés par l'icône et `ctl`.

use std::fs;
use std::path::PathBuf;

use crate::presets::{self, Preset};
use crate::quick_panel::PanelPrefs;
use crate::scenes::{self, Scene};

/// Dossier des données de l'application.
#[cfg(not(target_os = "linux"))]
pub fn data_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("momentum-ctl-win"))
}

/// Dossier des données de l'application : `$XDG_DATA_HOME`, sinon
/// `~/.local/share` (spécification XDG).
#[cfg(target_os = "linux")]
pub fn data_dir() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => PathBuf::from(std::env::var_os("HOME")?).join(".local/share"),
    };
    Some(base.join("momentum-ctl-win"))
}

/// `raccourcis.txt` : raccourcis clavier globaux (Windows).
pub fn hotkeys_file() -> Option<PathBuf> {
    Some(data_dir()?.join("raccourcis.txt"))
}

/// Lit `raccourcis.txt`, créé avec les valeurs par défaut s'il manque.
pub fn load_hotkey_config() -> String {
    let path = hotkeys_file();
    match path.as_ref().map(fs::read_to_string) {
        Some(Ok(text)) => text,
        _ => {
            if let Some(p) = &path {
                let _ = p.parent().map(fs::create_dir_all);
                let _ = fs::write(p, crate::hotkeys::DEFAULT_CONFIG);
            }
            crate::hotkeys::DEFAULT_CONFIG.to_string()
        }
    }
}

/// `prereglages.txt` : préréglages personnels de l'égaliseur.
pub fn presets_file() -> Option<PathBuf> {
    Some(data_dir()?.join("prereglages.txt"))
}

/// `batterie.csv` : historique local de la batterie.
pub fn battery_file() -> Option<PathBuf> {
    Some(data_dir()?.join("batterie.csv"))
}

/// `casque.txt` : empreinte du casque utilisé (sans son adresse), pour le
/// retrouver même renommé ou à côté d'un autre casque.
pub fn headset_file() -> Option<PathBuf> {
    Some(data_dir()?.join("casque.txt"))
}

/// Empreinte du casque mémorisé, s'il y en a un.
pub fn load_headset_fingerprint() -> Option<String> {
    let text = fs::read_to_string(headset_file()?).ok()?;
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "empreinte")
            .then(|| value.trim().to_string())
            .filter(|v| v.len() == 16 && v.bytes().all(|b| b.is_ascii_hexdigit()))
    })
}

/// Mémorise le casque utilisé (empreinte seulement) ; rien n'est écrit si
/// c'est déjà lui.
pub fn store_headset_fingerprint(fingerprint: &str) -> std::io::Result<()> {
    if load_headset_fingerprint().as_deref() == Some(fingerprint) {
        return Ok(());
    }
    let path =
        headset_file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    write_user_file(
        &path,
        &format!(
            "# Casque utilisé par OpenHeadset Control (empreinte, sans adresse).\n\
             # Supprimer ce fichier pour en choisir un autre.\nempreinte = {fingerprint}\n"
        ),
    )
}

/// `scenes.txt` : scènes de l'utilisateur.
pub fn scenes_file() -> Option<PathBuf> {
    Some(data_dir()?.join("scenes.txt"))
}

/// Préréglages personnels (fichier absent ou illisible : aucun).
pub fn load_user_presets() -> Vec<Preset> {
    presets_file()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| presets::parse_user_presets(&text))
        .unwrap_or_default()
}

/// Préréglages utilisables par une scène : ceux de l'app (égaliseur du M5,
/// 8 bandes) puis les personnels.
pub fn scene_presets() -> Vec<Preset> {
    let mut all = presets::builtin_presets(8);
    all.extend(load_user_presets());
    all
}

/// Lit les scènes ; crée le fichier avec les exemples s'il n'existe pas.
/// Renvoie les scènes valides et les erreurs (numéros de ligne).
pub fn load_scenes() -> (Vec<Scene>, Vec<String>) {
    let Some(path) = scenes_file() else {
        return scenes::parse_scenes(scenes::DEFAULT_SCENES);
    };
    match fs::read_to_string(&path) {
        Ok(text) => scenes::parse_scenes(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let created = path
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| {
                    // Jamais d'écrasement : le fichier n'est créé que s'il manque.
                    use std::io::Write;
                    fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .and_then(|mut f| f.write_all(scenes::DEFAULT_SCENES.as_bytes()))
                });
            let (list, mut errors) = scenes::parse_scenes(scenes::DEFAULT_SCENES);
            if let Err(e) = created {
                errors.push(format!("{} non créé : {e}", path.display()));
            }
            (list, errors)
        }
        Err(e) => (
            Vec::new(),
            vec![format!("{} illisible : {e}", path.display())],
        ),
    }
}

/// `panneau.txt` : contenu du panneau rapide.
pub fn panel_file() -> Option<PathBuf> {
    Some(data_dir()?.join("panneau.txt"))
}

/// Préférences du panneau rapide (fichier absent ou illisible : défaut).
pub fn load_panel_prefs() -> PanelPrefs {
    panel_file()
        .and_then(|p| fs::read_to_string(p).ok())
        .map(|text| PanelPrefs::parse(&text))
        .unwrap_or_default()
}

/// Enregistre les préréglages personnels (`prereglages.txt`).
pub fn store_user_presets(list: &[Preset]) -> std::io::Result<()> {
    let path =
        presets_file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    write_user_file(&path, &presets::serialize_user_presets(list))
}

/// Dossier des fichiers exportés : Documents s'il existe, sinon le
/// dossier personnel.
pub fn export_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE");
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    let home = PathBuf::from(home?);
    let documents = home.join("Documents");
    Some(if documents.is_dir() { documents } else { home })
}

/// Fichier exporté daté : `<prefixe>-<date>.<extension>` dans
/// [`export_dir`].
pub fn export_file(prefix: &str, extension: &str, unix_now: u64) -> Option<PathBuf> {
    Some(export_dir()?.join(format!(
        "{prefix}-{}.{extension}",
        crate::profile::file_stamp(unix_now)
    )))
}

/// Écrit un fichier de l'utilisateur sans le laisser à moitié écrit :
/// l'ancien contenu est gardé dans `<nom>.bak`, le nouveau est écrit dans
/// `<nom>.tmp` puis remplace le fichier.
pub fn write_user_file(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let with_ext = |ext: &str| {
        let mut name = path.as_os_str().to_owned();
        name.push(ext);
        PathBuf::from(name)
    };
    if path.exists() {
        fs::copy(path, with_ext(".bak"))?;
    }
    let tmp = with_ext(".tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_files_are_replaced_with_a_backup() {
        let dir =
            std::env::temp_dir().join(format!("momentum-ctl-win-test-{}", std::process::id()));
        let path = dir.join("scenes.txt");
        write_user_file(&path, "un").unwrap();
        assert!(!dir.join("scenes.txt.bak").exists());
        write_user_file(&path, "deux").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "deux");
        assert_eq!(
            fs::read_to_string(dir.join("scenes.txt.bak")).unwrap(),
            "un"
        );
        assert!(!dir.join("scenes.txt.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
