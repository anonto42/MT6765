//! walpad-ctl: command-line client for walpad-core (falls back to the legacy devd.cmd FIFO and
//! status.json while the Python device daemon is the runtime).
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;
use walpad_hw::Root;

pub const USAGE: &str = "usage: walpad-ctl <command>
  status                          show device status (JSON)
  screen on|off|toggle
  brightness N|+N|-N              native units 0-255
  volume N|+N|-N                  output volume in percent (0-100)
  mute on|off
  rotate auto|lock|toggle|0|90|180|270
  keyboard [show|hide|toggle]
  home | back | screenshot | restart-shell
  launch <app-id>
  wifi-scan                       scan and print the networks (waits for the result)
  wifi-connect <ssid> [--open]    join a network; the passphrase is read from stdin (never argv)
  wifi-saved | wifi-disconnect | wifi-forget <id>
  wifi on|off                     the Wi-Fi switch (network level; the chip stays powered)
  hotspot on|off                  start/stop the hotspot (needs a password: hotspot-config)
  hotspot-config [--name N] [--password]   set name and/or password (password read from stdin)
  power poweroff|reboot --yes     refuses without --yes
  key-held <name|code>            volumedown|volumeup|power or 114|115|116; exit 0 held, 1 not held,
                                  2 error/unknown device (never blocks, needs no core)";

/// Translate CLI words into (cmd, args). Legacy syntax is kept as-is.
pub fn build_request(args: &[String]) -> Result<(String, Value), String> {
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    let need = |n: usize| {
        if a.len() > n {
            Ok(a[n])
        } else {
            Err(USAGE.to_string())
        }
    };
    Ok(match a.first().copied() {
        Some("status") => ("get_status".into(), Value::Null),
        Some("screen") => ("screen".into(), json!({"mode": need(1)?})),
        Some("rotate") => ("rotate".into(), json!({"mode": need(1)?})),
        Some("brightness") => {
            let s = need(1)?;
            let n: i32 = s.parse().map_err(|_| format!("bad number {s:?}"))?;
            if s.starts_with('+') || s.starts_with('-') {
                ("set_brightness".into(), json!({"delta": n}))
            } else {
                ("set_brightness".into(), json!({"value": n}))
            }
        }
        Some("volume") => {
            let s = need(1)?;
            let n: i32 = s.parse().map_err(|_| format!("bad number {s:?}"))?;
            if s.starts_with('+') || s.starts_with('-') {
                ("volume_delta".into(), json!({"delta": n}))
            } else {
                ("volume_set".into(), json!({"percent": n}))
            }
        }
        Some("mute") => {
            let on = match need(1)? {
                "on" => true,
                "off" => false,
                _ => return Err(USAGE.to_string()),
            };
            ("mute".into(), json!({"on": on}))
        }
        Some("keyboard") => (
            "keyboard".into(),
            json!({"mode": a.get(1).copied().unwrap_or("toggle")}),
        ),
        Some("home") => ("home".into(), Value::Null),
        Some("back") => ("back".into(), Value::Null),
        Some("screenshot") => ("screenshot".into(), Value::Null),
        Some("restart-shell") => ("restart_shell".into(), Value::Null),
        Some("launch") => ("launch".into(), json!({"app": need(1)?})),
        Some("wifi-saved") => ("wifi_list_saved".into(), Value::Null),
        Some("wifi-disconnect") => ("wifi_disconnect".into(), Value::Null),
        Some("wifi-forget") => {
            let id: u32 = need(1)?
                .parse()
                .map_err(|_| "saved id must be a number".to_string())?;
            ("wifi_forget".into(), json!({"saved_id": id}))
        }
        Some(w @ ("wifi" | "hotspot")) => {
            let on = match need(1)? {
                "on" => true,
                "off" => false,
                _ => return Err(USAGE.to_string()),
            };
            let cmd = if w == "wifi" {
                "wifi_enable"
            } else {
                "hotspot"
            };
            (cmd.into(), json!({"on": on}))
        }
        Some("power") => (
            "power".into(),
            json!({"action": need(1)?, "confirmed": a.contains(&"--yes")}),
        ),
        _ => return Err(USAGE.to_string()),
    })
}

fn rpc(sock: &std::path::Path, cmd: &str, args: Value) -> Result<Value, String> {
    let s = UnixStream::connect(sock).map_err(|e| format!("connect: {e}"))?;
    let t = Some(Duration::from_secs(5));
    s.set_read_timeout(t).map_err(|e| e.to_string())?;
    s.set_write_timeout(t).map_err(|e| e.to_string())?;
    let mut w = s.try_clone().map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut call = |id: u64, cmd: &str, args: Value| -> Result<Value, String> {
        let req = json!({"v": 1, "id": id, "cmd": cmd, "args": args});
        writeln!(w, "{req}").map_err(|e| format!("send: {e}"))?;
        let mut line = String::new();
        r.read_line(&mut line).map_err(|e| format!("read: {e}"))?;
        let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))?;
        if v["ok"] == true {
            Ok(v["data"].clone())
        } else {
            Err(format!(
                "{}: {}",
                v["error"]["code"].as_str().unwrap_or("error"),
                v["error"]["msg"].as_str().unwrap_or("")
            ))
        }
    };
    call(1, "hello", json!({"client": "walpad-ctl", "proto": 1}))?;
    call(2, cmd, args)
}

