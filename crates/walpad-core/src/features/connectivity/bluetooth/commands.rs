//! Bluetooth command entry points: enable, scan, pair, connect, disconnect, forget, confirm.

use super::{kill_group, Bluetooth, BluetoothError, Job, Scan, ENABLED_FILE};
use serde_json::{json, Value};
use std::process::Stdio;
use std::time::{Duration, Instant};
use walpad_proto::valid_bt_address;

impl Bluetooth {
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

    pub(crate) fn stop_scan(&mut self, tell_bluez: bool) {
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
}
