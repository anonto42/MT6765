//! Parsers for the voice and SMS replies of the modem (owner request 2026-11-11). Like the
//! network parsers they take the information lines of one reply and ignore what they do not
//! know. Numbers and message data pass through here: nothing is printed.
use crate::modem::parse::fields;

/// One line of `AT+CLCC`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clcc {
    pub idx: u8,
    /// Mobile terminated: the call came in.
    pub incoming: bool,
    pub stat: ClccStat,
    pub number: Option<String>,
}

/// `<stat>` of `+CLCC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClccStat {
    Active,
    Held,
    Dialing,
    Alerting,
    Incoming,
    Waiting,
}

/// `+CLCC: <id>,<dir>,<stat>,<mode>,<mpty>[,<number>,<type>[,<alpha>]]`; only voice calls
/// (`<mode>` 0) are listed.
pub fn clcc(lines: &[String]) -> Vec<Clcc> {
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("+CLCC:"))
        .filter_map(|p| {
            let f = fields(p);
            let idx: u8 = f.first()?.parse().ok()?;
            let incoming = f.get(1)? == "1";
            let stat = match f.get(2)?.as_str() {
                "0" => ClccStat::Active,
                "1" => ClccStat::Held,
                "2" => ClccStat::Dialing,
                "3" => ClccStat::Alerting,
                "4" => ClccStat::Incoming,
                "5" => ClccStat::Waiting,
                _ => return None,
            };
            if f.get(3).is_some_and(|m| m != "0") {
                return None; // data or fax
            }
            let number = f.get(5).filter(|n| !n.is_empty()).map(|n| clean_number(n));
            Some(Clcc {
                idx,
                incoming,
                stat,
                number,
            })
        })
        .collect()
}

/// Keep what can be part of a phone number.
pub fn clean_number(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, '+' | '*' | '#'))
        .take(32)
        .collect()
}

/// The caller of a `+CLIP: "<number>",<type>...` URC that slipped into a reply.
pub fn clip(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .find_map(|l| l.strip_prefix("+CLIP:"))
        .and_then(|p| fields(p).into_iter().next())
        .map(|n| clean_number(&n))
        .filter(|n| !n.is_empty())
}

/// Did the reply carry a ring URC?
pub fn rang(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|l| l == "RING" || l.starts_with("+CRING") || l.starts_with("+CLIP:"))
}

/// Did the reply carry a new-message URC (`+CMTI`, `+CMT`)?
pub fn new_sms_urc(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|l| l.starts_with("+CMTI:") || l.starts_with("+CMT:"))
}

/// Why a call ended, from a MediaTek `+ECPI: <call_id>,<msg_type>,...,<cause>` DISCONNECT line
/// (msg_type 6): the Q.931 cause. Not every modem sends it.
pub fn ecpi_cause(lines: &[String]) -> Option<u16> {
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("+ECPI:"))
        .find_map(|p| {
            let f = fields(p);
            if f.get(1).map(String::as_str) != Some("6") {
                return None;
            }
            f.get(9).and_then(|c| c.parse().ok())
        })
}

/// A Q.931 disconnect cause as the reason shown to the user.
pub fn cause_reason(c: u16) -> &'static str {
    match c {
        17 => "busy",
        18 | 19 => "no_answer",
        21 => "rejected",
        _ => "remote",
    }
}

/// One message of `AT+CMGL` in PDU mode: `+CMGL: <index>,<stat>,[<alpha>],<length>` and the PDU
/// on the next line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub index: u32,
    /// 0 received unread, 1 received read, 2 stored unsent, 3 stored sent.
    pub stat: u8,
    pub pdu: String,
}

pub fn cmgl(lines: &[String]) -> Vec<Listed> {
    let mut out = Vec::new();
    let mut it = lines.iter().peekable();
    while let Some(l) = it.next() {
        let Some(h) = l.strip_prefix("+CMGL:") else {
            continue;
        };
        let f = fields(h);
        let (Some(index), Some(stat)) = (
            f.first().and_then(|x| x.parse().ok()),
            f.get(1).and_then(|x| x.parse().ok()),
        ) else {
            continue;
        };
        if let Some(p) = it.peek() {
            if !p.starts_with('+') && p.bytes().all(|b| b.is_ascii_hexdigit()) && !p.is_empty() {
                out.push(Listed {
                    index,
                    stat,
                    pdu: (*p).clone(),
                });
                it.next();
            }
        }
    }
    out
}

