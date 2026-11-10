//! Bluetooth through the daemon's IPC socket (owner request 2026-11-10). `bluetoothctl` and
//! `walpad-bluetooth` are fakes inside the test root; the fake bluetoothctl keeps its "controller"
//! in `<root>/bt/` and records every argv, so the tests can prove only validated addresses and
//! fixed words reach it.
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

const FAKE_CTL: &str = r#"#!/bin/sh
R=$WALPAD_BT_ROOT; S=$R/bt; mkdir -p "$S"
echo "$*" >> "$S/calls"
T=""
if [ "$1" = --timeout ]; then T=$2; shift 2; fi
get() { cat "$S/$1" 2>/dev/null || echo "$2"; }
# rewrite field $3 (1-based) of device $2 with $4 (devices: addr|name|paired|connected|rssi|icon)
setf() { awk -F'|' -v OFS='|' -v a="$1" -v f="$2" -v v="$3" '$1==a{$f=v}1' "$S/devices" > "$S/devices.new"; mv "$S/devices.new" "$S/devices"; }
if [ ! -f "$S/controller" ]; then echo "No default controller available"; exit 1; fi
interactive() {
  while read -r line; do
    case $line in
      "pair "*)
        a=${line#pair }; echo "Attempting to pair with $a"
        case $(get pair-mode confirm) in
          confirm) printf '[agent] Confirm passkey 123456 (yes/no): '; read -r ans
            if [ "$ans" = yes ]; then setf "$a" 3 yes; printf '\nPairing successful\n'
            else printf '\nFailed to pair: org.bluez.Error.AuthenticationFailed\n'; fi ;;
          keyboard) printf '[agent] Passkey: 482913\n'; sleep 0.3; setf "$a" 3 yes; echo "Pairing successful" ;;
          pin) printf '[agent] Enter PIN code: '; read -r pin; echo "$pin" > "$S/pin-answer"; setf "$a" 3 yes; printf '\nPairing successful\n' ;;
          hang) sleep 30 ;;
        esac ;;
      "trust "*) echo "Changing ${line#trust } trust succeeded" ;;
      "connect "*) setf "${line#connect }" 4 yes; echo "Connection successful" ;;
      quit) exit 0 ;;
    esac
  done
}
case $1 in
  "") interactive ;;
  show) printf 'Controller 00:11:22:33:44:55 (public)\n\tName: walpad\n\tAlias: WalPad\n\tPowered: %s\n\tDiscovering: %s\n' "$(get powered no)" "$(get disc no)" ;;
  devices) while IFS='|' read -r a n p c r i; do echo "Device $a $n"; done < "$S/devices" ;;
  info)
    IFS='|' read -r a n p c r i <<EOF
$(grep "^$2|" "$S/devices")
EOF
    if [ -z "$a" ]; then echo "Device $2 not available"; else
      printf 'Device %s (public)\n\tName: %s\n\tAlias: %s\n\tIcon: %s\n\tPaired: %s\n\tConnected: %s\n' "$a" "$n" "$n" "$i" "$p" "$c"
      [ -n "$r" ] && printf '\tRSSI: 0xffffffc4 (%s)\n' "$r"
    fi ;;
  power) if [ "$2" = on ]; then echo yes; else echo no; fi > "$S/powered"; echo "Changing power $2 succeeded" ;;
  scan)
    if [ "$2" = on ]; then
      echo yes > "$S/disc"
      grep -q '^66:55:44:33:22:11|' "$S/devices" || echo '66:55:44:33:22:11|Test Phone|no|no|-55|phone' >> "$S/devices"
      trap 'echo no > "$S/disc"; exit 0' TERM INT
      sleep "${T:-30}" & wait $!
      echo no > "$S/disc"
    else echo no > "$S/disc"; fi ;;
  connect)
    if [ -f "$S/connect-fail" ]; then echo "Failed to connect: org.bluez.Error.Failed"; exit 1; fi
    setf "$2" 4 yes; echo "Connection successful" ;;
  disconnect) setf "$2" 4 no; echo "Successful disconnected" ;;
  remove) grep -v "^$2|" "$S/devices" > "$S/devices.new"; mv "$S/devices.new" "$S/devices"; echo "Device has been removed" ;;
  cancel-pairing) ;;
