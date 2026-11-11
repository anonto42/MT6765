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
use serde_json::{json, Value};

// ---- tunables (documented in docs/44-battery-soc.md) -------------------------------------
/// Internal resistance used when the gauge does not give a sane `ptim_resist_get`, in ohm.
pub const R_INT_DEFAULT_OHM: f32 = 0.2;
/// `ptim_resist_get` (milliohm) is only trusted inside this window.
pub const R_INT_SANE_MOHM: std::ops::RangeInclusive<u32> = 50..=800;
/// Time constant of the filter on the compensated open-circuit voltage, seconds.
pub const TAU_OCV_S: f32 = 180.0;
/// Time constant with which the coulomb-counted value is pulled to the voltage-derived one, seconds.
pub const TAU_CORRECT_S: f32 = 1800.0;
/// The voltage correction moves the estimate at most this many points per hour (calm indicator).
pub const MAX_CORRECT_PTS_PER_H: f32 = 12.0;
/// The shown percent moves at most one point per this many seconds.
pub const SLEW_S_PER_POINT: f32 = 60.0;
/// Time constant of the smoothed current used for the time estimates, seconds.
pub const TAU_CURRENT_S: f32 = 300.0;
/// Seconds of current data needed before a time estimate is given.
pub const MIN_TIME_DATA_S: f32 = 120.0;
/// A single reading above this multiple of the smoothed current is a spike and is ignored.
pub const SPIKE_FACTOR: f32 = 4.0;
/// Currents outside this window (amps, magnitude) are garbage / idle noise for the time estimate.
pub const I_VALID_A: std::ops::RangeInclusive<f32> = 0.02..=4.0;
/// Time estimates longer than this are not shown (an idle screen-off tablet, not a prediction).
pub const MAX_TIME_MIN: f32 = 72.0 * 60.0;
/// Charge current taper: above this percent the constant-voltage phase halves the current.
pub const CV_START_PCT: f32 = 80.0;
pub const CV_CURRENT_FACTOR: f32 = 0.5;
/// A persisted percent more than this far from the rested voltage is not trusted (charged while off).
pub const SEED_TRUST_PTS: f32 = 25.0;
/// A persisted value older than this is ignored.
pub const SEED_MAX_AGE_S: u64 = 7 * 24 * 3600;
/// Capacity fallbacks (mAh).
pub const CAPACITY_FALLBACK_MAH: f32 = 3000.0;
pub const CAPACITY_MIN_HINT_MAH: f32 = 1500.0;

/// Rested open-circuit voltage (mV) -> percent for a generic 4.2 V single-cell Li-ion/Li-poly.
const OCV: [(f32, f32); 22] = [
    (3300.0, 0.0),
    (3500.0, 2.0),
    (3610.0, 5.0),
    (3690.0, 10.0),
    (3710.0, 15.0),
    (3730.0, 20.0),
    (3750.0, 25.0),
    (3770.0, 30.0),
    (3790.0, 35.0),
    (3800.0, 40.0),
    (3820.0, 45.0),
    (3840.0, 50.0),
    (3850.0, 55.0),
    (3870.0, 60.0),
    (3910.0, 65.0),
    (3950.0, 70.0),
    (3980.0, 75.0),
    (4020.0, 80.0),
    (4080.0, 85.0),
    (4110.0, 90.0),
    (4150.0, 95.0),
    (4200.0, 100.0),
];

pub fn ocv_to_pct(mv: f32) -> f32 {
    if mv <= OCV[0].0 {
        return 0.0;
    }
    for w in OCV.windows(2) {
        let ((v0, p0), (v1, p1)) = (w[0], w[1]);
        if mv <= v1 {
            return p0 + (p1 - p0) * (mv - v0) / (v1 - v0);
        }
    }
    100.0
}

/// Capacity in mAh: the owner's `battery_mah` (from the label), else `charge_full` when plausible,
/// else a 3000 mAh default.
pub fn capacity_mah(cfg_mah: Option<u32>, charge_full_uah: Option<u64>) -> f32 {
    if let Some(m) = cfg_mah.filter(|m| (500..=30_000).contains(m)) {
        return m as f32;
    }
    match charge_full_uah.map(|u| u as f32 / 1000.0) {
        Some(m) if (CAPACITY_MIN_HINT_MAH..=30_000.0).contains(&m) => m,
        _ => CAPACITY_FALLBACK_MAH,
    }
}

