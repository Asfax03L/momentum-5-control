//! Affichage hexadécimal des trames (« FF 03 00 00 … »).

/// Formate des octets en hexadécimal majuscule séparé par des espaces.
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{b:02X}"));
    }
    out
}

/// Lit une chaîne hexadécimale (espaces ignorés). `None` si invalide.
pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let digits: Vec<u8> = text.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let (pairs, rest) = digits.as_chunks::<2>();
    if !rest.is_empty() {
        return None;
    }
    pairs
        .iter()
        .map(|pair| {
            let s = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(s, 16).ok()
        })
        .collect()
}

/// Contenu d'une trame pour les journaux ordinaires (F10 de la revue) : les
/// réglages tiennent en 3 octets au plus ; au-delà (nom du casque, adresses
/// et noms d'appareils, profil), seule la taille est écrite. Les trames
/// brutes complètes ne vont qu'au niveau `trace`, activé volontairement.
pub fn loggable(payload: &[u8]) -> String {
    if payload.len() <= 3 {
        to_hex(payload)
    } else {
        format!("<{} octets masqués>", payload.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_payloads_are_hidden_from_ordinary_logs() {
        assert_eq!(loggable(&[0x01, 0x02]), "01 02");
        // « MOMENTUM 5 x » : le nom n'apparaît pas.
        assert_eq!(loggable(b"MOMENTUM 5 x"), "<12 octets masqués>");
    }

    #[test]
    fn round_trip() {
        let bytes = [0xFF, 0x03, 0x00, 0x2A];
        assert_eq!(to_hex(&bytes), "FF 03 00 2A");
        assert_eq!(from_hex("ff03 00 2a"), Some(bytes.to_vec()));
        assert_eq!(from_hex("F"), None);
        assert_eq!(from_hex("ZZ"), None);
    }
}
