//! Tap / long-press / drag recogniser for a single pointer (the touch bridge injects button 1).
//! Rules (docs/rust/UI.md): tap = release within 10 px and 500 ms; long press = 500 ms held within
//! 10 px; a drag beyond 10 px scrolls (no inertia). Pure: time is passed in.
use walpad_ui::tokens::{LONG_PRESS_MS, SLOP_PX, TAP_MAX_MS};

pub type Ms = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Down { x: i32, y: i32 },
    Move { x: i32, y: i32 },
    Up { x: i32, y: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// Finger down: the pressed state is drawn on the next frame.
    Pressed {
        x: i32,
        y: i32,
    },
    /// The press turned into a drag or long press (or was cancelled): remove the pressed state.
    Cancel,
    Tap {
        x: i32,
        y: i32,
    },
    LongPress {
        x: i32,
        y: i32,
    },
    DragStart {
        x: i32,
        y: i32,
    },
    /// Movement since the previous event (positive dy = finger moved down).
    Drag {
        dx: i32,
        dy: i32,
    },
    DragEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Down,
    LongFired,
    Dragging,
}

#[derive(Debug)]
pub struct Recogniser {
    state: State,
    start: (i32, i32),
    start_ms: Ms,
    last: (i32, i32),
}

impl Default for Recogniser {
    fn default() -> Self {
        Self::new()
    }
}

fn beyond_slop(a: (i32, i32), b: (i32, i32)) -> bool {
    (a.0 - b.0).abs() > SLOP_PX || (a.1 - b.1).abs() > SLOP_PX
}

impl Recogniser {
    pub fn new() -> Self {
        Recogniser {
            state: State::Idle,
            start: (0, 0),
            start_ms: 0,
            last: (0, 0),
        }
    }

    pub fn feed(&mut self, input: Input, now: Ms) -> Vec<Gesture> {
        let mut out = Vec::new();
        match (self.state, input) {
            (_, Input::Down { x, y }) => {
                self.state = State::Down;
                self.start = (x, y);
                self.last = (x, y);
                self.start_ms = now;
                out.push(Gesture::Pressed { x, y });
            }
            (State::Down, Input::Move { x, y }) => {
                if beyond_slop(self.start, (x, y)) {
                    self.state = State::Dragging;
                    out.push(Gesture::Cancel);
                    out.push(Gesture::DragStart {
                        x: self.start.0,
                        y: self.start.1,
                    });
                    out.push(Gesture::Drag {
                        dx: x - self.start.0,
                        dy: y - self.start.1,
                    });
                    self.last = (x, y);
                }
            }
            (State::Dragging, Input::Move { x, y }) => {
                let (dx, dy) = (x - self.last.0, y - self.last.1);
                if dx != 0 || dy != 0 {
                    out.push(Gesture::Drag { dx, dy });
                }
                self.last = (x, y);
            }
            (State::Down, Input::Up { x, y }) => {
                let held = now.saturating_sub(self.start_ms);
                if beyond_slop(self.start, (x, y)) {
                    out.push(Gesture::Cancel);
                } else if held >= LONG_PRESS_MS {
                    // the timer did not get a chance to fire: it is still a long press
                    out.push(Gesture::Cancel);
                    out.push(Gesture::LongPress {
                        x: self.start.0,
                        y: self.start.1,
                    });
                } else if held <= TAP_MAX_MS {
                    out.push(Gesture::Tap { x, y });
                }
                self.state = State::Idle;
            }
            (State::Dragging, Input::Up { .. }) => {
                out.push(Gesture::DragEnd);
                self.state = State::Idle;
            }
            (State::LongFired, Input::Up { .. }) => self.state = State::Idle,
            (State::LongFired, Input::Move { .. }) | (State::Idle, _) => {}
        }
        out
    }

    /// Fires the long press once the finger has been held still for 500 ms.
    pub fn tick(&mut self, now: Ms) -> Vec<Gesture> {
        if self.state == State::Down && now.saturating_sub(self.start_ms) >= LONG_PRESS_MS {
            self.state = State::LongFired;
            return vec![
                Gesture::Cancel,
                Gesture::LongPress {
                    x: self.start.0,
                    y: self.start.1,
                },
            ];
        }
        Vec::new()
    }

