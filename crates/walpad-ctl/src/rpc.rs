use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

pub(super) fn rpc(sock: &Path, cmd: &str, args: Value) -> Result<Value, String> {
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
pub(super) fn legacy_line(args: &[String]) -> Option<String> {
    match args.first().map(String::as_str) {
        Some("screen" | "rotate" | "brightness") if args.len() > 1 => Some(args.join(" ")),
        Some("keyboard") => Some("keyboard".into()),
        _ => None,
    }
}

/// Request that finishes with a `wifi` event of `kind` (scan, connect). Waits up to `wait`.
pub(super) fn rpc_wifi(
    sock: &Path,
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

pub(super) fn wifi_scan_text(data: &Value) -> String {
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

pub(super) fn walpad_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
