#![no_main]

use libfuzzer_sys::fuzz_target;
use momentum_core::protocol::{Frame, HEADER_LEN, MAX_PAYLOAD_LEN, ProtocolError};

fuzz_target!(|data: &[u8]| {
    match Frame::decode(data) {
        Ok((frame, used)) => {
            assert!((HEADER_LEN..=data.len()).contains(&used));
            assert_eq!(used, HEADER_LEN + frame.payload.len());
            assert!(frame.payload.len() <= MAX_PAYLOAD_LEN);
            // La version 4 est normalisée en version 3 par encode ; les
            // champs typés et le nombre d'octets doivent rester identiques.
            let encoded = frame.encode();
            assert!(encoded.is_ok());
            if let Ok(encoded) = encoded {
                assert_eq!(Frame::decode(&encoded), Ok((frame, used)));
            }
        }
        Err(ProtocolError::Incomplete { needed }) => assert!(needed > 0),
        Err(_) => {}
    }
});
