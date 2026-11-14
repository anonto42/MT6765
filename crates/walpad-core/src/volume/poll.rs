use super::parse::{field, Job, Run};
use super::Volume;
use std::time::Instant;

impl Volume {
    /// Non-blocking check on running helpers, scheduled saves and periodic probes. Returns true
    /// when the reported status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st;
        if let Some((child, deadline)) = self.tone.as_mut() {
            match child.try_wait() {
                Ok(None) if now < *deadline => {}
                Ok(None) => {
                    if let Some((c, _)) = self.tone.take() {
                        Self::kill(Run {
                            job: Job::Get,
                            child: c,
                            deadline: now,
                        });
                    }
                }
                _ => self.tone = None,
            }
        }
        if let Some(t) = self.save_at {
            if now >= t {
                self.save_at = None;
                self.save();
            }
        }
        if let Some(r) = self.run.as_mut() {
            match r.child.try_wait() {
                Ok(Some(status)) => {
                    let Some(r) = self.run.take() else {
                        return false;
                    };
                    let text = std::fs::read_to_string(&self.out).unwrap_or_default();
                    self.finish(r.job, status.success(), &text);
                }
                Ok(None) if now >= r.deadline => {
                    if let Some(r) = self.run.take() {
                        Self::kill(r);
                    }
                    self.st.available = false;
                }
                Ok(None) => {}
                Err(_) => {
                    self.run = None;
                    self.st.available = false;
                }
            }
        }
        if self.run.is_none() {
            if self.pending {
                self.pending = false;
                self.spawn(Job::Apply, now);
            } else if now >= self.next_probe {
                self.next_probe = now + self.probe_every;
                self.spawn(Job::Get, now);
            }
        }
        self.st != before
    }

    fn finish(&mut self, job: Job, ok: bool, out: &str) {
        let avail = ok && field(out, "available") == Some("1");
        self.st.available = avail;
        if job == Job::Get {
            let backend = field(out, "backend").map(str::to_string);
            // a sound server that started (or stopped) since the last probe: apply the stored state
            if avail && backend != self.backend {
                self.pending = true;
            }
            self.backend = backend;
        } else if avail {
            self.backend = field(out, "backend")
                .map(str::to_string)
                .or(self.backend.take());
        }
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        if let Some(r) = self.run.take() {
            Self::kill(r);
        }
        if let Some((mut c, _)) = self.tone.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if self.save_at.is_some() {
            self.save();
        }
    }
}
