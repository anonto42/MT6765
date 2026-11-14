use super::codec::*;
use super::submit::Alphabet;

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
pub(super) fn scts_ms(b: &[u8]) -> Option<u64> {
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
