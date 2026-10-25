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

/// Lit `raccourcis.txt`. Absent seulement : créé avec les valeurs par
/// défaut. Illisible (autre encodage, droits) : `Err`, le fichier reste tel
/// quel (A05 de l'audit du 24/10/2026).
pub fn hotkey_config() -> std::io::Result<String> {
    let path =
        hotkeys_file().ok_or_else(|| std::io::Error::other("dossier des données introuvable"))?;
    read_or_create_hotkeys(&path)
}

fn read_or_create_hotkeys(path: &std::path::Path) -> std::io::Result<String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir)?;
            }
            // Création exclusive : un fichier apparu entre-temps est gardé.
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(crate::hotkeys::DEFAULT_CONFIG.as_bytes())?;
                    Ok(crate::hotkeys::DEFAULT_CONFIG.to_string())
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => fs::read_to_string(path),
                Err(e) => Err(e),
            }
        }
        Err(e) => Err(e),
    }
}

/// Raccourcis à utiliser : ceux de `raccourcis.txt`, ou les valeurs par
/// défaut (en mémoire seulement) si le fichier est illisible.
pub fn load_hotkey_config() -> String {
    hotkey_config().unwrap_or_else(|e| {
        tracing::warn!(%e, "raccourcis.txt illisible : gardé tel quel, valeurs par défaut");
        crate::hotkeys::DEFAULT_CONFIG.to_string()
    })
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

/// Verrou des fichiers d'un dossier de données, commun à tous les
/// processus (interfaces, service, `ctl`, import) : chaque écriture, et
/// chaque modification « lire, vérifier, remplacer », se fait d'un bloc
/// (A03 de l'audit du 24/10/2026). Libéré à la fin de vie de la valeur.
pub struct DirLock {
    _file: fs::File,
}

/// Prend le verrou du dossier `dir` (fichier `.verrou`), en attendant
/// qu'un autre auteur ait fini.
pub fn lock_dir(dir: &std::path::Path) -> std::io::Result<DirLock> {
    fs::create_dir_all(dir)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(".verrou"))?;
    file.lock()?;
    Ok(DirLock { _file: file })
}

fn lock_parent(path: &std::path::Path) -> std::io::Result<DirLock> {
    lock_dir(path.parent().unwrap_or_else(|| std::path::Path::new(".")))
}

/// Modifie un fichier de l'utilisateur d'un bloc, sous le verrou de son
/// dossier : `change` reçoit le contenu actuel (`None` : absent) et rend le
/// nouveau (`None` : rien à écrire). Rend `true` si le fichier a été écrit.
pub fn update_user_file(
    path: &std::path::Path,
    change: impl FnOnce(Option<&str>) -> Result<Option<String>, String>,
) -> Result<bool, String> {
    let _lock = lock_parent(path).map_err(|e| e.to_string())?;
    let current = match fs::read_to_string(path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.to_string()),
    };
    match change(current.as_deref())? {
        Some(text) => write_locked(path, &text)
            .map(|()| true)
            .map_err(|e| e.to_string()),
        None => Ok(false),
    }
}

/// Écrit un fichier de l'utilisateur sans le laisser à moitié écrit :
/// l'ancien contenu est gardé dans `<nom>.bak`, le nouveau est écrit dans
/// `<nom>.tmp` puis remplace le fichier. Sous le verrou du dossier.
pub fn write_user_file(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    let _lock = lock_parent(path)?;
    write_locked(path, text)
}

