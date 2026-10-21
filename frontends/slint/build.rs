//! Compile la fenêtre principale Slint avec le thème Fluent (Windows 11) et,
//! sous Windows, intègre le logo à l'exécutable (Explorateur, menu Démarrer,
//! barre des tâches, Alt+Tab).

use std::path::{Path, PathBuf};

include!("../../assets/logo/embed_logo.rs");

fn main() {
    // La fenêtre existe sous Windows et Linux.
    let windows = std::env::var("CARGO_CFG_WINDOWS").is_ok();
    let linux = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "linux");
    if !windows && !linux {
        return;
    }
    let config = slint_build::CompilerConfiguration::new().with_style("fluent".into());
    if let Err(e) = slint_build::compile_with_config("ui/app.slint", config) {
        panic!("compilation de ui/app.slint impossible : {e}");
    }
    if windows {
        embed_logo();
    }
}
