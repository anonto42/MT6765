//! Wi-Fi through the daemon's IPC socket, with a fake supplicant behind it.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::wpa::fake;
use walpad_core::{daemon::Daemon, fake as fakeroot};
use walpad_hw::Root;

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl Conn {
    fn new(root: &std::path::Path) -> Conn {
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
    /// Send a request and return its response, skipping events that arrive in between.
    fn call(&mut self, id: u64, cmd: &str, args: Value) -> Value {
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        loop {
            let v = self.recv();
            if v["id"] == id {
                return v;
            }
        }
    }
    fn event(&mut self, kind: &str) -> Value {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            let v = self.recv();
            if v["event"] == "wifi" && v["data"]["kind"] == kind {
                return v["data"].clone();
            }
        }
        panic!("no wifi event {kind}");
    }
}

#[test]
fn scan_connect_forget_over_ipc() {
    let root = std::env::temp_dir().join(format!("walpad-wifiipc-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("etc/wpa_supplicant")).unwrap();
    fakeroot::populate(&root).unwrap();
    let conf = root.join("etc/wpa_supplicant/wpa_supplicant.conf");
    let sup = fake::spawn(&root, fake::default_aps(), Some(conf)).unwrap();
    let mut d = Daemon::new(Root::new(&root)).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let h = std::thread::spawn(move || d.run(&s2));

    let mut c = Conn::new(&root);
    assert_eq!(
        c.call(2, "subscribe", json!({"topics":["wifi","status"]}))["ok"],
        true
    );

    // scan: accepted at once, a second one is busy, results arrive as an event
    let t = Instant::now();
    loop {
        let r = c.call(3, "wifi_scan", Value::Null);
        if r["ok"] == true {
            assert_eq!(r["data"]["accepted"], true);
            break;
        }
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "supplicant never became reachable: {r}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(c.call(4, "wifi_scan", Value::Null)["error"]["code"], "busy");
    let scan = c.event("scan");
    let names: Vec<&str> = scan["networks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["ssid"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["Home", "Cafe Open", "Café"]);

    // connect to Home with the real passphrase, SSID given as hex
    let ssid_hex = scan["networks"][0]["ssid_hex"]
        .as_str()
        .unwrap()
        .to_string();
    let r = c.call(
        5,
        "wifi_connect",
        json!({"ssid": ssid_hex, "secret": "correct horse"}),
    );
    assert_eq!(r["data"]["accepted"], true, "{r}");
    let done = c.event("connect");
    assert_eq!(
        (
            done["ok"].clone(),
            done["error"].clone(),
            done["ssid"].clone()
        ),
        (json!(true), Value::Null, json!("Home"))
    );
    assert!(
        !sup.log.lock().unwrap().join("\n").contains("correct horse"),
        "passphrase never reaches the supplicant"
    );

    // saved list shows both networks, then forget the new one
    let saved = c.call(6, "wifi_list_saved", Value::Null);
    let ids: Vec<u64> = saved["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, vec![0, 1]);
    assert_eq!(c.call(7, "wifi_forget", json!({"saved_id": 1}))["ok"], true);
    assert_eq!(
        c.call(8, "wifi_list_saved", Value::Null)["data"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    // bad input is rejected by validation, not by the supplicant
    assert_eq!(
        c.call(
            9,
            "wifi_connect",
            json!({"ssid":"486f6d65","secret":"short"})
        )["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(
            10,
            "wifi_connect",
            json!({"ssid":"zz","secret":"correct horse"})
        )["error"]["code"],
        "invalid_args"
    );

    stop.store(true, Ordering::Relaxed);
    h.join().unwrap();
    drop(sup);
    let _ = std::fs::remove_dir_all(&root);
}
