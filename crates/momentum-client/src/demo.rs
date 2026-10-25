//! Mode démonstration (`--demo`) : un casque simulé à la place du service,
//! sans Bluetooth (repris de l'ancienne icône Slint).
//!
//! Il parle le langage de l'API (commandes → événements), pour montrer les
//! interfaces sans casque. Rien ici n'est une capacité confirmée du
//! MOMENTUM 5 : les valeurs sont choisies pour l'illustration, et les
//! interfaces affichent « Démonstration : données simulées » en
//! permanence. Le service n'est pas joint, et aucun fichier de
//! l'utilisateur n'est modifié par les données simulées.

use momentum_core::device::{
    AudioMode, DeviceInfo, EqState, HeadsetState, NoiseModeRequest, NoiseState, PromptMode,
    SoundProfileStatus,
};

use crate::{Command, ConnectionInfo, Event, Snapshot};
use momentum_core::i18n::tr;

/// Option de la ligne de commande.
pub const FLAG: &str = "--demo";
/// Nom affiché du casque simulé.
pub const NAME: &str = "MOMENTUM 5 (démo)";
/// Bandeau permanent des interfaces.
pub const BANNER: &str = "Démonstration : données simulées (aucun casque, aucun service)";

/// Lancé avec `--demo`.
pub fn requested() -> bool {
    std::env::args().any(|a| a == FLAG)
}

/// Casque simulé.
#[derive(Clone, Debug)]
pub struct Demo {
    state: HeadsetState,
    eq: EqState,
    info: DeviceInfo,
    connections: Vec<ConnectionInfo>,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    pub fn new() -> Self {
        let link = |name: &str, connected: bool, this_device: bool| ConnectionInfo {
            id: format!("demo-{name}"),
            name: name.to_string(),
            connected,
            this_device,
            state: if connected { 2 } else { 1 },
        };
        Self {
            state: HeadsetState {
                battery: 82,
                noise: NoiseState {
                    anc_enabled: true,
                    adaptive: true,
                    anti_wind: false,
                    level: 0,
                },
                spatial_audio: Some(false),
                head_tracking: Some(false),
                audio_mode: Some(AudioMode::Equalizer),
                sound_profile: Some(SoundProfileStatus::Received),
                battery_protection: Some(false),
                touch_controls: Some(true),
                smart_pause: Some(true),
                auto_answer: Some(false),
                comfort_call: Some(true),
                auto_off_seconds: Some(1800),
                prompt_mode: Some(PromptMode::TonesAndVoice),
                codec: None,
                sample_rate_hz: Some(48_000),
            },
            eq: EqState {
                min_gain: -60,
                max_gain: 60,
                gains: vec![35, 20, 10, -5, -15, 0, 10, 25],
                bass_boost: false,
            },
            info: DeviceInfo {
                name: Some(tr("MOMENTUM 5 (démo)").to_string()),
                model: Some(tr("Simulé").to_string()),
                firmware: Some(tr("Simulé").to_string()),
                multipoint: Some(true),
                prompt_volume: Some(0),
                ..DeviceInfo::default()
            },
            connections: vec![
                link(tr("Ce PC"), true, true),
                link(tr("Téléphone"), true, false),
                link(tr("Tablette"), false, false),
            ],
        }
    }