/// Legacy fallback: translate back to a devd.cmd line (only the commands the Python daemon knew).
fn legacy_line(args: &[String]) -> Option<String> {
    match args.first().map(String::as_str) {
        Some("screen" | "rotate" | "brightness") if args.len() > 1 => Some(args.join(" ")),
        Some("keyboard") => Some("keyboard".into()),
        _ => None,
    }
}

/// Request that finishes with a `wifi` event of `kind` (scan, connect). Waits up to `wait`.
fn rpc_wifi(
    sock: &std::path::Path,
    cmd: &str,
    args: Value,
    kind: &str,
    wait: Duration,
) -> Result<Value, String> {
    let s = UnixStream::connect(sock).map_err(|e| format!("connect: {e}"))?;
    s.set_read_timeout(Some(wait)).map_err(|e| e.to_string())?;
    let mut w = s.try_clone().map_err(|e| e.to_string())?;
    let mut r = BufReader::new(s);
    let mut send = |id: u64, cmd: &str, args: Value| {
        writeln!(w, "{}", json!({"v": 1, "id": id, "cmd": cmd, "args": args}))
            .map_err(|e| format!("send: {e}"))
    };
    let mut read = || -> Result<Value, String> {
        let mut line = String::new();
        r.read_line(&mut line)
            .map_err(|e| format!("waiting for the result: {e}"))?;
        serde_json::from_str(&line).map_err(|e| format!("bad reply: {e}"))
    };
    let expect_ok = |v: Value| -> Result<(), String> {
        if v["ok"] == true {
            Ok(())
        } else {
            Err(format!(
                "{}: {}",
                v["error"]["code"].as_str().unwrap_or("error"),
                v["error"]["msg"].as_str().unwrap_or("")
            ))
        }
    };
    send(1, "hello", json!({"client": "walpad-ctl", "proto": 1}))?;
    expect_ok(read()?)?;
    send(2, "subscribe", json!({"topics": ["wifi"]}))?;
    expect_ok(read()?)?;
    send(3, cmd, args)?;
    loop {
        let v = read()?;
        if v["id"] == 3 {
            expect_ok(v)?;
        } else if v["event"] == "wifi" && v["data"]["kind"] == kind {
            return Ok(v["data"].clone());
        }
    }
}

fn wifi_scan_text(data: &Value) -> String {
    let mut out = String::new();
    for n in data["networks"].as_array().unwrap_or(&Vec::new()) {
        out.push_str(&format!(
            "{:>4} dBm  {:<5} {}\n",
            n["signal_dbm"],
            n["security"].as_str().unwrap_or("?"),
            n["ssid"].as_str().unwrap_or("")
        ));
    }
    out.trim_end().to_string()
}

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

