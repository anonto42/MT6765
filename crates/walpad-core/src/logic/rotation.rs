//! Auto-rotation (parity: walpad-devd.py decide()/set_orientation()/rotate commands).
use serde::Deserialize;
use walpad_proto::RotateMode;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub auto_rotate: bool,
    pub rotate_flip: bool,
    pub tilt_min: i32,
    pub tilt_ratio: f64,
    pub rotate_samples: u32,
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            auto_rotate: true,
            rotate_flip: false,
            tilt_min: 3000,
            tilt_ratio: 1.35,
            rotate_samples: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Write /run/walpad/orientation.
    WriteOrientation(u16),
    SetStruts(u16),
    /// Dock the on-screen keyboard (follows struts).
    DockKeyboard(u16),
}

/// Rule 2: map an accelerometer sample to an orientation, or None for "no change".
pub fn decide(x: i32, y: i32, cfg: &Cfg) -> Option<u16> {
    if x.abs().max(y.abs()) < cfg.tilt_min {
        return None;
    }
    let (ax, ay) = (x.abs() as f64, y.abs() as f64);
    let o = if ax > ay * cfg.tilt_ratio {
        if x > 0 {
            90
        } else {
            270
        }
    } else if ay > ax * cfg.tilt_ratio {
        if y > 0 {
            0
        } else {
            180
        }
    } else {
        return None;
    };
    Some(if cfg.rotate_flip {
        match o {
            0 => 180,
            180 => 0,
            90 => 270,
            _ => 90,
        }
    } else {
        o
    })
}

/// Rule 5: _NET_WM_STRUT_PARTIAL (12 values) for an orientation.
pub fn strut_partial(o: u16) -> [u32; 12] {
    if matches!(o, 90 | 270) {
        [0, 0, 0, 480, 0, 0, 0, 0, 0, 0, 0, 1279]
    } else {
        [0, 480, 0, 0, 0, 0, 0, 1279, 0, 0, 0, 0]
    }
}
/// _NET_WM_STRUT (4 values).
pub fn strut(o: u16) -> [u32; 4] {
    let p = strut_partial(o);
    [p[0], p[1], p[2], p[3]]
}
/// Rule 4: keyboard geometry (x, y, w, h) docked at the bottom of the visible area.
pub fn keyboard_geometry(o: u16) -> (u32, u32, u32, u32) {
    let landscape = matches!(o, 90 | 270);
    let (vw, vh) = if landscape { (1280, 800) } else { (800, 1280) };
    let kh = if landscape { 340 } else { 420 };
    (0, vh - kh, vw, kh)
}

#[derive(Debug)]
pub struct Rotation {
    cfg: Cfg,
    orientation: u16,
    locked: bool,
    cand: Option<u16>,
    cand_n: u32,
}

impl Rotation {
    pub fn new(cfg: Cfg) -> Self {
        Rotation {
            cfg,
            orientation: 0,
            locked: false,
            cand: None,
            cand_n: 0,
        }
    }
    pub fn orientation(&self) -> u16 {
        self.orientation
    }
    pub fn locked(&self) -> bool {
        self.locked
    }
    pub fn set_cfg(&mut self, cfg: Cfg) {
        self.cfg = cfg;
    }
    /// Orientation is 0 at startup and the file is always written.
    pub fn init(&self) -> Vec<Act> {
        Self::acts(self.orientation)
    }
    fn acts(o: u16) -> Vec<Act> {
        vec![
            Act::WriteOrientation(o),
            Act::SetStruts(o),
            Act::DockKeyboard(o),
        ]
    }
    fn set(&mut self, o: u16) -> Vec<Act> {
        if o == self.orientation {
            return Vec::new();
        }
        self.orientation = o;
        Self::acts(o)
    }

    /// Rules 1+3: called every ~400 ms with a gsensor sample; the caller skips the read when
    /// auto_rotate is off, the lock is on, or the screen is off (see `wants_sample`).
    pub fn wants_sample(&self, screen_off: bool) -> bool {
        self.cfg.auto_rotate && !self.locked && !screen_off
    }
    pub fn sample(&mut self, g: Option<(i32, i32, i32)>) -> Vec<Act> {
        let Some((x, y, _)) = g else {
            return Vec::new();
        };
        match decide(x, y, &self.cfg) {
            Some(o) if o != self.orientation => {
                self.cand_n = if self.cand == Some(o) {
                    self.cand_n + 1
                } else {
                    1
                };
                self.cand = Some(o);
                if self.cand_n >= self.cfg.rotate_samples {
                    self.cand_n = 0;
                    return self.set(o);
                }
                Vec::new()
            }
            _ => {
                self.cand = None;
                self.cand_n = 0;
                Vec::new()
            }
        }
    }

