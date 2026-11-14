use super::super::ctrl::CtrlClient;
use super::super::parse::{self, Event};
use super::types::{Op, WifiEvent};
use super::{Wifi, CONNECT_POLL, STATUS_EVERY};
use std::time::Instant;

impl Wifi {
    fn finish_connect(&mut self, ok: bool, error: Option<&'static str>, out: &mut Vec<WifiEvent>) {
        let Op::Connecting {
            id,
            ssid,
            was_new,
            others,
            ..
        } = std::mem::replace(&mut self.op, Op::Idle)
        else {
            return;
        };
        // SELECT_NETWORK disabled every other saved network: re-enable them so they auto-connect again
        for o in &others {
            let _ = self.req(&format!("ENABLE_NETWORK {o}"));
        }
        if ok {
            self.save_config();
        } else if was_new {
            let _ = self.req(&format!("REMOVE_NETWORK {id}"));
        }
        self.next_status = Instant::now();
        out.push(WifiEvent::Connect { ok, error, ssid });
    }

    fn refresh_status(&mut self) {
        if let Ok(t) = self.req("STATUS") {
            let st = parse::parse_status(&t);
            let completed = st.state.as_deref() == Some("COMPLETED");
            self.live.state = st.state;
            self.live.ssid = st.ssid.map(|s| parse::ssid_display(&s));
            self.live.ip = st.ip;
            self.live.signal_dbm = if completed {
                self.req("SIGNAL_POLL")
                    .ok()
                    .and_then(|t| parse::parse_signal_poll(&t))
            } else {
                None
            };
        }
    }

    /// Reconnect if needed, consume events, advance the running operation, refresh status.
    pub fn tick(&mut self, now: Instant) -> Vec<WifiEvent> {
        let mut out = Vec::new();
        if self.ctrl.is_none() {
            if now >= self.next_reconnect {
                self.next_reconnect = now + self.reconnect_every;
                if let Ok(c) = CtrlClient::connect(&self.run_dir, &self.server) {
                    self.ctrl = Some(c);
                    self.next_status = now;
                }
            }
            return out;
        }
        // events
        let mut scan_done = false;
        let mut connected = false;
        let mut wrong_key: Option<u32> = None;
        loop {
            let ev = match self.ctrl.as_ref().map(|c| c.poll_event()) {
                Some(Ok(Some(l))) => parse::parse_event(&l),
                Some(Ok(None)) | None => break,
                Some(Err(_)) => {
                    self.ctrl = None;
                    self.op = Op::Idle;
                    return out;
                }
            };
            match ev {
                Event::ScanResults => scan_done = true,
                Event::Connected => connected = true,
                Event::Disconnected => {
                    if !self.busy() {
                        out.push(WifiEvent::Disconnected);
                    }
                    self.next_status = now;
                }
                Event::TempDisabled { id, reason } if reason == "WRONG_KEY" => wrong_key = Some(id),
                _ => {}
            }
        }
        // operations
        match &self.op {
            Op::Scanning { deadline } => {
                if scan_done || now >= *deadline {
                    self.op = Op::Idle;
                    let networks = self
                        .req("SCAN_RESULTS")
                        .map(|t| parse::parse_scan_results(&t))
                        .unwrap_or_default();
                    out.push(WifiEvent::Scan { networks });
                }
            }
            Op::Connecting {
                id,
                ssid,
                deadline,
                next_poll,
                ..
            } => {
                let (id, ssid, deadline, poll_due) =
                    (*id, ssid.clone(), *deadline, now >= *next_poll || connected);
                if wrong_key == Some(id) {
                    self.finish_connect(false, Some("wrong_password"), &mut out);
                } else if now >= deadline {
                    self.finish_connect(false, Some("timeout"), &mut out);
                } else if poll_due {
                    if let Op::Connecting { next_poll, .. } = &mut self.op {
                        *next_poll = now + CONNECT_POLL;
                    }
                    if let Ok(t) = self.req("STATUS") {
                        let st = parse::parse_status(&t);
                        if st.state.as_deref() == Some("COMPLETED")
                            && st.ssid.as_deref() == Some(ssid.as_slice())
                        {
                            self.finish_connect(true, None, &mut out);
                        }
                    }
                }
            }
            Op::Idle => {}
        }
        if self.ctrl.is_some() && now >= self.next_status {
            self.next_status = now + STATUS_EVERY;
            self.refresh_status();
        }
        out
    }
}
