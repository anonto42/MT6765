//! The decision engine shared by shadow mode (effects are printed) and live mode (effects are
//! executed). Pure: time and sensor readings are passed in; nothing here touches the system.
use crate::logic::{
    battery, gesture, key_gestures as kg, power_key as pk, rotation, screen_timeout as st,
    touch_map as tm, Ms,
};
use serde::de::DeserializeOwned;
use walpad_hw::battery::RawBattery;
use walpad_hw::evdev::{
    InputEvent, EV_ABS, EV_KEY, EV_SW, KEY_POWER, KEY_VOLUMEDOWN, KEY_VOLUMEUP,
};
use walpad_proto::{ScreenMode, ScreenState, ShellInput};

mod effect;
#[cfg(test)]
mod tests;

pub use effect::{Effect, Role};

const GSENSOR_EVERY: Ms = 400;
const BATTERY_EVERY: Ms = 10_000;
const GESTURE_TICK: Ms = 40;
/// A finger is down or a fling runs: tick faster (long press, fling smoothness).
const TOUCH_TICK: Ms = 16;

pub struct Brain {
    pub screen: st::Screen,
    pub rotation: rotation::Rotation,
    power: pk::PowerKey,
    gestures: kg::Gestures,
    touch: tm::TouchMap,
    gest: gesture::Gestures,
    pub guard: battery::Guard,
    pub batt_cfg: battery::Cfg,
    last_g: Option<Ms>,
    last_b: Option<Ms>,
    last_gest: Ms,
    /// Set by the caller from /run/walpad/powermenu-open before feeding events.
    pub menu_open: bool,
    /// Set by the caller while the shell's power menu is open: volume keys go to the shell as
    /// `Effect::ShellKey` and never reach the volume gestures.
    pub shell_keys: bool,
}

fn cfg<T: DeserializeOwned + Default>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_default()
}

impl Brain {
    /// `devd`, `keys`, `touch` are the contents of the three config files (empty = defaults).
    pub fn new(devd: &str, keys: &str, touch: &str, backlight: Option<u32>, now: Ms) -> Self {
        let tcfg: tm::Cfg = cfg(touch);
        Brain {
            gest: gesture::Gestures::new(tcfg.gestures.clone()),
            screen: st::Screen::new(cfg(devd), backlight, now),
            rotation: rotation::Rotation::new(cfg(devd)),
            power: pk::PowerKey::default(),
            gestures: kg::Gestures::new(cfg(keys)),
            touch: tm::TouchMap::new(tcfg),
            guard: battery::Guard::default(),
            batt_cfg: cfg(devd),
            last_g: None,
            last_b: None,
            last_gest: now,
            menu_open: false,
            shell_keys: false,
        }
    }

    pub fn reload(&mut self, devd: &str, keys: &str, touch: &str) {
        self.screen.set_cfg(cfg(devd));
        self.rotation.set_cfg(cfg(devd));
        self.batt_cfg = cfg(devd);
        self.guard.invalidate_charger();
        self.gestures.set_cfg(cfg(keys));
        let tcfg: tm::Cfg = cfg(touch);
        self.gest = gesture::Gestures::new(tcfg.gestures.clone());
        self.touch = tm::TouchMap::new(tcfg);
    }

    pub fn orientation(&self) -> u16 {
        self.rotation.orientation()
    }

    pub fn screen_effects(a: Vec<st::Act>, out: &mut Vec<Effect>) {
        out.extend(
            a.into_iter()
                .map(|st::Act::Backlight(v)| Effect::Backlight(v)),
        );
    }
    pub fn rotation_effects(a: Vec<rotation::Act>, out: &mut Vec<Effect>) {
        for x in a {
            out.push(match x {
                rotation::Act::WriteOrientation(o) => Effect::Orientation(o),
                rotation::Act::SetStruts(o) => Effect::Struts(o),
                rotation::Act::DockKeyboard(o) => Effect::KeyboardDock(o),
            });
        }
    }
    fn actions(a: Vec<kg::Action>, out: &mut Vec<Effect>) {
        out.extend(
            a.into_iter()
                .filter(|x| *x != kg::Action::None)
                .map(Effect::Action),
        );
    }
    /// Finger actions -> gestures -> pointer effects. `probe` classifies a touch at its start.
    fn touch_effects(
        &mut self,
        a: Vec<tm::Act>,
        now: Ms,
        probe: &mut dyn FnMut(i32, i32) -> gesture::Mode,
        out: &mut Vec<Effect>,
    ) {
        for act in a {
            let g = self.gest.on_act(act, now, probe);
            Self::gesture_effects(g, out);
        }
    }
    fn gesture_effects(g: Vec<gesture::Out>, out: &mut Vec<Effect>) {
        out.extend(g.into_iter().map(|o| match o {
            gesture::Out::Move(x, y) => Effect::Pointer(tm::Act::Move(x, y)),
            gesture::Out::Button(1, true) => Effect::Pointer(tm::Act::Press),
            gesture::Out::Button(1, false) => Effect::Pointer(tm::Act::Release),
            gesture::Out::Button(n, d) => Effect::Button(n, d),
            gesture::Out::Wheel { button, ctrl } => Effect::Wheel { button, ctrl },
        }));
    }
    fn power_effects(&mut self, a: Vec<pk::Act>, now: Ms, out: &mut Vec<Effect>) {
        for x in a {
            match x {
                pk::Act::ToggleScreen => {
                    Self::screen_effects(self.screen.screen(ScreenMode::Toggle, now), out);
                    if self.screen.state() == ScreenState::Off {
                        out.push(Effect::LockNow);
                    }
                }
                pk::Act::WakeScreen => {
                    Self::screen_effects(self.screen.screen(ScreenMode::On, now), out)
                }
                pk::Act::ActivateMenuItem => out.push(Effect::PowerMenuActivate),
                pk::Act::OpenPowerMenu => out.push(Effect::PowerMenuOpen),
            }
        }
    }

