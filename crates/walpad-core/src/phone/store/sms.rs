use super::{read_json, same_number, write_json, MAX_SMS};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sms {
    pub id: u64,
    pub number: String,
    pub text: String,
    /// Milliseconds since the epoch.
    pub ts: u64,
    pub incoming: bool,
    pub read: bool,
    /// Outgoing only: `sending`, `sent` or `failed`.
    #[serde(default)]
    pub state: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SmsFile {
    next_id: u64,
    messages: Vec<Sms>,
}

pub struct SmsStore {
    path: PathBuf,
    f: SmsFile,
}

impl SmsStore {
    pub fn load(path: PathBuf) -> SmsStore {
        let mut f: SmsFile = read_json(&path);
        // a message that was still "sending" when the core stopped did not go out
        for m in f.messages.iter_mut().filter(|m| m.state == "sending") {
            m.state = "failed".into();
        }
        let max = f.messages.iter().map(|m| m.id).max().unwrap_or(0);
        f.next_id = f.next_id.max(max + 1).max(1);
        SmsStore { path, f }
    }

    pub fn all(&self) -> &[Sms] {
        &self.f.messages
    }

    pub fn unread(&self) -> u32 {
        self.f
            .messages
            .iter()
            .filter(|m| m.incoming && !m.read)
            .count() as u32
    }

    /// Add a message, trimming the oldest beyond the cap. Returns its id.
    pub fn add(&mut self, mut m: Sms) -> u64 {
        m.id = self.f.next_id;
        self.f.next_id += 1;
        let id = m.id;
        self.f.messages.push(m);
        if self.f.messages.len() > MAX_SMS {
            let drop = self.f.messages.len() - MAX_SMS;
            self.f.messages.drain(..drop);
        }
        self.save();
        id
    }

    pub fn set_state(&mut self, id: u64, state: &str) {
        if let Some(m) = self.f.messages.iter_mut().find(|m| m.id == id) {
            m.state = state.into();
            self.save();
        }
    }

    pub fn delete(&mut self, id: u64) -> bool {
        let n = self.f.messages.len();
        self.f.messages.retain(|m| m.id != id);
        let gone = self.f.messages.len() != n;
        if gone {
            self.save();
        }
        gone
    }

    /// Mark the conversation with `number` read; true when something changed.
    pub fn mark_read(&mut self, number: &str) -> bool {
        let mut any = false;
        for m in self
            .f
            .messages
            .iter_mut()
            .filter(|m| m.incoming && !m.read && same_number(&m.number, number))
        {
            m.read = true;
            any = true;
        }
        if any {
            self.save();
        }
        any
    }

    fn save(&self) -> bool {
        write_json(&self.path, &self.f)
    }
}
