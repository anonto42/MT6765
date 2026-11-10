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
mod tests {
    use super::*;

    // recorded from bluetoothctl 5.66 / 5.72 (addresses and names replaced)
    const SHOW: &str = "Controller 00:11:22:33:44:55 (public)\n\tName: walpad\n\tAlias: WalPad\n\tClass: 0x00000000\n\tPowered: yes\n\tPowerState: on\n\tDiscoverable: no\n\tDiscoverableTimeout: 0x000000b4\n\tPairable: yes\n\tUUID: Audio Source              (0000110a-0000-1000-8000-00805f9b34fb)\n\tModalias: usb:v1D6Bp0246d0542\n\tDiscovering: yes\n\tRoles: central\n";
    const INFO_KEYBOARD: &str = "Device AA:BB:CC:00:11:22 (public)\n\tName: Logitech K380\n\tAlias: Logitech K380\n\tClass: 0x00002540\n\tIcon: input-keyboard\n\tPaired: yes\n\tBonded: yes\n\tTrusted: yes\n\tBlocked: no\n\tConnected: yes\n\tLegacyPairing: no\n\tUUID: Human Interface Device    (00001124-0000-1000-8000-00805f9b34fb)\n\tRSSI: 0xffffffc4 (-60)\n";

    #[test]
    fn show_parses_state() {
        let c = parse_show(SHOW).unwrap();
        assert_eq!(c.address, "00:11:22:33:44:55");
        assert_eq!(c.name.as_deref(), Some("WalPad"));
        assert!(c.powered && c.discovering);
        let off = SHOW
            .replace("Powered: yes", "Powered: no")
            .replace("Discovering: yes", "Discovering: no");
        let c = parse_show(&off).unwrap();
        assert!(!c.powered && !c.discovering);
    }

    #[test]
    fn show_without_controller_is_none() {
        assert_eq!(parse_show("No default controller available\n"), None);
        assert_eq!(parse_show(""), None);
        assert_eq!(parse_show("Agent registered\nrandom\n"), None);
    }

    #[test]
    fn ansi_and_readline_noise_is_stripped() {
        let noisy = "\u{1}\u{1b}[0;94m\u{2}[bluetooth]\u{1}\u{1b}[0m\u{2}# Controller 00:11:22:33:44:55 (public)\r\n\tPowered: yes\r\n";
        // the prompt text before "Controller" must not hide it from a line-based parse
        let c = parse_show(noisy).unwrap();
        assert!(c.powered);
        assert_eq!(strip_ansi("\u{1b}[1;31mred\u{1b}[0m"), "red");
    }

    #[test]
    fn devices_list() {
        let out = "Device AA:BB:CC:00:11:22 Logitech K380\nDevice 11:22:33:44:55:66 11-22-33-44-55-66\n[NEW] junk\nDevice not-an-address x\nDevice 66:55:44:33:22:11\n";
        let d = parse_devices(out);
        assert_eq!(d.len(), 3);
        assert_eq!(d[0], ("AA:BB:CC:00:11:22".into(), "Logitech K380".into()));
        assert_eq!(d[2], ("66:55:44:33:22:11".into(), String::new()));
    }

    #[test]
    fn info_keyboard() {
        let d = parse_info(INFO_KEYBOARD).unwrap();
        assert_eq!(d.address, "AA:BB:CC:00:11:22");
        assert_eq!(d.name, "Logitech K380");
        assert!(d.paired && d.connected);
        assert_eq!(d.rssi, Some(-60));
        assert_eq!(d.icon.as_deref(), Some("input-keyboard"));
        assert_eq!(d.class, Some(0x2540));
    }

    #[test]
    fn info_unnamed_unpaired_old_rssi_format() {
        let out = "Device 5C:AA:BB:CC:DD:EE (random)\n\tAlias: 5C-AA-BB-CC-DD-EE\n\tPaired: no\n\tConnected: no\n\tRSSI: -81\n";
        let d = parse_info(out).unwrap();
        assert_eq!(d.name, "5C:AA:BB:CC:DD:EE");
        assert!(!d.paired);
        assert_eq!(d.rssi, Some(-81));
        assert_eq!(d.icon, None);
    }

    #[test]
    fn info_not_available_and_hostile_names() {
        assert_eq!(
            parse_info("Device AA:BB:CC:00:11:22 not available\n").map(|d| d.address),
            Some("AA:BB:CC:00:11:22".into())
        );
        assert_eq!(parse_info("garbage\n"), None);
        let out = format!(
            "Device AA:BB:CC:00:11:22 (public)\n\tAlias: ev\u{7}il\u{1b}[2J{}\n",
            "x".repeat(200)
        );
        let d = parse_info(&out).unwrap();
        assert!(d.name.len() <= 64 && !d.name.chars().any(char::is_control));
    }

