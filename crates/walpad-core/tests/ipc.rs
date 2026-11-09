use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;

struct Harness {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    pub root: std::path::PathBuf,
}
impl Harness {
    fn start(name: &str) -> Harness {
        let root = std::env::temp_dir().join(format!("walpad-ipc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        fake::populate(&root).unwrap();
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let handle = std::thread::spawn(move || d.run(&s2));
        Harness {
            stop,
            handle: Some(handle),
            root,
        }
    }
    fn sock(&self) -> std::path::PathBuf {
        self.root.join("run/walpad/control.sock")
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl Conn {
    fn new(h: &Harness) -> Conn {
        let s = UnixStream::connect(h.sock()).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
        }
    }
    fn send(&mut self, v: Value) {
        writeln!(self.w, "{v}").unwrap();
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
    fn call(&mut self, id: u64, cmd: &str, args: Value) -> Value {
        self.send(json!({"v":1,"id":id,"cmd":cmd,"args":args}));
        self.recv()
    }
    fn hello(&mut self) {
        let r = self.call(1, "hello", json!({"client":"test","proto":1}));
        assert_eq!(r["ok"], true);
    }
}

#[test]
fn hello_is_required_and_version_checked() {
    let h = Harness::start("hello");
    let mut c = Conn::new(&h);
    let r = c.call(1, "get_status", Value::Null);
    assert_eq!(r["error"]["code"], "not_hello");
    let r = c.call(2, "hello", json!({"client":"t","proto":2}));
    assert_eq!(r["error"]["code"], "version");
    c.hello();
    assert_eq!(c.call(3, "get_status", Value::Null)["ok"], true);
}

#[test]
fn status_has_fixture_values_and_honest_battery() {
    let h = Harness::start("status");
    let mut c = Conn::new(&h);
    c.hello();
    let r = c.call(2, "get_status", Value::Null);
    let d = &r["data"];
    assert_eq!(d["battery"]["source"], "voltage_estimate");
    assert_eq!(d["battery"]["percent"], 62);
    assert_eq!(d["brightness"], 120);
    assert_eq!(d["wifi"]["radio"], "up");
    assert_eq!(d["wifi"]["ip"], "192.168.0.100");
    assert_eq!(d["hardware"]["audio"], "unverified");
    assert_eq!(d["hardware"]["touch"], "ok");
}

#[test]
fn brightness_rotate_screen_and_errors() {
    let h = Harness::start("cmds");
    let mut c = Conn::new(&h);
    c.hello();
    assert_eq!(
        c.call(2, "set_brightness", json!({"value":200}))["data"]["brightness"],
        200
    );
    let bl =
        std::fs::read_to_string(h.root.join("sys/class/leds/lcd-backlight/brightness")).unwrap();
    assert_eq!(bl.trim(), "200");
    assert_eq!(
        c.call(3, "set_brightness", json!({"delta":100}))["data"]["brightness"],
        255
    );
    assert_eq!(
        c.call(4, "set_brightness", json!({"value":999}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(5, "rotate", json!({"mode":"90"}))["data"]["orientation"],
        90
    );
    assert_eq!(
        std::fs::read_to_string(h.root.join("run/walpad/orientation")).unwrap(),
        "90"
    );
    assert_eq!(
        c.call(6, "screen", json!({"mode":"off"}))["data"]["screen"],
        "off"
    );
    assert_eq!(
        c.call(7, "bogus", Value::Null)["error"]["code"],
        "unknown_command"
    );
    assert_eq!(
        c.call(8, "power", json!({"action":"poweroff"}))["error"]["code"],
        "refused"
    );
    assert_eq!(c.call(9, "wifi_scan", Value::Null)["ok"], false);
}

#[test]
fn status_event_on_change_and_devd_fifo_and_legacy_json() {
    let h = Harness::start("events");
    let mut c = Conn::new(&h);
    c.hello();
    assert_eq!(
        c.call(2, "subscribe", json!({"topics":["status"]}))["ok"],
        true
    );
    // first event arrives (initial push), drain it
    let first = c.recv();
    assert_eq!(first["event"], "status");
    // legacy FIFO command changes state -> event
    std::fs::write(h.root.join("run/walpad/devd.cmd"), "brightness 33\n").unwrap();
    loop {
        let e = c.recv();
        if e["event"] == "status" && e["data"]["brightness"] == 33 {
            break;
        }
    }
    // status.json compat file has the old field set
    std::thread::sleep(Duration::from_millis(300));
    let j: Value = serde_json::from_str(
        &std::fs::read_to_string(h.root.join("run/walpad/status.json")).unwrap(),
    )
    .unwrap();
    let mut keys: Vec<_> = j.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "battery",
            "brightness",
            "lid_closed",
            "low_battery",
            "orientation",
            "rotation_locked",
            "screen",
            "time",
            "uptime_s",
            "wifi"
        ]
    );
}

#[test]
fn oversized_line_closes_connection_and_socket_mode_is_0600() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::start("big");
    let mode = std::fs::metadata(h.sock()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let mut s = UnixStream::connect(h.sock()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let big = vec![b'x'; 70 * 1024];
    let _ = s.write_all(&big);
    let mut r = BufReader::new(s);
    let mut l = String::new();
    assert_eq!(
        r.read_line(&mut l).unwrap_or(0),
        0,
        "connection closed without a reply"
    );
}

#[test]
fn settings_launch_screenshot_power_and_restart_shell() {
    let root = std::env::temp_dir().join(format!("walpad-ipc-{}-newcmds", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("usr/bin")).unwrap();
    std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
    fake::populate(&root).unwrap();
    std::fs::write(root.join("usr/bin/sleep"), "").unwrap();
    std::fs::write(
        root.join("etc/walpad/apps.json"),
        r#"[{"id":"sleeper","name":"S","exec":["sleep","30"],"single_instance":true},{"id":"ghost","name":"G","exec":["nope-not-here"]}]"#,
    )
    .unwrap();
    std::fs::create_dir_all(root.join("run/walpad/xfb")).unwrap();
    std::fs::write(root.join("run/walpad/xfb/Xvfb_screen0"), "XWD").unwrap();
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let handle = std::thread::spawn(move || d.run(&s2));
    let h = Harness {
        stop,
        handle: Some(handle),
        root: root.clone(),
    };

    let mut c = Conn::new(&h);
    c.hello();
    // set_setting rewrites devd.json atomically and keeps other keys
    std::fs::write(root.join("etc/walpad/devd.json"), r#"{"tilt_min":1234}"#).unwrap();
    let r = c.call(2, "set_setting", json!({"key":"dim_after_s","value":45}));
    assert_eq!(r["ok"], true, "{r}");
    let j: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("etc/walpad/devd.json")).unwrap())
            .unwrap();
    assert_eq!(
        (j["dim_after_s"].clone(), j["tilt_min"].clone()),
        (json!(45), json!(1234))
    );
    assert_eq!(
        c.call(3, "set_setting", json!({"key":"clock_24h","value":true}))["ok"],
        true
    );
    assert!(root.join("etc/walpad/shell.json").exists());
    assert_eq!(
        c.call(4, "set_setting", json!({"key":"nope","value":1}))["error"]["code"],
        "invalid_args"
    );

    // apps: only installed ones are listed; launch by id only
    let st = c.call(5, "get_status", Value::Null);
    assert_eq!(
        st["data"]["apps"],
        json!([{"app":"sleeper","running":false}])
    );
    assert_eq!(
        c.call(6, "launch", json!({"app":"ghost"}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call(7, "launch", json!({"app":"rm -rf /"}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(8, "launch", json!({"app":"zzz"}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(9, "launch", json!({"app":"sleeper"}))["data"]["result"],
        "launched"
    );
    assert_eq!(
        c.call(10, "launch", json!({"app":"sleeper"}))["data"]["result"],
        "already_running"
    );
    assert_eq!(
        c.call(11, "get_status", Value::Null)["data"]["apps"][0]["running"],
        true
    );

    // screenshot copies the XWD, power needs confirmation, restart_shell finds nothing in a test root
    let r = c.call(12, "screenshot", Value::Null);
    let path = r["data"]["path"].as_str().unwrap().to_string();
    assert!(root.join(path.trim_start_matches('/')).exists());
    assert_eq!(
        c.call(13, "power", json!({"action":"reboot"}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call(14, "power", json!({"action":"reboot","confirmed":true}))["ok"],
        true
    );
    assert_eq!(
        c.call(15, "restart_shell", Value::Null)["data"]["terminated"],
        0
    );
    assert_eq!(c.call(16, "home", Value::Null)["data"]["focused"], false);
}
