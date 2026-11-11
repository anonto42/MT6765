//! GPS switch and position through the daemon's IPC socket (owner request 2026-11-11). The
//! walpad-gps script is faked inside the test root (it records its argv); gnss.json, which
//! walpad-gnss writes on the device, is written by the test.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walpad_core::{daemon::Daemon, fake as fakeroot};
use walpad_hw::Root;

const FAKE_SCRIPT: &str = r#"#!/bin/sh
R=$WALPAD_GPS_ROOT
echo "$*" >> "$R/gps-calls"
case $1 in
  start)
    if [ -f "$R/gps-hang" ]; then sleep 30; fi
    if [ -f "$R/gps-fail" ]; then echo state=failed; echo "reason=$(cat "$R/gps-fail")"; exit 1; fi
    : > "$R/gps-running"; echo state=on ;;
  stop) rm -f "$R/gps-running"; echo state=off ;;
  status) if [ -f "$R/gps-running" ]; then echo state=on; else echo state=off; fi ;;
esac
"#;

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl Conn {
    fn new(root: &Path) -> Conn {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut c = Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
        };
        assert_eq!(
            c.call(1, "hello", json!({"client":"t","proto":1}))["ok"],
            true
        );
        c
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
    fn call(&mut self, id: u64, cmd: &str, args: Value) -> Value {
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        loop {
            let v = self.recv();
            if v["id"] == id {
                return v;
            }
        }
    }
    /// Next `gps` event whose state is `state`.
    fn gps(&mut self, state: &str) -> Value {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            let v = self.recv();
            if v["event"] == "gps" && v["data"]["state"] == state {
                return v["data"].clone();
            }
        }
        panic!("no gps event {state}");
    }
    fn status(&mut self, id: u64) -> Value {
        self.call(id, "get_status", Value::Null)["data"].clone()
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Rig {
    fn new(tag: &str, short_timeouts: bool, preset: impl FnOnce(&Path)) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-gps-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        let script = root.join("usr/local/sbin/walpad-gps");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, FAKE_SCRIPT).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        preset(&root);
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        if short_timeouts {
            d.gnss_mut().set_timeouts(
                Duration::from_millis(600),
                Duration::from_secs(5),
                Duration::from_secs(5),
            );
        }
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root,
            stop,
            h: Some(h),
        }
    }
    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("gps-calls")).unwrap_or_default()
    }
    /// What walpad-gnss would publish.
    fn put_json(&self, state: &str, with_fix: bool) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let doc = json!({
            "v": 1, "state": state, "updated": now,
            "fix": if with_fix { json!({"lat": 23.7512345, "lon": 90.4098765, "alt": 12.4,
                                        "accuracy": 6.6, "age_ms": 1000}) } else { Value::Null },
            "satellites": {"view": 4, "used": 3, "list": [
                {"snr": 20, "used": true}, {"snr": 41, "used": true},
                {"snr": null, "used": false}, {"snr": 30, "used": true}]},
        });
        let p = self.root.join("run/walpad/gnss.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, doc.to_string()).unwrap();
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn switch_start_position_and_stop() {
    let rig = Rig::new("cycle", false, |_| {});
    let mut c = Conn::new(&rig.root);
    assert_eq!(
        c.call(2, "subscribe", json!({"topics":["status"]}))["ok"],
        true
    );
    let s = c.status(3);
    assert_eq!(s["gps"]["enabled"], false);
    assert_eq!(s["gps"]["state"], "off");
    assert!(s["gps"]["lat"].is_null());

    let r = c.call(4, "gps", json!({"on": true}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["state"], "starting");
    assert_eq!(
        std::fs::read_to_string(rig.root.join("var/lib/walpad/gps-enabled"))
            .unwrap()
            .trim(),
        "1"
    );
    c.gps("searching");
    assert!(rig.calls().contains("start"), "{}", rig.calls());

    rig.put_json("fix", true);
    let fix = c.gps("fix");
    assert_eq!(fix["lat"], 23.75123, "{fix}");
    assert_eq!(fix["lon"], 90.40988);
    assert_eq!(fix["alt_m"], 12);
    assert_eq!(fix["accuracy_m"], 7);
    assert_eq!(fix["satellites_used"], 3);
    assert_eq!(fix["satellites_view"], 4);
    assert_eq!(fix["signals"], json!([41, 30, 20, 0]));
    assert!(fix["age_s"].as_u64().unwrap() <= 4);
    let s = c.status(5);
    assert_eq!(s["gps"]["state"], "fix");
    assert_eq!(s["gps"]["enabled"], true);
    // the position stays out of the legacy status file everyone can read
    let legacy = std::fs::read_to_string(rig.root.join("run/walpad/status.json")).unwrap_or_default();
    assert!(!legacy.contains("23.75"), "{legacy}");

    // a second "on" while running changes nothing and does not run the script again
    let r = c.call(6, "gps", json!({"on": true}));
    assert_eq!(r["data"]["state"], "fix", "{r}");
    assert_eq!(rig.calls().matches("start").count(), 1);

    let r = c.call(7, "gps", json!({"on": false}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["state"], "stopping");
    let off = c.gps("off");
    assert!(off["lat"].is_null() && off["signals"] == json!([]), "{off}");
    assert_eq!(off["enabled"], false);
    assert!(rig.calls().contains("stop"));
    assert_eq!(
        std::fs::read_to_string(rig.root.join("var/lib/walpad/gps-enabled"))
            .unwrap()
            .trim(),
        "0"
    );
    assert_eq!(
        c.call(8, "gps", json!({}))["error"]["code"],
        "invalid_args"
    );
}

#[test]
fn start_failure_is_reported_with_its_reason() {
    let rig = Rig::new("fail", false, |r| {
        std::fs::write(r.join("gps-fail"), "turn Wi-Fi on first").unwrap();
    });
    let mut c = Conn::new(&rig.root);
    c.call(2, "subscribe", json!({"topics":["status"]}));
    c.call(3, "gps", json!({"on": true}));
    let f = c.gps("failed");
    assert_eq!(f["reason"], "turn Wi-Fi on first");
    assert_eq!(f["enabled"], true, "the switch stays on: it is retried");
    assert!(f["lat"].is_null());
}

#[test]
fn a_switched_on_gps_starts_after_a_boot() {
    let rig = Rig::new("boot", false, |r| {
        std::fs::create_dir_all(r.join("var/lib/walpad")).unwrap();
        std::fs::write(r.join("var/lib/walpad/gps-enabled"), "1\n").unwrap();
    });
    let mut c = Conn::new(&rig.root);
    c.call(2, "subscribe", json!({"topics":["status"]}));
    let t = Instant::now();
    while !rig.calls().contains("start") && t.elapsed() < Duration::from_secs(8) {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(rig.calls().contains("start"), "{}", rig.calls());
    // the start may have finished before this client connected: poll the status
    let t = Instant::now();
    loop {
        let s = c.status(5);
        if s["gps"]["state"] == "searching" {
            assert_eq!(s["gps"]["enabled"], true);
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(8), "{s}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_hanging_start_is_busy_then_times_out() {
    let rig = Rig::new("hang", true, |r| {
        std::fs::write(r.join("gps-hang"), "").unwrap();
    });
    let mut c = Conn::new(&rig.root);
    c.call(2, "subscribe", json!({"topics":["status"]}));
    let r = c.call(3, "gps", json!({"on": true}));
    assert_eq!(r["data"]["state"], "starting", "{r}");
    let r = c.call(4, "gps", json!({"on": false}));
    assert_eq!(r["error"]["code"], "busy", "{r}");
    let f = c.gps("failed");
    assert_eq!(f["reason"], "walpad-gps start timed out");
}

#[test]
fn a_stale_state_file_does_not_claim_a_fix() {
    let rig = Rig::new("stale", false, |_| {});
    let mut c = Conn::new(&rig.root);
    c.call(2, "subscribe", json!({"topics":["status"]}));
    c.call(3, "gps", json!({"on": true}));
    c.gps("searching");
    // the daemon stopped writing 60 s ago
    let old = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
        - 60_000;
    let doc = json!({"state": "fix", "updated": old,
        "fix": {"lat": 1.0, "lon": 2.0, "accuracy": 3.0, "age_ms": 0}});
    std::fs::write(rig.root.join("run/walpad/gnss.json"), doc.to_string()).unwrap();
    let t = Instant::now();
    loop {
        let s = c.status(10);
        // never "fix" from a stale file; the old position may be shown with its age
        assert_ne!(s["gps"]["state"], "fix", "{s}");
        if s["gps"]["age_s"].as_u64().is_some_and(|a| a >= 59) {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "stale file never read");
        std::thread::sleep(Duration::from_millis(300));
    }
}
