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
  rotate auto|lock|toggle|0|90|180|270
  keyboard [show|hide|toggle]
  home | screenshot | restart-shell
  launch <app-id>
  wifi-scan | wifi-saved | wifi-disconnect | wifi-forget <id>
  power poweroff|reboot --yes     refuses without --yes";

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
        Some("keyboard") => (
            "keyboard".into(),
            json!({"mode": a.get(1).copied().unwrap_or("toggle")}),
        ),
        Some("home") => ("home".into(), Value::Null),
        Some("screenshot") => ("screenshot".into(), Value::Null),
        Some("restart-shell") => ("restart_shell".into(), Value::Null),
        Some("launch") => ("launch".into(), json!({"app": need(1)?})),
        Some("wifi-scan") => ("wifi_scan".into(), Value::Null),
        Some("wifi-saved") => ("wifi_list_saved".into(), Value::Null),
        Some("wifi-disconnect") => ("wifi_disconnect".into(), Value::Null),
        Some("wifi-forget") => {
            let id: u32 = need(1)?
                .parse()
                .map_err(|_| "saved id must be a number".to_string())?;
            ("wifi_forget".into(), json!({"saved_id": id}))
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

pub fn run(args: &[String], root: &Root) -> Result<String, String> {
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
        assert!(build_request(&s(&["screen"])).is_err());
        assert!(build_request(&s(&["nonsense"])).is_err());
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
        assert!(run(&s(&["wifi-scan"]), &r)
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
        assert!(run(&s(&["wifi-scan"]), &r)
            .unwrap_err()
            .contains("not implemented"));
        stop.store(true, Ordering::Relaxed);
        h.join().unwrap();
        let _ = std::fs::remove_dir_all(&root);
    }
}
