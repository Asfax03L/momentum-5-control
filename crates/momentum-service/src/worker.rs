//! Thread « casque » : connexion, abonnement aux notifications, application
//! des commandes du menu et reconnexion automatique.
//!
//! Il possède seul la connexion RFCOMM (WinRT sous Windows, BlueZ sous
//! Linux). L'interface lui parle par un canal `mpsc` (commandes) et il
//! répond par une fonction `notify` (événements).

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

use momentum_core::device::{
    Connection, DeviceError, DeviceInfo, EqState, Headset, HeadsetState, NoiseModeRequest,
    ProfileSelection, PromptMode,
};
use momentum_core::presets::Preset;
use momentum_core::protocol::messages::M5NoiseMode;
use momentum_core::scenes::{Scene, SceneReport, apply_scene};
use momentum_core::transport::Transport;
#[cfg(target_os = "linux")]
use momentum_core::transport::bluez_rfcomm::{
    BluezTransport as PlatformTransport, find_gaia_device,
};
#[cfg(windows)]
use momentum_core::transport::windows_rfcomm::{
    RfcommTransport as PlatformTransport, find_gaia_service,
};
use tracing::{debug, info, warn};
#[cfg(windows)]
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};

/// Délai d'attente de chaque réponse du casque.
const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// Attente maximale d'une notification avant de revérifier les commandes.
const POLL_INTERVAL: Duration = Duration::from_millis(200);
/// Pause entre deux tentatives de connexion.
const RETRY_DELAY: Duration = Duration::from_secs(5);
/// Relecture complète périodique (sert aussi à détecter un lien mort).
const FULL_REFRESH: Duration = Duration::from_secs(60);
/// Délai avant de relire les connexions après un changement (Multipoint,
/// connexion ou déconnexion d'un appareil).
const LINKS_RECHECK_DELAY: Duration = Duration::from_secs(3);

const NAME_FILTERS: [&str; 2] = ["momentum", "sennheiser"];

#[derive(Debug)]
pub enum WorkerCommand {
    SetMode(NoiseModeRequest),
    SetLevel(u8),
    SetAntiWind(bool),
    SetSpatial(bool),
    /// Suivi de tête (refusé par la bibliothèque sans audio spatial).
    SetHeadTracking(bool),
    /// Multipoint (le couper ne garde qu'un appareil connecté).
    SetMultipoint(bool),
    /// Gain d'une bande, en dixièmes de dB.
    SetEqBand(u8, i8),
    /// Toutes les bandes à 0 dB.
    SetEqFlat,
    /// Courbe complète (préréglage), en dixièmes de dB.
    SetEqGains(Vec<i8>),
    /// Sauvegarde brute du profil de personnalisation dans ce fichier.
    ExportProfile(std::path::PathBuf),
    SetBassBoost(bool),
    SetSpeechClarity(bool),
    /// Profil personnalisé sélectionné (sinon retour à l'égaliseur).
    SetSoundProfile(bool),
    SetTouchControls(bool),
    SetBatteryProtection(bool),
    SetSmartPause(bool),
    SetAutoAnswer(bool),
    SetComfortCall(bool),
    SetAutoOff(u16),
    SetPromptMode(PromptMode),
    /// Volume des invites, valeur du casque (−20 à +6).
    SetPromptVolume(i8),
    /// Nom du casque (vérifié par `check_name` avant l'envoi).
    SetName(String),
    /// Connecte (`connect`) ou déconnecte cet appareil, retrouvé par son
    /// adresse dans la liste courante (sinon refus). Rien n'est envoyé s'il
    /// est déjà dans l'état demandé.
    ToggleConnection {
        device: Connection,
        connect: bool,
    },
    /// Applique une scène (préréglages déjà lus) ; le rapport part aussi
    /// vers `reply` quand la demande vient du canal local.
    ApplyScene {
        scene: Scene,
        presets: Vec<Preset>,
        reply: Option<Sender<SceneReport>>,
    },
    /// État courant pour le canal local, sans trafic vers le casque. Hors
    /// session, la demande est abandonnée : le canal lit « non connecté ».
    Query(Sender<HeadsetState>),
    Quit,
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Searching,
    /// `read_only` : modèle lu, s'il n'est pas validé pour les écritures.
    Connected {
        name: String,
        read_only: Option<String>,
    },
    State(HeadsetState),
    Equalizer(EqState),
    /// Appareils appairés au casque et leur état de connexion.
    Connections(Vec<Connection>),
    /// Identité et réglages en lecture seule.
    Info(DeviceInfo),
    /// Résultat d'un export du profil : fichier écrit, ou erreur.
    Exported(Result<std::path::PathBuf, String>),
    /// Connexion perdue (la recherche reprend aussitôt).
    Lost(String),
    /// Réglage refusé par le casque ; la connexion reste ouverte.
    Refused(String),
    /// Scène appliquée, en tout ou en partie.
    SceneApplied(SceneReport),
}

