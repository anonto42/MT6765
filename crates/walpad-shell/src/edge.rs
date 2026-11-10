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
mod tests {
    use super::*;

    const AREA: (i32, i32) = (800, 1280);

    fn down(e: &mut EdgeSwipe, x: i32, y: i32, t: Ms) -> Vec<EdgeEvent> {
        e.feed(Input::Down { x, y }, t)
    }
    fn mv(e: &mut EdgeSwipe, x: i32, y: i32, t: Ms) -> Vec<EdgeEvent> {
        e.feed(Input::Move { x, y }, t)
    }
    fn up(e: &mut EdgeSwipe, x: i32, y: i32, t: Ms) -> Vec<EdgeEvent> {
        e.feed(Input::Up { x, y }, t)
    }

    #[test]
    fn zone_is_the_bottom_24_px_in_both_orientations() {
        assert_eq!(zone_rect((800, 1280)), Rect::new(0, 1256, 728, 24));
        assert_eq!(zone_rect((1280, 800)), Rect::new(0, 776, 1208, 24));
        assert!(in_zone(AREA, 400, 1256));
        assert!(in_zone(AREA, 0, 1279));
        assert!(!in_zone(AREA, 400, 1255));
        assert!(!in_zone(AREA, 400, 1280));
        // the dock handle's columns at the right end are not part of the zone
        assert!(!in_zone(AREA, 760, 1270));
        assert!(in_zone(AREA, 727, 1270));
    }

    #[test]
    fn short_swipe_up_is_home() {
        let mut e = EdgeSwipe::new(AREA);
        assert_eq!(down(&mut e, 400, 1270, 0), vec![EdgeEvent::Started]);
        assert!(mv(&mut e, 400, 1200, 30).is_empty());
        assert!(mv(&mut e, 400, 1100, 60).is_empty());
        assert_eq!(up(&mut e, 400, 1100, 90), vec![EdgeEvent::Home]);
        assert!(!e.active());
    }

    #[test]
    fn swipe_and_hold_300_ms_is_recents_once_and_release_is_silent() {
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        mv(&mut e, 400, 1100, 50);
        assert_eq!(e.next_deadline(), Some(350));
        assert!(e.tick(349).is_empty());
        assert_eq!(e.tick(350), vec![EdgeEvent::Recents]);
        assert!(e.tick(500).is_empty(), "fires once");
        assert!(mv(&mut e, 400, 1000, 600).is_empty());
        assert!(up(&mut e, 400, 1000, 700).is_empty());
    }

    #[test]
    fn small_jitter_keeps_the_hold_but_real_movement_restarts_it() {
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        mv(&mut e, 400, 1100, 50);
        mv(&mut e, 405, 1096, 200);
        assert_eq!(e.tick(350), vec![EdgeEvent::Recents], "jitter within 12 px");
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        mv(&mut e, 400, 1100, 50);
        mv(&mut e, 400, 1000, 200);
        assert!(
            e.tick(350).is_empty(),
            "moved 100 px at 200 ms: hold restarts"
        );
        assert_eq!(e.tick(500), vec![EdgeEvent::Recents]);
    }

    #[test]
    fn slow_release_without_a_tick_still_counts_as_a_hold() {
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        mv(&mut e, 400, 1100, 50);
        assert_eq!(up(&mut e, 400, 1100, 900), vec![EdgeEvent::Recents]);
    }

    #[test]
    fn a_tap_or_a_short_travel_in_the_zone_is_cancelled() {
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        assert_eq!(up(&mut e, 400, 1270, 80), vec![EdgeEvent::Cancelled]);
        down(&mut e, 400, 1270, 100);
        mv(&mut e, 400, 1230, 120); // only 40 px
        assert_eq!(up(&mut e, 400, 1230, 150), vec![EdgeEvent::Cancelled]);
    }

    #[test]
    fn returning_below_the_threshold_cancels_the_swipe_and_the_hold() {
        let mut e = EdgeSwipe::new(AREA);
        down(&mut e, 400, 1270, 0);
        mv(&mut e, 400, 1150, 20);
        assert!(e.next_deadline().is_some());
        mv(&mut e, 400, 1260, 40);
        assert_eq!(e.next_deadline(), None);
        assert!(e.tick(1000).is_empty());
        assert_eq!(up(&mut e, 400, 1260, 1100), vec![EdgeEvent::Cancelled]);
    }

    #[test]
    fn touches_that_start_outside_the_zone_are_ignored() {
        let mut e = EdgeSwipe::new(AREA);
        // 100 px above the bottom (a page drag in Firefox) and a drag at the top
        for y0 in [1180, 640, 10, 1255] {
            assert!(down(&mut e, 400, y0, 0).is_empty());
            assert!(mv(&mut e, 400, y0 - 300, 20).is_empty());
            assert!(e.tick(5000).is_empty());
            assert!(up(&mut e, 400, y0 - 300, 6000).is_empty());
            assert!(!e.active());
        }
    }

    #[test]
    fn zone_follows_the_orientation_and_a_new_press_restarts_cleanly() {
        let mut e = EdgeSwipe::new(AREA);
        e.set_area((1280, 800));
        assert!(
            down(&mut e, 600, 1270, 0).is_empty(),
            "portrait zone is not landscape's"
        );
        assert_eq!(down(&mut e, 600, 790, 5), vec![EdgeEvent::Started]);
        // a lost Up: the next Down restarts
        assert_eq!(down(&mut e, 600, 790, 9), vec![EdgeEvent::Started]);
        mv(&mut e, 600, 600, 20);
        assert_eq!(up(&mut e, 600, 600, 40), vec![EdgeEvent::Home]);
    }
}
