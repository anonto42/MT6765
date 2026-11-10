//! Pure parsers for the modem's AT replies (owner request 2026-11-10). Every function takes the
//! reply lines of one command (echo and final result already removed) and returns None / Unknown
//! for anything it does not understand: unknown URCs (`+EIND: 64`, `+ECSQ: ...`) are ignored.
use super::at::{Final, Reply};
use walpad_proto::{MobileReg, SimState};

/// Split `a,"b,c",d` into fields; quotes are removed, fields trimmed.
pub fn fields(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => out.push(std::mem::take(&mut cur).trim().to_string()),
            c => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

/// The payloads of every line that starts with `prefix` (e.g. "+CSQ:").
fn payloads<'a>(r: &'a Reply, prefix: &str) -> impl Iterator<Item = &'a str> {
    let p = prefix.to_string();
    r.lines
        .iter()
        .filter_map(move |l| l.strip_prefix(p.as_str()).map(str::trim))
}

/// `+CFUN: 1` -> Some(true); 0 (minimum) and 4 (flight mode) are off.
pub fn cfun(r: &Reply) -> Option<bool> {
    if r.fin != Final::Ok {
        return None;
    }
    let v: u32 = payloads(r, "+CFUN:")
        .next()?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()?;
    Some(v == 1)
}

/// SIM state from the reply to `AT+CPIN?` (an error reply is meaningful: no SIM / busy).
pub fn cpin(r: &Reply) -> SimState {
    match &r.fin {
        Final::Ok => match payloads(r, "+CPIN:").next().map(str::to_ascii_uppercase) {
            Some(s) if s == "READY" => SimState::Ready,
            Some(s) if s == "SIM PIN" => SimState::PinRequired,
            Some(s) if s == "SIM PUK" => SimState::PukRequired,
            Some(s) if s.contains("BUSY") => SimState::Busy,
            _ => SimState::Unknown,
        },
        Final::Cme(e) => cme_sim(e),
        _ => SimState::Unknown,
    }
}

/// SIM-related `+CME ERROR` codes (numeric with AT+CMEE=1, text with AT+CMEE=2).
pub fn cme_sim(e: &str) -> SimState {
    let t = e.trim().to_ascii_lowercase();
    match t.as_str() {
        "10" => SimState::Absent,
        "11" => SimState::PinRequired,
        "12" => SimState::PukRequired,
        "14" => SimState::Busy,
        _ if t.contains("not inserted") => SimState::Absent,
        _ if t.contains("busy") => SimState::Busy,
        _ if t.contains("puk required") => SimState::PukRequired,
        _ if t.contains("pin required") => SimState::PinRequired,
        _ => SimState::Unknown,
    }
}

/// `+CSQ: <rssi>,<ber>` -> (bars 0-4, dBm). 99 = not known.
pub fn csq(r: &Reply) -> Option<(u8, i32)> {
    if r.fin != Final::Ok {
        return None;
    }
    let rssi: i32 = payloads(r, "+CSQ:")
        .next()?
        .split(',')
        .next()?
        .trim()
        .parse()
        .ok()?;
    if !(0..=31).contains(&rssi) {
        return None;
    }
    // the levels Android uses for GSM signal strength (asu): >= 12 great, 8 good, 5 moderate
    let bars = match rssi {
        r if r >= 12 => 4,
        r if r >= 8 => 3,
        r if r >= 5 => 2,
        r if r >= 3 => 1,
        _ => 0,
    };
    Some((bars, -113 + 2 * rssi))
}

/// Registration status from `+CREG:` / `+CGREG:` / `+CEREG:` (the query reply is `<n>,<stat>[,...]`;
/// a lone `<stat>` is the URC form).
pub fn reg(r: &Reply, prefix: &str) -> Option<u8> {
    if r.fin != Final::Ok {
        return None;
    }
    let mut best = None;
    for p in payloads(r, prefix) {
        let f = fields(p);
        let stat = match f.len() {
            1 => f[0].parse().ok(),
            _ => f[1].parse().ok(),
        };
        // prefer the query reply (>= 2 fields) over an URC that slipped in
        if f.len() >= 2 || best.is_none() {
            best = stat.or(best);
        }
    }
    best
}

/// Combine the CREG / CGREG / CEREG states: registered anywhere wins.
pub fn combine_reg(stats: &[Option<u8>]) -> MobileReg {
    let has = |v: u8| stats.contains(&Some(v));
    if has(1) {
        MobileReg::Home
    } else if has(5) {
        MobileReg::Roaming
    } else if has(2) {
        MobileReg::Searching
    } else if has(3) {
        MobileReg::Denied
    } else if stats.iter().any(|s| s.is_some()) {
        MobileReg::NotRegistered
    } else {
        MobileReg::Unknown
    }
}

