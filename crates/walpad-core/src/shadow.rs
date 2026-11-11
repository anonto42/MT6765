//! Shadow mode (M2.2): feed every input through the Brain and report what the Rust core WOULD do,
//! writing nothing. A thin wrapper that renders `Effect`s as `WOULD ...` lines.
use crate::brain::{Brain, Role};
use crate::logic::Ms;
use walpad_hw::battery::RawBattery;
use walpad_hw::evdev::InputEvent;

pub struct Shadow {
    brain: Brain,
}

fn lines(e: Vec<crate::brain::Effect>) -> Vec<String> {
    e.iter()
        .map(ToString::to_string)
        .filter(|s| !s.is_empty())
        .collect()
}

impl Shadow {
    pub fn new(devd: &str, keys: &str, touch: &str, backlight: Option<u32>, now: Ms) -> Self {
        Shadow {
            brain: Brain::new(devd, keys, touch, backlight, now),
        }
    }
    pub fn orientation(&self) -> u16 {
        self.brain.orientation()
    }
    pub fn on_event(&mut self, role: Role, ev: &InputEvent, now: Ms) -> Vec<String> {
        lines(self.brain.on_event(role, ev, now))
    }
    pub fn tick(
        &mut self,
        now: Ms,
        gsensor: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
        batt: &mut dyn FnMut() -> RawBattery,
    ) -> Vec<String> {
        lines(self.brain.tick(now, gsensor, batt))
    }
}

/// Replay a recorded stream: events are (role, event); time is the event timestamp. The loop ticks
/// every 40 ms between events like the live core. Returns (ms, line).
pub fn replay(
    sh: &mut Shadow,
    events: &[(Role, InputEvent)],
    until_ms: Ms,
    g: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
) -> Vec<(Ms, String)> {
    const STEP: Ms = 40;
    let mut out = Vec::new();
    let mut b = || RawBattery {
        capacity: Some(80),
        status: Some("Charging".into()),
        mv: Some(4000),
        temp_dc: None,
        ..Default::default()
    };
    let Some(first) = events.first() else {
        return out;
    };
    let mut t = first.1.ms() as Ms;
    for (role, ev) in events {
        let et = ev.ms() as Ms;
        while t + STEP <= et {
            t += STEP;
            out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
        }
        out.extend(sh.on_event(*role, ev, et).into_iter().map(|l| (et, l)));
        t = t.max(et);
    }
    while t < until_ms {
        t += STEP;
        out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use walpad_hw::evdev::{
        ABS_MT_POSITION_X, ABS_MT_POSITION_Y, EV_ABS, EV_KEY, EV_SW, EV_SYN, KEY_POWER,
        KEY_VOLUMEUP, SYN_REPORT,
    };

    fn ev(ms: i64, kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            sec: ms / 1000,
            usec: (ms % 1000) * 1000,
            kind,
            code,
            value,
        }
    }
    fn new() -> Shadow {
        Shadow::new("", "", "", Some(150), 0)
    }
    fn lns(v: Vec<(Ms, String)>) -> Vec<String> {
        v.into_iter().map(|(_, l)| l).collect()
    }
    fn no_g() -> impl FnMut() -> Option<(i32, i32, i32)> {
        || None
    }

    #[test]
    fn volume_single_tap_would_send_key_up() {
        let mut sh = new();
        let evs = vec![
            (Role::Key, ev(1000, EV_KEY, KEY_VOLUMEUP, 1)),
            (Role::Key, ev(1050, EV_KEY, KEY_VOLUMEUP, 0)),
        ];
        assert_eq!(
            lns(replay(&mut sh, &evs, 2000, &mut no_g())),
            vec!["WOULD action key:Up"]
        );
    }

    #[test]
    fn power_short_press_toggles_screen_and_long_opens_menu() {
        let mut sh = new();
        let evs = vec![
            (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
            (Role::Key, ev(1200, EV_KEY, KEY_POWER, 0)),
        ];
        assert_eq!(
            lns(replay(&mut sh, &evs, 1500, &mut no_g())),
            vec!["WOULD backlight 0"]
        );
        let mut sh = new();
        let evs = vec![
            (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
            (Role::Key, ev(2500, EV_KEY, KEY_POWER, 0)),
        ];
        assert_eq!(
            lns(replay(&mut sh, &evs, 3000, &mut no_g())),
            vec!["WOULD powermenu open"]
        );
    }

    #[test]
    fn touch_becomes_pointer_press_move_release() {
        let mut sh = new();
        let evs = vec![
            (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_X, 400)),
            (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_Y, 640)),
            (Role::Touch, ev(1000, EV_SYN, SYN_REPORT, 0)),
        ];
        assert_eq!(
            lns(replay(&mut sh, &evs, 1400, &mut no_g())),
            vec![
                "WOULD pointer move 399 639",
                "WOULD pointer press",
                "WOULD pointer release"
            ]
        );
    }

    #[test]
    fn idle_dims_then_touch_wakes() {
        let mut sh = new();
        let evs = vec![
            (Role::Touch, ev(1000, EV_ABS, ABS_MT_POSITION_X, 1)),
            (Role::Touch, ev(100_000, EV_ABS, ABS_MT_POSITION_X, 1)),
        ];
        let out = lns(replay(&mut sh, &evs, 100_100, &mut no_g()));
        assert!(out.contains(&"WOULD backlight 20".to_string()), "{out:?}");
        assert!(out.contains(&"WOULD backlight 150".to_string()), "{out:?}");
    }

    #[test]
    fn cover_close_turns_screen_off() {
        let mut sh = new();
        let evs = vec![(Role::Hall, ev(1000, EV_SW, 0, 1))];
        assert_eq!(
            lns(replay(&mut sh, &evs, 1100, &mut no_g())),
            vec!["WOULD cover closed", "WOULD backlight 0"]
        );
    }

    #[test]
    fn tilt_rotates_after_three_samples() {
        let mut sh = new();
        let mut g = || Some((4000, 100, 0));
        let evs = vec![(Role::Key, ev(0, EV_ABS, 0, 0))];
        let out = lns(replay(&mut sh, &evs, 1500, &mut g));
        assert!(out.contains(&"WOULD orientation 90".to_string()), "{out:?}");
        assert_eq!(sh.orientation(), 90);
    }
}
