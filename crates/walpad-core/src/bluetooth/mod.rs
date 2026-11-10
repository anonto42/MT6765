//! Bluetooth (owner request 2026-11-10). BlueZ's `bluetoothd` does the work; the core talks to it
//! through `/usr/bin/bluetoothctl` run as a short-lived child (fixed path, empty environment +
//! PATH, own process group, every call has a timeout, arguments only ever a validated address or a
//! fixed word - never a shell string). `/usr/local/sbin/walpad-bluetooth start|stop` brings the
//! controller chain up and down.
//!
//! All blocking work runs on one worker thread fed by a channel; the daemon thread only queues
//! jobs and drains results in `tick`. Discovery is a separate child (`bluetoothctl --timeout 30
//! scan on`) that the daemon owns, because BlueZ stops discovery when its client goes away.
//! Pairing runs `bluetoothctl` interactively with an agent (`KeyboardDisplay`); passkey
//! confirmations travel as `notice` events and the `bluetooth_confirm` command.
pub mod parse;

use parse::{ControllerInfo, PairEvent, PairParser};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{valid_bt_address, BluetoothController, BluetoothDevice, BluetoothStatus};

pub const CTL: &str = "/usr/bin/bluetoothctl";
pub const SCRIPT: &str = "/usr/local/sbin/walpad-bluetooth";
pub const ENABLED_FILE: &str = "/var/lib/walpad/bluetooth-enabled";
const STATE_FILE: &str = "/run/walpad/bt-state";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Discovery stops by itself after this long.
pub const SCAN_SECS: u64 = 30;
const MAX_DEVICES: usize = 24;
/// Pairing PIN answered to legacy "Enter PIN code" requests (headsets, old keyboards).
const LEGACY_PIN: &str = "0000";

#[derive(Debug, PartialEq, Eq)]
pub enum BluetoothError {
    Busy(String),
    Refused(String),
    Failed(String),
}

/// Timeouts in milliseconds (shared with the worker so tests can shorten them).
pub struct Limits {
    pub cmd_ms: AtomicU64,
    pub connect_ms: AtomicU64,
    pub pair_ms: AtomicU64,
    pub start_ms: AtomicU64,
    pub stop_ms: AtomicU64,
}

impl Limits {
    fn new() -> Self {
        Limits {
            cmd_ms: AtomicU64::new(8_000),
            connect_ms: AtomicU64::new(25_000),
            pair_ms: AtomicU64::new(60_000),
            start_ms: AtomicU64::new(120_000),
            stop_ms: AtomicU64::new(30_000),
        }
    }
    fn get(a: &AtomicU64) -> Duration {
        Duration::from_millis(a.load(Ordering::Relaxed))
    }
}

#[derive(Debug)]
enum Job {
    Refresh,
    Enable(bool),
    PowerOn,
    ScanOff,
    Pair(String),
    Connect(String),
    Disconnect(String),
    Forget(String),
    Confirm(String, bool),
}

struct Snapshot {
    info: Option<ControllerInfo>,
    devices: Vec<BluetoothDevice>,
}

enum Msg {
    Snapshot(Snapshot),
    Notice(Value),
    /// A job finished (after its refresh).
    Done,
}

#[derive(Clone)]
struct Cfg {
    ctl: PathBuf,
    script: PathBuf,
    /// Test root, passed to the children as WALPAD_BT_ROOT / WALPAD_ROOT.
    test_root: Option<PathBuf>,
    limits: Arc<Limits>,
}

impl Cfg {
    fn command(&self, prog: &PathBuf) -> Command {
        let mut c = Command::new(prog);
        c.env_clear().env("PATH", PATH).stdin(Stdio::null());
        if let Some(r) = &self.test_root {
            c.env("WALPAD_BT_ROOT", r).env("WALPAD_ROOT", r);
        }
        {
            use std::os::unix::process::CommandExt;
            c.process_group(0);
        }
        c
    }
}

