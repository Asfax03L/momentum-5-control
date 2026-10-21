//! Captures réelles d'un MOMENTUM 5 (modèle « M5AEBT Black », firmware 6.21.5),
//! relevées avec `probe --all-gets` le 08/10/2026, sous Windows 11, en RFCOMM
//! sur le service SDP « GAIA » (UUID A2129FF3-081B-4C45-8AFE-469D9C4842EC).

use momentum_core::hex::from_hex;
use momentum_core::protocol::Frame;
use momentum_core::protocol::command::get;
use momentum_core::protocol::messages::{
    NoiseControlMode, describe, parse_battery, parse_codec, parse_firmware_version,
    parse_model_name, parse_noise_control_modes, parse_switch,
};

/// (requête, réponse) telles que vues sur le fil.
const EXCHANGES: &[(&str, &str)] = &[
    ("FF 03 00 00 04 95 06 03", "FF 03 00 01 04 95 07 03 64"),
    (
        "FF 03 00 00 04 95 12 06",
        "FF 03 00 0D 04 95 13 06 4D 35 41 45 42 54 20 42 6C 61 63 6B 00",
    ),
    (
        "FF 03 00 00 04 95 12 01",
        "FF 03 00 06 04 95 13 01 00 06 00 15 00 05",
    ),
    ("FF 03 00 00 04 95 08 00", "FF 03 00 01 04 95 09 00 01"),
    (
        "FF 03 00 00 04 95 1A 01",
        "FF 03 00 06 04 95 1B 01 01 01 03 00 02 00",
    ),
    ("FF 03 00 00 04 95 1A 05", "FF 03 00 01 04 95 1B 05 01"),
    ("FF 03 00 00 04 95 18 05", "FF 03 00 01 04 95 19 05 00"),
    ("FF 03 00 00 04 95 1A 03", "FF 03 00 01 04 95 1B 03 00"),
];

fn response(request_hex: &str) -> Frame {
    let (_, resp) = EXCHANGES
        .iter()
        .find(|(req, _)| *req == request_hex)
        .expect("capture présente");
    let bytes = from_hex(resp).unwrap();
    let (frame, used) = Frame::decode(&bytes).unwrap();
    assert_eq!(used, bytes.len());
    frame
}

#[test]
fn every_request_matches_our_encoder_and_gets_its_response_id() {
    let commands = [
        get::BATTERY,
        get::MODEL_NAME,
        get::FIRMWARE_VERSION,
        get::CODEC,
        get::NOISE_CONTROL_MODES,
        get::ANC_ENABLED,
        get::TRANSPARENCY_ENABLED,
        get::TRANSPARENCY_LEVEL,
    ];
    for ((req_hex, _), command) in EXCHANGES.iter().zip(commands) {
        let encoded = Frame::sennheiser_request(command).encode().unwrap();
        assert_eq!(encoded, from_hex(req_hex).unwrap(), "{command}");
        assert_eq!(response(req_hex).command, command.response(), "{command}");
    }
}

#[test]
fn m5_values() {
    assert_eq!(
        parse_battery(&response("FF 03 00 00 04 95 06 03").payload),
        Ok(100)
    );
    assert_eq!(
        parse_model_name(&response("FF 03 00 00 04 95 12 06").payload).unwrap(),
        "M5AEBT Black"
    );
    assert_eq!(
        parse_firmware_version(&response("FF 03 00 00 04 95 12 01").payload),
        Ok((6, 21, 5))
    );
    // Codec 1 = AAC d'après la table de l'APK (Windows 11 sait faire de l'AAC).
    let codec = parse_codec(&response("FF 03 00 00 04 95 08 00").payload).unwrap();
    assert_eq!((codec.0, codec.name()), (1, Some("AAC")));
    // Même encodage en paires que le M4, mais ordre différent.
    assert_eq!(
        parse_noise_control_modes(&response("FF 03 00 00 04 95 1A 01").payload).unwrap(),
        vec![
            (NoiseControlMode::AntiWind, 1),
            (NoiseControlMode::Adaptive, 0),
            (NoiseControlMode::Transparency, 0),
        ]
    );
    assert_eq!(
        parse_switch(
            get::ANC_ENABLED,
            &response("FF 03 00 00 04 95 1A 05").payload
        ),
        Ok(true)
    );
    assert_eq!(
        parse_switch(
            get::TRANSPARENCY_ENABLED,
            &response("FF 03 00 00 04 95 18 05").payload
        ),
        Ok(false)
    );
}

