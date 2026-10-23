#![no_main]

use libfuzzer_sys::fuzz_target;
use momentum_core::sdp::{DataElement, rfcomm_channel};
use std::hint::black_box;

fuzz_target!(|data: &[u8]| {
    if let Ok(element) = DataElement::parse(data) {
        // Utiliser aussi le résultat, y compris les chaînes non UTF-8 et
        // les listes de protocoles ; aucun objet Bluetooth n'est créé.
        black_box(element.as_text());
        black_box(rfcomm_channel(&element));
    }
});
