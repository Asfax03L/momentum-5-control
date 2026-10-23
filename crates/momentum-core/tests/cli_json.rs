//! Contrats JSON publics : lectures absentes, échappement et compatibilité.

use momentum_core::device::{AudioMode, EqState, HeadsetState, NoiseState, PromptMode};
use momentum_core::json::{self, ConnectionSummary, Readback};
use serde_json::{Value, json};

fn parse(text: String) -> Value {
    serde_json::from_str(&text).unwrap()
}

#[test]
fn etat_historique_identique_octet_pour_octet() {
    assert_eq!(json::state_json(None), r#"{"version":1,"connecte":false}"#);
    let state = HeadsetState {
        battery: 70,
        noise: NoiseState {
            anc_enabled: true,
            adaptive: false,
            anti_wind: true,
            level: 0,
        },
        audio_mode: Some(AudioMode::Personalization),
        spatial_audio: Some(false),
        // Les nouveaux champs du résultat des réglages ne doivent pas
        // apparaître dans le contrat historique d'etat.
        smart_pause: Some(true),
        ..HeadsetState::default()
    };
    assert_eq!(
        json::state_json(Some(&state)),
        concat!(
            r#"{"version":1,"connecte":true,"batterie":70,"bruit":{"mode":"personnalise","niveau":0,"anti_vent":true},"#,
            r#""audio_spatial":false,"suivi_tete":null,"mode_audio":"profil","profil":null,"commandes_tactiles":null,"codec":null,"frequence_hz":null,"flux":null}"#
        )
    );
}

#[test]
fn lectures_absentes_et_liste_vide_sont_distinctes() {
    assert_eq!(
        parse(json::equalizer_json(None)),
        json!({"version": 1, "egaliseur": null})
    );
    assert_eq!(
        parse(json::name_json(None)),
        json!({"version": 1, "nom": null})
    );
    assert_eq!(
        parse(json::connections_json(None)),
        json!({"version": 1, "connexions": null})
    );
    assert_eq!(
        parse(json::connections_json(Some(&[]))),
        json!({"version": 1, "connexions": []})
    );
    let eq = EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![-35, 0, 30],
        bass_boost: false,
    };
    assert_eq!(
        parse(json::equalizer_json(Some(&eq)))["egaliseur"],
        json!({
            "min_dixiemes_db": -60, "max_dixiemes_db": 60, "gains_dixiemes_db": [-35, 0, 30], "basses": false,
        })
    );
}

#[test]
fn chaines_echappees_et_connexions_sans_identifiant() {
    let name = "Téléphone \"démo\"\n\\fin\u{1}";
    let list = [ConnectionSummary {
        name: name.into(),
        connected: false,
        this_device: true,
    }];
    let row = &parse(json::connections_json(Some(&list)))["connexions"][0];
    assert_eq!(row, &json!({"nom": name, "connecte": false, "ce_pc": true}));
    assert_eq!(parse(json::name_json(Some(name)))["nom"], name);
    let scenes = parse(json::scenes_json(&[name], &["ligne 2 : erreur".into()]));
    assert_eq!(scenes["scenes"], json!([name]));
    assert_eq!(scenes["avertissements"], json!(["ligne 2 : erreur"]));
}

#[test]
fn echec_conserve_la_relecture_partielle_et_masque_les_adresses() {
    let readback = Readback {
        noise: Some(NoiseState {
            anc_enabled: true,
            level: 41,
            ..NoiseState::default()
        }),
        ..Readback::default()
    };
    let value = parse(json::result_json(
        &readback,
        Some("erreur AA:BB:CC:DD:EE:FF\nrefus"),
        Some("non appliqué"),
        None,
    ));
    assert_eq!(value["ok"], false);
    assert_eq!(value["etat"]["bruit"]["niveau"], 41);
    assert!(value["etat"]["batterie"].is_null());
    assert!(value["etat"]["egaliseur"].is_null());
    assert!(!value.to_string().contains("AA:BB:CC:DD:EE:FF"));
    let absent = Readback::default().value();
    assert!(absent.as_object().unwrap().values().all(Value::is_null));
}

#[test]
fn reglages_optionnels_conservent_false_et_zero() {
    let readback = Readback {
        state: Some(HeadsetState {
            smart_pause: Some(false),
            auto_answer: Some(true),
            comfort_call: Some(false),
            battery_protection: Some(true),
            auto_off_seconds: Some(0),
            prompt_mode: Some(PromptMode::TonesOnly),
            ..HeadsetState::default()
        }),
        multipoint: Some(false),
        prompt_volume: Some(0),
        ..Readback::default()
    };
    let value = parse(json::result_json(&readback, None, None, None));
    assert_eq!(value["ok"], true);
    assert_eq!(value["etat"]["smart_pause"], false);
    assert_eq!(value["etat"]["reponse_auto"], true);
    assert_eq!(value["etat"]["appels_confort"], false);
    assert_eq!(value["etat"]["protection_batterie"], true);
    assert_eq!(value["etat"]["extinction_secondes"], 0);
    assert_eq!(value["etat"]["invites"], "tonalites");
    assert_eq!(value["etat"]["multipoint"], false);
    assert_eq!(value["etat"]["volume_invites"], 77);
    assert!(value.get("erreur").is_none());
}
