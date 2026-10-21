//! `momentum-daemon` : service sans fenêtre, seul propriétaire de la
//! connexion au casque. Les interfaces (Kirigami, WinUI, Slint, `ctl`) lui
//! parlent par l'API de `momentum-client`.
//!
//! Linux : bus de session D-Bus, nom `org.openheadset.Control`, interface
//! `org.openheadset.Control1` ([`linux`]). Windows : canal nommé de
//! l'utilisateur ([`win`]).
//!
//! Le service lance le worker de `momentum-service`, garde l'état courant
//! (pour les interfaces qui arrivent), tient l'historique de la batterie et
//! affiche les notifications (batterie faible, connexion, perte).

// Windows : pas de console pour un service lancé à l'ouverture de session.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(windows, target_os = "linux"))]
mod service;
#[cfg(windows)]
mod win;

fn main() {
    #[cfg(any(windows, target_os = "linux"))]
    {
        init_logging();
        #[cfg(target_os = "linux")]
        let result = linux::run();
        #[cfg(windows)]
        let result = win::run();
        if let Err(e) = result {
            tracing::error!("arrêt sur erreur : {e:#}");
            eprintln!("erreur : {e:#}");
            std::process::exit(1);
        }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    eprintln!("momentum-daemon : Windows et Linux seulement");
}

#[cfg(any(windows, target_os = "linux"))]
fn filter() -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "momentum_core=info,momentum_service=info,momentum_daemon=info".into())
}

/// Linux : sortie d'erreur (recueillie par le journal de systemd).
#[cfg(target_os = "linux")]
fn init_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_ansi(false)
        .init();
}

/// Windows : `momentum-daemon.log` dans le dossier de données, archivé
/// au-delà de 1 Mo (pas de console).
#[cfg(windows)]
fn init_logging() {
    use momentum_service::rotating_log::RotatingFile;

    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter())
        .with_ansi(false);
    let file = momentum_core::paths::data_dir().and_then(|dir| {
        std::fs::create_dir_all(&dir).ok()?;
        RotatingFile::open(
            dir.join("momentum-daemon.log"),
            dir.join("momentum-daemon.old.log"),
            1024 * 1024,
        )
        .ok()
    });
    match file {
        Some(file) => builder.with_writer(std::sync::Mutex::new(file)).init(),
        None => builder.init(),
    }
}
