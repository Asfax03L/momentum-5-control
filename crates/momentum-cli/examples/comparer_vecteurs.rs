//! Compare le décodage des trames de ce projet à un fichier de vecteurs
//! publics au format de jarek102/ohr (`{"version": 1, "vectors": [...]}`).
//! Outil de développement, jamais livré : n'ouvre aucune connexion et ne lit
//! que le fichier donné. Les vecteurs ne sont pas copiés dans ce dépôt.
//!
//! - `frame` : la trame se décode, son payload et sa ré-encodage sont identiques ;
//! - `stream` : les morceaux livrés dans l'ordre donnent les mêmes trames et le
//!   même reste en tampon.
//!
//! Les autres sortes (`payload`, `sequence`) sont comptées, pas vérifiées.
//!
//! Usage : cargo run -p momentum-cli --example comparer_vecteurs -- <framing.json>

use momentum_core::hex::{from_hex, to_hex};
use momentum_core::protocol::{Frame, FrameDecoder};
use serde_json::Value;

fn bytes(v: &Value) -> Vec<u8> {
    from_hex(v.as_str().unwrap_or("")).unwrap_or_default()
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage : comparer_vecteurs <fichier.json>");
        std::process::exit(2);
    };
    let text = std::fs::read_to_string(&path).expect("fichier de vecteurs");
    let root: Value = serde_json::from_str(&text).expect("JSON");
    let (mut ok, mut ko, mut skipped) = (0, 0, 0);
    for vector in root["vectors"].as_array().into_iter().flatten() {
        let id = vector["id"].as_str().unwrap_or("?");
        let verdict = match vector["kind"].as_str() {
            Some("frame") => {
                let wire = bytes(&vector["bytes"]);
                match Frame::decode(&wire) {
                    Ok((frame, used)) => {
                        let payload = bytes(&vector["payload"]);
                        let again = frame.encode().unwrap_or_default();
                        if used != wire.len() {
                            Some(format!("{used} octets lus sur {}", wire.len()))
                        } else if frame.payload != payload {
                            Some(format!("payload {}", to_hex(&frame.payload)))
                        } else if again != wire {
                            Some(format!("ré-encodé {}", to_hex(&again)))
                        } else {
                            None
                        }
                    }
                    Err(e) => Some(format!("refusée : {e}")),
                }
            }
            Some("stream") => {
                let mut decoder = FrameDecoder::new();
                let mut frames = Vec::new();
                for chunk in vector["chunks"].as_array().into_iter().flatten() {
                    decoder.push(&bytes(chunk));
                    while let Some(result) = decoder.next_frame() {
                        if let Ok(frame) = result {
                            frames.push(frame.encode().unwrap_or_default());
                        }
                    }
                }
                let expected: Vec<Vec<u8>> = vector["frames"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(bytes)
                    .collect();
                let remainder = bytes(&vector["remainder"]);
                if frames != expected {
                    Some(format!(
                        "{} trames au lieu de {}",
                        frames.len(),
                        expected.len()
                    ))
                } else if decoder.pending() != remainder.as_slice() {
                    Some(format!("reste {}", to_hex(decoder.pending())))
                } else {
                    None
                }
            }
            _ => {
                skipped += 1;
                continue;
            }
        };
        match verdict {
            None => ok += 1,
            Some(why) => {
                ko += 1;
                println!("ÉCART {id} : {why}");
            }
        }
    }
    println!("{ok} conformes, {ko} écarts, {skipped} non vérifiés (autres sortes)");
}
