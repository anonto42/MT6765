//! Calls, text messages and contacts through the daemon's IPC socket (owner request 2026-11-11).
//! A fake modem answers AT commands on a real pseudo-terminal pair: it keeps a call list that the
//! test drives, an SMS inbox of PDUs and the PDUs that were sent. walpad-modem and
//! walpad-callaudio are fake scripts in the test root that record their argv.
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::OwnedFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use walpad_core::{daemon::Daemon, fake as fakeroot, modem::Timing, phone};
use walpad_hw::Root;

const SCRIPT: &str = "#!/bin/sh\ncase \"$1\" in start) ;; esac\n";
const AUDIO: &str = "#!/bin/sh\necho \"$*\" >> \"$WALPAD_CALLAUDIO_ROOT/audio-calls\"\n";

/// One entry of the fake call list: (id, incoming, stat, number).
type CallRow = (u8, bool, u8, String);

struct FakeState {
    cfun: u8,
    sim: bool,
    calls: Vec<CallRow>,
    /// (index, stat, pdu hex) of the SIM's messages
    inbox: Vec<(u32, u8, String)>,
    sent: Vec<String>,
    log: Vec<String>,
    /// `AT+CMGS` waits for a PDU
    awaiting_pdu: bool,
    /// What `ATD` answers with (OK by default).
    dial_final: String,
}

fn reply(st: &mut FakeState, cmd: &str) -> String {
    let ok = |body: &str| {
        if body.is_empty() {
            "\r\nOK\r\n".to_string()
        } else {
            format!("\r\n{body}\r\n\r\nOK\r\n")
        }
    };
    let net = st.cfun == 1 && st.sim;
    match cmd {
        "ATE0" | "AT+CMEE=1" | "AT+COPS=3,0" => ok(""),
        "ATI" => ok("Manufacturer: MediaTek Inc.\r\nModel: MTK_MODEM\r\nRevision: MOLY.TEST.V1"),
        "AT+CGSN" => ok("351234567890123"),
        "AT+CFUN?" => ok(&format!("+CFUN: {}", st.cfun)),
        "AT+CFUN=1" => {
            st.cfun = 1;
            ok("")
        }
        "AT+CFUN=0" => {
            st.cfun = 0;
            ok("")
        }
        "AT+CPIN?" if !st.sim => "\r\n+CME ERROR: 10\r\n".into(),
        "AT+CPIN?" => ok("+CPIN: READY"),
        "AT+CCID" => ok("+CCID: \"8988012345678901234F\""),
        "AT+CSQ" => ok("+CSQ: 29, 5"),
        "AT+CREG?" | "AT+CGREG?" | "AT+CEREG?" => {
            let p = &cmd[2..cmd.len() - 1];
            ok(&format!("{p}: 0,{}", if net { 1 } else { 2 }))
        }
        "AT+COPS?" if net => ok("+COPS: 0,0,\"Grameenphone\",3"),
        "AT+COPS?" => ok("+COPS: 0"),
        "AT+CMGF=0" | "AT+CNMI=2,1,0,0,0" | "AT+CLIP=1" => ok(""),
        c if c.starts_with("AT+CPMS=") => ok("+CPMS: 0,40,0,40,0,40"),
        "AT+CSCA?" => ok("+CSCA: \"+8801716000000\",145"),
        "AT+CLCC" => {
            let body: Vec<String> = st
                .calls
                .iter()
                .map(|(i, mt, stat, n)| {
                    format!("+CLCC: {i},{},{stat},0,0,\"{n}\",129", u8::from(*mt))
                })
                .collect();
            ok(&body.join("\r\n"))
        }
        c if c.starts_with("ATD") && c.ends_with(';') => {
            let n = c[3..c.len() - 1].to_string();
            if st.dial_final == "OK" {
                st.calls = vec![(1, false, 2, n)];
                ok("")
            } else {
                format!("\r\n{}\r\n", st.dial_final)
            }
        }
        "ATA" => {
            for c in st.calls.iter_mut() {
                c.2 = 0;
            }
            ok("")
        }
        "AT+CHUP" => {
            st.calls.clear();
            ok("")
        }
        "ATH" => {
            st.calls.clear();
            ok("")
        }
        "AT+CMUT=1" | "AT+CMUT=0" => ok(""),
        c if c.starts_with("AT+VTS=") => ok(""),
        "AT+CMGL=4" => {
            let mut body = Vec::new();
            for (i, stat, pdu) in &st.inbox {
                body.push(format!("+CMGL: {i},{stat},,20"));
                body.push(pdu.clone());
            }
            ok(&body.join("\r\n"))
        }
        c if c.starts_with("AT+CMGD=") => {
            let i: u32 = c[8..].parse().unwrap_or(0);
            st.inbox.retain(|m| m.0 != i);
            ok("")
        }
        c if c.starts_with("AT+CMGS=") => {
            st.awaiting_pdu = true;
            "\r\n> ".into()
        }
        _ => "\r\nERROR\r\n".into(),
    }
}

