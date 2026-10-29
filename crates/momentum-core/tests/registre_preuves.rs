//! Contrôle du registre des preuves (`docs/preuves/registre-m5.json`,
//! proposition R01 de Codex) : chaque entrée a une nature connue et la
//! source qui va avec, cite des tests qui existent, et le registre reste
//! cohérent avec `docs/MOMENTUM5.md`. Aucun accès au casque.

use std::path::{Path, PathBuf};

use serde_json::Value;

fn racine() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registre() -> Value {
    let texte = std::fs::read_to_string(racine().join("docs/preuves/registre-m5.json"))
        .expect("registre des preuves");
    serde_json::from_str(&texte).expect("registre JSON valide")
}

fn texte<'a>(v: &'a Value, cle: &str) -> &'a str {
    v.get(cle)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("« {cle} » manquant dans {v}"))
}

fn hex_exact(s: &str, longueur: usize) -> bool {
    s.len() == longueur && s.chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn chaque_entree_a_une_source_conforme_a_sa_nature() {
    let r = registre();
    assert_eq!(r["version"], 1);
    let entrees = r["entrees"].as_array().expect("entrées");
    assert!(!entrees.is_empty());
    let mut ids = std::collections::HashSet::new();
    for e in entrees {
        let id = texte(e, "id");
        assert!(ids.insert(id), "identifiant en double : {id}");
        assert!(hex_exact(texte(e, "commande"), 4), "{id} : commande");
        let source = &e["source"];
        match texte(e, "nature") {
            "capture_app" => {
                assert!(
                    hex_exact(texte(source, "capture_sha256"), 64),
                    "{id} : empreinte"
                );
                let records = source["records"].as_array().expect("records");
                assert!(
                    !records.is_empty() && records.iter().all(Value::is_u64),
                    "{id} : records"
                );
            }
            "essai_pc" => {
                for cle in ["date", "systeme", "outil", "commit", "resultat"] {
                    assert!(!texte(source, cle).is_empty(), "{id} : {cle}");
                }
                assert_eq!(source["journal_prive"], true, "{id} : journal privé");
                assert!(
                    racine().join(texte(source, "outil")).is_file(),
                    "{id} : outil absent"
                );
            }
            "synthetique" => {
                // Un cas synthétique ne se présente jamais comme une capture.
                assert!(source.get("capture_sha256").is_none(), "{id}");
                assert!(source.get("records").is_none(), "{id}");
            }
            autre => panic!("{id} : nature inconnue « {autre} »"),
        }
        let tests = e["tests"].as_array().expect("tests");
        assert!(!tests.is_empty(), "{id} : aucun test cité");
        for t in tests {
            let chemin = t.as_str().expect("chemin");
            assert!(
                racine().join(chemin).is_file(),
                "{id} : test absent {chemin}"
            );
        }
    }
}

#[test]
fn les_empreintes_publiees_sont_celles_de_la_documentation() {
    let r = registre();
    let doc = std::fs::read_to_string(racine().join("docs/MOMENTUM5.md")).unwrap();
    for e in r["entrees"].as_array().unwrap() {
        if let Some(sha) = e["source"].get("capture_sha256").and_then(Value::as_str) {
            assert!(
                doc.contains(sha),
                "{} : empreinte absente de MOMENTUM5.md",
                e["id"]
            );
        }
    }
}

#[test]
fn chaque_fonction_dit_ce_qui_reste_a_valider() {
    // Une fonction sans essai PC sur un système doit le déclarer, pour ne
    // jamais présenter une capture du téléphone comme un essai depuis le PC.
    let r = registre();
    let a_valider: Vec<&str> = r["a_valider_sur_le_materiel"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| texte(v, "fonction"))
        .collect();
    for e in r["entrees"].as_array().unwrap() {
        let fonction = texte(e, "fonction");
        assert!(
            a_valider.contains(&fonction),
            "{fonction} : rien n'est déclaré à valider"
        );
    }
}

#[test]
fn aucune_adresse_ni_chemin_personnel() {
    let texte = std::fs::read_to_string(racine().join("docs/preuves/registre-m5.json")).unwrap();
    let octets: Vec<&str> = texte
        .split(|c: char| !c.is_ascii_hexdigit() && c != ':')
        .collect();
    for mot in octets {
        let parties: Vec<&str> = mot.split(':').collect();
        let adresse = parties.len() == 6 && parties.iter().all(|p| hex_exact(p, 2));
        assert!(
            !adresse || mot == "XX:XX:XX:XX:XX:XX",
            "adresse Bluetooth : {mot}"
        );
    }
    for interdit in ["C:\\\\Users", "/home/", "Poldi"] {
        assert!(!texte.contains(interdit), "{interdit}");
    }
}
