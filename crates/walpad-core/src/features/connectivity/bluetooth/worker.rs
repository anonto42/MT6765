//! Background Bluetooth worker thread: controller refresh, one-shot actions, and job loop.

use super::parse;
use super::types::{run_capture, Cfg, Job, Limits, Msg, Snapshot, MAX_DEVICES};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;
use walpad_proto::BluetoothDevice;

pub(crate) struct Worker {
    pub(crate) cfg: Cfg,
    pub(crate) rx: Receiver<Job>,
    pub(crate) tx: Sender<Msg>,
    pub(crate) pending: VecDeque<Job>,
}

impl Worker {
    pub(super) fn ctl(&self, args: &[&str], timeout: Duration) -> Result<(bool, String), String> {
        let mut c = self.cfg.command(&self.cfg.ctl);
        c.args(args);
        run_capture(c, timeout)
    }

    pub(super) fn cmd_timeout(&self) -> Duration {
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

    pub(super) fn pair_notice(&self, addr: &str, state: &str, extra: Value) {
        let mut v = json!({"kind":"bluetooth_pair","address":addr,"state":state});
        if let (Some(m), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        self.notice(v);
    }

    pub(crate) fn main(mut self) {
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
