//! Error mapping, file/time utilities, and Wi-Fi state/event formatting for `Daemon`.

use crate::bluetooth::BluetoothError;
use crate::gnss::GnssError;
use crate::hotspot::HotspotError;
use crate::lock::LockError;
use crate::modem::MobileError;
use crate::pkg::PkgError;
use crate::vpn::VpnError;
use crate::wpa::manager::{WifiError, WifiEvent};
use crate::wpa::parse::{hex, ssid_display};
use serde_json::{json, Value};
use std::path::PathBuf;
use walpad_hw::Root;
use walpad_proto::{ErrorCode, WifiRadio, WifiStatus};

pub(super) type CmdResult = Result<Value, (ErrorCode, String)>;

pub(super) fn read_file(root: &Root, p: &str) -> String {
    std::fs::read_to_string(root.p(p)).unwrap_or_default()
}

pub(super) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Keep `/root/.config/gtk-3.0/settings.ini` in sync with the device dark/light theme.
pub(super) fn sync_gtk_theme(root: &Root, light: bool) {
    let path = root.p("/root/.config/gtk-3.0/settings.ini");
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let val = if light { "0" } else { "1" };
    let key = "gtk-application-prefer-dark-theme";
    let existing = std::fs::read_to_string(&path).unwrap_or_else(|_| "[Settings]\n".to_string());
    let mut found = false;
    let mut lines: Vec<String> = existing
        .lines()
        .map(|l| {
            if l.trim_start().starts_with(key) {
                found = true;
                format!("{key}={val}")
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{key}={val}"));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    let _ = write_atomic(&path, out.as_bytes());
}

/// Milliseconds since the epoch: the screen lock's clock (its rate-limit state is persisted).
pub(super) fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn lock_err(e: LockError) -> (ErrorCode, String) {
    match e {
        LockError::WrongSecret { retry_in_s: None } => {
            (ErrorCode::Refused, "wrong PIN or password".into())
        }
        LockError::WrongSecret {
            retry_in_s: Some(s),
        }
        | LockError::Wait { retry_in_s: s } => (
            ErrorCode::Refused,
            format!("too many wrong attempts, wait {s} s"),
        ),
        LockError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        LockError::NotSet => (ErrorCode::Refused, "no screen lock is set".into()),
        LockError::Io(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn internal<E: std::fmt::Display>(e: E) -> (ErrorCode, String) {
    (ErrorCode::Internal, e.to_string())
}

pub(super) fn wifi_err(e: WifiError) -> (ErrorCode, String) {
    match e {
        WifiError::Busy => (ErrorCode::Busy, "another Wi-Fi operation is running".into()),
        WifiError::Unavailable => (
            ErrorCode::Internal,
            "Wi-Fi control is unavailable (wpa_supplicant socket)".into(),
        ),
        WifiError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        WifiError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn bluetooth_err(e: BluetoothError) -> (ErrorCode, String) {
    match e {
        BluetoothError::Busy(m) => (ErrorCode::Busy, m),
        BluetoothError::Refused(m) => (ErrorCode::Refused, m),
        BluetoothError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn mobile_err(e: MobileError) -> (ErrorCode, String) {
    match e {
        MobileError::Busy(m) => (ErrorCode::Busy, m),
        MobileError::Refused(m) => (ErrorCode::Refused, m),
        MobileError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        MobileError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn pkg_err(e: PkgError) -> (ErrorCode, String) {
    match e {
        PkgError::Busy => (
            ErrorCode::Busy,
            "another install or uninstall is running".into(),
        ),
        PkgError::Unknown(m) => (ErrorCode::InvalidArgs, m),
        PkgError::Refused(m) => (ErrorCode::Refused, m),
        PkgError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn gnss_err(e: GnssError) -> (ErrorCode, String) {
    match e {
        GnssError::Busy => (ErrorCode::Busy, "the GPS is starting or stopping".into()),
        GnssError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn hotspot_err(e: HotspotError) -> (ErrorCode, String) {
    match e {
        HotspotError::Busy => (
            ErrorCode::Busy,
            "the hotspot is starting or stopping".into(),
        ),
        HotspotError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        HotspotError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn vpn_err(e: VpnError) -> (ErrorCode, String) {
    match e {
        VpnError::Busy => (ErrorCode::Busy, "the VPN is starting or stopping".into()),
        VpnError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        VpnError::Failed(m) => (ErrorCode::Internal, m),
    }
}

pub(super) fn wifi_event_json(e: &WifiEvent) -> Value {
    match e {
        WifiEvent::Scan { networks } => json!({
            "kind": "scan",
            "networks": networks.iter().map(|n| json!({
                "ssid_hex": hex(&n.ssid), "ssid": ssid_display(&n.ssid), "signal_dbm": n.signal_dbm,
                "security": n.security.as_str(), "freq_mhz": n.freq_mhz,
            })).collect::<Vec<_>>(),
        }),
        WifiEvent::Connect { ok, error, ssid } => {
            json!({"kind": "connect", "ok": ok, "error": error, "ssid_hex": hex(ssid), "ssid": ssid_display(ssid)})
        }
        WifiEvent::Disconnected => json!({"kind": "disconnected"}),
    }
}

/// `<wpa_state> <ipv4>` | `started` | `failed` | missing.
pub fn parse_wifi_state(raw: Option<&str>) -> WifiStatus {
    let none = |radio| WifiStatus {
        radio,
        state: None,
        ssid: None,
        ip: None,
        signal_dbm: None,
        enabled: true,
    };
    match raw.map(str::trim) {
        None | Some("") => none(WifiRadio::Down),
        Some("failed") => none(WifiRadio::Failed),
        Some("started") => none(WifiRadio::Starting),
        // client off by the switch / paused for an exclusive hotspot (owner request 2026-11-10)
        Some(s @ ("disabled" | "hotspot")) => WifiStatus {
            state: Some(s.to_string()),
            ..none(WifiRadio::Down)
        },
        Some(s) => {
            let mut it = s.split_whitespace();
            WifiStatus {
                radio: WifiRadio::Up,
                state: it.next().map(String::from),
                ssid: None,
                ip: it.next().map(String::from),
                signal_dbm: None,
                enabled: true,
            }
        }
    }
}

#[cfg(test)]
mod tests;
