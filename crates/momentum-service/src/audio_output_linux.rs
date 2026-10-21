//! Sorties audio du système, sous Linux : pas encore lues (PipeWire, à
//! faire). Même interface que la version Windows (`audio_output.rs`).

use momentum_core::audio_check::WindowsAudio;

/// Toujours une erreur explicite : la page « Connexion et audio » l'affiche
/// à la place des sorties.
pub fn read(_device_name: Option<&str>) -> Result<WindowsAudio, String> {
    Err("lecture des sorties audio pas encore disponible sous Linux (PipeWire)".to_string())
}
