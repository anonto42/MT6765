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

const ACCDET_INPUT_NAME: &str = "mt63xx-accdet Headset";

/// Hardware-level 3.5 mm jack state when a detector (`mt6357-accdet` evdev or switch/extcon)
/// is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JackState {
    Empty,
    Headphones,
    HeadsetWithMic,
}

fn read_accdet_evdev(root: &Root) -> Option<JackState> {
    let entries = std::fs::read_dir(root.p("/sys/class/input")).ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        let Ok(name) = std::fs::read_to_string(dir.join("name")) else {
            continue;
        };
        if name.trim() != ACCDET_INPUT_NAME {
            continue;
        }
        let Ok(sub) = std::fs::read_dir(&dir) else {
            continue;
        };
        let Some(event_name) = sub.flatten().find_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("event").then_some(n)
        }) else {
            continue;
        };
        let dev_path = root.p("/dev/input").join(event_name);
        let Ok(bits) = crate::evdev::read_sw_bitmap(&dev_path) else {
            continue;
        };
        if crate::evdev::key_bit(&bits, crate::evdev::SW_MICROPHONE_INSERT) {
            return Some(JackState::HeadsetWithMic);
        }
        if crate::evdev::key_bit(&bits, crate::evdev::SW_HEADPHONE_INSERT)
            || crate::evdev::key_bit(&bits, crate::evdev::SW_LINEOUT_INSERT)
        {
            return Some(JackState::Headphones);
        }
        return Some(JackState::Empty);
    }
    None
}

/// `Some(state)` when a hardware jack detector is present (`mt63xx-accdet Headset` evdev node or
/// switch/extcon sysfs node); `None` when no detector is loaded on this boot.
pub fn read_jack_state(root: &Root) -> Option<JackState> {
    if let Some(st) = read_accdet_evdev(root) {
        return Some(st);
    }
    PRESENCE_NODES.iter().find_map(|n| {
        let s = std::fs::read_to_string(root.p(n)).ok()?;
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        Some(if s == "0" {
            JackState::Empty
        } else {
            JackState::Headphones
        })
    })
}

/// `Some(true)` = something is inserted, `Some(false)` = empty, `None` = no detector on this boot.
pub fn read_jack(root: &Root) -> Option<bool> {
    read_jack_state(root).map(|s| s != JackState::Empty)
}

#[cfg(test)]
mod tests;
