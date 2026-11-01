//! Essai ciblé de l'état de charge (fiche d'implémentation du 28/10/2026,
//! section 8). Outil de développement, jamais livré : n'envoie que les deux
//! lectures d'identité (`1206`, `1201`) puis l'échange du palier choisi ;
//! aucune écriture de réglage.
//!
//! - palier 1 : une seule lecture `0602`, aucun abonnement ;
//! - palier 2 : un seul abonnement `0007 [03]`, aucune lecture `0602`, puis
//!   écoute : le casque envoie-t-il lui-même `0682` ? L'utilisateur peut
//!   brancher ou débrancher le câble pendant l'écoute.
//!
//! Arrêt sans rien envoyer de plus si l'identité n'est pas exactement
//! M5AEBT 6.21.5, au premier refus, expiration, perte du transport, code
//! inconnu ou notification de charge malformée. Une seule tentative.
//!
//! Usage : cargo run --release -p momentum-cli --example essai_charge -- --palier 1 --journal <fichier>

#[path = "commun/exclusion.rs"]
mod exclusion;

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use momentum_core::client::RawEvent;
use momentum_core::device::{ChargerStatus, Headset};
use momentum_core::hex::to_hex;
use momentum_core::protocol::command::{REGISTER_NOTIFICATION, feature, get};
use momentum_core::protocol::messages::{parse_charge_notification, parse_charge_response};
use momentum_core::protocol::{Frame, VENDOR_SENNHEISER};
use momentum_core::transport::Transport;

#[derive(Parser)]
struct Args {
    /// Palier : 1 (une lecture 0602) ou 2 (abonnement et écoute, sans 0602).
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=2))]
    palier: u8,
    /// Journal privé (CSV : temps relatif, événement, détail).
    #[arg(long)]
    journal: std::path::PathBuf,
    /// Délai d'une requête, en millisecondes.
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
    /// Durée d'écoute du palier 2, en secondes.
    #[arg(long, default_value_t = 30)]
    ecoute_s: u64,
}

/// Lignes du compte rendu (sans nom d'appareil ni adresse).
struct Report {
    start: Instant,
    lines: Vec<String>,
    charge_notifications: usize,
    last_status: Option<ChargerStatus>,
}

/// Geste demandé à l'utilisateur, d'après le dernier état annoncé.
fn gesture(status: Option<ChargerStatus>) -> &'static str {
    match status {
        Some(ChargerStatus::Charging) => ">>> DÉBRANCHEZ LE CÂBLE MAINTENANT <<<",
        Some(ChargerStatus::Disconnected) => ">>> BRANCHEZ LE CÂBLE MAINTENANT <<<",
        _ => ">>> CHANGEZ L'ÉTAT DU CÂBLE MAINTENANT <<<",
    }
}

fn describe(status: ChargerStatus) -> String {
    match status {
        ChargerStatus::Disconnected => "00 chargeur débranché".into(),
        ChargerStatus::Charging => "01 en charge".into(),
        ChargerStatus::Unknown(b) => format!("{b:02X} inconnu"),
    }
}

impl Report {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            lines: vec!["ms;evenement;detail".to_string()],
            charge_notifications: 0,
            last_status: None,
        }
    }

    fn note(&mut self, event: &str, detail: &str) {
        let line = format!("{};{event};{detail}", self.start.elapsed().as_millis());
        println!("{line}");
        self.lines.push(line);
    }

    /// Toutes les notifications sont notées ; seule `0495:0682` est
    /// interprétée. Un code inconnu ou une trame malformée arrête l'essai,
    /// après avoir noté le lot entier.
    fn notifications(&mut self, frames: &[Frame]) -> anyhow::Result<()> {
        let mut anomaly = None;
        for f in frames {
            let detail = format!(
                "vendor {:04X} id {:04X} payload {}",
                f.vendor,
                f.command.raw(),
                to_hex(&f.payload)
            );
            if f.vendor == VENDOR_SENNHEISER && f.command == get::CHARGER_STATE.notification() {
                self.charge_notifications += 1;
                match parse_charge_notification(&f.payload) {
                    Ok(ChargerStatus::Unknown(b)) => {
                        self.note(
                            "notification-charge",
                            &format!("{detail} : {b:02X} inconnu"),
                        );
                        anomaly.get_or_insert(format!("notification de charge {b:02X} inconnue"));
                    }
                    Ok(status) => {
                        self.last_status = Some(status);
                        self.note(
                            "notification-charge",
                            &format!("{detail} : {}", describe(status)),
                        );
                    }
                    Err(e) => {
                        self.note(
                            "notification-charge",
                            &format!("{detail} : malformée ({e})"),
                        );
                        anomaly.get_or_insert(format!("notification de charge malformée ({e})"));
                    }
                }
            } else {
                self.note("notification-autre", &detail);
            }
        }
        match anomaly {
            Some(reason) => {
                self.note("arret-notification", &reason);
                bail!("{reason} : essai arrêté, relecture des preuves avant de continuer")
            }
            None => Ok(()),
        }
    }
}