/// [`write_user_file`], verrou déjà pris.
fn write_locked(path: &std::path::Path, text: &str) -> std::io::Result<()> {
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
    // Nom temporaire propre à cette écriture : deux interfaces qui
    // enregistrent en même temps ne se marchent pas dessus (F14).
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = with_ext(&format!(".{}-{n}.tmp", std::process::id()));
    if let Err(e) = fs::write(&tmp, text).and_then(|()| fs::rename(&tmp, path)) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

/// Écrit `text` dans `path` seulement si le fichier est toujours tel qu'il
/// a été lu (`loaded`, `None` : absent) : une modification faite ailleurs
/// entre-temps (autre interface, éditeur de texte) n'est pas écrasée (F14).
pub fn write_if_unchanged(
    path: &std::path::Path,
    loaded: Option<&str>,
    text: &str,
) -> Result<(), String> {
    update_user_file(path, |current| {
        if current != loaded {
            return Err(format!(
                "{} a été modifié ailleurs depuis son ouverture : relu, à refaire",
                path.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ));
        }
        Ok(Some(text.to_string()))
    })
    .map(|_| ())
}

/// Texte de `panneau.txt`, s'il existe.
pub fn read_panel_text() -> Option<String> {
    fs::read_to_string(panel_file()?).ok()
}

/// Modifie les préréglages personnels du fichier `path` : relus juste avant
/// la modification (une autre interface a pu en ajouter), puis réécrits.
/// Rend la liste à jour.
fn modify_presets(
    path: &std::path::Path,
    change: impl FnOnce(&mut Vec<Preset>) -> Result<(), String>,
) -> Result<Vec<Preset>, String> {
    let mut list = Vec::new();
    update_user_file(path, |current| {
        list = current.map(presets::parse_user_presets).unwrap_or_default();
        change(&mut list)?;
        Ok(Some(presets::serialize_user_presets(&list)))
    })?;
    Ok(list)
}

/// Ajoute (ou remplace) le préréglage personnel `name` ; rend la liste à
/// jour, avec ceux enregistrés entre-temps par une autre interface.
pub fn upsert_user_preset(name: &str, gains: &[i8]) -> Result<Vec<Preset>, String> {
    let path = presets_file().ok_or("dossier des données introuvable")?;
    modify_presets(&path, |list| {
        presets::upsert(list, name, gains).map_err(str::to_string)
    })
}

/// Supprime le préréglage personnel `name` (absent : rien à faire) ; rend
/// la liste à jour.
pub fn remove_user_preset(name: &str) -> Result<Vec<Preset>, String> {
    let path = presets_file().ok_or("dossier des données introuvable")?;
    modify_presets(&path, |list| {
        presets::remove(list, name);
        Ok(())
    })
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
        // Aucun fichier temporaire ne reste.
        let leftovers = fs::read_dir(&dir)
            .unwrap()
            .filter(|e| e.as_ref().unwrap().path().extension() == Some("tmp".as_ref()))
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_missing_hotkey_file_is_created() {
        // A05 : un fichier présent mais illisible (UTF-16) reste intact.
        let dir = std::env::temp_dir().join(format!(
            "momentum-ctl-win-test-raccourcis-{}",
            std::process::id()
        ));
        let path = dir.join("raccourcis.txt");
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(
            read_or_create_hotkeys(&path).unwrap(),
            crate::hotkeys::DEFAULT_CONFIG
        );
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            crate::hotkeys::DEFAULT_CONFIG
        );
        fs::write(&path, "scene Transport = Ctrl+Alt+T\n").unwrap();
        assert_eq!(
            read_or_create_hotkeys(&path).unwrap(),
            "scene Transport = Ctrl+Alt+T\n"
        );
        let utf16 = [0xFF, 0xFE, b's', 0, b'c', 0, 0x0A, 0];
        fs::write(&path, utf16).unwrap();
        assert!(read_or_create_hotkeys(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), utf16);
        assert!(!dir.join("raccourcis.txt.bak").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_checked_writes_never_both_succeed() {
        // A03 : deux auteurs partent du même contenu ; l'un écrit, l'autre
        // apprend le conflit. Jamais deux succès avec une perte.
        let dir = std::env::temp_dir().join(format!(
            "momentum-ctl-win-test-course-{}",
            std::process::id()
        ));
        let path = dir.join("scenes.txt");
        for round in 0..200 {
            write_user_file(&path, "départ").unwrap();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let writers: Vec<_> = ["A", "B"]
                .into_iter()
                .map(|text| {
                    let (path, barrier) = (path.clone(), std::sync::Arc::clone(&barrier));
                    std::thread::spawn(move || {
                        barrier.wait();
                        write_if_unchanged(&path, Some("départ"), text).is_ok()
                    })
                })
                .collect();
            let ok: Vec<bool> = writers.into_iter().map(|w| w.join().unwrap()).collect();
            assert_eq!(
                ok.iter().filter(|&&o| o).count(),
                1,
                "tour {round} : {ok:?}"
            );
            let written = fs::read_to_string(&path).unwrap();
            assert_eq!(written, if ok[0] { "A" } else { "B" });
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_preset_changes_are_both_kept() {
        let dir = std::env::temp_dir().join(format!(
            "momentum-ctl-win-test-presets-course-{}",
            std::process::id()
        ));
        let path = dir.join("prereglages.txt");
        for round in 0..50 {
            let _ = fs::remove_file(&path);
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let writers: Vec<_> = ["Soirée", "Train"]
                .into_iter()
                .map(|name| {
                    let (path, barrier) = (path.clone(), std::sync::Arc::clone(&barrier));
                    std::thread::spawn(move || {
                        barrier.wait();
                        modify_presets(&path, |list| {
                            presets::upsert(list, name, &[0; 8]).map_err(str::to_string)
                        })
                        .unwrap();
                    })
                })
                .collect();
            writers.into_iter().for_each(|w| w.join().unwrap());
            let names: Vec<String> =
                presets::parse_user_presets(&fs::read_to_string(&path).unwrap())
                    .into_iter()
                    .map(|p| p.name)
                    .collect();
            assert_eq!(names.len(), 2, "tour {round} : {names:?}");
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_changed_elsewhere_is_not_overwritten() {
        let dir = std::env::temp_dir().join(format!(
            "momentum-ctl-win-test-conflit-{}",
            std::process::id()
        ));
        let path = dir.join("raccourcis.txt");
        write_user_file(&path, "lu").unwrap();
        // Modifié ailleurs après la lecture : refusé, contenu intact.
        write_user_file(&path, "ailleurs").unwrap();
        assert!(write_if_unchanged(&path, Some("lu"), "mien").is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "ailleurs");
        // Inchangé depuis la lecture : écrit.
        write_if_unchanged(&path, Some("ailleurs"), "mien").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "mien");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_preset_saved_elsewhere_is_not_lost() {
        // F14 : l'interface A a chargé la liste avant que B n'ajoute
        // « Soirée » ; A ajoute « Train » : les deux restent.
        let dir = std::env::temp_dir().join(format!(
            "momentum-ctl-win-test-presets-{}",
            std::process::id()
        ));
        let path = dir.join("prereglages.txt");
        let add = |name: &str| {
            modify_presets(&path, |list| {
                presets::upsert(list, name, &[0; 8]).map_err(str::to_string)
            })
            .unwrap()
        };
        add("Soirée");
        let list = add("Train");
        let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Soirée", "Train"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