    /// Route one input event exactly as walpad-devd/-keys/-touchd do.
    pub fn on_event(&mut self, role: Role, ev: &InputEvent, now: Ms) -> Vec<Effect> {
        self.on_event_probe(role, ev, now, &mut |_, _| gesture::Mode::Mouse)
    }

    /// `on_event` with the touch classifier: `probe(x, y)` says whether the touch that starts at
    /// this logical point is a mouse touch (shell windows, apps with `touch: mouse`) or a
    /// touchscreen touch (scroll, long press).
    pub fn on_event_probe(
        &mut self,
        role: Role,
        ev: &InputEvent,
        now: Ms,
        probe: &mut dyn FnMut(i32, i32) -> gesture::Mode,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        match role {
            Role::Hall if ev.kind == EV_SW => {
                let closed = ev.value != 0;
                let acts = self.screen.on_cover(closed, now);
                if !acts.is_empty() || self.screen.lid_closed() == closed {
                    out.push(Effect::Cover(closed));
                }
                Self::screen_effects(acts, &mut out);
            }
            Role::Key if ev.kind == EV_KEY && ev.code == KEY_POWER => {
                let a = self.power.on_key(ev.value, now, self.menu_open);
                self.power_effects(a, now, &mut out);
            }
            Role::Key | Role::Touch if ev.kind == EV_KEY || ev.kind == EV_ABS => {
                let is_key = role == Role::Key;
                let kind = if is_key && ev.kind == EV_ABS {
                    st::Activity::NoWake
                } else {
                    st::Activity::Wakes
                };
                let (acts, woke) = self.screen.on_activity(kind, is_key, now);
                Self::screen_effects(acts, &mut out);
                if woke {
                    self.gestures.note_wake(now);
                }
                if is_key {
                    let vol = match ev.code {
                        KEY_VOLUMEUP => Some(ShellInput::VolumeUp),
                        KEY_VOLUMEDOWN => Some(ShellInput::VolumeDown),
                        _ => None,
                    };
                    if let (true, Some(k), true) = (self.shell_keys, vol, ev.kind == EV_KEY) {
                        if ev.value == 1 {
                            out.push(Effect::ShellKey(k));
                        }
                    } else if ev.kind == EV_KEY {
                        Self::actions(self.gestures.on_key(ev.code, ev.value, now), &mut out);
                    }
                } else {
                    let o = self.rotation.orientation();
                    let a = self.touch.on_event(ev, now, o);
                    self.touch_effects(a, now, probe, &mut out);
                }
            }
            Role::Touch => {
                let o = self.rotation.orientation();
                let a = self.touch.on_event(ev, now, o);
                self.touch_effects(a, now, probe, &mut out);
            }
            _ => {}
        }
        out
    }

    /// Timers: call at least every 40 ms.
    pub fn tick(
        &mut self,
        now: Ms,
        gsensor: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
        batt: &mut dyn FnMut() -> RawBattery,
    ) -> Vec<Effect> {
        let mut out = Vec::new();
        let a = self
            .power
            .tick(now, self.menu_open, self.screen.state() == ScreenState::On);
        self.power_effects(a, now, &mut out);
        // the lift first (a tap clicks), then the long-press and fling timers
        let lift = self.touch.tick(now);
        self.touch_effects(lift, now, &mut |_, _| gesture::Mode::Mouse, &mut out);
        let g = self.gest.tick(now);
        Self::gesture_effects(g, &mut out);
        if now.saturating_sub(self.last_gest) >= GESTURE_TICK {
            self.last_gest = now;
            Self::actions(self.gestures.tick(now), &mut out);
        }
        Self::screen_effects(self.screen.tick(now), &mut out);
        if self
            .last_g
            .is_none_or(|t| now.saturating_sub(t) >= GSENSOR_EVERY)
        {
            self.last_g = Some(now);
            if self
                .rotation
                .wants_sample(self.screen.state() == ScreenState::Off)
            {
                let g = gsensor();
                Self::rotation_effects(self.rotation.sample(g), &mut out);
            }
        }
        if self
            .last_b
            .is_none_or(|t| now.saturating_sub(t) >= BATTERY_EVERY)
        {
            self.last_b = Some(now);
            for a in self.guard.step(&batt(), &self.batt_cfg) {
                out.push(match a {
                    battery::Act::CriticalPoweroff { mv } => Effect::CriticalPoweroff(mv),
                    battery::Act::LowChanged(l) => Effect::LowBattery(l),
                });
            }
        }
        out
    }

    /// How long the caller may sleep before the next tick is needed (power key down = faster).
    pub fn poll_interval_ms(&self) -> Ms {
        let t = self.power.poll_interval_ms().min(GESTURE_TICK);
        if self.gest.active() {
            t.min(TOUCH_TICK)
        } else if self.screen.state() == ScreenState::Off && t >= GESTURE_TICK {
            1000
        } else {
            t
        }
    }
}
