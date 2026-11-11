//! Header stats through the daemon's IPC socket (owner request 2026-11-11): the switches persist,
//! only enabled items are sampled, unreadable sources are unavailable.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake as fakeroot};
use walpad_hw::Root;

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
    fn call(&mut self, id: u64, cmd: &str, args: Value) -> Value {
        writeln!(self.w, "{}", json!({"v":1,"id":id,"cmd":cmd,"args":args})).unwrap();
        loop {
            let mut l = String::new();
            self.r.read_line(&mut l).unwrap();
            let v: Value = serde_json::from_str(&l).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }
    fn stats(&mut self, id: u64) -> Value {
        self.call(id, "get_status", Value::Null)["data"]["stats"].clone()
    }
    /// Poll the status until `ok` accepts the stats block.
    fn wait(&mut self, id: u64, mut ok: impl FnMut(&Value) -> bool) -> Value {
        let t = Instant::now();
        loop {
            let s = self.stats(id);
            if ok(&s) {
                return s;
            }
            assert!(t.elapsed() < Duration::from_secs(8), "{s}");
            std::thread::sleep(Duration::from_millis(60));
        }
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}
impl Rig {
    fn new(tag: &str, preset: impl FnOnce(&Path)) -> Rig {
        let root = std::env::temp_dir().join(format!("walpad-stats-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        fakeroot::populate(&root).unwrap();
        preset(&root);
        let mut d = Daemon::new(Root::new(&root)).unwrap();
        d.stats_mut().set_every(Duration::from_millis(100));
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root,
            stop,
            h: Some(h),
        }
    }
    fn put(&self, rel: &str, c: &str) {
        let p = self.root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
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

const NET0: &str =
    " wlan0: 1000 1 0 0 0 0 0 0 500 1 0 0 0 0 0 0\n    lo: 5 1 0 0 0 0 0 0 5 1 0 0 0 0 0 0\n";

#[test]
fn defaults_switch_persist_and_sample() {
    let rig = Rig::new("cycle", |r| {
        let w = |rel: &str, c: &str| {
            let p = r.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, c).unwrap();
        };
        w("proc/stat", "cpu  100 0 100 800 0 0 0 0\n");
        w("proc/meminfo", "MemTotal: 2000 kB\nMemAvailable: 500 kB\n");
        w("proc/net/dev", NET0);
    });
    let mut c = Conn::new(&rig.root);
    // CPU and RAM are on by default; ROM and network are off and carry no figures
    let s = c.stats(2);
    assert_eq!(
        s["items"],
        json!({"cpu": true, "ram": true, "rom": false, "net": false})
    );
    rig.put("proc/stat", "cpu  200 0 200 900 0 0 0 0\n");
    let s = c.wait(3, |s| !s["cpu_pct"].is_null());
    assert_eq!(s["cpu_pct"], 67, "{s}");
    assert_eq!(s["ram_used_kb"], 1500);
    assert_eq!(s["ram_total_kb"], 2000);
    assert!(s["rom_total_b"].is_null() && s["net_down_bps"].is_null());

    // turn ROM and network on, CPU off: only the named switches change
    let r = c.call(
        4,
        "stats_config",
        json!({"rom": true, "net": true, "cpu": false}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(
        r["data"]["items"],
        json!({"cpu": false, "ram": true, "rom": true, "net": true})
    );
    let saved: Value = serde_json::from_str(
        &std::fs::read_to_string(rig.root.join("var/lib/walpad/statusbar.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["net"], true);
    assert_eq!(saved["cpu"], false);
    let s = c.wait(5, |s| !s["rom_total_b"].is_null());
    assert!(s["cpu_pct"].is_null(), "{s}");
    assert!(s["rom_used_b"].as_u64().unwrap() <= s["rom_total_b"].as_u64().unwrap());
    // the byte counters move: 2 KB down and 1 KB up while the sampler runs
    let mut n = 0u64;
    let s = c.wait(6, |s| {
        n += 1;
        rig.put(
            "proc/net/dev",
            &format!(
                " wlan0: {} 1 0 0 0 0 0 0 {} 1 0 0 0 0 0 0\n    lo: 5 1 0 0 0 0 0 0 5 1 0 0 0 0 0 0\n",
                1000 + n * 5000,
                500 + n * 1000
            ),
        );
        s["net_down_bps"].as_u64().is_some_and(|v| v > 0)
    });
    assert!(s["net_up_bps"].as_u64().unwrap() > 0, "{s}");

    // everything off: the figures disappear
    let r = c.call(
        7,
        "stats_config",
        json!({"cpu": false, "ram": false, "rom": false, "net": false}),
    );
    assert_eq!(r["ok"], true);
    let s = c.stats(8);
    assert!(s["ram_used_kb"].is_null() && s["rom_total_b"].is_null() && s["net_up_bps"].is_null());

    // bad requests
    assert_eq!(
        c.call(9, "stats_config", json!({}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(10, "stats_config", json!({"cpu": "yes"}))["error"]["code"],
        "invalid_args"
    );
}

#[test]
fn stored_switches_are_read_at_start_and_missing_sources_are_unavailable() {
    let rig = Rig::new("stored", |r| {
        let p = r.join("var/lib/walpad/statusbar.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, r#"{"cpu":false,"ram":true,"rom":false,"net":true}"#).unwrap();
    });
    let mut c = Conn::new(&rig.root);
    let s = c.stats(2);
    assert_eq!(
        s["items"],
        json!({"cpu": false, "ram": true, "rom": false, "net": true})
    );
    // no /proc/meminfo or /proc/net/dev in the fake root: nothing is invented
    std::thread::sleep(Duration::from_millis(500));
    let s = c.stats(3);
    assert!(
        s["ram_used_kb"].is_null() && s["net_down_bps"].is_null(),
        "{s}"
    );
}