#[derive(Debug)]
enum SessionEnd {
    Quit,
    Lost(String),
}

pub fn run(commands: Receiver<WorkerCommand>, notify: impl Fn(WorkerEvent)) {
    // SAFETY: première chose faite par ce thread, avant tout objet WinRT.
    #[cfg(windows)]
    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        warn!(%e, "RoInitialize");
    }
    loop {
        notify(WorkerEvent::Searching);
        match connect() {
            Ok((name, mut headset)) => {
                // Écritures réservées au modèle validé ; la lecture reste
                // possible sur tout casque GAIA.
                let model = headset.identify();
                let read_only = (!headset.writes_allowed()).then_some(model);
                info!(%name, ?read_only, "casque connecté");
                notify(WorkerEvent::Connected { name, read_only });
                match session(headset, &commands, &notify) {
                    SessionEnd::Quit => return,
                    SessionEnd::Lost(reason) => {
                        warn!(%reason, "connexion perdue");
                        notify(WorkerEvent::Lost(reason));
                    }
                }
            }
            Err(reason) => debug!(%reason, "casque introuvable"),
        }
        // Attente avant nouvel essai, en restant réactif à « Quitter ».
        match commands.recv_timeout(RETRY_DELAY) {
            Ok(WorkerCommand::Quit) | Err(RecvTimeoutError::Disconnected) => return,
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(windows)]
fn connect() -> Result<(String, Headset<PlatformTransport>), String> {
    let found = find_gaia_service(&NAME_FILTERS)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "aucun casque avec service GAIA".to_string())?;
    let transport = PlatformTransport::connect(&found.service).map_err(|e| e.to_string())?;
    Ok((found.device_name, Headset::new(transport, REPLY_TIMEOUT)))
}

/// Casque appairé et connecté dans BlueZ ; le profil client BlueZ choisit
/// le canal GAIA (comme `ctl` sans `--canal`, validé sur le M5).
#[cfg(target_os = "linux")]
fn connect() -> Result<(String, Headset<PlatformTransport>), String> {
    let found = find_gaia_device(&NAME_FILTERS)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "aucun casque connecté avec service GAIA".to_string())?;
    let transport = PlatformTransport::connect(&found, None).map_err(|e| e.to_string())?;
    Ok((found.device_name, Headset::new(transport, REPLY_TIMEOUT)))
}