/// Lit l'identité ; refuse tout autre couple que M5AEBT 6.21.5.
fn check_identity<T: Transport>(
    headset: &mut Headset<T>,
    report: &mut Report,
) -> anyhow::Result<()> {
    let model = headset.identify();
    match headset.write_block_reason() {
        None => {
            report.note("identite", &format!("{model} / 6.21.5"));
            Ok(())
        }
        Some(reason) => {
            report.note("identite-refusee", reason);
            bail!("identité non établie ({reason}) : essai arrêté sans aucun échange de charge");
        }
    }
}

/// Notifications d'avant la réponse et suffixe du décodeur.
fn drain<T: Transport>(headset: &mut Headset<T>) -> Vec<Frame> {
    let frames = headset.client().pending_notifications().to_vec();
    headset.client().take_unsolicited();
    frames
}

/// La lecture `0602` du palier 1. La requête est envoyée ici, sans passer par
/// `Headset::charge_state` qui écarte les notifications de charge antérieures
/// à la réponse : toutes sont notées et contrôlées avant de lire la réponse.
fn read_once<T: Transport>(
    headset: &mut Headset<T>,
    timeout: Duration,
    report: &mut Report,
) -> anyhow::Result<()> {
    report.note("envoi", "0602 (vide)");
    let reply = headset
        .client()
        .request(&Frame::sennheiser_request(get::CHARGER_STATE), timeout);
    let frames = drain(headset);
    let checked = report.notifications(&frames);
    let reply = match reply {
        Ok(frame) => frame,
        Err(e) => {
            report.note("echec-lecture", &e.to_string());
            bail!("lecture 0602 sans réponse valide : essai arrêté ({e})");
        }
    };
    checked?;
    match parse_charge_response(&reply.payload) {
        Ok(reading) => {
            let second = reading
                .secondary_raw
                .map_or("absent".to_string(), |b| format!("{b:02X}"));
            report.note(
                "reponse",
                &format!(
                    "0702 {} ; second octet brut : {second}",
                    describe(reading.primary_status)
                ),
            );
            if let ChargerStatus::Unknown(b) = reading.primary_status {
                bail!(
                    "code {b:02X} inconnu : essai arrêté, relecture des preuves avant de continuer"
                );
            }
            Ok(())
        }
        Err(e) => {
            report.note(
                "reponse-malformee",
                &format!("{} ({e})", to_hex(&reply.payload)),
            );
            bail!("réponse 0702 malformée : essai arrêté ({e})");
        }
    }
}

/// Palier 2 : l'abonnement à la feature 3, puis écoute sans aucune requête.
fn subscribe_and_listen<T: Transport>(
    headset: &mut Headset<T>,
    timeout: Duration,
    listen: Duration,
    report: &mut Report,
) -> anyhow::Result<()> {
    report.note("envoi", "0007 [03]");
    let request = Frame::new(
        VENDOR_SENNHEISER,
        REGISTER_NOTIFICATION,
        vec![feature::BATTERY],
    );
    let reply = headset.client().request(&request, timeout);
    let frames = drain(headset);
    let checked = report.notifications(&frames);
    if let Err(e) = reply {
        report.note("echec-abonnement", &e.to_string());
        bail!("abonnement 0007 [03] refusé ou sans réponse : essai arrêté ({e})");
    }
    report.note("reponse", "0107 (accusé)");
    checked?;
    report.note(
        "consigne",
        &format!(
            "écoute {} s : NE TOUCHEZ À RIEN, le programme dira quand agir",
            listen.as_secs()
        ),
    );
    let start = Instant::now();
    // Une durée trop courte pour un geste (tests) n'en demande aucun.
    let mut pending = listen >= Duration::from_secs(20);
    while start.elapsed() < listen {
        if pending && start.elapsed() >= Duration::from_secs(5) {
            pending = false;
            report.note("geste", gesture(report.last_status));
        }
        let frames = headset
            .client()
            .poll(Duration::from_millis(250))
            .context("réception des notifications")?;
        report.notifications(&frames)?;
    }
    Ok(())
}