    /// Rule 6: rotate commands.
    pub fn command(&mut self, mode: RotateMode) -> Vec<Act> {
        match mode {
            RotateMode::Toggle => {
                self.locked = !self.locked;
                Vec::new()
            }
            RotateMode::Auto => {
                self.locked = false;
                Vec::new()
            }
            RotateMode::Lock => {
                self.locked = true;
                Vec::new()
            }
            RotateMode::R0 | RotateMode::R90 | RotateMode::R180 | RotateMode::R270 => {
                self.locked = true;
                let o = match mode {
                    RotateMode::R0 => 0,
                    RotateMode::R90 => 90,
                    RotateMode::R180 => 180,
                    _ => 270,
                };
                self.set(o)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule2_decide_table() {
        let c = Cfg::default();
        for (x, y, want) in [
            (0, 0, None),
            (2999, 100, None),
            (4000, 100, Some(90)),
            (-4000, 100, Some(270)),
            (100, 4000, Some(0)),
            (100, -4000, Some(180)),
            (4000, 3500, None), // inside the ratio dead zone
            (3000, 0, Some(90)),
            (1350, 1000, None), // below tilt_min
            (5400, 4000, None), // exactly at ratio: not strictly greater
            (5401, 4000, Some(90)),
        ] {
            assert_eq!(decide(x, y, &c), want, "x {x} y {y}");
        }
    }

    #[test]
    fn rule2_flip() {
        let c = Cfg {
            rotate_flip: true,
            ..Cfg::default()
        };
        assert_eq!(decide(100, 4000, &c), Some(180));
        assert_eq!(decide(100, -4000, &c), Some(0));
        assert_eq!(decide(4000, 100, &c), Some(270));
        assert_eq!(decide(-4000, 100, &c), Some(90));
    }

    #[test]
    fn rule3_debounce() {
        let mut r = Rotation::new(Cfg::default());
        let g = Some((4000, 100, 0));
        assert!(r.sample(g).is_empty());
        assert!(r.sample(g).is_empty());
        assert_eq!(
            r.sample(g),
            vec![
                Act::WriteOrientation(90),
                Act::SetStruts(90),
                Act::DockKeyboard(90)
            ]
        );
        assert_eq!(r.orientation(), 90);
    }

    #[test]
    fn rule3_no_change_sample_resets_candidate() {
        let mut r = Rotation::new(Cfg::default());
        let g = Some((4000, 100, 0));
        r.sample(g);
        r.sample(g);
        r.sample(Some((0, 0, 9000))); // flat: resets
        assert!(r.sample(g).is_empty());
        assert!(r.sample(g).is_empty());
        assert!(!r.sample(g).is_empty());
        // a different candidate restarts the count
        let mut r = Rotation::new(Cfg::default());
        r.sample(g);
        r.sample(g);
        assert!(r.sample(Some((-4000, 100, 0))).is_empty());
        assert_eq!(r.orientation(), 0);
    }

    #[test]
    fn missing_sample_does_nothing() {
        let mut r = Rotation::new(Cfg::default());
        assert!(r.sample(None).is_empty());
    }

    #[test]
    fn rule1_sampling_conditions() {
        let mut r = Rotation::new(Cfg::default());
        assert!(r.wants_sample(false));
        assert!(!r.wants_sample(true));
        r.command(RotateMode::Lock);
        assert!(!r.wants_sample(false));
        let r2 = Rotation::new(Cfg {
            auto_rotate: false,
            ..Cfg::default()
        });
        assert!(!r2.wants_sample(false));
    }

    #[test]
    fn rule4_5_geometry_and_struts() {
        assert_eq!(strut_partial(0), [0, 480, 0, 0, 0, 0, 0, 1279, 0, 0, 0, 0]);
        assert_eq!(strut_partial(180), strut_partial(0));
        assert_eq!(strut_partial(90), [0, 0, 0, 480, 0, 0, 0, 0, 0, 0, 0, 1279]);
        assert_eq!(strut_partial(270), strut_partial(90));
        assert_eq!(strut(0), [0, 480, 0, 0]);
        assert_eq!(strut(90), [0, 0, 0, 480]);
        assert_eq!(keyboard_geometry(0), (0, 860, 800, 420));
        assert_eq!(keyboard_geometry(90), (0, 460, 1280, 340));
    }

    #[test]
    fn rule5_starts_at_zero_and_writes_file() {
        let r = Rotation::new(Cfg::default());
        assert_eq!(r.orientation(), 0);
        assert_eq!(r.init()[0], Act::WriteOrientation(0));
    }

    #[test]
    fn rule6_commands() {
        let mut r = Rotation::new(Cfg::default());
        r.command(RotateMode::Toggle);
        assert!(r.locked());
        r.command(RotateMode::Toggle);
        assert!(!r.locked());
        r.command(RotateMode::Lock);
        r.command(RotateMode::Auto);
        assert!(!r.locked());
        let acts = r.command(RotateMode::R180);
        assert!(r.locked());
        assert_eq!(r.orientation(), 180);
        assert_eq!(acts[0], Act::WriteOrientation(180));
        assert!(
            r.command(RotateMode::R180).is_empty(),
            "same angle: nothing to apply"
        );
    }
}
