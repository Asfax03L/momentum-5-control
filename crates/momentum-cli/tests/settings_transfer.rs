//! Les sous-commandes locales fonctionnent même sans bus ni service.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ctl-reglages-{}-{}",
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

fn ctl(root: &Path, args: &[&str], archive: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctl"))
        .arg("--json")
        .arg("reglages")
        .arg(args[0])
        .arg(archive)
        .args(&args[1..])
        .env("XDG_DATA_HOME", root)
        .env("LOCALAPPDATA", root)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", root.join("absent-session-bus").display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", root.join("absent-system-bus").display()),
        )
        .output()
        .unwrap()
}
fn object(output: &Output) -> serde_json::Value {
    let value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(String::from_utf8_lossy(&output.stdout).lines().count(), 1);
    value
}

#[test]
fn export_simulate_import_and_no_overwrite_work_without_service() {
    let temp = Temp::new();
    let source = temp.0.join("source");
    let target = temp.0.join("destination");
    let source_data = source.join("momentum-ctl-win");
    fs::create_dir_all(&source_data).unwrap();
    fs::write(
        source_data.join("apparence.txt"),
        "# Exemple\ntheme=sombre\n",
    )
    .unwrap();
    fs::write(source_data.join("batterie.csv"), "100;50\n").unwrap();
    let archive = temp.0.join("reglages.json");
    let output = ctl(&source, &["exporter"], &archive);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(object(&output)["ok"], true);
    let before = fs::read(&archive).unwrap();
    assert!(!String::from_utf8_lossy(&before).contains("batterie.csv"));
    let output = ctl(&source, &["exporter"], &archive);
    assert!(!output.status.success());
    assert_eq!(object(&output)["ok"], false);
    assert_eq!(fs::read(&archive).unwrap(), before);
    let output = ctl(&target, &["importer", "--simulation"], &archive);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(object(&output)["ok"], true);
    assert!(!target.exists(), "simulation sans création du dossier");
    let output = ctl(&target, &["importer"], &archive);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(object(&output)["ok"], true);
    assert_eq!(
        fs::read(target.join("momentum-ctl-win/apparence.txt")).unwrap(),
        fs::read(source_data.join("apparence.txt")).unwrap()
    );
}

#[test]
fn malformed_input_leaves_local_files_untouched_and_battery_is_opt_in() {
    let temp = Temp::new();
    let root = temp.0.join("donnees");
    let data = root.join("momentum-ctl-win");
    fs::create_dir_all(&data).unwrap();
    fs::write(data.join("batterie.csv"), "100;50\n").unwrap();
    let archive = temp.0.join("avec-batterie.json");
    let output = ctl(&root, &["exporter", "--avec-batterie"], &archive);
    assert!(output.status.success());
    assert!(
        fs::read_to_string(&archive)
            .unwrap()
            .contains("batterie.csv")
    );
    let dest = temp.0.join("destination");
    assert!(ctl(&dest, &["importer"], &archive).status.success());
    assert!(!dest.join("momentum-ctl-win/batterie.csv").exists());
    assert!(
        ctl(&dest, &["importer", "--avec-batterie"], &archive)
            .status
            .success()
    );
    assert_eq!(
        fs::read(dest.join("momentum-ctl-win/batterie.csv")).unwrap(),
        b"100;50\n"
    );
    fs::write(&archive, "{corrompu}").unwrap();
    let output = ctl(&dest, &["importer"], &archive);
    assert!(!output.status.success());
    assert_eq!(object(&output)["ok"], false);
    assert_eq!(
        fs::read(dest.join("momentum-ctl-win/batterie.csv")).unwrap(),
        b"100;50\n"
    );
    assert!(!dest.join("momentum-ctl-win/batterie.csv.ancien").exists());
}
