//! Transport factice pour les tests : réponses scriptées, sans casque.

use std::collections::VecDeque;
use std::time::Duration;

use super::{Transport, TransportError};
use crate::hex::to_hex;

#[derive(Debug, Default)]
pub struct MockTransport {
    /// Requêtes attendues dans l'ordre, avec les morceaux à renvoyer.
    script: VecDeque<(Vec<u8>, Vec<Vec<u8>>)>,
    /// Morceaux prêts à être lus par `receive`.
    inbox: VecDeque<Vec<u8>>,
    sent: Vec<Vec<u8>>,
}

impl MockTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Quand `request` est envoyée, le mock renverra `reply` d'un seul bloc.
    pub fn expect(self, request: &[u8], reply: &[u8]) -> Self {
        self.expect_chunked(request, &[reply])
    }

    /// Comme `expect`, mais la réponse arrive en plusieurs lectures
    /// (pour simuler une trame coupée par le flux RFCOMM).
    pub fn expect_chunked(mut self, request: &[u8], chunks: &[&[u8]]) -> Self {
        self.script.push_back((
            request.to_vec(),
            chunks
                .iter()
                .filter(|c| !c.is_empty())
                .map(|c| c.to_vec())
                .collect(),
        ));
        self
    }

    /// Ajoute des octets non sollicités (ex. une notification).
    pub fn push_incoming(&mut self, bytes: &[u8]) {
        self.inbox.push_back(bytes.to_vec());
    }

    /// Toutes les trames envoyées, dans l'ordre.
    pub fn sent(&self) -> &[Vec<u8>] {
        &self.sent
    }

    /// Vrai si toutes les requêtes scriptées ont été envoyées.
    pub fn is_exhausted(&self) -> bool {
        self.script.is_empty()
    }
}

impl Transport for MockTransport {
    fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
        self.sent.push(bytes.to_vec());
        match self.script.front() {
            Some((expected, _)) if expected == bytes => {
                if let Some((_, replies)) = self.script.pop_front() {
                    self.inbox.extend(replies);
                }
                Ok(())
            }
            _ => Err(TransportError::UnexpectedRequest(to_hex(bytes))),
        }
    }

    fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
        // Pas d'attente réelle : un mock vide expire immédiatement.
        self.inbox
            .pop_front()
            .ok_or(TransportError::Timeout(timeout))
    }
}
