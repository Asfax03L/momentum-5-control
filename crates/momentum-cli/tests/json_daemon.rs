//! Vrai binaire et faux service sur un bus privé : aucun accès au casque.
#![cfg(target_os = "linux")]

mod common;

use std::process::{Command, Output};
use std::sync::{Arc, Mutex};

use momentum_client::{ConnectionInfo, Event, Outcome, Snapshot};
use momentum_core::device::{DeviceInfo, EqState, HeadsetState, NoiseState};
use serde_json::{Value, json};
use zbus::object_server::SignalEmitter;

struct Data {
    snapshot: Snapshot,
    outcome: Outcome,
    commands: Vec<momentum_client::Command>,
    scene_state: HeadsetState,
}

struct Fake(Arc<Mutex<Data>>);

#[zbus::interface(name = "org.openheadset.Control1")]
impl Fake {
    fn snapshot(&self) -> String {
        serde_json::to_string(&self.0.lock().unwrap().snapshot).unwrap()
    }

    async fn submit(
        &self,
        command: &str,
        session: u64,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<u64> {
        let command: momentum_client::Command = serde_json::from_str(command).unwrap();
        assert_eq!(
            session,
            if command == momentum_client::Command::ReconnectHeadset {
                0
            } else {
                1
            }
        );
        let scene = matches!(command, momentum_client::Command::ApplyScene(_));
        let (outcome, scene_state) = {
            let mut data = self.0.lock().unwrap();
            data.commands.push(command);
            (data.outcome.clone(), data.scene_state)
        };
        let emit = |event| serde_json::to_string(&event).unwrap();
        // Un résultat étranger ne doit ni terminer la demande ni ajouter
        // un objet JSON à sa sortie.
        Self::event(
            &emitter,
            &emit(Event::Done {
                id: 99,
                outcome: Outcome::Refused("autre demande".into()),
                details: None,
            }),
        )
        .await?;
        Self::event(
            &emitter,
            &emit(Event::Done {
                id: 7,
                outcome,
                details: scene.then(|| "rapport de scène".into()),
            }),
        )
        .await?;
        if scene {
            // Le snapshot reste volontairement ancien : seul cet événement
            // porte la relecture publiée après Done, comme dans le worker.
            Self::event(&emitter, &emit(Event::State(scene_state))).await?;
        }
        Ok(7)
    }

    #[zbus(signal)]
    async fn event(emitter: &SignalEmitter<'_>, json: &str) -> zbus::Result<()>;
}

struct Bus {
    bus: common::PrivateBus,
    _connection: zbus::Connection,
    data: Arc<Mutex<Data>>,
}

impl Bus {
    fn new() -> Self {
        let bus = common::PrivateBus::start();
        let data = Arc::new(Mutex::new(Data {
            snapshot: Snapshot {
                session: 1,
                state: Some(HeadsetState {
                    battery: 70,
                    ..HeadsetState::default()
                }),
                equalizer: Some(EqState {
                    min_gain: -60,
                    max_gain: 60,
                    gains: vec![0; 8],
                    bass_boost: false,
                }),
                info: Some(DeviceInfo {
                    name: Some("Casque démo".into()),
                    ..DeviceInfo::default()
                }),
                connections: vec![ConnectionInfo {
                    id: "identifiant-interne".into(),
                    name: "Téléphone démo".into(),
                    connected: true,
                    ..ConnectionInfo::default()
                }],
                ..Snapshot::default()
            },
            outcome: Outcome::Applied,
            commands: Vec::new(),
            scene_state: HeadsetState {
                battery: 71,
                noise: NoiseState {
                    anc_enabled: true,
                    level: 42,
                    ..NoiseState::default()
                },
                ..HeadsetState::default()
            },
        }));
        let connection = zbus::block_on(async {
            zbus::connection::Builder::address(bus.address.as_str())
                .name(momentum_client::BUS_NAME)
                .serve_at(momentum_client::OBJECT_PATH, Fake(data.clone()))
                .build()
                .await
                .unwrap()
        });
        Self {
            bus,
            _connection: connection,
            data,
        }
    }

    fn ctl(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ctl"))
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", self.bus.address.as_str())
            .output()
            .unwrap()
    }

    fn ctl_in(&self, args: &[&str], root: &std::path::Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ctl"))
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", self.bus.address.as_str())
            .env("XDG_DATA_HOME", root)
            .output()
            .unwrap()
    }
}

fn one_object(output: &Output) -> Value {
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("un seul document JSON, sans message humain");
    assert!(value.is_object());
    assert_eq!(value["version"], 1);
    value
}

struct SceneFiles(std::path::PathBuf);
impl SceneFiles {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ctl-scene-capture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}
impl Drop for SceneFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn scene_enregistrer_uses_snapshot_and_never_submits_a_command() {
    let bus = Bus::new();
    let root = SceneFiles::new();
    let data_dir = root.0.join("momentum-ctl-win");
    std::fs::create_dir_all(&data_dir).unwrap();
    let original = "# Commentaire à garder\n[Autre]\nbruit=adaptatif\n";
    std::fs::write(data_dir.join("scenes.txt"), original).unwrap();
    {
        let mut data = bus.data.lock().unwrap();
        data.snapshot.read_only = Some("modèle non validé : lecture seule".into());
        data.snapshot.state.as_mut().unwrap().audio_mode =
            Some(momentum_core::device::AudioMode::Equalizer);
    }
    let output = bus.ctl_in(&["--json", "scene-enregistrer", "Scène démo"], &root.0);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(one_object(&output)["ok"], true);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("identifiant-interne"));
    let saved = std::fs::read_to_string(data_dir.join("scenes.txt")).unwrap();
    assert!(saved.starts_with(original));
    let (scenes, errors) = momentum_core::scenes::parse_scenes(&saved);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(scenes[1].equalizer.as_deref(), Some("Scène démo"));
    let presets = std::fs::read_to_string(data_dir.join("prereglages.txt")).unwrap();
    bus.data
        .lock()
        .unwrap()
        .snapshot
        .equalizer
        .as_mut()
        .unwrap()
        .gains[0] = 20;
    let output = bus.ctl_in(&["scene-enregistrer", "scène démo", "--json"], &root.0);
    assert!(!output.status.success());
    assert_eq!(one_object(&output)["ok"], false);
    assert_eq!(
        std::fs::read_to_string(data_dir.join("scenes.txt")).unwrap(),
        saved
    );
    assert_eq!(
        std::fs::read_to_string(data_dir.join("prereglages.txt")).unwrap(),
        presets
    );
    let output = bus.ctl_in(
        &["scene-enregistrer", "Scène démo", "--remplacer", "--json"],
        &root.0,
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(one_object(&output)["ok"], true);
    let text = std::fs::read_to_string(data_dir.join("prereglages.txt")).unwrap();
    assert_eq!(
        momentum_core::presets::parse_user_presets(&text)[0].gains[0],
        20
    );
    assert!(bus.data.lock().unwrap().commands.is_empty());
}

#[test]
fn missing_snapshot_or_service_does_not_create_files_or_connect_directly() {
    let bus = Bus::new();
    let root = SceneFiles::new();
    bus.data.lock().unwrap().snapshot.state = None;
    let output = bus.ctl_in(&["--json", "scene-enregistrer", "Scène démo"], &root.0);
    assert!(!output.status.success());
    assert_eq!(one_object(&output)["ok"], false);
    assert!(!root.0.join("momentum-ctl-win").exists());
    assert!(bus.data.lock().unwrap().commands.is_empty());
    zbus::block_on(bus._connection.release_name(momentum_client::BUS_NAME)).unwrap();
    let output = bus.ctl_in(&["--json", "scene-enregistrer", "Scène démo"], &root.0);
    assert!(!output.status.success());
    assert_eq!(one_object(&output)["ok"], false);
    assert!(
        one_object(&output)["erreur"]
            .as_str()
            .unwrap()
            .contains("service requis")
    );
    assert!(!root.0.join("momentum-ctl-win").exists());
}

#[test]
fn service_requis_refuse_sans_service_et_ne_cherche_pas_le_casque() {
    let bus = Bus::new();
    let output = bus.ctl(&["--json", "--service-requis", "etat"]);
    assert!(output.status.success());
    assert_eq!(one_object(&output)["connecte"], true);
    zbus::block_on(bus._connection.release_name(momentum_client::BUS_NAME)).unwrap();
    for args in [
        vec!["--json", "--service-requis", "etat"],
        vec!["--json", "--service-requis", "mode", "arret"],
    ] {
        let output = bus.ctl(&args);
        assert!(!output.status.success(), "{args:?}");
        let error = one_object(&output)["erreur"].as_str().unwrap().to_string();
        assert!(error.contains("aucune connexion directe"), "{error}");
        // Aucune recherche Bluetooth : pas de message de connexion directe.
        assert!(!String::from_utf8_lossy(&output.stderr).contains("Connecté à"));
    }
}

#[test]
fn etat_reste_identique_avant_et_apres_la_sous_commande() {
    let bus = Bus::new();
    let expected = format!(
        "{}\n",
        momentum_core::json::state_json(bus.data.lock().unwrap().snapshot.state.as_ref())
    );
    for args in [["etat", "--json"], ["--json", "etat"]] {
        let output = bus.ctl(&args);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(output.stdout, expected.as_bytes());
        assert!(String::from_utf8_lossy(&output.stderr).contains("par le service"));
    }
    assert!(bus.data.lock().unwrap().commands.is_empty());
}

#[test]
fn lectures_connexions_sans_identifiants_et_valeurs_non_lues_nulles() {
    let bus = Bus::new();
    let output = bus.ctl(&["connexions", "--json"]);
    let value = one_object(&output);
    assert!(output.status.success());
    assert_eq!(
        value["connexions"],
        json!([{"nom": "Téléphone démo", "connecte": true, "ce_pc": false}])
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("identifiant-interne"));
    {
        let mut data = bus.data.lock().unwrap();
        data.snapshot.info = None;
        data.snapshot.equalizer = None;
        data.snapshot.connections.clear();
    }
    for (command, key) in [
        ("eq", "egaliseur"),
        ("nom", "nom"),
        ("connexions", "connexions"),
    ] {
        let output = bus.ctl(&["--json", command]);
        assert!(output.status.success(), "{output:?}");
        assert!(one_object(&output)[key].is_null());
    }
}

#[test]
fn tous_les_reglages_envoient_une_seule_demande_et_un_seul_objet() {
    let bus = Bus::new();
    let commands: &[&[&str]] = &[
        &["mode", "adaptatif"],
        &["niveau", "42"],
        &["anti-vent", "on"],
        &["spatial", "off"],
        &["suivi-tete", "off"],
        &["multipoint", "on"],
        &["bande", "1", "-3.5"],
        &["plat"],
        &["basses", "on"],
        &["voix", "off"],
        &["profil", "off"],
        &["tactile", "on"],
        &["protection-batterie", "on"],
        &["smart-pause", "on"],
        &["reponse-auto", "off"],
        &["appels-confort", "off"],
        &["extinction", "30"],
        &["invites", "tonalites"],
        &["volume-invites", "77"],
        &["nom", "Casque démo"],
        &["connecter", "1"],
        &["deconnecter", "1"],
        &["exporter-profil", "profil-simule.json"],
        &["scene", "Travail"],
    ];
    for (index, command) in commands.iter().enumerate() {
        let mut args = vec!["--json"];
        args.extend_from_slice(command);
        let output = bus.ctl(&args);
        assert!(output.status.success(), "{command:?} : {output:?}");
        assert_eq!(one_object(&output)["ok"], true, "{command:?}");
        assert_eq!(bus.data.lock().unwrap().commands.len(), index + 1);
    }
}

#[test]
fn echecs_du_service_conservent_les_codes_et_produisent_du_json() {
    let bus = Bus::new();
    for (outcome, code) in [
        (Outcome::Refused("refus simulé".into()), 1),
        (Outcome::Cancelled("remplacé".into()), 1),
        (Outcome::NotApplied("relu autrement".into()), 2),
        (Outcome::Unknown("lien perdu".into()), 3),
    ] {
        bus.data.lock().unwrap().outcome = outcome;
        let output = bus.ctl(&["niveau", "42", "--json"]);
        assert_eq!(output.status.code(), Some(code));
        let value = one_object(&output);
        assert_eq!(value["ok"], false);
        assert!(value["erreur"].is_string());
    }
}

#[test]
fn scene_attend_la_relecture_et_acquittement_ne_fabrique_pas_un_etat() {
    let bus = Bus::new();
    let output = bus.ctl(&["--json", "scene", "Travail"]);
    let value = one_object(&output);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(value["etat"]["batterie"], 71);
    assert_eq!(value["etat"]["bruit"]["niveau"], 42);
    bus.data.lock().unwrap().outcome = Outcome::Acknowledged;
    let output = bus.ctl(&["--json", "connecter", "1"]);
    let value = one_object(&output);
    assert!(output.status.success());
    assert_eq!(value["resultat"], "accepté par le casque");
    assert!(value["etat"]["connexions"].is_null());
}

#[test]
fn arguments_invalides_produisent_un_objet_et_aucune_demande() {
    let bus = Bus::new();
    for args in [
        vec!["niveau", "101", "--json"],
        vec!["--json", "commande-inconnue"],
    ] {
        let output = bus.ctl(&args);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(one_object(&output)["ok"], false);
    }
    assert!(bus.data.lock().unwrap().commands.is_empty());
}

#[test]
fn reconnect_submits_while_disconnected_and_waits_for_its_own_done() {
    let bus = Bus::new();
    let message = "Demande système simulée ; audio non confirmé.";
    {
        let mut data = bus.data.lock().unwrap();
        data.snapshot.state = None;
        data.snapshot.device_name = None;
        data.outcome = Outcome::SystemRequested(message.into());
    }
    for args in [vec!["reconnecter", "--json"], vec!["reconnecter"]] {
        let output = bus.ctl(&args);
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(message));
        if args.contains(&"--json") {
            assert_eq!(one_object(&output)["ok"], true);
        }
    }
    // Une seule demande par appel de `ctl reconnecter`.
    assert_eq!(bus.data.lock().unwrap().commands.len(), 2);
    bus.data.lock().unwrap().outcome = Outcome::Refused("aucun casque mémorisé".into());
    let output = bus.ctl(&["--json", "reconnecter"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(one_object(&output)["ok"], false);
    assert!(String::from_utf8_lossy(&output.stdout).contains("aucun casque mémorisé"));
    assert!(
        bus.data
            .lock()
            .unwrap()
            .commands
            .iter()
            .all(|command| *command == momentum_client::Command::ReconnectHeadset)
    );
    zbus::block_on(bus._connection.release_name(momentum_client::BUS_NAME)).unwrap();
    let output = bus.ctl(&["--json", "reconnecter"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        one_object(&output)["erreur"]
            .as_str()
            .unwrap()
            .contains("service requis")
    );
}

#[test]
fn essais_simulation_sends_nothing_and_a_wrong_readback_stops_after_a_restore_attempt() {
    let bus = Bus::new();
    // Simulation : plan seulement.
    let output = bus.ctl(&["--json", "essais", "--simulation"]);
    assert!(output.status.success(), "{output:?}");
    let plan = one_object(&output);
    assert_eq!(plan["simulation"], true);
    assert_eq!(plan["ok"], true);
    assert!(plan["rapport"].as_str().unwrap().contains("Simulation"));
    assert!(!plan["rapport"].as_str().unwrap().contains("Casque démo"));
    assert!(bus.data.lock().unwrap().commands.is_empty());
    // Essai hors de la liste : refusé avant tout envoi.
    let output = bus.ctl(&["--json", "essais", "--liste", "nom"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(bus.data.lock().unwrap().commands.is_empty());
    // Le faux service acquitte sans rien changer : la relecture montre
    // l'ancienne valeur, la suite s'arrête après une tentative de remise.
    let output = bus.ctl(&["--json", "essais", "--liste", "anti-vent,tactile"]);
    assert_eq!(output.status.code(), Some(1));
    let result = one_object(&output);
    assert_eq!(result["ok"], false);
    assert_eq!(result["essais"][1]["statut"], "non_lance");
    let commands = bus.data.lock().unwrap().commands.clone();
    assert_eq!(
        commands,
        [
            momentum_client::Command::SetAntiWind(true),
            momentum_client::Command::SetAntiWind(false),
        ]
    );
}