    /// When `tick` next needs to run (for the poll timeout).
    pub fn next_deadline(&self) -> Option<Ms> {
        (self.state == State::Down).then(|| self.start_ms + LONG_PRESS_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn down(r: &mut Recogniser, x: i32, y: i32, t: Ms) -> Vec<Gesture> {
        r.feed(Input::Down { x, y }, t)
    }

    #[test]
    fn tap_within_slop_and_time() {
        let mut r = Recogniser::new();
        assert_eq!(
            down(&mut r, 100, 100, 0),
            vec![Gesture::Pressed { x: 100, y: 100 }]
        );
        assert_eq!(
            r.feed(Input::Up { x: 105, y: 96 }, 200),
            vec![Gesture::Tap { x: 105, y: 96 }]
        );
        assert_eq!(r.next_deadline(), None);
    }

    #[test]
    fn slop_boundary_is_10_px() {
        let mut r = Recogniser::new();
        down(&mut r, 0, 0, 0);
        assert_eq!(
            r.feed(Input::Move { x: 10, y: 10 }, 10),
            vec![],
            "exactly 10 px is still a press"
        );
        assert_eq!(
            r.feed(Input::Up { x: 10, y: 10 }, 20),
            vec![Gesture::Tap { x: 10, y: 10 }]
        );
        down(&mut r, 0, 0, 100);
        let g = r.feed(Input::Move { x: 11, y: 0 }, 110);
        assert_eq!(
            g,
            vec![
                Gesture::Cancel,
                Gesture::DragStart { x: 0, y: 0 },
                Gesture::Drag { dx: 11, dy: 0 }
            ]
        );
    }

    #[test]
    fn long_press_fires_once_at_500ms_and_release_is_silent() {
        let mut r = Recogniser::new();
        down(&mut r, 50, 60, 1000);
        assert_eq!(r.next_deadline(), Some(1500));
        assert!(r.tick(1499).is_empty());
        assert_eq!(
            r.tick(1500),
            vec![Gesture::Cancel, Gesture::LongPress { x: 50, y: 60 }]
        );
        assert!(r.tick(1600).is_empty(), "fires once");
        assert!(
            r.feed(Input::Move { x: 200, y: 200 }, 1700).is_empty(),
            "after a long press, moving does not scroll"
        );
        assert!(r.feed(Input::Up { x: 200, y: 200 }, 1800).is_empty());
    }

    #[test]
    fn slow_release_without_a_tick_still_counts_as_long_press() {
        let mut r = Recogniser::new();
        down(&mut r, 5, 5, 0);
        assert_eq!(
            r.feed(Input::Up { x: 5, y: 5 }, 700),
            vec![Gesture::Cancel, Gesture::LongPress { x: 5, y: 5 }]
        );
    }

    #[test]
    fn tap_boundary_is_500ms() {
        let mut r = Recogniser::new();
        down(&mut r, 5, 5, 0);
        assert_eq!(
            r.feed(Input::Up { x: 5, y: 5 }, 499),
            vec![Gesture::Tap { x: 5, y: 5 }]
        );
    }

    #[test]
    fn drag_scrolls_with_deltas_and_ends_on_release() {
        let mut r = Recogniser::new();
        down(&mut r, 100, 500, 0);
        let first = r.feed(Input::Move { x: 100, y: 450 }, 20);
        assert_eq!(first[2], Gesture::Drag { dx: 0, dy: -50 });
        assert_eq!(
            r.feed(Input::Move { x: 100, y: 430 }, 40),
            vec![Gesture::Drag { dx: 0, dy: -20 }]
        );
        assert!(
            r.feed(Input::Move { x: 100, y: 430 }, 50).is_empty(),
            "no movement, no event"
        );
        assert_eq!(
            r.feed(Input::Up { x: 100, y: 430 }, 60),
            vec![Gesture::DragEnd]
        );
        // a long hold while dragging never becomes a long press
        let mut r = Recogniser::new();
        down(&mut r, 0, 0, 0);
        r.feed(Input::Move { x: 0, y: 40 }, 10);
        assert!(r.tick(5000).is_empty());
    }

    #[test]
    fn stray_events_when_idle_are_ignored_and_new_press_restarts() {
        let mut r = Recogniser::new();
        assert!(r.feed(Input::Move { x: 1, y: 1 }, 0).is_empty());
        assert!(r.feed(Input::Up { x: 1, y: 1 }, 1).is_empty());
        down(&mut r, 1, 1, 2);
        // a second Down without an Up (lost event) restarts cleanly
        assert_eq!(down(&mut r, 9, 9, 3), vec![Gesture::Pressed { x: 9, y: 9 }]);
        assert_eq!(
            r.feed(Input::Up { x: 9, y: 9 }, 50),
            vec![Gesture::Tap { x: 9, y: 9 }]
        );
    }
}
