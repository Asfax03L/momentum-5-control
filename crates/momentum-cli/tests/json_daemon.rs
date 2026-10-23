//! Vrai binaire et faux service sur un bus privé : aucun accès au casque.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Output, Stdio};
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
        assert_eq!(session, 1);
        let command: momentum_client::Command = serde_json::from_str(command).unwrap();
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
    daemon: Child,
    address: String,
    _connection: zbus::Connection,
    data: Arc<Mutex<Data>>,
}

impl Bus {
    fn new() -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("bus de test privé");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
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
            zbus::connection::Builder::address(address.trim())
                .name(momentum_client::BUS_NAME)
                .serve_at(momentum_client::OBJECT_PATH, Fake(data.clone()))
                .build()
                .await
                .unwrap()
        });
        Self {
            daemon,
            address,
            _connection: connection,
            data,
        }
    }

    fn ctl(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ctl"))
            .args(args)
            .env("DBUS_SESSION_BUS_ADDRESS", self.address.trim())
            .output()
            .unwrap()
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

fn one_object(output: &Output) -> Value {
    let value: Value =
        serde_json::from_slice(&output.stdout).expect("un seul document JSON, sans message humain");
    assert!(value.is_object());
    assert_eq!(value["version"], 1);
    value
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
