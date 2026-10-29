//! Scénarios synthétiques : aucune fréquence ni écriture n'est mesurée ici.
use momentum_core::{
    device::EqState,
    presets::{self, AbCompare, EqTopology},
};

fn topology() -> EqTopology {
    EqTopology::momentum5(&EqState {
        min_gain: -60,
        max_gain: 60,
        gains: vec![0; 8],
        ..EqState::default()
    })
    .unwrap()
}

#[test]
fn legacy_presets_survive_and_require_explicit_association() {
    let old = "Écoute = 1;2;3;4;5;6;7;8\nAutre = 2;3\n";
    let mut presets = presets::parse_user_presets(old);
    let topo = topology();
    assert!(presets[0].check_topology(&topo).is_err());
    let saved_gains: Vec<_> = presets.iter().map(|p| p.gains.clone()).collect();
    assert_eq!(presets::bind_legacy(&mut presets, &topo), 1);
    assert!(presets[0].check_topology(&topo).is_ok());
    assert!(presets[1].topology.is_none());
    assert_eq!(
        presets.iter().map(|p| p.gains.clone()).collect::<Vec<_>>(),
        saved_gains
    );
    let serialized = presets::serialize_user_presets(&presets);
    assert!(serialized.contains("Écoute = 1;2;3;4;5;6;7;8"));
    assert_eq!(presets::parse_user_presets(&serialized), presets);
}

#[test]
fn same_band_count_does_not_make_different_frequencies_compatible() {
    let topo = topology();
    let mut list = Vec::new();
    presets::upsert_for(&mut list, "Essai", &[0; 8], &topo).unwrap();
    assert!(list[0].check_topology(&topo).is_ok());
    let mut changed = topo.clone();
    changed.frequencies_hz.swap(0, 1);
    assert!(list[0].check_topology(&changed).is_err());
    changed = topo.clone();
    changed.max_gain = 30;
    assert!(list[0].check_topology(&changed).is_err());
    changed = topo;
    changed.model = "Autre modèle".into();
    assert!(list[0].check_topology(&changed).is_err());
}

#[test]
fn malformed_metadata_and_out_of_range_curves_are_not_auto_qualified() {
    let list = presets::parse_user_presets("# topologie = invalide\nEssai = 0;0;0;0;0;0;0;0\n");
    assert!(list[0].check_topology(&topology()).is_err());
    let mut list = Vec::new();
    assert!(presets::upsert_for(&mut list, "Essai", &[127; 8], &topology()).is_err());
    assert!(list.is_empty());
}

#[test]
fn ab_refuses_a_changed_topology_and_keeps_both_curves_otherwise() {
    let topo = topology();
    let mut ab = AbCompare::default();
    ab.remember_for(&topo, &[0; 8]).unwrap();
    assert_eq!(ab.toggle_for(&topo, &[10; 8]).unwrap(), Some(vec![0; 8]));
    assert_eq!(ab.toggle_for(&topo, &[0; 8]).unwrap(), Some(vec![10; 8]));
    let mut changed = topo;
    changed.frequencies_hz[0] = 100;
    assert!(ab.toggle_for(&changed, &[10; 8]).is_err());
    assert!(!ab.ready());
}

#[test]
fn unsupported_band_counts_do_not_get_m5_frequencies() {
    assert!(
        EqTopology::momentum5(&EqState {
            gains: vec![0; 7],
            ..EqState::default()
        })
        .is_none()
    );
}

#[test]
fn a_legacy_preset_refuses_a_scene_before_any_hardware_exchange() {
    use momentum_core::{
        device::Headset,
        scenes::{self, Scene, SceneSound},
        transport::MockTransport,
    };
    let mut headset =
        Headset::validated_for_tests(MockTransport::new(), std::time::Duration::from_millis(10));
    let scene = Scene {
        name: "Essai".into(),
        sound: Some(SceneSound::Equalizer),
        equalizer: Some("Ancien".into()),
        ..Scene::default()
    };
    let list = presets::parse_user_presets("Ancien = 0;0;0;0;0;0;0;0\n");
    let report = scenes::apply_scene(&mut headset, &scene, &list);
    assert!(report.failed.is_some());
    assert!(report.applied.is_empty());
    assert!(headset.client().transport().sent().is_empty());
}