#[test]
fn describe_m5_responses() {
    let anc = describe(&response("FF 03 00 00 04 95 1A 05"));
    assert!(anc.ends_with("ANC activé"), "{anc}");
    let level = describe(&response("FF 03 00 00 04 95 1A 03"));
    assert!(level.contains("niveau de transparence 0"), "{level}");
}

/// Inventaire `probe --deep` du même M5 (réglages par défaut + EQ perso).
/// Les noms des appareils appairés et le numéro de série sont volontairement
/// absents de ces tests.
mod deep {
    use super::*;
    use momentum_core::protocol::VENDOR_SENNHEISER;
    use momentum_core::protocol::messages::{parse_core_features, parse_eq_config, parse_eq_gain};

    fn decode(hex: &str) -> Frame {
        let bytes = from_hex(hex).unwrap();
        Frame::decode(&bytes).unwrap().0
    }

    #[test]
    fn equalizer_has_eight_bands_of_plus_minus_6_db() {
        let cfg = decode("FF 03 00 05 04 95 11 00 08 C4 3C 00 00");
        let cfg = parse_eq_config(&cfg.payload).unwrap();
        assert_eq!((cfg.bands, cfg.min_gain, cfg.max_gain), (8, -60, 60));
    }

    #[test]
    fn equalizer_band_answer_is_a_single_gain_byte() {
        // Requête « bande 0 » → réponse [DD] = −3,5 dB (pas d'écho de la bande).
        let req = Frame::new(VENDOR_SENNHEISER, get::EQ_BAND, vec![0]);
        assert_eq!(
            req.encode().unwrap(),
            from_hex("FF 03 00 01 04 95 10 02 00").unwrap()
        );
        let gains: Vec<i8> = ["DD", "F6", "0A", "06", "F6", "FB", "FB", "1E"]
            .iter()
            .map(|g| {
                let f = decode(&format!("FF 03 00 01 04 95 11 02 {g}"));
                parse_eq_gain(&f.payload).unwrap()
            })
            .collect();
        assert_eq!(gains, vec![-35, -10, 10, 6, -10, -5, -5, 30]);
    }

    #[test]
    fn qualcomm_core_feature_list() {
        let f = decode("FF 03 00 0D 00 1D 01 01 00 0D 01 0C 01 06 02 04 02 07 01 00 05");
        let (more, features) = parse_core_features(&f.payload).unwrap();
        assert_eq!(more, 0);
        assert_eq!(
            features,
            vec![(13, 1), (12, 1), (6, 2), (4, 2), (7, 1), (0, 5)]
        );
    }

    #[test]
    fn settings_are_readable() {
        let cases = [
            ("FF 03 00 01 04 95 09 0D 01", "Smart Pause activée"),
            ("FF 03 00 01 04 95 05 01 01", "détection de port activée"),
            ("FF 03 00 01 04 95 09 02 02", "tonalités et voix"),
            ("FF 03 00 01 04 95 09 07 02", "(français)"),
            ("FF 03 00 01 04 95 05 06 01", "Qualité audio avancée"),
            (
                "FF 03 00 01 04 95 05 06 00",
                "haute résolution 24 bits/96 kHz activée",
            ),
            (
                "FF 03 00 01 04 95 05 08 01",
                "sans perte 16 bits/44,1 kHz activé",
            ),
            ("FF 03 00 03 04 95 07 01 00 00 00", "= jamais"),
            ("FF 03 00 03 04 95 07 01 00 07 08", "= 30 min (1800 s)"),
            ("FF 03 00 02 04 95 15 00 00 03", "3 appareil(s)"),
        ];
        for (hex, expected) in cases {
            let text = describe(&decode(hex));
            assert!(text.contains(expected), "{text}");
        }
    }
}

/// Relevé croisé du 08/10/2026 : chaque mode choisi dans Smart Control Plus
/// (via ADB), puis lu sur le PC. Réponses brutes de 1A05, 1A01 et 1A03.
mod noise_cross_capture {
    use super::*;
    use momentum_core::protocol::messages::{M5NoiseMode, m5_noise_mode};

    fn mode(anc: &str, pairs: &str, level: &str) -> M5NoiseMode {
        let frame = |hex: &str| {
            let bytes = from_hex(hex).unwrap();
            Frame::decode(&bytes).unwrap().0
        };
        let anc = parse_switch(get::ANC_ENABLED, &frame(anc).payload).unwrap();
        let pairs = parse_noise_control_modes(&frame(pairs).payload).unwrap();
        let adaptive = pairs
            .iter()
            .any(|(m, s)| *m == NoiseControlMode::Adaptive && *s == 1);
        let level = frame(level).payload[0];
        m5_noise_mode(anc, adaptive, level)
    }