/// The service centre number of `+CSCA: "<sca>",<tosca>`, valid digits only.
pub fn csca(lines: &[String]) -> Option<String> {
    let p = lines.iter().find_map(|l| l.strip_prefix("+CSCA:"))?;
    let f = fields(p);
    let n = f.first()?;
    let digits = n.strip_prefix('+').unwrap_or(n);
    (!digits.is_empty() && digits.len() <= 20 && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| n.to_string())
}

fn decode_ucs2_hex(s: &str) -> Option<String> {
    if s.is_empty() || !s.len().is_multiple_of(4) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let units: Vec<u16> = s
        .as_bytes()
        .as_chunks::<4>()
        .0
        .iter()
        .filter_map(|c| {
            std::str::from_utf8(c)
                .ok()
                .and_then(|h| u16::from_str_radix(h, 16).ok())
        })
        .collect();
    Some(String::from_utf16_lossy(&units))
}

/// One `+CUSD: <m>[,"<str>"[,<dcs>]]` USSD response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cusd {
    pub m: u8,
    pub text: Option<String>,
    pub dcs: u8,
}

pub fn cusd(lines: &[String]) -> Option<Cusd> {
    let idx = lines.iter().rposition(|l| l.starts_with("+CUSD:"))?;
    let mut joined = lines[idx].strip_prefix("+CUSD:")?.to_string();
    // Multi-line USSD text (e.g. "+CUSD: 2, \"MSISDN:\n88013...\", 15"): join until quotes balance.
    let mut i = idx + 1;
    while joined.bytes().filter(|&b| b == b'"').count() % 2 == 1 && i < lines.len() {
        joined.push('\n');
        joined.push_str(&lines[i]);
        i += 1;
    }
    let f = fields(&joined);
    let m: u8 = f.first()?.parse().ok()?;
    let dcs: u8 = f.get(2).and_then(|d| d.parse().ok()).unwrap_or(15);
    let text = f
        .get(1)
        .map(|s| {
            if dcs == 72 || dcs == 8 {
                decode_ucs2_hex(s).unwrap_or_else(|| s.clone())
            } else {
                s.clone()
            }
        })
        .filter(|s| !s.is_empty());
    Some(Cusd { m, text, dcs })
}

/// Range from `AT+CPBR=?` (`+CPBR: (1-250),...`), clamped to `1..=500`.
pub fn cpbr_range(lines: &[String]) -> Option<(u16, u16)> {
    let p = lines.iter().find_map(|l| l.strip_prefix("+CPBR:"))?;
    let first = fields(p).into_iter().next()?;
    let inner = first.trim().trim_start_matches('(').trim_end_matches(')');
    let (a, b) = inner.split_once('-')?;
    let lo: u16 = a.trim().parse().ok()?;
    let hi: u16 = b.trim().parse().ok()?;
    (lo >= 1 && hi >= lo).then(|| (lo, hi.min(500)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimContact {
    pub name: String,
    pub number: String,
}

/// Entries from `+CPBR: <idx>,"<number>",<type>,"<name>"`.
pub fn cpbr(lines: &[String]) -> Vec<SimContact> {
    lines
        .iter()
        .filter_map(|l| l.strip_prefix("+CPBR:"))
        .filter_map(|p| {
            let f = fields(p);
            let num = clean_number(f.get(1)?);
            if num.is_empty() {
                return None;
            }
            let raw_name = f.get(3).map(String::as_str).unwrap_or("").trim();
            let name = decode_ucs2_hex(raw_name).unwrap_or_else(|| raw_name.to_string());
            let name = if name.is_empty() { num.clone() } else { name };
            Some(SimContact { name, number: num })
        })
        .collect()
}

#[cfg(test)]
mod tests;
