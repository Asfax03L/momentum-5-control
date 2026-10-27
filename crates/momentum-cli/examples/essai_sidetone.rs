//! Essai ciblé du réglage sidetone (captures du 25/10/2026). Outil de
//! développement, jamais livré. N'envoie que :
//! - les lectures d'identité `1206`, `1201` (arrêt si ce n'est pas
//!   M5AEBT 6.21.5) ;
//! - la lecture initiale `0806` (arrêt sans rien écrire si la valeur n'est
//!   pas 0, 1 ou 2) ;
//! - pour chaque autre niveau observé dans l'app (0, 1, 2) : une écriture
//!   `0805 [niveau]`, puis une relecture `0806` ;
//! - la remise de la valeur initiale, relue.
//!
//! Une seule tentative par requête. Au premier échec après une écriture,
//! une seule tentative de remise de la valeur initiale, puis arrêt.
//! Notifications `0886` notées. Journal privé : octets bruts compris.
//!
//! Usage : cargo run --release -p momentum-cli --example essai_sidetone -- --journal <fichier>

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use clap::Parser;
use momentum_core::client::RawEvent;
use momentum_core::device::{Headset, SIDETONE_LEVELS};
use momentum_core::hex::to_hex;
use momentum_core::protocol::{Frame, VENDOR_SENNHEISER};
use momentum_core::transport::Transport;

#[derive(Parser)]
struct Args {
    /// Journal privé (CSV : temps relatif, événement, détail).
    #[arg(long)]
    journal: std::path::PathBuf,
    /// Délai d'une requête, en millisecondes.
    #[arg(long, default_value_t = 3000)]
    timeout_ms: u64,
}

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

    fn notifications(&mut self, frames: &[Frame]) {
        for f in frames {
            let detail = format!(
                "vendor {:04X} id {:04X} payload {}",
                f.vendor,
                f.command.raw(),
                to_hex(&f.payload)
            );
            let kind = if f.vendor == VENDOR_SENNHEISER && f.command.raw() == 0x0886 {
                "notification-sidetone"
            } else {
                "notification-autre"
            };
            self.note(kind, &detail);
        }
    }
}

/// Lecture `0806`.
fn read<T: Transport>(headset: &mut Headset<T>, report: &mut Report) -> anyhow::Result<u8> {
    report.note("envoi", "0806 (vide)");
    let value = headset
        .sidetone()
        .inspect_err(|e| report.note("echec-lecture", &e.to_string()))
        .context("lecture 0806")?;
    report.note("reponse", &format!("0906 [{value:02X}]"));
    let frames = headset.client().take_unsolicited();
    report.notifications(&frames);
    Ok(value)
}

/// Écriture `0805 [niveau]` puis relecture ; `Err` si refus ou relecture
/// différente.
fn write_and_check<T: Transport>(
    headset: &mut Headset<T>,
    report: &mut Report,
    level: u8,
) -> anyhow::Result<()> {
    report.note("envoi", &format!("0805 [{level:02X}]"));
    headset
        .set_sidetone(level)
        .inspect_err(|e| report.note("echec-ecriture", &e.to_string()))
        .context("écriture 0805")?;
    report.note("reponse", "0905 (accusé)");
    let frames = headset.client().take_unsolicited();
    report.notifications(&frames);
    let back = read(headset, report)?;
    if back != level {
        report.note("ecart", &format!("demandé {level:02X}, relu {back:02X}"));
        bail!("relecture {back:02X} au lieu de {level:02X}");
    }
    Ok(())
}

/// Niveaux essayés, sans la valeur initiale, puis remise de celle-ci.
fn plan(initial: u8) -> Vec<u8> {
    let mut steps: Vec<u8> = [2, 1, 0].into_iter().filter(|&l| l != initial).collect();
    steps.push(initial);
    steps
}

