//! Contrôle non officiel des casques Sennheiser (Momentum 5 visé) sous Windows.
//!
//! Couches :
//! - [`protocol`] : trames GAIA V3, pur et testé sans matériel ;
//! - [`transport`] : flux d'octets (RFCOMM Windows, ou mock pour les tests) ;
//! - [`client`] : échange requête → réponse au-dessus d'un transport ;
//! - [`sdp`] : lecture des attributs SDP, pour le diagnostic.

pub mod alerts;
pub mod audio_check;
pub mod battery_log;
pub mod client;
pub mod device;
pub mod diagnostic;
pub mod hex;
pub mod hotkeys;
pub mod ipc;
pub mod paths;
#[cfg(windows)]
pub mod pipe;
pub mod presets;
pub mod profile;
pub mod protocol;
pub mod quick_panel;
pub mod scene_editor;
pub mod scenes;
pub mod sdp;
pub mod transport;
pub mod tray_menu;
pub mod ui;
pub mod viewmodel;