fn session<T: Transport>(
    mut headset: Headset<T>,
    commands: &Receiver<WorkerCommand>,
    notify: &impl Fn(WorkerEvent),
) -> SessionEnd {
    let lost = |e: DeviceError| SessionEnd::Lost(e.to_string());

    if let Err(e) = headset.register_notifications() {
        return lost(e);
    }
    let mut state = match headset.read_state() {
        Ok(s) => s,
        Err(e) => return lost(e),
    };
    notify(WorkerEvent::State(state));
    let mut eq = match headset.read_eq() {
        Ok(e) => e,
        Err(e) => return lost(e),
    };
    notify(WorkerEvent::Equalizer(eq.clone()));
    let mut paired = match read_paired(&mut headset) {
        Ok(p) => p,
        Err(e) => return lost(e),
    };
    notify(WorkerEvent::Connections(paired.clone()));
    let mut info = match headset.read_device_info() {
        Ok(i) => i,
        Err(e) => return lost(e),
    };
    notify(WorkerEvent::Info(info.clone()));
    let mut last_refresh = Instant::now();
    // Relecture différée de la liste des connexions (le casque ne notifie
    // pas toujours la déconnexion d'un autre appareil).
    let mut links_recheck: Option<Instant> = None;

    loop {
        // Toutes les commandes en attente d'un coup : si l'utilisateur en a
        // empilé plusieurs (curseur au clavier, clics rapides), seule la
        // dernière valeur de chaque réglage part vers le casque.
        let mut pending = Vec::new();
        loop {
            match commands.try_recv() {
                Ok(WorkerCommand::Quit) | Err(TryRecvError::Disconnected) => {
                    return SessionEnd::Quit;
                }
                Ok(WorkerCommand::Query(reply)) => {
                    let _ = reply.send(state);
                }
                Ok(command) => pending.push(command),
                Err(TryRecvError::Empty) => break,
            }
        }
        if !pending.is_empty() {
            let batch = coalesce(pending);
            let touches_eq = batch
                .iter()
                .any(|c| c.is_eq() || matches!(c, WorkerCommand::ApplyScene { .. }));
            let touches_state = batch.iter().any(|c| !c.is_eq());
            if batch.iter().any(|c| {
                matches!(
                    c,
                    WorkerCommand::SetMultipoint(_) | WorkerCommand::ToggleConnection { .. }
                )
            }) {
                links_recheck = Some(Instant::now() + LINKS_RECHECK_DELAY);
            }
            let touches_info = batch.iter().any(|c| {
                matches!(
                    c,
                    WorkerCommand::SetPromptVolume(_)
                        | WorkerCommand::SetName(_)
                        | WorkerCommand::SetMultipoint(_)
                )
            });
            if let Err(end) = run_batch(&mut headset, batch, &state, &mut eq, &paired, notify) {
                return end;
            }
            // Relecture : l'interface reflète toujours l'état réel du casque.
            if touches_eq {
                match headset.read_eq() {
                    Ok(e) => {
                        debug!(?e, "égaliseur relu après commande");
                        eq = e;
                        notify(WorkerEvent::Equalizer(eq.clone()));
                    }
                    Err(e) => return lost(e),
                }
            }
            if touches_info {
                match headset.read_device_info() {
                    Ok(i) => {
                        info = i;
                        notify(WorkerEvent::Info(info.clone()));
                    }
                    Err(e) => return lost(e),
                }
            }
            if touches_state {
                match headset.read_state() {
                    Ok(s) => {
                        debug!(?s, "état relu après commande");
                        state = keep_stream_info(&state, s);
                        notify(WorkerEvent::State(state));
                        last_refresh = Instant::now();
                    }
                    Err(e) => return lost(e),
                }
            }
            continue;
        }

        match headset.poll_notifications(POLL_INTERVAL) {
            Ok(frames) => {
                let (mut changed, mut eq_changed, mut info_changed) = (false, false, false);
                // Feature 10 (1491, 1484…) : une connexion a changé.
                let links_changed = frames.iter().any(|f| f.command.feature() == 10);
                for frame in &frames {
                    changed |= state.apply_notification(frame);
                    eq_changed |= eq.apply_notification(frame);
                    info_changed |= info.apply_notification(frame);
                }
                if info_changed {
                    notify(WorkerEvent::Info(info.clone()));
                }
                if links_changed {
                    match read_paired(&mut headset) {
                        Ok(p) if p != paired => {
                            paired = p;
                            notify(WorkerEvent::Connections(paired.clone()));
                        }
                        Ok(_) => {}
                        Err(e) => return lost(e),
                    }
                }
                if changed {
                    debug!(?state, "état mis à jour par notification");
                    notify(WorkerEvent::State(state));
                }
                if eq_changed {
                    debug!(?eq, "égaliseur mis à jour par notification");
                    notify(WorkerEvent::Equalizer(eq.clone()));
                }
            }
            Err(e) => return lost(e),
        }

        if links_recheck.is_some_and(|at| Instant::now() >= at) {
            links_recheck = None;
            match read_paired(&mut headset) {
                Ok(p) if p != paired => {
                    paired = p;
                    notify(WorkerEvent::Connections(paired.clone()));
                }
                Ok(_) => {}
                Err(e) => return lost(e),
            }
        }

        if last_refresh.elapsed() >= FULL_REFRESH {
            match headset.read_state() {
                Ok(s) => {
                    let s = keep_stream_info(&state, s);
                    if s != state {
                        state = s;
                        notify(WorkerEvent::State(state));
                    }
                    last_refresh = Instant::now();
                }
                Err(e) => return lost(e),
            }
            match read_paired(&mut headset) {
                Ok(p) if p != paired => {
                    paired = p;
                    notify(WorkerEvent::Connections(paired.clone()));
                }
                Ok(_) => {}
                Err(e) => return lost(e),
            }
            match headset.read_device_info() {
                Ok(i) if i != info => {
                    info = i;
                    notify(WorkerEvent::Info(info.clone()));
                }
                Ok(_) => {}
                Err(e) => return lost(e),
            }
            match headset.read_eq() {
                Ok(e) if e != eq => {
                    eq = e;
                    notify(WorkerEvent::Equalizer(eq.clone()));
                }
                Ok(_) => {}
                Err(e) => return lost(e),
            }
        }
    }
}

