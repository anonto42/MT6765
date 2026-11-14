//! Parsing of `bluetoothctl` output (BlueZ 5.5x-5.7x). Pure functions; the text comes from a
//! remote-influenced source (device names), so everything is stripped of control characters and
//! bounded, and unknown lines are ignored.
use walpad_proto::{valid_bt_address, BluetoothDevice};

/// Remove ANSI escape sequences, readline markers and carriage returns.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{1b}' => {
                if it.peek() == Some(&'[') {
                    it.next();
                    // parameters and intermediates, then one final byte 0x40-0x7e
                    for n in it.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                } else {
                    it.next();
                }
            }
            '\u{1}' | '\u{2}' | '\r' => {}
            c => out.push(c),
        }
    }
    out
}

/// A device name or message from the radio: printable, at most `max` characters.
pub fn clean_text(s: &str, max: usize) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(max).collect();
    t.trim().to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ControllerInfo {
    pub address: String,
    pub name: Option<String>,
    pub powered: bool,
    pub discovering: bool,
}

fn yes(v: &str) -> bool {
    v.trim() == "yes"
}

/// `bluetoothctl show`. None when there is no controller ("No default controller available").
pub fn parse_show(out: &str) -> Option<ControllerInfo> {
    let out = strip_ansi(out);
    let mut info = ControllerInfo::default();
    let mut found = false;
    for l in out.lines() {
        // an echoed prompt ("[bluetooth]# ") may precede the first line
        let l = l.rsplit("]# ").next().unwrap_or(l).trim();
        if let Some(rest) = l.strip_prefix("Controller ") {
            let a = rest.split_whitespace().next().unwrap_or("");
            if valid_bt_address(a) {
                info.address = a.to_ascii_uppercase();
                found = true;
            }
        } else if let Some(v) = l.strip_prefix("Alias:") {
            info.name = Some(clean_text(v, 64)).filter(|n| !n.is_empty());
        } else if let Some(v) = l.strip_prefix("Powered:") {
            info.powered = yes(v);
        } else if let Some(v) = l.strip_prefix("Discovering:") {
            info.discovering = yes(v);
        }
    }
    found.then_some(info)
}

/// `bluetoothctl devices`: (ADDRESS, name) per `Device AA:BB:.. name` line.
pub fn parse_devices(out: &str) -> Vec<(String, String)> {
    strip_ansi(out)
        .lines()
        .filter_map(|l| {
            let l = l.trim().strip_prefix("Device ")?;
            let (a, name) = l.split_once(' ').unwrap_or((l, ""));
            valid_bt_address(a).then(|| (a.to_ascii_uppercase(), clean_text(name, 64)))
        })
        .collect()
}

fn parse_int(v: &str) -> Option<i32> {
    let v = v.trim();
    // newer BlueZ prints `0xffffffc4 (-60)`
    if let (Some(a), Some(b)) = (v.find('('), v.rfind(')')) {
        if a < b {
            return v[a + 1..b].trim().parse().ok();
        }
    }
    v.parse().ok()
}

/// `bluetoothctl info ADDR`. None for "Device ... not available" and unrecognised output.
pub fn parse_info(out: &str) -> Option<BluetoothDevice> {
    let out = strip_ansi(out);
    let mut d = BluetoothDevice::default();
    let mut name = None;
    let mut alias = None;
    let mut found = false;
    for l in out.lines() {
        let l = l.trim();
        if let Some(rest) = l.strip_prefix("Device ") {
            let a = rest.split_whitespace().next().unwrap_or("");
            if valid_bt_address(a) && !found {
                d.address = a.to_ascii_uppercase();
                found = true;
            }
        } else if let Some(v) = l.strip_prefix("Name:") {
            name = Some(clean_text(v, 64));
        } else if let Some(v) = l.strip_prefix("Alias:") {
            alias = Some(clean_text(v, 64));
        } else if let Some(v) = l.strip_prefix("Class:") {
            d.class = u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok();
        } else if let Some(v) = l.strip_prefix("Icon:") {
            d.icon = Some(clean_text(v, 40)).filter(|i| !i.is_empty());
        } else if let Some(v) = l.strip_prefix("Paired:") {
            d.paired = yes(v);
        } else if let Some(v) = l.strip_prefix("Connected:") {
            d.connected = yes(v);
        } else if let Some(v) = l.strip_prefix("RSSI:") {
            d.rssi = parse_int(v);
        }
    }
    if !found {
        return None;
    }
    // BlueZ's alias of an unnamed device is its address with dashes
    let dashed = d.address.replace(':', "-");
    d.name = [alias, name]
        .into_iter()
        .flatten()
        .find(|n| !n.is_empty() && !n.eq_ignore_ascii_case(&dashed))
        .unwrap_or_else(|| d.address.clone());
    Some(d)
}

/// Why a `bluetoothctl` action failed, in words for the screen; None when no failure line.
pub fn failure(out: &str) -> Option<String> {
    let out = strip_ansi(out);
    for l in out.lines() {
        let l = l.trim();
        let l = l.strip_prefix("[agent]").unwrap_or(l).trim();
        if l.starts_with("Failed to")
            || l.contains("not available")
            || l.starts_with("No default controller")
            || l.starts_with("Failed")
        {
            return Some(friendly(l));
        }
    }
    None
}