fn kill_group(child: &mut Child) {
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run to completion with a timeout. (exit success, stdout).
fn run_capture(mut cmd: Command, timeout: Duration) -> Result<(bool, String), String> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "not installed".to_string(),
            _ => format!("cannot run: {e}"),
        })?;
    let mut out = child.stdout.take().ok_or("no stdout")?;
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut out).take(256 * 1024).read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    match rx.recv_timeout(timeout) {
        Ok(buf) => {
            let ok = child.wait().map(|s| s.success()).unwrap_or(false);
            Ok((ok, String::from_utf8_lossy(&buf).into_owned()))
        }
        Err(_) => {
            kill_group(&mut child);
            Err("timed out".into())
        }
    }
}

struct Worker {
    cfg: Cfg,
    rx: Receiver<Job>,
    tx: Sender<Msg>,
    pending: VecDeque<Job>,
}

impl Worker {
    fn ctl(&self, args: &[&str], timeout: Duration) -> Result<(bool, String), String> {
        let mut c = self.cfg.command(&self.cfg.ctl);
        c.args(args);
        run_capture(c, timeout)
    }

    fn cmd_timeout(&self) -> Duration {
        Limits::get(&self.cfg.limits.cmd_ms)
    }

    fn notice(&self, v: Value) {
        let _ = self.tx.send(Msg::Notice(v));
    }

    fn refresh(&self) {
        let t = self.cmd_timeout();
        let info = self
            .ctl(&["show"], t)
            .ok()
            .and_then(|(_, o)| parse::parse_show(&o));
        let mut devices = Vec::new();
        if info.is_some() {
            if let Ok((_, o)) = self.ctl(&["devices"], t) {
                for (addr, name) in parse::parse_devices(&o).into_iter().take(MAX_DEVICES) {
                    match self
                        .ctl(&["info", &addr], t)
                        .map(|(_, o)| parse::parse_info(&o))
                    {
                        Ok(Some(d)) => devices.push(d),
                        // listed but no details: still show it
                        _ => devices.push(BluetoothDevice {
                            name: if name.is_empty() { addr.clone() } else { name },
                            address: addr,
                            ..Default::default()
                        }),
                    }
                }
            }
        }
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(b.paired.cmp(&a.paired))
                .then(b.rssi.unwrap_or(-200).cmp(&a.rssi.unwrap_or(-200)))
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        let _ = self.tx.send(Msg::Snapshot(Snapshot { info, devices }));
    }

    /// One-shot action: ok / reason.
    fn action(&self, args: &[&str], timeout: Duration) -> Result<(), String> {
        let (ok, out) = self.ctl(args, timeout)?;
        match parse::failure(&out) {
            Some(r) => Err(r),
            None if ok => Ok(()),
            None => Err("bluetoothctl failed".into()),
        }
    }

    fn run_script(&self, verb: &str, timeout: Duration) -> Result<(), String> {
        let mut c = self.cfg.command(&self.cfg.script);
        c.arg(verb);
        let (ok, out) = run_capture(c, timeout).map_err(|e| {
            if e == "not installed" {
                "walpad-bluetooth is not installed".to_string()
            } else {
                e
            }
        })?;
        if ok {
            return Ok(());
        }
        let why = out
            .lines()
            .rev()
            .find_map(|l| l.split_once("FAILED:").map(|(_, r)| r.trim().to_string()))
            .map(|r| parse::clean_text(&r, 100))
            .unwrap_or_else(|| "walpad-bluetooth failed".into());
        Err(why)
    }

    fn run(&mut self, job: Job) {
        let t = self.cmd_timeout();
        let connect_t = Limits::get(&self.cfg.limits.connect_ms);
        match job {
            Job::Refresh => {}
            Job::Enable(true) => {
                let r = self
                    .run_script("start", Limits::get(&self.cfg.limits.start_ms))
                    .and_then(|_| self.action(&["power", "on"], t));
                self.op_notice("enable", "", r);
            }
            Job::Enable(false) => {
                let _ = self.action(&["power", "off"], t);
                let r = self.run_script("stop", Limits::get(&self.cfg.limits.stop_ms));
                self.op_notice("enable", "", r);
            }
            Job::PowerOn => {
                let r = self.action(&["power", "on"], t);
                if r.is_err() {
                    self.op_notice("power", "", r);
                }
            }
            Job::ScanOff => {
                let _ = self.ctl(&["scan", "off"], t);
            }
            Job::Connect(a) => {
                let r = self.action(&["connect", &a], connect_t);
                self.op_notice("connect", &a, r);
            }
            Job::Disconnect(a) => {
                let r = self.action(&["disconnect", &a], t);
                self.op_notice("disconnect", &a, r);
            }
            Job::Forget(a) => {
                let _ = self.action(&["disconnect", &a], t);
                let r = self.action(&["remove", &a], t);
                self.op_notice("forget", &a, r);
            }
            Job::Pair(a) => self.pair(&a),
            Job::Confirm(..) => {} // nothing is waiting
        }
        self.refresh();
        let _ = self.tx.send(Msg::Done);
    }

