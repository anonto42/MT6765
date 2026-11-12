//! Headphone jack hardware read (owner request 2026-11-12, "the headphone jack have to work and
//! also the microphons ahve to work"). HONEST LIMIT, confirmed on the device (read-only): the
//! kernel config builds the MT6357 accdet driver only as a loadable module
//! (`CONFIG_SND_SOC_MT6357_ACCDET=m`) and it is present, prebuilt, at
//! `/vendor/lib/modules/mt6357-accdet.ko`, but it is not in `lsmod` on this boot image, so there
//! is no `/sys/class/switch`, no extcon node, no jack-related `/proc/bus/input/devices` entry and
//! no live jack-insert interrupt (see docs/45-headphone-mic.md). `Headphone Plugged In`
//! (an ALSA control the codec driver exposes) is a write-only software flag the vendor HAL used to
//! set by hand after its own accdet read; the kernel never updates it by itself, so it is not a
//! sensor either.
//!
//! `read_jack` therefore returns `None` right now and is expected to keep doing so until a future,
//! explicitly-approved change loads that module. It stays a real sysfs probe (and `Option<bool>`,
//! never guessed) so that change only has to add a node here, not touch any caller.
use crate::Root;

/// Switch/extcon nodes a loaded accdet module is known to create on other MediaTek boards; none
/// exist on this kernel build (verified: `find /sys/class/switch /sys/class/extcon` found
/// nothing). A plain switch can only report presence, not whether the headset has a mic, so a
/// `Some(true)` here can only ever become `JackKind::Headphones`; telling a mic-equipped headset
/// apart needs accdet's own classification (or the owner's manual override).
const PRESENCE_NODES: &[&str] = &[
    "/sys/class/switch/h2w/state",
    "/sys/class/extcon/extcon0/state",
];

/// `Some(true)` = something is inserted, `Some(false)` = empty, `None` = no detector on this boot.
pub fn read_jack(root: &Root) -> Option<bool> {
    PRESENCE_NODES.iter().find_map(|n| {
        let s = std::fs::read_to_string(root.p(n)).ok()?;
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        Some(s != "0")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{put, tmp};

    #[test]
    fn no_detector_is_none() {
        let r = Root::new(tmp());
        assert_eq!(read_jack(&r), None, "no accdet module loaded: never guess");
    }

    #[test]
    fn reads_a_switch_node_when_one_exists() {
        let r = Root::new(tmp());
        put(&r, "/sys/class/switch/h2w/state", "0\n");
        assert_eq!(read_jack(&r), Some(false));
        put(&r, "/sys/class/switch/h2w/state", "1\n");
        assert_eq!(read_jack(&r), Some(true));
    }

    #[test]
    fn falls_back_to_extcon() {
        let r = Root::new(tmp());
        put(&r, "/sys/class/extcon/extcon0/state", "1\n");
        assert_eq!(read_jack(&r), Some(true));
    }
}
