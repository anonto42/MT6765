//! VPN, DNS and proxy through the daemon's IPC socket (owner request 2026-11-10, change
//! vpn-and-dns). The walpad-vpn script is faked inside the test root; it records its argv so the
//! tests can prove the key never reaches it. The key below is a dummy marker, not a real key.
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

const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

const FAKE_SCRIPT: &str = r#"#!/bin/sh
R=$WALPAD_VPN_ROOT
echo "$*" >> "$R/vpn-calls"
case $1 in
  up)
    if [ -f "$R/vpn-hang" ]; then sleep 30; fi
    if [ -f "$R/vpn-fail" ]; then echo state=failed; echo "reason=$(cat "$R/vpn-fail")"; exit 1; fi
    grep -q '^profile=' "$R/etc/walpad/vpn.conf" || { echo state=failed; echo reason=no profile; exit 1; }
    echo on > "$R/vpn-state"
    printf 'state=on\nkind=wireguard\nhandshake_age=3\nrx=1000\ntx=200\n' ;;
  down) echo off > "$R/vpn-state"; echo state=off ;;
  status)
    if [ "$(cat "$R/vpn-state" 2>/dev/null)" = on ]; then printf 'state=on\nhandshake_age=4\nrx=2000\ntx=300\n'; [ -f "$R/vpn-revert" ] && echo revert_in=42
    elif [ -f "$R/vpn-reverted" ]; then printf 'state=failed\nreason=reverted: the kill switch was not confirmed\n'; exit 1
    else echo state=off; fi ;;
  apply) echo state=applied ;;
  confirm) rm -f "$R/vpn-revert"; echo confirmed=1 ;;
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
    /// Next `vpn` wifi event whose state is `state`.
    fn vpn(&mut self, state: &str) -> Value {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            let v = self.recv();
            if v["event"] == "wifi" && v["data"]["kind"] == "vpn" && v["data"]["state"] == state {
                return v["data"].clone();
            }
        }
        panic!("no vpn event {state}");
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
        let root = std::env::temp_dir().join(format!("walpad-vpn-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        let script = root.join("usr/local/sbin/walpad-vpn");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, FAKE_SCRIPT).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(root.join("root/Downloads")).unwrap();
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        if short_timeouts {
            d.vpn_mut().set_timeouts(
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
        std::fs::read_to_string(self.root.join("vpn-calls")).unwrap_or_default()
    }
    fn put(&self, rel: &str, text: &str) -> String {
        let p = self.root.join(rel.trim_start_matches('/'));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        rel.to_string()
    }
    fn wg(&self, name: &str, extra: &str) -> String {
        self.put(
            &format!("/root/Downloads/{name}.conf"),
            &format!("[Interface]\nPrivateKey = {KEY}\nAddress = 10.8.0.2/32\n{extra}\n[Peer]\nPublicKey = {KEY}\nEndpoint = vpn.example.net:51820\nAllowedIPs = 0.0.0.0/0\n"),
        )
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
fn import_validates_and_stores_the_profile_0600() {
    let rig = Rig::new("import", false);
    let mut c = Conn::new(&rig.root);
    let good = rig.wg("home", "DNS = 10.8.0.1");
    let evil = rig.wg("evil", "PostUp = touch /x");
    rig.put("/root/Downloads/odd.conf", "this is not a vpn config\n");
    let r = c.call(2, "vpn_files", json!({}));
    let files = r["data"]["files"].as_array().unwrap();
    let names: Vec<&str> = files.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["home"], "only valid profiles are offered: {r}");
    assert!(!r.to_string().contains(KEY));

    let r = c.call(3, "vpn_import", json!({"path": good}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["data"]["name"], "home");
    assert!(
        !r.to_string().contains(KEY),
        "the reply never carries the key"
    );
    let store = rig.root.join("var/lib/walpad/vpn");
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&store), 0o700);
    assert_eq!(mode(&store.join("home.conf")), 0o600);
    assert!(std::fs::read_to_string(store.join("home.conf"))
        .unwrap()
        .contains(KEY));
    // the first profile is selected
    let s = c.status(4);
    assert_eq!(s["vpn"]["profile"], "home");
    assert_eq!(
        s["vpn"]["profiles"][0],
        json!({"name":"home","kind":"wireguard"})
    );
    assert!(!s.to_string().contains(KEY), "status never carries the key");

    // refusals
    for (n, args) in [
        (5, json!({"path": good})), // exists
        (6, json!({"path": evil})), // runs scripts
        (7, json!({"path": "/etc/shadow"})),
        (8, json!({"path": "/root/Downloads/nope.conf"})),
        (9, json!({"path": "/root/Downloads/odd.conf"})),
    ] {
        let r = c.call(n, "vpn_import", args);
        assert_eq!(r["ok"], false, "{r}");
        assert!(!r.to_string().contains(KEY));
    }
    // a symlink is not followed
    std::os::unix::fs::symlink(
        store.join("home.conf"),
        rig.root.join("root/Downloads/link.conf"),
    )
    .unwrap();
    let r = c.call(
        10,
        "vpn_import",
        json!({"path": "/root/Downloads/link.conf", "name": "other"}),
    );
    assert_eq!(r["ok"], false, "{r}");

    let r = c.call(11, "vpn_remove", json!({"name": "home"}));
    assert_eq!(r["ok"], true, "{r}");
    assert!(!store.join("home.conf").exists());
    assert!(c.status(12)["vpn"]["profile"].is_null());
}

#[test]
fn on_and_off_run_the_script_and_report_the_tunnel() {
    let rig = Rig::new("onoff", false);
    let mut c = Conn::new(&rig.root);
    let r = c.call(2, "vpn", json!({"on": true}));
    assert_eq!(r["error"]["code"], "invalid_args", "no profile yet: {r}");
    c.call(3, "vpn_import", json!({"path": rig.wg("home", "")}));
    c.call(4, "subscribe", json!({"topics": ["wifi"]}));
    let r = c.call(5, "vpn", json!({"on": true}));
    assert_eq!(r["data"]["state"], "starting", "{r}");
    let ev = c.vpn("on");
    assert_eq!(ev["handshake_age_s"], 3);
    assert_eq!(ev["rx_bytes"], 1000);
    assert_eq!(ev["tunnel"], "wireguard");
    // profile and kill switch cannot change under a running tunnel
    let r = c.call(6, "vpn_config", json!({"kill_switch": true}));
    assert_eq!(r["error"]["code"], "invalid_args", "{r}");
    // the status poll refreshes the counters
    let t = Instant::now();
    loop {
        let s = c.status(7);
        if s["vpn"]["rx_bytes"] == 2000 {
            assert_eq!(s["vpn"]["state"], "on");
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "no status poll: {s}");
        std::thread::sleep(Duration::from_millis(300));
    }
    let r = c.call(8, "vpn", json!({"on": false}));
    assert_eq!(r["data"]["state"], "stopping", "{r}");
    c.vpn("off");
    assert_eq!(c.status(9)["vpn"]["state"], "off");
    let calls = rig.calls();
    for l in calls.lines() {
        assert!(
            ["up", "down", "status", "apply"].contains(&l),
            "script argv carries only a verb: {l:?}"
        );
    }
    assert!(!calls.contains(KEY));
}

#[test]
fn a_failing_or_hanging_tunnel_is_reported_and_cleaned_up() {
    let rig = Rig::new("fail", true);
    let mut c = Conn::new(&rig.root);
    c.call(2, "vpn_import", json!({"path": rig.wg("home", "")}));
    c.call(3, "subscribe", json!({"topics": ["wifi"]}));
    rig.put("/vpn-fail", "no handshake with the server");
    assert_eq!(c.call(4, "vpn", json!({"on": true}))["ok"], true);
    let ev = c.vpn("failed");
    assert_eq!(ev["reason"], "no handshake with the server");
    // a hung start is killed and a cleanup `down` follows
    std::fs::remove_file(rig.root.join("vpn-fail")).unwrap();
    rig.put("/vpn-hang", "");
    assert_eq!(c.call(5, "vpn", json!({"on": true}))["ok"], true);
    let ev = c.vpn("failed");
    assert_eq!(ev["reason"], "walpad-vpn up timed out");
    let t = Instant::now();
    while !rig.calls().contains("down") {
        assert!(t.elapsed() < Duration::from_secs(8), "no cleanup down");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(c.status(6)["vpn"]["state"], "failed", "the failure is kept");
}

#[test]
fn kill_switch_confirm_and_revert_are_reported() {
    let rig = Rig::new("ks", false);
    let mut c = Conn::new(&rig.root);
    c.call(2, "vpn_import", json!({"path": rig.wg("home", "")}));
    c.call(3, "subscribe", json!({"topics": ["wifi"]}));
    let r = c.call(4, "vpn_config", json!({"kill_switch": true}));
    assert_eq!(r["data"]["kill_switch"], true, "{r}");
    let conf = rig.root.join("etc/walpad/vpn.conf");
    assert!(std::fs::read_to_string(&conf)
        .unwrap()
        .contains("kill_switch=1"));
    assert_eq!(
        std::fs::metadata(&conf).unwrap().permissions().mode() & 0o777,
        0o600
    );
    rig.put("/vpn-revert", "");
    c.call(5, "vpn", json!({"on": true}));
    c.vpn("on");
    let t = Instant::now();
    loop {
        let s = c.status(6);
        if s["vpn"]["revert_in_s"] == 42 {
            break;
        }
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "no revert countdown: {s}"
        );
        std::thread::sleep(Duration::from_millis(300));
    }
    assert_eq!(c.call(7, "vpn_confirm", json!({}))["ok"], true);
    // the helper runs asynchronously: wait for its call to be logged
    let t = Instant::now();
    while !rig.calls().lines().any(|l| l == "confirm") {
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "confirm never reached the helper"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    // the watcher reverted on its own (nobody confirmed): the next poll shows a failure
    rig.put("/vpn-reverted", "");
    std::fs::write(rig.root.join("vpn-state"), "off\n").unwrap();
    let ev = c.vpn("failed");
    assert!(
        ev["reason"].as_str().unwrap().starts_with("reverted"),
        "{ev}"
    );
}

#[test]
fn dns_and_proxy_are_stored_validated_and_applied() {
    let rig = Rig::new("dns", false);
    let mut c = Conn::new(&rig.root);
    let r = c.call(
        2,
        "vpn_config",
        json!({"dns": ["1.1.1.1", "9.9.9.9"], "proxy": "http://10.0.0.2:3128"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    let s = c.status(3);
    assert_eq!(s["vpn"]["dns"], json!(["1.1.1.1", "9.9.9.9"]));
    assert_eq!(s["vpn"]["proxy"], "http://10.0.0.2:3128");
    let t = Instant::now();
    while rig.calls().lines().filter(|l| *l == "apply").count() < 2 {
        // startup apply + the one after the change
        assert!(
            t.elapsed() < Duration::from_secs(8),
            "calls: {}",
            rig.calls()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    for bad in [
        json!({"dns": ["not-an-ip"]}),
        json!({"proxy": "http://user:pw@h:80"}),
        json!({}),
    ] {
        let r = c.call(4, "vpn_config", bad);
        assert_eq!(r["error"]["code"], "invalid_args", "{r}");
    }
    // clearing
    let r = c.call(5, "vpn_config", json!({"dns": [], "proxy": ""}));
    assert_eq!(r["ok"], true);
    let s = c.status(6);
    assert_eq!(s["vpn"]["dns"], json!([]));
    assert!(s["vpn"]["proxy"].is_null());
}