fn walpad_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use walpad_core::{daemon::Daemon, fake};

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn key_held_exit_status() {
        let root = std::env::temp_dir().join(format!("walpad-ctl-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let r = Root::new(&root);
        let a = s(&["key-held", "volumedown"]);
        assert_eq!(key_held_status(&a, &r), 2); // no device
        std::fs::create_dir_all(root.join("sys/class/input/input2/event1")).unwrap();
        std::fs::write(root.join("sys/class/input/input2/name"), "mtk-kpd\n").unwrap();
        std::fs::create_dir_all(root.join("dev/input")).unwrap();
        std::fs::write(root.join("dev/input/event1"), vec![0u8; 96]).unwrap();
        assert_eq!(key_held_status(&a, &r), 1);
        let mut b = vec![0u8; 96];
        b[14] = 4;
        std::fs::write(root.join("dev/input/event1"), b).unwrap();
        assert_eq!(key_held_status(&a, &r), 0);
        assert_eq!(key_held_status(&s(&["key-held", "114"]), &r), 0);
        assert_eq!(key_held_status(&s(&["key-held", "115"]), &r), 1); // same device, not held
        assert_eq!(key_held_status(&s(&["key-held", "116"]), &r), 2); // no mtk-pmic-keys here
        assert_eq!(key_held_status(&s(&["key-held", "bogus"]), &r), 2);
        assert_eq!(key_held_status(&s(&["key-held"]), &r), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn request_building() {
        assert_eq!(
            build_request(&s(&["brightness", "+30"])).unwrap(),
            ("set_brightness".into(), json!({"delta": 30}))
        );
        assert_eq!(
            build_request(&s(&["brightness", "90"])).unwrap().1,
            json!({"value": 90})
        );
        assert_eq!(
            build_request(&s(&["rotate", "toggle"])).unwrap().1,
            json!({"mode": "toggle"})
        );
        assert_eq!(
            build_request(&s(&["power", "poweroff"])).unwrap().1["confirmed"],
            false
        );
        assert_eq!(
            build_request(&s(&["power", "poweroff", "--yes"]))
                .unwrap()
                .1["confirmed"],
            true
        );
        assert!(build_request(&s(&["brightness", "x"])).is_err());
        assert_eq!(
            build_request(&s(&["volume", "+5"])).unwrap(),
            ("volume_delta".into(), json!({"delta": 5}))
        );
        assert_eq!(
            build_request(&s(&["volume", "40"])).unwrap(),
            ("volume_set".into(), json!({"percent": 40}))
        );
        assert_eq!(
            build_request(&s(&["mute", "on"])).unwrap(),
            ("mute".into(), json!({"on": true}))
        );
        assert!(build_request(&s(&["mute", "maybe"])).is_err());
        assert!(build_request(&s(&["volume", "loud"])).is_err());
        assert!(build_request(&s(&["screen"])).is_err());
        assert!(build_request(&s(&["nonsense"])).is_err());
        assert_eq!(
            build_request(&s(&["wifi", "off"])).unwrap(),
            ("wifi_enable".into(), json!({"on": false}))
        );
        assert_eq!(
            build_request(&s(&["hotspot", "on"])).unwrap(),
            ("hotspot".into(), json!({"on": true}))
        );
        assert!(build_request(&s(&["hotspot", "maybe"])).is_err());
    }

    #[test]
    fn talks_to_fake_core_and_falls_back_without_it() {
        let root = std::env::temp_dir().join(format!("walpad-ctl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        fake::populate(&root).unwrap();
        let r = Root::new(&root);
        // legacy fallback with no core: status reads status.json, commands go to the FIFO
        std::fs::create_dir_all(root.join("run/walpad")).unwrap();
        std::fs::write(root.join("run/walpad/status.json"), "{\"legacy\":true}").unwrap();
        assert!(run(&s(&["status"]), &r).unwrap().contains("legacy"));
        assert!(run(&s(&["wifi-saved"]), &r)
            .unwrap_err()
            .contains("not running"));
        std::fs::write(root.join("run/walpad/devd.cmd"), "").unwrap();
        run(&s(&["rotate", "lock"]), &r).unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("run/walpad/devd.cmd"))
                .unwrap()
                .trim(),
            "rotate lock"
        );
        std::fs::remove_file(root.join("run/walpad/devd.cmd")).unwrap();

        let mut d = Daemon::new(Root::new(&root)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        let out = run(&s(&["status"]), &r).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["battery"]["source"], "voltage_estimate");
        assert!(run(&s(&["brightness", "+20"]), &r).unwrap().contains("140"));
        assert!(run(&s(&["power", "reboot"]), &r)
            .unwrap_err()
            .contains("--yes"));
        // Wi-Fi through the core with a fake supplicant: scan prints networks, connect reads the passphrase from stdin
        std::fs::create_dir_all(root.join("etc/wpa_supplicant")).unwrap();
        let _sup =
            walpad_core::wpa::fake::spawn(&root, walpad_core::wpa::fake::default_aps(), None)
                .unwrap();
        let t = std::time::Instant::now();
        let scan = loop {
            match run(&s(&["wifi-scan"]), &r) {
                Ok(t) => break t,
                Err(e) if t.elapsed() < Duration::from_secs(5) && e.contains("unavailable") => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(e) => panic!("wifi-scan failed: {e}"),
            }
        };
        assert!(
            scan.contains("Home") && scan.contains("wpa") && scan.contains("Cafe Open"),
            "{scan}"
        );
        let mut pw = std::io::Cursor::new(b"correct horse\n".to_vec());
        assert_eq!(
            run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut pw).unwrap(),
            "connected to Home"
        );
        let mut wrong = std::io::Cursor::new(b"definitely wrong\n".to_vec());
        assert!(
            run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut wrong)
                .unwrap_err()
                .contains("wrong_password")
        );
        let mut none = std::io::Cursor::new(Vec::new());
        assert!(run_with_stdin(&s(&["wifi-connect", "Home"]), &r, &mut none)
            .unwrap_err()
            .contains("no passphrase"));
        assert!(run_with_stdin(
            &s(&["wifi-connect", "Cafe Open", "--open"]),
            &r,
            &mut std::io::Cursor::new(Vec::new())
        )
        .unwrap()
        .contains("connected"));
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
