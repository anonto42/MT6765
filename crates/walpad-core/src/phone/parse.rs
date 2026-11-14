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
    let p = lines.iter().rev().find_map(|l| l.strip_prefix("+CUSD:"))?;
    let f = fields(p);
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
mod tests {
    use super::*;

    fn l(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn clcc_lists_calls_of_every_state() {
        let c = clcc(&l(&[
            "+CLCC: 1,1,4,0,0,\"01711234567\",129",
            "+CLCC: 2,0,0,0,0,\"+8801811000000\",145,\"\"",
            "+CLCC: 3,0,2,0,0",
            "+CLCC: 4,1,5,0,0,\"999\",129",
            "+CLCC: 5,0,3,0,0,\"112\",129",
            "+CLCC: 6,0,1,0,0,\"12\",129",
        ]));
        assert_eq!(c.len(), 6);
        assert_eq!(c[0].stat, ClccStat::Incoming);
        assert!(c[0].incoming);
        assert_eq!(c[0].number.as_deref(), Some("01711234567"));
        assert_eq!(c[1].stat, ClccStat::Active);
        assert_eq!(c[1].number.as_deref(), Some("+8801811000000"));
        assert_eq!(c[2].number, None);
        assert_eq!(c[3].stat, ClccStat::Waiting);
        assert_eq!(c[4].stat, ClccStat::Alerting);
        assert_eq!(c[5].stat, ClccStat::Held);
    }

    #[test]
    fn clcc_ignores_data_calls_and_noise() {
        let c = clcc(&l(&[
            "+EIND: 64",
            "+CLCC: 1,0,0,1,0,\"123\",129",
            "+CLCC: x",
            "+CLCC: 2,0,9,0,0",
            "RING",
        ]));
        assert!(c.is_empty());
    }

    #[test]
    fn urc_helpers() {
        assert!(rang(&l(&["RING"])));
        assert!(rang(&l(&["+CRING: VOICE"])));
        assert_eq!(
            clip(&l(&["RING", "+CLIP: \"01711234567\",129,\"\",,\"\",0"])).as_deref(),
            Some("01711234567")
        );
        assert_eq!(clip(&l(&["+CLIP: \"\",128"])), None);
        assert!(new_sms_urc(&l(&["+CMTI: \"SM\",3"])));
        assert!(!new_sms_urc(&l(&["+CSQ: 1,2"])));
    }

    #[test]
    fn ecpi_disconnect_cause() {
        let busy = l(&["+ECPI: 1,6,1,0,0,0,\"\",0,\"\",17"]);
        assert_eq!(ecpi_cause(&busy), Some(17));
        assert_eq!(cause_reason(17), "busy");
        assert_eq!(cause_reason(19), "no_answer");
        assert_eq!(cause_reason(21), "rejected");
        assert_eq!(cause_reason(16), "remote");
        // other ECPI messages (alerting 2, connected 3) carry no cause
        assert_eq!(ecpi_cause(&l(&["+ECPI: 1,2,1,0,0,0,\"\",0"])), None);
    }

    #[test]
    fn cmgl_pairs_headers_with_pdus() {
        let m = cmgl(&l(&[
            "+CMGL: 1,0,,23",
            "0791448720003023240DD0E474D81C0EBB010000111011311640000A E8329BFD4697D9EC37"
                .replace(' ', "")
                .as_str(),
            "+CMGL: 2,1,,10",
            "00AABB",
            "+CMGL: 3,3,,5",
        ]));
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].index, m[0].stat), (1, 0));
        assert_eq!((m[1].index, m[1].stat), (2, 1));
        assert_eq!(m[1].pdu, "00AABB");
    }

    #[test]
    fn csca_needs_valid_digits() {
        assert_eq!(
            csca(&l(&["+CSCA: \"+8801716000000\",145"])).as_deref(),
            Some("+8801716000000")
        );
        assert_eq!(csca(&l(&["+CSCA: \"\",128"])), None);
        assert_eq!(csca(&l(&["+CSCA: \"12;rm\",128"])), None);
        assert_eq!(csca(&l(&[])), None);
    }

    #[test]
    fn cusd_parses_ascii_and_ucs2() {
        let a = cusd(&l(&["+CUSD: 0,\"Balance: 45.50 Tk\",15"])).unwrap();
        assert_eq!(
            (a.m, a.text.as_deref(), a.dcs),
            (0, Some("Balance: 45.50 Tk"), 15)
        );
        let u = cusd(&l(&["+CUSD: 1,\"098609A809C1\",72"])).unwrap();
        assert_eq!((u.m, u.text.as_deref(), u.dcs), (1, Some("আনু"), 72));
    }

    #[test]
    fn cpbr_parses_range_and_contacts() {
        assert_eq!(cpbr_range(&l(&["+CPBR: (1-250),40,14"])), Some((1, 250)));
        assert_eq!(cpbr_range(&l(&["+CPBR: (1-999),40,14"])), Some((1, 500)));
        let list = cpbr(&l(&[
            "+CPBR: 1,\"+8801711234567\",145,\"Rahim\"",
            "+CPBR: 2,\"01811000000\",129,\"\"",
            "+CPBR: 3,\"01911000000\",129,\"0041006D0069\"",
        ]));
        assert_eq!(list.len(), 3);
        assert_eq!(
            (list[0].name.as_str(), list[0].number.as_str()),
            ("Rahim", "+8801711234567")
        );
        assert_eq!(
            (list[1].name.as_str(), list[1].number.as_str()),
            ("01811000000", "01811000000")
        );
        assert_eq!(
            (list[2].name.as_str(), list[2].number.as_str()),
            ("Ami", "01911000000")
        );
    }
}
