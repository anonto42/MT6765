//! Touch bridge mapping (parity: walpad-touchd.py).
use super::Ms;
use serde::Deserialize;
use walpad_hw::evdev::{
    InputEvent, ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_X, ABS_Y, BTN_TOUCH, EV_ABS, EV_KEY,
    EV_SYN, SYN_MT_REPORT, SYN_REPORT,
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
    /// Report a second finger (protocol A: two contacts in one frame) as `Act::Pair` once two
    /// consecutive frames agree. Off = the legacy single pointer.
    pub multitouch: bool,
    /// Gesture recogniser settings (touch.json key `gestures`).
    pub gestures: super::gesture::Cfg,
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
            multitouch: true,
            gestures: super::gesture::Cfg::default(),
        }
    }
}

/// X pointer actions (XTEST).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Move(i32, i32),
    Press,
    Release,
    /// Two fingers, confirmed by two consecutive frames (logical positions). Replaces `Move`.
    Pair((i32, i32), (i32, i32)),
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
    /// Coordinates of the contact being reported (protocol A), and the finished contacts of the
    /// frame so far.
    cx: Option<i32>,
    cy: Option<i32>,
    contacts: Vec<(i32, i32)>,
    /// Consecutive frames with two or more contacts.
    multi_frames: u8,
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
            cx: None,
            cy: None,
            contacts: Vec::new(),
            multi_frames: 0,
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

    fn logical(&self, p: (i32, i32), orientation: u16) -> (i32, i32) {
        let (ox, oy) = self.calibrate(p.0, p.1);
        to_logical(ox, oy, orientation)
    }

    /// Rules 1-3. `orientation` is the cached value from /run/walpad/orientation.
    pub fn on_event(&mut self, ev: &InputEvent, now: Ms, orientation: u16) -> Vec<Act> {
        let mut out = Vec::new();
        match (ev.kind, ev.code) {
            (EV_ABS, ABS_MT_POSITION_X) | (EV_ABS, ABS_X) => {
                self.x = Some(ev.value);
                self.cx = Some(ev.value);
                self.moved = true;
                self.last = now;
            }
            (EV_ABS, ABS_MT_POSITION_Y) | (EV_ABS, ABS_Y) => {
                self.y = Some(ev.value);
                self.cy = Some(ev.value);
                self.moved = true;
                self.last = now;
            }
            // protocol A: one contact is complete
            (EV_SYN, SYN_MT_REPORT) => {
                if let (Some(x), Some(y)) = (self.cx.take(), self.cy.take()) {
                    if self.contacts.len() < 4 {
                        self.contacts.push((x, y));
                    }
                }
            }
            (EV_KEY, BTN_TOUCH) if ev.value == 1 => {
                self.last = now;
                self.moved = true;
            }
            (EV_KEY, BTN_TOUCH) if ev.value == 0 => {
                if self.down {
                    self.down = false;
                    self.x = None;
                    self.y = None;
                    self.cx = None;
                    self.cy = None;
                    self.contacts.clear();
                    self.multi_frames = 0;
                    self.moved = false;
                    out.push(Act::Release);
                }
            }
            (EV_SYN, SYN_REPORT) if self.moved => {
                let contacts = std::mem::take(&mut self.contacts);
                self.cx = None;
                self.cy = None;
                // the first contact of the frame is the pointer; without contact markers (ABS_X
                // devices, no SYN_MT_REPORT) the last reported coordinates are the only one
                let pts: Vec<(i32, i32)> = if !contacts.is_empty() {
                    contacts
                } else if let (Some(x), Some(y)) = (self.x, self.y) {
                    vec![(x, y)]
                } else {
                    Vec::new()
                };
                if let Some(&(x, y)) = pts.first() {
                    self.moved = false;
                    let (lx, ly) = self.logical((x, y), orientation);
                    if self.cfg.multitouch && pts.len() >= 2 {
                        self.multi_frames = self.multi_frames.saturating_add(1);
                    } else {
                        self.multi_frames = 0;
                    }
                    let pair = (self.multi_frames >= 2 && self.down).then(|| pts[1]);
                    match pair {
                        Some(b) => out.push(Act::Pair((lx, ly), self.logical(b, orientation))),
                        None => out.push(Act::Move(lx, ly)),
                    }
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
            self.multi_frames = 0;
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
mod tests;