/// Access technology (27.007 <AcT>) as shown to the user.
pub fn tech(act: u32) -> Option<&'static str> {
    match act {
        0 | 1 | 3 => Some("2G"),
        2 | 4..=6 => Some("3G"),
        7 | 9 | 10 => Some("4G"),
        _ => None,
    }
}

/// `+COPS: <mode>[,<format>,"<oper>"[,<AcT>]]` -> (operator, tech).
pub fn cops(r: &Reply) -> (Option<String>, Option<&'static str>) {
    if r.fin != Final::Ok {
        return (None, None);
    }
    let Some(p) = payloads(r, "+COPS:").next() else {
        return (None, None);
    };
    let f = fields(p);
    let oper = f
        .get(2)
        .map(|s| clean(s, 40))
        .filter(|s| !s.is_empty() && !s.bytes().all(|b| b.is_ascii_digit()));
    let act = f.get(3).and_then(|a| a.parse::<u32>().ok()).and_then(tech);
    (oper, act)
}

/// Firmware from `ATI` (the "Revision:" line when there is one, else the last informative line).
pub fn ati(r: &Reply) -> Option<String> {
    if r.fin != Final::Ok {
        return None;
    }
    let rev = r.lines.iter().find_map(|l| {
        l.strip_prefix("Revision:")
            .or_else(|| l.strip_prefix("+CGMR:"))
            .map(str::trim)
    });
    let pick = rev.or_else(|| {
        r.lines
            .iter()
            .map(|l| l.trim())
            .rfind(|l| !l.is_empty() && !l.starts_with('+'))
    })?;
    Some(clean(pick, 80)).filter(|s| !s.is_empty())
}

/// The IMEI from `AT+CGSN` (`+CGSN: "..."` or a bare line of digits): 14-17 digits.
pub fn imei(r: &Reply) -> Option<String> {
    digits_line(r, "+CGSN:", 14..=17)
}

/// The ICCID from `AT+CCID` / `AT+ICCID` (19-20 digits, may end in F).
pub fn iccid(r: &Reply) -> Option<String> {
    if r.fin != Final::Ok {
        return None;
    }
    r.lines.iter().find_map(|l| {
        let v = l
            .strip_prefix("+CCID:")
            .or_else(|| l.strip_prefix("+ICCID:"))
            .unwrap_or(l)
            .trim()
            .trim_matches('"');
        let ok = (18..=22).contains(&v.len())
            && v.bytes()
                .all(|b| b.is_ascii_digit() || b == b'F' || b == b'f');
        ok.then(|| v.to_string())
    })
}

fn digits_line(r: &Reply, prefix: &str, len: std::ops::RangeInclusive<usize>) -> Option<String> {
    if r.fin != Final::Ok {
        return None;
    }
    r.lines.iter().find_map(|l| {
        let v = l.strip_prefix(prefix).unwrap_or(l).trim().trim_matches('"');
        (len.contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit())).then(|| v.to_string())
    })
}

/// Printable, trimmed, at most `max` characters.
pub fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

