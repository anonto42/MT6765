//! Shadow mode (M2.2): feed every input through the logic modules and report what the Rust
//! core WOULD do, writing nothing. Pure (time and sensors are passed in) so fixtures can be replayed.
use crate::logic::{
    battery, key_gestures as kg, power_key as pk, rotation, screen_timeout as st, touch_map as tm,
    Ms,
};
use serde::de::DeserializeOwned;
use walpad_hw::battery::RawBattery;
use walpad_hw::evdev::{InputEvent, EV_ABS, EV_KEY, EV_SW, KEY_POWER};
use walpad_proto::ScreenMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Touch,
    Key,
    Hall,
}

const GSENSOR_EVERY: Ms = 400;
const BATTERY_EVERY: Ms = 10_000;
const GESTURE_TICK: Ms = 40;

pub struct Shadow {
    screen: st::Screen,
    rotation: rotation::Rotation,
    power: pk::PowerKey,
    gestures: kg::Gestures,
    touch: tm::TouchMap,
    guard: battery::Guard,
    batt_cfg: battery::Cfg,
    last_g: Option<Ms>,
    last_b: Option<Ms>,
    last_gest: Ms,
}

fn cfg<T: DeserializeOwned + Default>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_default()
}

impl Shadow {
    /// `devd`, `keys`, `touch` are the contents of the three config files (empty = defaults).
    pub fn new(devd: &str, keys: &str, touch: &str, backlight: Option<u32>, now: Ms) -> Self {
        Shadow {
            screen: st::Screen::new(cfg(devd), backlight, now),
            rotation: rotation::Rotation::new(cfg(devd)),
            power: pk::PowerKey::default(),
            gestures: kg::Gestures::new(cfg(keys)),
            touch: tm::TouchMap::new(cfg(touch)),
            guard: battery::Guard::default(),
            batt_cfg: cfg(devd),
            last_g: None,
            last_b: None,
            last_gest: now,
        }
    }

    pub fn orientation(&self) -> u16 {
        self.rotation.orientation()
    }

    fn screen_acts(a: Vec<st::Act>, out: &mut Vec<String>) {
        for st::Act::Backlight(v) in a {
            out.push(format!("WOULD backlight {v}"));
        }
    }
    fn rot_acts(a: Vec<rotation::Act>, out: &mut Vec<String>) {
        for x in a {
            match x {
                rotation::Act::WriteOrientation(o) => out.push(format!("WOULD orientation {o}")),
                rotation::Act::SetStruts(o) => out.push(format!("WOULD struts {o}")),
                rotation::Act::DockKeyboard(o) => out.push(format!("WOULD keyboard-dock {o}")),
            }
        }
    }
    fn actions(a: Vec<kg::Action>, out: &mut Vec<String>) {
        for x in a {
            out.push(match x {
                kg::Action::None => continue,
                kg::Action::Key(k) => format!("WOULD action key:{k}"),
                kg::Action::Ctl(c) => format!("WOULD action ctl:{c}"),
                kg::Action::Launch(a) => format!("WOULD action launch:{a}"),
                kg::Action::Run(c) => format!("WOULD action run:{c} (rejected under Rust)"),
                kg::Action::Home => "WOULD action home".into(),
                kg::Action::Keyboard => "WOULD action keyboard".into(),
                kg::Action::PowerMenu => "WOULD action powermenu".into(),
                kg::Action::Screenshot => "WOULD action screenshot".into(),
                kg::Action::Volume(v) => format!("WOULD action volume:{v:+}"),
                kg::Action::Unknown(s) => format!("WOULD action unknown:{s}"),
            });
        }
    }
    fn touch_acts(a: Vec<tm::Act>, out: &mut Vec<String>) {
        for x in a {
            out.push(match x {
                tm::Act::Move(x, y) => format!("WOULD pointer move {x} {y}"),
                tm::Act::Press => "WOULD pointer press".into(),
                tm::Act::Release => "WOULD pointer release".into(),
            });
        }
    }
    fn power_acts(&mut self, a: Vec<pk::Act>, now: Ms, out: &mut Vec<String>) {
        for x in a {
            match x {
                pk::Act::ToggleScreen => {
                    Self::screen_acts(self.screen.screen(ScreenMode::Toggle, now), out)
                }
                pk::Act::WakeScreen => {
                    Self::screen_acts(self.screen.screen(ScreenMode::On, now), out)
                }
                pk::Act::ActivateMenuItem => out.push("WOULD powermenu activate".into()),
                pk::Act::OpenPowerMenu => out.push("WOULD powermenu open".into()),
            }
        }
    }

    /// Route one input event exactly as walpad-devd/-keys/-touchd do.
    pub fn on_event(&mut self, role: Role, ev: &InputEvent, now: Ms) -> Vec<String> {
        let mut out = Vec::new();
        match role {
            Role::Hall if ev.kind == EV_SW => {
                let closed = ev.value != 0;
                let acts = self.screen.on_cover(closed, now);
                if !acts.is_empty() || self.screen.lid_closed() == closed {
                    out.push(format!(
                        "WOULD cover {}",
                        if closed { "closed" } else { "open" }
                    ));
                }
                Self::screen_acts(acts, &mut out);
            }
            Role::Key if ev.kind == EV_KEY && ev.code == KEY_POWER => {
                let a = self.power.on_key(ev.value, now, false);
                self.power_acts(a, now, &mut out);
            }
            Role::Key | Role::Touch if ev.kind == EV_KEY || ev.kind == EV_ABS => {
                let is_key = role == Role::Key;
                let kind = if is_key && ev.kind == EV_ABS {
                    st::Activity::NoWake
                } else {
                    st::Activity::Wakes
                };
                let (acts, woke) = self.screen.on_activity(kind, is_key, now);
                Self::screen_acts(acts, &mut out);
                if woke {
                    self.gestures.note_wake(now);
                }
                if is_key {
                    if ev.kind == EV_KEY {
                        Self::actions(self.gestures.on_key(ev.code, ev.value, now), &mut out);
                    }
                } else {
                    let o = self.rotation.orientation();
                    Self::touch_acts(self.touch.on_event(ev, now, o), &mut out);
                }
            }
            Role::Touch => {
                let o = self.rotation.orientation();
                Self::touch_acts(self.touch.on_event(ev, now, o), &mut out);
            }
            _ => {}
        }
        out
    }

