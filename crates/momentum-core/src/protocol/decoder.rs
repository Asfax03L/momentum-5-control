//! Découpage d'un flux d'octets en trames.
//!
//! RFCOMM est un flux (comme TCP) : une lecture peut renvoyer une demi-trame,
//! ou deux trames collées. `FrameDecoder` accumule les octets et rend les
//! trames complètes une par une.

use super::{Frame, ProtocolError, START_OF_FRAME};

#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: Vec<u8>,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Octets reçus mais pas encore consommés.
    pub fn pending(&self) -> &[u8] {
        &self.buf
    }

    /// Renvoie la prochaine trame complète.
    ///
    /// - `None` : il faut plus d'octets ;
    /// - `Some(Err(Garbage))` : des octets parasites ont été jetés jusqu'au
    ///   prochain 0xFF (resynchronisation) ;
    /// - `Some(Err(UnsupportedVersion | UnsupportedFlags | PayloadTooLong))` :
    ///   en-tête non pris en charge, le 0xFF est jeté.
    pub fn next_frame(&mut self) -> Option<Result<Frame, ProtocolError>> {
        let first = *self.buf.first()?;
        if first != START_OF_FRAME {
            let skipped = self
                .buf
                .iter()
                .position(|&b| b == START_OF_FRAME)
                .unwrap_or(self.buf.len());
            self.buf.drain(..skipped);
            return Some(Err(ProtocolError::Garbage { skipped }));
        }
        match Frame::decode(&self.buf) {
            Ok((frame, used)) => {
                self.buf.drain(..used);
                Some(Ok(frame))
            }
            Err(ProtocolError::Incomplete { .. }) => None,
            Err(err) => {
                // On jette le 0xFF fautif pour pouvoir se resynchroniser.
                self.buf.drain(..1);
                Some(Err(err))
            }
        }
    }
}
