//! Mode démonstration (`--demo`) : un faux worker, sans Bluetooth.
//!
//! Il remplace `momentum_service::worker::run` et parle le même langage
//! (commandes → événements), pour montrer l'interface sans casque. Rien
//! ici n'est une capacité confirmée du MOMENTUM 5 : les valeurs sont
//! choisies pour l'illustration, et la fenêtre affiche « Démonstration :
//! données simulées » en permanence. Le vrai worker n'est pas lancé et
//! aucun fichier de l'utilisateur n'est modifié par les données simulées.
//!
//! Chaque commande est « confirmée » après un court délai, comme le ferait
//! le casque : l'interface montre donc aussi son état d'envoi.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use momentum_core::device::{
    AudioMode, Connection, DeviceInfo, EqState, HeadsetState, NoiseModeRequest, NoiseState,
    PromptMode, SoundProfileStatus,
};
use momentum_service::worker::{WorkerCommand, WorkerEvent};

/// Délai simulé entre l'envoi et la confirmation du casque.
const CONFIRM_DELAY: Duration = Duration::from_millis(450);
/// Délai simulé de la recherche puis de la connexion.
const CONNECT_DELAY: Duration = Duration::from_millis(700);

/// Nom affiché du casque simulé.
pub const DEMO_NAME: &str = "MOMENTUM 5 (démo)";

fn initial_state() -> HeadsetState {
    HeadsetState {
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
    }
}

fn initial_eq() -> EqState {
    EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![35, 20, 10, -5, -15, 0, 10, 25],
        bass_boost: false,
    }
}

fn initial_info() -> DeviceInfo {
    DeviceInfo {
        name: Some(DEMO_NAME.to_string()),
        model: Some("Simulé".to_string()),
        firmware: Some("Simulé".to_string()),
        multipoint: Some(true),
        prompt_volume: Some(0),
        ..DeviceInfo::default()
    }
}

/// Faux worker : même signature que `worker::run`.
pub fn run(commands: Receiver<WorkerCommand>, notify: impl Fn(WorkerEvent)) {
    notify(WorkerEvent::Searching);
    std::thread::sleep(CONNECT_DELAY);
    notify(WorkerEvent::Connected {
        name: DEMO_NAME.to_string(),
        read_only: None,
    });
    let mut state = initial_state();
    let mut eq = initial_eq();
    let mut info = initial_info();
    let mut devices = vec![
        Connection::synthetic(1, "Ce PC", true),
        Connection::synthetic(2, "Téléphone", true),
        Connection::synthetic(3, "Tablette", false),
    ];
    notify(WorkerEvent::State(state));
    notify(WorkerEvent::Equalizer(eq.clone()));
    notify(WorkerEvent::Connections(devices.clone()));
    notify(WorkerEvent::Info(info.clone()));

    loop {
        let command = match commands.recv_timeout(Duration::from_secs(60)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        if let WorkerCommand::Query(reply) = command {
            let _ = reply.send(state);
            continue;
        }
        if let WorkerCommand::Quit = command {
            return;
        }
        std::thread::sleep(CONFIRM_DELAY);
        match command {
            WorkerCommand::SetMode(mode) => {
                state.noise.anc_enabled = !matches!(mode, NoiseModeRequest::Off);
                state.noise.adaptive = matches!(mode, NoiseModeRequest::Adaptive);
            }
            WorkerCommand::SetLevel(level) => {
                state.noise.level = level;
                state.noise.anc_enabled = true;
                state.noise.adaptive = false;
            }
            WorkerCommand::SetAntiWind(on) => state.noise.anti_wind = on,
            WorkerCommand::SetSpatial(on) => {
                state.spatial_audio = Some(on);
                // Comme le casque : audio spatial et profil s'excluent.
                if on && state.audio_mode == Some(AudioMode::Personalization) {
                    state.audio_mode = Some(AudioMode::Equalizer);
                }
                if !on {
                    state.head_tracking = Some(false);
                }
            }
            WorkerCommand::SetHeadTracking(on) => state.head_tracking = Some(on),
            WorkerCommand::SetMultipoint(on) => {
                info.multipoint = Some(on);
                notify(WorkerEvent::Info(info.clone()));
            }
            WorkerCommand::SetEqBand(band, gain) => {
                if let Some(g) = eq.gains.get_mut(usize::from(band)) {
                    *g = gain.clamp(eq.min_gain, eq.max_gain);
                }
                notify(WorkerEvent::Equalizer(eq.clone()));
            }
            WorkerCommand::SetEqFlat => {
                eq.gains.iter_mut().for_each(|g| *g = 0);
                notify(WorkerEvent::Equalizer(eq.clone()));
            }
            WorkerCommand::SetEqGains(gains) => {
                if gains.len() == eq.gains.len() {
                    eq.gains = gains;
                }
                notify(WorkerEvent::Equalizer(eq.clone()));
            }
            WorkerCommand::SetBassBoost(on) => {
                eq.bass_boost = on;
                notify(WorkerEvent::Equalizer(eq.clone()));
            }
            WorkerCommand::SetSpeechClarity(on) => {
                state.audio_mode = Some(if on {
                    AudioMode::SpeechClarity
                } else {
                    AudioMode::Equalizer
                });
            }
            WorkerCommand::SetSoundProfile(on) => {
                state.audio_mode = Some(if on {
                    AudioMode::Personalization
                } else {
                    AudioMode::Equalizer
                });
                if on {
                    state.spatial_audio = Some(false);
                }
            }
            WorkerCommand::SetTouchControls(on) => state.touch_controls = Some(on),
            WorkerCommand::SetBatteryProtection(on) => state.battery_protection = Some(on),
            WorkerCommand::SetSmartPause(on) => state.smart_pause = Some(on),
            WorkerCommand::SetAutoAnswer(on) => state.auto_answer = Some(on),
            WorkerCommand::SetComfortCall(on) => state.comfort_call = Some(on),
            WorkerCommand::SetAutoOff(seconds) => state.auto_off_seconds = Some(seconds),
            WorkerCommand::SetPromptMode(mode) => state.prompt_mode = Some(mode),
            WorkerCommand::SetPromptVolume(v) => {
                info.prompt_volume = Some(v);
                notify(WorkerEvent::Info(info.clone()));
            }
            WorkerCommand::SetName(name) => {
                info.name = Some(name);
                notify(WorkerEvent::Info(info.clone()));
            }
            WorkerCommand::ToggleConnection { device, connect } => {
                for d in devices.iter_mut().filter(|d| d.same_device(&device)) {
                    d.connected = connect;
                    d.state = if connect { 2 } else { 1 };
                }
                notify(WorkerEvent::Connections(devices.clone()));
            }
            WorkerCommand::ExportProfile(_) => {
                notify(WorkerEvent::Exported(Err(
                    "mode démonstration : aucun profil réel à exporter".to_string(),
                )));
            }
            WorkerCommand::ApplyScene { .. } => {
                notify(WorkerEvent::Refused(
                    "mode démonstration : les scènes ne sont pas simulées".to_string(),
                ));
            }
            WorkerCommand::Query(_) | WorkerCommand::Quit => {}
        }
        notify(WorkerEvent::State(state));
    }
}