/// An identifier for a log line: only the last four characters.
pub fn mask(id: &str) -> String {
    let n = id.chars().count();
    let tail: String = id.chars().skip(n.saturating_sub(4)).collect();
    format!("***{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reply as `Port::command` would return it, from a recorded transcript (echo and the final
    /// line are split off the same way).
    fn rec(cmd: &str, transcript: &str) -> Reply {
        super::super::at::split_reply(cmd, transcript).expect("final result")
    }

    #[test]
    fn field_splitting_keeps_quoted_commas() {
        assert_eq!(
            fields(r#"0,0,"Grameen,phone",3"#),
            ["0", "0", "Grameen,phone", "3"]
        );
        assert_eq!(fields(" 1 "), ["1"]);
    }

    #[test]
    fn cfun_and_cpin() {
        assert_eq!(
            cfun(&rec("AT+CFUN?", "\r\n+CFUN: 0\r\n\r\nOK\r\n")),
            Some(false)
        );
        assert_eq!(
            cfun(&rec("AT+CFUN?", "AT+CFUN?\r\r\n+CFUN: 1\r\n\r\nOK\r\n")),
            Some(true)
        );
        assert_eq!(cfun(&rec("AT+CFUN?", "+CFUN: 4\r\nOK\r\n")), Some(false));
        assert_eq!(cfun(&rec("AT+CFUN?", "ERROR\r\n")), None);
        let p = |t: &str| cpin(&rec("AT+CPIN?", t));
        assert_eq!(p("\r\n+CPIN: READY\r\n\r\nOK\r\n"), SimState::Ready);
        assert_eq!(p("+CPIN: SIM PIN\r\nOK\r\n"), SimState::PinRequired);
        assert_eq!(p("+CPIN: SIM PUK\r\nOK\r\n"), SimState::PukRequired);
        assert_eq!(p("+CPIN: SIM busy\r\nOK\r\n"), SimState::Busy);
        assert_eq!(p("\r\n+CME ERROR: 10\r\n"), SimState::Absent);
        assert_eq!(p("+CME ERROR: SIM not inserted\r\n"), SimState::Absent);
        assert_eq!(p("+CME ERROR: 14\r\n"), SimState::Busy);
        assert_eq!(p("ERROR\r\n"), SimState::Unknown);
        // an unknown URC before the answer is ignored
        assert_eq!(
            p("\r\n+EIND: 64\r\n\r\n+CPIN: READY\r\n\r\nOK\r\n"),
            SimState::Ready
        );
    }

    #[test]
    fn signal_quality() {
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 29, 5\r\nOK\r\n")), Some((4, -55)));
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 9,99\r\nOK\r\n")), Some((3, -95)));
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 6,99\r\nOK\r\n")), Some((2, -101)));
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 3,99\r\nOK\r\n")), Some((1, -107)));
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 0,99\r\nOK\r\n")), Some((0, -113)));
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: 99,99\r\nOK\r\n")), None);
        assert_eq!(csq(&rec("AT+CSQ", "+CME ERROR: 100\r\n")), None);
        assert_eq!(csq(&rec("AT+CSQ", "+CSQ: x\r\nOK\r\n")), None);
    }

    #[test]
    fn registration() {
        let r = |p: &str, t: &str| reg(&rec("AT+CREG?", t), p);
        assert_eq!(r("+CREG:", "+CREG: 0,1\r\nOK\r\n"), Some(1));
        assert_eq!(
            r("+CEREG:", "+CEREG: 2,5,\"1A2B\",\"01C2D3E\",7\r\nOK\r\n"),
            Some(5)
        );
        // URC (one field) and the query reply: the reply wins
        assert_eq!(r("+CREG:", "+CREG: 2\r\n+CREG: 0,1\r\nOK\r\n"), Some(1));
        assert_eq!(r("+CGREG:", "+CREG: 0,1\r\nOK\r\n"), None);
        assert_eq!(r("+CREG:", "ERROR\r\n"), None);
        assert_eq!(combine_reg(&[Some(0), Some(1), Some(2)]), MobileReg::Home);
        assert_eq!(combine_reg(&[Some(5), None, Some(2)]), MobileReg::Roaming);
        assert_eq!(combine_reg(&[Some(2), Some(0), None]), MobileReg::Searching);
        assert_eq!(combine_reg(&[Some(3), Some(0), None]), MobileReg::Denied);
        assert_eq!(combine_reg(&[Some(0), Some(4)]), MobileReg::NotRegistered);
        assert_eq!(combine_reg(&[None, None]), MobileReg::Unknown);
    }

    #[test]
    fn operator_and_technology() {
        let c = |t: &str| cops(&rec("AT+COPS?", t));
        assert_eq!(
            c("+COPS: 0,0,\"Grameenphone\",3\r\nOK\r\n"),
            (Some("Grameenphone".into()), Some("2G"))
        );
        assert_eq!(
            c("+COPS: 0,0,\"Robi\",7\r\nOK\r\n"),
            (Some("Robi".into()), Some("4G"))
        );
        assert_eq!(
            c("+COPS: 0,0,\"banglalink\",2\r\nOK\r\n"),
            (Some("banglalink".into()), Some("3G"))
        );
        assert_eq!(c("+COPS: 0\r\nOK\r\n"), (None, None));
        // numeric format is not a name
        assert_eq!(c("+COPS: 0,2,\"47001\",7\r\nOK\r\n"), (None, Some("4G")));
        assert_eq!(c("+CME ERROR: 30\r\n"), (None, None));
    }

    #[test]
    fn identity() {
        let a = rec(
            "ATI",
            "ATI\r\r\nManufacturer: MediaTek Inc.\r\nModel: MTK_MODEM\r\nRevision: MOLY.LR12A.R3.MP.V98.P10\r\nIMEI: 351234567890123\r\n\r\nOK\r\n",
        );
        assert_eq!(ati(&a).as_deref(), Some("MOLY.LR12A.R3.MP.V98.P10"));
        assert_eq!(ati(&rec("ATI", "MTK2\r\nOK\r\n")).as_deref(), Some("MTK2"));
        assert_eq!(
            imei(&rec("AT+CGSN", "\r\n351234567890123\r\n\r\nOK\r\n")).as_deref(),
            Some("351234567890123")
        );
        assert_eq!(
            imei(&rec("AT+CGSN", "+CGSN: \"351234567890123\"\r\nOK\r\n")).as_deref(),
            Some("351234567890123")
        );
        assert_eq!(imei(&rec("AT+CGSN", "ERROR\r\n")), None);
        assert_eq!(
            iccid(&rec("AT+CCID", "+CCID: \"8988012345678901234F\"\r\nOK\r\n")).as_deref(),
            Some("8988012345678901234F")
        );
        assert_eq!(mask("351234567890123"), "***0123");
        assert_eq!(mask("12"), "***12");
    }
}
