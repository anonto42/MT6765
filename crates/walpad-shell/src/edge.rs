//! Bottom-edge navigation gesture (`app-switcher-navigation`): a touch that starts in the thin
//! edge zone and travels up is a swipe. A short swipe (released soon after it passed
//! `SWIPE_MIN_PX`) is Home; a swipe that is held still for `HOLD_MS` opens Recents. A touch that
//! starts anywhere else is never looked at (the shell maps an input-only window over the zone
//! only, so page scrolling and the other gestures are untouched). Pure: time is passed in.
use crate::dock::{HANDLE_MARGIN, HANDLE_W};
use crate::gesture::{Input, Ms};
use walpad_ui::Rect;

/// Height of the edge zone at the bottom of the visible area (px). Shared contract with the
/// touch gesture layer: a touch that starts here belongs to navigation, above it to the app.
pub const EDGE_ZONE_PX: i32 = 24;
/// Upward travel after which the touch counts as a swipe.
pub const SWIPE_MIN_PX: i32 = 64;
/// A swipe held still this long opens Recents.
pub const HOLD_MS: Ms = 300;
/// Movement smaller than this while holding does not restart the hold.
pub const HOLD_SLOP_PX: i32 = 12;

/// The zone, in root coordinates, for a visible area (w, h): the bottom 24 px over the full width
/// except the columns of the collapsed dock handle at the right end (it stays tappable).
pub fn zone_rect(area: (i32, i32)) -> Rect {
    Rect::new(
        0,
        area.1 - EDGE_ZONE_PX,
        area.0 - HANDLE_W - 2 * HANDLE_MARGIN,
        EDGE_ZONE_PX,
    )
}

/// Does a touch at (`x`, `y`) start in the zone?
pub fn in_zone(area: (i32, i32), x: i32, y: i32) -> bool {
    zone_rect(area).contains(x, y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeEvent {
    /// A touch started in the zone (the shell captures a fresh thumbnail of the app in front).
    Started,
    /// Short swipe up: Home.
    Home,
    /// Swipe up and hold: Recents.
    Recents,
    /// The touch ended without a swipe (a tap in the zone, or the finger went back down).
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    /// Down in the zone, not yet a swipe.
    Down {
        start: (i32, i32),
    },
    /// A swipe; `anchor`/`since` mark where and since when the finger has been still.
    Armed {
        start: (i32, i32),
        anchor: (i32, i32),
        since: Ms,
    },
    /// Recents was fired; wait for the release.
    Fired,
}

#[derive(Debug)]
pub struct EdgeSwipe {
    area: (i32, i32),
    state: State,
}

impl EdgeSwipe {
    pub fn new(area: (i32, i32)) -> EdgeSwipe {
        EdgeSwipe {
            area,
            state: State::Idle,
        }
    }

    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.state = State::Idle;
    }

    /// True while a touch that started in the zone is being tracked.
    pub fn active(&self) -> bool {
        self.state != State::Idle
    }

    pub fn feed(&mut self, input: Input, now: Ms) -> Vec<EdgeEvent> {
        let mut out = Vec::new();
        match (self.state, input) {
            (_, Input::Down { x, y }) => {
                if in_zone(self.area, x, y) {
                    self.state = State::Down { start: (x, y) };
                    out.push(EdgeEvent::Started);
                } else {
                    self.state = State::Idle;
                }
            }
            (State::Down { start }, Input::Move { x, y }) => {
                if start.1 - y >= SWIPE_MIN_PX {
                    self.state = State::Armed {
                        start,
                        anchor: (x, y),
                        since: now,
                    };
                }
            }
            (
                State::Armed {
                    start,
                    anchor,
                    since,
                },
                Input::Move { x, y },
            ) => {
                if start.1 - y < SWIPE_MIN_PX {
                    // the finger went back down: not a swipe any more
                    self.state = State::Down { start };
                } else if (x - anchor.0).abs() > HOLD_SLOP_PX || (y - anchor.1).abs() > HOLD_SLOP_PX
                {
                    self.state = State::Armed {
                        start,
                        anchor: (x, y),
                        since: now,
                    };
                } else {
                    self.state = State::Armed {
                        start,
                        anchor,
                        since,
                    };
                }
            }
            (State::Down { .. }, Input::Up { .. }) => {
                self.state = State::Idle;
                out.push(EdgeEvent::Cancelled);
            }
            (State::Armed { since, .. }, Input::Up { .. }) => {
                self.state = State::Idle;
                // held long enough but no tick got to run: it is still a hold
                out.push(if now.saturating_sub(since) >= HOLD_MS {
                    EdgeEvent::Recents
                } else {
                    EdgeEvent::Home
                });
            }
            (State::Fired, Input::Up { .. }) => self.state = State::Idle,
            (State::Fired, Input::Move { .. }) | (State::Idle, _) => {}
        }
        out
    }

    /// Fires Recents once the swipe has been held still for `HOLD_MS`.
    pub fn tick(&mut self, now: Ms) -> Vec<EdgeEvent> {
        if let State::Armed { since, .. } = self.state {
            if now.saturating_sub(since) >= HOLD_MS {
                self.state = State::Fired;
                return vec![EdgeEvent::Recents];
            }
        }
        Vec::new()
    }

    pub fn next_deadline(&self) -> Option<Ms> {
        match self.state {
            State::Armed { since, .. } => Some(since + HOLD_MS),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
