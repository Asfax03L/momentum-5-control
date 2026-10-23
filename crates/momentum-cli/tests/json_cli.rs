//! Commandes locales du vrai binaire, sans bus ni données de l'utilisateur.

use serde_json::Value;
use std::process::{Command, Output};

fn ctl(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctl"))
        .args(args)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/bus-inexistant-ctl")
        .output()
        .unwrap()
}

#[test]
fn documentation_json_contient_un_seul_objet_et_le_texte_complet() {
    for args in [
        vec!["man"],
        vec!["completions", "bash"],
        vec!["completions", "zsh"],
        vec!["completions", "fish"],
        vec!["completions", "powershell"],
        vec!["completions", "elvish"],
    ] {
        let plain = ctl(&args);
        let mut json_args = args.clone();
        json_args.push("--json");
        let output = ctl(&json_args);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["contenu"], String::from_utf8(plain.stdout).unwrap());
    }
}

#[test]
fn scenes_json_sans_service_et_avec_erreurs_de_fichier() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("ctl-json-scenes-{}-{unique}", std::process::id()));
    let data = root.join("momentum-ctl-win");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("scenes.txt"),
        "[Démo]\nbruit = adaptatif\n[Invalide]\nniveau = 120\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_ctl"))
        .args(["scenes", "--json"])
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/bus-inexistant-ctl")
        .env("XDG_DATA_HOME", &root)
        .env("LOCALAPPDATA", &root)
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(output.status.success(), "{output:?}");
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["scenes"], serde_json::json!(["Démo"]));
    assert!(!value["avertissements"].as_array().unwrap().is_empty());
}
