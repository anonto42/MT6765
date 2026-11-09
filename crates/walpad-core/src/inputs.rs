//! Input devices discovered by name (never by event number), opened read-only.
use crate::brain::Role;
use std::time::{Duration, Instant};
use walpad_hw::{
    evdev::{EvdevReader, InputEvent},
    input, Root,
};

pub struct Inputs {
    readers: Vec<(Role, EvdevReader)>,
    next_discover: Instant,
    pub log_prefix: &'static str,
}

fn role_of(name: &str) -> Option<Role> {
    if input::is_touch(name) {
        Some(Role::Touch)
    } else if name == "mtk-kpd" || name == "mtk-pmic-keys" {
        Some(Role::Key)
    } else if name == "HALL_DEV" {
        Some(Role::Hall)
    } else {
        None
    }
}

impl Inputs {
    pub fn new(root: &Root, log_prefix: &'static str) -> Self {
        let mut i = Inputs {
            readers: Vec::new(),
            next_discover: Instant::now(),
            log_prefix,
        };
        i.discover(root);
        i
    }

    fn discover(&mut self, root: &Root) {
        self.next_discover = Instant::now() + Duration::from_secs(5);
        if !self.readers.is_empty() {
            return;
        }
        for d in input::discover(root) {
            let Some(role) = role_of(&d.name) else {
                continue;
            };
            if let Err(e) = input::ensure_node(&d) {
                eprintln!("{}: {}: {e}", self.log_prefix, d.node.display());
                continue;
            }
            match EvdevReader::open(&d.node) {
                Ok(r) => {
                    eprintln!(
                        "{}: reading {} ({}) as {role:?}",
                        self.log_prefix,
                        d.node.display(),
                        d.name
                    );
                    self.readers.push((role, r));
                }
                Err(e) => eprintln!("{}: cannot open {}: {e}", self.log_prefix, d.node.display()),
            }
        }
    }

    pub fn device_count(&self) -> usize {
        self.readers.len()
    }

    /// All pending events from every device, ordered by kernel timestamp. Re-discovers every 5 s
    /// while nothing is open.
    pub fn poll(&mut self, root: &Root) -> Vec<(Role, InputEvent)> {
        if self.readers.is_empty() && Instant::now() >= self.next_discover {
            self.discover(root);
        }
        let mut batch = Vec::new();
        for (role, r) in &mut self.readers {
            match r.read_available() {
                Ok(evs) => batch.extend(evs.into_iter().map(|e| (*role, e))),
                Err(e) => eprintln!("{}: read error: {e}", self.log_prefix),
            }
        }
        batch.sort_by_key(|(_, e)| e.ms());
        batch
    }
}