fn run<T: Transport>(headset: &mut Headset<T>, report: &mut Report) -> anyhow::Result<()> {
    let model = headset.identify();
    if let Some(reason) = headset.write_block_reason() {
        report.note("identite-refusee", reason);
        bail!("identité non établie ({reason}) : essai arrêté sans aucun échange sidetone");
    }
    report.note("identite", &format!("{model} / 6.21.5"));
    let initial = read(headset, report)?;
    if !SIDETONE_LEVELS.contains(&initial) {
        report.note(
            "arret",
            &format!("valeur initiale {initial:02X} hors des niveaux observés"),
        );
        bail!("valeur initiale {initial:02X} inattendue : rien n'est écrit");
    }
    report.note("initial", &format!("{initial:02X}"));
    for level in plan(initial) {
        if let Err(e) = write_and_check(headset, report, level) {
            if level != initial && !headset.client().is_desynchronized() {
                report.note("restauration", &format!("une tentative vers {initial:02X}"));
                let _ = write_and_check(headset, report, initial);
            }
            return Err(e);
        }
    }
    report.note(
        "fin",
        &format!("valeur initiale {initial:02X} remise et relue"),
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
    let transport = connect()?;
    report.note(
        "connexion",
        "canal GAIA ouvert (appareil non nommé dans ce journal)",
    );
    let mut headset = Headset::new(transport, Duration::from_millis(args.timeout_ms));
    headset.client().record_raw_log(true);
    let result = run(&mut headset, &mut report);
    if let Err(e) = &result {
        report.note("arret", &format!("{e:#}"));
    }
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

    fn get(mock: MockTransport, value: &str) -> MockTransport {
        mock.expect(
            &h("FF 03 00 00 04 95 08 06"),
            &h(&format!("FF 03 00 01 04 95 09 06 {value}")),
        )
    }

    fn set(mock: MockTransport, value: &str) -> MockTransport {
        mock.expect(
            &h(&format!("FF 03 00 01 04 95 08 05 {value}")),
            &h("FF 03 00 00 04 95 09 05"),
        )
    }

    #[test]
    fn plan_ends_on_the_initial_value() {
        assert_eq!(plan(0), vec![2, 1, 0]);
        assert_eq!(plan(1), vec![2, 0, 1]);
        assert_eq!(plan(2), vec![1, 0, 2]);
    }

    #[test]
    fn full_cycle_from_off_is_restored_and_read_back() {
        let mut mock = get(identified(), "00");
        for v in ["02", "01", "00"] {
            mock = get(set(mock, v), v);
        }
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        run(&mut headset, &mut report).unwrap();
        assert!(headset.client().transport().is_exhausted());
        // 2 identité + 1 lecture + 3 × (écriture + relecture).
        assert_eq!(headset.client().transport().sent().len(), 9);
    }

    #[test]
    fn unexpected_initial_value_writes_nothing() {
        let mock = get(identified(), "04");
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        assert!(run(&mut headset, &mut report).is_err());
        assert_eq!(headset.client().transport().sent().len(), 3);
    }

    #[test]
    fn mismatch_triggers_one_restoration_then_stops() {
        // Relu 01 au lieu de 02 : une remise à 00, relue, puis arrêt.
        let mock = get(set(get(identified(), "00"), "02"), "01");
        let mock = get(set(mock, "00"), "00");
        let mut headset = Headset::new(mock, T);
        let mut report = Report::new();
        assert!(run(&mut headset, &mut report).is_err());
        assert!(headset.client().transport().is_exhausted());
        assert!(report.lines.iter().any(|l| l.contains("restauration")));
    }

    #[test]
    fn levels_three_and_four_are_never_sent() {
        let mock = MockTransport::new();
        let mut headset = Headset::validated_for_tests(mock, T);
        assert!(headset.set_sidetone(3).is_err());
        assert!(headset.set_sidetone(4).is_err());
        assert!(headset.client().transport().sent().is_empty());
    }
}