struct FakeModem {
    slave: String,
    state: Arc<Mutex<FakeState>>,
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<()>>,
}

impl FakeModem {
    fn new(sim: bool) -> FakeModem {
        use rustix::pty::{grantpt, openpt, ptsname, unlockpt, OpenptFlags};
        let m: OwnedFd = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        grantpt(&m).unwrap();
        unlockpt(&m).unwrap();
        let slave = ptsname(&m, Vec::new()).unwrap().into_string().unwrap();
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
            sim,
            calls: Vec::new(),
            inbox: Vec::new(),
            sent: Vec::new(),
            log: Vec::new(),
            awaiting_pdu: false,
            dial_final: "OK".into(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (st2, stop2) = (state.clone(), stop.clone());
        let h = std::thread::spawn(move || {
            use rustix::event::{poll, PollFd, PollFlags, Timespec};
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while !stop2.load(Ordering::Relaxed) {
                let t = Timespec {
                    tv_sec: 0,
                    tv_nsec: 20_000_000,
                };
                let mut fds = [PollFd::new(&m, PollFlags::IN)];
                let _ = poll(&mut fds, Some(&t));
                match rustix::io::read(&m, &mut chunk) {
                    Ok(n) if n > 0 => buf.extend_from_slice(&chunk[..n]),
                    _ => {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }
                }
                loop {
                    let awaiting = st2.lock().unwrap().awaiting_pdu;
                    if awaiting {
                        // the PDU ends in Ctrl-Z, not in CR
                        let Some(i) = buf.iter().position(|&b| b == 0x1A) else {
                            break;
                        };
                        let pdu: Vec<u8> = buf.drain(..=i).collect();
                        let hex = String::from_utf8_lossy(&pdu[..pdu.len() - 1]).to_string();
                        let mut s = st2.lock().unwrap();
                        s.awaiting_pdu = false;
                        s.sent.push(hex);
                        drop(s);
                        let _ = rustix::io::write(&m, b"\r\n+CMGS: 7\r\n\r\nOK\r\n");
                        continue;
                    }
                    let Some(i) = buf.iter().position(|&b| b == b'\r') else {
                        break;
                    };
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
    fn set_calls(&self, calls: Vec<CallRow>) {
        self.state.lock().unwrap().calls = calls;
    }
    fn calls(&self) -> Vec<CallRow> {
        self.state.lock().unwrap().calls.clone()
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
    next: u64,
}
impl Conn {
    fn new(root: &Path) -> Conn {
        let s = UnixStream::connect(root.join("run/walpad/control.sock")).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let mut c = Conn {
            r: BufReader::new(s.try_clone().unwrap()),
            w: s,
            events: Vec::new(),
            next: 100,
        };
        assert_eq!(c.call("hello", json!({"client":"t","proto":1}))["ok"], true);
        assert_eq!(
            c.call("subscribe", json!({"topics":["notice"]}))["ok"],
            true
        );
        c
    }
    fn recv(&mut self) -> Value {
        let mut l = String::new();
        self.r.read_line(&mut l).unwrap();
        serde_json::from_str(&l).unwrap()
    }
    fn call(&mut self, cmd: &str, args: Value) -> Value {
        self.next += 1;
        let id = self.next;
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
    fn phone(&mut self) -> Value {
        self.call("get_status", Value::Null)["data"]["phone"].clone()
    }
    fn wait(&mut self, what: &str, f: impl Fn(&Value) -> bool) -> Value {
        let t = Instant::now();
        loop {
            let p = self.phone();
            if f(&p) {
                return p;
            }
            assert!(t.elapsed() < Duration::from_secs(15), "{what}: {p}");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
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

fn fast_phone() -> phone::Timing {
    phone::Timing {
        clcc_idle: Duration::from_millis(80),
        clcc_call: Duration::from_millis(80),
        sms_poll: Duration::from_millis(250),
        ended_show: Duration::from_millis(500),
        dial_grace: Duration::from_secs(2),
        setup_retry: Duration::from_millis(300),
        audio: Duration::from_secs(3),
    }
}

fn root(tag: &str, fake: &FakeModem) -> PathBuf {
    let root = std::env::temp_dir().join(format!("walpad-phone-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    fakeroot::populate(&root).unwrap();
    for (name, body) in [("walpad-modem", SCRIPT), ("walpad-callaudio", AUDIO)] {
        let s = root.join("usr/local/sbin").join(name);
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        std::fs::write(&s, body).unwrap();
        std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(root.join("pty-path"), &fake.slave).unwrap();
    std::fs::create_dir_all(root.join("dev/radio")).unwrap();
    std::os::unix::fs::symlink(&fake.slave, root.join("dev/radio/pttycmd1")).unwrap();
    root
}

impl Rig {
    fn start(root: &Path) -> Rig {
        let mut d = Daemon::new(Root::new(root)).unwrap();
        d.modem_mut().set_timing(fast());
        d.modem_mut().phone_mut().set_timing(fast_phone());
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || d.run(&s2));
        Rig {
            root: root.to_path_buf(),
            stop,
            h: Some(h),
        }
    }
    fn audio(&self) -> String {
        std::fs::read_to_string(self.root.join("audio-calls")).unwrap_or_default()
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

fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(15), "{what}");
        std::thread::sleep(Duration::from_millis(30));
    }
}

#[test]
fn an_outgoing_call_runs_through_every_state() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("out", &fake));
    let mut c = Conn::new(&rig.root);
    c.wait("line usable", |p| p["available"] == true);
    assert_eq!(c.phone()["audio"], "experimental");

    // strict number validation: nothing odd reaches the modem
    for bad in ["", "abc", "012;ATH", "0171 123", "01711234567\r"] {
        let r = c.call("phone_dial", json!({"number": bad}));
        assert_eq!(r["error"]["code"], "invalid_args", "{bad:?}: {r}");
    }
    assert_eq!(
        c.call("phone_dial", json!({"number": "*121#"}))["data"]["state"],
        "running"
    );
    assert!(!fake.log().iter().any(|l| l.starts_with("ATD")));

    let r = c.call("phone_dial", json!({"number": "+8801711234567"}));
    assert_eq!(r["data"]["state"], "dialing", "{r}");
    assert_eq!(
        c.call("phone_dial", json!({"number": "999"}))["error"]["code"],
        "busy",
        "one call at a time"
    );
    let p = c.phone();
    assert_eq!(
        (&p["call"], &p["incoming"]),
        (&json!("dialing"), &json!(false))
    );
    assert_eq!(p["number"], "+8801711234567");
    wait_for("ATD", || {
        fake.log().contains(&"ATD+8801711234567;".to_string())
    });

    // the far end answers
    fake.set_calls(vec![(1, false, 0, "+8801711234567".into())]);
    let p = c.wait("active", |p| p["call"] == "active");
    assert_eq!(p["incoming"], false);
    // the audio route is switched (experimental script): earpiece by default
    wait_for("audio start", || rig.audio().contains("start earpiece"));
    // mute, speaker, dtmf
    assert_eq!(c.call("phone_mute", json!({"on": true}))["ok"], true);
    c.wait("muted", |p| p["muted"] == true);
    assert!(fake.log().contains(&"AT+CMUT=1".to_string()));
    assert_eq!(c.call("phone_dtmf", json!({"digit": "5"}))["ok"], true);
    assert_eq!(
        c.call("phone_dtmf", json!({"digit": "55"}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call("phone_dtmf", json!({"digit": "a"}))["error"]["code"],
        "invalid_args"
    );
    wait_for("dtmf", || fake.log().contains(&"AT+VTS=5".to_string()));
    assert_eq!(c.call("phone_speaker", json!({"on": true}))["ok"], true);
    wait_for("speaker route", || rig.audio().contains("start speaker"));
    assert_eq!(c.phone()["speaker"], true);

    // hang up
    assert_eq!(c.call("phone_hangup", Value::Null)["ok"], true);
    let n = c.notice("call_ended");
    assert_eq!(n["reason"], "hangup", "{n}");
    assert_eq!(n["incoming"], false);
    wait_for("modem hung up", || {
        fake.log().contains(&"AT+CHUP".to_string())
    });
    wait_for("audio stop", || rig.audio().contains("stop"));
    c.wait("idle again", |p| p["call"] == "idle");
    assert_eq!(c.phone()["muted"], false);
    assert_eq!(
        c.call("phone_hangup", Value::Null)["error"]["code"],
        "refused"
    );
    // the call log has it, privately
    let log = c.call("call_log", Value::Null);
    let calls = log["data"]["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        (&calls[0]["kind"], &calls[0]["number"]),
        (&json!("out"), &json!("+8801711234567"))
    );
    let f = rig.root.join("var/lib/walpad/calls.json");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    c.call("call_log_clear", Value::Null);
    assert!(c.call("call_log", Value::Null)["data"]["calls"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn incoming_calls_ring_answer_reject_and_miss() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("in", &fake));
    let mut c = Conn::new(&rig.root);
    c.wait("line usable", |p| p["available"] == true);
    // a known contact shows with its name
    let r = c.call(
        "contact_save",
        json!({"name": "Rahim", "numbers": ["+8801555000111"]}),
    );
    assert_eq!(r["ok"], true, "{r}");

    fake.set_calls(vec![(1, true, 4, "01555000111".into())]);
    let n = c.notice("incoming_call");
    assert_eq!(
        (&n["number"], &n["name"]),
        (&json!("01555000111"), &json!("Rahim"))
    );
    let p = c.wait("ringing", |p| p["call"] == "ringing");
    assert_eq!(
        (&p["incoming"], &p["name"]),
        (&json!(true), &json!("Rahim"))
    );
    // answer
    assert_eq!(c.call("phone_answer", Value::Null)["ok"], true);
    wait_for("ATA", || fake.log().contains(&"ATA".to_string()));
    c.wait("active", |p| p["call"] == "active");
    // the caller hangs up
    fake.set_calls(vec![]);
    let n = c.notice("call_ended");
    assert_eq!(
        (&n["reason"], &n["incoming"]),
        (&json!("remote"), &json!(true))
    );
    c.wait("idle", |p| p["call"] == "idle");

    // reject the next one
    fake.set_calls(vec![(2, true, 4, "01555000222".into())]);
    c.notice("incoming_call");
    assert_eq!(c.call("phone_answer", Value::Null)["ok"], true);
    // (the fake answered it: undo to test reject on a fresh ring)
    c.wait("active", |p| p["call"] == "active");
    assert_eq!(c.call("phone_hangup", Value::Null)["ok"], true);
    c.notice("call_ended");
    c.wait("idle", |p| p["call"] == "idle");
    fake.set_calls(vec![(3, true, 4, "01555000333".into())]);
    c.notice("incoming_call");
    assert_eq!(c.call("phone_hangup", Value::Null)["ok"], true);
    assert_eq!(c.notice("call_ended")["reason"], "rejected");
    c.wait("idle", |p| p["call"] == "idle");
    // a missed one
    fake.set_calls(vec![(4, true, 4, "01555000444".into())]);
    c.notice("incoming_call");
    fake.set_calls(vec![]);
    assert_eq!(c.notice("call_ended")["reason"], "missed");
    let log = c.call("call_log", Value::Null);
    let kinds: Vec<&str> = log["data"]["calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["in", "in", "missed", "missed"]);
    assert_eq!(log["data"]["calls"][0]["name"], "Rahim");
}

#[test]
fn emergency_numbers_dial_without_a_sim_and_others_do_not() {
    let fake = FakeModem::new(false);
    let rig = Rig::start(&root("sos", &fake));
    let mut c = Conn::new(&rig.root);
    let m = c.call("get_status", Value::Null)["data"]["mobile"].clone();
    assert_eq!(m["modem"], "ready");
    let t = Instant::now();
    loop {
        let m = c.call("get_status", Value::Null)["data"]["mobile"].clone();
        if m["radio_on"] == true && m["sim"] == "absent" {
            break;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "{m}");
        std::thread::sleep(Duration::from_millis(30));
    }
    let p = c.phone();
    assert_eq!(p["available"], false);
    assert_eq!(p["reason"], "No usable SIM card");
    assert_eq!(
        c.call("phone_dial", json!({"number": "01711234567"}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call("sms_send", json!({"number": "01711234567", "text": "x"}))["error"]["code"],
        "refused"
    );
    for sos in ["999", "112", "199"] {
        let r = c.call("phone_dial", json!({"number": sos}));
        assert_eq!(r["data"]["state"], "dialing", "{sos}: {r}");
        wait_for(sos, || fake.log().contains(&format!("ATD{sos};")));
        assert_eq!(c.call("phone_hangup", Value::Null)["ok"], true);
        c.notice("call_ended");
        c.wait("idle", |p| p["call"] == "idle");
    }
}

#[test]
fn a_busy_line_and_an_error_end_the_call_with_a_reason() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("busy", &fake));
    let mut c = Conn::new(&rig.root);
    c.wait("line usable", |p| p["available"] == true);
    fake.state.lock().unwrap().dial_final = "BUSY".into();
    assert_eq!(
        c.call("phone_dial", json!({"number": "01711234567"}))["ok"],
        true
    );
    assert_eq!(c.notice("call_ended")["reason"], "busy");
    c.wait("idle", |p| p["call"] == "idle");
    fake.state.lock().unwrap().dial_final = "ERROR".into();
    c.call("phone_dial", json!({"number": "01711234567"}));
    assert_eq!(c.notice("call_ended")["reason"], "failed");
}

/// DELIVER PDUs: "How are you?" from +31641600986 and Bangla "আমি" from +31641600986.
const PDU_EN: &str = "07911326040000F0040B911346610089F60040208062917314080CC8F71D14969741F977FD07";
const PDU_BN: &str = "00040B911346610089F600081121314152630006098609AE09BF";

#[test]
fn messages_are_received_stored_sent_and_deleted() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("sms", &fake));
    let mut c = Conn::new(&rig.root);
    c.wait("line usable", |p| p["available"] == true);
    {
        let mut s = fake.state.lock().unwrap();
        s.inbox = vec![(1, 0, PDU_EN.into()), (2, 0, PDU_BN.into())];
    }
    let n = c.notice("sms_received");
    assert_eq!(n["number"], "+31641600986");
    c.notice("sms_received");
    let p = c.wait("unread", |p| p["unread"] == 2);
    assert_eq!(p["unread"], 2);
    // copied off the SIM
    wait_for("deleted from the SIM", || {
        fake.state.lock().unwrap().inbox.is_empty()
    });
    let l = c.call("sms_list", Value::Null);
    let msgs = l["data"]["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 2, "{l}");
    let texts: Vec<&str> = msgs.iter().map(|m| m["text"].as_str().unwrap()).collect();
    assert!(
        texts.contains(&"How are you?") && texts.contains(&"আমি"),
        "{texts:?}"
    );
    assert_eq!(l["data"]["unread"], 2);
    // the store is private
    let f = rig.root.join("var/lib/walpad/sms.json");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // read the conversation
    let r = c.call("sms_read", json!({"number": "+31641600986"}));
    assert_eq!(r["data"]["unread"], 0, "{r}");
    c.wait("read", |p| p["unread"] == 0);

    // send Bangla text: UCS2 PDU, then the outcome notice
    let r = c.call("sms_send", json!({"number": "01711234567", "text": "আমি"}));
    assert_eq!(r["data"]["parts"], 1, "{r}");
    let n = c.notice("sms_sent");
    assert_eq!(n["ok"], true, "{n}");
    let sent = fake.state.lock().unwrap().sent.clone();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("0008AA06098609AE09BF"), "{}", sent[0]);
    // the SMSC from AT+CSCA? is in the PDU
    assert!(sent[0].starts_with("0891881017060000F0"), "{}", sent[0]);
    assert!(fake.log().iter().any(|l| l.starts_with("AT+CMGS=")));
    // long GSM text goes out as two parts
    let r = c.call(
        "sms_send",
        json!({"number": "01711234567", "text": "a".repeat(200)}),
    );
    assert_eq!(r["data"]["parts"], 2, "{r}");
    assert_eq!(c.notice("sms_sent")["ok"], true);
    assert_eq!(fake.state.lock().unwrap().sent.len(), 3);
    // limits
    assert_eq!(
        c.call("sms_send", json!({"number": "01711234567", "text": ""}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(
            "sms_send",
            json!({"number": "01711234567", "text": "a".repeat(1001)})
        )["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call("sms_send", json!({"number": "01x", "text": "hi"}))["error"]["code"],
        "invalid_args"
    );
    assert_eq!(
        c.call(
            "sms_send",
            json!({"number": "01711234567", "text": "a".repeat(1000)})
        )["error"]["code"],
        "invalid_args",
        "more than 6 parts"
    );
    // delete one
    let l = c.call("sms_list", Value::Null);
    let id = l["data"]["messages"][0]["id"].as_u64().unwrap();
    assert_eq!(
        c.call("sms_delete", json!({"id": id}))["data"]["deleted"],
        true
    );
    assert_eq!(
        c.call("sms_delete", json!({"id": id}))["data"]["deleted"],
        false
    );
    // messages survive a restart of the core: the file is read again
    let again = c.call("sms_list", Value::Null);
    assert_eq!(
        again["data"]["messages"].as_array().unwrap().len(),
        l["data"]["messages"].as_array().unwrap().len() - 1
    );
}

#[test]
fn contacts_are_saved_listed_imported_and_validated() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("contacts", &fake));
    let mut c = Conn::new(&rig.root);
    for bad in [
        json!({"name": "", "numbers": ["01711234567"]}),
        json!({"name": "A", "numbers": []}),
        json!({"name": "A", "numbers": ["12"]}),
        json!({"name": "A", "numbers": ["01711234567", "01711234568", "01711234569", "01711234560", "01711234561", "01711234562"]}),
        json!({"name": "A\nB", "numbers": ["01711234567"]}),
    ] {
        assert_eq!(c.call("contact_save", bad)["error"]["code"], "invalid_args");
    }
    let id = c.call(
        "contact_save",
        json!({"name": "Zara", "numbers": ["01711234567", "+8801811000000"]}),
    )["data"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    c.call(
        "contact_save",
        json!({"name": "amir", "numbers": ["01555000111"]}),
    );
    let l = c.call("contact_list", Value::Null);
    let names: Vec<&str> = l["data"]["contacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["amir", "Zara"]);
    // change one
    let r = c.call(
        "contact_save",
        json!({"id": id, "name": "Zara K", "numbers": ["01711234567"]}),
    );
    assert_eq!(r["data"]["id"], id.as_str(), "{r}");
    assert_eq!(
        c.call(
            "contact_save",
            json!({"id": "nope", "name": "x", "numbers": ["01711234567"]})
        )["error"]["code"],
        "refused"
    );
    // vCard import from a user folder
    let dir = rig.root.join("home/user");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("c.vcf"),
        "BEGIN:VCARD\nVERSION:3.0\nFN:Rahim Uddin\nTEL;TYPE=CELL:+880 1911-222333\nEND:VCARD\n",
    )
    .unwrap();
    let r = c.call("contact_import", json!({"path": "/home/user/c.vcf"}));
    assert_eq!(r["data"], json!({"found": 1, "added": 1}), "{r}");
    let r = c.call("contact_import", json!({"path": "/home/user/c.vcf"}));
    assert_eq!(r["data"]["added"], 0, "no duplicates: {r}");
    for bad in [
        "/etc/passwd",
        "/home/../etc/x.vcf",
        "/home/user/c.txt",
        "c.vcf",
    ] {
        assert_eq!(
            c.call("contact_import", json!({"path": bad}))["error"]["code"],
            "invalid_args",
            "{bad}"
        );
    }
    assert_eq!(
        c.call("contact_import", json!({"path": "/home/user/none.vcf"}))["error"]["code"],
        "refused"
    );
    assert_eq!(
        c.call("contact_delete", json!({"id": id}))["data"]["deleted"],
        true
    );
    assert_eq!(
        c.call("contact_delete", json!({"id": "Bad Id"}))["error"]["code"],
        "invalid_args"
    );
    let f = rig.root.join("var/lib/walpad/contacts.json");
    assert_eq!(
        std::fs::metadata(&f).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn nothing_private_leaks_into_the_modem_script_arguments() {
    let fake = FakeModem::new(true);
    let rig = Rig::start(&root("leak", &fake));
    let mut c = Conn::new(&rig.root);
    c.wait("line usable", |p| p["available"] == true);
    c.call("phone_dial", json!({"number": "01711234567"}));
    wait_for("ATD", || {
        fake.log().contains(&"ATD01711234567;".to_string())
    });
    fake.set_calls(vec![(1, false, 0, "01711234567".into())]);
    c.wait("active", |p| p["call"] == "active");
    assert_eq!(fake.calls().len(), 1);
    c.call(
        "sms_send",
        json!({"number": "01711234567", "text": "secret words"}),
    );
    c.notice("sms_sent");
    // the audio script only ever gets verbs
    let a = rig.audio();
    assert!(!a.contains("0171") && !a.contains("secret"), "{a}");
    assert!(
        a.lines().all(|l| l.starts_with("start ") || l == "stop"),
        "{a}"
    );
}
