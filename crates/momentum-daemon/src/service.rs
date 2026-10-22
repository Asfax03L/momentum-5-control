//! Partie commune du service, quel que soit le transport : état courant,
//! traduction des commandes de l'API, boucle des événements du worker
//! (historique de la batterie, notifications).

use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, Sender};

use anyhow::Context;
use momentum_client::{Command, ConnectionInfo, Event, Snapshot};
use momentum_core::alerts::{Alert, AlertTracker};
use momentum_core::battery_log::BatteryLog;
use momentum_core::device::Connection;
use momentum_core::request::Outcome;
use momentum_core::{paths, scenes};
use momentum_service::worker::{self, Job, WorkerCommand, WorkerEvent};

/// État partagé entre le transport et la boucle des événements.
#[derive(Default)]
pub struct Shared {
    pub snapshot: Snapshot,
    /// Appareils appairés avec leur adresse (jamais envoyée aux interfaces).
    pub devices: Vec<Connection>,
    /// Historique de la batterie, écrit dans `batterie.csv` à chaque mesure.
    pub battery_log: BatteryLog,
    /// Dernier numéro de demande attribué.
    pub last_id: u64,
    /// Entrée des événements du worker : le service y publie le résultat
    /// des demandes qu'il traite lui-même.
    pub events: Option<Sender<WorkerEvent>>,
}

/// Reçoit une demande d'une interface : numéro attribué, puis la demande
/// part au worker (ou est faite ici). Son résultat arrive en `Event::Done`
/// avec ce numéro. `Err` : refusée tout de suite, sans numéro ni envoi.
/// `session` : connexion vue par l'interface (0 : la connexion courante).
pub fn submit(
    command: Command,
    session: u64,
    shared: &mut Shared,
    commands: &Sender<Job>,
) -> Result<u64, String> {
    if session != 0 && session != shared.snapshot.session {
        return Err("demande faite pour une connexion précédente : état à relire".to_string());
    }
    let worker_command = handle(command, shared)?;
    shared.last_id += 1;
    let id = shared.last_id;
    match worker_command {
        Some(command) => commands
            .send(Job {
                id,
                session,
                command,
            })
            .map_err(|_| "le worker s'est arrêté".to_string())?,
        None => {
            if let Some(events) = &shared.events {
                let _ = events.send(WorkerEvent::Done {
                    id,
                    outcome: Outcome::Applied,
                    details: None,
                });
            }
        }
    }
    Ok(id)
}

/// Traite une commande de l'API : `Some` pour le worker (casque), `None`
/// si le service l'a faite lui-même ; `Err` : refusée (raison).
pub fn handle(command: Command, shared: &mut Shared) -> Result<Option<WorkerCommand>, String> {
    if command == Command::ClearBatteryHistory {
        shared.battery_log.clear();
        store_battery_log(&shared.battery_log);
        tracing::info!("historique de la batterie effacé");
        return Ok(None);
    }
    to_worker(command, shared).map(Some)
}

