//! Sorties audio du système, sous Linux : pas encore lues (PipeWire, à
//! faire). Même interface que la version Windows (`audio_output.rs`).

use momentum_core::audio_check::{LINUX_OUTPUTS_UNREAD, WindowsAudio};

/// Toujours une erreur explicite : la page « Connexion et audio » l'affiche
/// à la place des sorties.
pub fn read(_device_name: Option<&str>) -> Result<WindowsAudio, String> {
    Err(LINUX_OUTPUTS_UNREAD.to_string())
}
