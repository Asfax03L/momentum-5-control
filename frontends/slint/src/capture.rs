//! Captures de vérification (`--capture <dossier>`, mode démonstration) :
//! la fenêtre est rendue à plusieurs tailles, pages et thèmes, et chaque
//! rendu est enregistré en PNG. Sert à vérifier la disposition adaptative
//! sans casque ni manipulation à la main.

use std::io::{self, Write};
use std::path::Path;

/// Une capture : nom du fichier, taille logique, page, thème.
pub struct Shot {
    pub name: &'static str,
    pub width: f32,
    pub height: f32,
    pub page: i32,
    pub dark: bool,
}

/// Tailles représentatives : grand écran, fenêtre intermédiaire, mobile.
pub const SHOTS: &[Shot] = &[
    Shot {
        name: "01-large-accueil-sombre",
        width: 1440.0,
        height: 960.0,
        page: 0,
        dark: true,
    },
    Shot {
        name: "02-large-accueil-clair",
        width: 1440.0,
        height: 960.0,
        page: 0,
        dark: false,
    },
    Shot {
        name: "03-intermediaire-accueil",
        width: 1000.0,
        height: 860.0,
        page: 0,
        dark: true,
    },
    Shot {
        name: "04-etroit-accueil",
        width: 760.0,
        height: 900.0,
        page: 0,
        dark: true,
    },
    Shot {
        name: "05-mobile-accueil",
        width: 400.0,
        height: 860.0,
        page: 0,
        dark: true,
    },
    Shot {
        name: "06-large-egaliseur",
        width: 1280.0,
        height: 900.0,
        page: 1,
        dark: true,
    },
    Shot {
        name: "07-mobile-egaliseur",
        width: 400.0,
        height: 860.0,
        page: 1,
        dark: false,
    },
    Shot {
        name: "08-intermediaire-parametres",
        width: 960.0,
        height: 860.0,
        page: 2,
        dark: true,
    },
    Shot {
        name: "09-large-appareil",
        width: 1280.0,
        height: 900.0,
        page: 4,
        dark: true,
    },
    Shot {
        name: "10-large-batterie",
        width: 1280.0,
        height: 900.0,
        page: 3,
        dark: false,
    },
    Shot {
        name: "11-large-commandes",
        width: 1100.0,
        height: 860.0,
        page: 7,
        dark: true,
    },
    Shot {
        name: "12-mobile-scenes",
        width: 400.0,
        height: 860.0,
        page: 5,
        dark: true,
    },
    Shot {
        name: "13-large-application",
        width: 1280.0,
        height: 900.0,
        page: 6,
        dark: true,
    },
];

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(data.len() + 4);
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// PNG RGBA 8 bits, sans compression (blocs « stored » de deflate) :
/// volumineux mais sans dépendance.
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks(row).take(height as usize) {
        raw.push(0); // filtre « aucun »
        raw.extend_from_slice(line);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(i + 1 == blocks.len()));
        let len = block.len() as u16;
        zlib.extend_from_slice(&len.to_le_bytes());
        zlib.extend_from_slice(&(!len).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

pub fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> io::Result<()> {
    std::fs::File::create(path)?.write_all(&encode_png(width, height, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_and_adler_match_reference_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn png_has_signature_header_and_end() {
        let png = encode_png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert_eq!(&png[12..16], b"IHDR");
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
    }
}
