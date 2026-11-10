//! Screen lock over the control socket (OpenSpec `lock-screen`): set, lock, unlock, rate limit,
//! locked-at-boot, rescue; and that no secret ever reaches the log or the lock file.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake};
use walpad_hw::Root;

struct Core {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Core {
    fn start(root: &std::path::Path) -> Core {
        let mut d = Daemon::new(Root::new(root)).unwrap();
        d.lock_mut().set_iterations(8);
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        Core {
            stop,
            handle: Some(std::thread::spawn(move || d.run(&s2))),
        }
    }
    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
impl Drop for Core {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Env {
    root: PathBuf,
}
impl Env {
    fn new(name: &str) -> Env {
        let root =
            std::env::temp_dir().join(format!("walpad-lockipc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        fake::populate(&root).unwrap();
        Env { root }
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

struct Conn {
    w: UnixStream,
    r: BufReader<UnixStream>,
}
impl Conn {
    fn new(env: &Env) -> Conn {
        let sock = env.root.join("run/walpad/control.sock");
        let t = Instant::now();
        let s = loop {
            match UnixStream::connect(&sock) {
                Ok(s) => break s,
                Err(_) if t.elapsed() < Duration::from_secs(3) => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                Err(e) => panic!("connect: {e}"),
            }
        };
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut c = Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
        };
        assert_eq!(
            c.call(1, "hello", json!({"client":"test","proto":1}))["ok"],
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
            if v.get("event").is_none() {
                return v;
            }
        }
    }
    fn lock(&mut self) -> Value {
        self.call(90, "get_status", Value::Null)["data"]["lock"].clone()
    }
}

fn wait_for(c: &mut Conn, what: &str, mut pred: impl FnMut(&Value) -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(4) {
        if pred(&c.lock()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for {what}: {}", c.lock());
}

#[test]
fn set_lock_screen_off_unlock_and_the_file_holds_no_pin() {
    let env = Env::new("flow");
    let _core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    assert_eq!(c.lock()["configured"], false);
    assert_eq!(c.lock()["locked"], false);

    // bad secrets are refused by the protocol
    for args in [
        json!({"kind":"pin","secret":"12"}),
        json!({"kind":"pin","secret":"12ab"}),
        json!({"kind":"pattern","secret":"0110"}),
        json!({"kind":"nonsense","secret":"1234"}),
    ] {
        assert_eq!(c.call(2, "lock_set", args)["error"]["code"], "invalid_args");
    }
    let r = c.call(3, "lock_set", json!({"kind":"pin","secret":"4321"}));
    assert_eq!(r["ok"], true, "{r}");
    let l = c.lock();
    assert_eq!(
        (l["configured"].clone(), l["kind"].clone()),
        (json!(true), json!("pin"))
    );
    assert_eq!(l["locked"], false, "setting a lock does not lock");

    let f = env.root.join("var/lib/walpad/lock.json");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(!std::fs::read_to_string(&f).unwrap().contains("4321"));

    // the screen turns off: locked at once; on again: still locked
    assert_eq!(c.call(4, "screen", json!({"mode":"off"}))["ok"], true);
    wait_for(&mut c, "locked after screen off", |l| l["locked"] == true);
    assert_eq!(c.call(5, "screen", json!({"mode":"on"}))["ok"], true);
    assert_eq!(c.lock()["locked"], true);

    let bad = c.call(6, "unlock", json!({"secret":"1111"}));
    assert_eq!(bad["data"]["unlocked"], false);
    assert_eq!(bad["data"]["attempts_left"], 4);
    assert_eq!(c.lock()["locked"], true);
    let ok = c.call(7, "unlock", json!({"secret":"4321"}));
    assert_eq!(ok["data"]["unlocked"], true);
    assert_eq!(c.lock()["locked"], false);
    assert!(!env.root.join("run/walpad/locked").exists());

    // lock_now locks without touching the screen
    assert_eq!(c.call(8, "lock_now", Value::Null)["data"]["locked"], true);
    assert_eq!(c.lock()["locked"], true);
    assert!(env.root.join("run/walpad/locked").exists());
}

#[test]
fn five_wrong_pins_show_a_countdown_and_a_right_pin_during_it_is_refused() {
    let env = Env::new("rate");
    let _core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    c.call(2, "lock_set", json!({"kind":"pin","secret":"4321"}));
    c.call(3, "lock_now", Value::Null);
    for i in 0..4 {
        let r = c.call(10 + i, "unlock", json!({"secret":"0000"}));
        assert_eq!(r["data"]["unlocked"], false);
        assert!(r["data"]["retry_in_s"].is_null());
    }
    let r = c.call(20, "unlock", json!({"secret":"0000"}));
    let wait = r["data"]["retry_in_s"].as_u64().unwrap();
    assert!((29..=30).contains(&wait), "{r}");
    let l = c.lock();
    assert!(l["retry_in_s"].as_u64().unwrap() >= 28, "{l}");
    let r = c.call(21, "unlock", json!({"secret":"4321"}));
    assert_eq!(
        r["data"]["unlocked"], false,
        "right PIN during the delay: {r}"
    );
    assert_eq!(c.lock()["locked"], true);
    // changing or removing the lock is rate limited the same way
    let r = c.call(22, "lock_remove", json!({"current":"4321"}));
    assert_eq!(r["error"]["code"], "refused");
    assert!(r["error"]["msg"].as_str().unwrap().contains("wait"));
}

#[test]
fn changing_and_removing_need_the_current_secret() {
    let env = Env::new("change");
    let _core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    c.call(2, "lock_set", json!({"kind":"pin","secret":"4321"}));
    let r = c.call(
        3,
        "lock_set",
        json!({"kind":"password","secret":"correct horse"}),
    );
    assert_eq!(r["error"]["code"], "refused", "no current secret: {r}");
    let r = c.call(
        4,
        "lock_set",
        json!({"kind":"password","secret":"correct horse","current":"9999"}),
    );
    assert_eq!(r["error"]["code"], "refused");
    let r = c.call(
        5,
        "lock_set",
        json!({"kind":"password","secret":"correct horse","current":"4321"}),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(c.lock()["kind"], "password");
    assert_eq!(
        c.call(6, "lock_remove", json!({"current":"wrong one"}))["ok"],
        false
    );
    assert_eq!(
        c.call(7, "lock_remove", json!({"current":"correct horse"}))["ok"],
        true
    );
    let l = c.lock();
    assert_eq!(
        (l["configured"].clone(), l["locked"].clone()),
        (json!(false), json!(false))
    );
    assert!(!env.root.join("var/lib/walpad/lock.json").exists());
    // auto-lock delay needs a lock
    assert_eq!(
        c.call(8, "lock_config", json!({"auto_lock_s":30}))["error"]["code"],
        "refused"
    );
    c.call(9, "lock_set", json!({"kind":"pattern","secret":"01258"}));
    assert_eq!(
        c.call(10, "lock_config", json!({"auto_lock_s":30}))["ok"],
        true
    );
    assert_eq!(c.lock()["auto_lock_s"], 30);
    assert_eq!(
        c.call(11, "lock_config", json!({"auto_lock_s":99999}))["error"]["code"],
        "invalid_args"
    );
}

#[test]
fn the_tablet_comes_up_locked_after_a_reboot_but_not_after_a_core_restart() {
    let env = Env::new("boot");
    let mut core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    c.call(2, "lock_set", json!({"kind":"pin","secret":"4321"}));
    drop(c);
    core.stop();
    // core restart in the same boot: unlocked stays unlocked
    let mut core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    assert_eq!(c.lock()["locked"], false);
    drop(c);
    core.stop();
    // reboot: /run is empty again
    let _ = std::fs::remove_file(env.root.join("run/walpad/lock-boot"));
    let mut core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    assert_eq!(c.lock()["locked"], true, "locked at boot");
    // a core restart while locked stays locked (app crash / shell restart do not touch it either)
    drop(c);
    core.stop();
    let _core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    assert_eq!(c.lock()["locked"], true);
}

#[test]
fn removing_lock_json_is_the_rescue_and_unlocks() {
    let env = Env::new("rescue");
    let _core = Core::start(&env.root);
    let mut c = Conn::new(&env);
    c.call(2, "lock_set", json!({"kind":"pin","secret":"4321"}));
    c.call(3, "lock_now", Value::Null);
    assert_eq!(c.lock()["locked"], true);
    std::fs::remove_file(env.root.join("var/lib/walpad/lock.json")).unwrap();
    wait_for(&mut c, "unlocked after rescue", |l| {
        l["locked"] == false && l["configured"] == false
    });
    // the next wake shows no lock
    c.call(4, "screen", json!({"mode":"off"}));
    c.call(5, "screen", json!({"mode":"on"}));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(c.lock()["locked"], false);
}
