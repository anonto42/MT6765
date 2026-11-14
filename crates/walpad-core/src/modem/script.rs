use super::parse;
use super::types::{field, Run, Verb, PATH};
use super::Modem;
use serde_json::json;
use std::process::{Command, Stdio};
use std::time::Instant;
use walpad_proto::{valid_apn, MobileData};

impl Modem {
    pub(super) fn spawn(&mut self, verb: Verb, now: Instant) -> Result<(), String> {
        if let Some(d) = self.out.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let out = std::fs::File::create(&self.out).map_err(|e| format!("output file: {e}"))?;
        let err = out.try_clone().map_err(|e| format!("output file: {e}"))?;
        let mut cmd = Command::new(&self.script);
        cmd.args(verb.args())
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err);
        if self.root.base() != std::path::Path::new("/") {
            cmd.env("WALPAD_MODEM_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-modem is not installed".to_string(),
            _ => format!("cannot run walpad-modem: {e}"),
        })?;
        let t = match verb {
            Verb::Start => self.t.start,
            Verb::DataOn | Verb::DataOff => self.t.data,
            Verb::DataStatus => self.t.status,
        };
        self.run = Some(Run {
            verb,
            child,
            deadline: now + t,
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

    pub(super) fn reason_of(out: &str, code: Option<i32>) -> String {
        if let Some(r) = field(out, "reason") {
            return parse::clean(r, 120);
        }
        // `walpad-modem start` speaks in "[modem] ..." lines: the last one says how it ended
        if let Some(l) = out.lines().rev().find_map(|l| l.strip_prefix("[modem] ")) {
            return parse::clean(l, 120);
        }
        match code {
            Some(c) => format!("walpad-modem failed (exit {c})"),
            None => "walpad-modem was killed".into(),
        }
    }

    fn finish(&mut self, verb: Verb, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let state = field(&out, "state");
        match verb {
            Verb::Start => {
                if self.tty.exists() {
                    self.become_ready(now);
                } else {
                    let why = Self::reason_of(&out, code);
                    self.fail(format!("the modem did not start: {why}"));
                    self.next_check = now + self.t.recheck;
                }
            }
            Verb::DataOn | Verb::DataOff | Verb::DataStatus => {
                let ip = field(&out, "ip").map(|s| parse::clean(s, 45));
                let apn = field(&out, "apn")
                    .filter(|a| valid_apn(a))
                    .map(String::from);
                match (verb, code, state) {
                    (Verb::DataStatus, c, _) if c != Some(0) => return, // older script
                    (_, Some(0), Some("on")) => {
                        self.st.data = MobileData::On;
                        self.st.data_reason = None;
                        self.st.ip = ip;
                        self.st.apn_in_use = apn;
                    }
                    (_, Some(0), Some("off") | None) if verb != Verb::DataOn => {
                        self.st.data = MobileData::Off;
                        self.st.data_reason = None;
                        self.st.ip = None;
                    }
                    _ => {
                        self.st.data = MobileData::Failed;
                        self.st.data_reason = Some(Self::reason_of(&out, code));
                        self.st.ip = ip;
                        if apn.is_some() {
                            self.st.apn_in_use = apn;
                        }
                    }
                }
                if verb != Verb::DataStatus {
                    eprintln!(
                        "walpad-core: mobile data {} -> {:?}{}",
                        if verb == Verb::DataOn { "on" } else { "off" },
                        self.st.data,
                        self.st
                            .data_reason
                            .as_deref()
                            .map(|r| format!(" ({r})"))
                            .unwrap_or_default()
                    );
                    let n = json!({"kind": "mobile_data", "state": self.st.data,
                        "reason": self.st.data_reason, "ip": self.st.ip});
                    self.notice(n);
                }
                self.next_poll = now;
            }
        }
    }

    pub(super) fn tick_run(&mut self, now: Instant) {
        let Some(mut r) = self.run.take() else {
            return;
        };
        match r.child.try_wait() {
            Ok(Some(es)) => self.finish(r.verb, es.code(), now),
            Ok(None) if now >= r.deadline => {
                let verb = r.verb;
                Self::kill(r);
                match verb {
                    Verb::Start => {
                        if self.tty.exists() {
                            self.become_ready(now)
                        } else {
                            self.fail("walpad-modem start timed out".into());
                            self.next_check = now + self.t.recheck;
                        }
                    }
                    Verb::DataOn | Verb::DataOff => {
                        self.st.data = MobileData::Failed;
                        self.st.data_reason = Some("walpad-modem data timed out".into());
                        let n = json!({"kind": "mobile_data", "state": self.st.data,
                            "reason": self.st.data_reason, "ip": null});
                        self.notice(n);
                    }
                    Verb::DataStatus => {}
                }
            }
            Ok(None) => self.run = Some(r),
            Err(_) => Self::kill(r),
        }
    }
}
