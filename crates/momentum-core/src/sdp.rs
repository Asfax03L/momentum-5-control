//! Lecture des attributs SDP bruts (Bluetooth Core, vol. 3, partie B, §3).
//!
//! Windows renvoie chaque attribut SDP sous forme de « data element » brut.
//! On en extrait seulement ce qui sert au diagnostic : le nom du service
//! (attribut 0x0100) et le canal RFCOMM (attribut 0x0004).

use thiserror::Error;

/// Attribut « ServiceName » (base de langue primaire 0x0100 + offset 0).
pub const ATTR_SERVICE_NAME: u32 = 0x0100;
/// Attribut « ProtocolDescriptorList ».
pub const ATTR_PROTOCOL_DESCRIPTOR_LIST: u32 = 0x0004;
/// UUID court du protocole RFCOMM (Assigned Numbers Bluetooth SIG).
pub const UUID_RFCOMM: u16 = 0x0003;

const MAX_DEPTH: usize = 16;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SdpError {
    #[error("data element tronqué")]
    Truncated,
    #[error("taille invalide pour le type {0}")]
    BadSize(u8),
    #[error("type de data element réservé : {0}")]
    ReservedType(u8),
    #[error("imbrication trop profonde")]
    TooDeep,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SdpUuid {
    Short16(u16),
    Short32(u32),
    Full(u128),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataElement {
    Nil,
    Uint(u128),
    Int(i128),
    Uuid(SdpUuid),
    Text(Vec<u8>),
    Bool(bool),
    Sequence(Vec<DataElement>),
    Alternative(Vec<DataElement>),
    Url(Vec<u8>),
}

impl DataElement {
    /// Décode un data element complet (les octets en trop sont ignorés).
    pub fn parse(bytes: &[u8]) -> Result<DataElement, SdpError> {
        parse_at(bytes, 0).map(|(el, _)| el)
    }

    pub fn as_text(&self) -> Option<String> {
        match self {
            DataElement::Text(t) | DataElement::Url(t) => {
                // Certains appareils terminent la chaîne par \0.
                let end = t.iter().position(|&b| b == 0).unwrap_or(t.len());
                Some(String::from_utf8_lossy(&t[..end]).into_owned())
            }
            _ => None,
        }
    }
}

/// Lit un entier big-endian de `n` octets (n ≤ 16).
fn read_uint(bytes: &[u8]) -> u128 {
    bytes
        .iter()
        .fold(0u128, |acc, &b| (acc << 8) | u128::from(b))
}

fn take(bytes: &[u8], start: usize, n: usize) -> Result<&[u8], SdpError> {
    bytes
        .get(start..start.checked_add(n).ok_or(SdpError::Truncated)?)
        .ok_or(SdpError::Truncated)
}

/// Renvoie l'élément et le nombre d'octets consommés.
fn parse_at(bytes: &[u8], depth: usize) -> Result<(DataElement, usize), SdpError> {
    if depth > MAX_DEPTH {
        return Err(SdpError::TooDeep);
    }
    let header = *bytes.first().ok_or(SdpError::Truncated)?;
    let ty = header >> 3;
    let size_index = header & 0x07;

    // Indices 0..=4 : taille fixe 1, 2, 4, 8, 16 octets.
    // Indices 5..=7 : la taille suit sur 1, 2 ou 4 octets.
    let (data_len, header_len) = match size_index {
        0..=4 => (1usize << size_index, 1usize),
        5 => (read_uint(take(bytes, 1, 1)?) as usize, 2),
        6 => (read_uint(take(bytes, 1, 2)?) as usize, 3),
        _ => (read_uint(take(bytes, 1, 4)?) as usize, 5),
    };
    // Le type Nil n'a pas de données.
    let data_len = if ty == 0 { 0 } else { data_len };
    let data = take(bytes, header_len, data_len)?;
    let used = header_len + data_len;

    let element = match ty {
        0 => DataElement::Nil,
        1 => {
            if size_index > 4 {
                return Err(SdpError::BadSize(ty));
            }
            DataElement::Uint(read_uint(data))
        }
        2 => {
            if size_index > 4 {
                return Err(SdpError::BadSize(ty));
            }
            // Extension de signe depuis la taille réelle.
            let bits = data_len * 8;
            let raw = read_uint(data);
            let value = if bits < 128 && raw & (1u128 << (bits - 1)) != 0 {
                (raw | (!0u128 << bits)) as i128
            } else {
                raw as i128
            };
            DataElement::Int(value)
        }
        3 => DataElement::Uuid(match data_len {
            2 => SdpUuid::Short16(read_uint(data) as u16),
            4 => SdpUuid::Short32(read_uint(data) as u32),
            16 => SdpUuid::Full(read_uint(data)),
            _ => return Err(SdpError::BadSize(ty)),
        }),
        4 => DataElement::Text(data.to_vec()),
        5 => DataElement::Bool(data.first().is_some_and(|&b| b != 0)),
        6 | 7 => {
            let mut items = Vec::new();
            let mut offset = 0;
            while offset < data.len() {
                let (item, n) = parse_at(&data[offset..], depth + 1)?;
                items.push(item);
                offset += n;
            }
            if ty == 6 {
                DataElement::Sequence(items)
            } else {
                DataElement::Alternative(items)
            }
        }
        8 => DataElement::Url(data.to_vec()),
        other => return Err(SdpError::ReservedType(other)),
    };
    Ok((element, used))
}

/// Extrait le canal RFCOMM d'une ProtocolDescriptorList.
///
/// Structure attendue : `seq( seq(L2CAP), seq(UUID RFCOMM 0x0003, uint8 canal) )`.
pub fn rfcomm_channel(protocol_list: &DataElement) -> Option<u8> {
    let DataElement::Sequence(protocols) = protocol_list else {
        return None;
    };
    protocols.iter().find_map(|proto| match proto {
        DataElement::Sequence(items) => match items.as_slice() {
            [
                DataElement::Uuid(SdpUuid::Short16(UUID_RFCOMM)),
                DataElement::Uint(ch),
                ..,
            ] => u8::try_from(*ch).ok(),
            _ => None,
        },
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_name() {
        // Type 4 (texte), taille sur 1 octet (index 5) : 0x25, longueur 5.
        let raw = [0x25, 0x05, b'G', b'A', b'I', b'A', 0x00];
        let el = DataElement::parse(&raw).unwrap();
        assert_eq!(el.as_text().as_deref(), Some("GAIA"));
    }

    #[test]
    fn parses_rfcomm_channel_15() {
        // seq { seq { uuid16 0x0100 (L2CAP) }, seq { uuid16 0x0003, uint8 15 } }
        let raw = [
            0x35, 0x0C, // séquence, 12 octets
            0x35, 0x03, 0x19, 0x01, 0x00, // L2CAP
            0x35, 0x05, 0x19, 0x00, 0x03, 0x08, 0x0F, // RFCOMM canal 15
        ];
        let el = DataElement::parse(&raw).unwrap();
        assert_eq!(rfcomm_channel(&el), Some(15));
    }

    #[test]
    fn parses_uuid128_and_negative_int() {
        let mut raw = vec![0x1C];
        raw.extend_from_slice(&[0x11; 16]);
        assert_eq!(
            DataElement::parse(&raw).unwrap(),
            DataElement::Uuid(SdpUuid::Full(0x1111_1111_1111_1111_1111_1111_1111_1111))
        );
        assert_eq!(
            DataElement::parse(&[0x10, 0xFF]).unwrap(),
            DataElement::Int(-1)
        );
    }

    #[test]
    fn rejects_truncated_input() {
        assert_eq!(
            DataElement::parse(&[0x35, 0x05, 0x19]),
            Err(SdpError::Truncated)
        );
        assert_eq!(DataElement::parse(&[]), Err(SdpError::Truncated));
        assert_eq!(
            DataElement::parse(&[0x48, 0x00]),
            Err(SdpError::ReservedType(9))
        );
    }
}
