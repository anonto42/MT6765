//! Live shadow runner: opens the real input devices read-only (never grabs), feeds `Shadow`,
//! and prints `<epoch_ms> WOULD ...` lines. Writes nothing to the hardware.
use crate::inputs::Inputs;
use crate::shadow::Shadow;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use walpad_hw::{battery::read_battery, gsensor::read_gsensor, sysfs, Root};

pub struct Live {
    shadow: Shadow,
    inputs: Inputs,
    started: Instant,
}

fn read(root: &Root, p: &str) -> String {
    std::fs::read_to_string(root.p(p)).unwrap_or_default()
}

impl Live {
    pub fn new(root: &Root) -> Self {
        let shadow = Shadow::new(
            &read(root, "/etc/walpad/devd.json"),
            &read(root, "/etc/walpad/keys.json"),
            &read(root, "/etc/walpad/touch.json"),
            sysfs::read_backlight(root),
            0,
        );
        Live {
            shadow,
            inputs: Inputs::new(root, "walpad-core[shadow]"),
            started: Instant::now(),
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
        let now = self.started.elapsed().as_millis() as u64;
        for (role, ev) in self.inputs.poll(root) {
            Self::emit(self.shadow.on_event(role, &ev, now));
        }
        let mut g = || read_gsensor(root);
        let mut b = || read_battery(root);
        Self::emit(self.shadow.tick(now, &mut g, &mut b));
    }
}
