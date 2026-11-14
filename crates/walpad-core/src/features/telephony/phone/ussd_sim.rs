//! USSD queries, SIM phonebook loading, contacts, and call log access for [`super::Phone`].
use super::ops::Op;
use super::{parse, store, Phone};
use crate::modem::MobileError;
use serde_json::{json, Value};
use std::time::Instant;
use walpad_proto::UssdStatus;

impl Phone {
    /// Apply a `+CUSD:` USSD response (from either the AT reply or the `/dev/radio/pttynoti` URC).
    pub fn on_cusd(&mut self, c: parse::Cusd) {
        // On MediaTek/3GPP, m=2 ("terminated by network") often carries the final USSD text.
        let failed = matches!(c.m, 4 | 5) || (matches!(c.m, 2 | 3) && c.text.is_none());
        let state = if failed { "failed" } else { "done" };
        self.ussd = Some(UssdStatus {
            state: state.into(),
            text: c
                .text
                .or_else(|| failed.then(|| "USSD request failed".into())),
            interactive: c.m == 1,
        });
        self.dirty = true;
    }

    /// `phone_ussd`: send a USSD code (`AT+CUSD=1,"<code>",15`).
    pub fn ussd(&mut self, code: &str, _now: Instant) -> Result<Value, MobileError> {
        if !walpad_proto::valid_dial_number(code) {
            return Err(MobileError::Invalid(
                "code must be 1-32 characters of 0-9 + * # ,".into(),
            ));
        }
        self.need_line(false)?;
        self.ussd = Some(UssdStatus {
            state: "running".into(),
            text: None,
            interactive: false,
        });
        self.queue.push_front(Op::Ussd(code.to_string()));
        self.dirty = true;
        Ok(json!({"state": "running"}))
    }

    pub fn ussd_cancel(&mut self) -> Result<Value, MobileError> {
        self.ussd = None;
        if self.gate.ready {
            self.queue.push_front(Op::UssdCancel);
        }
        self.dirty = true;
        Ok(json!({"cancelled": true}))
    }

    /// `phone_sim_contacts`: read SIM phonebook entries (`AT+CPBS="SM"` / `AT+CPBR`).
    pub fn sim_contacts(&mut self) -> Result<Value, MobileError> {
        self.need_line(false)?;
        self.queue.push_back(Op::SimContactsRange);
        Ok(json!({"state": "loading"}))
    }

    pub fn contact_list(&self) -> Value {
        json!({"contacts": self.contacts.all().iter()
            .map(|c| json!({"id": c.id, "name": c.name, "numbers": c.numbers}))
            .collect::<Vec<_>>()})
    }

    pub fn contact_save(
        &mut self,
        id: Option<&str>,
        name: &str,
        numbers: Vec<String>,
    ) -> Result<Value, MobileError> {
        match self.contacts.save(id, name, numbers) {
            Some(id) => Ok(json!({"id": id})),
            None => Err(MobileError::Refused(
                "the contact list is full or the contact does not exist".into(),
            )),
        }
    }

    pub fn contact_delete(&mut self, id: &str) -> Result<Value, MobileError> {
        Ok(json!({"deleted": self.contacts.delete(id)}))
    }

    /// Import a vCard file (`root` re-maps the path in tests).
    pub fn contact_import(&mut self, path: &std::path::Path) -> Result<Value, MobileError> {
        let meta = std::fs::metadata(path)
            .map_err(|_| MobileError::Refused("cannot read the file".into()))?;
        if meta.len() > 1024 * 1024 {
            return Err(MobileError::Refused("the file is too large".into()));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|_| MobileError::Refused("cannot read the file".into()))?;
        let cards = store::parse_vcards(&text);
        let found = cards.len();
        let added = self.contacts.import(cards);
        Ok(json!({"found": found, "added": added}))
    }

    pub fn call_log(&self) -> Value {
        json!({"calls": self.calls.all().iter()
            .map(|c| json!({"number": c.number, "name": self.name_of(Some(&c.number)),
                "ts": c.ts, "dur_s": c.dur_s, "kind": c.kind}))
            .collect::<Vec<_>>()})
    }

    pub fn call_log_clear(&mut self) -> Result<Value, MobileError> {
        self.calls.clear();
        Ok(json!({}))
    }
}
