//! Parsers for wpa_supplicant control-interface text. Pure.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Open,
    Wep,
    Wpa,
}
impl Security {
    pub fn as_str(self) -> &'static str {
        match self {
            Security::Open => "open",
            Security::Wep => "wep",
            Security::Wpa => "wpa",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanEntry {
    pub ssid: Vec<u8>,
    pub signal_dbm: i32,
    pub freq_mhz: u32,
    pub security: Security,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub id: u32,
    pub ssid: Vec<u8>,
    pub flags: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WpaStatus {
    pub state: Option<String>,
    pub ssid: Option<Vec<u8>>,
    pub ip: Option<String>,
    pub id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ScanResults,
    Connected,
    Disconnected,
    TempDisabled { id: u32, reason: String },
    Other,
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Decode wpa_supplicant's printf-style escapes (`\xNN`, `\\`, `\"`, `\n`, `\r`, `\t`, `\e`) into bytes.
pub fn decode_escapes(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        match b[i + 1] {
            b'x' if s
                .get(i + 2..i + 4)
                .is_some_and(|h| h.bytes().all(|c| c.is_ascii_hexdigit())) =>
            {
                out.push(u8::from_str_radix(&s[i + 2..i + 4], 16).unwrap_or(b'?'));
                i += 4;
            }
            b'\\' => {
                out.push(b'\\');
                i += 2;
            }
            b'"' => {
                out.push(b'"');
                i += 2;
            }
            b'n' => {
                out.push(b'\n');
                i += 2;
            }
            b'r' => {
                out.push(b'\r');
                i += 2;
            }
            b't' => {
                out.push(b'\t');
                i += 2;
            }
            b'e' => {
                out.push(0x1b);
                i += 2;
            }
            _ => {
                out.push(b'\\');
                i += 1;
            }
        }
    }
    out
}

/// Lossy UTF-8 for display (SSIDs are bytes; the hex form is the identity).
pub fn ssid_display(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

pub fn security_of(flags: &str) -> Security {
    if flags.contains("WPA") || flags.contains("RSN") {
        Security::Wpa
    } else if flags.contains("WEP") {
        Security::Wep
    } else {
        Security::Open
    }
}

/// `SCAN_RESULTS`: header, then `bssid \t freq \t signal \t flags \t ssid`. Hidden (empty) SSIDs are
/// dropped, duplicates (several APs of one network) merged keeping the strongest, sorted by signal.
pub fn parse_scan_results(text: &str) -> Vec<ScanEntry> {
    let mut out: Vec<ScanEntry> = Vec::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 5 {
            continue;
        }
        let ssid = decode_escapes(f[4]);
        if ssid.is_empty() || ssid.iter().all(|&c| c == 0) {
            continue;
        }
        let (Ok(freq), Ok(sig)) = (f[1].trim().parse::<u32>(), f[2].trim().parse::<i32>()) else {
            continue;
        };
        let e = ScanEntry {
            ssid,
            signal_dbm: sig,
            freq_mhz: freq,
            security: security_of(f[3]),
        };
        match out.iter_mut().find(|x| x.ssid == e.ssid) {
            Some(x) if e.signal_dbm > x.signal_dbm => *x = e,
            Some(_) => {}
            None => out.push(e),
        }
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.signal_dbm));
    out
}

/// `LIST_NETWORKS`: header, then `id \t ssid \t bssid \t flags`.
pub fn parse_list_networks(text: &str) -> Vec<Saved> {
    text.lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            Some(Saved {
                id: f.first()?.trim().parse().ok()?,
                ssid: decode_escapes(f.get(1)?),
                flags: f.get(3).unwrap_or(&"").to_string(),
            })
        })
        .collect()
}

pub fn parse_status(text: &str) -> WpaStatus {
    let mut s = WpaStatus::default();
    for l in text.lines() {
        if let Some((k, v)) = l.split_once('=') {
            match k {
                "wpa_state" => s.state = Some(v.to_string()),
                "ssid" => s.ssid = Some(decode_escapes(v)),
                "ip_address" => s.ip = Some(v.to_string()),
                "id" => s.id = v.parse().ok(),
                _ => {}
            }
        }
    }
    s
}

pub fn parse_signal_poll(text: &str) -> Option<i32> {
    text.lines()
        .find_map(|l| l.strip_prefix("RSSI=")?.trim().parse().ok())
}

/// Unsolicited message: `<3>CTRL-EVENT-...`.
pub fn parse_event(line: &str) -> Event {
    let l = line.trim();
    let l = l
        .strip_prefix('<')
        .and_then(|r| r.split_once('>'))
        .map(|(_, r)| r)
        .unwrap_or(l);
    if l.starts_with("CTRL-EVENT-SCAN-RESULTS") {
        Event::ScanResults
    } else if l.starts_with("CTRL-EVENT-CONNECTED") {
        Event::Connected
    } else if l.starts_with("CTRL-EVENT-DISCONNECTED") {
        Event::Disconnected
    } else if l.starts_with("CTRL-EVENT-SSID-TEMP-DISABLED") {
        let get = |key: &str| {
            l.split_whitespace()
                .find_map(|w| w.strip_prefix(key))
                .map(str::to_string)
        };
        match get("id=").and_then(|v| v.parse().ok()) {
            Some(id) => Event::TempDisabled {
                id,
                reason: get("reason=").unwrap_or_default(),
            },
            None => Event::Other,
        }
    } else {
        Event::Other
    }
}

#[cfg(test)]
mod tests;
