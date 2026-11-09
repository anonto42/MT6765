//! Live shadow runner: opens the real input devices read-only (never grabs), feeds `Shadow`,
//! and prints `<epoch_ms> WOULD ...` lines. Writes nothing to the hardware.
use crate::shadow::{Role, Shadow};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walpad_hw::{
    battery::read_battery, evdev::EvdevReader, gsensor::read_gsensor, input, sysfs, Root,
};

pub struct Live {
    shadow: Shadow,
    readers: Vec<(Role, EvdevReader)>,
    started: Instant,
    next_discover: Instant,
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

fn read(root: &Root, p: &str) -> String {
    std::fs::read_to_string(root.p(p)).unwrap_or_default()
}

impl Live {
    pub fn new(root: &Root) -> Self {
        let now = Instant::now();
        let shadow = Shadow::new(
            &read(root, "/etc/walpad/devd.json"),
            &read(root, "/etc/walpad/keys.json"),
            &read(root, "/etc/walpad/touch.json"),
            sysfs::read_backlight(root),
            0,
        );
        let mut l = Live {
            shadow,
            readers: Vec::new(),
            started: now,
            next_discover: now,
        };
        l.discover(root);
        l
    }

    fn discover(&mut self, root: &Root) {
        self.next_discover = Instant::now() + std::time::Duration::from_secs(5);
        if !self.readers.is_empty() {
            return;
        }
        for d in input::discover(root) {
            let Some(role) = role_of(&d.name) else {
                continue;
            };
            if let Err(e) = input::ensure_node(&d) {
                eprintln!("walpad-core[shadow]: {}: {e}", d.node.display());
                continue;
            }
            match EvdevReader::open(&d.node) {
                Ok(r) => {
                    eprintln!(
                        "walpad-core[shadow]: reading {} ({}) as {role:?}",
                        d.node.display(),
                        d.name
                    );
                    self.readers.push((role, r));
                }
                Err(e) => eprintln!("walpad-core[shadow]: cannot open {}: {e}", d.node.display()),
            }
        }
    }

    fn emit(lines: Vec<String>) {
        if lines.is_empty() {
            return;
        }
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        for l in lines {
            eprintln!("{ms} {l}");
        }
    }

    /// Call every <= 40 ms.
    pub fn pump(&mut self, root: &Root) {
        if self.readers.is_empty() && Instant::now() >= self.next_discover {
            self.discover(root);
        }
        let now = self.started.elapsed().as_millis() as u64;
        let mut batch = Vec::new();
        for (role, r) in &mut self.readers {
            match r.read_available() {
                Ok(evs) => batch.extend(evs.into_iter().map(|e| (*role, e))),
                Err(e) => eprintln!("walpad-core[shadow]: read error: {e}"),
            }
        }
        // events are processed in kernel-timestamp order across devices
        batch.sort_by_key(|(_, e)| e.ms());
        for (role, ev) in batch {
            Self::emit(self.shadow.on_event(role, &ev, now));
        }
        let mut g = || read_gsensor(root);
        let mut b = || read_battery(root);
        Self::emit(self.shadow.tick(now, &mut g, &mut b));
    }
}
