//! Screen state machine (parity: walpad-devd.py screen(), activity, cover, brightness command).
use super::Ms;
use serde::Deserialize;
use walpad_proto::{ScreenMode, ScreenState};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub dim_after_s: u32,
    pub dim_level: u32,
    pub off_after_s: u32,
    pub default_brightness: u32,
    pub lid_off: bool,
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            dim_after_s: 90,
            dim_level: 20,
            off_after_s: 240,
            default_brightness: 120,
            lid_off: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Backlight(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brightness {
    Absolute(i32),
    Delta(i32),
}

/// What kind of input happened (rule 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activity {
    /// Touch EV_KEY/EV_ABS or a key EV_KEY: counts as activity and can wake the screen.
    Wakes,
    /// Key-device EV_ABS: counts as activity but never wakes.
    NoWake,
}

#[derive(Debug)]
pub struct Screen {
    cfg: Cfg,
    state: ScreenState,
    brightness: u32,
    saved: u32,
    last_act: Ms,
    lid_closed: bool,
}

impl Screen {
    /// Rule 5: startup brightness = current backlight, or default if it reads 0 or fails.
    pub fn new(cfg: Cfg, backlight: Option<u32>, now: Ms) -> Self {
        let b = match backlight {
            Some(v) if v > 0 => v.min(255),
            _ => cfg.default_brightness,
        };
        Screen {
            cfg,
            state: ScreenState::On,
            brightness: b,
            saved: b,
            last_act: now,
            lid_closed: false,
        }
    }
    pub fn state(&self) -> ScreenState {
        self.state
    }
    pub fn brightness(&self) -> u32 {
        self.brightness
    }
    pub fn lid_closed(&self) -> bool {
        self.lid_closed
    }
    pub fn set_cfg(&mut self, cfg: Cfg) {
        self.cfg = cfg;
    }

    fn set_brightness(&mut self, v: i64, out: &mut Vec<Act>) {
        self.brightness = v.clamp(0, 255) as u32;
        out.push(Act::Backlight(self.brightness));
    }

    /// `screen on|off|toggle|dim` command (rules 1-2). Resets the idle timer.
    pub fn screen(&mut self, mode: ScreenMode, now: Ms) -> Vec<Act> {
        let mut out = Vec::new();
        let mode = match mode {
            ScreenMode::Toggle => {
                if self.state != ScreenState::Off {
                    ScreenMode::Off
                } else {
                    ScreenMode::On
                }
            }
            m => m,
        };
        match mode {
            ScreenMode::On => {
                if self.state != ScreenState::On {
                    self.set_brightness(self.saved as i64, &mut out);
                }
                self.state = ScreenState::On;
            }
            ScreenMode::Off => {
                if self.state != ScreenState::Off {
                    if self.state == ScreenState::On {
                        self.saved = self.leave_value();
                    }
                    self.set_brightness(0, &mut out);
                    self.state = ScreenState::Off;
                }
            }
            ScreenMode::Toggle => {}
        }
        self.last_act = now;
        out
    }

    fn dim(&mut self, now: Ms) -> Vec<Act> {
        let mut out = Vec::new();
        if self.state == ScreenState::On {
            self.saved = self.leave_value();
            self.set_brightness(self.cfg.dim_level as i64, &mut out);
            self.state = ScreenState::Dim;
        }
        self.last_act = now;
        out
    }

    /// Rule 2: leaving `on` saves the current brightness (or the default if it was 0).
    fn leave_value(&self) -> u32 {
        if self.brightness == 0 {
            self.cfg.default_brightness
        } else {
            self.brightness
        }
    }

    /// Rule 4: `brightness N|+N|-N`.
    pub fn brightness_cmd(&mut self, req: Brightness) -> Vec<Act> {
        let mut out = Vec::new();
        let v = match req {
            Brightness::Absolute(n) => n as i64,
            Brightness::Delta(d) => self.brightness as i64 + d as i64,
        };
        self.set_brightness(v, &mut out);
        self.saved = self.brightness;
        self.state = if v > 0 {
            ScreenState::On
        } else {
            ScreenState::Off
        };
        out
    }

    /// Cover sensor: only when lid_off is configured (value 1 = closed).
    pub fn on_cover(&mut self, closed: bool, now: Ms) -> Vec<Act> {
        if !self.cfg.lid_off {
            return Vec::new();
        }
        self.lid_closed = closed;
        self.screen(
            if closed {
                ScreenMode::Off
            } else {
                ScreenMode::On
            },
            now,
        )
    }

    /// Rule 3. Returns (actions, woke): `woke` means a key press woke the screen and must be swallowed.
    pub fn on_activity(&mut self, a: Activity, is_key: bool, now: Ms) -> (Vec<Act>, bool) {
        self.last_act = now;
        if a == Activity::Wakes && self.state != ScreenState::On && !self.lid_closed {
            let acts = self.screen(ScreenMode::On, now);
            return (acts, is_key);
        }
        (Vec::new(), false)
    }

