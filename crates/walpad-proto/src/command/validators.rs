//! Pure field validators for IPC command arguments (paths, addresses, numbers, secrets, ids).
use crate::LockKind;

/// Bluetooth device address: six hex pairs separated by colons (nothing else reaches bluetoothctl).
pub fn valid_bt_address(s: &str) -> bool {
    s.len() == 17
        && s.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Hotspot name: 1-32 bytes of UTF-8 without control characters.
pub fn valid_hotspot_ssid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && !s.chars().any(char::is_control)
}

/// Hotspot WPA2 passphrase: 8-63 printable ASCII characters (what hostapd's wpa_passphrase takes).
pub fn valid_hotspot_secret(s: &str) -> bool {
    (8..=63).contains(&s.len()) && s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// VPN profile name: 1-24 of A-Z a-z 0-9 . - _, not starting with a dot (it is a file name).
pub fn valid_vpn_name(s: &str) -> bool {
    (1..=24).contains(&s.len())
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// A DNS server: an IPv4 or IPv6 address literal (no names: resolv.conf takes addresses).
pub fn valid_dns_server(s: &str) -> bool {
    s.parse::<std::net::IpAddr>().is_ok()
}

/// HTTP proxy `http://host:port`: no credentials (they would sit in a file and in every app's
/// environment), host of letters, digits, `.`, `-` or an IPv6 literal in brackets, port 1-65535.
pub fn valid_proxy(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("http://") else {
        return false;
    };
    let Some((host, port)) = rest.rsplit_once(':') else {
        return false;
    };
    let host_ok = if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        v6.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    };
    host_ok && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok_and(|p| p > 0)
}

/// Where `vpn_import` may read from: user folders and removable media, a `.conf` or `.ovpn` file,
/// no `..` component.
pub fn valid_vpn_import_path(s: &str) -> bool {
    const ROOTS: &[&str] = &[
        "/root/",
        "/home/",
        "/media/",
        "/mnt/",
        "/tmp/",
        "/var/lib/walpad/import/",
    ];
    s.len() <= 255
        && ROOTS.iter().any(|r| s.starts_with(r))
        && (s.ends_with(".conf") || s.ends_with(".ovpn"))
        && !s.split('/').any(|c| c == "..")
        && !s.chars().any(char::is_control)
}

/// Folders a `launch` file argument may come from (checked again by the core after resolving
/// symlinks).
pub const OPEN_ROOTS: &[&str] = &["/root", "/mnt", "/home", "/tmp/Downloads"];

/// Syntax of the `launch` file argument: absolute, inside an allowed root, no `.`/`..`/empty/hidden
/// component, no control characters or NUL, never option-looking, at most 1024 bytes. The core
/// additionally resolves symlinks (`apps::resolve_open_path`).
pub fn valid_open_path(s: &str) -> bool {
    if s.is_empty() || s.len() > 1024 || s.starts_with('-') || s.chars().any(char::is_control) {
        return false;
    }
    let Some(rest) = s.strip_prefix('/') else {
        return false;
    };
    if !rest
        .split('/')
        .all(|c| !c.is_empty() && c != "." && c != ".." && !c.starts_with(['.', '-']))
    {
        return false;
    }
    OPEN_ROOTS
        .iter()
        .any(|r| s.strip_prefix(r).is_some_and(|t| t.starts_with('/')))
}

/// Access point name: 1-62 characters of A-Z a-z 0-9 . - _ (safe in an AT command and a file).
pub fn valid_apn(s: &str) -> bool {
    (1..=62).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Do two numbers name the same party? `01711234567` and `+8801711234567` do: one is the suffix
/// of the other (digits only), with at least 7 digits in common; short numbers match exactly.
pub fn same_number(a: &str, b: &str) -> bool {
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let (a, b) = (digits(a), digits(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (long, short) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    short.len() >= 7 && long.ends_with(short.as_str())
}

/// A number to dial: 1-32 of `0-9 + * # ,` (a `,` is a pause). Nothing else reaches the modem.
pub fn valid_dial_number(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'*' | b'#' | b','))
}

/// One DTMF digit: 0-9, `*` or `#`.
pub fn valid_dtmf(s: &str) -> bool {
    s.len() == 1
        && s.bytes()
            .all(|b| b.is_ascii_digit() || b == b'*' || b == b'#')
}

/// An SMS destination or a contact number: 3-20 digits, an optional leading `+`.
pub fn valid_sms_number(s: &str) -> bool {
    let d = s.strip_prefix('+').unwrap_or(s);
    (3..=20).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit())
}

/// Message text: 1-1000 characters, no control characters except line breaks.
pub fn valid_sms_text(s: &str) -> bool {
    let n = s.chars().count();
    (1..=1000).contains(&n) && !s.chars().any(|c| c.is_control() && c != '\n')
}

pub fn valid_contact_name(s: &str) -> bool {
    (1..=64).contains(&s.chars().count()) && !s.chars().any(char::is_control)
}

/// Ids of stored contacts: 1-24 of `a-z 0-9 -`.
pub fn valid_store_id(s: &str) -> bool {
    (1..=24).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Where `contact_import` may read from: user folders and removable media, a `.vcf` file.
pub fn valid_vcard_path(s: &str) -> bool {
    const ROOTS: &[&str] = &["/root/", "/home/", "/media/", "/mnt/", "/tmp/"];
    s.len() <= 255
        && ROOTS.iter().any(|r| s.starts_with(r))
        && s.to_ascii_lowercase().ends_with(".vcf")
        && !s.split('/').any(|c| c == "..")
        && !s.chars().any(char::is_control)
}

/// SIM PIN: 4-8 ASCII digits.
pub fn valid_sim_pin(s: &str) -> bool {
    (4..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// Rule text for a lock secret kind (shown to the user as is).
pub fn lock_secret_rule(kind: LockKind) -> &'static str {
    match kind {
        LockKind::Pin => "PIN must be 4-12 digits",
        LockKind::Password => "password must be 4-64 printable characters",
        LockKind::Pattern => "pattern must connect 4-9 different dots",
    }
}

/// A new lock secret: PIN 4-12 digits, password 4-64 printable characters (no control
/// characters), pattern 4-9 different dots of the 3x3 grid written as the digits 0-8.
pub fn valid_lock_secret(kind: LockKind, s: &str) -> bool {
    match kind {
        LockKind::Pin => (4..=12).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()),
        LockKind::Password => {
            (4..=64).contains(&s.chars().count()) && !s.chars().any(char::is_control)
        }
        LockKind::Pattern => {
            (4..=9).contains(&s.len())
                && s.bytes().all(|b| (b'0'..=b'8').contains(&b))
                && s.bytes()
                    .enumerate()
                    .all(|(i, b)| !s.as_bytes()[..i].contains(&b))
        }
    }
}

/// A secret offered for verification: any 1-128 bytes (the stored kind decides what it can match).
pub fn valid_attempt(s: &str) -> bool {
    (1..=128).contains(&s.len())
}

/// App ids are short lower-case slugs ("browser", "system-monitor").
pub fn valid_app_id(app: &str) -> bool {
    !app.is_empty()
        && app.len() <= 32
        && app
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Device hostname: 1-63 characters of A-Z a-z 0-9 -, not starting or ending with `-`.
pub fn valid_hostname(s: &str) -> bool {
    (1..=63).contains(&s.len())
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
