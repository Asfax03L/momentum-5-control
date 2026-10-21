//! Captures vérifiées (PROTOCOL.md, ACCENTUM / Momentum 4) rejouées sans casque.

use std::time::Duration;

use momentum_core::client::{ClientError, GaiaClient, RawEvent};
use momentum_core::hex::from_hex;
use momentum_core::protocol::command::get;
use momentum_core::protocol::messages::{
    NoiseControlMode, describe, parse_battery, parse_codec, parse_model_name,
    parse_noise_control_modes,
};
use momentum_core::protocol::{Frame, FrameDecoder, PacketKind, VENDOR_SENNHEISER};
use momentum_core::transport::{MockTransport, TransportError};

const TIMEOUT: Duration = Duration::from_millis(100);

fn hex(s: &str) -> Vec<u8> {
    from_hex(s).expect("hex de test valide")
}

fn decode_one(s: &str) -> Frame {
    let bytes = hex(s);
    let (frame, used) = Frame::decode(&bytes).expect("trame valide");
    assert_eq!(used, bytes.len(), "toute la trame doit être consommée");
    frame
}

#[test]
fn battery_capture() {
    let req = Frame::sennheiser_request(get::BATTERY).encode().unwrap();
    assert_eq!(req, hex("FF 03 00 00 04 95 06 03"));

    let resp = decode_one("FF 03 00 01 04 95 07 03 2A");
    assert_eq!(resp.vendor, VENDOR_SENNHEISER);
    assert_eq!(resp.command, get::BATTERY.response());
    assert_eq!(resp.kind(), PacketKind::Response);
    assert_eq!(parse_battery(&resp.payload), Ok(42));
}

#[test]
fn noise_control_modes_capture() {
    let req = Frame::sennheiser_request(get::NOISE_CONTROL_MODES)
        .encode()
        .unwrap();
    assert_eq!(req, hex("FF 03 00 00 04 95 1A 01"));

    let resp = decode_one("FF 03 00 06 04 95 1B 01 01 02 02 00 03 01");
    assert_eq!(resp.command, get::NOISE_CONTROL_MODES.response());
    assert_eq!(
        parse_noise_control_modes(&resp.payload).unwrap(),
        vec![
            (NoiseControlMode::AntiWind, 2),
            (NoiseControlMode::Transparency, 0),
            (NoiseControlMode::Adaptive, 1),
        ]
    );
}

#[test]
fn model_name_request_and_response_shape() {
    let req = Frame::sennheiser_request(get::MODEL_NAME).encode().unwrap();
    assert_eq!(req, hex("FF 03 00 00 04 95 12 06"));

    // La capture exacte n'est pas fournie : on vérifie le format documenté
    // (ASCII terminé par \0) avec une chaîne fabriquée.
    let mut payload = b"TEST".to_vec();
    payload.push(0);
    let resp = Frame::new(VENDOR_SENNHEISER, get::MODEL_NAME.response(), payload);
    let (decoded, _) = Frame::decode(&resp.encode().unwrap()).unwrap();
    assert_eq!(parse_model_name(&decoded.payload).unwrap(), "TEST");
}

#[test]
fn codec_capture() {
    let req = Frame::sennheiser_request(get::CODEC).encode().unwrap();
    assert_eq!(req, hex("FF 03 00 00 04 95 08 00"));

    let resp = decode_one("FF 03 00 01 04 95 09 00 05");
    let codec = parse_codec(&resp.payload).unwrap();
    assert_eq!(codec.0, 5);
    assert_eq!(codec.name(), Some("aptX HD"));
}

#[test]
fn decoder_handles_byte_by_byte_stream() {
    let mut decoder = FrameDecoder::new();
    let bytes = hex("FF 03 00 01 04 95 07 03 2A");
    for (i, b) in bytes.iter().enumerate() {
        decoder.push(&[*b]);
        let next = decoder.next_frame();
        if i + 1 < bytes.len() {
            assert!(next.is_none(), "trame incomplète à l'octet {i}");
        } else {
            let frame = next.unwrap().unwrap();
            assert_eq!(parse_battery(&frame.payload), Ok(42));
        }
    }
    assert!(decoder.pending().is_empty());
}

