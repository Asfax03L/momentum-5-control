use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use momentum_core::settings_transfer::{export, prepare_import};
use serde_json::{Value, json};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "reglages-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn example() -> Value {
    json!({"version":1,"fichiers":{
        "scenes.txt":"# Exemple\r\n[Démo]\r\nbruit=arret\r\ntactile=on\r\n",
        "prereglages.txt":"# Courbe locale\nDémo = 0;10;-20;0;0;0;0;0\n",
        "raccourcis.txt":"# Portable\nscene:Démo = Win+Alt+F1\nfenetre = Ctrl+Alt+M\n",
        "panneau.txt":"bruit=oui\nprofil=non\nscenes=Démo\n",
        "apparence.txt":"# Choix conservé\ntheme = sombre # commentaire\n"
    }})
}
fn write_archive(root: &Path, value: &Value) -> PathBuf {
    let path = root.join("sauvegarde.json");
    fs::write(&path, serde_json::to_string(value).unwrap()).unwrap();
    path
}
fn texts(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut result: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().to_string(),
                fs::read(path).unwrap(),
            )
        })
        .collect();
    result.sort();
    result
}

#[test]
fn round_trip_preserves_text_and_keeps_old_files() {
    let temp = Temp::new();
    let original = example();
    let source = temp.0.join("source");
    let dest = temp.0.join("destination");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&dest).unwrap();
    for (name, text) in original["fichiers"].as_object().unwrap() {
        fs::write(source.join(name), text.as_str().unwrap()).unwrap();
        fs::write(dest.join(name), "# Réglages précédents\n").unwrap();
    }
    // Ces fichiers ne font jamais partie de l'archive.
    fs::write(source.join("profil-simule.json"), "NE_PAS_EXPORTER").unwrap();
    fs::write(source.join("casque.txt"), "IDENTIFIANT_FICTIF").unwrap();
    fs::write(source.join("batterie.csv"), "100;50\n").unwrap();
    let archive = temp.0.join("portable.json");
    export(&source, &archive, false).unwrap();
    let text = fs::read_to_string(&archive).unwrap();
    assert!(
        !text.contains("NE_PAS_EXPORTER")
            && !text.contains("IDENTIFIANT_FICTIF")
            && !text.contains("batterie.csv")
    );
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), original);
    let before = texts(&dest);
    let plan = prepare_import(&dest, &archive, false).unwrap();
    assert_eq!(texts(&dest), before, "la simulation ne doit rien écrire");
    assert!(plan.summary().contains("remplacer"));
    if cfg!(target_os = "linux") {
        assert!(plan.summary().contains("KDE"));
    }
    plan.apply().unwrap();
    for (name, text) in original["fichiers"].as_object().unwrap() {
        assert_eq!(
            fs::read_to_string(dest.join(name)).unwrap(),
            text.as_str().unwrap()
        );
        assert_eq!(
            fs::read_to_string(dest.join(format!("{name}.ancien"))).unwrap(),
            "# Réglages précédents\n"
        );
    }
    assert!(!dest.join(".reglages-import.lock").exists());
    let before = texts(&dest);
    prepare_import(&dest, &archive, false)
        .unwrap()
        .apply()
        .unwrap();
    assert_eq!(
        texts(&dest),
        before,
        "réimporter un texte identique est sans effet"
    );
    assert!(export(&source, &archive, false).is_err());
    assert_eq!(fs::read_to_string(&archive).unwrap(), text);
}

