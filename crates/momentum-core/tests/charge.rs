//! État de charge déclaré (`0602` / `0702` / `0682`), sans connexion
//! Bluetooth. Fixtures de `contrat-implementation-charge-2026-10-28.json`
//! (Codex) : les payloads `00 00`, `01 00`, `00`, `01` viennent des captures
//! de Smart Control Plus du 28/10/2026 (records 31362, 56016, 31222, 31901) ;
//! les autres cas sont synthétiques et ne sont jamais envoyés au casque.

use momentum_core::device::{ChargeReading, ChargerStatus, Headset, HeadsetState};
use momentum_core::hex::from_hex;
use momentum_core::protocol::command::{REGISTER_NOTIFICATION, get};
use momentum_core::protocol::messages::{parse_charge_notification, parse_charge_response};
use momentum_core::protocol::{Frame, VENDOR_QUALCOMM, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;
use std::time::Duration;

const T: Duration = Duration::from_millis(50);

fn h(hex: &str) -> Vec<u8> {
    from_hex(hex).unwrap()
}

fn reading(primary_status: ChargerStatus, secondary_raw: Option<u8>) -> ChargeReading {
    ChargeReading {
        primary_status,
        secondary_raw,
    }
}

#[test]
fn observed_payloads_decode_as_captured() {
    // Capture : 0702 [00 00] (31362), 0702 [01 00] (56016).
    assert_eq!(
        parse_charge_response(&[0, 0]).unwrap(),
        reading(ChargerStatus::Disconnected, Some(0))
    );
    assert_eq!(
        parse_charge_response(&[1, 0]).unwrap(),
        reading(ChargerStatus::Charging, Some(0))
    );
    // Capture : 0682 [00] (31222), 0682 [01] (31901).
    assert_eq!(
        parse_charge_notification(&[0]).unwrap(),
        ChargerStatus::Disconnected
    );
    assert_eq!(
        parse_charge_notification(&[1]).unwrap(),
        ChargerStatus::Charging
    );
}

#[test]
fn unobserved_and_unknown_values_are_kept_raw() {
    // Synthétique : réponse d'un octet (format prévu par l'app, non observé).
    assert_eq!(
        parse_charge_response(&[1]).unwrap(),
        reading(ChargerStatus::Charging, None)
    );
    // `02` (fin de charge dans le code de l'app) reste inconnu sur le M5.
    assert_eq!(
        parse_charge_response(&[2, 0]).unwrap(),
        reading(ChargerStatus::Unknown(2), Some(0))
    );
    assert_eq!(
        parse_charge_notification(&[0xFF]).unwrap(),
        ChargerStatus::Unknown(0xFF)
    );
    // Second octet futur : gardé brut, jamais interprété.
    assert_eq!(
        parse_charge_response(&[1, 0xFF]).unwrap(),
        reading(ChargerStatus::Charging, Some(0xFF))
    );
}

#[test]
fn invalid_structures_are_errors() {
    for payload in [&[][..], &[1, 0, 0]] {
        assert!(parse_charge_response(payload).is_err(), "{payload:?}");
    }
    for payload in [&[][..], &[1, 0]] {
        assert!(parse_charge_notification(payload).is_err(), "{payload:?}");
    }
}

#[test]
fn a_full_battery_never_means_charge_complete() {
    // Capture : 0703 [64] (100 %, record 55934) puis 0702 [01 00] (56016).
    let mock = MockTransport::new().expect(
        &h("FF 03 00 00 04 95 06 02"),
        &h("FF 03 00 02 04 95 07 02 01 00"),
    );
    let mut headset = Headset::new(mock, T);
    let state = HeadsetState {
        battery: 100,
        charge_state: Some(headset.charge_state().unwrap()),
        ..HeadsetState::default()
    };
    assert_eq!(state.battery, 100);
    assert_eq!(
        state.charge_state,
        Some(reading(ChargerStatus::Charging, Some(0)))
    );
}

#[test]
fn routing_only_accepts_the_charge_notification() {
    let mut state = HeadsetState::default();
    let notification = |payload| {
        Frame::new(
            VENDOR_SENNHEISER,
            get::CHARGER_STATE.notification(),
            payload,
        )
    };
    for frame in [
        Frame::new(VENDOR_QUALCOMM, get::CHARGER_STATE.notification(), vec![1]),
        Frame::new(VENDOR_SENNHEISER, get::CHARGER_STATE.response(), vec![1]),
        notification(vec![]),
        notification(vec![1, 0]),
    ] {
        let before = state;
        assert!(!state.apply_notification(&frame));
        assert_eq!(state, before);
    }
    // La batterie (0683) n'est pas l'état de charge.
    assert!(state.apply_notification(&Frame::new(
        VENDOR_SENNHEISER,
        get::BATTERY.notification(),
        vec![100]
    )));
    assert_eq!(state.charge_state, None);
    assert!(state.apply_notification(&notification(vec![1])));
    assert_eq!(
        state.charge_state,
        Some(reading(ChargerStatus::Charging, None))
    );
}

#[test]
fn a_one_byte_notification_forgets_the_old_secondary_field() {
    let mut state = HeadsetState {
        charge_state: Some(reading(ChargerStatus::Disconnected, Some(0))),
        ..HeadsetState::default()
    };
    let frame = Frame::new(
        VENDOR_SENNHEISER,
        get::CHARGER_STATE.notification(),
        vec![1],
    );
    assert!(state.apply_notification(&frame));
    assert_eq!(
        state.charge_state,
        Some(reading(ChargerStatus::Charging, None))
    );
    // Notification invalide : l'état précédent reste tel quel.
    let before = state;
    let bad = Frame::new(VENDOR_SENNHEISER, get::CHARGER_STATE.notification(), vec![]);
    assert!(!state.apply_notification(&bad));
    assert_eq!(state, before);
}

#[test]
fn older_charge_notification_does_not_override_the_response() {
    // Notification 01 avant la réponse 00 00, puis batterie : seule la
    // notification de charge antérieure est écartée (entrelacement synthétique).
    let mock = MockTransport::new().expect_chunked(
        &h("FF 03 00 00 04 95 06 02"),
        &[
            &h("FF 03 00 01 04 95 06 82 01"),
            &h("FF 03 00 01 04 95 06 83 47"),
            &h("FF 03 00 02 04 95 07 02 00 00"),
        ],
    );
    let mut headset = Headset::new(mock, T);
    assert_eq!(
        headset.charge_state().unwrap(),
        reading(ChargerStatus::Disconnected, Some(0))
    );
    let notifications = headset.poll_notifications(T).unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].command, get::BATTERY.notification());
    assert_eq!(headset.client().transport().sent().len(), 1);
}

