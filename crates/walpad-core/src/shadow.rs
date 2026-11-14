//! Shadow mode (M2.2): feed every input through the Brain and report what the Rust core WOULD do,
//! writing nothing. A thin wrapper that renders `Effect`s as `WOULD ...` lines.
use crate::brain::{Brain, Role};
use crate::logic::Ms;
use walpad_hw::battery::RawBattery;
use walpad_hw::evdev::InputEvent;

pub struct Shadow {
    brain: Brain,
}

fn lines(e: Vec<crate::brain::Effect>) -> Vec<String> {
    e.iter()
        .map(ToString::to_string)
        .filter(|s| !s.is_empty())
        .collect()
}

impl Shadow {
    pub fn new(devd: &str, keys: &str, touch: &str, backlight: Option<u32>, now: Ms) -> Self {
        Shadow {
            brain: Brain::new(devd, keys, touch, backlight, now),
        }
    }
    pub fn orientation(&self) -> u16 {
        self.brain.orientation()
    }
    pub fn on_event(&mut self, role: Role, ev: &InputEvent, now: Ms) -> Vec<String> {
        lines(self.brain.on_event(role, ev, now))
    }
    pub fn tick(
        &mut self,
        now: Ms,
        gsensor: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
        batt: &mut dyn FnMut() -> RawBattery,
    ) -> Vec<String> {
        lines(self.brain.tick(now, gsensor, batt))
    }
}

/// Replay a recorded stream: events are (role, event); time is the event timestamp. The loop ticks
/// every 40 ms between events like the live core. Returns (ms, line).
pub fn replay(
    sh: &mut Shadow,
    events: &[(Role, InputEvent)],
    until_ms: Ms,
    g: &mut dyn FnMut() -> Option<(i32, i32, i32)>,
) -> Vec<(Ms, String)> {
    const STEP: Ms = 40;
    let mut out = Vec::new();
    let mut b = || RawBattery {
        capacity: Some(80),
        status: Some("Charging".into()),
        mv: Some(4000),
        temp_dc: None,
        ..Default::default()
    };
    let Some(first) = events.first() else {
        return out;
    };
    let mut t = first.1.ms() as Ms;
    for (role, ev) in events {
        let et = ev.ms() as Ms;
        while t + STEP <= et {
            t += STEP;
            out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
        }
        out.extend(sh.on_event(*role, ev, et).into_iter().map(|l| (et, l)));
        t = t.max(et);
    }
    while t < until_ms {
        t += STEP;
        out.extend(sh.tick(t, g, &mut b).into_iter().map(|l| (t, l)));
    }
    out
}

#[cfg(test)]
mod tests;
