//! The phone's persistent data (owner request 2026-11-11): text messages, contacts and the call
//! log, each one small JSON file under /var/lib/walpad (mode 0600, written atomically, size-capped).
//! Everything in them is private: nothing here prints a number, a name or a text.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SMS_FILE: &str = "/var/lib/walpad/sms.json";
pub const CONTACTS_FILE: &str = "/var/lib/walpad/contacts.json";
pub const CALLS_FILE: &str = "/var/lib/walpad/calls.json";

pub const MAX_SMS: usize = 500;
pub const MAX_CONTACTS: usize = 1000;
pub const MAX_CALLS: usize = 200;
/// A file larger than this is not read (a damaged or foreign file never eats the memory).
const MAX_FILE: u64 = 2 * 1024 * 1024;

fn read_json<T: for<'a> Deserialize<'a> + Default>(p: &Path) -> T {
    match std::fs::metadata(p) {
        Ok(m) if m.len() <= MAX_FILE => std::fs::read_to_string(p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default(),
        _ => T::default(),
    }
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> bool {
    match serde_json::to_vec(v) {
        Ok(b) => crate::hotspot::write_600(p, &b).is_ok(),
        Err(_) => false,
    }
}

/// The digits of a number (no `+`, no separators).
pub fn digits(n: &str) -> String {
    n.chars().filter(char::is_ascii_digit).collect()
}

/// Do two numbers name the same party? `01711234567` and `+8801711234567` do: one is the
/// suffix of the other, with at least 7 digits in common.
pub fn same_number(a: &str, b: &str) -> bool {
    let (a, b) = (digits(a), digits(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (long, short) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    short.len() >= 7 && long.ends_with(short.as_str())
}

// ---------------------------------------------------------------------------- messages
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

// ---------------------------------------------------------------------------- contacts
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub numbers: Vec<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ContactsFile {
    next_id: u64,
    contacts: Vec<Contact>,
}

pub struct ContactStore {
    path: PathBuf,
    f: ContactsFile,
}

impl ContactStore {
    pub fn load(path: PathBuf) -> ContactStore {
        let mut f: ContactsFile = read_json(&path);
        f.next_id = f.next_id.max(1);
        ContactStore { path, f }
    }

    /// Sorted by name (case-insensitive).
    pub fn all(&self) -> Vec<&Contact> {
        let mut v: Vec<&Contact> = self.f.contacts.iter().collect();
        v.sort_by_key(|c| c.name.to_lowercase());
        v
    }

    pub fn name_of(&self, number: &str) -> Option<&str> {
        self.f
            .contacts
            .iter()
            .find(|c| c.numbers.iter().any(|n| same_number(n, number)))
            .map(|c| c.name.as_str())
    }

    /// Create (`id` None) or replace a contact. None when the store is full or the id is unknown.
    pub fn save(&mut self, id: Option<&str>, name: &str, numbers: Vec<String>) -> Option<String> {
        let id = match id {
            Some(i) => {
                let c = self.f.contacts.iter_mut().find(|c| c.id == i)?;
                c.name = name.into();
                c.numbers = numbers;
                i.to_string()
            }
            None => {
                if self.f.contacts.len() >= MAX_CONTACTS {
                    return None;
                }
                let i = format!("c{}", self.f.next_id);
                self.f.next_id += 1;
                self.f.contacts.push(Contact {
                    id: i.clone(),
                    name: name.into(),
                    numbers,
                });
                i
            }
        };
        self.persist();
        Some(id)
    }

    pub fn delete(&mut self, id: &str) -> bool {
        let n = self.f.contacts.len();
        self.f.contacts.retain(|c| c.id != id);
        let gone = self.f.contacts.len() != n;
        if gone {
            self.persist();
        }
        gone
    }

    /// Add parsed vCards, skipping a name with the same numbers; returns how many were new.
    pub fn import(&mut self, cards: Vec<(String, Vec<String>)>) -> usize {
        let mut added = 0;
        for (name, numbers) in cards {
            if self.f.contacts.len() >= MAX_CONTACTS {
                break;
            }
            let dup = self.f.contacts.iter().any(|c| {
                c.name == name
                    && numbers
                        .iter()
                        .all(|n| c.numbers.iter().any(|m| same_number(m, n)))
            });
            if dup {
                continue;
            }
            let id = format!("c{}", self.f.next_id);
            self.f.next_id += 1;
            self.f.contacts.push(Contact { id, name, numbers });
            added += 1;
        }
        if added > 0 {
            self.persist();
        }
        added
    }

    fn persist(&self) -> bool {
        write_json(&self.path, &self.f)
    }
}

/// Names and numbers of a vCard file (2.1 / 3.0 / 4.0): `FN` (or `N`) and every `TEL`.
pub fn parse_vcards(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut name: Option<String> = None;
    let mut nums: Vec<String> = Vec::new();
    let mut inside = false;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        let up = line.to_ascii_uppercase();
        if up == "BEGIN:VCARD" {
            inside = true;
            name = None;
            nums.clear();
        } else if up == "END:VCARD" {
            if inside {
                if let (Some(n), false) = (name.take(), nums.is_empty()) {
                    out.push((n, std::mem::take(&mut nums)));
                }
            }
            inside = false;
        } else if inside {
            let Some((key, val)) = line.split_once(':') else {
                continue;
            };
            let prop = key.split(';').next().unwrap_or("").to_ascii_uppercase();
            match prop.as_str() {
                "FN" if !val.trim().is_empty() => name = Some(clip_name(val)),
                "N" if name.is_none() => {
                    let parts: Vec<&str> =
                        val.split(';').filter(|p| !p.trim().is_empty()).collect();
                    // N is family;given;...: show "given family"
                    let n = match parts.as_slice() {
                        [fam, giv, ..] => format!("{giv} {fam}"),
                        [one] => one.to_string(),
                        _ => String::new(),
                    };
                    if !n.trim().is_empty() {
                        name = Some(clip_name(&n));
                    }
                }
                "TEL" => {
                    let n: String = val
                        .chars()
                        .filter(|c| c.is_ascii_digit() || *c == '+')
                        .collect();
                    if walpad_proto::valid_sms_number(&n) && nums.len() < 5 && !nums.contains(&n) {
                        nums.push(n);
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn clip_name(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_string()
}

// ---------------------------------------------------------------------------- call log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRec {
    pub number: String,
    pub ts: u64,
    pub dur_s: u64,
    /// `in`, `out` or `missed`.
    pub kind: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CallsFile {
    calls: Vec<CallRec>,
}

pub struct CallLog {
    path: PathBuf,
    f: CallsFile,
}

impl CallLog {
    pub fn load(path: PathBuf) -> CallLog {
        CallLog {
            f: read_json(&path),
            path,
        }
    }
    pub fn all(&self) -> &[CallRec] {
        &self.f.calls
    }
    pub fn add(&mut self, r: CallRec) {
        self.f.calls.push(r);
        if self.f.calls.len() > MAX_CALLS {
            let drop = self.f.calls.len() - MAX_CALLS;
            self.f.calls.drain(..drop);
        }
        write_json(&self.path, &self.f);
    }
    pub fn clear(&mut self) {
        self.f.calls.clear();
        write_json(&self.path, &self.f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("walpad-phone-store-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn numbers_match_across_formats() {
        assert!(same_number("01711234567", "+8801711234567"));
        assert!(same_number("+880 1711-234567", "01711234567"));
        assert!(same_number("999", "999"));
        assert!(
            !same_number("999", "1999"),
            "short numbers match exactly only"
        );
        assert!(!same_number("01711234567", "01711234568"));
        assert!(!same_number("", "123"));
    }

    #[test]
    fn sms_store_persists_trims_and_counts_unread() {
        let d = tmp("sms");
        let p = d.join("sms.json");
        let mut s = SmsStore::load(p.clone());
        let mk = |n: &str, inc: bool| Sms {
            id: 0,
            number: n.into(),
            text: "t".into(),
            ts: 1,
            incoming: inc,
            read: !inc,
            state: if inc { "".into() } else { "sending".into() },
        };
        let a = s.add(mk("01711234567", true));
        let b = s.add(mk("01711234567", false));
        assert_eq!((a, b), (1, 2));
        assert_eq!(s.unread(), 1);
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        // reload: the outgoing one that never finished is failed, ids continue
        let mut s2 = SmsStore::load(p.clone());
        assert_eq!(s2.all()[1].state, "failed");
        assert_eq!(s2.add(mk("999", true)), 3);
        assert!(s2.mark_read("+8801711234567"));
        assert_eq!(s2.unread(), 1);
        assert!(s2.delete(1));
        assert!(!s2.delete(1));
        // cap
        for _ in 0..MAX_SMS + 10 {
            s2.add(mk("999", false));
        }
        assert_eq!(s2.all().len(), MAX_SMS);
        // a damaged file starts empty instead of failing
        std::fs::write(&p, "{not json").unwrap();
        assert!(SmsStore::load(p).all().is_empty());
    }

    #[test]
    fn contacts_save_lookup_delete_import() {
        let d = tmp("contacts");
        let mut c = ContactStore::load(d.join("contacts.json"));
        let id = c.save(None, "Zara", vec!["01711234567".into()]).unwrap();
        assert!(c.save(Some("nope"), "x", vec![]).is_none());
        c.save(None, "amir", vec!["+8801811000000".into()]).unwrap();
        assert_eq!(c.all()[0].name, "amir", "sorted case-insensitively");
        assert_eq!(c.name_of("+8801711234567"), Some("Zara"));
        assert_eq!(c.name_of("01999999999"), None);
        c.save(Some(&id), "Zara K", vec!["01711234567".into()])
            .unwrap();
        assert_eq!(c.name_of("01711234567"), Some("Zara K"));
        let n = c.import(vec![
            ("Zara K".into(), vec!["+8801711234567".into()]),
            ("New".into(), vec!["01555000111".into()]),
        ]);
        assert_eq!(n, 1, "the duplicate is skipped");
        assert!(c.delete(&id));
        assert!(!c.delete(&id));
        // persisted
        let again = ContactStore::load(d.join("contacts.json"));
        assert_eq!(again.all().len(), 2);
    }

    #[test]
    fn vcards_are_parsed_loosely() {
        let v = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Rahim Uddin\r\nTEL;TYPE=CELL:+880 1711-234567\r\nTEL;CELL:01811000000\r\nEND:VCARD\r\n\
                 BEGIN:VCARD\r\nVERSION:2.1\r\nN:Begum;Salma;;;\r\nTEL:01555000111\r\nEND:VCARD\r\n\
                 BEGIN:VCARD\r\nFN:No Number\r\nEND:VCARD\r\n\
                 BEGIN:VCARD\r\nFN:Bad\r\nTEL:12\r\nEND:VCARD\r\n";
        let c = parse_vcards(v);
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].0, "Rahim Uddin");
        assert_eq!(c[0].1, ["+8801711234567", "01811000000"]);
        assert_eq!(c[1].0, "Salma Begum");
    }

    #[test]
    fn call_log_is_capped() {
        let d = tmp("calls");
        let mut l = CallLog::load(d.join("calls.json"));
        for i in 0..MAX_CALLS + 5 {
            l.add(CallRec {
                number: "999".into(),
                ts: i as u64,
                dur_s: 1,
                kind: "out".into(),
            });
        }
        assert_eq!(l.all().len(), MAX_CALLS);
        assert_eq!(l.all()[0].ts, 5);
        l.clear();
        assert!(CallLog::load(d.join("calls.json")).all().is_empty());
    }
}