    /// Rule 1: idle > dim_after_s -> dim; idle > off_after_s -> off. 0 disables a step.
    pub fn tick(&mut self, now: Ms) -> Vec<Act> {
        let idle_ms = now.saturating_sub(self.last_act);
        let mut out = Vec::new();
        let dim_ms = self.cfg.dim_after_s as u64 * 1000;
        let off_ms = self.cfg.off_after_s as u64 * 1000;
        if self.state == ScreenState::On && dim_ms > 0 && idle_ms > dim_ms {
            out.extend(self.dim(now));
        }
        // the dim step resets last_act in `screen()` terms, so evaluate off from the original idle time
        if matches!(self.state, ScreenState::On | ScreenState::Dim)
            && off_ms > 0
            && idle_ms > off_ms
        {
            out.extend(self.screen(ScreenMode::Off, now));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s() -> Screen {
        Screen::new(Cfg::default(), Some(150), 0)
    }

    #[test]
    fn rule5_startup_brightness() {
        assert_eq!(Screen::new(Cfg::default(), Some(150), 0).brightness(), 150);
        assert_eq!(Screen::new(Cfg::default(), Some(0), 0).brightness(), 120);
        assert_eq!(Screen::new(Cfg::default(), None, 0).brightness(), 120);
    }

    #[test]
    fn rule1_dim_then_off() {
        let mut sc = s();
        assert!(sc.tick(90_000).is_empty(), "strictly greater");
        assert_eq!(sc.tick(90_001), vec![Act::Backlight(20)]);
        assert_eq!(sc.state(), ScreenState::Dim);
        // idle timer restarts at the dim step (screen() resets last_act), off follows 240 s later
        assert!(sc.tick(90_001 + 240_000).is_empty());
        assert_eq!(sc.tick(90_001 + 240_001), vec![Act::Backlight(0)]);
        assert_eq!(sc.state(), ScreenState::Off);
    }

    #[test]
    fn rule1_zero_disables_steps() {
        let mut sc = Screen::new(
            Cfg {
                dim_after_s: 0,
                off_after_s: 0,
                ..Cfg::default()
            },
            Some(150),
            0,
        );
        assert!(sc.tick(10_000_000).is_empty());
        assert_eq!(sc.state(), ScreenState::On);
    }

    #[test]
    fn rule2_save_and_restore() {
        let mut sc = s();
        sc.screen(ScreenMode::Off, 1);
        assert_eq!(sc.brightness(), 0);
        assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(150)]);
        // leaving `on` with brightness 0 saves the default
        let mut sc = s();
        sc.brightness = 0;
        sc.screen(ScreenMode::Off, 1);
        assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(120)]);
    }

    #[test]
    fn toggle_flips_off_and_on() {
        let mut sc = s();
        sc.screen(ScreenMode::Toggle, 1);
        assert_eq!(sc.state(), ScreenState::Off);
        sc.screen(ScreenMode::Toggle, 2);
        assert_eq!(sc.state(), ScreenState::On);
    }

    #[test]
    fn rule3_activity_wakes_only_when_cover_open_and_swallows_key() {
        let mut sc = s();
        sc.screen(ScreenMode::Off, 1);
        let (acts, swallow) = sc.on_activity(Activity::Wakes, true, 10);
        assert_eq!(acts, vec![Act::Backlight(150)]);
        assert!(swallow);
        // touch wake does not swallow
        sc.screen(ScreenMode::Off, 20);
        assert!(!sc.on_activity(Activity::Wakes, false, 30).1);
        // key abs never wakes
        sc.screen(ScreenMode::Off, 40);
        let (acts, _) = sc.on_activity(Activity::NoWake, true, 50);
        assert!(acts.is_empty());
        assert_eq!(sc.state(), ScreenState::Off);
        // closed cover blocks wake
        sc.on_cover(true, 60);
        assert!(sc.on_activity(Activity::Wakes, false, 70).0.is_empty());
        assert_eq!(sc.state(), ScreenState::Off);
    }

    #[test]
    fn activity_resets_idle_timer() {
        let mut sc = s();
        sc.on_activity(Activity::Wakes, false, 80_000);
        assert!(sc.tick(100_000).is_empty());
    }

    #[test]
    fn rule4_brightness_commands() {
        let mut sc = s();
        assert_eq!(
            sc.brightness_cmd(Brightness::Delta(200)),
            vec![Act::Backlight(255)]
        );
        assert_eq!(
            sc.brightness_cmd(Brightness::Delta(-300)),
            vec![Act::Backlight(0)]
        );
        assert_eq!(sc.state(), ScreenState::Off);
        sc.brightness_cmd(Brightness::Absolute(90));
        assert_eq!((sc.state(), sc.brightness()), (ScreenState::On, 90));
        // saved value follows: off then on restores 90
        sc.screen(ScreenMode::Off, 1);
        assert_eq!(sc.screen(ScreenMode::On, 2), vec![Act::Backlight(90)]);
    }

    #[test]
    fn cover_only_when_lid_off() {
        let mut sc = Screen::new(
            Cfg {
                lid_off: false,
                ..Cfg::default()
            },
            Some(150),
            0,
        );
        assert!(sc.on_cover(true, 1).is_empty());
        assert_eq!(sc.state(), ScreenState::On);
        let mut sc = s();
        assert_eq!(sc.on_cover(true, 1), vec![Act::Backlight(0)]);
        assert!(sc.lid_closed());
        assert_eq!(sc.on_cover(false, 2), vec![Act::Backlight(150)]);
    }
}
