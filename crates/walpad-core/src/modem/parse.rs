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
mod tests;