fn run<T: Transport>(
    palier: u8,
    headset: &mut Headset<T>,
    timeout: Duration,
    listen: Duration,
    report: &mut Report,
) -> anyhow::Result<()> {
    check_identity(headset, report)?;
    match palier {
        1 => read_once(headset, timeout, report)?,
        _ => subscribe_and_listen(headset, timeout, listen, report)?,
    }
    let frames = headset.client().take_unsolicited();
    report.notifications(&frames)?;
    let sent = report.charge_notifications;
    report.note("fin", &format!("{sent} notification(s) 0682 reçue(s)"));
    Ok(())
}

#[cfg(windows)]
fn connect() -> anyhow::Result<momentum_core::transport::windows_rfcomm::RfcommTransport> {
    use momentum_core::transport::windows_rfcomm::{RfcommTransport, list_gaia_services};
    use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
    // SAFETY: appel unique au démarrage, avant tout objet WinRT.
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let found = list_gaia_services().context("recherche du service GAIA")?;
    match found.as_slice() {
        [one] => RfcommTransport::connect(&one.service).context("connexion RFCOMM au casque"),
        [] => bail!("aucun casque GAIA connecté à ce PC"),
        _ => bail!("plusieurs appareils GAIA connectés : essai arrêté (un seul casque attendu)"),
    }
}

