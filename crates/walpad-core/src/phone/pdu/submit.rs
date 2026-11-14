use super::codec::*;

/// One part ready for `AT+CMGS=<tpdu_len>`: `hex` is the whole PDU including the SMSC prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub hex: String,
    /// Length of the TPDU in octets (the PDU without the SMSC field): the CMGS argument.
    pub tpdu_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    BadNumber,
    TooLong,
}

/// Which alphabet a text needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    Gsm7,
    Ucs2,
}

pub fn alphabet(text: &str) -> Alphabet {
    if gsm7_encode(text).is_some() {
        Alphabet::Gsm7
    } else {
        Alphabet::Ucs2
    }
}

/// The SMSC field: `00` = the modem's default; otherwise length, type, swapped digits.
fn smsc_field(smsc: Option<&str>) -> Vec<u8> {
    let Some(s) = smsc.filter(|s| !s.is_empty()) else {
        return vec![0];
    };
    let (ton, digits) = match s.strip_prefix('+') {
        Some(d) => (0x91, d),
        None => (0x81, s),
    };
    match bcd_encode(digits) {
        Some(bcd) if !digits.is_empty() && bcd.len() < 11 => {
            let mut v = vec![(bcd.len() + 1) as u8, ton];
            v.extend(bcd);
            v
        }
        _ => vec![0],
    }
}

/// Encode `text` for `number` as one or more SMS-SUBMIT PDUs. `reference` numbers the parts of a
/// long message (the same value in every part).
pub fn encode_submit(
    smsc: Option<&str>,
    number: &str,
    text: &str,
    reference: u8,
) -> Result<Vec<Part>, EncodeError> {
    let (ton, digits) = match number.strip_prefix('+') {
        Some(d) => (0x91u8, d),
        None => (0x81u8, number),
    };
    if digits.is_empty() || digits.len() > 20 {
        return Err(EncodeError::BadNumber);
    }
    let da = bcd_encode(digits).ok_or(EncodeError::BadNumber)?;
    // the units a part carries: septets (escapes count 2) or UTF-16 units
    enum Units {
        Gsm(Vec<u8>),
        Ucs(Vec<u16>),
    }
    let units = match gsm7_encode(text) {
        Some(s) => Units::Gsm(s),
        None => Units::Ucs(text.encode_utf16().collect()),
    };
    let (len, single, per) = match &units {
        Units::Gsm(s) => (s.len(), SINGLE_GSM7, PART_GSM7),
        Units::Ucs(u) => (u.len(), SINGLE_UCS2, PART_UCS2),
    };
    if len == 0 {
        return Err(EncodeError::TooLong);
    }
    // split into chunks of whole characters: never cut an escape pair or a surrogate pair
    let mut chunks: Vec<std::ops::Range<usize>> = Vec::new();
    if len <= single {
        chunks.push(0..len);
    } else {
        let mut start = 0;
        while start < len {
            let mut end = (start + per).min(len);
            if end < len {
                let cut_inside = match &units {
                    Units::Gsm(s) => s[end - 1] == 0x1B,
                    Units::Ucs(u) => (0xD800..0xDC00).contains(&u[end - 1]),
                };
                if cut_inside {
                    end -= 1;
                }
            }
            chunks.push(start..end);
            start = end;
        }
    }
    if chunks.len() > MAX_PARTS {
        return Err(EncodeError::TooLong);
    }
    let total = chunks.len() as u8;
    let mut parts = Vec::new();
    for (i, r) in chunks.iter().enumerate() {
        let multi = total > 1;
        let mut tpdu = vec![
            if multi { 0x51 } else { 0x11 },
            0x00,
            digits.len() as u8,
            ton,
        ];
        tpdu.extend(&da);
        tpdu.push(0x00); // PID
        let udh: Vec<u8> = if multi {
            vec![0x05, 0x00, 0x03, reference, total, (i + 1) as u8]
        } else {
            Vec::new()
        };
        match &units {
            Units::Gsm(s) => {
                tpdu.push(0x00); // DCS: GSM 7-bit
                tpdu.push(0xAA); // validity: 4 days
                let seg = &s[r.clone()];
                if multi {
                    // the 6 header octets end at bit 48: the first septet starts at bit 49
                    tpdu.push((7 + seg.len()) as u8);
                    tpdu.extend(&udh);
                    tpdu.extend(pack7(seg, 1));
                } else {
                    tpdu.push(seg.len() as u8);
                    tpdu.extend(pack7(seg, 0));
                }
            }
            Units::Ucs(u) => {
                tpdu.push(0x08); // DCS: UCS2
                tpdu.push(0xAA);
                let bytes = ucs2_encode(&String::from_utf16_lossy(&u[r.clone()]));
                tpdu.push((udh.len() + bytes.len()) as u8);
                tpdu.extend(&udh);
                tpdu.extend(bytes);
            }
        }
        let mut pdu = smsc_field(smsc);
        let tpdu_len = tpdu.len();
        pdu.extend(tpdu);
        parts.push(Part {
            hex: hex_encode(&pdu),
            tpdu_len,
        });
    }
    Ok(parts)
}
