use super::at::{AtCmd, Job};
use super::types::{JobKind, MobileError, Verb};
use super::Modem;
use serde_json::{json, Value};
use std::time::Instant;
use walpad_proto::{valid_apn, valid_sim_pin, MobileData, ModemState, Secret, SimState};

impl Modem {
    /// `mobile_radio {on}`: persist the choice; send AT+CFUN when the modem is ready.
    pub fn set_radio(&mut self, on: bool, now: Instant) -> Result<Value, MobileError> {
        if self.st.modem == ModemState::Absent {
            return Err(MobileError::Refused(
                "this system has no modem support".into(),
            ));
        }
        if let Some(d) = self.radio_file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        crate::hotspot::write_600(&self.radio_file, if on { b"1\n" } else { b"0\n" })
            .map_err(|e| MobileError::Failed(format!("cannot store the radio switch: {e}")))?;
        self.radio_pref = on;
        let ready = self.st.modem == ModemState::Ready;
        if ready {
            self.pref_applied = true;
            self.queue_radio(on);
            self.dispatch(now);
        }
        Ok(json!({"on": on, "applied": ready}))
    }

    /// `mobile_data {on}`.
    pub fn set_data(&mut self, on: bool, now: Instant) -> Result<Value, MobileError> {
        if self.st.modem != ModemState::Ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        if self.pending_data.is_some()
            || self
                .run
                .as_ref()
                .is_some_and(|r| matches!(r.verb, Verb::DataOn | Verb::DataOff))
        {
            return Err(MobileError::Busy("mobile data is switching".into()));
        }
        if on {
            if self.st.radio_on != Some(true) {
                return Err(MobileError::Refused(
                    "turn the mobile radio on first".into(),
                ));
            }
            if self.st.sim != SimState::Ready {
                return Err(MobileError::Refused("no usable SIM card".into()));
            }
            self.st.data = MobileData::Connecting;
        }
        self.st.data_reason = None;
        if self.run.is_some() || self.inflight {
            // a status run or an AT job holds the channel: start right after it
            self.pending_data = Some(on);
        } else {
            self.spawn(if on { Verb::DataOn } else { Verb::DataOff }, now)
                .map_err(|e| {
                    self.st.data = MobileData::Failed;
                    self.st.data_reason = Some(e.clone());
                    MobileError::Failed(e)
                })?;
        }
        Ok(json!({"data": self.st.data}))
    }

    /// `mobile_apn {apn}`: stored in /etc/walpad/mobile.conf (0600); used by the next `data on`.
    pub fn set_apn(&mut self, apn: &str) -> Result<Value, MobileError> {
        if !valid_apn(apn) {
            return Err(MobileError::Invalid(
                "apn must be 1-62 characters of A-Z a-z 0-9 . - _".into(),
            ));
        }
        let text = format!(
            "# walpad-core mobile_apn (owner request 2026-11-10); read by walpad-modem data on\napn={apn}\n"
        );
        crate::hotspot::write_600(&self.conf, text.as_bytes())
            .map_err(|e| MobileError::Failed(format!("cannot store the APN: {e}")))?;
        self.st.apn = Some(apn.to_string());
        eprintln!("walpad-core: mobile APN set to {apn}");
        Ok(json!({"apn": apn, "reconnect": self.st.data == MobileData::On}))
    }

    /// `sim_pin {pin}`: AT+CPIN="<pin>" over the channel only.
    pub fn sim_pin(&mut self, pin: &Secret, now: Instant) -> Result<Value, MobileError> {
        if !valid_sim_pin(pin.expose()) {
            return Err(MobileError::Invalid("pin must be 4-8 digits".into()));
        }
        if self.st.modem != ModemState::Ready {
            return Err(MobileError::Refused("the modem is not ready".into()));
        }
        match self.st.sim {
            SimState::PinRequired => {}
            SimState::PukRequired => {
                return Err(MobileError::Refused(
                    "the SIM is locked with a PUK: unlock it in a phone first".into(),
                ))
            }
            _ => {
                return Err(MobileError::Refused(
                    "the SIM does not ask for a PIN".into(),
                ))
            }
        }
        if self.queue.iter().any(|j| j.kind == JobKind::Pin) {
            return Err(MobileError::Busy("a PIN is being checked".into()));
        }
        self.queue.push_back(Job {
            kind: JobKind::Pin,
            cmds: vec![AtCmd::secret(format!("AT+CPIN=\"{}\"", pin.expose()))],
        });
        self.dispatch(now);
        Ok(json!({"accepted": true}))
    }

    /// `mobile_info`: identifiers on request only (never in status, never logged).
    pub fn info(&self) -> Value {
        json!({
            "modem": self.st.modem,
            "firmware": self.st.firmware,
            "imei": self.imei,
            "iccid": self.iccid,
        })
    }
}
