//! Sauvegarde portable des seuls réglages locaux, sans accès au service.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::{
    appearance, battery_log::BatteryLog, hotkeys, presets, quick_panel::PanelPrefs, scenes,
};

const FILES: [&str; 5] = [
    "scenes.txt",
    "prereglages.txt",
    "raccourcis.txt",
    "panneau.txt",
    "apparence.txt",
];
const BATTERY: &str = "batterie.csv";
const MAX_TEXT: u64 = 2 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("fichier JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("version de sauvegarde inconnue : {0}")]
    Version(u32),
    #[error("réglages refusés : {0}")]
    Invalid(String),
    #[error("opération impossible : {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    version: u32,
    fichiers: BTreeMap<String, Option<String>>,
}

fn invalid(message: impl Into<String>) -> TransferError {
    TransferError::Invalid(message.into())
}

fn read(path: &Path, limit: u64) -> Result<Option<String>, TransferError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() => return Err(invalid("un fichier ordinaire est requis")),
        Ok(meta) if meta.len() > limit => return Err(invalid("fichier trop volumineux")),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        return Err(invalid("fichier trop volumineux"));
    }
    Ok(Some(text))
}

fn active_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Les analyseurs d'affichage tolèrent certaines lignes incorrectes. L'import
/// les vérifie aussi individuellement pour ne pas accepter une perte silencieuse.
fn validate(name: &str, text: &str) -> Result<(), TransferError> {
    let bad = || invalid(format!("{name} contient une ligne invalide"));
    match name {
        "scenes.txt" => {
            let (list, errors) = scenes::parse_scenes(text);
            if !errors.is_empty() || scenes::check_scenes(&list).is_err() {
                return Err(bad());
            }
        }
        "prereglages.txt" => {
            let mut names = Vec::new();
            for line in active_lines(text) {
                let parsed = presets::parse_user_presets(line);
                let Some(preset) = parsed.first() else {
                    return Err(bad());
                };
                let key = preset.name.to_lowercase();
                if names.contains(&key) {
                    return Err(bad());
                }
                names.push(key);
            }
        }
        "raccourcis.txt" => {
            let (keys, errors) = hotkeys::parse_config(text);
            let entries: Vec<_> = keys
                .into_iter()
                .map(|(a, k)| (a, hotkeys::format_hotkey(&k)))
                .collect();
            if !errors.is_empty() || hotkeys::build_config(&entries).is_err() {
                return Err(bad());
            }
        }
        "panneau.txt" | "apparence.txt" => {
            let mut keys = Vec::new();
            for line in active_lines(text) {
                let line = line.split('#').next().unwrap_or("").trim();
                let Some((key, value)) = line.split_once('=') else {
                    return Err(bad());
                };
                let key = key.trim();
                let value = value.trim();
                if keys.contains(&key) {
                    return Err(bad());
                }
                keys.push(key);
                if name == "apparence.txt" {
                    if key != "theme" || appearance::parse(line).is_none() {
                        return Err(bad());
                    }
                } else {
                    match key {
                        "bruit" | "niveau" | "profil" if matches!(value, "oui" | "non") => {}
                        "scenes" => {
                            let names: Vec<_> = value.split(',').map(str::trim).collect();
                            if !value.is_empty()
                                && (names.len() > crate::quick_panel::MAX_PANEL_SCENES
                                    || names.iter().any(|n| scenes::validate_name(n).is_err()))
                            {
                                return Err(bad());
                            }
                        }
                        _ => return Err(bad()),
                    }
                    let _ = PanelPrefs::parse(line);
                }
            }
        }
        BATTERY => {
            for line in active_lines(text) {
                let Some((time, _)) = line.split_once(';') else {
                    return Err(bad());
                };
                let time: u64 = time.parse().map_err(|_| bad())?;
                if time > u64::MAX - 60 || BatteryLog::parse(line, time).entries().len() != 1 {
                    return Err(bad());
                }
            }
        }
        _ => return Err(invalid("fichier non autorisé dans la sauvegarde")),
    }
    Ok(())
}