/// `Failed to connect: org.bluez.Error.Failed` -> a short sentence.
pub fn friendly(line: &str) -> String {
    let code = line
        .rsplit_once("org.bluez.Error.")
        .map(|(_, c)| c.split_whitespace().next().unwrap_or(""))
        .unwrap_or("");
    let text = match code {
        "AuthenticationFailed" => "Authentication failed",
        "AuthenticationRejected" => "Pairing was rejected",
        "AuthenticationCanceled" => "Pairing was cancelled",
        "AuthenticationTimeout" => "Pairing timed out",
        "ConnectionAttemptFailed" | "Failed" if line.contains("connect") => {
            "Could not connect (is the device on and in range?)"
        }
        "ConnectionAttemptFailed" => "Could not connect",
        "InProgress" => "Busy, try again",
        "NotReady" => "Bluetooth is not ready",
        "NotAvailable" | "DoesNotExist" => "Device not found",
        "AlreadyConnected" => "Already connected",
        "NotConnected" => "Not connected",
        "NotSupported" => "Not supported by this device",
        _ if line.starts_with("No default controller") => "No Bluetooth controller",
        _ if line.contains("not available") => "Device not found",
        _ => return clean_text(line, 80),
    };
    text.to_string()
}

/// What a running `bluetoothctl` pairing session says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairEvent {
    /// `Confirm passkey 123456 (yes/no):` - both sides show the number.
    Confirm(String),
    /// `Passkey: 123456`: type it on the device (keyboards).
    Display(String),
    /// `Enter PIN code:` (legacy pairing).
    PinRequest,
    /// `Enter passkey (number in 0-999999):`: the owner would have to type it; unsupported.
    PasskeyRequest,
    /// `Authorize service ...` / `Request authorization`.
    Authorize,
    Success,
    Failed(String),
    /// `Connection successful`.
    Connected,
    ConnectFailed(String),
}

/// Incremental parser: feed raw chunks, get events. Prompts end without a newline, so the
/// unfinished tail is also looked at when it ends the way a prompt does.
#[derive(Default)]
pub struct PairParser {
    buf: String,
}

fn digits(s: &str) -> Option<String> {
    let d: String = s
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (!d.is_empty() && d.len() <= 8).then_some(d)
}

fn classify(line: &str, tail: bool) -> Option<PairEvent> {
    // the interactive prompt prefix "[bluetooth]# " may precede the text
    let l = line.trim();
    let l = l.rsplit("]# ").next().unwrap_or(l).trim();
    let l = l.strip_prefix("[agent]").unwrap_or(l).trim();
    if let Some(r) = l.strip_prefix("Confirm passkey ") {
        let ends = r.trim_end().ends_with("(yes/no):");
        if !tail || ends {
            return digits(r).map(PairEvent::Confirm);
        }
        return None;
    }
    if let Some(r) = l.strip_prefix("Passkey:") {
        return digits(r).map(PairEvent::Display);
    }
    if l.starts_with("Enter PIN code") && (!tail || l.trim_end().ends_with(':')) {
        return Some(PairEvent::PinRequest);
    }
    if l.starts_with("Enter passkey") && (!tail || l.trim_end().ends_with(':')) {
        return Some(PairEvent::PasskeyRequest);
    }
    if tail {
        // other prompts: only the authorize questions, which end in (yes/no):
        if (l.starts_with("Authorize service") || l.starts_with("Request authorization"))
            && l.trim_end().ends_with("(yes/no):")
        {
            return Some(PairEvent::Authorize);
        }
        return None;
    }
    if l.starts_with("Authorize service") || l.starts_with("Request authorization") {
        return Some(PairEvent::Authorize);
    }
    if l.starts_with("Pairing successful") {
        return Some(PairEvent::Success);
    }
    if l.starts_with("Failed to pair") {
        return Some(PairEvent::Failed(friendly(l)));
    }
    if l.starts_with("Connection successful") {
        return Some(PairEvent::Connected);
    }
    if l.starts_with("Failed to connect") {
        return Some(PairEvent::ConnectFailed(friendly(l)));
    }
    if l.contains("not available") && l.starts_with("Device ") {
        return Some(PairEvent::Failed("Device not found".into()));
    }
    None
}

impl PairParser {
    pub fn feed(&mut self, chunk: &str) -> Vec<PairEvent> {
        self.buf.push_str(&strip_ansi(chunk));
        if self.buf.len() > 16 * 1024 {
            let cut = self.buf.len() - 4096;
            let cut = (cut..self.buf.len())
                .find(|i| self.buf.is_char_boundary(*i))
                .unwrap_or(self.buf.len());
            self.buf.drain(..cut);
        }
        let mut ev = Vec::new();
        while let Some(i) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=i).collect();
            ev.extend(classify(&line, false));
        }
        if let Some(e) = classify(&self.buf.clone(), true) {
            self.buf.clear();
            ev.push(e);
        }
        ev
    }
}

#[cfg(test)]
mod tests;
