//! Exemples écrits à la main d'après les clés publiques de PipeWire ;
//! aucune capture, aucun nom ou identifiant d'un appareil réel.
use momentum_core::pipewire_audio::{self, AudioReadError, Profile};
use momentum_core::transport::choice::fingerprint;
use momentum_core::viewmodel::{self, AudioInputs};
use serde_json::{Value, json};

const A2DP: &str = include_str!("fixtures/pipewire-a2dp.json");
const HFP: &str = include_str!("fixtures/pipewire-hfp.json");
const ABSENT: &str = include_str!("fixtures/pipewire-absent.json");
fn parse(text: &str) -> Result<pipewire_audio::PipeWireAudio, AudioReadError> {
    pipewire_audio::parse_dump(text, Some(&fingerprint("02:00:00:00:00:01")))
}

#[test]
fn a2dp_aptx_uses_address_and_device_reference_not_name_or_microphone() {
    let audio = parse(A2DP).unwrap();
    assert_eq!(audio.outputs.len(), 1);
    assert_eq!(audio.outputs[0].profile, Some(Profile::A2dp));
    assert_eq!(audio.outputs[0].codec, Some("aptX"));
    assert_eq!(audio.outputs[0].default, Some(true));
    assert_eq!(audio.default_headset, Some(true));
    let exposed = format!("{audio:?}");
    for private in [
        "02:00:",
        "Casque démo",
        "sortie-casque",
        "sortie-autre",
        &fingerprint("02:00:00:00:00:01"),
    ] {
        assert!(!exposed.contains(private));
    }
}

#[test]
fn call_profile_and_effective_default_are_distinct_from_configured_default() {
    let audio = parse(HFP).unwrap();
    assert_eq!(audio.outputs[0].profile, Some(Profile::Hfp));
    assert_eq!(audio.outputs[0].codec, Some("mSBC"));
    assert_eq!(audio.outputs[0].default, Some(false));
    assert_eq!(audio.default_headset, Some(false));
    let view =
        viewmodel::pipewire_audio_view(&AudioInputs::default(), Some(&Ok(audio)), Some("12:00:00"));
    assert!(
        view.findings
            .iter()
            .any(|(p, text)| *p && text.contains("Profil appel"))
    );
    assert!(
        view.findings
            .iter()
            .any(|(p, text)| *p && text.contains("pas la sortie audio par défaut"))
    );
    assert!(!format!("{view:?}").contains("Windows"));
}

#[test]
fn missing_headset_and_old_service_never_select_another_bluetooth_device() {
    let audio = parse(ABSENT).unwrap();
    assert!(audio.outputs.is_empty());
    assert_eq!(audio.default_headset, Some(false));
    assert_eq!(
        pipewire_audio::parse_dump(A2DP, None),
        Err(AudioReadError::NoIdentity)
    );
    assert_eq!(
        pipewire_audio::parse_dump(A2DP, Some("")),
        Err(AudioReadError::NoIdentity)
    );
    let view = viewmodel::pipewire_audio_view(
        &AudioInputs::default(),
        Some(&Err(AudioReadError::Missing)),
        None,
    );
    assert!(
        view.windows
            .iter()
            .any(|row| row.value.contains("pw-dump est absent"))
    );
    assert!(!format!("{view:?}").contains("Windows"));
}

#[test]
fn absent_or_ambiguous_metadata_and_unknown_properties_stay_unknown() {
    let mut value: Value = serde_json::from_str(A2DP).unwrap();
    value
        .as_array_mut()
        .unwrap()
        .retain(|o| o["type"] != "PipeWire:Interface:Metadata");
    value[2]["info"]["props"]["api.bluez5.codec"] = json!("texte privé fictif");
    value[2]["info"]["props"]["api.bluez5.profile"] = json!("profil inconnu fictif");
    let audio = parse(&value.to_string()).unwrap();
    assert_eq!(audio.default_headset, None);
    assert_eq!(audio.outputs[0].default, None);
    assert_eq!(audio.outputs[0].profile, None);
    assert_eq!(audio.outputs[0].codec, None);
    assert!(!format!("{audio:?}").contains("privé"));
    let mut value: Value = serde_json::from_str(A2DP).unwrap();
    let mut other = value[4].clone();
    other["metadata"][0]["value"]["name"] = json!("sortie-autre-casque");
    value.as_array_mut().unwrap().push(other);
    assert_eq!(parse(&value.to_string()).unwrap().default_headset, None);
}

#[test]
fn malformed_oversized_or_ambiguous_graphs_cannot_invent_a_route() {
    assert_eq!(parse("{corrompu}"), Err(AudioReadError::Invalid));
    assert_eq!(parse("{}"), Err(AudioReadError::Invalid));
    assert_eq!(
        parse(&" ".repeat(pipewire_audio::MAX_DUMP_BYTES + 1)),
        Err(AudioReadError::TooLarge)
    );
    let mut value: Value = serde_json::from_str(A2DP).unwrap();
    let duplicate = value[1].clone();
    value.as_array_mut().unwrap().push(duplicate);
    assert!(parse(&value.to_string()).unwrap().outputs.is_empty());
    let mut value: Value = serde_json::from_str(A2DP).unwrap();
    value[1]["info"]["props"]["api.bluez5.address"] = json!("nom non utilisable comme adresse");
    assert!(parse(&value.to_string()).unwrap().outputs.is_empty());
}
