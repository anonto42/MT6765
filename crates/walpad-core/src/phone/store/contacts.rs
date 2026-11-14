use super::{read_json, same_number, write_json, MAX_CONTACTS};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
