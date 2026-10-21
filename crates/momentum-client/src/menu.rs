//! Choix du menu de l'icône (`momentum_core::tray_menu`) traduit en
//! commande pour le service. Rien n'est envoyé pour un réglage non lu, ni
//! pour une valeur qui est déjà celle du casque.

use momentum_core::device::{AudioMode, HeadsetState, NoiseModeRequest};
use momentum_core::protocol::messages::M5NoiseMode;
use momentum_core::tray_menu::MenuAction;

use crate::Command;

/// Commande à envoyer pour `action` ; `None` pour les choix propres à
/// l'interface (fenêtre, scènes rechargées, démarrage, quitter) ou sans
/// effet sur le casque.
pub fn command_for(action: &MenuAction, state: &HeadsetState) -> Option<Command> {
    let mode = state.noise.mode();
    match action {
        MenuAction::Mode(request) => {
            let current = match mode {
                M5NoiseMode::Adaptive => NoiseModeRequest::Adaptive,
                M5NoiseMode::Custom { .. } => NoiseModeRequest::Custom,
                M5NoiseMode::Off => NoiseModeRequest::Off,
            };
            (current != *request).then_some(Command::SetMode(*request))
        }
        MenuAction::Level(level) => {
            (mode != M5NoiseMode::Custom { level: *level }).then_some(Command::SetLevel(*level))
        }
        MenuAction::AntiWind => Some(Command::SetAntiWind(!state.noise.anti_wind)),
        MenuAction::Spatial => state.spatial_audio.map(|v| Command::SetSpatial(!v)),
        MenuAction::SpeechClarity => state
            .audio_mode
            .map(|m| Command::SetSpeechClarity(m != AudioMode::SpeechClarity)),
        MenuAction::SoundProfile => state
            .audio_mode
            .map(|m| Command::SetSoundProfile(m != AudioMode::Personalization)),
        MenuAction::TouchControls => state.touch_controls.map(|v| Command::SetTouchControls(!v)),
        MenuAction::BatteryProtection => state
            .battery_protection
            .map(|v| Command::SetBatteryProtection(!v)),
        MenuAction::SmartPause => state.smart_pause.map(|v| Command::SetSmartPause(!v)),
        MenuAction::AutoAnswer => state.auto_answer.map(|v| Command::SetAutoAnswer(!v)),
        MenuAction::ComfortCall => state.comfort_call.map(|v| Command::SetComfortCall(!v)),
        MenuAction::AutoOff(seconds) => state
            .auto_off_seconds
            .filter(|v| v != seconds)
            .map(|_| Command::SetAutoOff(*seconds)),
        MenuAction::Prompts(prompts) => state
            .prompt_mode
            .filter(|v| v != prompts)
            .map(|_| Command::SetPromptMode(*prompts)),
        MenuAction::Scene(name) => Some(Command::ApplyScene(name.clone())),
        MenuAction::OpenWindow
        | MenuAction::OpenEqualizer
        | MenuAction::ReloadScenes
        | MenuAction::Autostart
        | MenuAction::Quit => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use momentum_core::device::{NoiseState, PromptMode};

    fn state() -> HeadsetState {
        HeadsetState {
            noise: NoiseState {
                anc_enabled: true,
                adaptive: false,
                anti_wind: false,
                level: 25,
            },
            smart_pause: Some(true),
            auto_off_seconds: Some(3600),
            prompt_mode: Some(PromptMode::TonesAndVoice),
            ..HeadsetState::default()
        }
    }

    #[test]
    fn current_values_send_nothing() {
        let s = state();
        assert_eq!(
            command_for(&MenuAction::Mode(NoiseModeRequest::Custom), &s),
            None
        );
        assert_eq!(command_for(&MenuAction::Level(25), &s), None);
        assert_eq!(command_for(&MenuAction::AutoOff(3600), &s), None);
        assert_eq!(
            command_for(&MenuAction::Prompts(PromptMode::TonesAndVoice), &s),
            None
        );
    }

    #[test]
    fn toggles_invert_the_read_value_and_skip_unread_ones() {
        let s = state();
        assert_eq!(
            command_for(&MenuAction::SmartPause, &s),
            Some(Command::SetSmartPause(false))
        );
        assert_eq!(
            command_for(&MenuAction::AntiWind, &s),
            Some(Command::SetAntiWind(true))
        );
        assert_eq!(command_for(&MenuAction::AutoAnswer, &s), None);
        assert_eq!(
            command_for(&MenuAction::Level(50), &s),
            Some(Command::SetLevel(50))
        );
        assert_eq!(command_for(&MenuAction::Quit, &s), None);
    }
}
