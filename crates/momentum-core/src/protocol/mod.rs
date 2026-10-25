//! Encodage et décodage des trames GAIA, sans aucune I/O.
//!
//! Format observé sur le M5 (champs multi-octets en big-endian) :
//!
//! ```text
//! octet  0     : 0xFF            début de trame
//! octet  1     : version         03 en émission ; 03 ou 04 en réception
//! octet  2     : flags           toujours 00 dans les captures
//! octet  3     : longueur du payload (u8)
//! octets 4-5   : vendor id (0x0495 Sennheiser, 0x001D Qualcomm)
//! octets 6-7   : command id (voir `CommandId`)
//! octets 8..   : payload
//! ```
//!
//! La longueur ne compte que le payload : `FF 03 00 01 04 95 07 03 2A` porte
//! 1 octet utile (`2A`). Smart Control Plus émet en version 3 et reçoit des
//! réponses en version 3 puis 4 (après la négociation `001D:000D`), toujours
//! avec des flags nuls. Les formes à flags non nuls (somme de contrôle,
//! longueur étendue du framing Qualcomm) n'ont jamais été observées sur le
//! M5 : elles sont rejetées plutôt que devinées.

pub mod command;
pub mod decoder;
pub mod messages;

pub use command::{CommandId, PacketKind, VENDOR_QUALCOMM, VENDOR_SENNHEISER};
pub use decoder::FrameDecoder;

use crate::i18n::trf;
use thiserror::Error;

pub const START_OF_FRAME: u8 = 0xFF;
/// Version utilisée en émission (celle de Smart Control Plus).
pub const PROTOCOL_VERSION: u8 = 0x03;
/// Versions acceptées en réception (observées dans les captures Android).
pub const SUPPORTED_VERSIONS: [u8; 2] = [0x03, 0x04];
pub const HEADER_LEN: usize = 8;
/// Longueur maximale d'un payload : 254 octets, limite conservatrice du
/// framing Qualcomm à longueur sur un octet (aucun payload M5 connu ne
/// dépasse 24 octets).
pub const MAX_PAYLOAD_LEN: usize = 254;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("{}", trf("trame incomplète : {} octet(s) manquant(s)", &[&.needed]))]
    Incomplete { needed: usize },
    #[error("{}", trf("octet de début invalide : {} (attendu 0xFF)", &[&format!("{:#04X}", .0)]))]
    BadStartByte(u8),
    #[error("{}", trf("version de trame non prise en charge : {} (03 et 04 sont connues)", &[&format!("{:#04X}", .0)]))]
    UnsupportedVersion(u8),
    #[error("{}", trf("flags de trame non pris en charge : {} (seul 0x00 est connu)", &[&format!("{:#04X}", .0)]))]
    UnsupportedFlags(u8),
    #[error("{}", trf("payload trop long : {} octets (max {})", &[&.0, &MAX_PAYLOAD_LEN]))]
    PayloadTooLong(usize),
    #[error("{}", trf("{} octet(s) parasite(s) ignoré(s) avant une trame", &[&.skipped]))]
    Garbage { skipped: usize },
    #[error("{}", trf("payload inattendu pour {} : {}", &[&.command, &.reason]))]
    BadPayload {
        command: CommandId,
        reason: &'static str,
    },
}

