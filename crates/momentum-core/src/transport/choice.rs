//! Choix du casque parmi les appareils connectés qui annoncent le service
//! GAIA (F04 de la revue). Le nom est un libellé que l'utilisateur peut
//! changer (`ctl nom`) : il ne sert qu'à départager, jamais à exclure. Le
//! casque déjà utilisé est retrouvé par l'empreinte de son identifiant
//! système, gardée dans `casque.txt` sans l'adresse en clair.

/// Appareil candidat : identifiant système stable (adresse ou identifiant
/// Windows, jamais affiché ni journalisé) et nom affiché.
#[derive(Clone, Copy, Debug)]
pub struct Candidate<'a> {
    pub key: &'a str,
    pub name: &'a str,
}

/// Résultat du choix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Choice {
    /// Position du candidat retenu.
    One(usize),
    /// Aucun appareil connecté n'annonce GAIA.
    None,
    /// Plusieurs candidats sans casque mémorisé ni nom distinctif : aucun
    /// n'est pris au hasard (noms des candidats).
    Ambiguous(Vec<String>),
}

/// Empreinte (FNV-1a 64 bits, stable d'une version à l'autre) d'un
/// identifiant d'appareil : permet de le reconnaître sans le stocker.
pub fn fingerprint(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.trim().to_ascii_uppercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Choisit le casque : celui mémorisé (`remembered`, empreinte) s'il est
/// là ; sinon le seul candidat ; sinon le seul dont le nom contient l'un
/// des `name_hints` (insensible à la casse).
pub fn choose(candidates: &[Candidate], remembered: Option<&str>, name_hints: &[&str]) -> Choice {
    if let Some(known) = remembered
        && let Some(i) = candidates.iter().position(|c| fingerprint(c.key) == known)
    {
        return Choice::One(i);
    }
    match candidates {
        [] => Choice::None,
        [_] => Choice::One(0),
        _ => {
            let hinted: Vec<usize> = candidates
                .iter()
                .enumerate()
                .filter(|(_, c)| {
                    let lower = c.name.to_lowercase();
                    name_hints.iter().any(|h| lower.contains(&h.to_lowercase()))
                })
                .map(|(i, _)| i)
                .collect();
            match hinted[..] {
                [only] => Choice::One(only),
                _ => Choice::Ambiguous(candidates.iter().map(|c| c.name.to_string()).collect()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HINTS: [&str; 2] = ["momentum", "sennheiser"];

    fn c<'a>(key: &'a str, name: &'a str) -> Candidate<'a> {
        Candidate { key, name }
    }

    #[test]
    fn a_renamed_headset_is_still_found() {
        // Seul appareil GAIA connecté : retenu, quel que soit son nom.
        assert_eq!(
            choose(&[c("02:00:00:00:00:01", "Studio")], None, &HINTS),
            Choice::One(0)
        );
        // Mémorisé : retenu même renommé, même avec un autre casque présent.
        let known = fingerprint("02:00:00:00:00:01");
        let both = [
            c("02:00:00:00:00:02", "MOMENTUM 4"),
            c("02:00:00:00:00:01", "Studio"),
        ];
        assert_eq!(choose(&both, Some(&known), &HINTS), Choice::One(1));
    }

    #[test]
    fn several_candidates_are_never_picked_at_random() {
        let both = [
            c("02:00:00:00:00:01", "Studio"),
            c("02:00:00:00:00:02", "Bureau"),
        ];
        assert_eq!(
            choose(&both, None, &HINTS),
            Choice::Ambiguous(vec!["Studio".into(), "Bureau".into()])
        );
        // Un seul nom distinctif départage ; deux ne départagent pas.
        let hinted = [
            c("02:00:00:00:00:01", "Studio"),
            c("02:00:00:00:00:02", "MOMENTUM 5"),
        ];
        assert_eq!(choose(&hinted, None, &HINTS), Choice::One(1));
        let twins = [c("a", "MOMENTUM 5"), c("b", "MOMENTUM 5")];
        assert!(matches!(choose(&twins, None, &HINTS), Choice::Ambiguous(_)));
        // Mémorisé mais absent : règles ordinaires.
        assert!(matches!(
            choose(&twins, Some(&fingerprint("c")), &HINTS),
            Choice::Ambiguous(_)
        ));
        assert_eq!(choose(&[], None, &HINTS), Choice::None);
    }

    #[test]
    fn the_fingerprint_hides_the_address_and_is_stable() {
        let f = fingerprint("02:00:00:00:00:01");
        assert_eq!(f, fingerprint(" 02:00:00:00:00:01 "));
        assert_eq!(f, fingerprint("02:00:00:00:00:01".to_lowercase().as_str()));
        assert_eq!(f.len(), 16);
        assert!(!f.contains(':'));
        assert_ne!(f, fingerprint("02:00:00:00:00:02"));
    }
}
