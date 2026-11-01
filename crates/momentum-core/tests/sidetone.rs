//! Sidetone dans l'état et les interfaces, sans connexion Bluetooth : les
//! notifications `0886`, les niveaux observés (0, 1, 2) et la valeur brute d'un
//! niveau non observé.

use momentum_core::device::{DeviceError, Headset, HeadsetState, SIDETONE_LEVELS};
use momentum_core::json::{self, Readback};
use momentum_core::protocol::command::get;
use momentum_core::protocol::{Frame, VENDOR_QUALCOMM, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;
use momentum_core::viewmodel;
use std::time::Duration;

fn notification(payload: Vec<u8>) -> Frame {
    Frame::new(VENDOR_SENNHEISER, get::SIDETONE.notification(), payload)
}

#[test]
fn only_the_sidetone_notification_changes_the_state() {
    let mut state = HeadsetState::default();
    for frame in [
        Frame::new(VENDOR_QUALCOMM, get::SIDETONE.notification(), vec![1]),
        Frame::new(VENDOR_SENNHEISER, get::SIDETONE.response(), vec![1]),
        notification(vec![]),
        notification(vec![1, 0]),
    ] {
        assert!(!state.apply_notification(&frame));
        assert_eq!(state.sidetone, None);
    }
    assert!(state.apply_notification(&notification(vec![2])));
    assert_eq!(state.sidetone, Some(2));
}

#[test]
fn unobserved_levels_are_refused_before_any_send() {
    for level in [3, 4, 5, 255] {
        let mut headset = Headset::new(MockTransport::new(), Duration::from_millis(20));
        assert!(matches!(
            headset.set_sidetone(level),
            Err(DeviceError::InvalidSidetone(l)) if l == level
        ));
        assert!(headset.client().transport().sent().is_empty());
    }
}

#[test]
fn the_view_maps_each_observed_level_to_its_label() {
    assert_eq!(SIDETONE_LEVELS.len(), viewmodel::SIDETONE_LABELS.len());
    for (index, level) in SIDETONE_LEVELS.into_iter().enumerate() {
        let state = HeadsetState {
            sidetone: Some(level),
            ..HeadsetState::default()
        };
        let view = viewmodel::settings_view(Some(&state), None, true);
        assert_eq!(view.sidetone, index as i32);
        assert!(view.sidetone_enabled);
    }
}

#[test]
fn the_view_says_when_the_level_is_unread_or_unknown_or_locked() {
    let unread = viewmodel::settings_view(Some(&HeadsetState::default()), None, true);
    assert_eq!(unread.sidetone, -1);
    assert!(!unread.sidetone_enabled);

    // Niveau non observé : aucun choix affiché comme actif, 0 à 2 restent permis.
    let state = HeadsetState {
        sidetone: Some(3),
        ..HeadsetState::default()
    };
    let unknown = viewmodel::settings_view(Some(&state), None, true);
    assert_eq!(unknown.sidetone, -1);
    assert!(unknown.sidetone_enabled);

    // Micrologiciel non validé : lecture seule.
    let state = HeadsetState {
        sidetone: Some(1),
        ..HeadsetState::default()
    };
    let locked = viewmodel::settings_view(Some(&state), None, false);
    assert_eq!(locked.sidetone, 1);
    assert!(!locked.sidetone_enabled);

    let absent = viewmodel::settings_view(None, None, true);
    assert_eq!(absent.sidetone, -1);
    assert!(!absent.sidetone_enabled);
}

#[test]
fn an_older_service_without_the_field_is_still_understood() {
    let value = serde_json::to_value(HeadsetState::default()).unwrap();
    assert!(value.get("sidetone").is_none());
    let old: HeadsetState = serde_json::from_value(value).unwrap();
    assert_eq!(old.sidetone, None);
    let state = HeadsetState {
        sidetone: Some(0),
        ..HeadsetState::default()
    };
    let back: HeadsetState = serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    assert_eq!(back.sidetone, Some(0));
}

#[test]
fn the_result_json_reports_the_read_value_but_not_the_historic_state() {
    let state = HeadsetState {
        sidetone: Some(2),
        ..HeadsetState::default()
    };
    let readback = Readback {
        state: Some(state),
        ..Readback::default()
    };
    let result: serde_json::Value =
        serde_json::from_str(&json::result_json(&readback, None, None, None)).unwrap();
    assert_eq!(result["etat"]["sidetone"], 2);
    // Le contrat historique d'`etat` garde ses champs, dans le même ordre.
    assert!(!json::state_json(Some(&state)).contains("sidetone"));
}