/// Exécute un lot de commandes dans l'ordre. `eq` reste fidèle au casque
/// entre deux commandes : les écritures d'égaliseur le tiennent à jour, et
/// il est relu avant la commande d'égaliseur qui suit une scène (la scène
/// écrit avec sa propre lecture). Une erreur qui désynchronise la session
/// l'arrête sans rien renvoyer.
fn run_batch<T: Transport>(
    headset: &mut Headset<T>,
    batch: Vec<WorkerCommand>,
    state: &HeadsetState,
    eq: &mut EqState,
    paired: &[Connection],
    notify: &impl Fn(WorkerEvent),
) -> Result<(), SessionEnd> {
    let lost = |e: DeviceError| SessionEnd::Lost(e.to_string());
    let mut eq_stale = false;
    for command in batch {
        if let WorkerCommand::ApplyScene {
            scene,
            presets,
            reply,
        } = command
        {
            let report = apply_scene(headset, &scene, &presets);
            info!(scene = %report.scene, complete = report.is_complete(), "scène appliquée");
            if let Some(reply) = reply {
                let _ = reply.send(report.clone());
            }
            notify(WorkerEvent::SceneApplied(report));
            if headset.client().is_desynchronized() {
                return Err(SessionEnd::Lost(
                    "échange interrompu pendant une scène".to_string(),
                ));
            }
            eq_stale = true;
            continue;
        }
        if let WorkerCommand::ToggleConnection { device, connect } = command {
            let Some(current) = paired.iter().find(|d| d.same_device(&device)) else {
                notify(WorkerEvent::Refused(
                    "la liste des appareils a changé, réessaie".to_string(),
                ));
                continue;
            };
            if current.connected == connect {
                continue;
            }
            if let Err(e) = headset.set_connection(current, connect) {
                if headset.client().is_desynchronized() {
                    return Err(lost(e));
                }
                notify(WorkerEvent::Refused(e.to_string()));
            }
            // L'état suit par les notifications 1491/1484.
            continue;
        }
        if let WorkerCommand::ExportProfile(path) = command {
            let result = export_profile(headset, &path);
            if headset.client().is_desynchronized() {
                return Err(SessionEnd::Lost(
                    "délai dépassé pendant l'export".to_string(),
                ));
            }
            notify(WorkerEvent::Exported(result.map(|()| path)));
            continue;
        }
        if command.is_eq() && eq_stale {
            *eq = headset.read_eq().map_err(lost)?;
            eq_stale = false;
        }
        if let Err(e) = apply(headset, state, eq, command) {
            if headset.client().is_desynchronized() {
                // Pas de nouvel essai : l'écriture a pu être appliquée.
                return Err(lost(e));
            }
            notify(WorkerEvent::Refused(e.to_string()));
        }
    }
    Ok(())
}

/// Lit le profil (lecture seule) et l'écrit dans un fichier neuf.
fn export_profile<T: Transport>(
    headset: &mut Headset<T>,
    path: &std::path::Path,
) -> Result<(), String> {
    use std::io::Write;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let export = headset.read_sound_profile(now).map_err(|e| e.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(export.to_json().as_bytes()))
        .map_err(|e| format!("{} : {e}", path.display()))
}

/// La fréquence d'échantillonnage n'est connue que par notification : une
/// relecture complète la conserve.
fn keep_stream_info(old: &HeadsetState, mut new: HeadsetState) -> HeadsetState {
    new.sample_rate_hz = new.sample_rate_hz.or(old.sample_rate_hz);
    new
}

/// Appareils appairés ; une lecture refusée par le casque donne une liste
/// vide (seule une erreur de transport coupe la session).
fn read_paired<T: Transport>(headset: &mut Headset<T>) -> Result<Vec<Connection>, DeviceError> {
    match headset.read_connections() {
        Ok(devices) => Ok(devices),
        Err(e @ DeviceError::Client(momentum_core::client::ClientError::Transport(_))) => Err(e),
        Err(e) if headset.client().is_desynchronized() => Err(e),
        Err(e) => {
            debug!(%e, "appareils appairés illisibles");
            Ok(Vec::new())
        }
    }
}

