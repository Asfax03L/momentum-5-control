//! Fenêtres de l'interface Slint sous Linux. Même interface que
//! `window.rs` (Windows).
//!
//! Sous Wayland, une application ne connaît ni la position du pointeur ni
//! celle de l'icône, et ne place pas ses fenêtres elle-même : le panneau
//! rapide s'ouvre là où le bureau le décide.

use std::path::PathBuf;
use std::process::Command;

/// Zone de travail inconnue : l'appelant prend sa valeur par défaut.
pub fn work_area_at(_x: i32, _y: i32) -> Option<(i32, i32, i32, i32)> {
    None
}

/// Position du pointeur inconnue sous Wayland.
pub fn cursor_position() -> (i32, i32) {
    (0, 0)
}

/// Le bureau donne lui-même le premier plan à une fenêtre qui s'affiche.
pub fn focus_panel() {}

/// Premier plan inconnu : le panneau ne se ferme pas tout seul (Échap ou
/// le bouton de fermeture le cachent).
pub fn panel_has_focus() -> bool {
    false
}

/// Boîte « Ouvrir » du bureau pour une courbe d'égaliseur (*.txt) :
/// `kdialog` (KDE), sinon `zenity` (GNOME). `None` si l'utilisateur annule
/// ou si aucun des deux n'est installé.
pub fn pick_curve_file() -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let attempts: [(&str, Vec<String>); 2] = [
        (
            "kdialog",
            vec![
                "--title".into(),
                "Importer une courbe d'égaliseur".into(),
                "--getopenfilename".into(),
                home,
                "Courbes d'égaliseur (*.txt)".into(),
            ],
        ),
        (
            "zenity",
            vec![
                "--file-selection".into(),
                "--title=Importer une courbe d'égaliseur".into(),
                "--file-filter=Courbes d'égaliseur | *.txt".into(),
            ],
        ),
    ];
    for (program, args) in attempts {
        match Command::new(program).args(&args).output() {
            Ok(output) => {
                // Code non nul : annulation.
                if !output.status.success() {
                    return None;
                }
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return (!text.is_empty()).then(|| PathBuf::from(text));
            }
            // Programme absent : essayer le suivant.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                tracing::warn!(%e, program, "boîte « Ouvrir »");
                return None;
            }
        }
    }
    tracing::warn!("ni kdialog ni zenity : boîte « Ouvrir » indisponible");
    None
}
