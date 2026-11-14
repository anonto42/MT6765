use super::at::{AtCmd, AtError, Done, Final, Job, Outcome, Reply};
use super::parse;
use super::types::{at_error_text, job_notice, ok_final, pin_error, JobKind, Verb};
use super::Modem;
use serde_json::json;
use std::time::Instant;
use walpad_proto::{MobileData, MobileReg, ModemState, SimState};

impl Modem {
    pub(super) fn poll_job(&self) -> Job<JobKind> {
        let ccid = self.st.sim == SimState::Ready && self.iccid.is_none();
        let full = self.st.radio_on == Some(true) && self.st.sim == SimState::Ready;
        let mut cmds = vec![AtCmd::new("AT+CFUN?"), AtCmd::new("AT+CPIN?")];
        if ccid {
            cmds.push(AtCmd::new("AT+CCID"));
        }
        if full {
            for c in [
                "AT+CSQ",
                "AT+CREG?",
                "AT+CGREG?",
                "AT+CEREG?",
                "AT+COPS=3,0",
                "AT+COPS?",
            ] {
                cmds.push(AtCmd::new(c));
            }
        }
        Job {
            kind: JobKind::Poll { ccid, full },
            cmds,
        }
    }

    pub(super) fn dispatch(&mut self, now: Instant) {
        if self.run.is_some() || self.inflight || self.st.modem != ModemState::Ready {
            return;
        }
        if let Some(on) = self.pending_data.take() {
            if let Err(e) = self.spawn(if on { Verb::DataOn } else { Verb::DataOff }, now) {
                self.st.data = MobileData::Failed;
                self.st.data_reason = Some(e);
            }
            return;
        }
        let job = match self.queue.pop_front() {
            Some(j) => j,
            None => match self.phone.next_job(now) {
                Some(j) => j,
                // the slow network poll waits while a call is on
                None if self.phone.in_call() => return,
                None if now >= self.next_poll => {
                    // pushed back again when the result arrives; this guards against a stuck worker
                    self.next_poll = now + self.t.poll_on.max(self.t.at_cmd * 10);
                    self.poll_job()
                }
                None => return,
            },
        };
        match &self.worker {
            Some(w) if w.send(job) => self.inflight = true,
            _ => {
                self.worker = None;
                self.fail("the AT worker stopped".into());
            }
        }
    }

    pub(super) fn handle(&mut self, d: Done<JobKind>, now: Instant) {
        if let Some(Err(AtError::Open(e))) = d.results.first() {
            let why = format!("AT channel unavailable ({e})");
            if let JobKind::Radio(_) | JobKind::Pin = d.kind {
                self.notice(json!({"kind": job_notice(d.kind), "ok": false, "error": "the modem is not available"}));
            }
            return self.lost(&why, now);
        }
        let timed_out = d.results.contains(&Err(AtError::Timeout));
        match d.kind {
            JobKind::Init => {
                let r = |i: usize| d.results.get(i).and_then(|o| o.as_ref().ok());
                if let Some(f) = r(2).and_then(parse::ati) {
                    self.st.firmware = Some(f);
                }
                if let Some(i) = r(3).and_then(parse::imei) {
                    eprintln!("walpad-core: modem IMEI {}", parse::mask(&i));
                    self.imei = Some(i);
                }
            }
            JobKind::Poll { ccid, full } => self.handle_poll(&d.results, ccid, full, now),
            JobKind::Phone(k) => self.phone.handle(k, &d.results, now),
            JobKind::Radio(on) => {
                let ok = ok_final(d.results.first());
                if ok {
                    self.st.radio_on = Some(on);
                    if !on {
                        self.clear_network();
                        if matches!(self.st.data, MobileData::On | MobileData::Connecting) {
                            self.pending_data = Some(false);
                        }
                    }
                }
                eprintln!(
                    "walpad-core: mobile radio {} {}",
                    if on { "on" } else { "off" },
                    if ok { "done" } else { "failed" }
                );
                let err = (!ok).then(|| at_error_text(d.results.first()));
                self.notice(json!({"kind": "mobile_radio", "on": on, "ok": ok, "error": err}));
                self.next_poll = now;
            }
            JobKind::Pin => {
                let (ok, err) = match d.results.first() {
                    Some(Ok(r)) if r.fin == Final::Ok => (true, None),
                    Some(Ok(Reply {
                        fin: Final::Cme(e), ..
                    })) => (false, Some(pin_error(e))),
                    other => (false, Some(at_error_text(other))),
                };
                // never the PIN, only the outcome
                eprintln!(
                    "walpad-core: SIM PIN {}",
                    if ok { "accepted" } else { "rejected" }
                );
                self.notice(json!({"kind": "mobile_pin", "ok": ok, "error": err}));
                self.next_poll = now;
            }
        }
        if timed_out {
            self.st.reason = Some("the modem does not answer (AT timeout)".into());
        } else if self.st.modem == ModemState::Ready {
            self.st.reason = None;
        }
    }

