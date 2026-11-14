//! VPN config persistence and WireGuard/OpenVPN import validation.

use super::MAX_PROFILE;
use std::path::Path;
use walpad_proto::{valid_dns_server, valid_proxy, valid_vpn_name, VpnKind};

pub(crate) fn kind_of_name(file: &str) -> Option<VpnKind> {
    if file.ends_with(".conf") {
        Some(VpnKind::Wireguard)
    } else if file.ends_with(".ovpn") {
        Some(VpnKind::Openvpn)
    } else {
        None
    }
}

pub(crate) fn kind_ext(k: VpnKind) -> &'static str {
    match k {
        VpnKind::Wireguard => "conf",
        VpnKind::Openvpn => "ovpn",
    }
}

/// Settings from /etc/walpad/vpn.conf.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub profile: Option<String>,
    pub kill_switch: bool,
    pub dns: Vec<String>,
    pub proxy: Option<String>,
}

pub fn parse_settings(text: &str) -> Settings {
    let mut s = Settings::default();
    for l in text.lines() {
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k {
            "profile" if valid_vpn_name(v) => s.profile = Some(v.to_string()),
            "kill_switch" => s.kill_switch = v == "1",
            "dns" => {
                s.dns = v
                    .split(',')
                    .map(str::trim)
                    .filter(|d| valid_dns_server(d))
                    .take(3)
                    .map(str::to_string)
                    .collect()
            }
            "proxy" if valid_proxy(v) => s.proxy = Some(v.to_string()),
            _ => {}
        }
    }
    s
}

pub(crate) fn render_settings(s: &Settings) -> String {
    format!(
        "# walpad-core vpn_config (owner request 2026-11-10); no secrets here\nprofile={}\nkill_switch={}\ndns={}\nproxy={}\n",
        s.profile.as_deref().unwrap_or(""),
        u8::from(s.kill_switch),
        s.dns.join(","),
        s.proxy.as_deref().unwrap_or("")
    )
}

/// Keys of a WireGuard config that run commands or change the host: refused.
const WG_FORBIDDEN: &[&str] = &["preup", "postup", "predown", "postdown"];
/// OpenVPN directives that run programs, include other files or write outside the store: refused.
const OVPN_FORBIDDEN: &[&str] = &[
    "up",
    "down",
    "route-up",
    "route-pre-down",
    "tls-verify",
    "auth-user-pass-verify",
    "client-connect",
    "client-disconnect",
    "learn-address",
    "ipchange",
    "plugin",
    "script-security",
    "setenv-safe",
    "config",
    "management",
    "writepid",
    "log",
    "log-append",
    "cd",
    "chroot",
    "daemon",
];

fn is_b64_key(v: &str) -> bool {
    v.len() == 44
        && v.ends_with('=')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
}

/// Check a WireGuard config. The error text never contains config content.
pub fn validate_wireguard(text: &str) -> Result<(), String> {
    let (mut section, mut has_iface, mut has_peer) = (String::new(), false, false);
    let (mut key, mut addr, mut pubkey, mut endpoint, mut allowed) =
        (false, false, false, false, false);
    for raw in text.lines() {
        let l = raw.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        if let Some(h) = l.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            section = h.trim().to_ascii_lowercase();
            match section.as_str() {
                "interface" => has_iface = true,
                "peer" => has_peer = true,
                _ => return Err("unknown section in the WireGuard config".into()),
            }
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            return Err("a line of the WireGuard config is not key = value".into());
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if WG_FORBIDDEN.contains(&k.as_str()) {
            return Err("the profile runs scripts (PreUp/PostUp): not allowed".into());
        }
        match (section.as_str(), k.as_str()) {
            ("interface", "privatekey") => {
                if !is_b64_key(v) {
                    return Err("PrivateKey is not a WireGuard key".into());
                }
                key = true;
            }
            ("interface", "address") => addr = !v.is_empty(),
            ("peer", "publickey") => {
                if !is_b64_key(v) {
                    return Err("PublicKey is not a WireGuard key".into());
                }
                pubkey = true;
            }
            ("peer", "endpoint") => endpoint = v.contains(':'),
            ("peer", "allowedips") => allowed = !v.is_empty(),
            _ => {}
        }
    }
    if !has_iface || !has_peer {
        return Err("a WireGuard config needs [Interface] and [Peer]".into());
    }
    if !key {
        return Err("the config has no PrivateKey".into());
    }
    if !addr {
        return Err("the config has no Address".into());
    }
    if !(pubkey && endpoint && allowed) {
        return Err("the peer needs PublicKey, Endpoint (host:port) and AllowedIPs".into());
    }
    Ok(())
}

/// Check an OpenVPN config (inline `<ca>` style blocks are skipped).
pub fn validate_openvpn(text: &str) -> Result<(), String> {
    let (mut remote, mut inline) = (false, false);
    for raw in text.lines() {
        let l = raw.trim();
        if inline {
            if l.starts_with("</") {
                inline = false;
            }
            continue;
        }
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if l.starts_with('<') {
            inline = !l.contains("</");
            continue;
        }
        let mut it = l.split_whitespace();
        let d = it.next().unwrap_or("").trim_start_matches("--");
        if OVPN_FORBIDDEN.contains(&d) {
            return Err("the profile uses a directive that runs programs or changes files".into());
        }
        if d == "auth-user-pass" && it.next().is_none() {
            return Err(
                "the profile asks for a password interactively: embed it or drop it".into(),
            );
        }
        if d == "remote" && it.next().is_some() {
            remote = true;
        }
    }
    if inline {
        return Err("an inline block is not closed".into());
    }
    if !remote {
        return Err("the profile has no remote server".into());
    }
    Ok(())
}

/// A profile name from a file name: stem, invalid characters to `-`, at most 24.
pub fn name_from_path(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vpn");
    let mut n: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(24)
        .collect();
    while n.starts_with('.') {
        n.remove(0);
    }
    if n.is_empty() {
        "vpn".into()
    } else {
        n
    }
}

/// Read a candidate file: a regular file (no symlink), small, UTF-8.
pub(crate) fn read_candidate(path: &Path) -> Result<String, String> {
    let md = std::fs::symlink_metadata(path).map_err(|_| "cannot read the file".to_string())?;
    if !md.file_type().is_file() {
        return Err("not a regular file".into());
    }
    if md.len() > MAX_PROFILE {
        return Err("the file is too large for a VPN profile".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "cannot read the file".to_string())?;
    String::from_utf8(bytes).map_err(|_| "the file is not text".to_string())
}
