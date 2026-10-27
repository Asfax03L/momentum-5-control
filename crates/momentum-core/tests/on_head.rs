//! Lecture et notifications du port, sans connexion Bluetooth.
//! Payloads observés les 25 et 27/10/2026 ; provenance dans docs/MOMENTUM5.md.
//! Les cas inconnus, malformés et refusés sont des entrées synthétiques.

use momentum_core::device::{Headset, HeadsetState, OnHeadState};
use momentum_core::hex::from_hex;
use momentum_core::protocol::command::{REGISTER_NOTIFICATION, feature, get};
use momentum_core::protocol::messages::{describe, parse_on_head_state};
use momentum_core::protocol::{Frame, VENDOR_QUALCOMM, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;
use std::time::Duration;

const T: Duration = Duration::from_millis(50);

fn h(hex: &str) -> Vec<u8> {
    from_hex(hex).unwrap()
}

#[test]
fn physical_state_is_not_the_sensor_switch() {
    assert_eq!(parse_on_head_state(&[2]).unwrap(), OnHeadState::NotWorn);
    assert_eq!(parse_on_head_state(&[3]).unwrap(), OnHeadState::Worn);
    for value in [0, 1, 4, 255] {
        assert_eq!(
            parse_on_head_state(&[value]).unwrap(),
            OnHeadState::Unknown(value)
        );
    }
    for payload in [&[][..], &[2, 3], &[3, 0, 0]] {
        assert!(parse_on_head_state(payload).is_err());
    }
    let response = Frame::new(VENDOR_SENNHEISER, get::ON_HEAD_STATE.response(), vec![3]);
    assert!(describe(&response).contains("casque porté"));
    let response = Frame::new(VENDOR_SENNHEISER, get::ON_HEAD_STATE.response(), vec![1]);
    assert!(describe(&response).contains("état de port inconnu (01)"));
}

#[test]
fn older_wearing_notification_does_not_override_the_get_response() {
    // GET vide / réponse 02 : records 22342/22345. Notification 03 : record 23003.
    // L'entrelacement ci-dessous est synthétique et éprouve le routage du client.
    let mock = MockTransport::new().expect_chunked(
        &h("FF 03 00 00 04 95 04 02"),
        &[
            &h("FF 03 00 01 04 95 04 82 03"),
            &h("FF 03 00 01 04 95 06 83 47"),
            &h("FF 03 00 01 04 95 05 02 02"),
        ],
    );
    // La lecture reste disponible sans autoriser les écritures.
    let mut headset = Headset::new(mock, T);
    assert_eq!(headset.on_head_state().unwrap(), OnHeadState::NotWorn);
    let notifications = headset.poll_notifications(T).unwrap();
    // L'état de port antérieur est remplacé par la réponse ; la batterie reste.
    assert_eq!(notifications.len(), 1);
    let mut state = HeadsetState {
        on_head_state: Some(OnHeadState::NotWorn),
        ..HeadsetState::default()
    };
    assert!(state.apply_notification(&notifications[0]));
    assert_eq!(state.battery, 71);
    assert_eq!(state.on_head_state, Some(OnHeadState::NotWorn));
    assert!(headset.client().transport().is_exhausted());
    assert_eq!(headset.client().transport().sent().len(), 1);
}

#[test]
fn notifications_after_response_survive_in_same_or_split_chunk() {
    let response = h("FF 03 00 01 04 95 05 02 02");
    let notification = h("FF 03 00 01 04 95 04 82 03");
    let mut combined = response.clone();
    combined.extend_from_slice(&notification);
    // Un suffixe complet ou fragmenté ne doit pas être supprimé avec le passé.
    for chunks in [
        vec![combined.as_slice()],
        vec![response.as_slice(), notification.as_slice()],
        vec![&combined[..12], &combined[12..]],
    ] {
        let mock = MockTransport::new().expect_chunked(&h("FF 03 00 00 04 95 04 02"), &chunks);
        let mut headset = Headset::new(mock, T);
        let mut state = HeadsetState {
            on_head_state: Some(headset.on_head_state().unwrap()),
            ..HeadsetState::default()
        };
        assert_eq!(state.on_head_state, Some(OnHeadState::NotWorn));
        let notifications = headset.poll_notifications(T).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(state.apply_notification(&notifications[0]));
        assert_eq!(state.on_head_state, Some(OnHeadState::Worn));
    }
}

#[test]
fn malformed_read_preserves_queued_wearing_notifications() {
    let mock = MockTransport::new().expect_chunked(
        &h("FF 03 00 00 04 95 04 02"),
        &[
            &h("FF 03 00 01 04 95 04 82 03"),
            &h("FF 03 00 00 04 95 05 02"),
        ],
    );
    let mut headset = Headset::new(mock, T);
    assert!(headset.on_head_state().is_err());
    assert!(!headset.client().is_desynchronized());
    let notifications = headset.poll_notifications(T).unwrap();
    assert_eq!(notifications.len(), 1);
    let mut state = HeadsetState::default();
    assert!(state.apply_notification(&notifications[0]));
    assert_eq!(state.on_head_state, Some(OnHeadState::Worn));
}

#[test]
fn malformed_or_unrelated_notifications_preserve_the_last_state() {
    let mut state = HeadsetState::default();
    let notification = |payload| {
        Frame::new(
            VENDOR_SENNHEISER,
            get::ON_HEAD_STATE.notification(),
            payload,
        )
    };
    assert!(state.apply_notification(&notification(vec![3])));
    assert!(!state.apply_notification(&notification(vec![3])));
    assert!(state.apply_notification(&notification(vec![2])));
    for frame in [
        notification(vec![]),
        notification(vec![2, 3]),
        Frame::new(VENDOR_QUALCOMM, get::ON_HEAD_STATE.notification(), vec![3]),
        Frame::new(VENDOR_SENNHEISER, get::ON_HEAD_STATE.response(), vec![3]),
        Frame::new(
            VENDOR_SENNHEISER,
            get::ON_HEAD_DETECTION.notification(),
            vec![1],
        ),
    ] {
        let before = state;
        assert!(!state.apply_notification(&frame));
        assert_eq!(state, before);
    }
    assert!(state.apply_notification(&notification(vec![1])));
    assert_eq!(state.on_head_state, Some(OnHeadState::Unknown(1)));
    assert!(state.apply_notification(&notification(vec![0])));
    assert_eq!(state.on_head_state, Some(OnHeadState::Unknown(0)));
}

fn subscriptions() -> MockTransport {
    let mut mock = MockTransport::new();
    // Les six abonnements historiques restent obligatoires, dans le même ordre.
    for feature in [3, 13, 27, 8, 4, 11] {
        let request = Frame::new(VENDOR_SENNHEISER, REGISTER_NOTIFICATION, vec![feature]);
        let response = Frame::new(VENDOR_SENNHEISER, REGISTER_NOTIFICATION.response(), vec![]);
        mock = mock.expect(&request.encode().unwrap(), &response.encode().unwrap());
    }
    mock
}

#[test]
fn optional_subscription_is_renewed_and_a_refusal_keeps_the_session_usable() {
    // Deux connexions distinctes doivent chacune demander la feature 02.
    for reply in ["FF 03 00 00 04 95 01 07", "FF 03 00 01 04 95 01 87 01"] {
        let mock = subscriptions()
            .expect(&h("FF 03 00 01 04 95 00 07 02"), &h(reply))
            .expect(
                &h("FF 03 00 00 04 95 04 02"),
                &h("FF 03 00 01 04 95 05 02 02"),
            );
        let mut headset = Headset::new(mock, T);
        headset.register_notifications().unwrap();
        assert!(!headset.client().is_desynchronized());
        assert_eq!(headset.on_head_state().unwrap(), OnHeadState::NotWorn);
        assert_eq!(feature::ON_HEAD, 2);
        assert!(headset.client().transport().is_exhausted());
    }
}

#[test]
fn optional_subscription_timeout_still_requires_a_new_connection() {
    let mock = subscriptions().expect_chunked(&h("FF 03 00 01 04 95 00 07 02"), &[]);
    let mut headset = Headset::new(mock, T);
    assert!(headset.register_notifications().is_err());
    assert!(headset.client().is_desynchronized());
    let sent = headset.client().transport().sent().len();
    assert!(headset.on_head_state().is_err());
    assert_eq!(headset.client().transport().sent().len(), sent);
}

#[test]
fn optional_read_does_not_hide_a_timeout() {
    let mock = MockTransport::new().expect_chunked(&h("FF 03 00 00 04 95 04 02"), &[]);
    let mut headset = Headset::new(mock, T);
    assert!(headset.on_head_state().is_err());
    assert!(headset.client().is_desynchronized());
}