/// Traduit une commande du casque en commande du worker ; `Err` : refusée
/// par le service (raison).
fn to_worker(command: Command, shared: &Shared) -> Result<WorkerCommand, String> {
    let writable = shared.snapshot.state.is_some() && shared.snapshot.read_only.is_none();
    if !writable {
        return Err("casque absent ou en lecture seule".to_string());
    }
    Ok(match command {
        Command::SetMode(m) => WorkerCommand::SetMode(m),
        Command::SetLevel(l) => WorkerCommand::SetLevel(l.min(100)),
        Command::SetAntiWind(on) => WorkerCommand::SetAntiWind(on),
        Command::SetSpatial(on) => WorkerCommand::SetSpatial(on),
        Command::SetHeadTracking(on) => WorkerCommand::SetHeadTracking(on),
        Command::SetMultipoint(on) => WorkerCommand::SetMultipoint(on),
        Command::SetEqBand(band, gain) => WorkerCommand::SetEqBand(band, gain),
        Command::SetEqFlat => WorkerCommand::SetEqFlat,
        Command::SetEqGains(gains) => WorkerCommand::SetEqGains(gains),
        Command::SetBassBoost(on) => WorkerCommand::SetBassBoost(on),
        Command::SetSpeechClarity(on) => WorkerCommand::SetSpeechClarity(on),
        Command::SetSoundProfile(on) => WorkerCommand::SetSoundProfile(on),
        Command::SetTouchControls(on) => WorkerCommand::SetTouchControls(on),
        Command::SetBatteryProtection(on) => WorkerCommand::SetBatteryProtection(on),
        Command::SetSmartPause(on) => WorkerCommand::SetSmartPause(on),
        Command::SetAutoAnswer(on) => WorkerCommand::SetAutoAnswer(on),
        Command::SetComfortCall(on) => WorkerCommand::SetComfortCall(on),
        Command::SetAutoOff(s) => WorkerCommand::SetAutoOff(s),
        Command::SetPromptMode(m) => WorkerCommand::SetPromptMode(m),
        Command::SetPromptVolume(v) => WorkerCommand::SetPromptVolume(v),
        Command::SetName(n) => WorkerCommand::SetName(n),
        Command::ToggleConnection {
            index,
            name,
            connect,
            device,
        } => {
            // L'appareil désigné par son identifiant opaque (F05 de la
            // revue) ; une interface ancienne donne la position et le nom,
            // acceptés seulement si ce nom est unique dans la liste. Le worker
            // le retrouve ensuite par son adresse.
            let found = match &device {
                Some(id) => shared.devices.iter().find(|d| &d.opaque_id() == id),
                None => {
                    if shared.devices.iter().filter(|d| d.name == name).count() > 1 {
                        return Err(format!(
                            "plusieurs appareils s'appellent « {name} » : interface à mettre à jour"
                        ));
                    }
                    shared.devices.get(index).filter(|d| d.name == name)
                }
            };
            let device = found.cloned().ok_or_else(|| {
                format!("appareil « {name} » introuvable dans la liste (liste changée ?)")
            })?;
            WorkerCommand::ToggleConnection { device, connect }
        }
        Command::ApplyScene(name) => {
            let (list, _) = paths::load_scenes();
            let scene = scenes::find(&list, &name)
                .cloned()
                .ok_or_else(|| format!("scène « {name} » introuvable dans le fichier"))?;
            WorkerCommand::ApplyScene {
                scene,
                presets: paths::scene_presets(),
                reply: None,
            }
        }
        Command::ExportProfile(path) => WorkerCommand::ExportProfile(path),
        Command::ClearBatteryHistory => unreachable!("traitée par `handle`"),
    })
}

/// Événement du worker → événement de l'API (les adresses restent ici).
fn to_api(event: WorkerEvent, shared: &mut Shared) -> Event {
    match event {
        WorkerEvent::Searching => Event::Searching,
        WorkerEvent::Connected {
            name,
            read_only,
            session,
        } => Event::Connected {
            name,
            read_only,
            session,
        },
        WorkerEvent::State(s) => Event::State(s),
        WorkerEvent::Equalizer(eq) => Event::Equalizer(eq),
        WorkerEvent::Connections(devices) => {
            let list = devices
                .iter()
                .map(|d| ConnectionInfo {
                    id: d.opaque_id(),
                    name: d.name.clone(),
                    connected: d.connected,
                    this_device: d.this_device,
                    state: d.state,
                })
                .collect();
            shared.devices = devices;
            Event::Connections(list)
        }
        WorkerEvent::Info(info) => Event::Info(info),
        WorkerEvent::Exported(r) => Event::Exported(r),
        WorkerEvent::Lost(reason) => Event::Lost(reason),
        WorkerEvent::Refused(reason) => Event::Refused(reason),
        WorkerEvent::SceneApplied(report) => Event::SceneApplied(report),
        WorkerEvent::Done {
            id,
            outcome,
            details,
        } => Event::Done {
            id,
            outcome,
            details,
        },
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn load_battery_log() -> BatteryLog {
    paths::battery_file()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|text| BatteryLog::parse(&text, unix_now()))
        .unwrap_or_default()
}

fn store_battery_log(log: &BatteryLog) {
    let Some(path) = paths::battery_file() else {
        return;
    };
    if let Err(e) = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(&path, log.to_text()))
    {
        tracing::warn!(%e, "enregistrement de l'historique de la batterie");
    }
}

fn toast(alert: Option<Alert>) {
    if let Some(a) = alert {
        momentum_service::desktop::show_toast(&a.title, &a.body);
    }
}

