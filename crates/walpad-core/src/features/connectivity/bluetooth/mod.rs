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

mod commands;
mod pair;
pub mod parse;
mod types;
mod worker;

pub use types::{
    kill_group, BluetoothError, Cfg, Job, Limits, Msg, Snapshot, CTL, ENABLED_FILE, SCAN_SECS,
    SCRIPT, STATE_FILE,
};

use serde_json::Value;
use std::collections::VecDeque;
use std::process::Child;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{BluetoothController, BluetoothStatus};
use worker::Worker;

pub(crate) struct Scan {
    pub(crate) child: Child,
    pub(crate) deadline: Instant,
}

pub struct Bluetooth {
    pub(crate) root: Root,
    pub(crate) cfg: Cfg,
    pub(crate) tx: Option<Sender<Job>>,
    pub(crate) rx: Option<Receiver<Msg>>,
    pub(crate) snap: Option<Snapshot>,
    pub(crate) inflight: usize,
    pub(crate) enabling: bool,
    pub(crate) enabled: bool,
    pub(crate) start_error: Option<String>,
    pub(crate) pairing: Option<String>,
    pub(crate) awaiting: Option<String>,
    pub(crate) scan: Option<Scan>,
    pub(crate) scan_limit: Duration,
    pub(crate) next_refresh: Instant,
    pub(crate) auto_power_done: bool,
    pub(crate) notices: Vec<Value>,
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

    pub(crate) fn send(&mut self, job: Job) {
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

    pub(crate) fn ready(&self, need_power: bool) -> Result<(), BluetoothError> {
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