/// Une trame GAIA décodée.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub vendor: u16,
    pub command: CommandId,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(vendor: u16, command: CommandId, payload: Vec<u8>) -> Self {
        Self {
            vendor,
            command,
            payload,
        }
    }

    /// Requête Sennheiser sans payload (cas de tous les GET).
    pub fn sennheiser_request(command: CommandId) -> Self {
        Self::new(VENDOR_SENNHEISER, command.request(), Vec::new())
    }

    pub fn kind(&self) -> PacketKind {
        self.command.kind()
    }

    /// Encode en version 3, flags nuls.
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        let len = u8::try_from(self.payload.len())
            .ok()
            .filter(|&l| usize::from(l) <= MAX_PAYLOAD_LEN)
            .ok_or(ProtocolError::PayloadTooLong(self.payload.len()))?;
        let mut out = Vec::with_capacity(HEADER_LEN + usize::from(len));
        out.extend_from_slice(&[START_OF_FRAME, PROTOCOL_VERSION, 0, len]);
        out.extend_from_slice(&self.vendor.to_be_bytes());
        out.extend_from_slice(&self.command.raw().to_be_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Décode une trame au début de `buf`.
    ///
    /// Renvoie la trame et le nombre d'octets consommés, pour pouvoir
    /// enchaîner les trames collées dans un même buffer.
    pub fn decode(buf: &[u8]) -> Result<(Frame, usize), ProtocolError> {
        if let Some(&first) = buf.first()
            && first != START_OF_FRAME
        {
            return Err(ProtocolError::BadStartByte(first));
        }
        if let Some(&version) = buf.get(1)
            && !SUPPORTED_VERSIONS.contains(&version)
        {
            return Err(ProtocolError::UnsupportedVersion(version));
        }
        if let Some(&flags) = buf.get(2)
            && flags != 0
        {
            return Err(ProtocolError::UnsupportedFlags(flags));
        }
        if buf.len() < HEADER_LEN {
            return Err(ProtocolError::Incomplete {
                needed: HEADER_LEN - buf.len(),
            });
        }
        let payload_len = usize::from(buf[3]);
        if payload_len > MAX_PAYLOAD_LEN {
            return Err(ProtocolError::PayloadTooLong(payload_len));
        }
        let total = HEADER_LEN + payload_len;
        if buf.len() < total {
            return Err(ProtocolError::Incomplete {
                needed: total - buf.len(),
            });
        }
        let vendor = u16::from_be_bytes([buf[4], buf[5]]);
        let command = CommandId::new(u16::from_be_bytes([buf[6], buf[7]]));
        let payload = buf[HEADER_LEN..total].to_vec();
        Ok((Frame::new(vendor, command, payload), total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_empty_request() {
        let bytes = Frame::sennheiser_request(command::get::BATTERY)
            .encode()
            .unwrap();
        assert_eq!(bytes, [0xFF, 0x03, 0x00, 0x00, 0x04, 0x95, 0x06, 0x03]);
    }

    #[test]
    fn decode_reports_missing_bytes() {
        assert_eq!(
            Frame::decode(&[0xFF, 0x03, 0x00]),
            Err(ProtocolError::Incomplete { needed: 5 })
        );
        assert_eq!(
            Frame::decode(&[0xFF, 0x03, 0x00, 0x02, 0x04, 0x95, 0x07, 0x03, 0x2A]),
            Err(ProtocolError::Incomplete { needed: 1 })
        );
        assert_eq!(
            Frame::decode(&[]),
            Err(ProtocolError::Incomplete { needed: 8 })
        );
    }

    #[test]
    fn decode_rejects_bad_header() {
        assert_eq!(
            Frame::decode(&[0x00, 0x03]),
            Err(ProtocolError::BadStartByte(0x00))
        );
        assert_eq!(
            Frame::decode(&[0xFF, 0x01, 0x00, 0x00, 0x04, 0x95, 0x06, 0x03]),
            Err(ProtocolError::UnsupportedVersion(0x01))
        );
        // Flags non nuls (somme de contrôle, longueur étendue) : jamais
        // observés sur le M5, donc refusés.
        assert_eq!(
            Frame::decode(&[0xFF, 0x03, 0x01, 0x00, 0x04, 0x95, 0x06, 0x03]),
            Err(ProtocolError::UnsupportedFlags(0x01))
        );
        assert_eq!(
            Frame::decode(&[0xFF, 0x04, 0x02, 0x00, 0x04, 0x95, 0x06, 0x03]),
            Err(ProtocolError::UnsupportedFlags(0x02))
        );
        let mut long = vec![0xFF, 0x03, 0x00, 0xFF, 0x04, 0x95, 0x06, 0x03];
        long.extend([0; 255]);
        assert_eq!(
            Frame::decode(&long),
            Err(ProtocolError::PayloadTooLong(255))
        );
    }

    #[test]
    fn version_4_answers_decode_like_version_3() {
        // Trame reçue par Smart Control Plus (capture Android).
        let v4 = [0xFF, 0x04, 0x00, 0x01, 0x04, 0x95, 0x20, 0x81, 0x02];
        let v3 = [0xFF, 0x03, 0x00, 0x01, 0x04, 0x95, 0x20, 0x81, 0x02];
        assert_eq!(Frame::decode(&v4), Frame::decode(&v3));
        assert_eq!(Frame::decode(&v4).unwrap().1, 9);
    }

    #[test]
    fn encode_rejects_oversized_payload() {
        let frame = Frame::new(VENDOR_SENNHEISER, command::get::BATTERY, vec![0; 255]);
        assert_eq!(frame.encode(), Err(ProtocolError::PayloadTooLong(255)));
        let frame = Frame::new(VENDOR_SENNHEISER, command::get::BATTERY, vec![0; 70_000]);
        assert_eq!(frame.encode(), Err(ProtocolError::PayloadTooLong(70_000)));
        let frame = Frame::new(VENDOR_SENNHEISER, command::get::BATTERY, vec![0; 254]);
        let bytes = frame.encode().unwrap();
        assert_eq!(&bytes[..4], &[0xFF, 0x03, 0x00, 0xFE]);
    }
}