#[cfg(target_os = "linux")]
fn connect() -> anyhow::Result<momentum_core::transport::bluez_rfcomm::BluezTransport> {
    use momentum_core::transport::bluez_rfcomm::{BluezTransport, list_gaia_devices};
    let found = list_gaia_devices().context("recherche du casque dans BlueZ")?;
    match found.as_slice() {
        [one] => BluezTransport::connect(one, None).context("connexion RFCOMM au casque"),
        [] => bail!("aucun casque GAIA connecté à ce PC"),
        _ => bail!("plusieurs appareils GAIA connectés : essai arrêté (un seul casque attendu)"),
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut report = Report::new();
    report.note("palier", &args.palier.to_string());
    // Aucun autre client du casque ni autre essai en cours (R07).
    exclusion::verifier()?;
    let transport = connect()?;
    report.note(
        "connexion",
        "canal GAIA ouvert (appareil non nommé dans ce journal)",
    );
    let timeout = Duration::from_millis(args.timeout_ms);
    let mut headset = Headset::new(transport, timeout);
    headset.client().record_raw_log(true);

    let result = run(
        args.palier,
        &mut headset,
        timeout,
        Duration::from_secs(args.ecoute_s),
        &mut report,
    );
    if let Err(e) = &result {
        report.note("arret", &e.to_string());
    }
    // Octets bruts envoyés et reçus, à garder en privé.
    for event in headset.client().take_raw_log() {
        let (sens, bytes) = match event {
            RawEvent::Sent(b) => ("brut-envoye", b),
            RawEvent::Received(b) => ("brut-recu", b),
        };
        report.lines.push(format!("-;{sens};{}", to_hex(&bytes)));
    }
    let mut file = std::fs::File::create(&args.journal)
        .with_context(|| format!("journal {}", args.journal.display()))?;
    file.write_all((report.lines.join("\n") + "\n").as_bytes())?;
    println!("journal privé : {}", args.journal.display());
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use momentum_core::hex::from_hex;
    use momentum_core::transport::MockTransport;

    const T: Duration = Duration::from_millis(20);
    const GET: &str = "FF 03 00 00 04 95 06 02";
    const SUBSCRIBE: &str = "FF 03 00 01 04 95 00 07 03";
    const ACK: &str = "FF 03 00 00 04 95 01 07";

    fn h(hex: &str) -> Vec<u8> {
        from_hex(hex).unwrap()
    }

    /// Identité M5AEBT 6.21.5.
    fn identified() -> MockTransport {
        MockTransport::new()
            .expect(
                &h("FF 03 00 00 04 95 12 06"),
                &h("FF 03 00 07 04 95 13 06 4D 35 41 45 42 54 00"),
            )
            .expect(
                &h("FF 03 00 00 04 95 12 01"),
                &h("FF 03 00 06 04 95 13 01 00 06 00 15 00 05"),
            )
    }

    fn trial(palier: u8, mock: MockTransport) -> (anyhow::Result<()>, Report, usize) {
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        let result = run(
            palier,
            &mut headset,
            T,
            Duration::from_millis(60),
            &mut report,
        );
        let sent = headset.client().transport().sent().len();
        (result, report, sent)
    }

    fn logged(report: &Report, text: &str) -> bool {
        report.lines.iter().any(|l| l.contains(text))
    }

    #[test]
    fn palier_1_sends_exactly_three_requests() {
        let mock = identified().expect(&h(GET), &h("FF 03 00 02 04 95 07 02 01 00"));
        let (result, report, sent) = trial(1, mock);
        result.unwrap();
        assert_eq!(sent, 3);
        assert!(logged(
            &report,
            "0702 01 en charge ; second octet brut : 00"
        ));
    }

    #[test]
    fn palier_2_subscribes_once_and_never_reads_the_charge() {
        let mut mock = identified().expect(&h(SUBSCRIBE), &h(ACK));
        mock.push_incoming(&h("FF 03 00 01 04 95 06 82 00"));
        let (result, report, sent) = trial(2, mock);
        result.unwrap();
        // 2 identité + 1 abonnement ; la notification vient du casque seul.
        assert_eq!(sent, 3);
        assert!(logged(&report, "00 chargeur débranché"));
        assert!(logged(&report, "1 notification(s) 0682 reçue(s)"));
    }

    #[test]
    fn the_gesture_follows_the_announced_state() {
        assert!(gesture(Some(ChargerStatus::Charging)).contains("DÉBRANCHEZ"));
        assert!(gesture(Some(ChargerStatus::Disconnected)).contains(">>> BRANCHEZ"));
        assert!(gesture(None).contains("CHANGEZ"));
    }

    #[test]
    fn palier_2_without_notification_says_so() {
        let mock = identified().expect(&h(SUBSCRIBE), &h(ACK));
        let (result, report, sent) = trial(2, mock);
        result.unwrap();
        assert_eq!(sent, 3);
        assert!(logged(&report, "0 notification(s) 0682 reçue(s)"));
    }

    #[test]
    fn unknown_identity_stops_before_any_charge_exchange() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 12 06"),
            &h("FF 03 00 02 04 95 13 06 4D 00"),
        );
        let (result, _, sent) = trial(1, mock);
        assert!(result.is_err());
        assert_eq!(sent, 1);
    }

    #[test]
    fn anomalies_stop_the_read() {
        for reply in [
            // Refus GAIA, réponse vide, trop longue, code inconnu.
            "FF 03 00 01 04 95 07 82 01",
            "FF 03 00 00 04 95 07 02",
            "FF 03 00 03 04 95 07 02 01 00 00",
            "FF 03 00 02 04 95 07 02 02 00",
        ] {
            let mock = identified().expect(&h(GET), &h(reply));
            let (result, _, sent) = trial(1, mock);
            assert!(result.is_err(), "{reply}");
            assert_eq!(sent, 3, "{reply}");
        }
    }

    #[test]
    fn a_silent_headset_stops_the_read_after_one_attempt() {
        let mock = identified().expect_chunked(&h(GET), &[]);
        let (result, _, sent) = trial(1, mock);
        assert!(result.is_err());
        assert_eq!(sent, 3);
    }

    #[test]
    fn refused_subscription_stops_the_trial() {
        let mock = identified().expect(&h(SUBSCRIBE), &h("FF 03 00 01 04 95 01 87 01"));
        let (result, _, sent) = trial(2, mock);
        assert!(result.is_err());
        assert_eq!(sent, 3);
    }

    #[test]
    fn invalid_charge_notifications_stop_the_trial() {
        for bad in [
            "FF 03 00 01 04 95 06 82 02",
            "FF 03 00 02 04 95 06 82 01 00",
        ] {
            let mut mock = identified().expect(&h(SUBSCRIBE), &h(ACK));
            mock.push_incoming(&h(bad));
            let (result, report, _) = trial(2, mock);
            assert!(result.is_err(), "{bad}");
            assert!(logged(&report, "arret-notification"), "{bad}");
        }
    }

    #[test]
    fn notification_before_the_reply_is_logged_not_hidden() {
        let mock = identified().expect(
            &h(GET),
            &h("FF 03 00 01 04 95 06 82 01 FF 03 00 02 04 95 07 02 00 00"),
        );
        let (result, report, _) = trial(1, mock);
        result.unwrap();
        assert!(logged(&report, "notification-charge"));
        assert!(logged(&report, "0702 00 chargeur débranché"));
    }

    #[test]
    fn other_notifications_are_only_logged() {
        let mut mock = identified().expect(&h(SUBSCRIBE), &h(ACK));
        mock.push_incoming(&h("FF 03 00 01 04 95 06 83 5A"));
        let (result, report, _) = trial(2, mock);
        result.unwrap();
        assert!(logged(&report, "notification-autre"));
    }
}