#[test]
fn decoder_splits_glued_frames_and_skips_garbage() {
    let mut decoder = FrameDecoder::new();
    decoder.push(&hex(
        "00 11 FF 03 00 01 04 95 07 03 2A FF 03 00 01 04 95 09 00 05",
    ));
    assert!(matches!(
        decoder.next_frame(),
        Some(Err(momentum_core::protocol::ProtocolError::Garbage {
            skipped: 2
        }))
    ));
    let a = decoder.next_frame().unwrap().unwrap();
    let b = decoder.next_frame().unwrap().unwrap();
    assert_eq!(a.command, get::BATTERY.response());
    assert_eq!(b.command, get::CODEC.response());
    assert!(decoder.next_frame().is_none());
}

#[test]
fn client_gets_battery_through_mock() {
    let mock = MockTransport::new().expect_chunked(
        &hex("FF 03 00 00 04 95 06 03"),
        &[&hex("FF 03 00"), &hex("01 04 95 07 03 2A")],
    );
    let mut client = GaiaClient::new(mock);
    client.record_raw_log(true);
    let resp = client
        .request(&Frame::sennheiser_request(get::BATTERY), TIMEOUT)
        .unwrap();
    assert_eq!(parse_battery(&resp.payload), Ok(42));
    assert!(client.transport().is_exhausted());
    assert_eq!(client.transport().sent(), &[hex("FF 03 00 00 04 95 06 03")]);
    assert_eq!(
        client.take_raw_log(),
        vec![
            RawEvent::Sent(hex("FF 03 00 00 04 95 06 03")),
            RawEvent::Received(hex("FF 03 00")),
            RawEvent::Received(hex("01 04 95 07 03 2A")),
        ]
    );
}

#[test]
fn client_sets_notifications_aside() {
    // Une notification batterie (0x0683) arrive avant la réponse.
    let mock = MockTransport::new().expect(
        &hex("FF 03 00 00 04 95 08 00"),
        &hex("FF 03 00 01 04 95 06 83 29 FF 03 00 01 04 95 09 00 05"),
    );
    let mut client = GaiaClient::new(mock);
    let resp = client
        .request(&Frame::sennheiser_request(get::CODEC), TIMEOUT)
        .unwrap();
    assert_eq!(parse_codec(&resp.payload).unwrap().0, 5);
    let notifs = client.take_unsolicited();
    assert_eq!(notifs.len(), 1);
    assert_eq!(notifs[0].kind(), PacketKind::Notification);
    assert_eq!(notifs[0].command, get::BATTERY.notification());
}

#[test]
fn client_reports_device_error() {
    // Réponse d'erreur : id | 0x0180 = 0x0783.
    let mock = MockTransport::new().expect(
        &hex("FF 03 00 00 04 95 06 03"),
        &hex("FF 03 00 01 04 95 07 83 01"),
    );
    let mut client = GaiaClient::new(mock);
    let err = client
        .request(&Frame::sennheiser_request(get::BATTERY), TIMEOUT)
        .unwrap_err();
    assert!(matches!(err, ClientError::Device { payload, .. } if payload == vec![0x01]));
}

#[test]
fn client_times_out_without_answer() {
    let mock = MockTransport::new().expect(&hex("FF 03 00 00 04 95 06 03"), &[]);
    let mut client = GaiaClient::new(mock);
    let err = client
        .request(&Frame::sennheiser_request(get::BATTERY), TIMEOUT)
        .unwrap_err();
    assert!(matches!(
        err,
        ClientError::Transport(TransportError::Timeout(_))
    ));
}

#[test]
fn describe_is_readable() {
    let text = describe(&decode_one("FF 03 00 01 04 95 07 03 2A"));
    assert!(text.contains("batterie 42 %"), "{text}");
    let unknown = describe(&decode_one("FF 03 00 01 04 95 7F 7F 00"));
    assert!(unknown.contains("feature 63"), "{unknown}");
}