    pub(super) fn handle_poll(
        &mut self,
        results: &[Outcome],
        ccid: bool,
        full: bool,
        now: Instant,
    ) {
        let mut it = results.iter();
        let mut next = || it.next().and_then(|o| o.as_ref().ok());
        let cfun = next().and_then(parse::cfun);
        let sim = next().map(parse::cpin);
        if cfun.is_some() {
            self.st.radio_on = cfun;
        }
        let old_sim = self.st.sim;
        if let Some(s) = sim {
            self.st.sim = s;
        }
        if ccid {
            if let Some(c) = next().and_then(parse::iccid) {
                self.iccid = Some(c);
            }
        }
        let usable = self.st.radio_on == Some(true) && self.st.sim == SimState::Ready;
        if full && usable {
            let sig = next().and_then(parse::csq);
            let regs = [
                next().and_then(|r| parse::reg(r, "+CREG:")),
                next().and_then(|r| parse::reg(r, "+CGREG:")),
                next().and_then(|r| parse::reg(r, "+CEREG:")),
            ];
            let _set_format = next();
            let (oper, tech) = next().map(parse::cops).unwrap_or((None, None));
            self.st.signal_bars = sig.map(|s| s.0);
            self.st.signal_dbm = sig.map(|s| s.1);
            self.st.registration = parse::combine_reg(&regs);
            let registered = matches!(self.st.registration, MobileReg::Home | MobileReg::Roaming);
            self.st.operator = oper.filter(|_| registered);
            self.st.tech = tech.filter(|_| registered).map(String::from);
        } else if !usable {
            self.clear_network();
        }
        if self.st.sim != old_sim {
            eprintln!("walpad-core: SIM {:?} -> {:?}", old_sim, self.st.sim);
            if self.st.sim != SimState::Ready {
                self.iccid = None;
            }
        }
        // apply the persisted radio choice once the modem told us where it stands
        if !self.pref_applied && self.st.radio_on.is_some() {
            self.pref_applied = true;
            if self.st.radio_on != Some(self.radio_pref) {
                self.queue_radio(self.radio_pref);
            }
        }
        // a SIM change, or a usable SIM seen by a short poll: the full poll follows at once
        self.next_poll = if self.st.sim != old_sim || (usable && !full) {
            now
        } else if self.st.radio_on == Some(true) {
            now + self.t.poll_on
        } else {
            now + self.t.poll_off
        };
    }

    pub(super) fn queue_radio(&mut self, on: bool) {
        self.queue.retain(|j| !matches!(j.kind, JobKind::Radio(_)));
        self.queue.push_back(Job {
            kind: JobKind::Radio(on),
            cmds: vec![AtCmd::new(if on { "AT+CFUN=1" } else { "AT+CFUN=0" })],
        });
    }

    /// Advance the worker and the script. Returns true when the status changed.
    pub(super) fn poll_noti(&mut self) {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;

        if self.st.modem != ModemState::Ready {
            self.noti = None;
            self.noti_buf.clear();
            return;
        }
        if self.noti.is_none() && self.noti_tty.exists() {
            if let Ok(f) = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
                .open(&self.noti_tty)
            {
                if let Ok(mut t) = rustix::termios::tcgetattr(&f) {
                    t.make_raw();
                    let _ =
                        rustix::termios::tcsetattr(&f, rustix::termios::OptionalActions::Now, &t);
                }
                self.noti = Some(f);
            }
        }
        let Some(f) = self.noti.as_mut() else {
            return;
        };
        let mut buf = [0u8; 1024];
        loop {
            match f.read(&mut buf) {
                Ok(0) => {
                    self.noti = None;
                    break;
                }
                Ok(n) => {
                    self.noti_buf.push_str(&String::from_utf8_lossy(&buf[..n]));
                    if self.noti_buf.len() > 8192 {
                        let cut = self.noti_buf.len() - 4096;
                        self.noti_buf.drain(..cut);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    self.noti = None;
                    break;
                }
            }
        }
        // Drain complete lines once any quoted +CUSD: payload has balanced quotes.
        if let Some(end) = self.noti_buf.rfind('\n') {
            let chunk = &self.noti_buf[..=end];
            if chunk.bytes().filter(|&b| b == b'"').count() % 2 == 0 {
                let lines: Vec<String> = chunk
                    .split(['\n', '\r'])
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if let Some(c) = crate::phone::parse::cusd(&lines) {
                    self.phone.on_cusd(c);
                }
                self.noti_buf.drain(..=end);
            }
        }
    }
}
