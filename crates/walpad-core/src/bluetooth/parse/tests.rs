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
            PairEvent::ConnectFailed("Could not connect (is the device on and in range?)".into())
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
        all(&["Confirm passkey 1234567890123456789012345678901234567890 (yes/no): "]).is_empty()
    );
}