#[test]
fn notifications_after_the_response_survive_in_same_or_split_chunk() {
    let response = h("FF 03 00 02 04 95 07 02 00 00");
    let notification = h("FF 03 00 01 04 95 06 82 01");
    let mut combined = response.clone();
    combined.extend_from_slice(&notification);
    for chunks in [
        vec![combined.as_slice()],
        vec![response.as_slice(), notification.as_slice()],
        vec![&combined[..12], &combined[12..]],
    ] {
        let mock = MockTransport::new().expect_chunked(&h("FF 03 00 00 04 95 06 02"), &chunks);
        let mut headset = Headset::new(mock, T);
        let mut state = HeadsetState {
            charge_state: Some(headset.charge_state().unwrap()),
            ..HeadsetState::default()
        };
        let notifications = headset.poll_notifications(T).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(state.apply_notification(&notifications[0]));
        assert_eq!(
            state.charge_state,
            Some(reading(ChargerStatus::Charging, None))
        );
    }
}

#[test]
fn a_failed_read_keeps_the_queued_charge_notification() {
    // Réponse malformée (vide), puis refus GAIA : la notification valide reçue
    // avant reste disponible.
    for reply in ["FF 03 00 00 04 95 07 02", "FF 03 00 01 04 95 07 82 01"] {
        let mock = MockTransport::new().expect_chunked(
            &h("FF 03 00 00 04 95 06 02"),
            &[&h("FF 03 00 01 04 95 06 82 01"), &h(reply)],
        );
        let mut headset = Headset::new(mock, T);
        assert!(headset.charge_state().is_err());
        assert!(!headset.client().is_desynchronized());
        let notifications = headset.poll_notifications(T).unwrap();
        assert_eq!(notifications.len(), 1, "{reply}");
        let mut state = HeadsetState::default();
        assert!(state.apply_notification(&notifications[0]));
        assert_eq!(
            state.charge_state,
            Some(reading(ChargerStatus::Charging, None))
        );
    }
}

#[test]
fn a_timeout_desynchronizes_and_forbids_a_second_send() {
    let mock = MockTransport::new().expect_chunked(&h("FF 03 00 00 04 95 06 02"), &[]);
    let mut headset = Headset::new(mock, T);
    assert!(headset.charge_state().is_err());
    assert!(headset.client().is_desynchronized());
    assert!(headset.charge_state().is_err());
    assert_eq!(headset.client().transport().sent().len(), 1);
}

#[test]
fn json_absence_unknown_and_zero_are_distinct() {
    // Ancien service : champ absent, accepté.
    let json = serde_json::to_value(HeadsetState::default()).unwrap();
    assert!(json.get("charge_state").is_none());
    let old: HeadsetState = serde_json::from_value(json).unwrap();
    assert_eq!(old.charge_state, None);
    for value in [
        reading(ChargerStatus::Disconnected, Some(0)),
        reading(ChargerStatus::Unknown(2), Some(0)),
        reading(ChargerStatus::Charging, None),
    ] {
        let state = HeadsetState {
            charge_state: Some(value),
            ..HeadsetState::default()
        };
        let back: HeadsetState =
            serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
        assert_eq!(back.charge_state, Some(value));
    }
}

#[test]
fn no_extra_battery_feature_subscription() {
    // Les abonnements restent ceux d'avant (feature 3 une seule fois) plus la
    // feature 2 du port ; aucun 0007 [03] supplémentaire pour la charge.
    let mut mock = MockTransport::new();
    for feature in [3u8, 13, 27, 8, 4, 11, 2] {
        let request = Frame::new(VENDOR_SENNHEISER, REGISTER_NOTIFICATION, vec![feature]);
        let response = Frame::new(VENDOR_SENNHEISER, REGISTER_NOTIFICATION.response(), vec![]);
        mock = mock.expect(&request.encode().unwrap(), &response.encode().unwrap());
    }
    let mut headset = Headset::new(mock, T);
    headset.register_notifications().unwrap();
    assert!(headset.client().transport().is_exhausted());
}
