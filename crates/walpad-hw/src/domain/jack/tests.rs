use super::*;
use crate::testutil::{put, tmp};

#[test]
fn reads_accdet_evdev_switch_states() {
    let dir = std::env::temp_dir().join(format!("walpad-accdet-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = Root::new(&dir);
    std::fs::create_dir_all(r.p("/sys/class/input/input5/event5")).unwrap();
    std::fs::create_dir_all(r.p("/dev/input")).unwrap();
    std::fs::write(
        r.p("/sys/class/input/input5/name"),
        "mt63xx-accdet Headset\n",
    )
    .unwrap();

    // Empty
    std::fs::write(r.p("/dev/input/event5"), [0u8; 8]).unwrap();
    assert_eq!(read_jack_state(&r), Some(JackState::Empty));
    assert_eq!(read_jack(&r), Some(false));

    // SW_HEADPHONE_INSERT (bit 2 -> 0x04)
    let mut hp = [0u8; 8];
    hp[0] = 1 << crate::evdev::SW_HEADPHONE_INSERT;
    std::fs::write(r.p("/dev/input/event5"), hp).unwrap();
    assert_eq!(read_jack_state(&r), Some(JackState::Headphones));
    assert_eq!(read_jack(&r), Some(true));

    // SW_HEADPHONE_INSERT + SW_MICROPHONE_INSERT (bits 2 and 4 -> 0x14)
    let mut hs = [0u8; 8];
    hs[0] = (1 << crate::evdev::SW_HEADPHONE_INSERT) | (1 << crate::evdev::SW_MICROPHONE_INSERT);
    std::fs::write(r.p("/dev/input/event5"), hs).unwrap();
    assert_eq!(read_jack_state(&r), Some(JackState::HeadsetWithMic));
    assert_eq!(read_jack(&r), Some(true));

    let _ = std::fs::remove_dir_all(&dir);
}

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