/// Lance le worker (seul propriétaire de la connexion) ; ses événements
/// arrivent sur le récepteur rendu, avec les résultats publiés par le
/// service lui-même (`Shared::events`).
pub fn spawn_worker(
    commands: Receiver<Job>,
    shared: &Mutex<Shared>,
) -> anyhow::Result<Receiver<WorkerEvent>> {
    let (events_tx, events_rx) = mpsc::channel::<WorkerEvent>();
    if let Ok(mut s) = shared.lock() {
        s.events = Some(events_tx.clone());
    }
    std::thread::Builder::new()
        .name("casque".into())
        .spawn(move || {
            worker::run(commands, |event| {
                let _ = events_tx.send(event);
            });
        })
        .context("lancement du thread casque")?;
    Ok(events_rx)
}

/// Boucle des événements du worker : historique de la batterie,
/// notifications, état partagé, puis `publish` pour le transport. Revient
/// quand le worker s'arrête.
pub fn pump(
    events: Receiver<WorkerEvent>,
    shared: &Mutex<Shared>,
    mut publish: impl FnMut(&Event),
) {
    if let Ok(mut s) = shared.lock() {
        s.battery_log = load_battery_log();
    }
    let mut alerts = AlertTracker::new();
    for event in events {
        match &event {
            WorkerEvent::Connected { name, .. } => {
                // Le nom du casque peut être personnel : pas dans le journal.
                tracing::info!("casque connecté");
                toast(alerts.on_connected(name));
            }
            WorkerEvent::State(s) => {
                toast(alerts.on_battery(s.battery));
                if let Ok(mut sh) = shared.lock()
                    && sh.battery_log.record(unix_now(), s.battery)
                {
                    store_battery_log(&sh.battery_log);
                }
            }
            WorkerEvent::Lost(reason) => {
                tracing::warn!(%reason, "connexion perdue");
                toast(alerts.on_lost());
                if let Ok(mut sh) = shared.lock()
                    && sh.battery_log.interrupt(unix_now())
                {
                    store_battery_log(&sh.battery_log);
                }
            }
            WorkerEvent::Refused(reason) => tracing::warn!(%reason, "réglage refusé"),
            _ => {}
        }
        let api_event = {
            let Ok(mut s) = shared.lock() else { break };
            let api_event = to_api(event, &mut s);
            s.snapshot.apply(&api_event);
            api_event
        };
        publish(&api_event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use momentum_core::device::{HeadsetState, NoiseModeRequest};

    fn connected() -> Shared {
        let mut s = Shared::default();
        s.snapshot.device_name = Some("MOMENTUM 5".into());
        s.snapshot.state = Some(HeadsetState::default());
        s
    }

    #[test]
    fn battery_history_is_cleared_without_the_headset() {
        let mut shared = Shared::default();
        shared.battery_log.record(1_000, 80);
        shared.battery_log.record(2_000, 79);
        assert!(!shared.battery_log.entries().is_empty());
        // Casque absent : la commande est quand même faite, sans le worker.
        assert!(matches!(
            handle(Command::ClearBatteryHistory, &mut shared),
            Ok(None)
        ));
        assert!(shared.battery_log.entries().is_empty());
    }

    #[test]
    fn commands_need_a_writable_headset() {
        let absent = Shared::default();
        assert!(to_worker(Command::SetAntiWind(true), &absent).is_err());
        let mut read_only = connected();
        read_only.snapshot.read_only = Some("autre modèle".into());
        assert!(to_worker(Command::SetAntiWind(true), &read_only).is_err());
        assert!(matches!(
            to_worker(Command::SetMode(NoiseModeRequest::Off), &connected()),
            Ok(WorkerCommand::SetMode(NoiseModeRequest::Off))
        ));
    }

    #[test]
    fn connection_toggle_checks_the_name_at_that_position() {
        let mut s = connected();
        s.devices = vec![Connection::synthetic(1, "Téléphone", true)];
        assert!(
            to_worker(
                Command::ToggleConnection {
                    index: 0,
                    name: "Tablette".into(),
                    connect: false,
                    device: None,
                },
                &s
            )
            .is_err()
        );
        assert!(
            to_worker(
                Command::ToggleConnection {
                    index: 0,
                    name: "Téléphone".into(),
                    connect: false,
                    device: None,
                },
                &s
            )
            .is_ok()
        );
    }

    #[test]
    fn level_is_capped_at_100() {
        assert!(matches!(
            to_worker(Command::SetLevel(250), &connected()),
            Ok(WorkerCommand::SetLevel(100))
        ));
    }

    #[test]
    fn requests_get_a_number_and_stale_sessions_are_refused() {
        let (commands, received) = mpsc::channel();
        let mut shared = connected();
        shared.snapshot.session = 2;
        let first = submit(Command::SetAntiWind(true), 2, &mut shared, &commands).unwrap();
        let second = submit(Command::SetAntiWind(false), 0, &mut shared, &commands).unwrap();
        assert!(second > first);
        let job = received.try_recv().unwrap();
        assert_eq!((job.id, job.session), (first, 2));
        // Demande faite pour la connexion 1 : refusée sans numéro ni envoi.
        assert!(submit(Command::SetAntiWind(true), 1, &mut shared, &commands).is_err());
        assert!(received.try_recv().is_ok());
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn a_request_done_by_the_service_publishes_its_result() {
        let (commands, received) = mpsc::channel();
        let (events, published) = mpsc::channel();
        let mut shared = Shared {
            events: Some(events),
            ..Shared::default()
        };
        let id = submit(Command::ClearBatteryHistory, 0, &mut shared, &commands).unwrap();
        assert!(received.try_recv().is_err());
        assert!(matches!(
            published.try_recv(),
            Ok(WorkerEvent::Done { id: done, outcome: Outcome::Applied, .. }) if done == id
        ));
    }

    #[test]
    fn a_device_is_targeted_by_its_opaque_id() {
        // F05 : deux « Téléphone », liste dans un autre ordre que celui vu
        // par l'interface : l'identifiant désigne le bon appareil.
        let mut shared = connected();
        let first = Connection::synthetic(1, "Téléphone", true);
        let second = Connection::synthetic(2, "Téléphone", false);
        let wanted = second.opaque_id();
        shared.devices = vec![second.clone(), first.clone()];
        let toggle = |index, device| Command::ToggleConnection {
            index,
            name: "Téléphone".into(),
            connect: true,
            device,
        };
        match to_worker(toggle(1, Some(wanted)), &shared) {
            Ok(WorkerCommand::ToggleConnection { device, .. }) => {
                assert!(device.same_device(&second));
            }
            other => panic!("{other:?}"),
        }
        // Ancienne forme : position et nom ambigus, refusée.
        assert!(to_worker(toggle(1, None), &shared).is_err());
        // Appareil disparu de la liste : refusé.
        let gone = Connection::synthetic(9, "Téléphone", false).opaque_id();
        assert!(to_worker(toggle(0, Some(gone)), &shared).is_err());
    }

    #[test]
    fn two_clients_each_get_their_own_result() {
        // F13 : faux worker et deux clients simultanés. Chaque demande reçoit
        // son résultat, malgré les événements de l'autre et une notification
        // intercalée. (Pas d'événement d'état ni de connexion : ils
        // écriraient l'historique ou afficheraient une notification.)
        use momentum_client::tracker::wait_for;
        use momentum_core::device::DeviceInfo;
        use std::sync::Arc;
        use std::time::Duration;

        let shared = Arc::new(Mutex::new(connected()));
        let (commands, jobs) = mpsc::channel::<Job>();
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerEvent>();
        std::thread::spawn(move || {
            for job in jobs {
                let _ = worker_tx.send(WorkerEvent::Info(DeviceInfo::default()));
                let outcome = match job.command {
                    WorkerCommand::SetAntiWind(true) => Outcome::Applied,
                    _ => Outcome::Refused("refusé par le casque".into()),
                };
                let _ = worker_tx.send(WorkerEvent::Done {
                    id: job.id,
                    outcome,
                    details: None,
                });
            }
        });
        let (a_tx, a_rx) = mpsc::channel();
        let (b_tx, b_rx) = mpsc::channel();
        let pumped = Arc::clone(&shared);
        std::thread::spawn(move || {
            pump(worker_rx, &pumped, |event| {
                let _ = a_tx.send(event.clone());
                let _ = b_tx.send(event.clone());
            });
        });
        let submit_as = |command| {
            let mut s = shared.lock().unwrap();
            submit(command, 0, &mut s, &commands).unwrap()
        };
        let a = submit_as(Command::SetAntiWind(true));
        let b = submit_as(Command::SetAntiWind(false));
        let wait = Duration::from_secs(5);
        assert_eq!(
            wait_for(&a_rx, a, wait).map(|r| r.0),
            Some(Outcome::Applied)
        );
        assert!(matches!(
            wait_for(&b_rx, b, wait).map(|r| r.0),
            Some(Outcome::Refused(_))
        ));
    }
}
