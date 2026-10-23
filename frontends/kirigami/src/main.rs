//! OpenHeadset Control, client Qt 6 / Kirigami pour KDE Plasma.
//!
//! Client du service `momentum-daemon` (API D-Bus) : le service tient la
//! connexion au casque, ce client ne fait qu'afficher et demander. Mêmes
//! modèles de vue (`momentum_core::viewmodel`) que l'interface WinUI.

#[cfg(target_os = "linux")]
mod autostart;
#[cfg(target_os = "linux")]
mod bridge;
#[cfg(target_os = "linux")]
mod instance;
#[cfg(target_os = "linux")]
mod pipewire;
#[cfg(target_os = "linux")]
mod shortcuts;
#[cfg(target_os = "linux")]
mod tray;

fn main() {
    #[cfg(target_os = "linux")]
    app::run();
    #[cfg(not(target_os = "linux"))]
    eprintln!("momentum-kirigami ne fonctionne que sous Linux (Qt 6 et Kirigami)");
}

#[cfg(target_os = "linux")]
mod app {
    use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

    pub fn run() {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "momentum_core=info,momentum_client=info,momentum_kirigami=info".into()
        });
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_ansi(false)
            .init();
        tracing::info!("démarrage du client Kirigami");

        // Une seule fenêtre : une deuxième instance demande à la première de
        // se montrer, puis s'arrête.
        let _instance = match crate::instance::acquire() {
            Ok(crate::instance::Instance::First(connection)) => Some(connection),
            Ok(crate::instance::Instance::AlreadyRunning) => {
                tracing::info!("déjà lancé : fenêtre de l'instance existante montrée");
                return;
            }
            Err(e) => {
                tracing::warn!(%e, "instance unique indisponible");
                None
            }
        };

        let mut app = QGuiApplication::new();
        // Après la création de l'application, avant la première fenêtre.
        crate::bridge::qobject::openheadset_set_identity();
        // Thème choisi dans `apparence.txt` (sinon celui du système).
        crate::bridge::qobject::openheadset_set_color_scheme(
            match momentum_core::appearance::load() {
                Some(true) => 2,
                Some(false) => 1,
                None => 0,
            },
        );
        let mut engine = QQmlApplicationEngine::new();
        if let Some(engine) = engine.as_mut() {
            engine.load(&QUrl::from(
                "qrc:/qt/qml/org/openheadset/control/qml/Main.qml",
            ));
        }
        if let Some(app) = app.as_mut() {
            app.exec();
        }
    }
}
