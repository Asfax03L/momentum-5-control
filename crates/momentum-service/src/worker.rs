//! Thread « casque » : connexion, abonnement aux notifications, application
//! des commandes du menu et reconnexion automatique.
//!
//! Il possède seul la connexion RFCOMM (WinRT sous Windows, BlueZ sous
//! Linux). L'interface lui parle par un canal `mpsc` (commandes) et il
//! répond par une fonction `notify` (événements).

use momentum_core::i18n::{tr, trf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::time::{Duration, Instant};

use momentum_core::device::{
    Connection, DeviceError, DeviceInfo, EqState, Headset, HeadsetState, NoiseModeRequest,
    ProfileSelection, PromptMode,
};
use momentum_core::paths;
use momentum_core::presets::Preset;
use momentum_core::protocol::messages::M5NoiseMode;
use momentum_core::request::Outcome;
use momentum_core::scenes::{Scene, SceneReport, apply_scene};
use momentum_core::transport::Transport;
#[cfg(target_os = "linux")]
use momentum_core::transport::bluez_rfcomm::{
    BluezTransport as PlatformTransport, list_gaia_devices,
};
use momentum_core::transport::choice::{Candidate, Choice, choose, fingerprint};
#[cfg(windows)]
use momentum_core::transport::windows_rfcomm::{
    RfcommTransport as PlatformTransport, list_gaia_services,
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

/// Demande au worker : une commande, son numéro (0 : aucun résultat
/// publié) et la connexion pour laquelle elle a été faite (0 : la connexion
/// courante, quelle qu'elle soit). Une demande faite pour une connexion
/// précédente est annulée sans rien envoyer.
#[derive(Debug)]
pub struct Job {
    pub id: u64,
    pub session: u64,
    pub command: WorkerCommand,
}

impl Job {
    /// Commande sans résultat publié (`Quit`, `Query`, essais).
    pub fn untracked(command: WorkerCommand) -> Self {
        Self {
            id: 0,
            session: 0,
            command,
        }
    }
}

impl AsRef<WorkerCommand> for Job {
    fn as_ref(&self) -> &WorkerCommand {
        &self.command
    }
}

impl AsRef<WorkerCommand> for WorkerCommand {
    fn as_ref(&self) -> &WorkerCommand {
        self
    }
}

#[derive(Debug, Clone)]
pub enum WorkerEvent {
    Searching,
    /// Recherche terminée sans casque (raison), avant un nouvel essai.
    NotFound(String),
    /// `read_only` : raison de la lecture seule (modèle ou micrologiciel non
    /// validé pour les écritures). `session` : numéro de cette connexion
    /// (croissant, à partir de 1).
    Connected {
        name: String,
        /// Empreinte déjà calculée lors du choix, sans adresse en clair.
        headset_id: String,
        read_only: Option<String>,
        session: u64,
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
    /// Résultat de la demande `id` (un seul par demande suivie) ;
    /// `details` : compte rendu d'une scène, fichier d'un export.
    Done {
        id: u64,
        outcome: Outcome,
        details: Option<String>,
    },
}

/// Demande faite pour une autre connexion que `generation` (0 : aucune
/// connexion précisée).
fn for_other_session(job: &Job, generation: u64) -> bool {
    job.session != 0 && job.session != generation
}

/// Publie le résultat d'une demande suivie.
fn finish(notify: &impl Fn(WorkerEvent), id: u64, outcome: Outcome, details: Option<String>) {
    if id != 0 {
        notify(WorkerEvent::Done {
            id,
            outcome,
            details,
        });
    }
}

/// Annule sans envoi les demandes en attente (connexion perdue). `true` :
/// le worker doit s'arrêter.
fn cancel_pending(commands: &Receiver<Job>, notify: &impl Fn(WorkerEvent), reason: &str) -> bool {
    loop {
        match commands.try_recv() {
            Ok(Job {
                command: WorkerCommand::Quit,
                ..
            })
            | Err(TryRecvError::Disconnected) => return true,
            Ok(job) => finish(notify, job.id, Outcome::Cancelled(reason.to_string()), None),
            Err(TryRecvError::Empty) => return false,
        }
    }
}

#[derive(Debug)]
enum SessionEnd {
    Quit,
    Lost(String),
}

pub fn run(commands: Receiver<Job>, notify: impl Fn(WorkerEvent)) {
    // SAFETY: première chose faite par ce thread, avant tout objet WinRT.
    #[cfg(windows)]
    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        warn!(%e, "RoInitialize");
    }
    // Numéro de la connexion courante : une demande faite pour une autre
    // connexion n'est jamais envoyée.
    let mut generation = 0u64;
    loop {
        notify(WorkerEvent::Searching);
        match connect() {
            Ok((name, print, mut headset)) => {
                // Écritures réservées au modèle et au micrologiciel
                // validés ; la lecture reste possible sur tout casque GAIA.
                headset.identify();
                let read_only = headset.write_block_reason().map(str::to_string);
                // Casque validé : mémorisé pour le retrouver même renommé.
                if read_only.is_none()
                    && let Err(e) = paths::store_headset_fingerprint(&print)
                {
                    warn!(%e, "mémorisation du casque");
                }
                generation += 1;
                // Le nom du casque peut être personnel : il ne va pas au journal.
                info!(?read_only, session = generation, "casque connecté");
                notify(WorkerEvent::Connected {
                    name,
                    headset_id: print,
                    read_only,
                    session: generation,
                });
                match session(headset, generation, &commands, &notify) {
                    SessionEnd::Quit => return,
                    SessionEnd::Lost(reason) => {
                        warn!(%reason, "connexion perdue");
                        notify(WorkerEvent::Lost(reason));
                        // Rien de l'ancienne connexion n'est rejoué.
                        if cancel_pending(&commands, &notify, "connexion perdue") {
                            return;
                        }
                    }
                }
            }
            Err(reason) => {
                debug!(%reason, "casque introuvable");
                notify(WorkerEvent::NotFound(reason));
            }
        }
        // Attente avant nouvel essai, en restant réactif à « Quitter ».
        match commands.recv_timeout(RETRY_DELAY) {
            Ok(Job {
                command: WorkerCommand::Quit,
                ..
            })
            | Err(RecvTimeoutError::Disconnected) => return,
            Ok(job) => finish(
                &notify,
                job.id,
                Outcome::Cancelled(tr("casque non connecté").to_string()),
                None,
            ),
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Repères du nom du casque : ils départagent plusieurs appareils GAIA
/// connectés, sans jamais en exclure un (le nom peut être changé).
const NAME_HINTS: [&str; 2] = ["momentum", "sennheiser"];

/// Casque choisi parmi les appareils GAIA connectés (F04 de la revue) : le
/// casque mémorisé, sinon le seul candidat, sinon le seul au nom distinctif.
/// Plusieurs candidats sans moyen de choisir : aucun n'est pris au hasard.
fn pick(candidates: &[Candidate]) -> Result<usize, String> {
    match choose(
        candidates,
        paths::load_headset_fingerprint().as_deref(),
        &NAME_HINTS,
    ) {
        Choice::One(i) => Ok(i),
        Choice::None => Err(tr("aucun casque connecté avec service GAIA").to_string()),
        // Les noms peuvent être personnels : seulement leur nombre.
        Choice::Ambiguous(names) => Err(trf(
            "{} appareils GAIA connectés et aucun casque mémorisé : en garder un seul connecté",
            &[&(names.len())],
        )),
    }
}

/// Casque connecté : nom, empreinte de son identifiant (à mémoriser une
/// fois validé) et canal GAIA ouvert.
type Found = (String, String, Headset<PlatformTransport>);

#[cfg(windows)]
fn connect() -> Result<Found, String> {
    let mut found = list_gaia_services().map_err(|e| e.to_string())?;
    let candidates: Vec<Candidate> = found
        .iter()
        .map(|s| Candidate {
            key: &s.device_id,
            name: &s.device_name,
        })
        .collect();
    let chosen = found.swap_remove(pick(&candidates)?);
    let print = fingerprint(&chosen.device_id);
    let transport = PlatformTransport::connect(&chosen.service).map_err(|e| e.to_string())?;
    Ok((
        chosen.device_name,
        print,
        Headset::new(transport, REPLY_TIMEOUT),
    ))
}

/// Casque appairé et connecté dans BlueZ ; le profil client BlueZ choisit
/// le canal GAIA (comme `ctl` sans `--canal`, validé sur le M5).
#[cfg(target_os = "linux")]
fn connect() -> Result<Found, String> {
    let mut found = list_gaia_devices().map_err(|e| e.to_string())?;
    let keys: Vec<String> = found.iter().map(|d| d.address.to_string()).collect();
    let candidates: Vec<Candidate> = found
        .iter()
        .zip(&keys)
        .map(|(d, key)| Candidate {
            key,
            name: &d.device_name,
        })
        .collect();
    let index = pick(&candidates)?;
    let print = fingerprint(&keys[index]);
    let chosen = found.swap_remove(index);
    let transport = PlatformTransport::connect(&chosen, None).map_err(|e| e.to_string())?;
    Ok((
        chosen.device_name,
        print,
        Headset::new(transport, REPLY_TIMEOUT),
    ))
}

fn session<T: Transport>(
    mut headset: Headset<T>,
    generation: u64,
    commands: &Receiver<Job>,
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
                Ok(Job {
                    command: WorkerCommand::Quit,
                    ..
                })
                | Err(TryRecvError::Disconnected) => {
                    return SessionEnd::Quit;
                }
                Ok(Job {
                    command: WorkerCommand::Query(reply),
                    ..
                }) => {
                    let _ = reply.send(state);
                }
                Ok(job) if for_other_session(&job, generation) => finish(
                    notify,
                    job.id,
                    Outcome::Cancelled(
                        tr("demande faite pour une connexion précédente").to_string(),
                    ),
                    None,
                ),
                Ok(job) => pending.push(job),
                Err(TryRecvError::Empty) => break,
            }
        }
        if !pending.is_empty() {
            let (batch, superseded) = coalesce(pending);
            for job in superseded {
                finish(
                    notify,
                    job.id,
                    Outcome::Cancelled(tr("remplacée par une demande plus récente").to_string()),
                    None,
                );
            }
            let touches_eq = batch.iter().any(|j| {
                j.command.is_eq() || matches!(j.command, WorkerCommand::ApplyScene { .. })
            });
            let touches_state = batch.iter().any(|j| !j.command.is_eq());
            if batch.iter().any(|j| {
                matches!(
                    j.command,
                    WorkerCommand::SetMultipoint(_) | WorkerCommand::ToggleConnection { .. }
                )
            }) {
                links_recheck = Some(Instant::now() + LINKS_RECHECK_DELAY);
            }
            let touches_info = batch.iter().any(|j| {
                matches!(
                    j.command,
                    WorkerCommand::SetPromptVolume(_)
                        | WorkerCommand::SetName(_)
                        | WorkerCommand::SetMultipoint(_)
                )
            });
            // Résultats des scènes, publiés après la relecture : un client
            // qui relit l'état à la réception de `Done` voit celui de la scène.
            let mut scenes_done = Vec::new();
            let sent = match run_batch(
                &mut headset,
                batch,
                &state,
                &mut eq,
                &paired,
                notify,
                &mut scenes_done,
            ) {
                Ok(sent) => sent,
                Err(end) => {
                    finish_all(notify, scenes_done);
                    return end;
                }
            };
            // Relecture : l'interface reflète toujours l'état réel du casque,
            // et chaque demande acquittée est confirmée sur cette relecture.
            // Une scène garde son résultat (chaque étape a été relue).
            let unverified = |reason: &str, scenes_done: Vec<Finished>| {
                for job in &sent {
                    finish(notify, job.id, Outcome::Unknown(reason.to_string()), None);
                }
                finish_all(notify, scenes_done);
            };
            if touches_eq {
                match headset.read_eq() {
                    Ok(e) => {
                        debug!(?e, "égaliseur relu après commande");
                        eq = e;
                        notify(WorkerEvent::Equalizer(eq.clone()));
                    }
                    Err(e) => {
                        unverified("connexion perdue avant la relecture", scenes_done);
                        return lost(e);
                    }
                }
            }
            if touches_info {
                match headset.read_device_info() {
                    Ok(i) => {
                        info = i;
                        notify(WorkerEvent::Info(info.clone()));
                    }
                    Err(e) => {
                        unverified("connexion perdue avant la relecture", scenes_done);
                        return lost(e);
                    }
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
                    Err(e) => {
                        unverified("connexion perdue avant la relecture", scenes_done);
                        return lost(e);
                    }
                }
            }
            for (i, job) in sent.iter().enumerate() {
                // Une demande suivie d'une autre qui touche au même réglage
                // (ou à un réglage lié) ne peut pas être jugée sur la relecture.
                let overridden = sent[i + 1..]
                    .iter()
                    .any(|later| related(&job.command, &later.command));
                let outcome = if overridden {
                    Outcome::Acknowledged
                } else {
                    verify(&job.command, &state, &eq, &info)
                };
                finish(notify, job.id, outcome, None);
            }
            finish_all(notify, scenes_done);
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

/// Exécute un lot de demandes dans l'ordre. `eq` reste fidèle au casque
/// entre deux commandes : les écritures d'égaliseur le tiennent à jour, et
/// il est relu avant la commande d'égaliseur qui suit une scène (la scène
/// écrit avec sa propre lecture). Rend les demandes acquittées, à confirmer
/// par la relecture ; les autres ont déjà leur résultat. Une erreur qui
/// désynchronise la session l'arrête sans rien renvoyer : la demande en
/// cours et celles déjà acquittées ont un résultat inconnu, les suivantes
/// sont annulées sans envoi.
/// Résultat différé d'une demande : numéro, issue, compte rendu.
type Finished = (u64, Outcome, Option<String>);

fn finish_all(notify: &impl Fn(WorkerEvent), done: Vec<Finished>) {
    for (id, outcome, details) in done {
        finish(notify, id, outcome, details);
    }
}

fn run_batch<T: Transport>(
    headset: &mut Headset<T>,
    batch: Vec<Job>,
    state: &HeadsetState,
    eq: &mut EqState,
    paired: &[Connection],
    notify: &impl Fn(WorkerEvent),
    scenes_done: &mut Vec<Finished>,
) -> Result<Vec<Job>, SessionEnd> {
    let mut sent: Vec<Job> = Vec::new();
    let mut eq_stale = false;
    let mut jobs = batch.into_iter();
    // Session perdue pendant `id` : rien n'est renvoyé.
    let abort = |id: u64,
                 reason: String,
                 sent: &[Job],
                 rest: &mut dyn Iterator<Item = Job>|
     -> SessionEnd {
        finish(notify, id, Outcome::Unknown(reason.clone()), None);
        for job in sent {
            finish(
                notify,
                job.id,
                Outcome::Unknown(tr("connexion perdue avant la relecture").to_string()),
                None,
            );
        }
        for job in rest {
            finish(
                notify,
                job.id,
                Outcome::Cancelled(tr("connexion perdue avant l'envoi").to_string()),
                None,
            );
        }
        SessionEnd::Lost(reason)
    };
    while let Some(job) = jobs.next() {
        let id = job.id;
        match job.command {
            WorkerCommand::ApplyScene {
                scene,
                presets,
                reply,
            } => {
                let report = apply_scene(headset, &scene, &presets);
                info!(complete = report.is_complete(), "scène appliquée");
                if let Some(reply) = reply {
                    let _ = reply.send(report.clone());
                }
                let details = Some(report.details());
                notify(WorkerEvent::SceneApplied(report.clone()));
                if headset.client().is_desynchronized() {
                    finish(
                        notify,
                        id,
                        Outcome::Unknown(tr("échange interrompu pendant la scène").to_string()),
                        details,
                    );
                    return Err(abort(
                        0,
                        "échange interrompu pendant une scène".to_string(),
                        &sent,
                        &mut jobs,
                    ));
                }
                let outcome = if report.is_complete() {
                    Outcome::Applied
                } else {
                    Outcome::NotApplied(tr("scène appliquée en partie").to_string())
                };
                scenes_done.push((id, outcome, details));
                eq_stale = true;
            }
            WorkerCommand::ToggleConnection { device, connect } => {
                let Some(current) = paired.iter().find(|d| d.same_device(&device)) else {
                    let reason = "la liste des appareils a changé, réessaie".to_string();
                    notify(WorkerEvent::Refused(reason.clone()));
                    finish(notify, id, Outcome::Refused(reason), None);
                    continue;
                };
                if current.connected == connect {
                    finish(notify, id, Outcome::AlreadySatisfied, None);
                    continue;
                }
                match headset.set_connection(current, connect) {
                    // L'état suit par les notifications 1491/1484.
                    Ok(()) => finish(notify, id, Outcome::Acknowledged, None),
                    Err(e) if headset.client().is_desynchronized() => {
                        return Err(abort(id, e.to_string(), &sent, &mut jobs));
                    }
                    Err(e) => {
                        notify(WorkerEvent::Refused(e.to_string()));
                        finish(notify, id, Outcome::Refused(e.to_string()), None);
                    }
                }
            }
            WorkerCommand::ExportProfile(path) => {
                let result = export_profile(headset, &path);
                if headset.client().is_desynchronized() {
                    return Err(abort(
                        id,
                        "délai dépassé pendant l'export".to_string(),
                        &sent,
                        &mut jobs,
                    ));
                }
                match &result {
                    Ok(()) => finish(
                        notify,
                        id,
                        Outcome::Applied,
                        Some(path.display().to_string()),
                    ),
                    Err(reason) => finish(notify, id, Outcome::Refused(reason.clone()), None),
                }
                notify(WorkerEvent::Exported(result.map(|()| path)));
            }
            command => {
                if command.is_eq() && eq_stale {
                    match headset.read_eq() {
                        Ok(fresh) => *eq = fresh,
                        Err(e) => {
                            let rest = std::iter::once(Job {
                                id,
                                session: job.session,
                                command,
                            });
                            let mut rest = rest.chain(&mut jobs);
                            return Err(abort(0, e.to_string(), &sent, &mut rest));
                        }
                    }
                    eq_stale = false;
                }
                let kept = command.clone_setting();
                match apply(headset, state, eq, command) {
                    Ok(()) => sent.push(Job {
                        id,
                        session: job.session,
                        command: kept,
                    }),
                    // Pas de nouvel essai : l'écriture a pu être appliquée.
                    Err(e) if headset.client().is_desynchronized() => {
                        return Err(abort(id, e.to_string(), &sent, &mut jobs));
                    }
                    Err(e) => {
                        notify(WorkerEvent::Refused(e.to_string()));
                        finish(notify, id, Outcome::Refused(e.to_string()), None);
                    }
                }
            }
        }
    }
    Ok(sent)
}

/// Résultat d'une écriture acquittée, d'après la relecture qui suit.
fn verify(
    command: &WorkerCommand,
    state: &HeadsetState,
    eq: &EqState,
    info: &DeviceInfo,
) -> Outcome {
    use momentum_core::device::AudioMode;
    let noise = &state.noise;
    let confirmed = match command {
        WorkerCommand::SetMode(NoiseModeRequest::Off) => Some(!noise.anc_enabled),
        WorkerCommand::SetMode(NoiseModeRequest::Adaptive) => {
            Some(noise.anc_enabled && noise.adaptive)
        }
        WorkerCommand::SetMode(NoiseModeRequest::Custom) => {
            Some(noise.anc_enabled && !noise.adaptive)
        }
        WorkerCommand::SetLevel(level) => {
            Some(noise.anc_enabled && !noise.adaptive && noise.level == *level)
        }
        WorkerCommand::SetAntiWind(on) => Some(noise.anti_wind == *on),
        WorkerCommand::SetSpatial(on) => Some(state.spatial_audio == Some(*on)),
        WorkerCommand::SetHeadTracking(on) => Some(state.head_tracking == Some(*on)),
        WorkerCommand::SetMultipoint(on) => Some(info.multipoint == Some(*on)),
        WorkerCommand::SetEqBand(band, gain) => {
            Some(eq.gains.get(usize::from(*band)) == Some(gain))
        }
        WorkerCommand::SetEqFlat => Some(eq.gains.iter().all(|&g| g == 0)),
        WorkerCommand::SetEqGains(gains) => Some(&eq.gains == gains),
        WorkerCommand::SetBassBoost(on) => Some(eq.bass_boost == *on),
        WorkerCommand::SetSpeechClarity(true) => {
            Some(state.audio_mode == Some(AudioMode::SpeechClarity))
        }
        WorkerCommand::SetSoundProfile(true) => {
            Some(state.audio_mode == Some(AudioMode::Personalization))
        }
        WorkerCommand::SetSpeechClarity(false) | WorkerCommand::SetSoundProfile(false) => {
            Some(state.audio_mode == Some(AudioMode::Equalizer))
        }
        WorkerCommand::SetTouchControls(on) => Some(state.touch_controls == Some(*on)),
        WorkerCommand::SetBatteryProtection(on) => Some(state.battery_protection == Some(*on)),
        WorkerCommand::SetSmartPause(on) => Some(state.smart_pause == Some(*on)),
        WorkerCommand::SetAutoAnswer(on) => Some(state.auto_answer == Some(*on)),
        WorkerCommand::SetComfortCall(on) => Some(state.comfort_call == Some(*on)),
        WorkerCommand::SetAutoOff(seconds) => Some(state.auto_off_seconds == Some(*seconds)),
        WorkerCommand::SetPromptMode(mode) => Some(state.prompt_mode == Some(*mode)),
        WorkerCommand::SetPromptVolume(value) => Some(info.prompt_volume == Some(*value)),
        WorkerCommand::SetName(name) => Some(info.name.as_deref() == Some(name.as_str())),
        WorkerCommand::ToggleConnection { .. }
        | WorkerCommand::ApplyScene { .. }
        | WorkerCommand::ExportProfile(_)
        | WorkerCommand::Query(_)
        | WorkerCommand::Quit => None,
    };
    match confirmed {
        Some(true) => Outcome::Applied,
        Some(false) => Outcome::NotApplied(
            tr("la relecture ne montre pas la valeur demandée (changée ailleurs ?)").to_string(),
        ),
        None => Outcome::Acknowledged,
    }
}

/// Deux réglages dont le second peut changer ce que la relecture montre du
/// premier (même réglage, modes de bruit, modes audio, courbe complète).
fn related(first: &WorkerCommand, later: &WorkerCommand) -> bool {
    use WorkerCommand as C;
    let audio = |c: &WorkerCommand| {
        matches!(
            c,
            C::SetSpatial(_)
                | C::SetHeadTracking(_)
                | C::SetSpeechClarity(_)
                | C::SetSoundProfile(_)
        )
    };
    let noise = |c: &WorkerCommand| matches!(c, C::SetMode(_) | C::SetLevel(_));
    let curve = |c: &WorkerCommand| matches!(c, C::SetEqFlat | C::SetEqGains(_));
    let gains = |c: &WorkerCommand| matches!(c, C::SetEqBand(..)) || curve(c);
    first.key() == later.key()
        || (audio(first) && audio(later))
        || (noise(first) && noise(later))
        || (gains(first) && gains(later) && (curve(first) || curve(later)))
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
/// relecture complète la conserve, mais seulement si le codec n'a pas
/// changé entre-temps (F11 : une ancienne fréquence n'est pas présentée
/// comme celle du flux actuel).
fn keep_stream_info(old: &HeadsetState, mut new: HeadsetState) -> HeadsetState {
    if new.sample_rate_hz.is_none() && new.codec == old.codec {
        new.sample_rate_hz = old.sample_rate_hz;
    }
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

    /// Copie d'un réglage, gardée pour la vérifier après la relecture.
    /// Les actions (scène, connexion, export, requête) ne passent pas par là.
    fn clone_setting(&self) -> WorkerCommand {
        use WorkerCommand as C;
        match self {
            C::SetMode(m) => C::SetMode(*m),
            C::SetLevel(l) => C::SetLevel(*l),
            C::SetAntiWind(on) => C::SetAntiWind(*on),
            C::SetSpatial(on) => C::SetSpatial(*on),
            C::SetHeadTracking(on) => C::SetHeadTracking(*on),
            C::SetMultipoint(on) => C::SetMultipoint(*on),
            C::SetEqBand(b, g) => C::SetEqBand(*b, *g),
            C::SetEqFlat => C::SetEqFlat,
            C::SetEqGains(g) => C::SetEqGains(g.clone()),
            C::SetBassBoost(on) => C::SetBassBoost(*on),
            C::SetSpeechClarity(on) => C::SetSpeechClarity(*on),
            C::SetSoundProfile(on) => C::SetSoundProfile(*on),
            C::SetTouchControls(on) => C::SetTouchControls(*on),
            C::SetBatteryProtection(on) => C::SetBatteryProtection(*on),
            C::SetSmartPause(on) => C::SetSmartPause(*on),
            C::SetAutoAnswer(on) => C::SetAutoAnswer(*on),
            C::SetComfortCall(on) => C::SetComfortCall(*on),
            C::SetAutoOff(s) => C::SetAutoOff(*s),
            C::SetPromptMode(m) => C::SetPromptMode(*m),
            C::SetPromptVolume(v) => C::SetPromptVolume(*v),
            C::SetName(n) => C::SetName(n.clone()),
            C::ExportProfile(p) => C::ExportProfile(p.clone()),
            C::ToggleConnection { device, connect } => C::ToggleConnection {
                device: device.clone(),
                connect: *connect,
            },
            C::ApplyScene { scene, presets, .. } => C::ApplyScene {
                scene: scene.clone(),
                presets: presets.clone(),
                reply: None,
            },
            C::Query(_) | C::Quit => C::Quit,
        }
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
///
/// Rend les demandes gardées et celles qu'une demande plus récente remplace.
fn coalesce<J: AsRef<WorkerCommand>>(commands: Vec<J>) -> (Vec<J>, Vec<J>) {
    let mut kept: Vec<J> = Vec::with_capacity(commands.len());
    let mut superseded = Vec::new();
    for item in commands {
        let command = item.as_ref();
        if matches!(
            command,
            WorkerCommand::SetEqFlat | WorkerCommand::SetEqGains(_)
        ) {
            let (bands, others): (Vec<J>, Vec<J>) = kept
                .into_iter()
                .partition(|c| matches!(c.as_ref(), WorkerCommand::SetEqBand(..)));
            superseded.extend(bands);
            kept = others;
        }
        if command.replaceable() {
            let key = command.key();
            for i in (0..kept.len()).rev() {
                if kept[i].as_ref().key() == key {
                    superseded.push(kept.remove(i));
                    break;
                }
                if !kept[i].as_ref().independent() {
                    break;
                }
            }
        }
        kept.push(item);
    }
    (kept, superseded)
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

    fn jobs(commands: Vec<WorkerCommand>) -> Vec<Job> {
        commands.into_iter().map(Job::untracked).collect()
    }

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
        let (out, _) = coalesce(vec![
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
        let (out, _) = coalesce(vec![
            WorkerCommand::SetEqBand(2, 30),
            WorkerCommand::SetEqFlat,
            WorkerCommand::SetEqBand(3, -20),
        ]);
        assert!(matches!(out[0], WorkerCommand::SetEqFlat));
        assert_eq!(bands(&out), [(3, -20)]);
    }

    #[test]
    fn a_preset_replaces_pending_band_moves() {
        let (out, _) = coalesce(vec![
            WorkerCommand::SetEqBand(1, 10),
            WorkerCommand::SetEqGains(vec![0; 8]),
            WorkerCommand::SetEqBand(2, 20),
        ]);
        assert!(matches!(out[0], WorkerCommand::SetEqGains(_)));
        assert_eq!(bands(&out), [(2, 20)]);
    }

    #[test]
    fn noise_modes_keep_their_order() {
        let (out, _) = coalesce(vec![
            WorkerCommand::SetMode(NoiseModeRequest::Adaptive),
            WorkerCommand::SetMode(NoiseModeRequest::Custom),
        ]);
        assert_eq!(out.len(), 2);
    }
    // Audit v0.5.0 (Codex) : des actions distinctes ne se remplacent pas.
    #[test]
    fn actions_for_distinct_devices_are_all_kept() {
        let (out, _) = coalesce(vec![
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
        let (out, _) = coalesce(vec![
            WorkerCommand::ExportProfile("synthetique-a.json".into()),
            WorkerCommand::ExportProfile("synthetique-b.json".into()),
        ]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn a_value_is_not_merged_across_a_dependent_setting() {
        // Suivi de tête : refusé sans audio spatial. Fusionner les deux
        // « spatial » enverrait le suivi de tête avant l'activation.
        let (out, _) = coalesce(vec![
            WorkerCommand::SetSpatial(true),
            WorkerCommand::SetHeadTracking(true),
            WorkerCommand::SetSpatial(false),
        ]);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], WorkerCommand::SetSpatial(true)));
        // Le niveau dépend du mode de bruit : pas de fusion par-dessus.
        let (out, _) = coalesce(vec![
            WorkerCommand::SetLevel(25),
            WorkerCommand::SetMode(NoiseModeRequest::Adaptive),
            WorkerCommand::SetLevel(50),
        ]);
        assert_eq!(out.len(), 3);
        // Par-dessus un réglage indépendant, la fusion reste permise.
        let (out, _) = coalesce(vec![
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
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            coalesce(jobs(batch)).0,
            &HeadsetState::default(),
            cache,
            &[],
            &|_| {},
            &mut Vec::new(),
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
    fn a_scene_result_waits_for_the_read_back() {
        // Le `Done` d'une scène part après la relecture de fin de lot, comme
        // celui des autres réglages : run_batch le met de côté.
        let mut mock = read_eq(MockTransport::new(), [0, 0]);
        mock = ack_band(ack_band(mock, 0, 5), 1, 5);
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
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        let events = std::cell::RefCell::new(Vec::new());
        let mut scenes_done = Vec::new();
        let result = run_batch(
            &mut headset,
            coalesce(jobs(vec![WorkerCommand::ApplyScene {
                scene,
                presets,
                reply: None,
            }]))
            .0,
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &[],
            &|e| events.borrow_mut().push(e),
            &mut scenes_done,
        );
        assert!(result.is_ok(), "{result:?}");
        assert!(
            !events
                .borrow()
                .iter()
                .any(|e| matches!(e, WorkerEvent::Done { .. }))
        );
        assert!(matches!(
            scenes_done.as_slice(),
            [(_, Outcome::Applied, Some(_))]
        ));
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
        let mut headset =
            Headset::validated_for_tests(MockTransport::new(), Duration::from_millis(20));
        let mut cache = eq([10, 10]);
        let result = run_batch(
            &mut headset,
            jobs(vec![
                WorkerCommand::SetEqFlat,
                WorkerCommand::SetBassBoost(true),
            ]),
            &HeadsetState::default(),
            &mut cache,
            &[],
            &|_| {},
            &mut Vec::new(),
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
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            jobs(vec![
                WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(2, "Téléphone", false),
                    connect: true,
                },
                // Déjà connecté : rien n'est envoyé.
                WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(1, "Téléphone", true),
                    connect: true,
                },
            ]),
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &paired,
            &|_| {},
            &mut Vec::new(),
        );
        assert!(result.is_ok());
        assert!(headset.client().transport().is_exhausted());
    }

    #[test]
    fn an_unknown_device_is_refused_without_traffic() {
        let refused = std::cell::Cell::new(false);
        let mut headset =
            Headset::validated_for_tests(MockTransport::new(), Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            jobs(vec![WorkerCommand::ToggleConnection {
                device: Connection::synthetic(9, "Ancien", false),
                connect: true,
            }]),
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &[Connection::synthetic(1, "Ancien", false)],
            &|e| refused.set(matches!(e, WorkerEvent::Refused(_))),
            &mut Vec::new(),
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
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        for _ in 0..2 {
            let result = run_batch(
                &mut headset,
                jobs(vec![WorkerCommand::SetSoundProfile(true)]),
                &cache,
                &mut eq([0, 0]),
                &[],
                &|_| {},
                &mut Vec::new(),
            );
            assert!(result.is_ok());
        }
        assert!(headset.client().transport().is_exhausted());
    }

    fn tracked(id: u64, command: WorkerCommand) -> Job {
        Job {
            id,
            session: 0,
            command,
        }
    }

    /// Résultats publiés, par numéro de demande.
    fn outcomes(events: &[WorkerEvent]) -> Vec<(u64, Outcome)> {
        events
            .iter()
            .filter_map(|e| match e {
                WorkerEvent::Done { id, outcome, .. } => Some((*id, outcome.clone())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_lost_session_gives_unknown_then_cancelled_results() {
        // F06 : la bande 0 part sans réponse (résultat inconnu) ; la demande
        // suivante n'est pas envoyée (annulée), et rien n'est renvoyé.
        let events = std::cell::RefCell::new(Vec::new());
        let mut headset =
            Headset::validated_for_tests(MockTransport::new(), Duration::from_millis(20));
        let result = run_batch(
            &mut headset,
            vec![
                tracked(1, WorkerCommand::SetEqFlat),
                tracked(2, WorkerCommand::SetBassBoost(true)),
            ],
            &HeadsetState::default(),
            &mut eq([10, 10]),
            &[],
            &|e| events.borrow_mut().push(e),
            &mut Vec::new(),
        );
        assert!(matches!(result, Err(SessionEnd::Lost(_))));
        let got = outcomes(&events.borrow());
        assert!(matches!(got[0], (1, Outcome::Unknown(_))), "{got:?}");
        assert!(matches!(got[1], (2, Outcome::Cancelled(_))), "{got:?}");
        assert_eq!(headset.client().transport().sent().len(), 1);
    }

    #[test]
    fn a_refusal_is_reported_to_its_own_request() {
        let events = std::cell::RefCell::new(Vec::new());
        let mut headset =
            Headset::validated_for_tests(MockTransport::new(), Duration::from_millis(20));
        let sent = run_batch(
            &mut headset,
            vec![tracked(
                7,
                WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(9, "Ancien", false),
                    connect: true,
                },
            )],
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &[Connection::synthetic(1, "Ancien", false)],
            &|e| events.borrow_mut().push(e),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(sent.is_empty());
        assert!(matches!(
            outcomes(&events.borrow())[..],
            [(7, Outcome::Refused(_))]
        ));
    }

    #[test]
    fn acknowledged_writes_wait_for_the_reading() {
        // Acquittée : pas encore de résultat, la demande attend la relecture.
        let mock = ack_band(MockTransport::new(), 0, 5);
        let events = std::cell::RefCell::new(Vec::new());
        let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
        let sent = run_batch(
            &mut headset,
            vec![tracked(3, WorkerCommand::SetEqBand(0, 5))],
            &HeadsetState::default(),
            &mut eq([0, 0]),
            &[],
            &|e| events.borrow_mut().push(e),
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(sent.len(), 1);
        assert!(outcomes(&events.borrow()).is_empty());
        // Relu à 5 : appliquée ; relu à 0 : non appliquée.
        let info = DeviceInfo::default();
        assert_eq!(
            verify(
                &sent[0].command,
                &HeadsetState::default(),
                &eq([5, 0]),
                &info
            ),
            Outcome::Applied
        );
        assert!(matches!(
            verify(
                &sent[0].command,
                &HeadsetState::default(),
                &eq([0, 0]),
                &info
            ),
            Outcome::NotApplied(_)
        ));
    }

    #[test]
    fn verification_reads_the_requested_value() {
        let mut state = HeadsetState::default();
        state.noise.anti_wind = true;
        let info = DeviceInfo::default();
        let curve = eq([0, 0]);
        assert_eq!(
            verify(&WorkerCommand::SetAntiWind(true), &state, &curve, &info),
            Outcome::Applied
        );
        assert!(matches!(
            verify(&WorkerCommand::SetAntiWind(false), &state, &curve, &info),
            Outcome::NotApplied(_)
        ));
        // Non relu ici : accepté seulement.
        assert_eq!(
            verify(
                &WorkerCommand::ToggleConnection {
                    device: Connection::synthetic(1, "Téléphone", true),
                    connect: false,
                },
                &state,
                &curve,
                &info
            ),
            Outcome::Acknowledged
        );
        // Un mode de bruit suivi du curseur : le premier n'est pas jugé seul.
        assert!(related(
            &WorkerCommand::SetMode(NoiseModeRequest::Adaptive),
            &WorkerCommand::SetLevel(20)
        ));
        assert!(!related(
            &WorkerCommand::SetEqBand(0, 5),
            &WorkerCommand::SetEqBand(1, 5)
        ));
    }

    #[test]
    fn superseded_requests_are_returned_for_cancellation() {
        let (kept, superseded) = coalesce(vec![
            tracked(1, WorkerCommand::SetLevel(10)),
            tracked(2, WorkerCommand::SetLevel(40)),
            tracked(3, WorkerCommand::SetEqBand(0, 5)),
            tracked(4, WorkerCommand::SetEqFlat),
        ]);
        let ids = |v: &[Job]| v.iter().map(|j| j.id).collect::<Vec<_>>();
        assert_eq!(ids(&kept), [2, 4]);
        assert_eq!(ids(&superseded), [1, 3]);
    }

    #[test]
    fn requests_waiting_for_a_lost_connection_are_cancelled() {
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(tracked(5, WorkerCommand::SetAntiWind(true)))
            .unwrap();
        tx.send(tracked(6, WorkerCommand::SetLevel(30))).unwrap();
        let events = std::cell::RefCell::new(Vec::new());
        assert!(!cancel_pending(
            &rx,
            &|e| events.borrow_mut().push(e),
            "connexion perdue"
        ));
        let got = outcomes(&events.borrow());
        assert!(matches!(
            got[..],
            [(5, Outcome::Cancelled(_)), (6, Outcome::Cancelled(_))]
        ));
        // Rien ne reste pour la connexion suivante ; « Quitter » arrête.
        tx.send(Job::untracked(WorkerCommand::Quit)).unwrap();
        assert!(cancel_pending(&rx, &|_| {}, "connexion perdue"));
    }

    #[test]
    fn a_request_for_a_previous_connection_is_not_sent() {
        let job = Job {
            id: 1,
            session: 3,
            command: WorkerCommand::SetAntiWind(true),
        };
        assert!(for_other_session(&job, 4));
        assert!(!for_other_session(&job, 3));
        assert!(!for_other_session(
            &Job::untracked(WorkerCommand::SetEqFlat),
            4
        ));
    }

    #[test]
    fn an_old_sample_rate_does_not_survive_a_codec_change() {
        let old = HeadsetState {
            codec: Some(1),
            sample_rate_hz: Some(48_000),
            ..HeadsetState::default()
        };
        let same = HeadsetState {
            codec: Some(1),
            ..HeadsetState::default()
        };
        assert_eq!(keep_stream_info(&old, same).sample_rate_hz, Some(48_000));
        let other = HeadsetState {
            codec: Some(2),
            ..HeadsetState::default()
        };
        assert_eq!(keep_stream_info(&old, other).sample_rate_hz, None);
    }

    #[test]
    fn an_unavailable_charge_read_does_not_reuse_the_previous_state() {
        use momentum_core::device::{ChargeReading, ChargerStatus};

        let old = HeadsetState {
            charge_state: Some(ChargeReading {
                primary_status: ChargerStatus::Charging,
                secondary_raw: Some(0),
            }),
            ..HeadsetState::default()
        };
        // Le cache de la fréquence audio ne s'applique pas à la charge.
        assert_eq!(
            keep_stream_info(&old, HeadsetState::default()).charge_state,
            None
        );
    }

    #[test]
    fn an_unavailable_wearing_read_does_not_reuse_the_previous_state() {
        use momentum_core::device::OnHeadState;

        let old = HeadsetState {
            on_head_state: Some(OnHeadState::Worn),
            ..HeadsetState::default()
        };
        let unread = keep_stream_info(&old, HeadsetState::default());
        assert_eq!(unread.on_head_state, None);
        let fresh = HeadsetState {
            on_head_state: Some(OnHeadState::NotWorn),
            ..HeadsetState::default()
        };
        assert_eq!(
            keep_stream_info(&old, fresh).on_head_state,
            fresh.on_head_state
        );
    }
}
