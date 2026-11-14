//! Shared types, constants, timeouts, and bounded subprocess execution for Bluetooth.

use super::parse::ControllerInfo;
use serde_json::Value;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;
use walpad_proto::BluetoothDevice;

pub const CTL: &str = "/usr/bin/bluetoothctl";
pub const SCRIPT: &str = "/usr/local/sbin/walpad-bluetooth";
pub const ENABLED_FILE: &str = "/var/lib/walpad/bluetooth-enabled";
pub const STATE_FILE: &str = "/run/walpad/bt-state";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Discovery stops by itself after this long.
pub const SCAN_SECS: u64 = 30;
pub(crate) const MAX_DEVICES: usize = 24;
/// Pairing PIN answered to legacy "Enter PIN code" requests (headsets, old keyboards).
pub(crate) const LEGACY_PIN: &str = "0000";

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
    pub(crate) fn new() -> Self {
        Limits {
            cmd_ms: AtomicU64::new(8_000),
            connect_ms: AtomicU64::new(25_000),
            pair_ms: AtomicU64::new(60_000),
            start_ms: AtomicU64::new(120_000),
            stop_ms: AtomicU64::new(30_000),
        }
    }

    pub(crate) fn get(a: &AtomicU64) -> Duration {
        Duration::from_millis(a.load(Ordering::Relaxed))
    }
}

#[derive(Debug)]
pub enum Job {
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

pub struct Snapshot {
    pub(crate) info: Option<ControllerInfo>,
    pub(crate) devices: Vec<BluetoothDevice>,
}

pub enum Msg {
    Snapshot(Snapshot),
    Notice(Value),
    /// A job finished (after its refresh).
    Done,
}

#[derive(Clone)]
pub struct Cfg {
    pub(crate) ctl: PathBuf,
    pub(crate) script: PathBuf,
    /// Test root, passed to the children as WALPAD_BT_ROOT / WALPAD_ROOT.
    pub(crate) test_root: Option<PathBuf>,
    pub(crate) limits: Arc<Limits>,
}

impl Cfg {
    pub(crate) fn command(&self, prog: &PathBuf) -> Command {
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

pub fn kill_group(child: &mut Child) {
    if let Some(pid) = rustix::process::Pid::from_raw(child.id() as i32) {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Run to completion with a timeout. (exit success, stdout).
pub(crate) fn run_capture(mut cmd: Command, timeout: Duration) -> Result<(bool, String), String> {
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
