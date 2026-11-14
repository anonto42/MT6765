use super::parse::{clean_reason, field, Verb};
use super::{Hotspot, Run, STATUS_EVERY};
use std::time::Instant;
use walpad_proto::{HotspotInternet, HotspotState};

impl Hotspot {
    fn apply_output(&mut self, out: &str) {
        self.st.clients = field(out, "clients").and_then(|c| c.parse().ok());
        self.st.internet = match field(out, "internet") {
            Some("shared") => Some(HotspotInternet::Shared),
            Some("no_uplink") => Some(HotspotInternet::NoUplink),
            Some("unavailable") => Some(HotspotInternet::Unavailable),
            _ => None,
        };
        self.st.exclusive = field(out, "exclusive") == Some("1");
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match (r.verb, r.cleanup) {
            (Verb::Stop, true) => {
                // keep the failure that caused the cleanup; the client is resumed by the script
                self.st.exclusive = false;
                self.st.clients = None;
                self.st.internet = None;
            }
            (Verb::Start, _) => {
                if code == Some(0) && state == Some("on") {
                    self.apply_output(&out);
                    self.st.state = HotspotState::On;
                    self.st.reason = None;
                    self.next_status = now + STATUS_EVERY;
                } else {
                    self.apply_output(&out);
                    self.st.state = HotspotState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-hotspot failed (exit {c})"),
                        None => "walpad-hotspot was killed".into(),
                    }));
                    self.st.clients = None;
                }
            }
            (Verb::Stop, false) => {
                self.apply_output(&out);
                self.st.clients = None;
                self.st.internet = None;
                if code == Some(0) {
                    self.st.state = HotspotState::Off;
                    self.st.reason = None;
                    if std::mem::take(&mut self.restart) {
                        if let Err(e) = self.spawn(Verb::Start, now, false) {
                            self.st.state = HotspotState::Failed;
                            self.st.reason = Some(e);
                        } else {
                            self.st.state = HotspotState::Starting;
                        }
                    }
                } else {
                    self.restart = false;
                    self.st.state = HotspotState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not stop the hotspot".into()));
                }
            }
            (Verb::Status, _) => {
                if code != Some(0) {
                    return;
                }
                self.apply_output(&out);
                match state {
                    Some("on") => self.st.state = HotspotState::On,
                    Some("failed") => {
                        self.st.state = HotspotState::Failed;
                        self.st.reason = reason;
                        self.st.clients = None;
                    }
                    Some("off") | None => {
                        self.st.state = HotspotState::Off;
                        self.st.clients = None;
                    }
                    Some(_) => {}
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
                        Verb::Status => self.next_status = now + STATUS_EVERY,
                        Verb::Start | Verb::Stop => {
                            self.restart = false;
                            self.st.state = HotspotState::Failed;
                            self.st.reason =
                                Some(format!("walpad-hotspot {} timed out", verb.arg()));
                            self.st.clients = None;
                            if verb == Verb::Start {
                                // tear down whatever the start left behind
                                let _ = self.spawn(Verb::Stop, now, true);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.st.state == HotspotState::On
            && now >= self.next_status
            && self.spawn(Verb::Status, now, false).is_err()
        {
            self.next_status = now + STATUS_EVERY;
        }
        self.st != before
    }
}