#[test]
fn corrupted_unknown_and_partly_invalid_archives_never_write() {
    let temp = Temp::new();
    let dest = temp.0.join("destination");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("apparence.txt"), "theme=clair\n").unwrap();
    let before = texts(&dest);
    for (name, bad) in [
        ("scenes.txt", "[Démo]\ninconnu=on\n"),
        ("prereglages.txt", "Démo=0;pas-un-nombre\n"),
        ("raccourcis.txt", "fenetre=F1\n"),
        ("panneau.txt", "bruit=peut-être\n"),
        ("apparence.txt", "theme=violet\n"),
    ] {
        let mut value = example();
        value["fichiers"][name] = json!(bad);
        let archive = write_archive(&temp.0, &value);
        assert!(prepare_import(&dest, &archive, false).is_err(), "{name}");
        assert_eq!(texts(&dest), before);
    }
    for value in [
        json!({"version":2,"fichiers":{}}),
        json!({"version":1,"fichiers":{}}),
        {
            let mut v = example();
            v["fichiers"]["../profil.json"] = json!("donnée fictive");
            v
        },
        {
            let mut v = example();
            v["profil"] = json!("donnée fictive");
            v
        },
    ] {
        let archive = write_archive(&temp.0, &value);
        assert!(prepare_import(&dest, &archive, false).is_err());
        assert_eq!(texts(&dest), before);
    }
    let archive = temp.0.join("corrompu.json");
    fs::write(&archive, b"{\"version\":1,").unwrap();
    let absent = temp.0.join("pas-encore-de-donnees");
    assert!(prepare_import(&absent, &archive, false).is_err());
    assert!(!absent.exists());
}

#[test]
fn missing_files_and_battery_require_explicit_consent() {
    let temp = Temp::new();
    let source = temp.0.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("batterie.csv"), "# Historique\n100;50\n101;-\n").unwrap();
    let archive = temp.0.join("avec-batterie.json");
    export(&source, &archive, true).unwrap();
    let dest = temp.0.join("destination");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("apparence.txt"), "theme=clair\n").unwrap();
    let plan = prepare_import(&dest, &archive, false).unwrap();
    assert!(plan.summary().contains("ignoré"));
    plan.apply().unwrap();
    assert!(!dest.join("batterie.csv").exists());
    assert_eq!(
        fs::read_to_string(dest.join("apparence.txt")).unwrap(),
        "theme=clair\n"
    );
    prepare_import(&dest, &archive, true)
        .unwrap()
        .apply()
        .unwrap();
    assert_eq!(
        fs::read(dest.join("batterie.csv")).unwrap(),
        fs::read(source.join("batterie.csv")).unwrap()
    );
}

#[test]
fn confirmation_uses_validated_content_and_refuses_changed_targets_or_backups() {
    let temp = Temp::new();
    let dest = temp.0.join("destination");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("apparence.txt"), "theme=clair\n").unwrap();
    let archive = write_archive(&temp.0, &example());
    let plan = prepare_import(&dest, &archive, false).unwrap();
    fs::write(
        dest.join("apparence.txt"),
        "# Modifié ailleurs\ntheme=clair\n",
    )
    .unwrap();
    let before = texts(&dest);
    assert!(plan.apply().is_err());
    assert_eq!(texts(&dest), before);
    let plan = prepare_import(&dest, &archive, false).unwrap();
    fs::write(dest.join("apparence.txt.ancien"), "sauvegarde à conserver").unwrap();
    let before = texts(&dest);
    assert!(plan.apply().is_err());
    assert_eq!(texts(&dest), before);
    fs::remove_file(dest.join("apparence.txt.ancien")).unwrap();
    let plan = prepare_import(&dest, &archive, false).unwrap();
    fs::write(&archive, "JSON remplacé après l'aperçu").unwrap();
    plan.apply().unwrap();
    assert_eq!(
        fs::read_to_string(dest.join("apparence.txt")).unwrap(),
        example()["fichiers"]["apparence.txt"].as_str().unwrap()
    );
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_redirect_an_import_or_export() {
    use std::os::unix::fs::symlink;
    let temp = Temp::new();
    let dir = temp.0.join("donnees");
    fs::create_dir(&dir).unwrap();
    let outside = temp.0.join("hors-donnees.txt");
    fs::write(&outside, "theme=clair\n").unwrap();
    symlink(&outside, dir.join("apparence.txt")).unwrap();
    let archive = write_archive(&temp.0, &example());
    assert!(prepare_import(&dir, &archive, false).is_err());
    assert!(export(&dir, &temp.0.join("export.json"), false).is_err());
    assert_eq!(fs::read_to_string(&outside).unwrap(), "theme=clair\n");
}