fn decode(text: &str) -> Result<Archive, TransferError> {
    if text.len() as u64 > MAX_ARCHIVE {
        return Err(invalid("sauvegarde trop volumineuse"));
    }
    let archive: Archive = serde_json::from_str(text)?;
    if archive.version != 1 {
        return Err(TransferError::Version(archive.version));
    }
    if FILES
        .iter()
        .any(|name| !archive.fichiers.contains_key(*name))
    {
        return Err(invalid(
            "la liste des cinq fichiers de réglages est incomplète",
        ));
    }
    for (name, text) in &archive.fichiers {
        if !FILES.contains(&name.as_str()) && name != BATTERY {
            return Err(invalid("fichier non autorisé dans la sauvegarde"));
        }
        if let Some(text) = text {
            if text.len() as u64 > MAX_TEXT {
                return Err(invalid("fichier trop volumineux"));
            }
            validate(name, text)?;
        }
    }
    Ok(archive)
}

/// Exporte les textes exacts. `null` représente un fichier absent. Le fichier
/// choisi doit être nouveau ; aucun profil ni identifiant d'appareil n'est lu.
pub fn export(dir: &Path, destination: &Path, battery: bool) -> Result<String, TransferError> {
    let mut fichiers = BTreeMap::new();
    for name in FILES.into_iter().chain(battery.then_some(BATTERY)) {
        let text = read(&dir.join(name), MAX_TEXT)?;
        if let Some(text) = &text {
            validate(name, text)?;
        }
        fichiers.insert(name.to_string(), text);
    }
    let text = serde_json::to_string_pretty(&Archive {
        version: 1,
        fichiers,
    })?;
    if text.len() as u64 > MAX_ARCHIVE {
        return Err(invalid("sauvegarde trop volumineuse"));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    Ok("Réglages exportés ; le fichier existant n'est jamais écrasé.".into())
}

#[derive(Debug)]
struct Change {
    name: String,
    before: Option<String>,
    after: String,
}

/// Aperçu immuable : le contenu validé est conservé jusqu'à la confirmation.
#[derive(Debug)]
pub struct ImportPlan {
    dir: PathBuf,
    changes: Vec<Change>,
    summary: String,
}

impl ImportPlan {
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Refuse un changement des fichiers depuis l'aperçu. Les `.ancien`
    /// existants ne sont jamais écrasés : les déplacer avant un nouvel import.
    pub fn apply(&self) -> Result<String, TransferError> {
        if self.changes.is_empty() {
            return Ok("Aucun fichier à modifier.".into());
        }
        self.check_current()?;
        fs::create_dir_all(&self.dir)?;
        let lock_path = self.dir.join(".reglages-import.lock");
        let lock = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)?;
        let _guard = TempFile {
            path: lock_path,
            _file: Some(lock),
        };
        self.check_current()?;
        let mut staged = Vec::new();
        for change in &self.changes {
            staged.push(stage(&self.dir, &change.after)?);
        }
        // Toutes les sauvegardes sont créées avant de remplacer le premier texte.
        // Elles restent disponibles même après une erreur d'entrée/sortie.
        for change in &self.changes {
            if let Some(old) = &change.before {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.dir.join(format!("{}.ancien", change.name)))?;
                file.write_all(old.as_bytes())?;
                file.sync_all()?;
            }
        }
        for (index, (change, temp)) in self.changes.iter().zip(&staged).enumerate() {
            let result = self.check_one(change).and_then(|()| {
                fs::rename(&temp.path, self.dir.join(&change.name)).map_err(TransferError::from)
            });
            if let Err(error) = result {
                let restored = self.rollback(index);
                return Err(invalid(format!(
                    "{error} ; {}",
                    if restored {
                        "textes précédents restaurés ; sauvegardes .ancien conservées"
                    } else {
                        "restauration incomplète : consulter les fichiers .ancien"
                    }
                )));
            }
        }
        Ok(format!(
            "{} fichier(s) importé(s). Les anciens textes sont conservés en .ancien. Relancer l'interface pour recharger tous les réglages.",
            self.changes.len()
        ))
    }

    fn check_one(&self, change: &Change) -> Result<(), TransferError> {
        if read(&self.dir.join(&change.name), MAX_TEXT)? != change.before {
            return Err(invalid(format!(
                "{} a changé depuis l'aperçu : refaire la simulation",
                change.name
            )));
        }
        Ok(())
    }

    fn check_current(&self) -> Result<(), TransferError> {
        for change in &self.changes {
            self.check_one(change)?;
            if change.before.is_some()
                && fs::symlink_metadata(self.dir.join(format!("{}.ancien", change.name))).is_ok()
            {
                return Err(invalid(format!(
                    "{}.ancien existe déjà : le conserver ailleurs avant de recommencer",
                    change.name
                )));
            }
        }
        Ok(())
    }

    fn rollback(&self, written: usize) -> bool {
        let mut ok = true;
        for change in self.changes[..written].iter().rev() {
            let path = self.dir.join(&change.name);
            let restored = match read(&path, MAX_TEXT) {
                Ok(Some(current)) if current == change.after => match &change.before {
                    Some(old) => stage(&self.dir, old).and_then(|temp| {
                        fs::rename(&temp.path, &path).map_err(TransferError::from)
                    }),
                    None => fs::remove_file(&path).map_err(TransferError::from),
                },
                _ => Err(invalid("fichier modifié ailleurs pendant la restauration")),
            };
            ok &= restored.is_ok();
        }
        ok
    }
}

