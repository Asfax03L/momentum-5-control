//! Vérifie que le choix du format ne change aucune trame ni aucun garde-fou.

use super::*;
use momentum_core::device::Headset;
use momentum_core::protocol::{CommandId, Frame, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;
use std::time::Duration;

fn frame(id: u16, payload: &[u8]) -> Vec<u8> {
    Frame::new(VENDOR_SENNHEISER, CommandId::new(id), payload.to_vec())
        .encode()
        .unwrap()
}

// Même capture que device::tests::read_noise_from_m5_answers, précédée
// de l'écriture 1A02 déjà utilisée par ctl niveau.
fn level_readback(level: u8) -> MockTransport {
    MockTransport::new()
        .expect(&frame(0x1A02, &[42]), &frame(0x1B02, &[]))
        .expect(&frame(0x1A05, &[]), &frame(0x1B05, &[1]))
        .expect(&frame(0x1A01, &[]), &frame(0x1B01, &[1, 1, 3, 0, 2, 0]))
        .expect(&frame(0x1A03, &[]), &frame(0x1B03, &[level]))
}

#[test]
fn format_json_ne_change_ni_les_trames_ni_la_relecture() {
    let mut traces = Vec::new();
    for json in [false, true] {
        let mut argv = vec!["ctl", "niveau", "42"];
        if json {
            argv.push("--json");
        }
        let args = Args::try_parse_from(argv).unwrap();
        let mut output = Output::new(json);
        let mut headset =
            Headset::validated_for_tests(level_readback(42), Duration::from_millis(20));
        app::run_direct(&args, &mut output, &mut headset).unwrap();
        assert!(headset.client().transport().is_exhausted());
        traces.push(headset.client().transport().sent().to_vec());
        let value: serde_json::Value =
            serde_json::from_str(&output.render(&args.command, None)).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["etat"]["bruit"]["niveau"], 42);
        assert!(value["etat"]["batterie"].is_null());
    }
    assert_eq!(traces[0], traces[1]);
    assert_eq!(traces[1].len(), 4);
}

#[test]
fn relecture_differente_reste_un_echec_et_ne_reessaie_pas() {
    let args = Args::try_parse_from(["ctl", "--json", "niveau", "42"]).unwrap();
    let mut output = Output::new(true);
    let mut headset = Headset::validated_for_tests(level_readback(41), Duration::from_millis(20));
    let error = app::run_direct(&args, &mut output, &mut headset)
        .unwrap_err()
        .to_string();
    let value: serde_json::Value =
        serde_json::from_str(&output.render(&args.command, Some(&error))).unwrap();
    assert_eq!(value["ok"], false);
    assert_eq!(value["etat"]["bruit"]["niveau"], 41);
    assert_eq!(headset.client().transport().sent().len(), 4);
}

#[test]
fn modele_non_valide_reste_en_lecture_seule() {
    let args = Args::try_parse_from(["ctl", "niveau", "42", "--json"]).unwrap();
    let mut output = Output::new(true);
    let mut headset = Headset::new(MockTransport::new(), Duration::from_millis(20));
    let error = app::run_direct(&args, &mut output, &mut headset)
        .unwrap_err()
        .to_string();
    assert!(error.contains("lecture seule"));
    assert!(headset.client().transport().sent().is_empty());
}

#[test]
fn reconnect_never_uses_the_direct_gaia_transport() {
    for json in [false, true] {
        let args = Args::try_parse_from(if json {
            vec!["ctl", "reconnecter", "--json"]
        } else {
            vec!["ctl", "reconnecter"]
        })
        .unwrap();
        let mut output = Output::new(json);
        let mut headset = Headset::new(MockTransport::new(), Duration::from_millis(20));
        assert!(
            app::run_direct(&args, &mut output, &mut headset)
                .unwrap_err()
                .to_string()
                .contains("service requis")
        );
        assert!(headset.client().transport().sent().is_empty());
    }
}
