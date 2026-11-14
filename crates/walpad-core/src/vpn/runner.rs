//! Script runner (`walpad-vpn` up|down|status|apply|confirm), output parsing, and polling.

use super::{Run, Verb, Vpn, STATUS_EVERY};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;
use walpad_proto::{VpnKind, VpnState};

const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// One `key=value` line of the script's output.
pub(crate) fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// Short, printable reason text from the script (never trusted to be tidy).
pub(crate) fn clean_reason(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_string()
}

impl Vpn {
    pub(crate) fn spawn(&mut self, verb: Verb, now: Instant, cleanup: bool) -> Result<(), String> {
        if let Some(r) = self.run.take() {
            Self::kill(r);
        }
        let out = std::fs::File::create(&self.out).map_err(|e| format!("output file: {e}"))?;
        let mut cmd = Command::new(&self.script);
        cmd.arg(verb.arg())
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(Stdio::null());
        if self.root.base() != Path::new("/") {
            cmd.env("WALPAD_VPN_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-vpn is not installed".to_string(),
            _ => format!("cannot run walpad-vpn: {e}"),
        })?;
        self.run = Some(Run {
            verb,
            child,
            deadline: now + self.timeouts[verb.idx()],
            cleanup,
        });
        Ok(())
    }

    fn kill(mut r: Run) {
        if let Some(pid) = rustix::process::Pid::from_raw(r.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    }

    fn apply_output(&mut self, out: &str) {
        self.st.handshake_age_s = field(out, "handshake_age").and_then(|v| v.parse().ok());
        self.st.rx_bytes = field(out, "rx").and_then(|v| v.parse().ok());
        self.st.tx_bytes = field(out, "tx").and_then(|v| v.parse().ok());
        self.st.revert_in_s = field(out, "revert_in").and_then(|v| v.parse().ok());
        if let Some(k) = field(out, "kind") {
            self.st.kind = match k {
                "wireguard" => Some(VpnKind::Wireguard),
                "openvpn" => Some(VpnKind::Openvpn),
                _ => self.st.kind,
            };
        }
    }

    fn clear_live(&mut self) {
        self.st.handshake_age_s = None;
        self.st.rx_bytes = None;
        self.st.tx_bytes = None;
        self.st.revert_in_s = None;
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match (r.verb, r.cleanup) {
            (Verb::Down, true) => self.clear_live(),
            (Verb::Up, _) => {
                if code == Some(0) && state == Some("on") {
                    self.apply_output(&out);
                    self.st.state = VpnState::On;
                    self.st.reason = None;
                    self.next_status = now + STATUS_EVERY;
                } else {
                    self.clear_live();
                    self.st.state = VpnState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-vpn failed (exit {c})"),
                        None => "walpad-vpn was killed".into(),
                    }));
                }
            }
            (Verb::Down, false) => {
                self.clear_live();
                if code == Some(0) {
                    self.st.state = VpnState::Off;
                    self.st.reason = None;
                } else {
                    self.st.state = VpnState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not turn the VPN off".into()));
                }
            }
            (Verb::Confirm, _) => {
                if code != Some(0) {
                    self.st.reason = reason;
                }
            }
            (Verb::Apply, _) => {
                // a tunnel that survived a core restart shows up in the next status poll
                if matches!(self.st.state, VpnState::Off) {
                    let _ = self.spawn(Verb::Status, now, false);
                }
            }
            (Verb::Status, _) => {
                // the script exits 1 for state=failed: the state field decides, not the exit code
                let Some(state) = state else { return };
                match state {
                    "on" => {
                        self.apply_output(&out);
                        self.st.state = VpnState::On;
                        self.st.reason = None;
                    }
                    "starting" => {
                        self.apply_output(&out);
                        self.st.state = VpnState::Starting;
                        self.st.reason = reason;
                    }
                    "failed" => {
                        self.clear_live();
                        self.st.state = VpnState::Failed;
                        self.st.reason = reason;
                    }
                    "off" => {
                        self.clear_live();
                        if matches!(self.st.state, VpnState::On | VpnState::Starting) {
                            self.st.state = VpnState::Off;
                        }
                    }
                    _ => {}
                }
                self.next_status = now + STATUS_EVERY;
            }
        }
    }

    /// Advance the running script. Returns true when the visible status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st.clone();
        if let Some(mut r) = self.run.take() {
            match r.child.try_wait() {
                Ok(Some(es)) => self.finish(r, es.code(), now),
                Ok(None) if now >= r.deadline => {
                    let verb = r.verb;
                    Self::kill(r);
                    match verb {
                        Verb::Status | Verb::Apply | Verb::Confirm => {
                            self.next_status = now + STATUS_EVERY
                        }
                        Verb::Up | Verb::Down => {
                            self.clear_live();
                            self.st.state = VpnState::Failed;
                            self.st.reason = Some(format!("walpad-vpn {} timed out", verb.arg()));
                            if verb == Verb::Up {
                                // tear down whatever the start left behind (routes, kill switch)
                                let _ = self.spawn(Verb::Down, now, true);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.apply_pending {
            self.apply_pending = false;
            let _ = self.spawn(Verb::Apply, now, false);
        } else if matches!(self.st.state, VpnState::On | VpnState::Starting)
            && now >= self.next_status
            && self.spawn(Verb::Status, now, false).is_err()
        {
            self.next_status = now + STATUS_EVERY;
        }
        self.st != before
    }
}
