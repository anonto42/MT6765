//! walpad-ctl: command-line client for walpad-core (falls back to the legacy devd.cmd FIFO and
//! status.json while the Python device daemon is the runtime).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod adapters;
pub mod domain;

use adapters::rpc;
use adapters::timeout;
use domain::request;
#[cfg(test)]
mod tests;

pub use request::{build_request, USAGE};
use rpc::{legacy_line, rpc, rpc_wifi, walpad_hex, wifi_scan_text};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::time::Duration;
pub use timeout::timeout_cmd;
use walpad_hw::Root;

/// `key-held <name|code>`: Ok(true) held, Ok(false) not held, Err = unknown key/device/read error.
pub fn key_held(args: &[String], root: &Root) -> Result<bool, String> {
    let k = args.get(1).ok_or_else(|| USAGE.to_string())?;
    let code = walpad_hw::input::parse_key(k).ok_or_else(|| format!("unknown key {k:?}"))?;
    walpad_hw::input::key_held(root, code)
}

/// Process exit status of `key-held`: 0 held, 1 not held, 2 error.
pub fn key_held_status(args: &[String], root: &Root) -> i32 {
    match key_held(args, root) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("{e}");
            2
        }
    }
}

pub fn run(args: &[String], root: &Root) -> Result<String, String> {
    run_with_stdin(args, root, &mut std::io::stdin().lock())
}

/// `run` with an injectable stdin (the passphrase for wifi-connect is read from it).
pub fn run_with_stdin(
    args: &[String],
    root: &Root,
    stdin: &mut dyn BufRead,
) -> Result<String, String> {
    let sock = root.p(walpad_proto::SOCKET_PATH);
    match args.first().map(String::as_str) {
        Some("lock_clear" | "lock-clear") => return lock_clear(root),
        Some("wifi-scan") => {
            let d = rpc_wifi(
                &sock,
                "wifi_scan",
                Value::Null,
                "scan",
                Duration::from_secs(15),
            )?;
            return Ok(wifi_scan_text(&d));
        }
        Some("wifi-connect") => {
            let ssid = args.get(1).ok_or_else(|| USAGE.to_string())?;
            let mut req = json!({"ssid": walpad_hex(ssid.as_bytes())});
            if !args.iter().any(|a| a == "--open") {
                let mut line = String::new();
                stdin
                    .read_line(&mut line)
                    .map_err(|e| format!("reading the passphrase: {e}"))?;
                let secret = line.trim_end_matches(['\n', '\r']).to_string();
                if secret.is_empty() {
                    return Err("no passphrase on stdin (use --open for an open network)".into());
                }
                req["secret"] = Value::String(secret);
            }
            let d = rpc_wifi(
                &sock,
                "wifi_connect",
                req,
                "connect",
                Duration::from_secs(35),
            )?;
            return if d["ok"] == true {
                Ok(format!("connected to {}", d["ssid"].as_str().unwrap_or("")))
            } else {
                Err(format!(
                    "could not connect: {}",
                    d["error"].as_str().unwrap_or("failed")
                ))
            };
        }
        Some("lock-set") => {
            let kind = args.get(1).ok_or_else(|| USAGE.to_string())?;
            if !matches!(kind.as_str(), "pin" | "password" | "pattern") {
                return Err(USAGE.to_string());
            }
            let mut req = json!({"kind": kind, "secret": read_secret(stdin, "the new secret")?});
            let mut cur = String::new();
            let n = stdin.read_line(&mut cur).map_err(|e| e.to_string())?;
            let cur = cur.trim_end_matches(['\n', '\r']);
            if n > 0 && !cur.is_empty() {
                req["current"] = Value::String(cur.to_string());
            }
            rpc(&sock, "lock_set", req)?;
            return Ok("screen lock set".into());
        }
        Some("lock-remove") => {
            let cur = read_secret(stdin, "the current secret")?;
            rpc(&sock, "lock_remove", json!({"current": cur}))?;
            return Ok("screen lock removed".into());
        }
        Some("hotspot-config") => {
            let mut req = serde_json::Map::new();
            let mut it = args.iter().skip(1);
            let mut password = false;
            while let Some(a) = it.next() {
                match a.as_str() {
                    "--name" => {
                        let n = it.next().ok_or_else(|| USAGE.to_string())?;
                        req.insert("ssid".into(), Value::String(n.clone()));
                    }
                    "--password" => password = true,
                    _ => return Err(USAGE.to_string()),
                }
            }
            if password {
                let mut line = String::new();
                stdin
                    .read_line(&mut line)
                    .map_err(|e| format!("reading the password: {e}"))?;
                let secret = line.trim_end_matches(['\n', '\r']).to_string();
                if secret.is_empty() {
                    return Err("no password on stdin".into());
                }
                req.insert("secret".into(), Value::String(secret));
            }
            if req.is_empty() {
                return Err(USAGE.to_string());
            }
            let d = rpc(&sock, "hotspot_config", Value::Object(req))?;
            return Ok(serde_json::to_string_pretty(&d).unwrap_or_default());
        }
        _ => {}
    }
    run_simple(args, root)
}

/// The screen-lock rescue. It removes the lock file and the "locked" marker directly (the core
/// notices within a second and unlocks), so it works without a PIN and even when the core is down.
/// Whoever can run this already has root on the tablet through SSH keys or the USB recovery shell;
/// the lock screen itself covers the tablet's own terminal.
fn lock_clear(root: &Root) -> Result<String, String> {
    let file = root.p("/var/lib/walpad/lock.json");
    let existed = file.exists();
    for p in [
        file,
        root.p("/run/walpad/locked"),
        root.p("/run/walpad/lock-rate"),
    ] {
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot remove {}: {e}", p.display())),
        }
    }
    Ok(if existed {
        "screen lock removed".into()
    } else {
        "no screen lock was set".into()
    })
}

fn read_secret(stdin: &mut dyn BufRead, what: &str) -> Result<String, String> {
    let mut line = String::new();
    stdin
        .read_line(&mut line)
        .map_err(|e| format!("reading {what}: {e}"))?;
    let s = line.trim_end_matches(['\n', '\r']).to_string();
    if s.is_empty() {
        return Err(format!("no {what} on stdin"));
    }
    Ok(s)
}

fn run_simple(args: &[String], root: &Root) -> Result<String, String> {
    let (cmd, a) = build_request(args)?;
    if cmd == "power" && a["confirmed"] != true {
        return Err("refusing: add --yes to confirm".into());
    }
    let sock = root.p(walpad_proto::SOCKET_PATH);
    match rpc(&sock, &cmd, a) {
        Ok(data) => Ok(serde_json::to_string_pretty(&data).unwrap_or_default()),
        Err(e) if e.starts_with("connect:") => {
            // core not running: legacy path
            if args.first().map(String::as_str) == Some("status") {
                return std::fs::read_to_string(root.p("/run/walpad/status.json"))
                    .map_err(|e| format!("no core and no status.json: {e}"));
            }
            let line =
                legacy_line(args).ok_or_else(|| format!("walpad-core is not running ({e})"))?;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .open(root.p("/run/walpad/devd.cmd"))
                .map_err(|e| format!("devd.cmd: {e}"))?;
            writeln!(f, "{line}").map_err(|e| e.to_string())?;
            Ok(String::new())
        }
        Err(e) => Err(e),
    }
}