    fn op_notice(&self, op: &str, address: &str, r: Result<(), String>) {
        let (ok, error) = match r {
            Ok(()) => (true, Value::Null),
            Err(e) => (false, json!(e)),
        };
        self.notice(json!({"kind":"bluetooth","op":op,"address":address,"ok":ok,"error":error}));
    }

    fn pair_notice(&self, addr: &str, state: &str, extra: Value) {
        let mut v = json!({"kind":"bluetooth_pair","address":addr,"state":state});
        if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        self.notice(v);
    }

    /// Interactive session with an agent. Returns when paired (and the follow-up trust/connect
    /// settled), failed, or out of time.
    fn pair(&mut self, addr: &str) {
        let mut cmd = self.cfg.command(&self.cfg.ctl);
        let spawned = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                self.pair_notice(
                    addr,
                    "failed",
                    json!({"error": format!("cannot run bluetoothctl: {e}")}),
                );
                return;
            }
        };
        let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            kill_group(&mut child);
            self.pair_notice(
                addr,
                "failed",
                json!({"error": "bluetoothctl has no pipes"}),
            );
            return;
        };
        let (otx, orx) = channel::<Vec<u8>>();
        std::thread::spawn(move || {
            let mut b = [0u8; 512];
            while let Ok(n) = stdout.read(&mut b) {
                if n == 0 || otx.send(b[..n].to_vec()).is_err() {
                    break;
                }
            }
        });
        let mut send = |s: &str| {
            let _ = stdin.write_all(s.as_bytes());
            let _ = stdin.flush();
        };
        send(&format!(
            "agent KeyboardDisplay\ndefault-agent\npair {addr}\n"
        ));
        self.pair_notice(addr, "started", json!({}));
        let deadline = Instant::now() + Limits::get(&self.cfg.limits.pair_ms);
        let mut parser = PairParser::default();
        let mut paired = false;
        let mut confirm_deadline: Option<Instant> = None;
        let mut result: Option<Result<(), String>> = None;
        let mut closed = false;
        while Instant::now() < deadline && !closed {
            // jobs that arrive meanwhile: answers are handled, the rest waits
            while let Ok(j) = self.rx.try_recv() {
                match j {
                    Job::Confirm(a, accept) if a == addr => {
                        send(if accept { "yes\n" } else { "no\n" })
                    }
                    other => self.pending.push_back(other),
                }
            }
            if confirm_deadline.is_some_and(|d| Instant::now() >= d) {
                break;
            }
            let chunk = match orx.recv_timeout(Duration::from_millis(100)) {
                Ok(c) => c,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    closed = true;
                    Vec::new()
                }
            };
            for ev in parser.feed(&String::from_utf8_lossy(&chunk)) {
                match ev {
                    PairEvent::Confirm(p) => {
                        self.pair_notice(addr, "confirm", json!({"passkey": p}))
                    }
                    PairEvent::Display(p) => {
                        self.pair_notice(addr, "passkey", json!({"passkey": p}))
                    }
                    PairEvent::PinRequest => send(&format!("{LEGACY_PIN}\n")),
                    PairEvent::PasskeyRequest => {
                        send("quit\n");
                        result = Some(Err("This device asks for a passkey that must be typed here (not supported)".into()));
                        closed = true;
                    }
                    PairEvent::Authorize => send("yes\n"),
                    PairEvent::Success => {
                        paired = true;
                        result = Some(Ok(()));
                        self.pair_notice(addr, "ok", json!({}));
                        send(&format!("trust {addr}\nconnect {addr}\n"));
                        confirm_deadline = Some(Instant::now() + Duration::from_secs(15));
                    }
                    PairEvent::Failed(e) => {
                        if !paired {
                            result = Some(Err(e));
                        }
                        closed = true;
                    }
                    PairEvent::Connected | PairEvent::ConnectFailed(_) => {
                        if paired {
                            closed = true;
                        }
                    }
                }
            }
        }
        send("quit\n");
        drop(stdin);
        let end = Instant::now() + Duration::from_secs(2);
        while Instant::now() < end && child.try_wait().map(|s| s.is_none()).unwrap_or(false) {
            std::thread::sleep(Duration::from_millis(50));
        }
        kill_group(&mut child);
        match result {
            Some(Ok(())) => {}
            Some(Err(e)) => self.pair_notice(addr, "failed", json!({"error": e})),
            None => {
                // time ran out while the device did not answer
                self.pair_notice(addr, "failed", json!({"error": "Pairing timed out"}));
                let _ = self.ctl(&["cancel-pairing", addr], self.cmd_timeout());
            }
        }
    }

    fn main(mut self) {
        loop {
            let job = match self.pending.pop_front() {
                Some(j) => j,
                None => match self.rx.recv() {
                    Ok(j) => j,
                    Err(_) => return,
                },
            };
            self.run(job);
        }
    }
}

