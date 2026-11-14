//! Software battery state-of-charge and time-left estimator (owner request 2026-11-11).
//!
//! The stock MediaTek gauge is not initialised (`capacity` = -1), so the percent is estimated:
//! * the terminal voltage is corrected for the load, `V_ocv = V -/+ I * R_int` (IR compensation),
//!   then low-pass filtered with a long time constant and mapped through a rested-voltage table;
//! * between voltage corrections the estimate moves by coulomb counting (integrating `current_now`
//!   over the battery capacity), and is pulled slowly towards the voltage-derived value (a
//!   complementary filter: current is smooth in the short term, voltage cannot drift);
//! * the shown whole percent is monotonic in the charge direction (never rises while discharging,
//!   never falls while charging), slew-limited and seeded from the value saved before the last
//!   shutdown. `Full` forces 100; charging never shows 100 before `Full`.
//!
//! Design and sources: docs/44-battery-soc.md. All inputs are optional; nothing is guessed.

mod ocv;
#[cfg(test)]
mod tests;

pub use ocv::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Charging,
    Discharging,
    Idle,
}

#[derive(Debug, Clone, Copy)]
pub struct Input {
    pub mv: Option<u32>,
    /// Magnitude is used; the sign comes from `dir` (kernels differ).
    pub current_ua: Option<i64>,
    pub dir: Dir,
    pub full: bool,
    /// A working gauge percent (0-100) wins over the estimate.
    pub gauge: Option<u8>,
    pub r_int_ohm: f32,
    pub capacity_mah: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Output {
    pub percent: Option<u8>,
    pub time_left_min: Option<u32>,
    pub time_to_full_min: Option<u32>,
    /// Direction known but not enough current data yet.
    pub time_calculating: bool,
}

#[derive(Debug, Default)]
pub struct Estimator {
    seed: Option<u8>,
    last_ms: Option<u64>,
    ocv_f: Option<f32>,
    est: Option<f32>,
    shown: Option<u8>,
    since_move_s: f32,
    i_avg: Option<f32>,
    i_secs: f32,
    i_dir: Option<Dir>,
}

impl Estimator {
    /// `persisted`: the last shown percent from before the shutdown (already age-checked).
    pub fn new(persisted: Option<u8>) -> Self {
        Estimator {
            seed: persisted.filter(|p| *p <= 100),
            ..Default::default()
        }
    }

    pub fn shown(&self) -> Option<u8> {
        self.shown
    }

    pub fn step(&mut self, now_ms: u64, inp: &Input) -> Output {
        let dt = self.last_ms.map_or(0.0, |l| {
            (now_ms.saturating_sub(l) as f32 / 1000.0).min(120.0)
        });
        self.last_ms = Some(now_ms);
        self.since_move_s += dt;
        let i_a = inp
            .current_ua
            .map(|c| c.unsigned_abs() as f32 / 1e6)
            .filter(|i| *i <= 6.0);
        let sign = match inp.dir {
            Dir::Charging => 1.0,
            Dir::Discharging => -1.0,
            Dir::Idle => 0.0,
        };

        if let Some(g) = inp.gauge.filter(|g| *g <= 100) {
            self.est = Some(g as f32);
            self.shown = Some(g);
        } else if let Some(mv) = inp.mv.filter(|m| *m > 2500) {
            let ocv_raw = mv as f32 - sign * i_a.unwrap_or(0.0) * inp.r_int_ohm * 1000.0;
            let a = if self.ocv_f.is_some() {
                1.0 - (-dt / TAU_OCV_S).exp()
            } else {
                1.0
            };
            let f = self.ocv_f.map_or(ocv_raw, |o| o + (ocv_raw - o) * a);
            self.ocv_f = Some(f);
            let target = ocv_to_pct(f);
            match self.est {
                None => {
                    let e = match self.seed {
                        Some(p) if (p as f32 - target).abs() <= SEED_TRUST_PTS => p as f32,
                        _ => target,
                    };
                    self.est = Some(e);
                    self.shown = Some(e.round() as u8);
                }
                Some(e) => {
                    let cc = match (i_a, inp.capacity_mah > 0.0) {
                        (Some(i), true) => {
                            sign * i * dt / 3600.0 / (inp.capacity_mah / 1000.0) * 100.0
                        }
                        _ => 0.0,
                    };
                    let k = 1.0 - (-dt / TAU_CORRECT_S).exp();
                    let max_corr = MAX_CORRECT_PTS_PER_H * dt / 3600.0;
                    let corr = ((target - e - cc) * k).clamp(-max_corr, max_corr);
                    self.est = Some((e + cc + corr).clamp(0.0, 100.0));
                }
            }
        }
        // whole-percent policy
        if let (Some(e), Some(shown)) = (self.est, self.shown) {
            let want = e.round().clamp(0.0, 100.0) as u8;
            let mut s = shown;
            if inp.gauge.is_none() && self.since_move_s >= SLEW_S_PER_POINT {
                match inp.dir {
                    Dir::Discharging if want < s => s -= 1,
                    Dir::Charging if want > s => s += 1,
                    _ => {}
                }
                if s != shown {
                    self.since_move_s = 0.0;
                }
            }
            if inp.full {
                s = 100;
                self.est = Some(100.0);
            } else if inp.dir == Dir::Charging && inp.gauge.is_none() {
                s = s.min(99);
                self.est = Some(e.min(99.0));
            }
            self.shown = Some(s);
        }
        self.time(dt, i_a, inp)
    }

    fn time(&mut self, dt: f32, i_a: Option<f32>, inp: &Input) -> Output {
        let percent = self
            .shown
            .filter(|_| inp.gauge.is_some() || inp.mv.is_some());
        if self.i_dir != Some(inp.dir) {
            self.i_dir = Some(inp.dir);
            self.i_avg = None;
            self.i_secs = 0.0;
        }
        let mut out = Output {
            percent,
            ..Default::default()
        };
        if inp.dir == Dir::Idle || inp.full || percent.is_none() {
            return out;
        }
        if let Some(i) = i_a.filter(|i| I_VALID_A.contains(i)) {
            match self.i_avg {
                None => self.i_avg = Some(i),
                Some(a) if i > a * SPIKE_FACTOR && self.i_secs >= MIN_TIME_DATA_S => {}
                Some(a) => {
                    self.i_avg = Some(a + (i - a) * (1.0 - (-dt / TAU_CURRENT_S).exp()));
                }
            }
            self.i_secs += dt;
        }
        let (Some(avg), Some(p)) = (self.i_avg, percent) else {
            out.time_calculating = true;
            return out;
        };
        if self.i_secs < MIN_TIME_DATA_S {
            out.time_calculating = true;
            return out;
        }
        let cap = inp.capacity_mah;
        let p = p as f32;
        let minutes = match inp.dir {
            Dir::Discharging => p / 100.0 * cap / (avg * 1000.0) * 60.0,
            _ => {
                let cc_part = (CV_START_PCT - p).max(0.0).min(100.0 - p) / 100.0 * cap;
                let cv_part = (100.0 - p.max(CV_START_PCT)).max(0.0) / 100.0 * cap;
                (cc_part / (avg * 1000.0) + cv_part / (avg * 1000.0 * CV_CURRENT_FACTOR)) * 60.0
            }
        };
        if minutes.is_finite() && minutes <= MAX_TIME_MIN {
            let m = Some(minutes.round().max(1.0) as u32);
            if inp.dir == Dir::Discharging {
                out.time_left_min = m;
            } else {
                out.time_to_full_min = m;
            }
        }
        out
    }
}

// ---- persistence -------------------------------------------------------------------------
