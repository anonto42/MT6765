//! Volume through the daemon's IPC socket (owner request 2026-11-10). walpad-volume is faked inside
//! the test root: it records its argv and keeps the "sound server" state in files.
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

const FAKE_SCRIPT: &str = r#"#!/bin/sh
R=$WALPAD_VOLUME_ROOT
echo "$*" >> "$R/vol-calls"
[ -f "$R/vol-hang" ] && sleep 30
[ -f "$R/vol-down" ] && { printf 'volume=0\nmuted=0\navailable=0\nbackend=none\n'; exit 1; }
B=pulse; [ -f "$R/vol-mixer" ] && B=mixer
case $1 in
  apply) echo "$2 $3" > "$R/vol-state"; printf 'volume=%s\nmuted=%s\navailable=1\nbackend=%s\n' "$2" "$3" "$B" ;;
  test) sleep 1 ;;
  get) printf 'volume=1\nmuted=0\navailable=1\nbackend=%s\n' "$B" ;;
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
    fn audio(&mut self, id: u64) -> Value {
        self.call(id, "get_status", Value::Null)["data"]["audio"].clone()
    }
    /// Poll get_status until `pred(audio)` holds.
    fn wait_audio(&mut self, id: u64, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        let mut id = id;
        while t.elapsed() < Duration::from_secs(8) {
            let a = self.audio(id);
            if pred(&a) {
                return a;
            }
            id += 1;
            std::thread::sleep(Duration::from_millis(60));
        }
        panic!("timed out waiting for {what}");
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Rig {
    fn new(tag: &str, script: bool, setup: impl FnOnce(&Path)) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-vol-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        if script {
            let s = root.join("usr/local/sbin/walpad-volume");
            std::fs::create_dir_all(s.parent().unwrap()).unwrap();
            std::fs::write(&s, FAKE_SCRIPT).unwrap();
            std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        setup(&root);
        Rig::spawn(root)
    }
    fn spawn(root: PathBuf) -> Rig {
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.volume_mut().set_timeout(Duration::from_millis(700));
        d.volume_mut().set_probe_every(Duration::from_millis(300));
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root,
            stop,
            h: Some(h),
        }
    }
    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("vol-calls")).unwrap_or_default()
    }
    fn wait_calls(&self, needle: &str) {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(8) {
            if self.calls().contains(needle) {
                return;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        panic!("script never ran `{needle}`: {}", self.calls());
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        self.halt();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn volume_commands_apply_persist_and_report() {
    let mut rig = Rig::new("cmds", true, |_| {});
    let mut c = Conn::new(&rig.root);
    // the first probe finds the sound server and applies the stored (default) level
    let a = c.wait_audio(2, "available", |a| a["available"] == true);
    assert_eq!((&a["volume"], &a["muted"]), (&json!(60), &json!(false)));
    rig.wait_calls("apply 60 0");

    let r = c.call(100, "volume_set", json!({"percent": 30}));
    assert_eq!(
        r["data"],
        json!({"volume": 30, "muted": false, "available": true}),
        "{r}"
    );
    rig.wait_calls("apply 30 0");
    let r = c.call(101, "volume_delta", json!({"delta": -10}));
    assert_eq!(r["data"]["volume"], 20, "{r}");
    let r = c.call(102, "volume_delta", json!({"delta": 100}));
    assert_eq!(r["data"]["volume"], 100, "clamped: {r}");
    let r = c.call(103, "mute", json!({"on": true}));
    assert_eq!(r["data"]["muted"], true, "{r}");
    rig.wait_calls("apply 100 1");
    let s = c.audio(104);
    assert_eq!((&s["volume"], &s["muted"]), (&json!(100), &json!(true)));
    // setting the level keeps the mute state; raising it with a key-style delta unmutes
    c.call(105, "volume_set", json!({"percent": 40}));
    assert_eq!(c.audio(106)["muted"], true);
    let r = c.call(107, "volume_delta", json!({"delta": 5}));
    assert_eq!(
        r["data"],
        json!({"volume": 45, "muted": false, "available": true}),
        "{r}"
    );
    let r = c.call(108, "volume_delta", json!({"delta": -5}));
    assert_eq!(r["data"]["muted"], false, "{r}");

    // refused arguments never reach the script
    for (id, cmd, args) in [
        (110, "volume_set", json!({"percent": 101})),
        (111, "volume_set", json!({"percent": -1})),
        (112, "volume_set", json!({})),
        (113, "volume_delta", json!({"delta": 101})),
        (114, "mute", json!({})),
        (115, "mute", json!({"on": "yes"})),
    ] {
        let r = c.call(id, cmd, args.clone());
        assert_eq!(r["error"]["code"], "invalid_args", "{cmd} {args}: {r}");
    }
    assert!(!rig.calls().contains("apply 101") && !rig.calls().contains("apply -1"));

    // the state is persisted (coalesced write) and survives a restart
    c.call(120, "volume_set", json!({"percent": 35}));
    let f = rig.root.join("var/lib/walpad/volume");
    let t = Instant::now();
    while std::fs::read_to_string(&f).unwrap_or_default().trim() != "35 0" {
        assert!(
            t.elapsed() < Duration::from_secs(5),
            "state file {:?}",
            std::fs::read_to_string(&f)
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(c);
    let root = rig.root.clone();
    rig.halt();
    let rig2 = Rig::spawn(root);
    let mut c = Conn::new(&rig2.root);
    assert_eq!(c.audio(2)["volume"], 35);
    std::mem::forget(rig); // its Drop would delete the root while rig2 runs; rig2 cleans it up
}

#[test]
fn events_say_where_the_change_came_from() {
    let rig = Rig::new("event", true, |_| {});
    let mut c = Conn::new(&rig.root);
    assert_eq!(
        c.call(2, "subscribe", json!({"topics":["status"]}))["ok"],
        true
    );
    writeln!(
        c.w,
        "{}",
        json!({"v":1,"id":3,"cmd":"volume_set","args":{"percent":80}})
    )
    .unwrap();
    let t = Instant::now();
    loop {
        assert!(t.elapsed() < Duration::from_secs(5), "no audio event");
        let v = c.recv();
        if v["event"] == "audio" {
            assert_eq!(v["data"]["volume"], 80);
            assert_eq!(v["data"]["source"], "ipc");
            break;
        }
    }
}

#[test]
fn no_script_or_no_sound_means_unavailable_but_the_level_is_kept() {
    let rig = Rig::new("none", false, |_| {});
    let mut c = Conn::new(&rig.root);
    std::thread::sleep(Duration::from_millis(300));
    let r = c.call(2, "volume_set", json!({"percent": 20}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["data"],
        json!({"volume": 20, "muted": false, "available": false})
    );
    assert_eq!(c.audio(3)["available"], false);

    let rig = Rig::new("down", true, |r| {
        std::fs::write(r.join("vol-down"), "").unwrap()
    });
    let mut c = Conn::new(&rig.root);
    rig.wait_calls("get");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(c.audio(2)["available"], false);
}

#[test]
fn a_hanging_script_is_killed() {
    let rig = Rig::new("hang", true, |r| {
        std::fs::write(r.join("vol-hang"), "").unwrap()
    });
    let mut c = Conn::new(&rig.root);
    rig.wait_calls("get");
    let t = Instant::now();
    // the daemon keeps answering while the script hangs, and gives up on it after the timeout
    assert_eq!(c.audio(2)["available"], false);
    std::thread::sleep(Duration::from_millis(900));
    assert_eq!(c.audio(3)["available"], false);
    assert!(t.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_sound_server_that_starts_later_gets_the_stored_level() {
    let rig = Rig::new("late", true, |r| {
        std::fs::write(r.join("vol-mixer"), "").unwrap();
        std::fs::create_dir_all(r.join("var/lib/walpad")).unwrap();
        std::fs::write(r.join("var/lib/walpad/volume"), "42 0\n").unwrap();
    });
    let mut c = Conn::new(&rig.root);
    rig.wait_calls("apply 42 0");
    assert_eq!(c.audio(2)["volume"], 42);
    // pulse comes up: the next probe notices the new backend and applies the level again
    std::fs::remove_file(rig.root.join("vol-mixer")).unwrap();
    let before = rig.calls().matches("apply 42 0").count();
    let t = Instant::now();
    while rig.calls().matches("apply 42 0").count() == before {
        assert!(
            t.elapsed() < Duration::from_secs(15),
            "never re-applied: {}",
            rig.calls()
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[test]
fn test_tone_runs_the_script_once_at_a_time() {
    let rig = Rig::new("tone", true, |_| {});
    let mut c = Conn::new(&rig.root);
    let r = c.call(2, "volume_test", Value::Null);
    assert_eq!(r["data"]["playing"], true, "{r}");
    rig.wait_calls("test");
    let r = c.call(3, "volume_test", json!({}));
    assert_eq!(r["error"]["code"], "busy", "{r}");
    std::thread::sleep(Duration::from_millis(1400));
    let r = c.call(4, "volume_test", Value::Null);
    assert_eq!(r["data"]["playing"], true, "plays again once finished: {r}");
    // no script: a clear error, nothing else breaks
    let rig2 = Rig::new("tone-none", false, |_| {});
    let mut c = Conn::new(&rig2.root);
    let r = c.call(2, "volume_test", Value::Null);
    assert_eq!(r["error"]["code"], "internal", "{r}");
    assert!(r["error"]["msg"]
        .as_str()
        .unwrap()
        .contains("not installed"));
}
