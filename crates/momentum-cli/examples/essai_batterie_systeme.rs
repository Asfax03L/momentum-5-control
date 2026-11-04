//! Batterie annoncée par le système pour le seul casque GAIA connecté.
//! Outil de développement, jamais livré : aucune trame GAIA. Sous Linux, la
//! recherche du casque (comme le service) puis BlueZ ; sous Windows, le casque
//! mémorisé (`casque.txt`) parmi les appareils appairés, puis les propriétés de
//! son nœud Bluetooth. N'affiche ni nom ni adresse.
//!
//! Usage : cargo run -p momentum-cli --example essai_batterie_systeme

fn main() {
    #[cfg(windows)]
    let percent = momentum_core::transport::windows_rfcomm::connected_headset_battery();
    #[cfg(target_os = "linux")]
    let percent = momentum_core::transport::bluez_rfcomm::connected_headset_battery();
    #[cfg(not(any(windows, target_os = "linux")))]
    let percent: Option<u8> = None;
    match percent {
        Some(p) => println!("Batterie annoncée par le système : {p} %"),
        None => println!("Aucune valeur : casque non trouvé ou non connecté, ou rien d'annoncé"),
    }
}
