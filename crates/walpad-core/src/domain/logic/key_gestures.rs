//! Volume-button gesture engine (parity: walpad-keys.py).
use super::Ms;
use serde::Deserialize;
use walpad_hw::evdev::{KEY_VOLUMEDOWN, KEY_VOLUMEUP};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ButtonCfg {
    pub single: String,
    pub double: String,
    pub triple: String,
    pub quad: String,
    pub long: String,
    pub hold_repeat: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BothCfg {
    pub press: String,
}
impl Default for BothCfg {
    fn default() -> Self {
        BothCfg {
            press: "ctl:rotate toggle".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub tap_window_ms: Ms,
    pub long_ms: Ms,
    pub chord_ms: Ms,
    pub repeat_ms: Ms,
    pub vol_up: ButtonCfg,
    pub vol_down: ButtonCfg,
    pub both: BothCfg,
}

fn b(single: &str, double: &str, triple: &str, long: &str) -> ButtonCfg {
    ButtonCfg {
        single: single.into(),
        double: double.into(),
        triple: triple.into(),
        quad: "none".into(),
        long: long.into(),
        hold_repeat: false,
    }
}
impl Default for ButtonCfg {
    fn default() -> Self {
        b("none", "none", "none", "none")
    }
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            tap_window_ms: 350,
            long_ms: 600,
            chord_ms: 160,
            repeat_ms: 140,
            vol_up: b("key:Up", "key:Prior", "ctl:brightness +40", "keyboard"),
            vol_down: b("key:Down", "key:Next", "ctl:brightness -40", "home"),
            both: BothCfg::default(),
        }
    }
}

/// A parsed key action (docs: keys.json actions).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    /// xdotool-style key spec, e.g. `ctrl+Tab`, `Prior`.
    Key(String),
    /// A devd.cmd line.
    Ctl(String),
    /// New: launch an app by allowlisted ID.
    Launch(String),
    /// Legacy `run:` (only honoured while Python is the runtime; the core rejects it).
    Run(String),
    Home,
    /// Alt+Left in the browser / file manager, Esc elsewhere (the app-switcher Back).
    Back,
    Keyboard,
    PowerMenu,
    Screenshot,
    Volume(i32),
    Unknown(String),
}

pub fn parse_action(s: &str) -> Action {
    let s = s.trim();
    if s.is_empty() || s == "none" {
        return Action::None;
    }
    match s {
        "home" => return Action::Home,
        "back" => return Action::Back,
        "keyboard" => return Action::Keyboard,
        "powermenu" => return Action::PowerMenu,
        "screenshot" => return Action::Screenshot,
        _ => {}
    }
    match s.split_once(':') {
        Some(("key", a)) => Action::Key(a.to_string()),
        Some(("ctl", a)) => Action::Ctl(a.to_string()),
        Some(("launch", a)) => Action::Launch(a.to_string()),
        Some(("run", a)) => Action::Run(a.to_string()),
        Some(("volume", a)) => a
            .trim()
            .parse::<i32>()
            .map(Action::Volume)
            .unwrap_or_else(|_| Action::Unknown(s.to_string())),
        _ => Action::Unknown(s.to_string()),
    }
}

#[derive(Debug, Default)]
struct Key {
    down: bool,
    t: Ms,
    long: bool,
    taps: u32,
    deadline: Ms,
    mute: bool,
    last_rep: Ms,
}

#[derive(Debug)]
pub struct Gestures {
    cfg: Cfg,
    up: Key,
    down: Key,
    /// Time of the last wake-by-key (rule: a press that woke the screen is swallowed).
    last_wake: Option<Ms>,
}

impl Gestures {
    pub fn new(cfg: Cfg) -> Self {
        Gestures {
            cfg,
            up: Key::default(),
            down: Key::default(),
            last_wake: None,
        }
    }
    pub fn set_cfg(&mut self, cfg: Cfg) {
        self.cfg = cfg;
    }
    pub fn note_wake(&mut self, now: Ms) {
        self.last_wake = Some(now);
    }
    /// wake-ts >= press time - 50 ms
    fn wake_after(&self, t: Ms) -> bool {
        self.last_wake.is_some_and(|w| w + 50 >= t)
    }

    /// Feed a KEY event (code 114/115; value 1 press, 0 release).
    pub fn on_key(&mut self, code: u16, value: i32, now: Ms) -> Vec<Action> {
        let mut out = Vec::new();
        if code != KEY_VOLUMEUP && code != KEY_VOLUMEDOWN {
            return out;
        }
        let up = code == KEY_VOLUMEUP;
        let chord_ms = self.cfg.chord_ms;
        let tap_window = self.cfg.tap_window_ms;
        let both_press = self.cfg.both.press.clone();
        let (k, other) = if up {
            (&mut self.up, &mut self.down)
        } else {
            (&mut self.down, &mut self.up)
        };
        if value == 1 {
            k.down = true;
            k.t = now;
            k.long = false;
            k.mute = false;
            k.last_rep = now;
            if other.down && now.saturating_sub(other.t) <= chord_ms && !other.long {
                k.mute = true;
                other.mute = true;
                k.taps = 0;
                other.taps = 0;
                out.push(parse_action(&both_press));
            }
        } else if value == 0 && k.down {
            k.down = false;
            let t = k.t;
            let (mute, long) = (k.mute, k.long);
            k.mute = false;
            if !(mute || long) && !self.wake_after(t) {
                let k = if up { &mut self.up } else { &mut self.down };
                k.taps += 1;
                k.deadline = now + tap_window;
            }
        }
        out
    }

    /// Called every 40 ms (rule 1). Order: vol_up then vol_down, as in the Python dict.
    pub fn tick(&mut self, now: Ms) -> Vec<Action> {
        let mut out = Vec::new();
        for up in [true, false] {
            let c = if up {
                self.cfg.vol_up.clone()
            } else {
                self.cfg.vol_down.clone()
            };
            let (long_ms, repeat_ms) = (self.cfg.long_ms, self.cfg.repeat_ms);
            let wake = {
                let t = if up { self.up.t } else { self.down.t };
                self.wake_after(t)
            };
            let k = if up { &mut self.up } else { &mut self.down };
            // Rule 4: held >= long_ms -> run `long` once.
            if k.down && !k.long && !k.mute && now.saturating_sub(k.t) >= long_ms {
                k.long = true;
                k.taps = 0;
                if !wake {
                    out.push(parse_action(&c.long));
                }
            }
            if k.down && k.long && c.hold_repeat && now.saturating_sub(k.last_rep) >= repeat_ms {
                k.last_rep = now;
                out.push(parse_action(&c.long));
            }
            // Rule 5: after the deadline with the key up.
            if k.taps > 0 && !k.down && now >= k.deadline {
                let name = match k.taps {
                    1 => &c.single,
                    2 => &c.double,
                    3 => &c.triple,
                    _ => &c.quad,
                };
                k.taps = 0;
                out.push(parse_action(name));
            }
        }
        out.retain(|a| *a != Action::None);
        out
    }
}

#[cfg(test)]
mod tests;
