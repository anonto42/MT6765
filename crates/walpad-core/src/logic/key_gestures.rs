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
mod tests {
    use super::*;

    const UP: u16 = KEY_VOLUMEUP;
    const DOWN: u16 = KEY_VOLUMEDOWN;

    fn tap(g: &mut Gestures, code: u16, at: Ms) {
        g.on_key(code, 1, at);
        g.on_key(code, 0, at + 50);
    }

    #[test]
    fn action_parser() {
        assert_eq!(parse_action("none"), Action::None);
        assert_eq!(parse_action(""), Action::None);
        assert_eq!(parse_action("key:ctrl+Tab"), Action::Key("ctrl+Tab".into()));
        assert_eq!(
            parse_action("ctl:brightness +40"),
            Action::Ctl("brightness +40".into())
        );
        assert_eq!(
            parse_action("launch:browser"),
            Action::Launch("browser".into())
        );
        assert_eq!(
            parse_action("run:firefox-esr"),
            Action::Run("firefox-esr".into())
        );
        assert_eq!(parse_action("volume:-5"), Action::Volume(-5));
        assert_eq!(parse_action("volume:+5"), Action::Volume(5));
        assert_eq!(parse_action("home"), Action::Home);
        assert_eq!(parse_action("back"), Action::Back);
        assert_eq!(parse_action("powermenu"), Action::PowerMenu);
        assert!(matches!(parse_action("rm -rf /"), Action::Unknown(_)));
    }

    #[test]
    fn rule3_5_tap_counts_after_window() {
        for (n, want) in [
            (1, "key:Up"),
            (2, "key:Prior"),
            (3, "ctl:brightness +40"),
            (4, "none"),
            (5, "none"),
        ] {
            let mut g = Gestures::new(Cfg::default());
            let mut t = 0;
            for _ in 0..n {
                tap(&mut g, UP, t);
                t += 100;
            }
            let last_release = t - 100 + 50;
            assert!(
                g.tick(last_release + 349).is_empty(),
                "n={n}: still inside the window"
            );
            let got = g.tick(last_release + 350);
            let want = parse_action(want);
            if want == Action::None {
                assert!(got.is_empty());
            } else {
                assert_eq!(got, vec![want], "n={n}");
            }
        }
    }

    #[test]
    fn rule3_quad_uses_quad_action_for_four_or_more() {
        let mut cfg = Cfg::default();
        cfg.vol_up.quad = "launch:browser".into();
        for n in [4, 6] {
            let mut g = Gestures::new(cfg.clone());
            for i in 0..n {
                tap(&mut g, UP, i * 100);
            }
            assert_eq!(
                g.tick(n * 100 + 400),
                vec![Action::Launch("browser".into())]
            );
        }
    }

    #[test]
    fn rule2_chord_mutes_both_and_runs_both_press() {
        let mut g = Gestures::new(Cfg::default());
        g.on_key(UP, 1, 0);
        assert_eq!(
            g.on_key(DOWN, 1, 100),
            vec![Action::Ctl("rotate toggle".into())]
        );
        g.on_key(UP, 0, 200);
        g.on_key(DOWN, 0, 210);
        assert!(g.tick(5000).is_empty(), "muted: no taps afterwards");
    }

    #[test]
    fn rule2_chord_window_and_long() {
        let mut g = Gestures::new(Cfg::default());
        g.on_key(UP, 1, 0);
        assert!(g.on_key(DOWN, 1, 161).is_empty(), "outside chord_ms");
        let mut g = Gestures::new(Cfg::default());
        g.on_key(UP, 1, 0);
        g.tick(700); // up is long now
        assert!(
            g.on_key(DOWN, 1, 100 + 600).is_empty(),
            "other already long"
        );
    }

    #[test]
    fn rule4_long_runs_once_and_clears_taps() {
        let mut g = Gestures::new(Cfg::default());
        tap(&mut g, DOWN, 0);
        g.on_key(DOWN, 1, 100);
        assert!(g.tick(699).is_empty());
        assert_eq!(g.tick(700), vec![Action::Home]);
        assert!(g.tick(900).is_empty(), "once");
        g.on_key(DOWN, 0, 1000);
        assert!(
            g.tick(3000).is_empty(),
            "release after long adds no tap, earlier taps cleared"
        );
    }

    #[test]
    fn rule4_hold_repeat_quirk_first_repeat_in_same_tick() {
        // Parity: last_rep is the press time, so the first repeat fires with the long action.
        let mut cfg = Cfg::default();
        cfg.vol_up.hold_repeat = true;
        let mut g = Gestures::new(cfg);
        g.on_key(UP, 1, 0);
        assert_eq!(g.tick(600), vec![Action::Keyboard, Action::Keyboard]);
        assert!(g.tick(700).is_empty());
        assert_eq!(g.tick(740), vec![Action::Keyboard]);
    }

    #[test]
    fn wake_press_is_swallowed() {
        let mut g = Gestures::new(Cfg::default());
        g.note_wake(1000);
        tap(&mut g, UP, 1000);
        assert!(
            g.tick(3000).is_empty(),
            "short press that woke the screen: no tap"
        );
        // long press that woke the screen: no action
        g.note_wake(5000);
        g.on_key(DOWN, 1, 5000);
        assert!(g.tick(5700).is_empty());
        // a later press is fine
        tap(&mut g, UP, 9000);
        assert_eq!(g.tick(9500), vec![Action::Key("Up".into())]);
    }

    #[test]
    fn other_keys_ignored() {
        let mut g = Gestures::new(Cfg::default());
        g.on_key(116, 1, 0);
        g.on_key(116, 0, 10);
        assert!(g.tick(1000).is_empty());
    }

    #[test]
    fn keys_json_defaults_deserialize() {
        let cfg: Cfg =
            serde_json::from_str(r#"{"vol_up":{"quad":"launch:browser"},"long_ms":500}"#).unwrap();
        assert_eq!(cfg.long_ms, 500);
        assert_eq!(cfg.vol_up.quad, "launch:browser");
        assert_eq!(
            cfg.vol_up.single, "none",
            "missing per-button keys fall back to the serde default"
        );
    }

    #[test]
    fn shipped_keys_json_parses_and_quad_launches_a_shipped_app() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rootfs/etc/walpad");
        let cfg: Cfg =
            serde_json::from_str(&std::fs::read_to_string(root.join("keys.json")).unwrap())
                .unwrap();
        assert_eq!(cfg.vol_up.quad, "launch:browser");
        assert_eq!(
            parse_action(&cfg.vol_up.quad),
            Action::Launch("browser".into())
        );
        for a in [
            &cfg.vol_up.single,
            &cfg.vol_up.double,
            &cfg.vol_up.triple,
            &cfg.vol_up.long,
            &cfg.vol_down.single,
            &cfg.vol_down.double,
            &cfg.vol_down.triple,
            &cfg.vol_down.quad,
            &cfg.vol_down.long,
            &cfg.both.press,
        ] {
            assert!(
                !matches!(parse_action(a), Action::Run(_)),
                "run: in keys.json: {a}"
            );
        }
        let apps: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("apps.json")).unwrap())
                .unwrap();
        assert!(
            apps.as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == "browser"),
            "apps.json has no browser"
        );
    }
}