impl WorkerCommand {
    fn is_eq(&self) -> bool {
        matches!(
            self,
            WorkerCommand::SetEqBand(..)
                | WorkerCommand::SetEqFlat
                | WorkerCommand::SetEqGains(_)
                | WorkerCommand::SetBassBoost(_)
        )
    }

    /// Réglage à valeur absolue : la dernière valeur demandée suffit. Les
    /// actions (connexion, export, scène) et les modes de contrôle du bruit
    /// (séquences de l'app) ne sont jamais remplacés.
    fn replaceable(&self) -> bool {
        !matches!(
            self,
            WorkerCommand::SetMode(_)
                | WorkerCommand::ApplyScene { .. }
                | WorkerCommand::ToggleConnection { .. }
                | WorkerCommand::ExportProfile(_)
                | WorkerCommand::Query(_)
                | WorkerCommand::Quit
        )
    }

    /// Réglage sans lien avec les autres : une commande peut en remplacer
    /// une précédente du même réglage par-dessus celui-ci sans changer le
    /// résultat. Ne le sont pas : actions, modes de bruit (le niveau en
    /// dépend), audio spatial et profil (le suivi de tête et la clarté en
    /// dépendent), suivi de tête, clarté, Multipoint et courbes complètes.
    fn independent(&self) -> bool {
        matches!(
            self,
            WorkerCommand::SetEqBand(..)
                | WorkerCommand::SetLevel(_)
                | WorkerCommand::SetAntiWind(_)
                | WorkerCommand::SetBassBoost(_)
                | WorkerCommand::SetTouchControls(_)
                | WorkerCommand::SetBatteryProtection(_)
                | WorkerCommand::SetSmartPause(_)
                | WorkerCommand::SetAutoAnswer(_)
                | WorkerCommand::SetComfortCall(_)
                | WorkerCommand::SetAutoOff(_)
                | WorkerCommand::SetPromptMode(_)
                | WorkerCommand::SetPromptVolume(_)
                | WorkerCommand::SetName(_)
        )
    }

    /// Réglage visé (avec la bande pour l'égaliseur).
    fn key(&self) -> (std::mem::Discriminant<WorkerCommand>, Option<u8>) {
        let band = match self {
            WorkerCommand::SetEqBand(band, _) => Some(*band),
            _ => None,
        };
        (std::mem::discriminant(self), band)
    }
}

/// Allège un lot sans changer son résultat : une valeur absolue remplace la
/// précédente du même réglage si seuls des réglages indépendants les
/// séparent (curseur au clavier, clics rapides). « Remettre à plat » et une
/// courbe complète annulent les gains de bande demandés avant eux. Toutes les
/// actions restent, dans l'ordre.
fn coalesce(commands: Vec<WorkerCommand>) -> Vec<WorkerCommand> {
    let mut kept: Vec<WorkerCommand> = Vec::with_capacity(commands.len());
    for command in commands {
        if matches!(
            command,
            WorkerCommand::SetEqFlat | WorkerCommand::SetEqGains(_)
        ) {
            kept.retain(|c| !matches!(c, WorkerCommand::SetEqBand(..)));
        }
        if command.replaceable() {
            let key = command.key();
            for i in (0..kept.len()).rev() {
                if kept[i].key() == key {
                    kept.remove(i);
                    break;
                }
                if !kept[i].independent() {
                    break;
                }
            }
        }
        kept.push(command);
    }
    kept
}