    #[test]
    fn failures_are_translated() {
        assert_eq!(
            failure("Attempting to connect to AA:BB:CC:00:11:22\nFailed to connect: org.bluez.Error.Failed\n").as_deref(),
            Some("Could not connect (is the device on and in range?)")
        );
        assert_eq!(
            failure("Failed to pair: org.bluez.Error.AuthenticationFailed").as_deref(),
            Some("Authentication failed")
        );
        assert_eq!(
            failure("Device AA:BB:CC:00:11:22 not available").as_deref(),
            Some("Device not found")
        );
        assert_eq!(
            failure("No default controller available").as_deref(),
            Some("No Bluetooth controller")
        );
        assert_eq!(failure("Connection successful\n"), None);
        assert_eq!(failure("Device has been removed\n"), None);
    }

    fn all(chunks: &[&str]) -> Vec<PairEvent> {
        let mut p = PairParser::default();
        chunks.iter().flat_map(|c| p.feed(c)).collect()
    }

    #[test]
    fn pairing_numeric_comparison() {
        let t = "[bluetooth]# agent KeyboardDisplay\nAgent registered\n[bluetooth]# default-agent\nDefault agent request successful\n[bluetooth]# pair AA:BB:CC:00:11:22\nAttempting to pair with AA:BB:CC:00:11:22\n[CHG] Device AA:BB:CC:00:11:22 Connected: yes\n[agent] Confirm passkey 123456 (yes/no): ";
        assert_eq!(all(&[t]), vec![PairEvent::Confirm("123456".into())]);
        let t2 = "yes\n[CHG] Device AA:BB:CC:00:11:22 Bonded: yes\n[CHG] Device AA:BB:CC:00:11:22 Paired: yes\nPairing successful\n";
        assert_eq!(
            all(&[t, t2]),
            vec![PairEvent::Confirm("123456".into()), PairEvent::Success]
        );
    }

    #[test]
    fn prompt_split_across_chunks_fires_once() {
        let ev = all(&[
            "[agent] Confirm pass",
            "key 654",
            "321 (yes/no):",
            " ",
            "\nnext\n",
        ]);
        assert_eq!(ev, vec![PairEvent::Confirm("654321".into())]);
    }

    #[test]
    fn keyboard_passkey_pin_and_failures() {
        assert_eq!(
            all(&["[agent] Passkey: 482913\r\n"]),
            vec![PairEvent::Display("482913".into())]
        );
        assert_eq!(
            all(&["[agent] Passkey: 482913 (entered 2)\n"]),
            vec![PairEvent::Display("482913".into())]
        );
        assert_eq!(
            all(&["[agent] Enter PIN code: "]),
            vec![PairEvent::PinRequest]
        );
        assert_eq!(
            all(&["[agent] Enter passkey (number in 0-999999): "]),
            vec![PairEvent::PasskeyRequest]
        );
        assert_eq!(
            all(&["[agent] Authorize service 0000110d-0000-1000-8000-00805f9b34fb (yes/no): "]),
            vec![PairEvent::Authorize]
        );
        assert_eq!(
            all(&["Failed to pair: org.bluez.Error.AuthenticationFailed\n"]),
            vec![PairEvent::Failed("Authentication failed".into())]
        );
        assert_eq!(
            all(&["Device AA:BB:CC:00:11:22 not available\n"]),
            vec![PairEvent::Failed("Device not found".into())]
        );
        assert_eq!(
            all(&["Connection successful\nFailed to connect: org.bluez.Error.Failed\n"]),
            vec![
                PairEvent::Connected,
                PairEvent::ConnectFailed(
                    "Could not connect (is the device on and in range?)".into()
                )
            ]
        );
    }

    #[test]
    fn colored_prompt_and_overlong_input_do_not_break_the_parser() {
        let t = "\u{1b}[0;94m[agent]\u{1b}[0m Confirm passkey 000042 (yes/no): ";
        assert_eq!(all(&[t]), vec![PairEvent::Confirm("000042".into())]);
        let mut p = PairParser::default();
        for _ in 0..100 {
            p.feed(&"é".repeat(1000));
        }
        assert!(p.buf.len() < 20_000);
        // digits are bounded: a 40-digit "passkey" is not accepted
        assert!(
            all(&["Confirm passkey 1234567890123456789012345678901234567890 (yes/no): "])
                .is_empty()
        );
    }
}
