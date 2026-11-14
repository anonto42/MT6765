use serde_json::{json, Value};

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
