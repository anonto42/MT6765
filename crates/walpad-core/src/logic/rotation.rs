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
    // the keyboard takes at most 30% of the visible height (owner's request, 2026-11-10)
    let kh = vh * 30 / 100;
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
mod tests;
