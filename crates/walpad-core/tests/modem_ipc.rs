//! The modem through the daemon's IPC socket (owner request 2026-11-10). A fake modem answers AT
//! commands on a real pseudo-terminal pair (the core opens the slave like /dev/radio/pttycmd1);
//! walpad-modem is a fake script in the test root that records its argv.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake as fakeroot, modem::Timing};
use walpad_hw::Root;

const PIN: &str = "2468";
const IMEI: &str = "351234567890123";

const FAKE_SCRIPT: &str = r#"#!/bin/sh
R=$WALPAD_MODEM_ROOT
echo "$*" >> "$R/modem-calls"
case "$1 $2" in
  "start "*)
    if [ -f "$R/start-fail" ]; then echo "[modem] after 60s: md_en=0"; exit 1; fi
    mkdir -p "$R/dev/radio"; ln -sfn "$(cat "$R/pty-path")" "$R/dev/radio/pttycmd1"
    echo "[modem] after 3s: md_en=ready" ;;
  "data on")
    APN=$(sed -n 's/^apn=//p' "$R/etc/walpad/mobile.conf" 2>/dev/null); APN=${APN:-internet}
    if [ -f "$R/data-fail" ]; then printf 'state=failed\nreason=data link not supported yet\nip=10.0.0.5\napn=%s\n' "$APN"; exit 1; fi
    printf 'state=on\nip=10.0.0.5\napn=%s\n' "$APN"; echo on > "$R/data-state" ;;
  "data off") echo state=off; rm -f "$R/data-state" ;;
  "data status") [ -f "$R/data-state" ] && echo state=on || echo state=off ;;
esac
"#;

/// What the fake modem knows.
struct FakeState {
    cfun: u8,
    locked: bool,
    log: Vec<String>,
}

fn reply(st: &mut FakeState, cmd: &str) -> String {
    let ok = |body: &str| {
        if body.is_empty() {
            "\r\nOK\r\n".to_string()
        } else {
            format!("\r\n{body}\r\n\r\nOK\r\n")
        }
    };
    let net = st.cfun == 1 && !st.locked;
    match cmd {
        "ATE0" | "AT+CMEE=1" | "AT+COPS=3,0" => ok(""),
        "ATI" => ok("Manufacturer: MediaTek Inc.\r\nModel: MTK_MODEM\r\nRevision: MOLY.TEST.V1"),
        "AT+CGSN" => ok(IMEI),
        "AT+CFUN?" => format!("\r\n+EIND: 64\r\n{}", ok(&format!("+CFUN: {}", st.cfun))),
        "AT+CFUN=1" => {
            st.cfun = 1;
            ok("")
        }
        "AT+CFUN=0" => {
            st.cfun = 0;
            ok("")
        }
        "AT+CPIN?" if st.locked => ok("+CPIN: SIM PIN"),
        "AT+CPIN?" => ok("+CPIN: READY"),
        c if c.starts_with("AT+CPIN=") => {
            if c == format!("AT+CPIN=\"{PIN}\"") {
                st.locked = false;
                ok("")
            } else {
                "\r\n+CME ERROR: 16\r\n".into()
            }
        }
        "AT+CCID" => ok("+CCID: \"8988012345678901234F\""),
        "AT+CSQ" => ok("+CSQ: 29, 5"),
        "AT+CREG?" | "AT+CGREG?" | "AT+CEREG?" => {
            let p = &cmd[2..cmd.len() - 1];
            ok(&format!("{p}: 0,{}", if net { 1 } else { 2 }))
        }
        "AT+COPS?" if net => ok("+COPS: 0,0,\"Grameenphone\",3"),
        "AT+COPS?" => ok("+COPS: 0"),
        _ => "\r\nERROR\r\n".into(),
    }
}

/// A pty pair whose master end is a fake modem thread.
struct FakeModem {
    slave: String,
    state: Arc<Mutex<FakeState>>,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}

