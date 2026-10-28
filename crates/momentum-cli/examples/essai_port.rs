//! Essai ciblé de l'état porté / non porté (fiche de qualification du
//! 27/10/2026). Outil de développement, jamais livré : n'envoie que les deux
//! lectures d'identité (`1206`, `1201`) puis les échanges du palier choisi,
//! dans un budget fixe ; aucune écriture de réglage, aucun autre abonnement.
//!
//! - palier 1 : une lecture `0402` ;
//! - palier 2 : un abonnement `0007 [02]`, puis trois cycles porté → posé
//!   faits par l'utilisateur (Entrée quand la position est stable) : une
//!   lecture `0402` par position, six au plus ; notifications `0482` notées ;
//! - palier 3 : nouvelle connexion (après extinction et rallumage du casque
//!   par l'utilisateur) : un abonnement et une lecture.
//!
//! Arrêt sans rien envoyer de plus si l'identité n'est pas exactement
//! M5AEBT 6.21.5, au premier refus, expiration ou perte du transport.
//! Une seule tentative par requête.
//!
//! Usage : cargo run --release -p momentum-cli --example essai_port -- --palier 1 --journal <fichier>

use std::io::Write;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use momentum_core::client::RawEvent;
use momentum_core::device::{Headset, OnHeadState};
use momentum_core::hex::to_hex;
use momentum_core::protocol::{Frame, VENDOR_SENNHEISER};
use momentum_core::transport::Transport;

#[derive(Parser)]
struct Args {
    /// Palier de la fiche : 1, 2 ou 3.
    #[arg(long, value_parser = clap::value_parser!(u8).range(1..=3))]
    palier: u8,
    /// Journal privé (CSV : temps relatif, sens, identifiant, payload).
    #[arg(long)]
    journal: std::path::PathBuf,
    /// Délai d'une requête, en millisecondes.
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
}

/// Budget d'échanges du port, fixé par palier.
struct Budget {
    gets: u8,
    subscriptions: u8,
}

impl Budget {
    fn of(palier: u8) -> Self {
        match palier {
            1 => Budget {
                gets: 1,
                subscriptions: 0,
            },
            2 => Budget {
                gets: 6,
                subscriptions: 1,
            },
            _ => Budget {
                gets: 1,
                subscriptions: 1,
            },
        }
    }
}

/// Lignes du compte rendu (sans nom d'appareil ni adresse).
struct Report {
    start: Instant,
    lines: Vec<String>,
}

