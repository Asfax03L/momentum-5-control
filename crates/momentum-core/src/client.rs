//! Échange requête → réponse GAIA au-dessus d'un `Transport`.
//!
//! Le client envoie une trame puis lit le flux jusqu'à la réponse
//! correspondante (`id | 0x0100`) ou l'erreur (`id | 0x0180`). Les autres
//! trames reçues entre-temps (notifications) sont mises de côté.
//!
//! GAIA ne porte pas d'identifiant de transaction : deux réponses au même
//! command id sont indiscernables. D'où deux règles :
//!
//! - une réponse déjà présente dans le tampon **avant** l'envoi ne peut pas
//!   répondre à la nouvelle requête : elle est jetée (avec un avertissement) ;
//! - après un échange interrompu (délai dépassé, mais aussi échec de l'envoi
//!   ou erreur de lecture : les octets ont pu partir), la réponse peut encore
//!   arriver plus tard et serait prise pour celle de la requête suivante. La
//!   session est donc marquée « désynchronisée » : toute requête suivante
//!   échoue sans rien envoyer, et l'appelant doit rouvrir la connexion. Aucune
//!   requête n'est renvoyée automatiquement (une écriture pourrait être
//!   appliquée deux fois).

use crate::i18n::trf;
use std::time::{Duration, Instant};

use thiserror::Error;
use tracing::{trace, warn};

use crate::hex::to_hex;
use crate::protocol::command::is_forbidden;
use crate::protocol::{CommandId, Frame, FrameDecoder, PacketKind, ProtocolError};
use crate::transport::{Transport, TransportError};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("{}", trf("le casque a répondu par une erreur à {} (payload {})", &[&.command, &format!("{:02X?}", .payload)]))]
    Device {
        command: CommandId,
        payload: Vec<u8>,
    },
    #[error("{}", trf("session désynchronisée après un échange interrompu sur {} : une réponse tardive serait ambiguë, il faut rouvrir la connexion", &[&.0]))]
    Desynchronized(CommandId),
    #[error("{}", trf("commande {} interdite : refusée sans rien envoyer au casque", &[&.0]))]
    Forbidden(CommandId),
}

/// Trace brute des échanges, pour le rapport du probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RawEvent {
    Sent(Vec<u8>),
    Received(Vec<u8>),
}

pub struct GaiaClient<T: Transport> {
    transport: T,
    decoder: FrameDecoder,
    /// Journal brut, seulement si `record_raw_log(true)` (probe) : une
    /// application qui tourne en continu ne doit pas l'accumuler.
    raw_log: Option<Vec<RawEvent>>,
    unsolicited: Vec<Frame>,
    /// Requête restée sans réponse sûre : la session n'est plus fiable.
    desynchronized: Option<CommandId>,
}

