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
mod tests;
