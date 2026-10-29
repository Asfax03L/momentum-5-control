//! Scénarios d'affichage synthétiques : aucun essai matériel ni échange GAIA.
use momentum_core::device::{
    AudioMode, ChargeReading, ChargerStatus, DeviceInfo, HeadsetState, OnHeadState,
};
use momentum_core::home_presentation::{
    Availability, availability, headset_summary, home_availability,
};
use momentum_core::viewmodel;

#[test]
fn full_battery_does_not_imply_finished_charging() {
    let mut state = HeadsetState {
        battery: 100,
        charge_state: Some(ChargeReading {
            primary_status: ChargerStatus::Charging,
            secondary_raw: Some(0),
        }),
        ..HeadsetState::default()
    };
    assert_eq!(headset_summary(Some(&state)).charge, "En charge");
    state.charge_state.as_mut().unwrap().secondary_raw = Some(255);
    assert_eq!(headset_summary(Some(&state)).charge, "En charge");
    state.charge_state.as_mut().unwrap().primary_status = ChargerStatus::Unknown(2);
    assert_eq!(
        headset_summary(Some(&state)).charge,
        "Charge inconnue (0x02)"
    );
    state.charge_state = None;
    assert_eq!(
        headset_summary(Some(&state)).charge,
        "État de charge non disponible"
    );
}

#[test]
fn wearing_is_a_measurement_independent_of_smart_pause() {
    let mut state = HeadsetState {
        smart_pause: Some(false),
        on_head_state: Some(OnHeadState::Worn),
        ..HeadsetState::default()
    };
    assert_eq!(headset_summary(Some(&state)).wearing, "Porté");
    state.smart_pause = Some(true);
    state.on_head_state = Some(OnHeadState::NotWorn);
    assert_eq!(headset_summary(Some(&state)).wearing, "Non porté");
    state.on_head_state = Some(OnHeadState::Unknown(7));
    assert_eq!(headset_summary(Some(&state)).wearing, "Port inconnu (0x07)");
}

#[test]
fn absent_session_cannot_display_previous_measurements() {
    let state = HeadsetState {
        codec: Some(1),
        on_head_state: Some(OnHeadState::Worn),
        charge_state: Some(ChargeReading {
            primary_status: ChargerStatus::Charging,
            secondary_raw: None,
        }),
        ..HeadsetState::default()
    };
    let previous = headset_summary(Some(&state));
    let current = headset_summary(None);
    assert_ne!(previous, current);
    assert_eq!(current.charge, "État de charge non disponible");
    assert_eq!(current.wearing, "État de port non disponible");
    assert_eq!(current.stream, "Flux audio non disponible");
    assert_eq!(
        home_availability(None, Some(&DeviceInfo::default()), true).noise,
        Availability::Disconnected
    );
}

#[test]
fn unread_is_not_reported_as_off_or_as_a_proven_refusal() {
    assert_eq!(availability(true, true, false), Availability::Unread);
    assert_eq!(availability(true, false, false), Availability::Unread);
    assert_eq!(availability(true, false, true), Availability::ReadOnly);
    assert_eq!(availability(false, true, true), Availability::Disconnected);
    let state = HeadsetState::default();
    let a = home_availability(Some(&state), None, true);
    assert_eq!(a.touch, Availability::Unread);
    assert_eq!(a.multipoint, Availability::Unread);
    assert!(a.noise.enabled()); // Les réglages indépendants restent accessibles.
    assert!(!Availability::Unread.reason().contains("refus"));
}

#[test]
fn slider_requires_custom_mode_and_write_permission() {
    let mut state = HeadsetState::default();
    state.noise.anc_enabled = true;
    state.noise.adaptive = true;
    assert_eq!(
        home_availability(Some(&state), None, true).level,
        Availability::CustomRequired
    );
    state.noise.adaptive = false;
    assert!(home_availability(Some(&state), None, true).level.enabled());
    assert_eq!(
        home_availability(Some(&state), None, false).level,
        Availability::ReadOnly
    );
}

#[test]
fn new_explanations_preserve_existing_spatial_and_touch_guards() {
    for writable in [false, true] {
        for spatial in [None, Some(false), Some(true)] {
            for tracking in [None, Some(false), Some(true)] {
                for mode in [AudioMode::Equalizer, AudioMode::Personalization] {
                    let state = HeadsetState {
                        spatial_audio: spatial,
                        head_tracking: tracking,
                        audio_mode: Some(mode),
                        touch_controls: Some(false),
                        ..HeadsetState::default()
                    };
                    let a = home_availability(Some(&state), None, writable);
                    let ro = (!writable).then_some("lecture seule");
                    let v = viewmodel::main_view(None, Some(&state), &[], None, ro);
                    assert_eq!(a.spatial.enabled(), v.controls.spatial_enabled);
                    assert_eq!(a.tracking.enabled(), v.controls.head_tracking_enabled);
                    assert_eq!(a.touch.enabled(), v.controls.touch_enabled);
                }
            }
        }
    }
}

#[test]
fn dependency_reasons_do_not_hide_read_only_or_absent_values() {
    let mut state = HeadsetState {
        spatial_audio: Some(false),
        head_tracking: Some(false),
        audio_mode: Some(AudioMode::Personalization),
        ..HeadsetState::default()
    };
    let a = home_availability(Some(&state), None, true);
    assert_eq!(a.spatial, Availability::ProfileSelected);
    assert_eq!(a.tracking, Availability::SpatialRequired);
    state.head_tracking = None;
    assert_eq!(
        home_availability(Some(&state), None, true).tracking,
        Availability::Unread
    );
    assert_eq!(
        home_availability(Some(&state), None, false).spatial,
        Availability::ReadOnly
    );
}

#[test]
fn unknown_codec_is_not_replaced_with_demo_aac() {
    let mut state = HeadsetState::default();
    assert_eq!(
        headset_summary(Some(&state)).stream,
        "Flux audio non disponible"
    );
    state.codec = Some(255);
    assert_eq!(headset_summary(Some(&state)).stream, "Pas de lecture");
    state.codec = Some(1);
    assert!(headset_summary(Some(&state)).stream.starts_with("AAC"));
}
