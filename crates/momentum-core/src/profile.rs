//! Export local, en lecture seule, du profil de personnalisation du son.
//!
//! Les trois blocs `2003`/`2005`/`2007` sont conservés **octet pour octet** :
//! c'est la seule forme fiable d'une sauvegarde. Les flottants big-endian
//! ne servent qu'à l'affichage (leur sens exact — seuils, rapports, gains —
//! reste une hypothèse). Aucune restauration n'est proposée : réécrire un
//! profil n'a pas été qualifié sur le matériel.
//!
//! L'export ne contient ni numéro de série ni adresse Bluetooth, mais le
//! profil lui-même est une donnée personnelle (audition) : à ne pas publier.

use crate::hex::to_hex;
use crate::protocol::CommandId;
use crate::protocol::messages::f32_block;

/// Version du format d'export.
pub const EXPORT_FORMAT: u32 = 1;

/// Un bloc du profil tel que lu sur le casque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileBlock {
    pub command: CommandId,
    pub payload: Vec<u8>,
}

/// Instantané du profil et de son contexte, sans identifiant d'appareil.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileExport {
    /// Date de lecture, en secondes depuis 1970 (UTC).
    pub unix_time: u64,
    pub model: Option<String>,
    pub firmware: Option<String>,
    /// Version de la feature 16 annoncée par le catalogue Sennheiser
    /// (`0495:0001`), si elle a pu être lue.
    pub feature_version: Option<u8>,
    /// Octets bruts de `2001` (état du profil) et `0804` (mode audio).
    pub status_raw: Option<Vec<u8>>,
    pub audio_mode_raw: Option<Vec<u8>>,
    pub blocks: Vec<ProfileBlock>,
}

impl ProfileExport {
    /// Empreinte FNV-1a 64 bits des blocs (id, longueur, octets). Sert à
    /// repérer un changement de profil ; ce n'est pas une signature.
    pub fn fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |byte: u8| {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };
        for block in &self.blocks {
            block
                .command
                .raw()
                .to_be_bytes()
                .into_iter()
                .for_each(&mut eat);
            // Au plus 254 octets par trame : la longueur tient sur 2 octets.
            (block.payload.len() as u16)
                .to_be_bytes()
                .into_iter()
                .for_each(&mut eat);
            block.payload.iter().copied().for_each(&mut eat);
        }
        hash
    }

    /// Sérialise en JSON (écrit à la main : pas de dépendance de plus).
    pub fn to_json(&self) -> String {
        let opt_str = |v: &Option<String>| v.as_deref().map_or("null".into(), json_string);
        let opt_hex = |v: &Option<Vec<u8>>| {
            v.as_deref()
                .map_or("null".into(), |b| json_string(&to_hex(b)))
        };
        let mut out = String::from("{\n");
        out += &format!("  \"format\": {EXPORT_FORMAT},\n");
        out += &format!(
            "  \"avertissement\": {},\n",
            json_string(
                "Profil auditif personnel. Sauvegarde brute en lecture seule : \
                 aucune restauration n'a été validée sur le matériel."
            )
        );
        out += &format!("  \"date_unix\": {},\n", self.unix_time);
        out += &format!(
            "  \"date_utc\": {},\n",
            json_string(&iso_utc(self.unix_time))
        );
        out += &format!("  \"modele\": {},\n", opt_str(&self.model));
        out += &format!("  \"firmware\": {},\n", opt_str(&self.firmware));
        out += &format!(
            "  \"version_feature_16\": {},\n",
            self.feature_version
                .map_or("null".into(), |v| v.to_string())
        );
        out += &format!("  \"etat_2001_brut\": {},\n", opt_hex(&self.status_raw));
        out += &format!(
            "  \"mode_audio_0804_brut\": {},\n",
            opt_hex(&self.audio_mode_raw)
        );
        out += &format!(
            "  \"empreinte_fnv1a64\": \"{:016x}\",\n",
            self.fingerprint()
        );
        out += "  \"blocs\": [\n";
        for (i, block) in self.blocks.iter().enumerate() {
            let (values, rest) = f32_block(&block.payload);
            let floats: Vec<String> = values
                .iter()
                .map(|v| {
                    if v.is_finite() {
                        format!("{v}")
                    } else {
                        "null".into()
                    }
                })
                .collect();
            out += "    {\n";
            out += &format!("      \"commande\": \"{}\",\n", block.command);
            out += &format!("      \"longueur\": {},\n", block.payload.len());
            out += &format!(
                "      \"octets\": {},\n",
                json_string(&to_hex(&block.payload))
            );
            out += &format!(
                "      \"flottants_be_affichage\": [{}],\n",
                floats.join(", ")
            );
            out += &format!(
                "      \"octets_restants\": {}\n",
                json_string(&to_hex(rest))
            );
            out += if i + 1 == self.blocks.len() {
                "    }\n"
            } else {
                "    },\n"
            };
        }
        out += "  ]\n}\n";
        out
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Horodatage pour un nom de fichier : `20261009-143205` (UTC).
pub fn file_stamp(unix: u64) -> String {
    iso_utc(unix)
        .trim_end_matches('Z')
        .replace(['-', ':'], "")
        .replace('T', "-")
}

/// Date ISO 8601 UTC (algorithme « days to civil » de H. Hinnant).
pub fn iso_utc(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let secs = unix % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs / 3_600,
        secs % 3_600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::from_hex;
    use crate::protocol::command::get;

    fn sample() -> ProfileExport {
        ProfileExport {
            unix_time: 1_791_504_000,
            model: Some("MOMENTUM 5".into()),
            firmware: Some("6.21.5".into()),
            feature_version: Some(2),
            status_raw: Some(vec![2]),
            audio_mode_raw: Some(vec![0, 1]),
            blocks: vec![ProfileBlock {
                command: get::SOUND_PROFILE_3.response(),
                payload: from_hex("BF 66 66 66 BF 66 66 66 BF 66 66 66 05 05").unwrap(),
            }],
        }
    }

    #[test]
    fn export_keeps_the_original_bytes() {
        let json = sample().to_json();
        assert!(json.contains("\"octets\": \"BF 66 66 66 BF 66 66 66 BF 66 66 66 05 05\""));
        assert!(json.contains("\"longueur\": 14"));
        assert!(json.contains("\"octets_restants\": \"05 05\""));
        assert!(json.contains("\"date_utc\": \"2026-10-09T00:00:00Z\""));
        assert!(json.contains("\"etat_2001_brut\": \"02\""));
    }

    #[test]
    fn fingerprint_changes_with_any_byte() {
        let a = sample();
        let mut b = sample();
        b.blocks[0].payload[13] = 6;
        assert_ne!(a.fingerprint(), b.fingerprint());
        // Le contexte (date, mode) ne change pas l'empreinte du profil.
        let mut c = sample();
        c.unix_time += 60;
        c.audio_mode_raw = Some(vec![0, 2]);
        assert_eq!(a.fingerprint(), c.fingerprint());
    }

    #[test]
    fn file_stamp_is_sortable() {
        assert_eq!(file_stamp(1_791_504_000 + 52_325), "20261009-143205");
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(json_string("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }
}
