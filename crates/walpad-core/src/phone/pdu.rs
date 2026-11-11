//! SMS PDU codec (3GPP TS 23.040 / 23.038), pure Rust: SMS-SUBMIT for sending and SMS-DELIVER for
//! receiving, GSM 7-bit (with the extension table) and UCS2 (Bangla and everything else), long
//! messages as concatenated parts (8-bit reference UDH). Text and numbers are private: nothing in
//! this module prints them.

/// The GSM 7-bit default alphabet; index = septet value. 0x1B is the escape to the extension table.
const GSM7: [char; 128] = [
    '@', '£', '$', '¥', 'è', 'é', 'ù', 'ì', 'ò', 'Ç', '\n', 'Ø', 'ø', '\r', 'Å', 'å', 'Δ', '_',
    'Φ', 'Γ', 'Λ', 'Ω', 'Π', 'Ψ', 'Σ', 'Θ', 'Ξ', '\u{1b}', 'Æ', 'æ', 'ß', 'É', ' ', '!', '"', '#',
    '¤', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', '0', '1', '2', '3', '4', '5', '6',
    '7', '8', '9', ':', ';', '<', '=', '>', '?', '¡', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I',
    'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', 'Ä', 'Ö',
    'Ñ', 'Ü', '§', '¿', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o',
    'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', 'ä', 'ö', 'ñ', 'ü', 'à',
];

/// The extension table (after 0x1B): (septet, char).
const EXT: [(u8, char); 10] = [
    (0x0A, '\u{0c}'),
    (0x14, '^'),
    (0x28, '{'),
    (0x29, '}'),
    (0x2F, '\\'),
    (0x3C, '['),
    (0x3D, '~'),
    (0x3E, ']'),
    (0x40, '|'),
    (0x65, '€'),
];

/// Longest message we send: this many parts (6 x 153 septets or 6 x 67 UCS2 units).
pub const MAX_PARTS: usize = 6;
pub const SINGLE_GSM7: usize = 160;
pub const SINGLE_UCS2: usize = 70;
pub const PART_GSM7: usize = 153;
pub const PART_UCS2: usize = 67;

/// The septets of `text`, None when a character is not in the GSM 7-bit alphabet.
pub fn gsm7_encode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        if let Some(i) = GSM7.iter().position(|g| *g == c).filter(|i| *i != 0x1B) {
            out.push(i as u8);
        } else if let Some((s, _)) = EXT.iter().find(|(_, e)| *e == c) {
            out.push(0x1B);
            out.push(*s);
        } else {
            return None;
        }
    }
    Some(out)
}

/// Septets back to text; unknown septets after an escape become a space, like phones do.
pub fn gsm7_decode(septets: &[u8]) -> String {
    let mut out = String::new();
    let mut it = septets.iter();
    while let Some(&s) = it.next() {
        if s == 0x1B {
            match it.next() {
                Some(&e) => out.push(EXT.iter().find(|(k, _)| *k == e).map_or(' ', |(_, c)| *c)),
                None => break,
            }
        } else {
            out.push(GSM7[(s & 0x7F) as usize]);
        }
    }
    out
}

/// Pack septets into octets, starting `pad` bits into the first octet (UDH alignment).
pub fn pack7(septets: &[u8], pad: usize) -> Vec<u8> {
    let total_bits = pad + septets.len() * 7;
    let mut out = vec![0u8; total_bits.div_ceil(8)];
    for (i, s) in septets.iter().enumerate() {
        let bit = pad + i * 7;
        let (byte, off) = (bit / 8, bit % 8);
        out[byte] |= s << off;
        if off > 1 {
            out[byte + 1] |= s >> (8 - off);
        }
    }
    out
}

/// Unpack `count` septets that start `skip` bits into `bytes`.
pub fn unpack7(bytes: &[u8], count: usize, skip: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let bit = skip + i * 7;
        let (byte, off) = (bit / 8, bit % 8);
        let lo = bytes.get(byte).copied().unwrap_or(0) as u16;
        let hi = bytes.get(byte + 1).copied().unwrap_or(0) as u16;
        out.push((((hi << 8 | lo) >> off) & 0x7F) as u8);
    }
    out
}