impl Report {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            lines: vec!["ms;evenement;detail".to_string()],
        }
    }

    fn note(&mut self, event: &str, detail: &str) {
        let line = format!("{};{event};{detail}", self.start.elapsed().as_millis());
        println!("{line}");
        self.lines.push(line);
    }

    /// Notifications reçues : seule `0495:0482` est interprétée. Toutes sont
    /// notées ; une notification de port malformée ou de code inconnu arrête
    /// l'essai (critère de la fiche), après avoir noté le lot entier. Les
    /// autres fonctions sont seulement notées.
    fn notifications(&mut self, frames: &[Frame]) -> anyhow::Result<()> {
        let mut anomaly = None;
        for f in frames {
            let detail = format!(
                "vendor {:04X} id {:04X} payload {}",
                f.vendor,
                f.command.raw(),
                to_hex(&f.payload)
            );
            if f.vendor == VENDOR_SENNHEISER && f.command.raw() == 0x0482 {
                match momentum_core::protocol::messages::parse_on_head_state(&f.payload) {
                    Ok(OnHeadState::Unknown(b)) => {
                        self.note("notification-port", &format!("{detail} : {b:02X} inconnu"));
                        anomaly.get_or_insert(format!("notification de port {b:02X} inconnue"));
                    }
                    Ok(state) => {
                        self.note(
                            "notification-port",
                            &format!("{detail} : {}", describe(state)),
                        );
                    }
                    Err(e) => {
                        self.note("notification-port", &format!("{detail} : malformée ({e})"));
                        anomaly.get_or_insert(format!("notification de port malformée ({e})"));
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

fn describe(state: OnHeadState) -> String {
    match state {
        OnHeadState::NotWorn => "02 non porté".into(),
        OnHeadState::Worn => "03 porté".into(),
        OnHeadState::Unknown(b) => format!("{b:02X} inconnu"),
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
            bail!("identité non établie ({reason}) : essai arrêté sans aucun échange du port");
        }
    }
}

/// Une lecture `0402`, décomptée du budget.
fn read_once<T: Transport>(
    headset: &mut Headset<T>,
    budget: &mut Budget,
    report: &mut Report,
    label: &str,
) -> anyhow::Result<OnHeadState> {
    if budget.gets == 0 {
        bail!("budget de lectures épuisé");
    }
    budget.gets -= 1;
    report.note("envoi", "0402 (vide)");
    match headset.on_head_state() {
        Ok(OnHeadState::Unknown(b)) => {
            report.note("reponse", &format!("0502 {b:02X} inconnu [{label}]"));
            bail!("code {b:02X} inconnu : essai arrêté, relecture des preuves avant de continuer");
        }
        Ok(state) => {
            report.note("reponse", &format!("0502 {} [{label}]", describe(state)));
            Ok(state)
        }
        Err(e) => {
            report.note("echec-lecture", &e.to_string());
            bail!("lecture 0402 sans réponse valide : essai arrêté ({e})");
        }
    }
}

/// Un abonnement `0007 [02]`, décompté du budget.
fn subscribe_once<T: Transport>(
    headset: &mut Headset<T>,
    budget: &mut Budget,
    report: &mut Report,
) -> anyhow::Result<()> {
    if budget.subscriptions == 0 {
        bail!("budget d'abonnement épuisé");
    }
    budget.subscriptions -= 1;
    report.note("envoi", "0007 [02]");
    match headset.register_on_head_notification() {
        Ok(()) => {
            report.note("reponse", "0107 (accusé)");
            Ok(())
        }
        Err(e) => {
            report.note("echec-abonnement", &e.to_string());
            bail!("abonnement 0007 [02] refusé ou sans réponse : essai arrêté ({e})");
        }
    }
}

/// Déroulé d'un palier sur un casque déjà connecté. `ready` reçoit un
/// signal chaque fois que l'utilisateur annonce une position stable
/// (palier 2) ; entre deux signaux, les notifications sont seulement lues.
fn run<T: Transport>(
    palier: u8,
    headset: &mut Headset<T>,
    ready: &Receiver<()>,
    report: &mut Report,
) -> anyhow::Result<()> {
    check_identity(headset, report)?;
    let mut budget = Budget::of(palier);
    match palier {
        1 => {
            read_once(headset, &mut budget, report, "casque posé")?;
        }
        2 => {
            subscribe_once(headset, &mut budget, report)?;
            let positions = ["porté", "posé"].repeat(3);
            for (i, position) in positions.iter().enumerate() {
                report.note(
                    "consigne",
                    &format!(
                        "cycle {} : casque {position}, puis Entrée après environ 10 s stables",
                        i / 2 + 1
                    ),
                );
                loop {
                    let frames = headset
                        .client()
                        .poll(Duration::from_millis(250))
                        .context("réception des notifications")?;
                    report.notifications(&frames)?;
                    match ready.try_recv() {
                        Ok(()) => break,
                        Err(TryRecvError::Empty) => {}
                        Err(TryRecvError::Disconnected) => bail!("saisie fermée : essai arrêté"),
                    }
                }
                read_once(headset, &mut budget, report, position)?;
                // Notifications reçues pendant la lecture (après elle).
                let frames = headset.client().take_unsolicited();
                report.notifications(&frames)?;
            }
        }
        _ => {
            subscribe_once(headset, &mut budget, report)?;
            read_once(headset, &mut budget, report, "après reconnexion")?;
            let frames = headset.client().take_unsolicited();
            report.notifications(&frames)?;
        }
    }
    report.note(
        "fin",
        &format!(
            "budget restant : {} lecture(s), {} abonnement(s)",
            budget.gets, budget.subscriptions
        ),
    );
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
    let transport = connect()?;
    report.note(
        "connexion",
        "canal GAIA ouvert (appareil non nommé dans ce journal)",
    );
    let mut headset = Headset::new(transport, Duration::from_millis(args.timeout_ms));
    headset.client().record_raw_log(true);

    // Entrée : position stable (palier 2).
    let (tx, ready) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lines() {
            if line.is_err() || tx.send(()).is_err() {
                break;
            }
        }
    });

    let result = run(args.palier, &mut headset, &ready, &mut report);
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

    fn signals(count: usize) -> Receiver<()> {
        let (tx, rx) = std::sync::mpsc::channel();
        for _ in 0..count {
            tx.send(()).unwrap();
        }
        rx
    }

    #[test]
    fn palier_1_sends_exactly_one_read() {
        let mock = identified().expect(
            &h("FF 03 00 00 04 95 04 02"),
            &h("FF 03 00 01 04 95 05 02 02"),
        );
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        run(1, &mut headset, &signals(0), &mut report).unwrap();
        assert!(headset.client().transport().is_exhausted());
        assert_eq!(headset.client().transport().sent().len(), 3);
        assert!(report.lines.iter().any(|l| l.contains("0502 02 non porté")));
    }

    #[test]
    fn palier_2_subscribes_once_and_reads_six_times() {
        let mut mock = identified().expect(
            &h("FF 03 00 01 04 95 00 07 02"),
            &h("FF 03 00 00 04 95 01 07"),
        );
        for reply in ["03", "02", "03", "02", "03", "02"] {
            mock = mock.expect(
                &h("FF 03 00 00 04 95 04 02"),
                &h(&format!("FF 03 00 01 04 95 05 02 {reply}")),
            );
        }
        // Notification de port déjà arrivée : notée et interprétée.
        mock.push_incoming(&h("FF 03 00 01 04 95 04 82 03"));
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        run(2, &mut headset, &signals(6), &mut report).unwrap();
        assert!(headset.client().transport().is_exhausted());
        // 2 identité + 1 abonnement + 6 lectures, rien d'autre.
        assert_eq!(headset.client().transport().sent().len(), 9);
        assert!(
            report
                .lines
                .iter()
                .any(|l| l.contains("notification-port") && l.contains("03 porté")),
            "{:?}",
            report.lines
        );
        assert!(
            report
                .lines
                .iter()
                .any(|l| l.contains("budget restant : 0 lecture(s), 0 abonnement(s)"))
        );
    }

    #[test]
    fn unknown_identity_stops_before_any_port_exchange() {
        let mock = MockTransport::new().expect(
            &h("FF 03 00 00 04 95 12 06"),
            &h("FF 03 00 02 04 95 13 06 4D 00"),
        );
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        assert!(run(1, &mut headset, &signals(0), &mut report).is_err());
        assert_eq!(headset.client().transport().sent().len(), 1);
    }

    #[test]
    fn refused_subscription_stops_the_trial() {
        let mock = identified().expect(
            &h("FF 03 00 01 04 95 00 07 02"),
            &h("FF 03 00 01 04 95 01 87 01"),
        );
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        assert!(run(3, &mut headset, &signals(0), &mut report).is_err());
        assert_eq!(headset.client().transport().sent().len(), 3);
    }

    #[test]
    fn unknown_code_stops_palier_1_and_3() {
        let mock = identified().expect(
            &h("FF 03 00 00 04 95 04 02"),
            &h("FF 03 00 01 04 95 05 02 01"),
        );
        let mut headset = Headset::new(mock, T);
        assert!(run(1, &mut headset, &signals(0), &mut Report::new()).is_err());
        let mock = identified()
            .expect(
                &h("FF 03 00 01 04 95 00 07 02"),
                &h("FF 03 00 00 04 95 01 07"),
            )
            .expect(
                &h("FF 03 00 00 04 95 04 02"),
                &h("FF 03 00 01 04 95 05 02 00"),
            );
        let mut headset = Headset::new(mock, T);
        assert!(run(3, &mut headset, &signals(0), &mut Report::new()).is_err());
    }

    #[test]
    fn invalid_port_notification_stops_before_the_next_request() {
        // Notification de port inconnue (01) puis malformée (deux octets) :
        // chacune arrête le palier 2 avant toute lecture.
        for bad in [
            "FF 03 00 01 04 95 04 82 01",
            "FF 03 00 02 04 95 04 82 03 00",
        ] {
            let mut mock = identified().expect(
                &h("FF 03 00 01 04 95 00 07 02"),
                &h("FF 03 00 00 04 95 01 07"),
            );
            mock.push_incoming(&h(bad));
            let mut headset = Headset::new(mock, T);
            let mut report = Report::new();
            assert!(run(2, &mut headset, &signals(6), &mut report).is_err());
            // 2 identité + 1 abonnement, aucune lecture 0402.
            assert_eq!(headset.client().transport().sent().len(), 3, "{bad}");
            assert!(
                report
                    .lines
                    .iter()
                    .any(|l| l.contains("arret-notification"))
            );
        }
    }

    #[test]
    fn other_notifications_are_only_logged() {
        let mut mock = identified().expect(
            &h("FF 03 00 00 04 95 04 02"),
            &h("FF 03 00 01 04 95 05 02 02"),
        );
        // Batterie (0683), autre fonction : notée, sans arrêt.
        mock.push_incoming(&h("FF 03 00 01 04 95 06 83 2A"));
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        run(1, &mut headset, &signals(0), &mut report).unwrap();
    }

    #[test]
    fn unknown_code_stops_palier_2() {
        let mock = identified()
            .expect(
                &h("FF 03 00 01 04 95 00 07 02"),
                &h("FF 03 00 00 04 95 01 07"),
            )
            .expect(
                &h("FF 03 00 00 04 95 04 02"),
                &h("FF 03 00 01 04 95 05 02 01"),
            );
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        let err = run(2, &mut headset, &signals(6), &mut report).unwrap_err();
        assert!(err.to_string().contains("01 inconnu") || err.to_string().contains("code 01"));
        assert_eq!(headset.client().transport().sent().len(), 4);
    }
}
