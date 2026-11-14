//! Window layout engine (change multitasking-windows): decides, from the app windows on screen and
//! the work area, which windows to un-maximise, maximise and place. Pure: the daemon feeds it
//! the X facts and applies the returned operations (`xbridge`). Geometry math is
//! `walpad_proto::wm`.
use std::collections::{HashMap, HashSet};
use walpad_proto::wm::{self, LayoutMode, WRect};

/// What the engine knows about one window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WinView {
    pub id: u32,
    pub rect: WRect,
    pub maximized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutOp {
    Maximize(u32),
    Unmaximize(u32),
    Place(u32, WRect),
}

pub struct Layout {
    pub mode: LayoutMode,
    pub ratio: u8,
    pub swap: bool,
    /// Which window sits in which tile of a split.
    slots: [Option<u32>; 2],
    /// Windows this engine un-maximised (to be maximised again in single mode).
    managed: HashSet<u32>,
    /// The last rectangle sent for a window (never send the same one twice).
    sent: HashMap<u32, WRect>,
    /// Mode and area of the last plan, to notice changes.
    last: Option<(LayoutMode, WRect)>,
}

impl Default for Layout {
    fn default() -> Self {
        Layout {
            mode: LayoutMode::Single,
            ratio: 50,
            swap: false,
            slots: [None, None],
            managed: HashSet::new(),
            sent: HashMap::new(),
            last: None,
        }
    }
}

impl Layout {
    pub fn new(mode: LayoutMode, ratio: u8, swap: bool) -> Self {
        Layout {
            mode,
            ratio: if wm::valid_ratio(ratio) { ratio } else { 50 },
            swap,
            ..Layout::default()
        }
    }

    /// Anything left to put back (single mode still has windows to maximise)?
    pub fn busy(&self) -> bool {
        self.mode != LayoutMode::Single || !self.managed.is_empty()
    }

    /// Forget what was sent (a setting changed: send everything again).
    pub fn invalidate(&mut self) {
        self.sent.clear();
    }

    /// A window the owner placed by hand (free mode): remember it as managed and sent.
    pub fn placed(&mut self, id: u32, r: WRect) {
        self.managed.insert(id);
        self.sent.insert(id, r);
    }

    /// Operations that bring the windows (back to front) in line with the mode.
    pub fn plan(&mut self, wins: &[WinView], area: WRect) -> Vec<LayoutOp> {
        let live: HashSet<u32> = wins.iter().map(|w| w.id).collect();
        self.managed.retain(|w| live.contains(w));
        self.sent.retain(|w, _| live.contains(w));
        for s in self.slots.iter_mut() {
            if s.is_some_and(|w| !live.contains(&w)) {
                *s = None;
            }
        }
        if self.last.is_some_and(|(m, a)| m != self.mode || a != area) {
            // a new mode or a new work area (rotation, keyboard): everything is placed again
            self.sent.clear();
        }
        self.last = Some((self.mode, area));
        let mut ops = Vec::new();
        match self.mode {
            LayoutMode::Single => {
                let mut back: Vec<u32> = self.managed.drain().collect();
                back.sort_unstable();
                for id in back {
                    ops.push(LayoutOp::Maximize(id));
                }
                self.sent.clear();
                self.slots = [None, None];
            }
            LayoutMode::Split => {
                let top_first: Vec<u32> = wins.iter().rev().map(|w| w.id).collect();
                self.slots = wm::assign_slots(self.slots, &top_first);
                let tiles = wm::tile_split(area, self.ratio, self.swap);
                let filled = self.slots.iter().flatten().count();
                for (i, slot) in self.slots.clone().iter().enumerate() {
                    let Some(id) = slot else { continue };
                    // a lone window takes the whole work area
                    let want = if filled == 1 { area } else { tiles[i] };
                    if filled == 1 && wins.iter().any(|w| w.id == *id && w.maximized) {
                        continue; // already fills the work area: leave it maximised
                    }
                    self.place(wins, *id, want, &mut ops);
                }
            }
            LayoutMode::Free => {
                for w in wins {
                    if !self.managed.contains(&w.id) {
                        // first time in free mode: a floating, cascaded window
                        let want = wm::cascade(area, self.managed.len());
                        self.place(wins, w.id, want, &mut ops);
                    } else {
                        // keep it inside the work area (rotation, keyboard, strips)
                        let want = wm::clamp_rect(w.rect, area);
                        if want != w.rect {
                            self.place(wins, w.id, want, &mut ops);
                        }
                    }
                }
            }
        }
        ops
    }

    fn place(&mut self, wins: &[WinView], id: u32, want: WRect, ops: &mut Vec<LayoutOp>) {
        let Some(w) = wins.iter().find(|w| w.id == id) else {
            return;
        };
        if w.maximized {
            ops.push(LayoutOp::Unmaximize(id));
        }
        self.managed.insert(id);
        if self.sent.get(&id) != Some(&want) && (w.rect != want || w.maximized) {
            ops.push(LayoutOp::Place(id, want));
        }
        self.sent.insert(id, want);
    }
}

#[cfg(test)]
mod tests;
