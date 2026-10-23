//! Génération réelle sans service ni casque, y compris sous Windows.

use std::process::{Command, Output};

fn ctl(args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_ctl"))
        .args(args)
        // Toute tentative d'accès au bus doit échouer : la génération est locale.
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/bus-inexistant-ctl")
        .output()
        .expect("lancement de ctl");
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    output
}

#[test]
fn completions_des_cinq_shells_sans_casque() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let output = ctl(&["completions", shell]);
        let text = String::from_utf8(output.stdout).unwrap();
        for name in ["ctl", "etat", "scene", "mode", "reconnecter"] {
            assert!(text.contains(name), "{shell} : {name} absent");
        }
        #[cfg(target_os = "linux")]
        assert!(text.contains("canal"), "{shell} : option Linux absente");
    }
}

#[test]
fn manuel_sans_casque() {
    let output = ctl(&["man"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains(".TH ctl 1"));
    for name in [
        "etat",
        "scene",
        "mode",
        "completions",
        "MOMENTUM 5",
        "reconnecter",
    ] {
        assert!(text.contains(name), "{name} absent du manuel");
    }
}