fn apply<T: Transport>(
    headset: &mut Headset<T>,
    state: &HeadsetState,
    eq: &mut EqState,
    command: WorkerCommand,
) -> Result<(), DeviceError> {
    match command {
        WorkerCommand::SetMode(mode) => headset.set_mode(mode),
        WorkerCommand::SetLevel(level) => {
            // Le curseur n'a d'effet qu'en mode Personnalisé (comme dans l'app).
            if !matches!(state.noise.mode(), M5NoiseMode::Custom { .. }) {
                headset.set_mode(NoiseModeRequest::Custom)?;
            }
            headset.set_level(level)
        }
        WorkerCommand::SetAntiWind(on) => headset.set_anti_wind(on),
        WorkerCommand::SetSpatial(on) => headset.set_spatial_audio(on),
        WorkerCommand::SetHeadTracking(on) => headset.set_head_tracking(on),
        WorkerCommand::SetMultipoint(on) => headset.set_multipoint(on),
        WorkerCommand::SetEqBand(band, gain) => headset.set_eq_band(eq, band, gain),
        WorkerCommand::SetEqFlat => {
            let flat = vec![0; eq.gains.len()];
            headset.set_eq_gains(eq, &flat)
        }
        WorkerCommand::SetEqGains(gains) => headset.set_eq_gains(eq, &gains),
        // Traitées avant `apply` (besoin du fichier ou de la liste des appareils).
        WorkerCommand::ExportProfile(_)
        | WorkerCommand::ToggleConnection { .. }
        | WorkerCommand::ApplyScene { .. }
        | WorkerCommand::Query(_) => Ok(()),
        WorkerCommand::SetBassBoost(on) => headset.set_bass_boost(on),
        WorkerCommand::SetSpeechClarity(on) => headset.set_speech_clarity(on),
        WorkerCommand::SetSoundProfile(true) => headset.select_sound_profile().map(|s| {
            if s == ProfileSelection::AlreadySelectedAtRead {
                info!("profil déjà sélectionné à la lecture : aucune écriture");
            }
        }),
        WorkerCommand::SetSoundProfile(false) => headset.set_speech_clarity(false),
        WorkerCommand::SetTouchControls(on) => headset.set_touch_controls(on),
        WorkerCommand::SetBatteryProtection(on) => headset.set_battery_protection(on),
        WorkerCommand::SetSmartPause(on) => headset.set_smart_pause(on),
        WorkerCommand::SetAutoAnswer(on) => headset.set_auto_answer(on),
        WorkerCommand::SetComfortCall(on) => headset.set_comfort_call(on),
        WorkerCommand::SetAutoOff(seconds) => headset.set_auto_off(seconds),
        WorkerCommand::SetPromptMode(mode) => headset.set_prompt_mode(mode),
        WorkerCommand::SetPromptVolume(value) => headset.set_prompt_volume(value),
        WorkerCommand::SetName(name) => headset.set_name(&name),
        WorkerCommand::Quit => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands(commands: &[WorkerCommand]) -> Vec<(u8, i8)> {
        commands
            .iter()
            .filter_map(|c| match c {
                WorkerCommand::SetEqBand(b, g) => Some((*b, *g)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn only_the_last_value_of_each_setting_is_sent() {
        let out = coalesce(vec![
            WorkerCommand::SetEqBand(0, 5),
            WorkerCommand::SetEqBand(1, 10),
            WorkerCommand::SetEqBand(0, 10),
            WorkerCommand::SetLevel(25),
            WorkerCommand::SetEqBand(0, 15),
            WorkerCommand::SetLevel(50),
        ]);
        assert_eq!(bands(&out), [(1, 10), (0, 15)]);
        assert!(matches!(out.last(), Some(WorkerCommand::SetLevel(50))));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn flat_cancels_earlier_bands_but_not_later_ones() {
        let out = coalesce(vec![
            WorkerCommand::SetEqBand(2, 30),
            WorkerCommand::SetEqFlat,
            WorkerCommand::SetEqBand(3, -20),
        ]);
        assert!(matches!(out[0], WorkerCommand::SetEqFlat));
        assert_eq!(bands(&out), [(3, -20)]);
    }

    #[test]
    fn a_preset_replaces_pending_band_moves() {
        let out = coalesce(vec![
            WorkerCommand::SetEqBand(1, 10),
            WorkerCommand::SetEqGains(vec![0; 8]),
            WorkerCommand::SetEqBand(2, 20),
        ]);
        assert!(matches!(out[0], WorkerCommand::SetEqGains(_)));
        assert_eq!(bands(&out), [(2, 20)]);
    }

    #[test]
    fn noise_modes_keep_their_order() {
        let out = coalesce(vec![
            WorkerCommand::SetMode(NoiseModeRequest::Adaptive),
            WorkerCommand::SetMode(NoiseModeRequest::Custom),
        ]);
        assert_eq!(out.len(), 2);
    }
    // Audit v0.5.0 (Codex) : des actions distinctes ne se remplacent pas.
    #[test]
    fn actions_for_distinct_devices_are_all_kept() {
        let out = coalesce(vec![
            WorkerCommand::ToggleConnection {
                device: Connection::synthetic(1, "Appareil A", false),
                connect: true,
            },
            WorkerCommand::ToggleConnection {
                device: Connection::synthetic(2, "Appareil B", false),
                connect: true,
            },
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn exports_to_distinct_files_are_all_kept() {
        let out = coalesce(vec![
            WorkerCommand::ExportProfile("synthetique-a.json".into()),
            WorkerCommand::ExportProfile("synthetique-b.json".into()),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn a_value_is_not_merged_across_a_dependent_setting() {
        // Suivi de tête : refusé sans audio spatial. Fusionner les deux
        // « spatial » enverrait le suivi de tête avant l'activation.
        let out = coalesce(vec![
            WorkerCommand::SetSpatial(true),
            WorkerCommand::SetHeadTracking(true),
            WorkerCommand::SetSpatial(false),
        ]);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], WorkerCommand::SetSpatial(true)));
        // Le niveau dépend du mode de bruit : pas de fusion par-dessus.
        let out = coalesce(vec![
            WorkerCommand::SetLevel(25),
            WorkerCommand::SetMode(NoiseModeRequest::Adaptive),
            WorkerCommand::SetLevel(50),
        ]);
        assert_eq!(out.len(), 3);
        // Par-dessus un réglage indépendant, la fusion reste permise.
        let out = coalesce(vec![
            WorkerCommand::SetSpatial(true),
            WorkerCommand::SetAntiWind(true),
            WorkerCommand::SetSpatial(false),
        ]);
        assert_eq!(out.len(), 2);
        assert!(matches!(out[1], WorkerCommand::SetSpatial(false)));
    }

    // Parcours du worker sur MockTransport (aucun casque).
    use momentum_core::presets::Preset;
    use momentum_core::protocol::{CommandId, Frame, VENDOR_SENNHEISER};
    use momentum_core::transport::MockTransport;

    fn frame(cmd: u16, payload: &[u8]) -> Vec<u8> {
        Frame::new(VENDOR_SENNHEISER, CommandId::new(cmd), payload.to_vec())
            .encode()
            .unwrap()
    }

    fn ack_band(mock: MockTransport, band: u8, gain: i8) -> MockTransport {
        mock.expect(&frame(0x1001, &[band, gain as u8]), &frame(0x1101, &[]))
    }

    /// Réponses à `read_eq` pour un égaliseur de deux bandes.
    fn read_eq(mock: MockTransport, gains: [i8; 2]) -> MockTransport {
        let mut mock = mock.expect(
            &frame(0x1000, &[]),
            &frame(0x1100, &[2, (-60i8) as u8, 60, 0, 0]),
        );
        for (band, gain) in gains.into_iter().enumerate() {
            mock = mock.expect(&frame(0x1002, &[band as u8]), &frame(0x1102, &[gain as u8]));
        }
        mock.expect(&frame(0x1009, &[]), &frame(0x1109, &[0]))
    }

    fn eq(gains: [i8; 2]) -> EqState {
        EqState {
            min_gain: -60,
            max_gain: 60,
            gains: gains.to_vec(),
            bass_boost: false,
        }
    }

    fn run(
        mock: MockTransport,
        batch: Vec<WorkerCommand>,
        cache: &mut EqState,
    ) -> Headset<MockTransport> {
        let mut headset = Headset::new(mock, Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            coalesce(batch),
            &HeadsetState::default(),
            cache,
            &[],
            &|_| {},
        );
        assert!(result.is_ok(), "{result:?}");
        headset
    }

    #[test]
    fn flat_then_restore_in_one_batch_sends_the_restore() {
        let mock = ack_band(ack_band(MockTransport::new(), 0, 0), 0, 10);
        let mut cache = eq([10, 0]);
        let mut headset = run(
            mock,
            vec![
                WorkerCommand::SetEqFlat,
                WorkerCommand::SetEqGains(vec![10, 0]),
            ],
            &mut cache,
        );
        assert!(headset.client().transport().is_exhausted());
        assert_eq!(cache.gains, [10, 0]);
    }

    #[test]
    fn a_curve_after_a_scene_is_compared_to_a_fresh_reading() {
        // La scène écrit « Basses » avec sa propre lecture ; la courbe qui
        // suit est comparée à une nouvelle lecture, pas au cache d'avant la
        // scène (qui l'aurait jugée inutile).
        let mut mock = read_eq(MockTransport::new(), [0, 0]);
        mock = ack_band(ack_band(mock, 0, 5), 1, 5);
        mock = read_eq(mock, [5, 5]);
        mock = ack_band(ack_band(mock, 0, 0), 1, 0);
        let scene = Scene {
            name: "Essai".into(),
            equalizer: Some("Basses".into()),
            ..Scene::default()
        };
        let presets = vec![Preset {
            name: "Basses".into(),
            gains: vec![5, 5],
            builtin: false,
        }];
        let mut cache = eq([0, 0]);
        let mut headset = run(
            mock,
            vec![
                WorkerCommand::ApplyScene {
                    scene,
                    presets,
                    reply: None,
                },
                WorkerCommand::SetEqGains(vec![0, 0]),
            ],
            &mut cache,
        );
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn a_band_refused_mid_curve_keeps_the_cache_true() {
        // Bande 0 acquittée, bande 1 refusée (1181) : le cache garde
        // l'ancien gain de la bande 1, et la commande suivante s'exécute.
        let mock = ack_band(MockTransport::new(), 0, 0)
            .expect(&frame(0x1001, &[1, 0]), &frame(0x1181, &[1]))
            .expect(&frame(0x1008, &[1]), &frame(0x1108, &[]));
        let mut cache = eq([10, 10]);
        let mut headset = run(
            mock,
            vec![WorkerCommand::SetEqFlat, WorkerCommand::SetBassBoost(true)],
            &mut cache,
        );
        assert!(headset.client().transport().is_exhausted());
        assert_eq!(cache.gains, [0, 10]);
    }

    #[test]
    fn a_timeout_stops_the_batch_without_resending() {
        // Aucune réponse à la première bande : session perdue, la seconde
        // bande et la commande suivante ne partent pas.
        let mut headset = Headset::new(MockTransport::new(), Duration::from_millis(20));
        let mut cache = eq([10, 10]);
        let result = run_batch(
            &mut headset,
            vec![WorkerCommand::SetEqFlat, WorkerCommand::SetBassBoost(true)],
            &HeadsetState::default(),
            &mut cache,
            &[],
            &|_| {},
        );
        assert!(matches!(result, Err(SessionEnd::Lost(_))));
        assert_eq!(headset.client().transport().sent().len(), 1);
        assert_eq!(cache.gains, [10, 10]);
    }

    #[test]
    fn a_connection_is_found_by_address_not_by_name() {
        // Deux appareils du même nom : seul le second est visé.
        let paired = [
            Connection::synthetic(1, "Téléphone", true),
            Connection::synthetic(2, "Téléphone", false),
        ];
        let mock =
            MockTransport::new().expect(&frame(0x140F, &[0, 0, 0, 0, 0, 2]), &frame(0x150F, &[]));
        let mut headset = Headset::new(mock, Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            vec![
                WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(2, "Téléphone", false),
                    connect: true,
                },
                // Déjà connecté : rien n'est envoyé.
                WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(1, "Téléphone", true),
                    connect: true,
                },
            ],
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &paired,
            &|_| {},
        );
        assert!(result.is_ok());
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn an_unknown_device_is_refused_without_traffic() {
        let refused = std::cell::Cell::new(false);
        let mut headset = Headset::new(MockTransport::new(), Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            vec![WorkerCommand::ToggleConnection {
                device: Connection::synthetic(9, "Ancien", false),
                connect: true,
            }],
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &[Connection::synthetic(1, "Ancien", false)],
            &|e| refused.set(matches!(e, WorkerEvent::Refused(_))),
        );
        assert!(result.is_ok());
        assert!(refused.get());
        assert!(headset.client().transport().sent().is_empty());
    }

    #[test]
    fn profile_selection_goes_through_the_fresh_reading_guard() {
        // Le cache du worker dit « profil » mais la lecture fraîche dit
        // « égaliseur » : 0803 part. Puis profil lu : rien n'est écrit.
        let cache = HeadsetState {
            audio_mode: Some(momentum_core::device::AudioMode::Personalization),
            ..HeadsetState::default()
        };
        let preconditions = |mock: MockTransport| {
            mock.expect(&frame(0x2001, &[]), &frame(0x2101, &[2]))
                .expect(&frame(0x3605, &[]), &frame(0x3705, &[0]))
        };
        let mock = preconditions(MockTransport::new())
            .expect(&frame(0x0804, &[]), &frame(0x0904, &[0, 1]))
            .expect(&frame(0x0803, &[0, 3]), &frame(0x0903, &[]));
        let mock = preconditions(mock).expect(&frame(0x0804, &[]), &frame(0x0904, &[0, 3]));
        let mut headset = Headset::new(mock, Duration::from_millis(20));
        for _ in 0..2 {
            let result = run_batch(
                &mut headset,
                vec![WorkerCommand::SetSoundProfile(true)],
                &cache,
                &mut eq([0, 0]),
                &[],
                &|_| {},
            );
            assert!(result.is_ok());
        }
        assert!(headset.client().transport().is_exhausted());
    }
}
