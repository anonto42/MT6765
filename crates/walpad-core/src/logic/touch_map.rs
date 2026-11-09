//! Touch bridge mapping (parity: walpad-touchd.py).
use super::Ms;
use serde::Deserialize;
use walpad_hw::evdev::{
    InputEvent, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_X, ABS_Y, BTN_TOUCH, EV_ABS, EV_KEY,
    EV_SYN, SYN_REPORT,
};

pub const W: i32 = 800;
pub const H: i32 = 1280;
const LIFT_MS: Ms = 150;
/// Orientation file is re-read at most this often (rule 5).
pub const ORIENTATION_REREAD_MS: Ms = 300;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub swap: bool,
    pub flipx: bool,
    pub flipy: bool,
    pub xmin: i32,
    pub xmax: i32,
    pub ymin: i32,
    pub ymax: i32,
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            swap: false,
            flipx: false,
            flipy: false,
            xmin: 0,
            xmax: W,
            ymin: 0,
            ymax: H,
        }
    }
}

/// X pointer actions (XTEST).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Move(i32, i32),
    Press,
    Release,
}

fn mapv(v: i32, lo: i32, hi: i32, size: i32, flip: bool) -> i32 {
    let r = ((v - lo) as f64 * (size - 1) as f64 / (hi - lo).max(1) as f64)
        .clamp(0.0, (size - 1) as f64);
    if flip {
        ((size - 1) as f64 - r) as i32
    } else {
        r as i32
    }
}

/// Rule 2 (rotation part): panel (800x1280) -> position on the 1280x1280 X screen.
pub fn to_logical(ox: i32, oy: i32, orientation: u16) -> (i32, i32) {
    match orientation {
        180 => (W - 1 - ox, H - 1 - oy),
        90 => (oy, W - 1 - ox),
        270 => (H - 1 - oy, ox),
        _ => (ox, oy),
    }
}

#[derive(Debug)]
pub struct TouchMap {
    cfg: Cfg,
    x: Option<i32>,
    y: Option<i32>,
    moved: bool,
    down: bool,
    last: Ms,
}

impl TouchMap {
    pub fn new(cfg: Cfg) -> Self {
        TouchMap {
            cfg,
            x: None,
            y: None,
            moved: false,
            down: false,
            last: 0,
        }
    }
    pub fn is_down(&self) -> bool {
        self.down
    }
    /// Raw panel point after calibration, before rotation.
    pub fn calibrate(&self, x: i32, y: i32) -> (i32, i32) {
        let (px, py) = if self.cfg.swap { (y, x) } else { (x, y) };
        (
            mapv(px, self.cfg.xmin, self.cfg.xmax, W, self.cfg.flipx),
            mapv(py, self.cfg.ymin, self.cfg.ymax, H, self.cfg.flipy),
        )
    }

