//! OpenHeadset Control, client WinUI 3 pour Windows (`windows-reactor`).
//!
//! Client du service `momentum-daemon` (canal nommé, `momentum_client`) :
//! le service tient la connexion au casque, cette fenêtre affiche et
//! demande. Mêmes modèles de vue (`momentum_core::viewmodel`) que les
//! autres interfaces. Contrôles WinUI natifs : NavigationView, Mica,
//! ToggleSwitch, RadioButtons, InfoBar.
//!
//! Une valeur n'est montrée qu'une fois relue sur le casque. Un contrôle qui
//! signale la valeur déjà lue n'envoie rien (évite toute écriture quand
//! WinUI met un contrôle à jour) ; les curseurs n'envoient qu'après 350 ms
//! sans mouvement.

#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod app;
#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod panel;
#[cfg(windows)]
mod taskbar;

fn main() {
    #[cfg(windows)]
    app::run();
    #[cfg(not(windows))]
    eprintln!("momentum-winui ne fonctionne que sous Windows (WinUI 3)");
}
