//! OpenHeadset Control, client GTK 4 / libadwaita pour GNOME.
//!
//! Client du service `momentum-daemon` (API D-Bus), comme le client KDE :
//! le service tient la connexion au casque, ce client ne fait qu'afficher
//! et demander. Mêmes modèles de vue (`momentum_core::viewmodel`) et même
//! suivi des demandes (`momentum_client::tracker`) que les autres
//! interfaces. Parité avec le client KDE : `docs/PARITE.md`.

#[cfg(target_os = "linux")]
mod app;
#[cfg(target_os = "linux")]
mod pages;
#[cfg(target_os = "linux")]
mod panel;
#[cfg(target_os = "linux")]
#[path = "../../kirigami/src/pipewire.rs"]
mod pipewire;
#[cfg(target_os = "linux")]
mod scene_panel;
#[cfg(target_os = "linux")]
mod shortcuts;

fn main() {
    #[cfg(target_os = "linux")]
    std::process::exit(app::run());
    #[cfg(not(target_os = "linux"))]
    eprintln!("momentum-gtk ne fonctionne que sous Linux (GTK 4 et libadwaita)");
}
