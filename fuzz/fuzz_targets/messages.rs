#![no_main]

use libfuzzer_sys::fuzz_target;
use momentum_core::protocol::command::get;
use momentum_core::protocol::messages::*;
use std::hint::black_box;

fuzz_target!(|payload: &[u8]| {
    // Toutes les fonctions parse_* publiques reçoivent le même payload
    // arbitraire ; black_box empêche de supprimer les résultats inutilisés.
    let _ = black_box(parse_battery(payload));
    let _ = black_box(parse_on_head_state(payload));
    let _ = black_box(parse_codec(payload));
    let _ = black_box(parse_paired_device(payload));
    let _ = black_box(parse_sample_rate(payload));
    let _ = black_box(parse_model_name(payload));
    let _ = black_box(parse_firmware_version(payload));
    let _ = black_box(parse_noise_control_modes(payload));
    let _ = black_box(parse_eq_config(payload));
    let _ = black_box(parse_eq_gain(payload));
    let _ = black_box(parse_band_table(payload));
    let _ = black_box(parse_core_features(payload));
    let _ = black_box(parse_switch(get::ANC_ENABLED, payload));
    black_box(f32_block(payload));
});
