//! Wi-Fi switch and hotspot through the daemon's IPC socket (owner request 2026-11-10). The
//! walpad-hotspot script is faked inside the test root; it records its argv so the tests can prove
//! the passphrase never reaches it.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake as fakeroot};
use walpad_hw::Root;

const SECRET: &str = "s3cret-hotspot-pass";

const FAKE_SCRIPT: &str = r#"#!/bin/sh
R=$WALPAD_HS_ROOT
echo "$*" >> "$R/hs-calls"
case $1 in
  start)
    if [ -f "$R/hs-hang" ]; then sleep 30; fi
    if [ -f "$R/hs-fail" ]; then echo state=failed; echo "reason=$(cat "$R/hs-fail")"; exit 1; fi
    grep -q '^wpa_passphrase=' "$R/etc/walpad/hotspot.conf" || { echo state=failed; echo reason=no password; exit 1; }
    printf 'state=on\ninternet=no_uplink\nexclusive=1\n' > "$R/hs-state"
    cat "$R/hs-state"; echo clients=0 ;;
  stop) printf 'state=off\nexclusive=0\n' > "$R/hs-state"; cat "$R/hs-state" ;;
  status) cat "$R/hs-state" 2>/dev/null || echo state=off; echo clients=2 ;;
esac
"#;

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl Conn {
    fn new(root: &Path) -> Conn {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(8))).unwrap();
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
    /// Next `hotspot` wifi event whose state is `state`.
    fn hotspot(&mut self, state: &str) -> Value {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            let v = self.recv();
            if v["event"] == "wifi" && v["data"]["kind"] == "hotspot" && v["data"]["state"] == state
            {
                return v["data"].clone();
            }
        }
        panic!("no hotspot event {state}");
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
    fn new(tag: &str, short_timeouts: bool) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-hs-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        let script = root.join("usr/local/sbin/walpad-hotspot");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, FAKE_SCRIPT).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        if short_timeouts {
            d.hotspot_mut().set_timeouts(
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
        std::fs::read_to_string(self.root.join("hs-calls")).unwrap_or_default()
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
fn wifi_switch_persists_and_hides_the_connection() {
    let rig = Rig::new("switch", false);
    let mut c = Conn::new(&rig.root);
    assert_eq!(c.status(2)["wifi"]["enabled"], true);
    let r = c.call(3, "wifi_enable", json!({"on": false}));
    assert_eq!(r["data"]["enabled"], false, "{r}");
    let f = rig.root.join("var/lib/walpad/wifi-enabled");
    assert_eq!(std::fs::read_to_string(&f).unwrap().trim(), "0");
    assert_eq!(c.status(4)["wifi"]["enabled"], false);
    // walpad-wifi's loop reports the client as disabled: no connection is shown
    std::fs::write(rig.root.join("run/walpad/wifi-state"), "disabled\n").unwrap();
    let s = c.status(5);
    assert_eq!(
        (&s["wifi"]["radio"], &s["wifi"]["state"], &s["wifi"]["ssid"]),
        (&json!("down"), &json!("disabled"), &Value::Null)
    );
    assert_eq!(
        c.call(6, "wifi_enable", json!({"on": true}))["data"]["enabled"],
        true
    );
    assert_eq!(std::fs::read_to_string(&f).unwrap().trim(), "1");
    assert_eq!(c.status(7)["wifi"]["enabled"], true);
    assert_eq!(
        c.call(8, "wifi_enable", json!({}))["error"]["code"],
        "invalid_args"
    );
}

#[test]
fn hotspot_config_start_status_stop() {
    let rig = Rig::new("cycle", false);
    let mut c = Conn::new(&rig.root);
    assert_eq!(c.call(2, "subscribe", json!({"topics":["wifi"]}))["ok"], true);

    // no password yet: refused before the script runs
    let r = c.call(3, "hotspot", json!({"on": true}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
    assert!(!rig.calls().contains("start"));

    let r = c.call(
        4,
        "hotspot_config",
        json!({"ssid": "Tab Net", "secret": SECRET}),
    );
    assert_eq!(
        (&r["data"]["ssid"], &r["data"]["has_secret"]),
        (&json!("Tab Net"), &json!(true)),
        "{r}"
    );
    assert!(!r.to_string().contains(SECRET), "reply never carries it");
    let conf = rig.root.join("etc/walpad/hotspot.conf");
    let meta = std::fs::metadata(&conf).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let text = std::fs::read_to_string(&conf).unwrap();
    assert!(text.contains("ssid2=546162204e6574\n"), "{text}");
    assert!(text.contains(&format!("wpa_passphrase={SECRET}\n")));
    // a name change keeps the stored password
    c.call(5, "hotspot_config", json!({"ssid": "Tab"}));
    assert!(std::fs::read_to_string(&conf)
        .unwrap()
        .contains(&format!("wpa_passphrase={SECRET}\n")));

    let r = c.call(6, "hotspot", json!({"on": true}));
    assert_eq!(r["data"]["state"], "starting", "{r}");
    let on = c.hotspot("on");
    assert_eq!(
        (&on["internet"], &on["exclusive"], &on["ssid"]),
        (&json!("no_uplink"), &json!(true), &json!("Tab"))
    );
    let s = c.status(7);
    assert_eq!(s["hotspot"]["state"], "on");
    assert!(!s.to_string().contains(SECRET));
    // already on: no second start
    assert_eq!(c.call(8, "hotspot", json!({"on": true}))["data"]["state"], "on");

    assert_eq!(
        c.call(9, "hotspot", json!({"on": false}))["data"]["state"],
        "stopping"
    );
    c.hotspot("off");
    assert_eq!(c.status(10)["hotspot"]["state"], "off");

    let calls = rig.calls();
    assert!(!calls.contains(SECRET), "argv never carries the passphrase");
    assert_eq!(
        calls.lines().filter(|l| *l == "start").count(),
        1,
        "{calls}"
    );
    assert!(calls.lines().all(|l| ["start", "stop", "status"].contains(&l)));
    for bad in [json!({}), json!({"secret": "short"}), json!({"ssid": ""})] {
        assert_eq!(
            c.call(11, "hotspot_config", bad)["error"]["code"],
            "invalid_args"
        );
    }
}

#[test]
fn hotspot_failure_and_timeout_paths() {
    let rig = Rig::new("fail", true);
    let mut c = Conn::new(&rig.root);
    assert_eq!(c.call(2, "subscribe", json!({"topics":["wifi"]}))["ok"], true);
    c.call(3, "hotspot_config", json!({"secret": SECRET}));
    // the name defaults to WalPad
    assert_eq!(c.status(4)["hotspot"]["ssid"], "WalPad");

    std::fs::write(rig.root.join("hs-fail"), "hostapd is not installed").unwrap();
    c.call(5, "hotspot", json!({"on": true}));
    let f = c.hotspot("failed");
    assert_eq!(f["reason"], "hostapd is not installed");
    std::fs::remove_file(rig.root.join("hs-fail")).unwrap();

    // a hung script is killed at the timeout and a cleanup stop runs
    std::fs::write(rig.root.join("hs-hang"), "").unwrap();
    assert_eq!(
        c.call(6, "hotspot", json!({"on": true}))["data"]["state"],
        "starting"
    );
    assert_eq!(
        c.call(7, "hotspot", json!({"on": false}))["error"]["code"],
        "busy"
    );
    let f = c.hotspot("failed");
    assert_eq!(f["reason"], "walpad-hotspot start timed out");
    let t = Instant::now();
    while !rig.calls().contains("stop") {
        assert!(t.elapsed() < Duration::from_secs(5), "{}", rig.calls());
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn hotspot_without_script_fails_cleanly() {
    let rig = Rig::new("noscript", false);
    std::fs::remove_file(rig.root.join("usr/local/sbin/walpad-hotspot")).unwrap();
    let mut c = Conn::new(&rig.root);
    c.call(2, "hotspot_config", json!({"secret": SECRET}));
    let r = c.call(3, "hotspot", json!({"on": true}));
    assert_eq!(r["error"]["code"], "internal");
    assert_eq!(r["error"]["msg"], "walpad-hotspot is not installed");
    assert_eq!(c.status(4)["hotspot"]["state"], "off");
}
