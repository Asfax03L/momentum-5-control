//! Sous Windows, intègre le logo à l'exécutable (Explorateur, menu Démarrer,
//! barre des tâches, Alt+Tab).

use std::path::{Path, PathBuf};

include!("../../assets/logo/embed_logo.rs");

fn main() {
    if std::env::var("CARGO_CFG_WINDOWS").is_ok() {
        embed_logo();
    }
}
