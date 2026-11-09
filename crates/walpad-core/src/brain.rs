//! The decision engine shared by shadow mode (effects are printed) and live mode (effects are
//! executed). Pure: time and sensor readings are passed in; nothing here touches the system.
use crate::logic::{
    battery, key_gestures as kg, power_key as pk, rotation, screen_timeout as st, touch_map as tm,
    Ms,
};
use serde::de::DeserializeOwned;
use std::fmt;
use walpad_hw::battery::RawBattery;
use walpad_hw::evdev::{InputEvent, EV_ABS, EV_KEY, EV_SW, KEY_POWER};
use walpad_proto::{ScreenMode, ScreenState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Touch,
    Key,
    Hall,
}

/// Something the core decided to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    // device subsystem
    Backlight(u32),
    Orientation(u16),
    Struts(u16),
    KeyboardDock(u16),
    PowerMenuOpen,
    PowerMenuActivate,
    Cover(bool),
    LowBattery(bool),
    CriticalPoweroff(u32),
    // input subsystem
    Pointer(tm::Act),
    Action(kg::Action),
}

impl Effect {
    pub fn is_device(&self) -> bool {
        !matches!(self, Effect::Pointer(_) | Effect::Action(_))
    }
}

/// The `WOULD ...` line used by shadow mode (format is what shadow_diff.sh parses).
impl fmt::Display for Effect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Effect::Backlight(v) => write!(f, "WOULD backlight {v}"),
            Effect::Orientation(o) => write!(f, "WOULD orientation {o}"),
            Effect::Struts(o) => write!(f, "WOULD struts {o}"),
            Effect::KeyboardDock(o) => write!(f, "WOULD keyboard-dock {o}"),
            Effect::PowerMenuOpen => write!(f, "WOULD powermenu open"),
            Effect::PowerMenuActivate => write!(f, "WOULD powermenu activate"),
            Effect::Cover(c) => write!(f, "WOULD cover {}", if *c { "closed" } else { "open" }),
            Effect::LowBattery(l) => write!(f, "WOULD low-battery {l}"),
            Effect::CriticalPoweroff(mv) => write!(f, "WOULD poweroff (critical battery {mv} mV)"),
            Effect::Pointer(tm::Act::Move(x, y)) => write!(f, "WOULD pointer move {x} {y}"),
            Effect::Pointer(tm::Act::Press) => write!(f, "WOULD pointer press"),
            Effect::Pointer(tm::Act::Release) => write!(f, "WOULD pointer release"),
            Effect::Action(a) => match a {
                kg::Action::None => Ok(()),
                kg::Action::Key(k) => write!(f, "WOULD action key:{k}"),
                kg::Action::Ctl(c) => write!(f, "WOULD action ctl:{c}"),
                kg::Action::Launch(a) => write!(f, "WOULD action launch:{a}"),
                kg::Action::Run(c) => write!(f, "WOULD action run:{c} (rejected under Rust)"),
                kg::Action::Home => write!(f, "WOULD action home"),
                kg::Action::Keyboard => write!(f, "WOULD action keyboard"),
                kg::Action::PowerMenu => write!(f, "WOULD action powermenu"),
                kg::Action::Screenshot => write!(f, "WOULD action screenshot"),
                kg::Action::Volume(v) => write!(f, "WOULD action volume:{v:+}"),
                kg::Action::Unknown(s) => write!(f, "WOULD action unknown:{s}"),
            },
        }
    }
}

const GSENSOR_EVERY: Ms = 400;
const BATTERY_EVERY: Ms = 10_000;
const GESTURE_TICK: Ms = 40;

pub struct Brain {
    pub screen: st::Screen,
    pub rotation: rotation::Rotation,
    power: pk::PowerKey,
    gestures: kg::Gestures,
    touch: tm::TouchMap,
    pub guard: battery::Guard,
    pub batt_cfg: battery::Cfg,
    last_g: Option<Ms>,
    last_b: Option<Ms>,
    last_gest: Ms,
    /// Set by the caller from /run/walpad/powermenu-open before feeding events.
    pub menu_open: bool,
}

fn cfg<T: DeserializeOwned + Default>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_default()
}

impl Brain {
    /// `devd`, `keys`, `touch` are the contents of the three config files (empty = defaults).
    pub fn new(devd: &str, keys: &str, touch: &str, backlight: Option<u32>, now: Ms) -> Self {
        Brain {
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
            menu_open: false,
        }
    }

    pub fn reload(&mut self, devd: &str, keys: &str, touch: &str) {
        self.screen.set_cfg(cfg(devd));
        self.rotation.set_cfg(cfg(devd));
        self.batt_cfg = cfg(devd);
        self.gestures.set_cfg(cfg(keys));
        self.touch = tm::TouchMap::new(cfg(touch));
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
    fn touch_effects(a: Vec<tm::Act>, out: &mut Vec<Effect>) {
        out.extend(a.into_iter().map(Effect::Pointer));
    }
    fn power_effects(&mut self, a: Vec<pk::Act>, now: Ms, out: &mut Vec<Effect>) {
        for x in a {
            match x {
                pk::Act::ToggleScreen => {
                    Self::screen_effects(self.screen.screen(ScreenMode::Toggle, now), out)
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
                    if ev.kind == EV_KEY {
                        Self::actions(self.gestures.on_key(ev.code, ev.value, now), &mut out);
                    }
                } else {
                    let o = self.rotation.orientation();
                    Self::touch_effects(self.touch.on_event(ev, now, o), &mut out);
                }
            }
            Role::Touch => {
                let o = self.rotation.orientation();
                Self::touch_effects(self.touch.on_event(ev, now, o), &mut out);
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
        Self::touch_effects(self.touch.tick(now), &mut out);
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
        self.power.poll_interval_ms().min(GESTURE_TICK)
    }
}