esac
"#;

const FAKE_SCRIPT: &str = r#"#!/bin/sh
S=$WALPAD_ROOT/bt; mkdir -p "$S"
echo "$*" >> "$S/script-calls"
case $1 in
  start) if [ -f "$S/start-fail" ]; then echo "[bt] FAILED: bridge did not create an HCI device"; exit 1; fi
         touch "$S/controller" ;;
  stop) rm -f "$S/controller"; echo no > "$S/powered" ;;
esac
"#;

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
    events: Vec<Value>,
}
impl Conn {
    fn new(root: &Path) -> Conn {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut c = Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
            events: Vec::new(),
        };
        assert_eq!(
            c.call(1, "hello", json!({"client":"t","proto":1}))["ok"],
            true
        );
        assert_eq!(
            c.call(2, "subscribe", json!({"topics":["notice"]}))["ok"],
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
            self.events.push(v);
        }
    }
    /// Next notice of `kind` (and `state`/`op` when given).
    fn notice(&mut self, kind: &str, key: &str, val: &str) -> Value {
        let m = |v: &Value| {
            v["event"] == "notice"
                && v["data"]["kind"] == kind
                && (key.is_empty() || v["data"][key] == val)
        };
        if let Some(i) = self.events.iter().position(m) {
            return self.events.remove(i)["data"].clone();
        }
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(10) {
            let v = self.recv();
            if m(&v) {
                return v["data"].clone();
            }
            self.events.push(v);
        }
        panic!("no notice {kind} {key}={val}");
    }
    fn bt(&mut self, id: u64) -> Value {
        self.call(id, "get_status", Value::Null)["data"]["bluetooth"].clone()
    }
    /// Poll the status until `f` holds.
    fn until(&mut self, what: &str, f: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        let mut id = 1000;
        while t.elapsed() < Duration::from_secs(10) {
            id += 1;
            let b = self.bt(id);
            if f(&b) {
                return b;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("timeout waiting for {what}: {}", self.bt(9999));
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
fn script(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, body).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}
impl Rig {
    /// `controller`: the fake controller exists (and is powered off) at core start.
    fn new(tag: &str, installed: bool, controller: bool) -> Rig {
        Rig::with(tag, installed, controller, None)
    }
    fn with(tag: &str, installed: bool, controller: bool, enabled: Option<&str>) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-bt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        if installed {
            script(&root, "usr/bin/bluetoothctl", FAKE_CTL);
            script(&root, "usr/local/sbin/walpad-bluetooth", FAKE_SCRIPT);
            let s = root.join("bt");
            std::fs::create_dir_all(&s).unwrap();
            std::fs::write(
                s.join("devices"),
                "AA:BB:CC:00:11:22|Logitech K380|yes|no|-60|input-keyboard\n",
            )
            .unwrap();
            if controller {
                std::fs::write(s.join("controller"), "").unwrap();
            }
        }
        if let Some(e) = enabled {
            std::fs::create_dir_all(root.join("var/lib/walpad")).unwrap();
            std::fs::write(root.join("var/lib/walpad/bluetooth-enabled"), e).unwrap();
        }
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.bluetooth_mut().set_scan_limit(Duration::from_secs(2));
        let l = d.bluetooth_mut().limits();
        l.pair_ms.store(3000, std::sync::atomic::Ordering::Relaxed);
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
        std::fs::read_to_string(self.root.join("bt/calls")).unwrap_or_default()
    }
    fn script_calls(&self) -> String {
        std::fs::read_to_string(self.root.join("bt/script-calls")).unwrap_or_default()
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

const PHONE: &str = "66:55:44:33:22:11";
const KBD: &str = "AA:BB:CC:00:11:22";

fn dev<'a>(b: &'a Value, a: &str) -> Option<&'a Value> {
    b["devices"].as_array()?.iter().find(|d| d["address"] == a)
}

#[test]
fn present_controller_is_powered_on_and_lists_devices() {
    let rig = Rig::new("list", true, true);
    let mut c = Conn::new(&rig.root);
    let b = c.until("present and powered", |b| {
        b["controller"] == "present" && b["powered"] == true
    });
    assert_eq!(b["enabled"], true);
    assert_eq!(b["discovering"], false);
    let k = dev(&b, KBD).unwrap();
    assert_eq!(k["name"], "Logitech K380");
    assert_eq!(k["paired"], true);
    assert_eq!(k["connected"], false);
    assert_eq!(k["rssi"], -60);
    assert_eq!(k["icon"], "input-keyboard");
    let s = c.call(5, "get_status", Value::Null);
    assert_eq!(s["data"]["hardware"]["bluetooth"], "ok");
}

#[test]
fn scan_is_bounded_and_finds_devices() {
    let rig = Rig::new("scan", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    let r = c.call(10, "bluetooth_scan", json!({"on": true}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(c.bt(11)["discovering"], true);
    let b = c.until("phone found", |b| dev(b, PHONE).is_some());
    assert_eq!(dev(&b, PHONE).unwrap()["paired"], false);
    // the fake runs `--timeout 2`: discovery ends by itself
    c.until("scan over", |b| b["discovering"] == false);
    assert!(
        rig.calls().contains("--timeout 2 scan on"),
        "{}",
        rig.calls()
    );
    // manual stop
    assert_eq!(
        c.call(12, "bluetooth_scan", json!({"on": true}))["ok"],
        true
    );
    assert_eq!(
        c.call(13, "bluetooth_scan", json!({"on": false}))["ok"],
        true
    );
    c.until("stopped", |b| b["discovering"] == false);
}

#[test]
fn pairing_with_confirmation_then_connect_forget() {
    let rig = Rig::new("pair", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    // not waiting yet
    let r = c.call(
        9,
        "bluetooth_confirm",
        json!({"address": PHONE, "accept": true}),
    );
    assert_eq!(r["error"]["code"], "refused");
    // discover the phone
    assert_eq!(
        c.call(10, "bluetooth_scan", json!({"on": true}))["ok"],
        true
    );
    c.until("phone found", |b| dev(b, PHONE).is_some());
    let r = c.call(
        11,
        "bluetooth_pair",
        json!({"address": PHONE.to_lowercase()}),
    );
    assert_eq!(r["ok"], true, "{r}");
    let n = c.notice("bluetooth_pair", "state", "confirm");
    assert_eq!(n["address"], PHONE);
    assert_eq!(n["passkey"], "123456");
    assert_eq!(c.bt(12)["pairing"], PHONE);
    // a second pairing is refused while one runs
    let r = c.call(13, "bluetooth_pair", json!({"address": KBD}));
    assert_eq!(r["error"]["code"], "busy");
    let r = c.call(
        14,
        "bluetooth_confirm",
        json!({"address": PHONE, "accept": true}),
    );
    assert_eq!(r["ok"], true, "{r}");
    c.notice("bluetooth_pair", "state", "ok");
    let b = c.until("paired and connected", |b| {
        dev(b, PHONE).is_some_and(|d| d["paired"] == true && d["connected"] == true)
    });
    assert_eq!(b["pairing"], Value::Null);
    // disconnect, connect, forget
    assert_eq!(
        c.call(20, "bluetooth_disconnect", json!({"address": PHONE}))["ok"],
        true
    );
    c.until("disconnected", |b| {
        dev(b, PHONE).is_some_and(|d| d["connected"] == false)
    });
    assert_eq!(
        c.call(21, "bluetooth_connect", json!({"address": PHONE}))["ok"],
        true
    );
    c.until("connected", |b| {
        dev(b, PHONE).is_some_and(|d| d["connected"] == true)
    });
    assert_eq!(
        c.call(22, "bluetooth_forget", json!({"address": PHONE}))["ok"],
        true
    );
    c.until("forgotten", |b| dev(b, PHONE).is_none());
    // only addresses and fixed words reached bluetoothctl
    for l in rig.calls().lines() {
        for w in l.split_whitespace() {
            assert!(
                w.len() == 17 && w.contains(':')
                    || [
                        "show",
                        "devices",
                        "info",
                        "power",
                        "on",
                        "off",
                        "scan",
                        "--timeout",
                        "2",
                        "connect",
                        "disconnect",
                        "remove"
                    ]
                    .contains(&w),
                "unexpected argv word {w:?} in {l:?}"
            );
        }
    }
}

#[test]
fn pairing_declined_reports_failure() {
    let rig = Rig::new("decline", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    assert_eq!(
        c.call(11, "bluetooth_scan", json!({"on": true}))["ok"],
        true
    );
    c.until("phone found", |b| dev(b, PHONE).is_some());
    assert_eq!(
        c.call(12, "bluetooth_pair", json!({"address": PHONE}))["ok"],
        true
    );
    c.notice("bluetooth_pair", "state", "confirm");
    assert_eq!(
        c.call(
            13,
            "bluetooth_confirm",
            json!({"address": PHONE, "accept": false})
        )["ok"],
        true
    );
    let n = c.notice("bluetooth_pair", "state", "failed");
    assert_eq!(n["error"], "Authentication failed");
    let b = c.until("not paired", |b| b["pairing"].is_null());
    assert_eq!(dev(&b, PHONE).unwrap()["paired"], false);
}

#[test]
fn keyboard_passkey_and_legacy_pin() {
    let rig = Rig::new("kbd", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    std::fs::write(rig.root.join("bt/pair-mode"), "keyboard").unwrap();
    assert_eq!(
        c.call(12, "bluetooth_pair", json!({"address": KBD}))["ok"],
        true
    );
    let n = c.notice("bluetooth_pair", "state", "passkey");
    assert_eq!(n["passkey"], "482913");
    c.notice("bluetooth_pair", "state", "ok");
    c.until("idle", |b| b["pairing"].is_null());
    std::fs::write(rig.root.join("bt/pair-mode"), "pin").unwrap();
    assert_eq!(
        c.call(13, "bluetooth_pair", json!({"address": KBD}))["ok"],
        true
    );
    c.notice("bluetooth_pair", "state", "ok");
    assert_eq!(
        std::fs::read_to_string(rig.root.join("bt/pin-answer"))
            .unwrap()
            .trim(),
        "0000"
    );
}

#[test]
fn silent_device_times_out() {
    let rig = Rig::new("hang", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    std::fs::write(rig.root.join("bt/pair-mode"), "hang").unwrap();
    assert_eq!(
        c.call(12, "bluetooth_pair", json!({"address": KBD}))["ok"],
        true
    );
    let n = c.notice("bluetooth_pair", "state", "failed");
    assert_eq!(n["error"], "Pairing timed out");
    c.until("idle", |b| b["pairing"].is_null());
}

#[test]
fn failed_connect_gives_a_notice() {
    let rig = Rig::new("cfail", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    std::fs::write(rig.root.join("bt/connect-fail"), "").unwrap();
    assert_eq!(
        c.call(12, "bluetooth_connect", json!({"address": KBD}))["ok"],
        true
    );
    let n = c.notice("bluetooth", "op", "connect");
    assert_eq!(n["ok"], false);
    assert_eq!(n["address"], KBD);
    assert!(n["error"]
        .as_str()
        .unwrap()
        .starts_with("Could not connect"));
}

#[test]
fn switch_persists_and_runs_the_script() {
    let rig = Rig::new("switch", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    assert_eq!(
        c.call(10, "bluetooth_enable", json!({"on": false}))["ok"],
        true
    );
    assert_eq!(
        std::fs::read_to_string(rig.root.join("var/lib/walpad/bluetooth-enabled")).unwrap(),
        "0\n"
    );
    let n = c.notice("bluetooth", "op", "enable");
    assert_eq!(n["ok"], true);
    let b = c.until("off", |b| b["controller"] == "absent");
    assert_eq!(b["enabled"], false);
    assert_eq!(b["powered"], false);
    assert!(rig.script_calls().contains("stop"));
    // scanning needs a powered controller
    let r = c.call(11, "bluetooth_scan", json!({"on": true}));
    assert_eq!(r["error"]["code"], "refused");
    assert_eq!(
        c.call(12, "bluetooth_enable", json!({"on": true}))["ok"],
        true
    );
    c.notice("bluetooth", "op", "enable");
    let b = c.until("on again", |b| {
        b["controller"] == "present" && b["powered"] == true
    });
    assert_eq!(b["enabled"], true);
    assert!(rig.script_calls().contains("start"));
    assert_eq!(
        std::fs::read_to_string(rig.root.join("var/lib/walpad/bluetooth-enabled")).unwrap(),
        "1\n"
    );
}

#[test]
fn start_failure_is_reported() {
    let rig = Rig::new("startfail", true, false);
    let mut c = Conn::new(&rig.root);
    let b = c.until("absent", |b| b["controller"] == "absent");
    assert_eq!(b["powered"], false);
    std::fs::write(rig.root.join("bt/start-fail"), "").unwrap();
    assert_eq!(
        c.call(10, "bluetooth_enable", json!({"on": true}))["ok"],
        true
    );
    let n = c.notice("bluetooth", "op", "enable");
    assert_eq!(n["ok"], false);
    assert_eq!(n["error"], "bridge did not create an HCI device");
    let b = c.until("reason", |b| b["reason"].is_string());
    assert_eq!(b["reason"], "bridge did not create an HCI device");
}

#[test]
fn disabled_switch_stays_off_at_startup() {
    let rig = Rig::with("dis", true, true, Some("0\n"));
    let mut c = Conn::new(&rig.root);
    let b = c.until("present", |b| b["controller"] == "present");
    assert_eq!(b["enabled"], false);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(c.bt(5)["powered"], false);
    assert!(!rig.calls().contains("power on"), "{}", rig.calls());
}

#[test]
fn not_installed_is_absent_and_refuses() {
    let rig = Rig::new("none", false, false);
    let mut c = Conn::new(&rig.root);
    let b = c.bt(5);
    assert_eq!(b["controller"], "absent");
    assert!(b["reason"].as_str().unwrap().contains("not installed"));
    let s = c.call(6, "get_status", Value::Null);
    assert_eq!(s["data"]["hardware"]["bluetooth"], "unavailable");
    for (cmd, args) in [
        ("bluetooth_enable", json!({"on": true})),
        ("bluetooth_scan", json!({"on": true})),
        ("bluetooth_pair", json!({"address": KBD})),
        ("bluetooth_connect", json!({"address": KBD})),
    ] {
        assert_eq!(c.call(7, cmd, args)["error"]["code"], "refused", "{cmd}");
    }
}

#[test]
fn bad_addresses_never_reach_bluetoothctl() {
    let rig = Rig::new("bad", true, true);
    let mut c = Conn::new(&rig.root);
    c.until("powered", |b| b["powered"] == true);
    for bad in [
        "-h",
        "AA:BB:CC:DD:EE:FF; reboot",
        "../../etc",
        "AA:BB:CC:DD:EE",
    ] {
        for cmd in [
            "bluetooth_pair",
            "bluetooth_connect",
            "bluetooth_disconnect",
            "bluetooth_forget",
            "bluetooth_confirm",
        ] {
            let r = c.call(7, cmd, json!({"address": bad, "accept": true}));
            assert_eq!(r["error"]["code"], "invalid_args", "{cmd} {bad}");
        }
    }
    assert!(!rig.calls().contains("reboot"));
}