impl<T: Transport> GaiaClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            decoder: FrameDecoder::new(),
            raw_log: None,
            unsolicited: Vec::new(),
            desynchronized: None,
        }
    }

    /// Active ou coupe l'enregistrement des octets bruts (désactivé par défaut).
    pub fn record_raw_log(&mut self, on: bool) {
        self.raw_log = on.then(Vec::new);
    }

    fn log_raw(&mut self, event: RawEvent) {
        if let Some(log) = &mut self.raw_log {
            log.push(event);
        }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Vrai si une requête a expiré : il faut rouvrir la connexion.
    pub fn is_desynchronized(&self) -> bool {
        self.desynchronized.is_some()
    }

    /// Récupère (et vide) le journal brut des octets envoyés et reçus.
    pub fn take_raw_log(&mut self) -> Vec<RawEvent> {
        self.raw_log
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Notifications reçues et pas encore transmises, sans les retirer :
    /// celles encore dans le décodeur (suffixe d'une réponse) y sont
    /// ajoutées. Le propriétaire du client les recevra par `poll`.
    pub fn pending_notifications(&mut self) -> &[Frame] {
        self.drain_unexpected(None);
        &self.unsolicited
    }

    /// Récupère (et vide) les trames reçues qui n'étaient pas une réponse.
    pub fn take_unsolicited(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.unsolicited)
    }

    /// Oublie une notification déjà mise de côté avant le retour de `request`.
    /// À appeler seulement après décodage réussi de la réponse qui la remplace.
    /// Le décodeur reste intact : son suffixe peut contenir des notifications
    /// postérieures à la réponse, qui doivent encore parvenir à l'appelant.
    pub(crate) fn forget_queued_notification(&mut self, vendor: u16, command: CommandId) {
        self.unsolicited
            .retain(|frame| frame.vendor != vendor || frame.command != command);
    }

    /// Attend au plus `timeout` des trames non sollicitées (notifications).
    ///
    /// Renvoie d'abord celles reçues pendant des requêtes précédentes, dans
    /// l'ordre d'arrivée. Un délai dépassé n'est pas une erreur : la liste
    /// est alors vide. Les réponses orphelines sont jetées.
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<Frame>, ClientError> {
        if self.unsolicited.is_empty() {
            match self.transport.receive(timeout) {
                Ok(chunk) => self.push_received(chunk),
                Err(TransportError::Timeout(_)) => {}
                Err(e) => return Err(e.into()),
            }
        }
        self.drain_unexpected(None);
        Ok(self.take_unsolicited())
    }

    fn push_received(&mut self, chunk: Vec<u8>) {
        trace!(rx = %to_hex(&chunk), "octets reçus");
        self.decoder.push(&chunk);
        self.log_raw(RawEvent::Received(chunk));
    }

    /// Range les trames complètes du tampon : notifications de côté,
    /// réponses et erreurs orphelines jetées. `before` est la requête sur le
    /// point de partir, pour le journal.
    fn drain_unexpected(&mut self, before: Option<&Frame>) {
        while let Some(result) = self.decoder.next_frame() {
            match result {
                Ok(frame) => self.set_aside(frame, before),
                Err(err) => warn!(%err, "flux GAIA corrompu, resynchronisation"),
            }
        }
    }

    fn set_aside(&mut self, frame: Frame, before: Option<&Frame>) {
        match frame.kind() {
            PacketKind::Response | PacketKind::Error => {
                // Aucune requête en attente ne peut lui correspondre.
                warn!(
                    command = %frame.command,
                    payload = %crate::hex::loggable(&frame.payload),
                    before = ?before.map(|r| r.command),
                    "réponse orpheline jetée (reçue sans requête en attente)"
                );
            }
            _ => {
                trace!(?frame, "trame non sollicitée");
                self.unsolicited.push(frame);
            }
        }
    }

    /// Envoie `request` et attend sa réponse pendant au plus `timeout`.
    ///
    /// Après un délai dépassé ou une erreur de transport pendant l'échange,
    /// la session est désynchronisée (voir le module) : les requêtes
    /// suivantes échouent sans rien envoyer.
    pub fn request(&mut self, request: &Frame, timeout: Duration) -> Result<Frame, ClientError> {
        // Seul chemin d'envoi : aucun appelant ne peut contourner ce refus.
        if is_forbidden(request.vendor, request.command) {
            warn!(command = %request.command, "commande interdite refusée");
            return Err(ClientError::Forbidden(request.command));
        }
        if let Some(command) = self.desynchronized {
            return Err(ClientError::Desynchronized(command));
        }
        let bytes = request.encode()?;
        // Ce qui est déjà dans le tampon est antérieur à cette requête.
        self.drain_unexpected(Some(request));

        trace!(tx = %to_hex(&bytes), "trame envoyée");
        self.log_raw(RawEvent::Sent(bytes.clone()));
        if let Err(e) = self.transport.send(&bytes) {
            // Envoi ambigu : tout ou partie des octets a pu partir, et le
            // casque peut encore répondre.
            warn!(command = %request.command, %e, "envoi interrompu : session désynchronisée");
            self.desynchronized = Some(request.command);
            return Err(e.into());
        }

        let expected = request.command.response();
        let error_id = request.command.error();
        let deadline = Instant::now() + timeout;

        loop {
            while let Some(result) = self.decoder.next_frame() {
                match result {
                    Ok(frame) if frame.vendor == request.vendor && frame.command == expected => {
                        return Ok(frame);
                    }
                    Ok(frame) if frame.vendor == request.vendor && frame.command == error_id => {
                        return Err(ClientError::Device {
                            command: frame.command,
                            payload: frame.payload,
                        });
                    }
                    Ok(frame) => self.set_aside(frame, Some(request)),
                    Err(err) => warn!(%err, "flux GAIA corrompu, resynchronisation"),
                }
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let received = if remaining.is_zero() {
                Err(TransportError::Timeout(timeout))
            } else {
                self.transport.receive(remaining)
            };
            match received {
                Ok(chunk) => self.push_received(chunk),
                Err(TransportError::Timeout(t)) => {
                    warn!(command = %request.command, "pas de réponse : session désynchronisée");
                    self.desynchronized = Some(request.command);
                    return Err(TransportError::Timeout(t).into());
                }
                Err(e) => {
                    warn!(command = %request.command, %e, "lecture interrompue : session désynchronisée");
                    self.desynchronized = Some(request.command);
                    return Err(e.into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::from_hex;
    use crate::protocol::VENDOR_SENNHEISER;
    use crate::protocol::command::get;
    use crate::transport::MockTransport;

    const T: Duration = Duration::from_millis(20);

    fn h(s: &str) -> Vec<u8> {
        from_hex(s).unwrap()
    }

    fn band(n: u8) -> Frame {
        Frame::new(VENDOR_SENNHEISER, get::EQ_BAND, vec![n])
    }

    #[test]
    fn late_answer_after_timeout_is_never_taken_for_the_next_request() {
        // Bande 0 : pas de réponse dans le délai.
        let mock = MockTransport::new()
            .expect_chunked(&h("FF 03 00 01 04 95 10 02 00"), &[])
            .expect(
                &h("FF 03 00 01 04 95 10 02 01"),
                &h("FF 03 00 01 04 95 11 02 14"),
            );
        let mut client = GaiaClient::new(mock);
        assert!(matches!(
            client.request(&band(0), T),
            Err(ClientError::Transport(TransportError::Timeout(_)))
        ));
        assert!(client.is_desynchronized());
        // La requête suivante refuse de partir : rien n'est envoyé.
        assert!(matches!(
            client.request(&band(1), T),
            Err(ClientError::Desynchronized(_))
        ));
        assert_eq!(client.transport().sent().len(), 1);
    }

    #[test]
    fn forbidden_commands_never_reach_the_transport() {
        use crate::protocol::VENDOR_SENNHEISER;
        use crate::protocol::command::FORBIDDEN_COMMANDS;
        let mut client = GaiaClient::new(MockTransport::new());
        for command in FORBIDDEN_COMMANDS {
            let frame = Frame::new(VENDOR_SENNHEISER, command, vec![]);
            assert!(matches!(
                client.request(&frame, T),
                Err(ClientError::Forbidden(c)) if c == command
            ));
        }
        assert!(client.transport().sent().is_empty());
        // Un refus n'est pas un échange interrompu : la session reste saine.
        assert!(!client.is_desynchronized());
    }

    /// Transport dont le premier envoi échoue alors que les octets sont
    /// partis : la réponse du casque arrive quand même, plus tard.
    #[derive(Default)]
    struct AmbiguousSend {
        sent: Vec<Vec<u8>>,
        late: Vec<Vec<u8>>,
    }

    impl Transport for AmbiguousSend {
        fn send(&mut self, bytes: &[u8]) -> Result<(), TransportError> {
            self.sent.push(bytes.to_vec());
            if self.sent.len() == 1 {
                // Accusé tardif d'une écriture de bande (même command ID
                // que la suivante).
                self.late.push(h("FF 03 00 00 04 95 11 01"));
                return Err(TransportError::Deadline {
                    context: "envoi RFCOMM (StoreAsync)",
                    timeout: Duration::from_secs(5),
                });
            }
            Ok(())
        }

        fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, TransportError> {
            if self.late.is_empty() {
                Err(TransportError::Timeout(timeout))
            } else {
                Ok(self.late.remove(0))
            }
        }
    }

    #[test]
    fn ambiguous_send_never_lets_a_second_write_go() {
        use crate::protocol::command::set;
        let write = |gain: u8| Frame::new(VENDOR_SENNHEISER, set::EQ_BAND, vec![0, gain]);
        let mut client = GaiaClient::new(AmbiguousSend::default());
        assert!(matches!(
            client.request(&write(0x14), T),
            Err(ClientError::Transport(TransportError::Deadline { .. }))
        ));
        assert!(client.is_desynchronized());
        // La seconde écriture n'est pas envoyée : l'accusé tardif de la
        // première ne peut pas lui être attribué.
        assert!(matches!(
            client.request(&write(0x28), T),
            Err(ClientError::Desynchronized(_))
        ));
        assert_eq!(client.transport().sent.len(), 1);
    }

    /// Transport qui coupe la connexion pendant l'attente de la réponse.
    #[derive(Default)]
    struct ClosedWhileWaiting {
        sent: usize,
    }

    impl Transport for ClosedWhileWaiting {
        fn send(&mut self, _bytes: &[u8]) -> Result<(), TransportError> {
            self.sent += 1;
            Ok(())
        }

        fn receive(&mut self, _timeout: Duration) -> Result<Vec<u8>, TransportError> {
            Err(TransportError::Closed)
        }
    }

    #[test]
    fn read_error_while_waiting_desynchronizes_the_session() {
        let mut client = GaiaClient::new(ClosedWhileWaiting::default());
        assert!(matches!(
            client.request(&band(0), T),
            Err(ClientError::Transport(TransportError::Closed))
        ));
        assert!(client.is_desynchronized());
        assert!(matches!(
            client.request(&band(1), T),
            Err(ClientError::Desynchronized(_))
        ));
        assert_eq!(client.transport().sent, 1);
    }

    #[test]
    fn orphan_answer_received_while_waiting_is_dropped() {
        // Une réponse à une autre commande arrive avant la bonne : jetée,
        // pas rangée avec les notifications.
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 06 03"),
            &h("FF 03 00 01 04 95 11 02 0A FF 03 00 01 04 95 07 03 2A"),
        );
        let mut client = GaiaClient::new(mock);
        let frame = client
            .request(&Frame::sennheiser_request(get::BATTERY), T)
            .unwrap();
        assert_eq!(frame.payload, [0x2A]);
        assert!(client.take_unsolicited().is_empty());
    }

    #[test]
    fn notifications_keep_their_arrival_order() {
        // Deux notifications de curseur reçues pendant une requête, puis une
        // troisième par `poll` : l'ordre d'arrivée est conservé, si bien que
        // la dernière valeur appliquée est la plus récente.
        let first =
            h("FF 03 00 01 04 95 1A 83 0A FF 03 00 01 04 95 1A 83 14 FF 03 00 01 04 95 07 03 2A");
        let later = h("FF 03 00 01 04 95 1A 83 1E");
        let mock =
            MockTransport::new().expect_chunked(&h("FF 03 00 00 04 95 06 03"), &[&first, &later]);
        let mut client = GaiaClient::new(mock);
        client
            .request(&Frame::sennheiser_request(get::BATTERY), T)
            .unwrap();
        let levels: Vec<u8> = client
            .poll(T)
            .unwrap()
            .into_iter()
            .chain(client.poll(T).unwrap())
            .map(|f| f.payload[0])
            .collect();
        assert_eq!(levels, [10, 20, 30]);
    }
}