struct TempFile {
    path: PathBuf,
    _file: Option<fs::File>,
}
impl Drop for TempFile {
    fn drop(&mut self) {
        self._file.take();
        let _ = fs::remove_file(&self.path);
    }
}

fn stage(dir: &Path, text: &str) -> Result<TempFile, TransferError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = dir.join(format!(
        ".reglages-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut temp = TempFile {
        _file: Some(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        ),
        path,
    };
    if let Some(file) = temp._file.as_mut() {
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
    }
    temp._file.take();
    Ok(temp)
}

/// Valide toute l'archive et prépare le résumé sans créer ni modifier de fichier.
/// Un fichier absent dans la sauvegarde ne supprime pas celui du PC destinataire.
pub fn prepare_import(
    dir: &Path,
    source: &Path,
    battery: bool,
) -> Result<ImportPlan, TransferError> {
    let text = read(source, MAX_ARCHIVE)?.ok_or_else(|| invalid("sauvegarde introuvable"))?;
    let archive = decode(&text)?;
    let mut changes = Vec::new();
    let mut lines = vec!["Import des réglages locaux (aucun accès au casque) :".to_string()];
    for (name, text) in archive.fichiers {
        if name == BATTERY && !battery {
            lines.push("batterie.csv : ignoré (option explicite requise)".into());
            continue;
        }
        let Some(after) = text else {
            lines.push(format!(
                "{name} : absent de la sauvegarde, conservé sur ce PC"
            ));
            continue;
        };
        let before = read(&dir.join(&name), MAX_TEXT)?;
        lines.push(format!(
            "{name} : {}",
            if before.as_ref() == Some(&after) {
                "identique"
            } else if before.is_some() {
                "remplacer (sauvegarde .ancien)"
            } else {
                "créer"
            }
        ));
        if name == "raccourcis.txt"
            && !hotkeys::parse_config(&after).0.is_empty()
            && cfg!(target_os = "linux")
        {
            lines.push("Raccourcis conservés au format Windows ; sous KDE, les configurer dans Paramètres système > Raccourcis. Ils ne sont pas enregistrés automatiquement dans KDE.".into());
        }
        if before.as_ref() != Some(&after) {
            changes.push(Change {
                name,
                before,
                after,
            });
        }
    }
    lines.push("Les autres fichiers et les réglages du casque restent inchangés. Relancer l'interface après import.".into());
    Ok(ImportPlan {
        dir: dir.to_path_buf(),
        changes,
        summary: lines.join("\n"),
    })
}