    /// État complet, comme le service le donne à une interface qui arrive.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            headset_id: Some(momentum_core::transport::choice::fingerprint(
                "02:00:00:00:00:01",
            )),
            device_name: Some(NAME.to_string()),
            read_only: None,
            session: 1,
            state: Some(self.state),
            equalizer: Some(self.eq.clone()),
            info: Some(self.info.clone()),
            connections: self.connections.clone(),
            problem: None,
        }
    }

    /// Applique une commande ; événements produits, comme le service.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        let s = &mut self.state;
        let mut events = Vec::new();
        match command {
            Command::SetMode(mode) => {
                s.noise.anc_enabled = !matches!(mode, NoiseModeRequest::Off);
                s.noise.adaptive = matches!(mode, NoiseModeRequest::Adaptive);
            }
            Command::SetLevel(level) => {
                s.noise.level = level.min(100);
                s.noise.anc_enabled = true;
                s.noise.adaptive = false;
            }
            Command::SetAntiWind(on) => s.noise.anti_wind = on,
            Command::SetSpatial(on) => {
                s.spatial_audio = Some(on);
                // Comme le casque : audio spatial et profil s'excluent.
                if on && s.audio_mode == Some(AudioMode::Personalization) {
                    s.audio_mode = Some(AudioMode::Equalizer);
                }
                if !on {
                    s.head_tracking = Some(false);
                }
            }
            Command::SetHeadTracking(on) => s.head_tracking = Some(on),
            Command::SetMultipoint(on) => {
                self.info.multipoint = Some(on);
                events.push(Event::Info(self.info.clone()));
            }
            Command::SetEqBand(band, gain) => {
                let (min, max) = (self.eq.min_gain, self.eq.max_gain);
                if let Some(g) = self.eq.gains.get_mut(usize::from(band)) {
                    *g = gain.clamp(min, max);
                }
                events.push(Event::Equalizer(self.eq.clone()));
            }
            Command::SetEqFlat => {
                self.eq.gains.iter_mut().for_each(|g| *g = 0);
                events.push(Event::Equalizer(self.eq.clone()));
            }
            Command::SetEqGains(gains) => {
                if gains.len() == self.eq.gains.len() {
                    self.eq.gains = gains;
                }
                events.push(Event::Equalizer(self.eq.clone()));
            }
            Command::SetBassBoost(on) => {
                self.eq.bass_boost = on;
                events.push(Event::Equalizer(self.eq.clone()));
            }
            Command::SetSpeechClarity(on) => {
                s.audio_mode = Some(if on {
                    AudioMode::SpeechClarity
                } else {
                    AudioMode::Equalizer
                });
            }
            Command::SetSoundProfile(on) => {
                s.audio_mode = Some(if on {
                    AudioMode::Personalization
                } else {
                    AudioMode::Equalizer
                });
                if on {
                    s.spatial_audio = Some(false);
                }
            }
            Command::SetTouchControls(on) => s.touch_controls = Some(on),
            Command::SetBatteryProtection(on) => s.battery_protection = Some(on),
            Command::SetSmartPause(on) => s.smart_pause = Some(on),
            Command::SetAutoAnswer(on) => s.auto_answer = Some(on),
            Command::SetComfortCall(on) => s.comfort_call = Some(on),
            Command::SetAutoOff(seconds) => s.auto_off_seconds = Some(seconds),
            Command::SetPromptMode(mode) => s.prompt_mode = Some(mode),
            Command::SetPromptVolume(v) => {
                self.info.prompt_volume = Some(v);
                events.push(Event::Info(self.info.clone()));
            }
            Command::SetName(name) => {
                self.info.name = Some(name);
                events.push(Event::Info(self.info.clone()));
            }
            Command::ToggleConnection { index, connect, .. } => {
                if let Some(c) = self.connections.get_mut(index).filter(|c| !c.this_device) {
                    c.connected = connect;
                    c.state = if connect { 2 } else { 1 };
                }
                events.push(Event::Connections(self.connections.clone()));
            }
            Command::ExportProfile(_) => events.push(Event::Exported(Err(tr(
                "mode démonstration : aucun profil réel à exporter",
            )
            .to_string()))),
            Command::ApplyScene(_) => events.push(Event::Refused(
                tr("mode démonstration : les scènes ne sont pas simulées").to_string(),
            )),
            Command::ClearBatteryHistory | Command::ClearJournal | Command::ReconnectHeadset => {}
        }
        events.push(Event::State(self.state));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_change_the_simulated_headset() {
        let mut demo = Demo::new();
        let mut snapshot = demo.snapshot();
        assert_eq!(snapshot.device_name.as_deref(), Some(NAME));
        for event in demo.apply(Command::SetMode(NoiseModeRequest::Off)) {
            snapshot.apply(&event);
        }
        assert!(!snapshot.state.unwrap().noise.anc_enabled);
        for event in demo.apply(Command::SetEqFlat) {
            snapshot.apply(&event);
        }
        assert!(snapshot.equalizer.unwrap().gains.iter().all(|&g| g == 0));
        // Ce PC ne se déconnecte pas lui-même.
        demo.apply(Command::ToggleConnection {
            index: 0,
            name: "Ce PC".into(),
            connect: false,
            device: None,
        });
        assert!(demo.snapshot().connections[0].connected);
        assert!(matches!(
            demo.apply(Command::ApplyScene("Travail".into()))[0],
            Event::Refused(_)
        ));
    }
}