    const ANC_ON: &str = "FF 03 00 01 04 95 1B 05 01";
    const ANC_OFF: &str = "FF 03 00 01 04 95 1B 05 00";
    const PAIRS_FIXED: &str = "FF 03 00 06 04 95 1B 01 01 01 03 00 02 00";
    const PAIRS_ADAPTIVE: &str = "FF 03 00 06 04 95 1B 01 01 01 03 01 02 00";

    #[test]
    fn app_modes_map_to_gaia_state() {
        let lvl = |v: u8| format!("FF 03 00 01 04 95 1B 03 {v:02X}");
        assert_eq!(mode(ANC_OFF, PAIRS_FIXED, &lvl(0)), M5NoiseMode::Off);
        assert_eq!(mode(ANC_ON, PAIRS_ADAPTIVE, &lvl(0)), M5NoiseMode::Adaptive);
        for v in [0, 25, 50, 75, 100] {
            assert_eq!(
                mode(ANC_ON, PAIRS_FIXED, &lvl(v)),
                M5NoiseMode::Custom { level: v }
            );
        }
    }

    #[test]
    fn anti_wind_is_the_first_pair() {
        let off = from_hex("FF 03 00 06 04 95 1B 01 01 00 03 00 02 00").unwrap();
        let pairs = parse_noise_control_modes(&Frame::decode(&off).unwrap().0.payload).unwrap();
        assert_eq!(pairs[0], (NoiseControlMode::AntiWind, 0));
    }
}

/// Capture HCI de Smart Control Plus (08/10/2026) : le casque répond à l'app
/// avec des trames « FF 04 » ; on n'en garde ici que les payloads, sans
/// adresses ni noms d'appareils.
mod app_capture {
    use momentum_core::protocol::command::get;
    use momentum_core::protocol::messages::{describe, parse_band_table};
    use momentum_core::protocol::{Frame, VENDOR_SENNHEISER};

    #[test]
    fn eq_center_frequencies_of_the_m5() {
        let payload = momentum_core::hex::from_hex(
            "00 00 5A 01 01 45 02 05 DC 03 09 C4 04 11 94 05 1F 40 06 32 C8 07 3E 80",
        )
        .unwrap();
        let freqs: Vec<u16> = parse_band_table(&payload)
            .unwrap()
            .into_iter()
            .map(|(_, hz)| hz)
            .collect();
        assert_eq!(freqs, [90, 325, 1500, 2500, 4500, 8000, 13000, 16000]);
    }

    #[test]
    fn sound_profile_blocks_are_big_endian_floats() {
        use momentum_core::protocol::messages::f32_block;
        // Trames 2002/2006 envoyées par l'app après le test (09/10/2026).
        let block1 = momentum_core::hex::from_hex(
            "C2 85 8D 6C C2 4F 1A D8 C2 99 56 8E C2 76 AD 1D C2 AD 1F B1 C2 8F 1F B1",
        )
        .unwrap();
        let (values, rest) = f32_block(&block1);
        assert_eq!(values.len(), 6);
        assert!((values[0] - -66.776).abs() < 0.01);
        assert!((values[1] - values[0] - 15.0).abs() < 0.01);
        assert!(rest.is_empty());

        let block3 =
            momentum_core::hex::from_hex("BF 66 66 66 BF 66 66 66 BF 66 66 66 05 05").unwrap();
        let (values, rest) = f32_block(&block3);
        assert_eq!(values, vec![-0.9; 3]);
        assert_eq!(rest, &[5, 5]);
    }

    #[test]
    fn profile_usage_notification() {
        // Capture HCI du 09/10/2026 : sélection du profil, égaliseur, Dolby Atmos.
        for (hex, expected) in [
            ("FF 04 00 01 04 95 20 8A 01", "sélectionné"),
            ("FF 04 00 01 04 95 20 8A 00", "non utilisé"),
            ("FF 04 00 01 04 95 20 8A 02", "Dolby Atmos activé"),
        ] {
            let frame =
                momentum_core::protocol::Frame::decode(&momentum_core::hex::from_hex(hex).unwrap())
                    .unwrap()
                    .0;
            let text = momentum_core::protocol::messages::describe(&frame);
            assert!(text.contains(expected), "{hex} → {text}");
        }
    }

    #[test]
    fn spatial_audio_state() {
        let resp = Frame::new(VENDOR_SENNHEISER, get::SPATIAL_AUDIO.response(), vec![1]);
        assert!(describe(&resp).ends_with("audio spatial activé"));
        assert_eq!(get::SPATIAL_AUDIO.feature(), 27);
    }
}