    /// Timers: call at least every 40 ms while shadowing.
    pub fn tick(
        &mut self,
        now: Ms,
        gsensor: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
        batt: &mut dyn FnMut() -> RawBattery,
    ) -> Vec<String> {
        let mut out = Vec::new();
        let a = self.power.tick(
            now,
            false,
            self.screen.state() == walpad_proto::ScreenState::On,
        );
        self.power_acts(a, now, &mut out);
        Self::touch_acts(self.touch.tick(now), &mut out);
        if now.saturating_sub(self.last_gest) >= GESTURE_TICK {
            self.last_gest = now;
            Self::actions(self.gestures.tick(now), &mut out);
        }
        Self::screen_acts(self.screen.tick(now), &mut out);
        if self
            .last_g
            .is_none_or(|t| now.saturating_sub(t) >= GSENSOR_EVERY)
        {
            self.last_g = Some(now);
            if self
                .rotation
                .wants_sample(self.screen.state() == walpad_proto::ScreenState::Off)
            {
                let g = gsensor();
                Self::rot_acts(self.rotation.sample(g), &mut out);
            }
        }
        if self
            .last_b
            .is_none_or(|t| now.saturating_sub(t) >= BATTERY_EVERY)
        {
            self.last_b = Some(now);
            for a in self.guard.step(&batt(), &self.batt_cfg) {
                match a {
                    battery::Act::CriticalPoweroff { mv } => {
                        out.push(format!("WOULD poweroff (critical battery {mv} mV)"))
                    }
                    battery::Act::LowChanged(l) => out.push(format!("WOULD low-battery {l}")),
                }
            }
        }
        out
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
    let mut out = Vec::new();
    let mut b = || RawBattery {
        capacity: Some(80),
        status: Some("Charging".into()),
        mv: Some(4000),
        temp_dc: None,
    };
    let Some(first) = events.first() else {
        return out;
    };
    let mut t = first.1.ms() as Ms;
    for (role, ev) in events {
        let et = ev.ms() as Ms;
        while t + GESTURE_TICK <= et {
            t += GESTURE_TICK;
            out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
        }
        out.extend(sh.on_event(*role, ev, et).into_iter().map(|l| (et, l)));
        t = t.max(et);
    }
    while t < until_ms {
        t += GESTURE_TICK;
        out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use walpad_hw::evdev::{
        ABS_MT_POSITION_X, ABS_MT_POSITION_Y, EV_SYN, KEY_VOLUMEUP, SYN_REPORT,
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
    fn lines(v: Vec<(Ms, String)>) -> Vec<String> {
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
        let out = lines(replay(&mut sh, &evs, 2000, &mut no_g()));
        assert_eq!(out, vec!["WOULD action key:Up"]);
    }

    #[test]
    fn power_short_press_toggles_screen_and_long_opens_menu() {
        let mut sh = new();
        let evs = vec![
            (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
            (Role::Key, ev(1200, EV_KEY, KEY_POWER, 0)),
        ];
        assert_eq!(
            lines(replay(&mut sh, &evs, 1500, &mut no_g())),
            vec!["WOULD backlight 0"]
        );
        let mut sh = new();
        let evs = vec![
            (Role::Key, ev(1000, EV_KEY, KEY_POWER, 1)),
            (Role::Key, ev(2500, EV_KEY, KEY_POWER, 0)),
        ];
        assert_eq!(
            lines(replay(&mut sh, &evs, 3000, &mut no_g())),
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
        let out = lines(replay(&mut sh, &evs, 1400, &mut no_g()));
        assert_eq!(
            out,
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
        let out = lines(replay(&mut sh, &evs, 100_100, &mut no_g()));
        assert!(out.contains(&"WOULD backlight 20".to_string()), "{out:?}");
        assert!(out.contains(&"WOULD backlight 150".to_string()), "{out:?}");
    }

    #[test]
    fn cover_close_turns_screen_off() {
        let mut sh = new();
        let evs = vec![(Role::Hall, ev(1000, EV_SW, 0, 1))];
        assert_eq!(
            lines(replay(&mut sh, &evs, 1100, &mut no_g())),
            vec!["WOULD cover closed", "WOULD backlight 0"]
        );
    }

    #[test]
    fn tilt_rotates_after_three_samples() {
        let mut sh = new();
        let mut g = || Some((4000, 100, 0));
        let evs = vec![(Role::Key, ev(0, EV_ABS, 0, 0))];
        let out = lines(replay(&mut sh, &evs, 1500, &mut g));
        assert!(out.contains(&"WOULD orientation 90".to_string()), "{out:?}");
        assert_eq!(sh.orientation(), 90);
    }
}