impl FakeModem {
    fn new(locked: bool) -> FakeModem {
        use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
        let m: OwnedFd = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        grantpt(&m).unwrap();
        unlockpt(&m).unwrap();
        let slave = ptsname(&m, Vec::new()).unwrap().into_string().unwrap();
        // raw from the start (the core makes it raw as well when it opens it)
        {
            let s = rustix::fs::open(
                slave.as_str(),
                rustix::fs::OFlags::RDWR | rustix::fs::OFlags::NOCTTY,
                rustix::fs::Mode::empty(),
            )
            .unwrap();
            let mut t = rustix::termios::tcgetattr(&s).unwrap();
            t.make_raw();
            rustix::termios::tcsetattr(&s, rustix::termios::OptionalActions::Now, &t).unwrap();
        }
        let state = Arc::new(Mutex::new(FakeState {
            cfun: 0,
            locked,
            log: Vec::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (st2, stop2) = (state.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            use rustix::event::{poll, PollFd, PollFlags, Timespec};
            let mut buf = Vec::new();
            let mut chunk = [0u8; 512];
            while !stop2.load(Ordering::Relaxed) {
                let t = Timespec {
                    tv_sec: 0,
                    tv_nsec: 50_000_000,
                };
                let mut fds = [PollFd::new(&m, PollFlags::IN)];
                let _ = poll(&mut fds, Some(&t));
                match rustix::io::read(&m, &mut chunk) {
                    Ok(n) if n > 0 => buf.extend_from_slice(&chunk[..n]),
                    // EIO while no slave is open: the core opens the channel per job
                    _ => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                }
                while let Some(i) = buf.iter().position(|&b| b == b'\r') {
                    let line: Vec<u8> = buf.drain(..=i).collect();
                    let cmd = String::from_utf8_lossy(&line).trim().to_string();
                    if cmd.is_empty() {
                        continue;
                    }
                    let out = {
                        let mut s = st2.lock().unwrap();
                        s.log.push(cmd.clone());
                        reply(&mut s, &cmd)
                    };
                    let _ = rustix::io::write(&m, out.as_bytes());
                }
            }
        });
        FakeModem {
            slave,
            state,
            stop,
            h: Some(h),
        }
    }
    fn log(&self) -> Vec<String> {
        self.state.lock().unwrap().log.clone()
    }
}

impl Drop for FakeModem {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

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
            if v["event"] == "notice" {
                self.events.push(v["data"].clone());
            }
        }
    }
    fn mobile(&mut self) -> Value {
        self.call(99, "get_status", Value::Null)["data"]["mobile"].clone()
    }
    /// Poll the status until `f` holds (the fake modem answers within milliseconds).
    fn wait(&mut self, what: &str, f: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        loop {
            let m = self.mobile();
            if f(&m) {
                return m;
            }
            assert!(t.elapsed() < Duration::from_secs(15), "{what}: {m}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    /// Next notice of `kind` (already received ones first).
    fn notice(&mut self, kind: &str) -> Value {
        if let Some(i) = self.events.iter().position(|e| e["kind"] == kind) {
            return self.events.remove(i);
        }
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(15) {
            let v = self.recv();
            if v["event"] == "notice" && v["data"]["kind"] == kind {
                return v["data"].clone();
            }
        }
        panic!("no notice {kind}");
    }
}

struct Rig {
    root: PathBuf,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}

fn fast() -> Timing {
    Timing {
        at_cmd: Duration::from_secs(2),
        poll_on: Duration::from_millis(300),
        poll_off: Duration::from_millis(300),
        start: Duration::from_secs(5),
        data: Duration::from_secs(5),
        status: Duration::from_secs(5),
        recheck: Duration::from_millis(200),
    }
}

/// A test root; `script` installs the fake walpad-modem, `tty` links the AT channel already.
fn root(tag: &str, script: bool, tty: Option<&FakeModem>) -> PathBuf {
    let root = std::env::temp_dir().join(format!("walpad-modem-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    fakeroot::populate(&root).unwrap();
    if script {
        let s = root.join("usr/local/sbin/walpad-modem");
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        std::fs::write(&s, FAKE_SCRIPT).unwrap();
        std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    if let Some(f) = tty {
        std::fs::write(root.join("pty-path"), &f.slave).unwrap();
        std::fs::create_dir_all(root.join("dev/radio")).unwrap();
        std::os::unix::fs::symlink(&f.slave, root.join("dev/radio/pttycmd1")).unwrap();
    }
    root
}

impl Rig {
    fn start(root: &Path) -> Rig {
        let mut d = Daemon::new(Root::new(root)).unwrap();
        d.modem_mut().set_timing(fast());
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root: root.to_path_buf(),
            stop,
            h: Some(h),
        }
    }
    fn calls(&self) -> String {
        std::fs::read_to_string(self.root.join("modem-calls")).unwrap_or_default()
    }
    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}
impl Drop for Rig {
    fn drop(&mut self) {
        self.halt();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn sim_pin_radio_apn_and_data_over_ipc() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("full", true, Some(&fake)));
    let mut c = Conn::new(&rig.root);

    // ready at once (the channel exists), the default radio choice (on) is applied
    let m = c.wait("radio on, PIN asked", |m| {
        m["modem"] == "ready" && m["radio_on"] == true && m["sim"] == "pin_required"
    });
    assert_eq!(m["firmware"], "MOLY.TEST.V1");
    assert_eq!(
        m["operator"],
        Value::Null,
        "no operator while the SIM is locked"
    );
    assert!(fake.log().contains(&"AT+CFUN=1".to_string()));
    assert!(rig.calls().contains("data status"), "{}", rig.calls());

    // data refused while the SIM is locked
    let r = c.call(3, "mobile_data", json!({"on": true}));
    assert_eq!(r["error"]["code"], "refused", "{r}");

    // wrong PIN, then the right one
    let r = c.call(4, "sim_pin", json!({"pin": "0000"}));
    assert_eq!(r["data"]["accepted"], true, "{r}");
    let n = c.notice("mobile_pin");
    assert_eq!(
        (&n["ok"], &n["error"]),
        (&json!(false), &json!("Wrong PIN"))
    );
    let r = c.call(5, "sim_pin", json!({"pin": PIN}));
    assert_eq!(r["data"]["accepted"], true, "{r}");
    assert_eq!(c.notice("mobile_pin")["ok"], true);
    let m = c.wait("registered", |m| m["operator"] == "Grameenphone");
    assert_eq!(
        (
            &m["sim"],
            &m["tech"],
            &m["signal_bars"],
            &m["signal_dbm"],
            &m["registration"]
        ),
        (
            &json!("ready"),
            &json!("2G"),
            &json!(4),
            &json!(-55),
            &json!("home")
        )
    );
    // the PIN went over the channel only: not in any argv, not in the status
    assert!(fake.log().contains(&format!("AT+CPIN=\"{PIN}\"")));
    assert!(!rig.calls().contains(PIN));
    assert!(!c
        .call(6, "get_status", Value::Null)
        .to_string()
        .contains(PIN));
    // the SIM no longer asks
    assert_eq!(
        c.call(7, "sim_pin", json!({"pin": PIN}))["error"]["code"],
        "refused"
    );

    // identifiers only on request, never in the status
    let status = c.call(8, "get_status", Value::Null).to_string();
    assert!(!status.contains(IMEI) && !status.contains("8988012345678901234F"));
    let i = c.call(9, "mobile_info", Value::Null);
    assert_eq!(i["data"]["imei"], IMEI, "{i}");
    assert_eq!(i["data"]["firmware"], "MOLY.TEST.V1");

    // APN: validated, stored 0600, used by the script
    assert_eq!(
        c.call(10, "mobile_apn", json!({"apn": "bad apn"}))["error"]["code"],
        "invalid_args"
    );
    let r = c.call(11, "mobile_apn", json!({"apn": "gpinternet"}));
    assert_eq!(r["data"]["apn"], "gpinternet", "{r}");
    let conf = rig.root.join("etc/walpad/mobile.conf");
    assert_eq!(
        std::fs::metadata(&conf).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(std::fs::read_to_string(&conf)
        .unwrap()
        .contains("apn=gpinternet\n"));
    assert_eq!(c.mobile()["apn"], "gpinternet");

    // data on: the script brings it up
    let r = c.call(12, "mobile_data", json!({"on": true}));
    assert_eq!(r["data"]["data"], "connecting", "{r}");
    let n = c.notice("mobile_data");
    assert_eq!((&n["state"], &n["ip"]), (&json!("on"), &json!("10.0.0.5")));
    let m = c.wait("data on", |m| m["data"] == "on");
    assert_eq!(
        (&m["ip"], &m["apn_in_use"]),
        (&json!("10.0.0.5"), &json!("gpinternet"))
    );
    assert!(rig.calls().contains("data on"));

    // airplane mode: radio off, data follows it down, the choice is persisted
    c.events.clear(); // (the radio-on notice from the start)
    let r = c.call(13, "mobile_radio", json!({"on": false}));
    assert_eq!(r["data"]["applied"], true, "{r}");
    let n = c.notice("mobile_radio");
    assert_eq!((&n["on"], &n["ok"]), (&json!(false), &json!(true)));
    let m = c.wait("radio off, data off", |m| {
        m["radio_on"] == false && m["data"] == "off"
    });
    assert_eq!(
        (&m["operator"], &m["signal_bars"]),
        (&Value::Null, &Value::Null)
    );
    assert!(rig.calls().contains("data off"));
    assert_eq!(
        std::fs::read_to_string(rig.root.join("var/lib/walpad/mobile-radio"))
            .unwrap()
            .trim(),
        "0"
    );
    let r = c.call(14, "mobile_data", json!({"on": true}));
    assert_eq!(r["error"]["code"], "refused", "{r}");
    assert!(r["error"]["msg"].as_str().unwrap().contains("radio"));
}

#[test]
fn data_failure_is_reported_with_its_reason() {
    let fake = FakeModem::new(false);
    let rig = Rig::start(&root("datafail", true, Some(&fake)));
    std::fs::write(rig.root.join("data-fail"), "").unwrap();
    let mut c = Conn::new(&rig.root);
    c.wait("registered", |m| m["operator"] == "Grameenphone");
    assert_eq!(c.mobile()["apn"], Value::Null, "no APN configured");
    c.call(3, "mobile_data", json!({"on": true}));
    let n = c.notice("mobile_data");
    assert_eq!(
        (&n["state"], &n["reason"]),
        (&json!("failed"), &json!("data link not supported yet"))
    );
    let m = c.mobile();
    assert_eq!(
        (&m["data"], &m["data_reason"], &m["apn_in_use"]),
        (
            &json!("failed"),
            &json!("data link not supported yet"),
            &json!("internet")
        )
    );
}

#[test]
fn the_core_boots_the_modem_once_per_boot_and_respects_radio_off() {
    let fake = FakeModem::new(false);
    let dir = root("boot", true, None);
    std::fs::write(dir.join("pty-path"), &fake.slave).unwrap();
    // the owner switched the radio off earlier
    std::fs::create_dir_all(dir.join("var/lib/walpad")).unwrap();
    std::fs::write(dir.join("var/lib/walpad/mobile-radio"), "0\n").unwrap();
    let mut rig = Rig::start(&dir);
    let mut c = Conn::new(&rig.root);
    let m = c.wait("booted", |m| {
        m["modem"] == "ready" && m["radio_on"] == false
    });
    assert_eq!(m["sim"], "ready");
    assert_eq!(rig.calls().matches("start").count(), 1);
    assert!(
        !fake.log().contains(&"AT+CFUN=1".to_string()),
        "radio off is respected"
    );
    assert!(rig.root.join("run/walpad/modem-start-attempted").exists());
    drop(c);
    rig.halt();
    // a core restart in the same boot (the channel gone) does not start the modem again
    std::fs::remove_file(rig.root.join("dev/radio/pttycmd1")).unwrap();
    let rig2 = Rig::start(&rig.root);
    let mut c = Conn::new(&rig2.root);
    assert_eq!(c.mobile()["modem"], "present");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(rig2.calls().matches("start").count(), 1);
}

#[test]
fn a_failed_boot_and_a_missing_modem_are_reported() {
    let dir = root("bootfail", true, None);
    std::fs::write(dir.join("start-fail"), "").unwrap();
    let rig = Rig::start(&dir);
    let mut c = Conn::new(&rig.root);
    let m = c.wait("failed", |m| m["modem"] == "failed");
    assert_eq!(m["reason"], "the modem did not start: after 60s: md_en=0");
    assert_eq!(
        c.call(3, "mobile_data", json!({"on": true}))["error"]["code"],
        "refused"
    );
    drop(c);
    drop(rig);

    let rig = Rig::start(&root("absent", false, None));
    let mut c = Conn::new(&rig.root);
    assert_eq!(c.mobile()["modem"], "absent");
    assert_eq!(
        c.call(3, "mobile_radio", json!({"on": true}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call(4, "sim_pin", json!({"pin": "12"}))["error"]["code"],
        "invalid_args"
    );
}