struct Scan {
    child: Child,
    deadline: Instant,
}

pub struct Bluetooth {
    root: Root,
    cfg: Cfg,
    tx: Option<Sender<Job>>,
    rx: Option<Receiver<Msg>>,
    snap: Option<Snapshot>,
    inflight: usize,
    enabling: bool,
    enabled: bool,
    start_error: Option<String>,
    pairing: Option<String>,
    awaiting: Option<String>,
    scan: Option<Scan>,
    scan_limit: Duration,
    next_refresh: Instant,
    auto_power_done: bool,
    notices: Vec<Value>,
}

impl Bluetooth {
    pub fn new(root: &Root) -> Self {
        let test_root =
            (root.base() != std::path::Path::new("/")).then(|| root.base().to_path_buf());
        let enabled = walpad_hw::sysfs::read_trim(root, ENABLED_FILE).as_deref() != Some("0");
        Bluetooth {
            root: root.clone(),
            cfg: Cfg {
                ctl: root.p(CTL),
                script: root.p(SCRIPT),
                test_root,
                limits: Arc::new(Limits::new()),
            },
            tx: None,
            rx: None,
            snap: None,
            inflight: 0,
            enabling: false,
            enabled,
            start_error: None,
            pairing: None,
            awaiting: None,
            scan: None,
            scan_limit: Duration::from_secs(SCAN_SECS),
            next_refresh: Instant::now(),
            auto_power_done: false,
            notices: Vec::new(),
        }
    }

    /// Test hooks: shorter limits.
    pub fn limits(&self) -> &Limits {
        &self.cfg.limits
    }
    pub fn set_scan_limit(&mut self, d: Duration) {
        self.scan_limit = d;
    }

    pub fn supported(&self) -> bool {
        self.cfg.ctl.exists() || self.cfg.script.exists()
    }

    fn send(&mut self, job: Job) {
        if self.tx.is_none() {
            let (jtx, jrx) = channel();
            let (mtx, mrx) = channel();
            let w = Worker {
                cfg: self.cfg.clone(),
                rx: jrx,
                tx: mtx,
                pending: VecDeque::new(),
            };
            std::thread::spawn(move || w.main());
            self.tx = Some(jtx);
            self.rx = Some(mrx);
        }
        self.inflight += 1;
        if let Some(tx) = &self.tx {
            if tx.send(job).is_err() {
                self.inflight -= 1;
            }
        }
    }

