//! Input devices discovered by name (never by event number), opened read-only.
use crate::brain::Role;
use std::time::{Duration, Instant};
use walpad_hw::{
    evdev::{EvdevReader, InputEvent},
    input, Root,
};

pub struct Inputs {
    readers: Vec<(Role, EvdevReader)>,
    /// Nodes already opened or given up on (logged once), so re-discovery only adds late devices.
    seen: Vec<std::path::PathBuf>,
    /// Nodes whose failure was already logged (retries stay quiet).
    warned: Vec<std::path::PathBuf>,
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
            seen: Vec::new(),
            warned: Vec::new(),
            next_discover: Instant::now(),
            log_prefix,
        };
        i.discover(root);
        i
    }

    fn discover(&mut self, root: &Root) {
        self.next_discover = Instant::now() + Duration::from_secs(5);
        for d in input::discover(root) {
            let Some(role) = role_of(&d.name) else {
                continue;
            };
            // devices such as the touch controller register late: only add the ones not yet handled
            if self.seen.contains(&d.node) {
                continue;
            }
            if let Err(e) = input::ensure_node(&d) {
                self.warn(&d.node, format!("{}: {e}", d.node.display()));
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
                    self.seen.push(d.node.clone());
                    self.readers.push((role, r));
                }
                Err(e) => self.warn(&d.node, format!("cannot open {}: {e}", d.node.display())),
            }
        }
    }

    fn warn(&mut self, node: &std::path::Path, msg: String) {
        if !self.warned.iter().any(|n| n == node) {
            self.warned.push(node.to_path_buf());
            eprintln!("{}: {msg}", self.log_prefix);
        }
    }

    pub fn device_count(&self) -> usize {
        self.readers.len()
    }

    /// All pending events from every device, ordered by kernel timestamp. Re-discovers every 5 s
    /// (new devices only) while fewer than the four expected devices are open.
    pub fn poll(&mut self, root: &Root) -> Vec<(Role, InputEvent)> {
        if self.readers.len() < 4 && Instant::now() >= self.next_discover {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn put(root: &Root, path: &str, body: &str) {
        let p = root.p(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    /// The touch controller registers a few seconds after the keys; it must still be picked up
    /// (the first release only retried while nothing was open, so touch never worked).
    #[test]
    fn late_touch_device_is_picked_up() {
        let dir = std::env::temp_dir().join(format!("walpad-inputs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = Root::new(&dir);
        put(&root, "/sys/class/input/input1/name", "mtk-kpd\n");
        put(&root, "/sys/class/input/input1/event1/dev", "13:65\n");
        put(&root, "/dev/input/event1", "");
        let mut i = Inputs::new(&root, "test");
        assert_eq!(i.device_count(), 1);
        put(&root, "/sys/class/input/input7/name", "mtk-tpd\n");
        put(&root, "/sys/class/input/input7/event4/dev", "13:68\n");
        put(&root, "/dev/input/event4", "");
        i.next_discover = Instant::now();
        i.poll(&root);
        assert_eq!(i.device_count(), 2);
        assert!(i.readers.iter().any(|(r, _)| *r == Role::Touch));
        i.next_discover = Instant::now();
        i.poll(&root);
        assert_eq!(i.device_count(), 2, "no duplicate readers");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
