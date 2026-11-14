use crate::logic::{key_gestures as kg, touch_map as tm};
use std::fmt;
use walpad_proto::ShellInput;

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
    /// The screen was switched off by hand (short power press): lock at once.
    LockNow,
    Cover(bool),
    LowBattery(bool),
    CriticalPoweroff(u32),
    // input subsystem
    /// Pointer motion and button 1 (what the touch bridge always did).
    Pointer(tm::Act),
    /// Another X button (the right click of a long press).
    Button(u8, bool),
    /// One wheel click (scroll gestures), Control held for a pinch.
    Wheel {
        button: u8,
        ctrl: bool,
    },
    Action(kg::Action),
    /// A volume key while the shell's power menu is open: goes to the shell, not to the gestures.
    ShellKey(ShellInput),
}

impl Effect {
    pub fn is_device(&self) -> bool {
        !matches!(
            self,
            Effect::Pointer(_)
                | Effect::Button(..)
                | Effect::Wheel { .. }
                | Effect::Action(_)
                | Effect::ShellKey(_)
        )
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
            // the Python daemon has no screen lock: nothing to compare in shadow mode
            Effect::LockNow => Ok(()),
            Effect::Cover(c) => write!(f, "WOULD cover {}", if *c { "closed" } else { "open" }),
            Effect::LowBattery(l) => write!(f, "WOULD low-battery {l}"),
            Effect::CriticalPoweroff(mv) => write!(f, "WOULD poweroff (critical battery {mv} mV)"),
            Effect::ShellKey(k) => write!(f, "WOULD shell-key {k:?}"),
            Effect::Pointer(tm::Act::Move(x, y)) => write!(f, "WOULD pointer move {x} {y}"),
            Effect::Pointer(tm::Act::Press) => write!(f, "WOULD pointer press"),
            Effect::Pointer(tm::Act::Release) => write!(f, "WOULD pointer release"),
            Effect::Pointer(tm::Act::Pair(..)) => Ok(()),
            Effect::Button(n, d) => write!(
                f,
                "WOULD pointer button {n} {}",
                if *d { "press" } else { "release" }
            ),
            Effect::Wheel { button, ctrl } => {
                write!(
                    f,
                    "WOULD pointer wheel {button}{}",
                    if *ctrl { " ctrl" } else { "" }
                )
            }
            Effect::Action(a) => match a {
                kg::Action::None => Ok(()),
                kg::Action::Key(k) => write!(f, "WOULD action key:{k}"),
                kg::Action::Ctl(c) => write!(f, "WOULD action ctl:{c}"),
                kg::Action::Launch(a) => write!(f, "WOULD action launch:{a}"),
                kg::Action::Run(c) => write!(f, "WOULD action run:{c} (rejected under Rust)"),
                kg::Action::Home => write!(f, "WOULD action home"),
                kg::Action::Back => write!(f, "WOULD action back"),
                kg::Action::Keyboard => write!(f, "WOULD action keyboard"),
                kg::Action::PowerMenu => write!(f, "WOULD action powermenu"),
                kg::Action::Screenshot => write!(f, "WOULD action screenshot"),
                kg::Action::Volume(v) => write!(f, "WOULD action volume:{v:+}"),
                kg::Action::Unknown(s) => write!(f, "WOULD action unknown:{s}"),
            },
        }
    }
}