    pub fn status(&self) -> BluetoothStatus {
        let mut st = BluetoothStatus {
            enabled: self.enabled,
            pairing: self.pairing.clone(),
            ..Default::default()
        };
        match self
            .snap
            .as_ref()
            .and_then(|s| s.info.as_ref().map(|i| (s, i)))
        {
            Some((s, i)) => {
                st.controller = BluetoothController::Present;
                st.powered = i.powered;
                st.discovering = i.discovering;
                st.devices = s.devices.clone();
            }
            None => {
                let state = walpad_hw::sysfs::read_trim(&self.root, STATE_FILE);
                if self.enabling || state.as_deref() == Some("starting") {
                    st.controller = BluetoothController::Starting;
                } else {
                    st.reason = if !self.supported() {
                        Some("Bluetooth is not installed on this system".into())
                    } else if !self.enabled {
                        None
                    } else if let Some(e) = &self.start_error {
                        Some(e.clone())
                    } else if state.as_deref() == Some("failed") {
                        Some("Bluetooth failed to start (see /var/log/walpad-bluetooth.log)".into())
                    } else {
                        None
                    };
                }
            }
        }
        if self.scan.is_some() && st.controller == BluetoothController::Present {
            st.discovering = true;
        }
        st
    }

    pub fn take_notices(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.notices)
    }

    fn ready(&self, need_power: bool) -> Result<(), BluetoothError> {
        let st = self.status();
        match st.controller {
            BluetoothController::Present if !need_power || st.powered => Ok(()),
            BluetoothController::Present => {
                Err(BluetoothError::Refused("Bluetooth is turned off".into()))
            }
            BluetoothController::Starting => {
                Err(BluetoothError::Busy("Bluetooth is starting".into()))
            }
            BluetoothController::Absent => Err(BluetoothError::Refused(
                st.reason
                    .unwrap_or_else(|| "No Bluetooth controller".into()),
            )),
        }
    }

    /// `bluetooth_enable`: persist the switch, then start/stop in the background.
    pub fn set_enabled(&mut self, on: bool) -> Result<Value, BluetoothError> {
        if !self.supported() {
            return Err(BluetoothError::Refused(
                "Bluetooth is not installed on this system".into(),
            ));
        }
        if self.enabling {
            return Err(BluetoothError::Busy(
                "Bluetooth is starting or stopping".into(),
            ));
        }
        let p = self.root.p(ENABLED_FILE);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| BluetoothError::Failed(e.to_string()))?;
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, if on { b"1\n" } else { b"0\n" })
            .and_then(|_| std::fs::rename(&tmp, &p))
            .map_err(|e| BluetoothError::Failed(e.to_string()))?;
        self.enabled = on;
        self.start_error = None;
        if !on {
            self.stop_scan(false);
        }
        self.enabling = true;
        self.auto_power_done = true;
        self.send(Job::Enable(on));
        Ok(json!({"enabled": on}))
    }

    /// `bluetooth_scan`: discovery for at most SCAN_SECS.
    pub fn set_scan(&mut self, on: bool) -> Result<Value, BluetoothError> {
        if !on {
            self.stop_scan(true);
            return Ok(json!({"scanning": false}));
        }
        self.ready(true)?;
        self.start_scan()?;
        Ok(json!({"scanning": true, "seconds": self.scan_limit.as_secs().max(1)}))
    }

    fn start_scan(&mut self) -> Result<(), BluetoothError> {
        if let Some(s) = self.scan.take() {
            let mut c = s.child;
            kill_group(&mut c);
        }
        let mut c = self.cfg.command(&self.cfg.ctl);
        let secs = self.scan_limit.as_secs().max(1).to_string();
        c.args(["--timeout", &secs, "scan", "on"])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = c
            .spawn()
            .map_err(|e| BluetoothError::Failed(format!("cannot run bluetoothctl: {e}")))?;
        self.scan = Some(Scan {
            child,
            deadline: Instant::now() + self.scan_limit + Duration::from_secs(3),
        });
        self.next_refresh = Instant::now() + Duration::from_secs(1);
        Ok(())
    }

    fn stop_scan(&mut self, tell_bluez: bool) {
        if let Some(s) = self.scan.take() {
            let mut c = s.child;
            kill_group(&mut c);
            if tell_bluez {
                self.send(Job::ScanOff);
            }
        }
    }

    pub fn pair(&mut self, address: &str) -> Result<Value, BluetoothError> {
        let a = Self::addr(address)?;
        self.ready(true)?;
        if self.pairing.is_some() {
            return Err(BluetoothError::Busy("another pairing is running".into()));
        }
        self.pairing = Some(a.clone());
        self.awaiting = None;
        self.send(Job::Pair(a.clone()));
        Ok(json!({"address": a, "pairing": true}))
    }

    pub fn connect(&mut self, address: &str) -> Result<Value, BluetoothError> {
        let a = Self::addr(address)?;
        self.ready(true)?;
        self.send(Job::Connect(a.clone()));
        Ok(json!({"address": a}))
    }

    pub fn disconnect(&mut self, address: &str) -> Result<Value, BluetoothError> {
        let a = Self::addr(address)?;
        self.ready(false)?;
        self.send(Job::Disconnect(a.clone()));
        Ok(json!({"address": a}))
    }

    pub fn forget(&mut self, address: &str) -> Result<Value, BluetoothError> {
        let a = Self::addr(address)?;
        self.ready(false)?;
        self.send(Job::Forget(a.clone()));
        Ok(json!({"address": a}))
    }

    pub fn confirm(&mut self, address: &str, accept: bool) -> Result<Value, BluetoothError> {
        let a = Self::addr(address)?;
        if self.awaiting.as_deref() != Some(a.as_str()) {
            return Err(BluetoothError::Refused(
                "no pairing is waiting for a confirmation".into(),
            ));
        }
        self.awaiting = None;
        // answers are not counted as jobs: the worker is inside the pairing job
        if let Some(tx) = &self.tx {
            let _ = tx.send(Job::Confirm(a.clone(), accept));
        }
        Ok(json!({"address": a, "accepted": accept}))
    }

    fn addr(a: &str) -> Result<String, BluetoothError> {
        if valid_bt_address(a) {
            Ok(a.to_ascii_uppercase())
        } else {
            Err(BluetoothError::Refused("bad Bluetooth address".into()))
        }
    }

    /// Daemon loop: drain the worker, supervise the scan child, schedule refreshes. True when the
    /// status (or notices) changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let mut changed = false;
        let mut msgs = Vec::new();
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                msgs.push(m);
            }
        }
        for m in msgs {
            changed = true;
            match m {
                Msg::Snapshot(s) => {
                    if s.info.is_some() {
                        self.start_error = None;
                    }
                    self.snap = Some(s);
                }
                Msg::Notice(n) => {
                    self.on_notice(&n);
                    self.notices.push(n);
                }
                Msg::Done => {
                    self.inflight = self.inflight.saturating_sub(1);
                    // a Confirm answer is not a job, Pair/Enable are
                }
            }
        }
        let scan_over = self.scan.as_mut().is_some_and(|s| {
            s.child.try_wait().map(|e| e.is_some()).unwrap_or(true) || now >= s.deadline
        });
        if scan_over {
            self.stop_scan(true);
            changed = true;
        }
        if !self.supported() {
            return changed;
        }
        let present = self.snap.as_ref().is_some_and(|s| s.info.is_some());
        // switch on and the controller is there but off (BlueZ may not auto-power): once per run
        if present && self.enabled && !self.auto_power_done {
            self.auto_power_done = true;
            if self
                .snap
                .as_ref()
                .and_then(|s| s.info.as_ref())
                .is_some_and(|i| !i.powered)
            {
                self.send(Job::PowerOn);
            }
        }
        if self.inflight == 0 && now >= self.next_refresh {
            let every = if self.scan.is_some() {
                Duration::from_secs(3)
            } else if present {
                Duration::from_secs(10)
            } else {
                Duration::from_secs(5)
            };
            self.next_refresh = now + every;
            self.send(Job::Refresh);
        }
        changed
    }

    fn on_notice(&mut self, n: &Value) {
        match n["kind"].as_str() {
            Some("bluetooth") if n["op"] == "enable" => {
                self.enabling = false;
                self.start_error = n["error"].as_str().map(str::to_string);
            }
            Some("bluetooth_pair") => {
                let addr = n["address"].as_str().map(str::to_string);
                match n["state"].as_str() {
                    Some("confirm") => self.awaiting = addr,
                    Some("ok" | "failed") => {
                        self.awaiting = None;
                        self.pairing = None;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

impl Drop for Bluetooth {
    fn drop(&mut self) {
        if let Some(s) = self.scan.take() {
            let mut c = s.child;
            kill_group(&mut c);
        }
    }
}
