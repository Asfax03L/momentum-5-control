//! Régressions issues de l'audit indépendant du 09/10/2026 (code au commit
//! 40efdd6 : cinq échecs, un succès). Tests hors ligne sur MockTransport ;
//! ils passent tous depuis les corrections de cette branche.
use momentum_core::protocol::{command::get, messages::parse_switch};
use momentum_core::{
    client::GaiaClient,
    device::{EqState, Headset},
    protocol::{CommandId, Frame, VENDOR_SENNHEISER},
    transport::MockTransport,
};
use std::time::Duration;
fn frame(cmd: u16, p: &[u8]) -> Vec<u8> {
    Frame::new(VENDOR_SENNHEISER, CommandId::new(cmd), p.to_vec())
        .encode()
        .unwrap()
}

#[test]
fn stale_buffered_response_must_not_satisfy_next_same_id_request() {
    let req = frame(0x1002, &[0]);
    let req2 = frame(0x1002, &[1]);
    let mut old = frame(0x1102, &[10]);
    old.extend(frame(0x1102, &[10]));
    let mock = MockTransport::new()
        .expect(&req, &old)
        .expect(&req2, &frame(0x1102, &[20]));
    let mut c = GaiaClient::new(mock);
    c.request(
        &Frame::new(VENDOR_SENNHEISER, get::EQ_BAND, vec![0]),
        Duration::from_millis(50),
    )
    .unwrap();
    let second = c
        .request(
            &Frame::new(VENDOR_SENNHEISER, get::EQ_BAND, vec![1]),
            Duration::from_millis(50),
        )
        .unwrap();
    assert_eq!(
        second.payload,
        vec![20],
        "a buffered response for the previous band was accepted"
    );
}
#[test]
fn invalid_bool_must_not_be_silently_off() {
    assert!(parse_switch(get::SMART_PAUSE, &[2]).is_err());
}
#[test]
fn invalid_eq_notification_must_not_replace_valid_state() {
    let mut eq = EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![0; 8],
        bass_boost: false,
    };
    let before = eq.clone();
    eq.apply_notification(&Frame::new(
        VENDOR_SENNHEISER,
        CommandId::new(0x1082),
        vec![127; 8],
    ));
    assert_eq!(eq, before);
}
#[test]
fn whole_eq_curve_must_be_validated_before_first_write() {
    let mock = MockTransport::new().expect(&frame(0x1001, &[0, 5]), &frame(0x1101, &[]));
    let mut h = Headset::new(mock, Duration::from_millis(50));
    let mut eq = EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![0; 2],
        bass_boost: false,
    };
    assert!(h.set_eq_gains(&mut eq, &[5, 100]).is_err());
    assert!(
        h.client().transport().sent().is_empty(),
        "first band changed before later invalid gain rejected"
    );
}
#[test]
fn observed_android_version4_frame_should_decode() {
    // Exact frame from recovered Android HCI, not a request to send.
    let raw = [0xFF, 0x04, 0x00, 0x01, 0x04, 0x95, 0x20, 0x81, 0x02];
    assert!(Frame::decode(&raw).is_ok());
}
#[test]
fn version3_encoder_must_not_treat_flags_as_high_length_byte() {
    let f = Frame::new(VENDOR_SENNHEISER, get::BATTERY, vec![0; 256]);
    assert!(
        f.encode().is_err(),
        "256-byte v3 payload writes 01 into the flags byte"
    );
}
