//! Audit indépendant de la v0.5.0 (Codex, 10/10/2026) : données fictives et
//! MockTransport uniquement, aucun casque.
//!
//! Les deux premiers tests échouaient sur la v0.5.0 (`94baef9`). Depuis,
//! `set_eq_gains` tient le cache à jour (`&mut EqState`) : il est passé
//! ainsi ci-dessous, comme le fait le worker.

use momentum_core::device::{EqState, Headset};
use momentum_core::diagnostic;
use momentum_core::protocol::{CommandId, Frame, VENDOR_SENNHEISER};
use momentum_core::transport::MockTransport;
use std::time::Duration;

fn frame(cmd: u16, payload: &[u8]) -> Vec<u8> {
    Frame::new(VENDOR_SENNHEISER, CommandId::new(cmd), payload.to_vec())
        .encode()
        .unwrap()
}

fn initial() -> EqState {
    EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![10, 0, 0, 0, 0, 0, 0, 0],
        bass_boost: false,
    }
}

#[test]
fn redact_unicode_with_compensating_case_lengths_must_not_panic() {
    // U+0130 s'allonge en minuscules, U+1E9E raccourcit : même longueur
    // totale, positions différentes.
    let output = diagnostic::redact("İẞX", &["ẞX"]);
    assert!(!output.contains("ẞX"));
}

#[test]
fn eq_flat_then_restore_with_one_shared_cache_sends_the_restore() {
    // Comme dans un lot du worker : le même cache sert aux deux opérations.
    let mock = MockTransport::new()
        .expect(&frame(0x1001, &[0, 0]), &frame(0x1101, &[]))
        .expect(&frame(0x1001, &[0, 10]), &frame(0x1101, &[]));
    let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
    let mut cached = initial();
    let original = cached.gains.clone();
    headset.set_eq_gains(&mut cached, &[0; 8]).unwrap();
    headset.set_eq_gains(&mut cached, &original).unwrap();
    assert!(
        headset.client().transport().is_exhausted(),
        "restauration non envoyée"
    );
    assert_eq!(cached.gains, original);
}

#[test]
fn fresh_eq_cache_positive_control_restores_the_curve() {
    let mock = MockTransport::new()
        .expect(&frame(0x1001, &[0, 0]), &frame(0x1101, &[]))
        .expect(&frame(0x1001, &[0, 10]), &frame(0x1101, &[]));
    let mut headset = Headset::validated_for_tests(mock, Duration::from_millis(20));
    let mut first = initial();
    headset.set_eq_gains(&mut first, &[0; 8]).unwrap();
    let mut current = EqState {
        gains: vec![0; 8],
        ..initial()
    };
    headset
        .set_eq_gains(&mut current, &initial().gains)
        .unwrap();
    assert!(headset.client().transport().is_exhausted());
}
