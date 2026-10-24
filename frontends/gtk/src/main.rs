//! OpenHeadset Control, client GTK 4 / libadwaita pour GNOME.
//!
//! Client du service `momentum-daemon` (API D-Bus), comme le client KDE :
//! le service tient la connexion au casque, ce client ne fait qu'afficher
//! et demander. Mêmes modèles de vue (`momentum_core::viewmodel`) et même
//! suivi des demandes (`momentum_client::tracker`) que les autres
//! interfaces. Première étape (jalon 0.12) : page Accueil ; parité prévue
//! dans `docs/PARITE.md`.

#[cfg(target_os = "linux")]
mod app;

fn main() {
    #[cfg(target_os = "linux")]
    std::process::exit(app::run());
    #[cfg(not(target_os = "linux"))]
    eprintln!("momentum-gtk ne fonctionne que sous Linux (GTK 4 et libadwaita)");
}
