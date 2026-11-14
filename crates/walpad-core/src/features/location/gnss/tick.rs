use super::reading::{clean_reason, field, parse_reading, wall_ms};
use super::{Gnss, Run, Verb, CHECK_EVERY, MAX_JSON, READ_EVERY, RETRY_EVERY, STALE_MS};
use std::time::Instant;
use walpad_proto::{GpsState, GpsStatus};

impl Gnss {
    fn clear_position(&mut self) {
        let keep = (self.st.enabled, self.st.state, self.st.reason.take());
        self.st = GpsStatus {
            enabled: keep.0,
            state: keep.1,
            reason: keep.2,
            ..GpsStatus::default()
        };
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match r.verb {
            Verb::Start => {
                if code == Some(0) && state == Some("on") {
                    self.st.state = GpsState::Searching;
                    self.st.reason = None;
                    self.next_read = now;
                    self.next_check = now + CHECK_EVERY;
                } else {
                    self.clear_position();
                    self.st.state = GpsState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-gps failed (exit {c})"),
                        None => "walpad-gps was killed".into(),
                    }));
                }
            }
            Verb::Stop => {
                if code == Some(0) {
                    self.clear_position();
                    self.st.state = GpsState::Off;
                } else {
                    self.st.state = GpsState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not stop the GPS".into()));
                }
            }
            Verb::Status => {
                if code != Some(0) {
                    return;
                }
                let on = state == Some("on");
                if on && !self.running_state() && self.st.enabled {
                    self.st.state = GpsState::Searching;
                    self.st.reason = None;
                } else if !on && self.running_state() {
                    // died behind our back: report it, the retry brings it up again
                    self.clear_position();
                    self.st.state = GpsState::Failed;
                    self.st.reason = Some("the GPS stopped unexpectedly".into());
                    self.next_start = now;
                } else if !on && self.st.state == GpsState::Off {
                    // nothing to do
                }
                self.next_check = now + CHECK_EVERY;
            }
        }
    }

    fn read_state(&mut self, now: Instant) {
        self.next_read = now + READ_EVERY;
        let Ok(meta) = std::fs::metadata(&self.json) else {
            return;
        };
        if meta.len() > MAX_JSON {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&self.json) else {
            return;
        };
        let wall = wall_ms();
        let Some(r) = parse_reading(&text, wall) else {
            return;
        };
        let stale = r.updated_ms > 0 && wall - r.updated_ms > STALE_MS;
        let state = match (r.state.as_str(), stale) {
            (_, true) | ("off", _) => GpsState::Searching,
            ("fix", false) => GpsState::Fix,
            _ => GpsState::Searching,
        };
        let enabled = self.st.enabled;
        self.st = GpsStatus {
            enabled,
            state,
            reason: r.reason,
            lat: r.lat,
            lon: r.lon,
            alt_m: r.alt_m,
            accuracy_m: r.accuracy_m,
            satellites_used: r.used,
            satellites_view: r.view,
            age_s: r.age_s,
            signals: r.signals,
        };
        if stale {
            // the daemon stopped writing: ask the script whether it is still alive
            self.next_check = now;
        }
    }

    /// Advance the running script, read the position. Returns true when the visible status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st.clone();
        if let Some(mut r) = self.run.take() {
            match r.child.try_wait() {
                Ok(Some(es)) => self.finish(r, es.code(), now),
                Ok(None) if now >= r.deadline => {
                    let verb = r.verb;
                    Self::kill(r);
                    match verb {
                        Verb::Status => self.next_check = now + CHECK_EVERY,
                        Verb::Start | Verb::Stop => {
                            self.clear_position();
                            self.st.state = GpsState::Failed;
                            self.st.reason = Some(format!("walpad-gps {} timed out", verb.arg()));
                            if verb == Verb::Start {
                                let _ = self.spawn(Verb::Stop, now);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.running_state() {
            if now >= self.next_check && self.spawn(Verb::Status, now).is_err() {
                self.next_check = now + CHECK_EVERY;
            }
        } else if self.st.enabled
            && matches!(self.st.state, GpsState::Off | GpsState::Failed)
            && now >= self.next_start
        {
            // the switch is on but nothing runs (boot ordering, a crash): try again
            self.next_start = now + RETRY_EVERY;
            if self.spawn(Verb::Start, now).is_ok() {
                self.st.state = GpsState::Starting;
            }
        }
        if self.running_state()
            && self.run.as_ref().is_none_or(|r| r.verb == Verb::Status)
            && now >= self.next_read
        {
            self.read_state(now);
        }
        self.st != before
    }
}
