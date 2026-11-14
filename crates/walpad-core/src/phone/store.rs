//! The phone's persistent data (owner request 2026-11-11): text messages, contacts and the call
//! log, each one small JSON file under /var/lib/walpad (mode 0600, written atomically, size-capped).
//! Everything in them is private: nothing here prints a number, a name or a text.
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const SMS_FILE: &str = "/var/lib/walpad/sms.json";
pub const CONTACTS_FILE: &str = "/var/lib/walpad/contacts.json";
pub const CALLS_FILE: &str = "/var/lib/walpad/calls.json";

pub const MAX_SMS: usize = 500;
pub const MAX_CONTACTS: usize = 1000;
pub const MAX_CALLS: usize = 200;
/// A file larger than this is not read (a damaged or foreign file never eats the memory).
const MAX_FILE: u64 = 2 * 1024 * 1024;

pub(super) fn read_json<T: for<'a> Deserialize<'a> + Default>(p: &Path) -> T {
    match std::fs::metadata(p) {
        Ok(m) if m.len() <= MAX_FILE => std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        _ => T::default(),
    }
}

pub(super) fn write_json<T: Serialize>(p: &Path, v: &T) -> bool {
    match serde_json::to_vec(v) {
        Ok(b) => crate::hotspot::write_600(p, &b).is_ok(),
        Err(_) => false,
    }
}

/// Do two numbers name the same party (national and international form)? Shared with the shell.
pub use walpad_proto::same_number;

mod calls;
mod contacts;
mod sms;
#[cfg(test)]
mod tests;

pub use calls::{CallLog, CallRec};
pub use contacts::{parse_vcards, Contact, ContactStore};
pub use sms::{Sms, SmsStore};