/// UCS2 is UTF-16 big endian.
pub fn ucs2_encode(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
}

pub fn ucs2_decode(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks(2)
        .filter(|c| c.len() == 2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn hex_encode(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

pub fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) || !s.is_ascii() {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

/// Semi-octet (swapped BCD) digits of a phone number; `*`, `#` and friends use 0xA-0xE.
fn bcd_encode(digits: &str) -> Option<Vec<u8>> {
    let mut nib: Vec<u8> = Vec::with_capacity(digits.len() + 1);
    for c in digits.chars() {
        nib.push(match c {
            '0'..='9' => c as u8 - b'0',
            '*' => 0xA,
            '#' => 0xB,
            _ => return None,
        });
    }
    if nib.len() % 2 == 1 {
        nib.push(0xF);
    }
    Some(nib.chunks(2).map(|p| p[0] | (p[1] << 4)).collect())
}

fn bcd_decode(bytes: &[u8], digits: usize) -> String {
    let mut s = String::new();
    for b in bytes {
        for n in [b & 0x0F, b >> 4] {
            if s.chars().count() >= digits {
                break;
            }
            s.push(match n {
                0..=9 => (b'0' + n) as char,
                0xA => '*',
                0xB => '#',
                0xC => 'a',
                0xD => 'b',
                0xE => 'c',
                _ => continue,
            });
        }
    }
    s
}

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

/// A received SMS-DELIVER.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliver {
    pub sender: String,
    pub text: String,
    /// Service-centre time stamp as UTC milliseconds since the epoch, when the PDU carries one.
    pub ts_ms: Option<u64>,
    /// (reference, total parts, this part 1-based) of a concatenated message.
    pub concat: Option<(u16, u8, u8)>,
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The 7 octets of TP-SCTS (swapped BCD, the last one is the zone in quarter hours).
fn scts_ms(b: &[u8]) -> Option<u64> {
    let bcd = |x: u8| -> Option<i64> {
        let (lo, hi) = ((x & 0x0F) as i64, (x >> 4) as i64);
        (lo <= 9 && hi <= 9).then_some(lo * 10 + hi)
    };
    let (yy, mo, dd, hh, mi, ss) = (
        bcd(b[0])?,
        bcd(b[1])?,
        bcd(b[2])?,
        bcd(b[3])?,
        bcd(b[4])?,
        bcd(b[5])?,
    );
    if !(1..=12).contains(&mo) || !(1..=31).contains(&dd) || hh > 23 || mi > 59 || ss > 59 {
        return None;
    }
    // zone: bit 3 of the low nibble is the sign, the rest are quarter hours
    let z = b[6];
    let q = (((z & 0x07) as i64) * 10) + (z >> 4) as i64;
    let q = if z & 0x08 != 0 { -q } else { q };
    let days = days_from_civil(2000 + yy, mo, dd);
    let secs = days * 86400 + hh * 3600 + mi * 60 + ss - q * 900;
    u64::try_from(secs).ok().map(|s| s * 1000)
}

/// Decode a PDU as listed by `AT+CMGL` / `AT+CMGR` in PDU mode. None for anything that is not an
/// SMS-DELIVER (status reports, stored-sent messages) or that is malformed.
pub fn decode_deliver(hex: &str) -> Option<Deliver> {
    let b = hex_decode(hex)?;
    let mut i = 0;
    let smsc_len = *b.get(i)? as usize;
    i += 1 + smsc_len;
    let first = *b.get(i)?;
    i += 1;
    if first & 0x03 != 0 {
        return None; // not SMS-DELIVER
    }
    let has_udh = first & 0x40 != 0;
    // originating address
    let oa_digits = *b.get(i)? as usize;
    let oa_type = *b.get(i + 1)?;
    i += 2;
    let oa_octets = oa_digits.div_ceil(2);
    let oa = b.get(i..i + oa_octets)?;
    i += oa_octets;
    let _pid = *b.get(i)?;
    let dcs = *b.get(i + 1)?;
    i += 2;
    let ts_ms = scts_ms(b.get(i..i + 7)?);
    i += 7;
    let udl = *b.get(i)? as usize;
    i += 1;
    let ud = b.get(i..)?;
    let sender = if oa_type & 0x70 == 0x50 {
        // alphanumeric sender: GSM 7-bit packed, digits count = nibbles
        let septets = oa_digits * 4 / 7;
        gsm7_decode(&unpack7(oa, septets, 0))
    } else {
        let d = bcd_decode(oa, oa_digits);
        if oa_type & 0x70 == 0x10 {
            format!("+{d}")
        } else {
            d
        }
    };
    let alphabet = match dcs {
        d if d & 0xC0 == 0x00 && (d >> 2) & 3 == 2 => Alphabet::Ucs2,
        d if d & 0xF0 == 0xE0 => Alphabet::Ucs2,
        // data coding / message class group: bit 2 set = 8-bit data, not text
        d if d & 0xF0 == 0xF0 && d & 0x04 != 0 => return None,
        _ => Alphabet::Gsm7,
    };
    let (udh_len, concat) = if has_udh {
        let hl = *ud.first()? as usize;
        let h = ud.get(1..1 + hl)?;
        let mut concat = None;
        let mut k = 0;
        while k + 1 < h.len() {
            let (iei, l) = (h[k], h[k + 1] as usize);
            let d = h.get(k + 2..k + 2 + l)?;
            match (iei, l) {
                (0x00, 3) => concat = Some((d[0] as u16, d[1], d[2])),
                (0x08, 4) => concat = Some((u16::from_be_bytes([d[0], d[1]]), d[2], d[3])),
                _ => {}
            }
            k += 2 + l;
        }
        (1 + hl, concat)
    } else {
        (0, None)
    };
    let text = match alphabet {
        Alphabet::Ucs2 => ucs2_decode(ud.get(udh_len..udl.min(ud.len()))?),
        Alphabet::Gsm7 => {
            let skip_bits = udh_len * 8;
            let skip_septets = skip_bits.div_ceil(7);
            let septets = udl.checked_sub(skip_septets)?;
            gsm7_decode(&unpack7(ud, skip_septets + septets, 0)[skip_septets..])
        }
    };
    Some(Deliver {
        sender,
        text,
        ts_ms,
        concat,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn septets_pack_like_the_spec_example() {
        // 3GPP TS 23.038 example: "hellohello" -> E8329BFD4697D9EC37
        let s = gsm7_encode("hellohello").unwrap();
        assert_eq!(hex_encode(&pack7(&s, 0)), "E8329BFD4697D9EC37");
        assert_eq!(gsm7_decode(&unpack7(&pack7(&s, 0), 10, 0)), "hellohello");
    }

    #[test]
    fn gsm7_alphabet_and_extension() {
        assert_eq!(gsm7_encode("A@£$").unwrap(), [0x41, 0x00, 0x01, 0x02]);
        // euro and brackets need an escape: two septets each
        assert_eq!(gsm7_encode("€").unwrap(), [0x1B, 0x65]);
        assert_eq!(
            gsm7_decode(&gsm7_encode("a[b]{c}~\\|^€").unwrap()),
            "a[b]{c}~\\|^€"
        );
        // Bangla and emoji are not GSM
        assert!(gsm7_encode("আমি").is_none());
        assert!(gsm7_encode("hi 😀").is_none());
        assert_eq!(alphabet("hello"), Alphabet::Gsm7);
        assert_eq!(alphabet("আমি"), Alphabet::Ucs2);
    }

    #[test]
    fn ucs2_round_trips_bangla_and_emoji() {
        let t = "আমি বাংলায় গান গাই 😀";
        assert_eq!(ucs2_decode(&ucs2_encode(t)), t);
        assert_eq!(hex_encode(&ucs2_encode("আ")), "0986");
    }

    #[test]
    fn submit_single_gsm7_matches_a_known_pdu() {
        let p = encode_submit(None, "+447700900123", "hellohello", 0).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(
            p[0].hex,
            "001100".to_string() + "0C91447700091032" + "0000AA0A" + "E8329BFD4697D9EC37"
        );
        // TPDU = everything after the one-octet SMSC field
        assert_eq!(p[0].tpdu_len, p[0].hex.len() / 2 - 1);
    }

    #[test]
    fn submit_national_number_and_smsc() {
        let p = encode_submit(Some("+8801716000000"), "01711234567", "Hi", 1).unwrap();
        let h = &p[0].hex;
        assert!(h.starts_with("0891881017060000F0"), "{h}");
        assert!(h.contains("0B811017214365F7"), "{h}");
        assert_eq!(p[0].tpdu_len, h.len() / 2 - 9);
        // a broken SMSC falls back to the default
        let q = encode_submit(Some("12ab"), "01711234567", "Hi", 1).unwrap();
        assert!(q[0].hex.starts_with("0011"));
    }

    #[test]
    fn submit_ucs2_for_bangla() {
        let p = encode_submit(None, "01711234567", "আমি", 0).unwrap();
        assert_eq!(p.len(), 1);
        let h = &p[0].hex;
        assert!(h.contains("0008AA06098609AE09BF"), "{h}");
        assert_eq!(deliver_from_submit(h).text, "আমি");
    }

    /// Turn a SUBMIT PDU into the matching DELIVER (same address and data) to test both ways.
    fn deliver_from_submit(h: &str) -> Deliver {
        let b = hex_decode(h).unwrap();
        let mut i = 1 + b[0] as usize;
        let first = b[i];
        i += 2; // first octet, message reference
        let da_len = b[i] as usize;
        let oa_end = i + 2 + da_len.div_ceil(2);
        let oa = &b[i..oa_end];
        let (pid, dcs) = (b[oa_end], b[oa_end + 1]);
        let ud = &b[oa_end + 3..]; // skip the validity octet
        let mut d = vec![0x00, first & 0x40];
        d.extend(oa);
        d.extend([pid, dcs]);
        d.extend([0x11, 0x21, 0x31, 0x21, 0x43, 0x65, 0x23]);
        d.extend(ud);
        decode_deliver(&hex_encode(&d)).unwrap()
    }

    #[test]
    fn long_messages_are_split_with_concat_headers() {
        // 160 septets fit in one SMS, 161 need two parts of up to 153
        let t160 = "a".repeat(160);
        assert_eq!(
            encode_submit(None, "01711234567", &t160, 7).unwrap().len(),
            1
        );
        let t161 = "a".repeat(161);
        let p = encode_submit(None, "01711234567", &t161, 7).unwrap();
        assert_eq!(p.len(), 2);
        // UDH 050003 07 02 01 / 02 right after UDL
        assert!(p[0].hex.contains("050003070201"), "{}", p[0].hex);
        assert!(p[1].hex.contains("050003070202"), "{}", p[1].hex);
        // UCS2: 70 units single, 71 need two parts of up to 67
        let b70 = "আ".repeat(70);
        assert_eq!(
            encode_submit(None, "01711234567", &b70, 1).unwrap().len(),
            1
        );
        let b71 = "আ".repeat(71);
        assert_eq!(
            encode_submit(None, "01711234567", &b71, 1).unwrap().len(),
            2
        );
        // the cap: 6 parts
        let ok = "a".repeat(PART_GSM7 * MAX_PARTS);
        assert_eq!(
            encode_submit(None, "01711234567", &ok, 1).unwrap().len(),
            MAX_PARTS
        );
        let too = "a".repeat(PART_GSM7 * MAX_PARTS + 1);
        assert_eq!(
            encode_submit(None, "01711234567", &too, 1),
            Err(EncodeError::TooLong)
        );
        assert_eq!(
            encode_submit(None, "01711234567", "", 1),
            Err(EncodeError::TooLong)
        );
        assert_eq!(
            encode_submit(None, "01x", "a", 1),
            Err(EncodeError::BadNumber)
        );
    }

    #[test]
    fn an_escape_pair_is_never_cut_between_parts() {
        // 152 'a' then a euro (2 septets) would straddle the 153 boundary
        let t = format!("{}€{}", "a".repeat(152), "b".repeat(20));
        let p = encode_submit(None, "01711234567", &t, 3).unwrap();
        assert_eq!(p.len(), 2);
        let d1 = part_text(&p[0]);
        let d2 = part_text(&p[1]);
        assert_eq!(format!("{d1}{d2}"), t);
        assert_eq!(d1.chars().count(), 152);
    }

    fn part_text(p: &Part) -> String {
        deliver_from_submit(&p.hex).text
    }

    #[test]
    fn concatenated_gsm7_decodes_back_in_order() {
        let t: String = (0..400)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        let parts = encode_submit(None, "+8801711234567", &t, 9).unwrap();
        assert_eq!(parts.len(), 3);
        let mut got = String::new();
        for (i, p) in parts.iter().enumerate() {
            let d = deliver_from_submit(&p.hex);
            assert_eq!(d.concat, Some((9, 3, (i + 1) as u8)));
            assert_eq!(d.sender, "+8801711234567");
            got.push_str(&d.text);
        }
        assert_eq!(got, t);
    }

    #[test]
    fn concatenated_ucs2_decodes_back() {
        let t = "আমার সোনার বাংলা ".repeat(8);
        let parts = encode_submit(None, "01711234567", &t, 2).unwrap();
        assert!(parts.len() >= 2);
        let got: String = parts
            .iter()
            .map(|p| deliver_from_submit(&p.hex).text)
            .collect();
        assert_eq!(got, t);
    }

    #[test]
    fn deliver_from_the_wild() {
        // a widely quoted example: sender +31641600986, "How are you?"
        let d = decode_deliver(
            "07911326040000F0040B911346610089F60040208062917314080CC8F71D14969741F977FD07",
        )
        .unwrap();
        assert_eq!(d.sender, "+31641600986");
        assert_eq!(d.text, "How are you?");
        assert!(d.ts_ms.is_some());
        assert_eq!(d.concat, None);
    }

    #[test]
    fn alphanumeric_sender_and_status_reports() {
        let sender = hex_encode(&pack7(&gsm7_encode("Dhaka").unwrap(), 0));
        assert_eq!(sender.len(), 10);
        let ud = hex_encode(&pack7(&gsm7_encode("ok").unwrap(), 0));
        let pdu = format!("00040AD0{sender}0000112131415263 0002{ud}").replace(' ', "");
        let d = decode_deliver(&pdu).unwrap();
        assert_eq!(d.sender, "Dhaka");
        assert_eq!(d.text, "ok");
        // SMS-STATUS-REPORT (MTI 2) and garbage are not messages
        assert_eq!(decode_deliver("0006010C910000"), None);
        assert_eq!(decode_deliver("zz"), None);
        assert_eq!(decode_deliver(""), None);
        assert_eq!(decode_deliver("0004"), None);
    }

    #[test]
    fn timestamps_use_the_zone() {
        // 2020-05-17 10:30:45 at +6:00 (24 quarters = 0x42 -> swapped nibbles "42") = 04:30:45 UTC
        let b = [0x02, 0x50, 0x71, 0x01, 0x03, 0x54, 0x42];
        let ms = scts_ms(&b).unwrap();
        // 2020-05-17T04:30:45Z
        assert_eq!(ms / 1000, 1_589_689_845);
        assert_eq!(
            scts_ms(&[0x02, 0x50, 0x71, 0x01, 0x03, 0x54, 0x4A]).map(|m| m / 1000),
            Some(1_589_689_845 + 43_200)
        );
        assert_eq!(scts_ms(&[0xFF; 7]), None);
    }

    #[test]
    fn hex_helpers_reject_garbage() {
        assert_eq!(hex_decode("0A1b"), Some(vec![0x0A, 0x1B]));
        assert_eq!(hex_decode("0A1"), None);
        assert_eq!(hex_decode("0G"), None);
        assert_eq!(hex_decode("é1"), None);
    }
}