    /// Rules 1-3. `orientation` is the cached value from /run/walpad/orientation.
    pub fn on_event(&mut self, ev: &InputEvent, now: Ms, orientation: u16) -> Vec<Act> {
        let mut out = Vec::new();
        match (ev.kind, ev.code) {
            (EV_ABS, ABS_MT_POSITION_X) | (EV_ABS, ABS_X) => {
                self.x = Some(ev.value);
                self.moved = true;
                self.last = now;
            }
            (EV_ABS, ABS_MT_POSITION_Y) | (EV_ABS, ABS_Y) => {
                self.y = Some(ev.value);
                self.moved = true;
                self.last = now;
            }
            (EV_KEY, BTN_TOUCH) if ev.value == 1 => {
                self.last = now;
                self.moved = true;
            }
            (EV_SYN, SYN_REPORT) if self.moved => {
                if let (Some(x), Some(y)) = (self.x, self.y) {
                    self.moved = false;
                    let (ox, oy) = self.calibrate(x, y);
                    let (lx, ly) = to_logical(ox, oy, orientation);
                    out.push(Act::Move(lx, ly));
                    if !self.down {
                        out.push(Act::Press);
                        self.down = true;
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// Rule 4: lift = 150 ms with no events -> release and forget the position.
    pub fn tick(&mut self, now: Ms) -> Vec<Act> {
        if self.down && now.saturating_sub(self.last) > LIFT_MS {
            self.down = false;
            self.x = None;
            self.y = None;
            return vec![Act::Release];
        }
        Vec::new()
    }
}

/// Rule 5: cache for the orientation file.
#[derive(Debug, Default)]
pub struct OrientationCache {
    value: u16,
    checked: Option<Ms>,
}
impl OrientationCache {
    /// `read` yields the file content (None if unreadable); invalid or missing means 0.
    pub fn get(&mut self, now: Ms, read: impl FnOnce() -> Option<String>) -> u16 {
        let stale = self
            .checked
            .is_none_or(|t| now.saturating_sub(t) > ORIENTATION_REREAD_MS);
        if stale {
            self.checked = Some(now);
            self.value = read()
                .and_then(|s| s.trim().parse::<u16>().ok())
                .filter(|v| matches!(v, 0 | 90 | 180 | 270))
                .unwrap_or(0);
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: u16, code: u16, value: i32) -> InputEvent {
        InputEvent {
            sec: 0,
            usec: 0,
            kind,
            code,
            value,
        }
    }
    fn touch(t: &mut TouchMap, x: i32, y: i32, now: Ms, o: u16) -> Vec<Act> {
        let mut out = t.on_event(&ev(EV_ABS, ABS_MT_POSITION_X, x), now, o);
        out.extend(t.on_event(&ev(EV_ABS, ABS_MT_POSITION_Y, y), now, o));
        out.extend(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), now, o));
        out
    }

    #[test]
    fn rule2_rotation_table() {
        // panel point (100, 200)
        assert_eq!(to_logical(100, 200, 0), (100, 200));
        assert_eq!(to_logical(100, 200, 180), (699, 1079));
        assert_eq!(to_logical(100, 200, 90), (200, 699));
        assert_eq!(to_logical(100, 200, 270), (1079, 100));
    }

    #[test]
    fn rule2_scale_clamp_flip_swap() {
        let t = TouchMap::new(Cfg::default());
        assert_eq!(t.calibrate(0, 0), (0, 0));
        assert_eq!(t.calibrate(800, 1280), (799, 1279));
        assert_eq!(t.calibrate(5000, -5), (799, 0));
        let t = TouchMap::new(Cfg {
            flipx: true,
            flipy: true,
            ..Cfg::default()
        });
        assert_eq!(t.calibrate(0, 0), (799, 1279));
        assert_eq!(t.calibrate(800, 1280), (0, 0));
        let t = TouchMap::new(Cfg {
            swap: true,
            xmax: 1280,
            ymax: 800,
            ..Cfg::default()
        });
        assert_eq!(t.calibrate(0, 0), (0, 0));
        // swap: x input feeds the y axis (range 0-800 mapped to 1280), y feeds x
        assert_eq!(t.calibrate(400, 0), (0, 639));
        let t = TouchMap::new(Cfg {
            xmin: 100,
            xmax: 700,
            ..Cfg::default()
        });
        assert_eq!(t.calibrate(100, 0).0, 0);
        assert_eq!(t.calibrate(700, 0).0, 799);
    }

    #[test]
    fn rule1_3_press_on_first_report_then_only_moves() {
        let mut t = TouchMap::new(Cfg::default());
        assert_eq!(
            touch(&mut t, 400, 640, 10, 0),
            vec![Act::Move(399, 639), Act::Press]
        );
        assert_eq!(touch(&mut t, 500, 640, 20, 0), vec![Act::Move(499, 639)]);
    }

    #[test]
    fn rule1_alt_codes_and_btn_touch_counts_as_movement() {
        let mut t = TouchMap::new(Cfg::default());
        t.on_event(&ev(EV_ABS, ABS_X, 10), 1, 0);
        t.on_event(&ev(EV_ABS, ABS_Y, 20), 1, 0);
        t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1, 0); // consumes moved
        assert!(
            t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 2, 0).is_empty(),
            "no movement: no report"
        );
        t.on_event(&ev(EV_KEY, BTN_TOUCH, 1), 3, 0);
        assert!(!t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 3, 0).is_empty());
    }

    #[test]
    fn syn_without_both_coordinates_is_ignored() {
        let mut t = TouchMap::new(Cfg::default());
        t.on_event(&ev(EV_ABS, ABS_MT_POSITION_X, 5), 1, 0);
        assert!(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1, 0).is_empty());
        assert!(!t.is_down());
    }

    #[test]
    fn rule4_lift_after_150ms() {
        let mut t = TouchMap::new(Cfg::default());
        touch(&mut t, 400, 640, 1000, 0);
        assert!(t.tick(1150).is_empty(), "strictly greater than 150 ms");
        assert_eq!(t.tick(1151), vec![Act::Release]);
        assert!(!t.is_down());
        // position forgotten: a lone SYN does nothing, next touch presses again
        assert!(t.on_event(&ev(EV_SYN, SYN_REPORT, 0), 1200, 0).is_empty());
        assert_eq!(touch(&mut t, 10, 10, 2000, 0).last(), Some(&Act::Press));
    }

    #[test]
    fn rule4_events_extend_the_touch() {
        let mut t = TouchMap::new(Cfg::default());
        touch(&mut t, 400, 640, 1000, 0);
        touch(&mut t, 401, 640, 1100, 0);
        assert!(t.tick(1200).is_empty());
    }

    #[test]
    fn rule5_orientation_cache() {
        let mut c = OrientationCache::default();
        assert_eq!(c.get(0, || Some("90\n".into())), 90);
        assert_eq!(c.get(299, || Some("180".into())), 90, "cached");
        assert_eq!(c.get(301, || Some("180".into())), 180);
        assert_eq!(c.get(700, || Some("45".into())), 0, "invalid -> 0");
        assert_eq!(c.get(1100, || None), 0, "unreadable -> 0");
    }
}