pub fn r_int_ohm(mohm: Option<u32>) -> f32 {
    match mohm {
        Some(m) if R_INT_SANE_MOHM.contains(&m) => m as f32 / 1000.0,
        _ => R_INT_DEFAULT_OHM,
    }
}

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
pub const PERSIST_PATH: &str = "/var/lib/walpad/battery.json";

pub fn to_json(percent: u8, saved_unix: u64) -> String {
    json!({"percent": percent, "saved_unix": saved_unix}).to_string()
}

/// The saved percent when the file is well formed, in range and not stale; garbage -> None.
pub fn parse_persisted(s: &str, now_unix: u64) -> Option<u8> {
    let v: Value = serde_json::from_str(s).ok()?;
    let p = v.get("percent")?.as_u64().filter(|p| *p <= 100)? as u8;
    let saved = v.get("saved_unix")?.as_u64()?;
    // the clock may not be set yet at boot (1970): then the age cannot be judged and the value is used
    if now_unix > 1_600_000_000 && (saved > now_unix + 3600 || now_unix - saved > SEED_MAX_AGE_S) {
        return None;
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAP: f32 = 3000.0;
    fn inp(mv: u32, ma: i64, dir: Dir) -> Input {
        Input {
            mv: Some(mv),
            current_ua: Some(ma * 1000),
            dir,
            full: false,
            gauge: None,
            r_int_ohm: 0.28,
            capacity_mah: CAP,
        }
    }
    /// Runs `secs` seconds in 10 s steps; returns the shown percent at every step.
    fn run(e: &mut Estimator, t: &mut u64, secs: u64, f: impl Fn(u64) -> Input) -> Vec<u8> {
        let t0 = *t;
        let mut v = Vec::new();
        while *t < t0 + secs * 1000 {
            let o = e.step(*t, &f((*t - t0) / 1000));
            v.push(o.percent.unwrap());
            *t += 10_000;
        }
        v
    }
    /// Terminal voltage of a cell at rest voltage `ocv` under `ma` discharge.
    fn vt(ocv: f32, ma: f32) -> u32 {
        (ocv - ma / 1000.0 * 280.0) as u32
    }

    #[test]
    fn table_is_monotonic_and_clamped() {
        let mut last = -1.0;
        for mv in (3000..4400).step_by(7) {
            let p = ocv_to_pct(mv as f32);
            assert!(p >= last);
            last = p;
        }
        assert_eq!(ocv_to_pct(3000.0), 0.0);
        assert_eq!(ocv_to_pct(4400.0), 100.0);
        assert!((ocv_to_pct(3800.0) - 40.0).abs() < 0.01);
    }

    #[test]
    fn capacity_and_resistance_fallbacks() {
        assert_eq!(capacity_mah(Some(4000), Some(2946000)), 4000.0);
        assert_eq!(capacity_mah(None, Some(2946000)), 2946.0);
        assert_eq!(capacity_mah(None, Some(900_000)), 3000.0);
        assert_eq!(capacity_mah(Some(5), None), 3000.0);
        assert_eq!(r_int_ohm(Some(283)), 0.283);
        assert_eq!(r_int_ohm(Some(0)), 0.2);
        assert_eq!(r_int_ohm(Some(9999)), 0.2);
        assert_eq!(r_int_ohm(None), 0.2);
    }

    #[test]
    fn brightness_and_cpu_steps_do_not_make_the_percent_jump() {
        // true charge 60 % (OCV 3870); light load, then a 300 -> 900 mA step with extra sag, back again
        let mut e = Estimator::new(None);
        let mut t = 0;
        let mut all = Vec::new();
        all.extend(run(&mut e, &mut t, 600, |_| {
            inp(vt(3870.0, 300.0), 300, Dir::Discharging)
        }));
        all.extend(run(&mut e, &mut t, 600, |_| {
            // 600 mA more current and an extra 120 mV of sag the IR term does not explain
            inp(vt(3870.0, 900.0) - 120, 900, Dir::Discharging)
        }));
        all.extend(run(&mut e, &mut t, 600, |_| {
            inp(vt(3870.0, 300.0), 300, Dir::Discharging)
        }));
        assert!(all.windows(2).all(|w| w[1] <= w[0]), "never rises: {all:?}");
        // at most one point per minute (6 steps)
        for w in all.windows(7) {
            assert!(w[0] - w[6] <= 1, "{w:?}");
        }
        assert!((all[0] as i32 - 60).abs() <= 1);
        // real drain: 300 mA x10 min + 900 mA x10 min + 300 mA x10 min is about 8.5 % of 3000 mAh, plus at most 6 points of voltage correction
        assert!(all[0] - *all.last().unwrap() <= 15);
    }

    #[test]
    fn voltage_rebound_never_raises_the_percent_while_discharging() {
        let mut e = Estimator::new(None);
        let mut t = 0;
        run(&mut e, &mut t, 300, |_| {
            inp(vt(3800.0, 900.0), 900, Dir::Discharging)
        });
        let a = e.shown().unwrap();
        // screen goes dim: voltage rebounds by 200 mV
        let v = run(&mut e, &mut t, 1200, |_| {
            inp(vt(3800.0, 100.0) + 100, 100, Dir::Discharging)
        });
        assert!(v.iter().all(|p| *p <= a));
    }

    #[test]
    fn charging_rises_monotonically_and_waits_for_full() {
        let mut e = Estimator::new(None);
        let mut t = 0;
        let v = run(&mut e, &mut t, 3 * 3600, |s| {
            // rested voltage climbs 3700 -> 4200 over three hours, 700 mA
            let ocv = 3700.0 + 500.0 * s as f32 / 10_800.0;
            let mut i = inp((ocv + 0.7 * 280.0) as u32, 700, Dir::Charging);
            if s % 97 == 0 {
                i.mv = i.mv.map(|m| m - 60); // a noisy dip
            }
            i
        });
        assert!(v.windows(2).all(|w| w[1] >= w[0]), "{v:?}");
        assert!(*v.last().unwrap() <= 99, "never 100 before Full");
        let mut f = inp(4340, 50, Dir::Idle);
        f.full = true;
        assert_eq!(e.step(t, &f).percent, Some(100));
    }

    #[test]
    fn charging_never_falls_and_full_forces_100() {
        let mut e = Estimator::new(Some(50));
        let mut t = 0;
        let a = run(&mut e, &mut t, 120, |_| inp(4000, 700, Dir::Charging));
        let b = run(&mut e, &mut t, 600, |_| inp(3700, 700, Dir::Charging));
        assert!(b.iter().all(|p| *p >= *a.last().unwrap()));
        let mut f = inp(4300, 0, Dir::Idle);
        f.full = true;
        let o = e.step(t, &f);
        assert_eq!(o.percent, Some(100));
        assert_eq!(o.time_to_full_min, None);
        assert_eq!(o.time_left_min, None);
    }

    #[test]
    fn boot_seeds_from_the_persisted_value_when_it_agrees_with_the_voltage() {
        // rested 3870 mV = 60 %, saved 55 %: keep 55 and resync slowly
        let mut e = Estimator::new(Some(55));
        assert_eq!(e.step(0, &inp(3870, 0, Dir::Discharging)).percent, Some(55));
        // saved 90 % but the voltage says 60 %: charged/discharged while off, voltage wins
        let mut e = Estimator::new(Some(90));
        assert_eq!(e.step(0, &inp(3870, 0, Dir::Discharging)).percent, Some(60));
        // nothing saved
        let mut e = Estimator::new(None);
        assert_eq!(e.step(0, &inp(3870, 0, Dir::Discharging)).percent, Some(60));
        // no voltage and no seed: unavailable
        let mut e = Estimator::new(None);
        let mut i = inp(0, 0, Dir::Discharging);
        i.mv = None;
        assert_eq!(e.step(0, &i).percent, None);
    }

    #[test]
    fn persisted_file_garbage_and_stale_are_ignored() {
        let now = 1_800_000_000;
        assert_eq!(parse_persisted(&to_json(42, now - 100), now), Some(42));
        assert_eq!(parse_persisted("", now), None);
        assert_eq!(parse_persisted("{not json", now), None);
        assert_eq!(
            parse_persisted(r#"{"percent":150,"saved_unix":1800000000}"#, now),
            None
        );
        assert_eq!(
            parse_persisted(r#"{"percent":-3,"saved_unix":1800000000}"#, now),
            None
        );
        assert_eq!(
            parse_persisted(r#"{"percent":"x","saved_unix":1}"#, now),
            None
        );
        assert_eq!(parse_persisted(r#"{"percent":50}"#, now), None);
        assert_eq!(
            parse_persisted(&to_json(42, now - SEED_MAX_AGE_S - 1), now),
            None
        );
        assert_eq!(parse_persisted(&to_json(42, now + 999_999), now), None);
        // clock not set yet: age cannot be judged
        assert_eq!(parse_persisted(&to_json(42, now), 100), Some(42));
    }

    #[test]
    fn a_working_gauge_wins() {
        let mut e = Estimator::new(Some(10));
        let mut i = inp(3870, 300, Dir::Discharging);
        i.gauge = Some(77);
        assert_eq!(e.step(0, &i).percent, Some(77));
        i.gauge = Some(76);
        assert_eq!(e.step(10_000, &i).percent, Some(76));
    }

    #[test]
    fn time_left_needs_data_then_uses_the_smoothed_current() {
        let mut e = Estimator::new(None);
        let mut t = 0;
        // 60 % of 3000 mAh = 1800 mAh at 400 mA = 4.5 h
        let o = e.step(t, &inp(vt(3870.0, 400.0), 400, Dir::Discharging));
        assert!(o.time_calculating && o.time_left_min.is_none());
        t += 10_000;
        run(&mut e, &mut t, 300, |_| {
            inp(vt(3870.0, 400.0), 400, Dir::Discharging)
        });
        let o = e.step(t, &inp(vt(3870.0, 400.0), 400, Dir::Discharging));
        assert!(!o.time_calculating);
        let m = o.time_left_min.unwrap() as i32;
        assert!((m - 270).abs() < 15, "{m}");
        // a single 5 A spike and a zero reading do not move it
        let o2 = e.step(t + 10_000, &inp(3000, 5000, Dir::Discharging));
        let o3 = e.step(t + 20_000, &inp(vt(3870.0, 0.0), 0, Dir::Discharging));
        assert!((o2.time_left_min.unwrap() as i32 - m).abs() < 15);
        assert!((o3.time_left_min.unwrap() as i32 - m).abs() < 15);
    }

    #[test]
    fn absurd_times_are_hidden_and_garbage_current_means_unknown() {
        let mut e = Estimator::new(None);
        let mut t = 0;
        // 21 mA idle: 1800 mAh / 21 mA is far above 72 h -> hidden
        run(&mut e, &mut t, 300, |_| inp(3870, 21, Dir::Discharging));
        assert_eq!(
            e.step(t, &inp(3870, 21, Dir::Discharging)).time_left_min,
            None
        );
        let mut e = Estimator::new(None);
        let mut i = inp(3870, 0, Dir::Discharging);
        i.current_ua = None;
        let mut t = 0;
        for _ in 0..40 {
            assert_eq!(e.step(t, &i).time_left_min, None);
            t += 10_000;
        }
    }

    #[test]
    fn time_to_full_accounts_for_the_taper() {
        let mut e = Estimator::new(Some(50));
        let mut t = 0;
        run(&mut e, &mut t, 400, |_| inp(3950 + 190, 700, Dir::Charging));
        let o = e.step(t, &inp(3950 + 190, 700, Dir::Charging));
        let m = o.time_to_full_min.unwrap() as f32;
        let p = o.percent.unwrap() as f32;
        let plain = (100.0 - p) / 100.0 * CAP / 700.0 * 60.0;
        assert!(m > plain * 1.1 && m < plain * 2.0, "{m} vs {plain}");
        assert_eq!(o.time_left_min, None);
    }

    #[test]
    fn direction_change_resets_the_time_data() {
        let mut e = Estimator::new(None);
        let mut t = 0;
        run(&mut e, &mut t, 300, |_| inp(3900, 400, Dir::Discharging));
        let o = e.step(t, &inp(4100, 700, Dir::Charging));
        assert!(o.time_calculating);
        assert_eq!(o.time_left_min, None);
    }
}
