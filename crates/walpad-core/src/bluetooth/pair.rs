//! Interactive Bluetooth pairing session with `bluetoothctl` (`KeyboardDisplay` agent).

use super::parse::{PairEvent, PairParser};
use super::types::{kill_group, Job, Limits, LEGACY_PIN};
use super::worker::Worker;
use serde_json::json;
use std::io::{Read, Write};
use std::process::Stdio;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

impl Worker {
    /// Interactive session with an agent. Returns when paired (and the follow-up trust/connect
    /// settled), failed, or out of time.
    pub(super) fn pair(&mut self, addr: &str) {
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
}
