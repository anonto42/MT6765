//! Battery policy (parity: walpad-devd.py battery()/battery_guard()).
use serde::Deserialize;
use walpad_hw::battery::RawBattery;
use walpad_proto::{Battery, BatterySource};

use super::soc;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub low_batt_mv: u32,
    pub critical_batt_mv: u32,
    /// Consecutive readings below critical before shutdown (decision D4: starts at 1).
    pub critical_readings: u32,
    /// Battery capacity in mAh from the label on the battery (owner request 2026-11-11); used for the
    /// time estimates. Unset or implausible: `charge_full` when >= 1500 mAh, else 3000.
    pub battery_mah: Option<u32>,
    /// Power-path / charging mode: "smart" (default), "bypass", "eco", "fast".
    pub charge_mode: String,
    /// Stop charging and run on charger power once battery reaches this % (50-100, default 85).
    pub charge_limit_pct: u8,
    /// Maximum charger input current in mA (500, 1000, 1500, 2000; default 1500).
    pub charge_current_ma: u32,
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            low_batt_mv: 3550,
            critical_batt_mv: 3350,
            critical_readings: 1,
            battery_mah: None,
            charge_mode: "smart".into(),
            charge_limit_pct: 85,
            charge_current_ma: 1500,
        }
    }
}

/// Hardware charger configuration computed by the power-path / battery-protection policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargerTarget {
    /// Input current limit in microamps (`100_000` for power-path hold, `500_000`..=`2_000_000`, or `-1`).
    pub input_limit_ua: i64,
    /// Disable battery charging completely (`true` only when strictly above limit + hysteresis).
    pub disable_charging: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Log, sync, `poweroff -f`.
    CriticalPoweroff {
        mv: u32,
    },
    LowChanged(bool),
}

pub fn is_discharging(status: Option<&str>) -> bool {
    status.is_some_and(|s| s.to_ascii_lowercase().starts_with("dischar"))
}

/// Rule 1: if the gauge is outside 0-100 and voltage > 2500 mV, estimate from voltage.
pub fn percent(capacity: Option<i32>, mv: Option<u32>) -> (Option<u8>, BatterySource) {
    match capacity {
        Some(c) if (0..=100).contains(&c) => (Some(c as u8), BatterySource::Gauge),
        _ => match mv {
            Some(mv) if mv > 2500 => {
                let p = ((mv as i64 - 3300) * 100 / 1000).clamp(0, 100);
                (Some(p as u8), BatterySource::VoltageEstimate)
            }
            _ => (None, BatterySource::Unknown),
        },
    }
}

/// The state to show: a charger that reports `online` means Charging even when the battery status lags or
/// says "Discharging"/"Not charging" (chargers the kernel does not classify as standard); "Full" stays.
pub fn effective_status(raw: &RawBattery) -> Option<String> {
    let s = raw.status.as_deref().unwrap_or("");
    let full = s.to_ascii_lowercase().starts_with("full");
    if raw.charger_online && !full {
        return Some("Charging".into());
    }
    raw.status.clone()
}

/// Estimated percent while charging: the terminal voltage sits above the resting voltage by the charge
/// current times the internal resistance (about 0.12 ohm for this pack), so subtract that, and never claim
/// 100 % until the battery itself says Full.
pub fn charging_percent(mv: u32, current_ua: Option<i64>, full: bool) -> u8 {
    let drop_mv = current_ua.map_or(0, |c| (c.max(0) * 12 / 100_000) as u32); // 0.12 ohm: mA * 0.12
    let p = ((mv.saturating_sub(drop_mv) as i64 - 3300) * 100 / 1000).clamp(0, 100) as u8;
    if full {
        100
    } else {
        p.min(99)
    }
}

/// Input for the estimator from one raw reading.
pub fn soc_input(raw: &RawBattery, cfg: &Cfg) -> soc::Input {
    let state = effective_status(raw);
    let s = state.as_deref().unwrap_or("").to_ascii_lowercase();
    let full = s.starts_with("full");
    let dir = if s.starts_with("charg") {
        soc::Dir::Charging
    } else if s.starts_with("dischar") {
        soc::Dir::Discharging
    } else {
        soc::Dir::Idle
    };
    soc::Input {
        mv: raw.mv,
        current_ua: raw.current_ua,
        dir,
        full,
        gauge: raw
            .capacity
            .filter(|c| (0..=100).contains(c))
            .map(|c| c as u8),
        r_int_ohm: soc::r_int_ohm(raw.r_int_mohm),
        capacity_mah: soc::capacity_mah(cfg.battery_mah, raw.charge_full_uah),
    }
}

/// The battery model. `est` is the smoothed estimator output; without it (tests, no estimator yet)
/// the percent is the instantaneous voltage estimate.
pub fn model(raw: &RawBattery, low: bool, est: Option<&soc::Output>) -> Battery {
    let (mut p, source) = percent(raw.capacity, raw.mv);
    let state = effective_status(raw);
    let charging = state
        .as_deref()
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("charg"));
    let full = state
        .as_deref()
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("full"));
    if source == BatterySource::VoltageEstimate && (charging || full) {
        if let Some(mv) = raw.mv {
            p = Some(charging_percent(mv, raw.current_ua, full));
        }
    }
    if let Some(e) = est {
        if source == BatterySource::VoltageEstimate && e.percent.is_some() {
            p = e.percent;
        }
    }
    let e = est.copied().unwrap_or_default();
    Battery {
        time_left_min: e.time_left_min,
        time_to_full_min: e.time_to_full_min,
        time_calculating: e.time_calculating,
        percent: p,
        source,
        state,
        mv: raw.mv,
        temp_c: raw.temp_dc.map(|t| t as f32 / 10.0),
        low,
    }
}

#[derive(Debug, Default)]
pub struct Guard {
    low: bool,
    below_critical: u32,
    paused_for_limit: bool,
    last_charger: Option<ChargerTarget>,
}

impl Guard {
    pub fn low(&self) -> bool {
        self.low
    }
    pub fn paused_for_limit(&self) -> bool {
        self.paused_for_limit
    }
    pub fn invalidate_charger(&mut self) {
        self.last_charger = None;
    }
    /// Compute the desired ETA6937 power-path / charger current target and return `Some` when changed.
    pub fn charger_step(
        &mut self,
        raw: &RawBattery,
        pct: Option<u8>,
        cfg: &Cfg,
    ) -> Option<ChargerTarget> {
        let target = if !raw.charger_online {
            self.paused_for_limit = false;
            ChargerTarget {
                input_limit_ua: -1,
                disable_charging: false,
            }
        } else if cfg.charge_mode == "bypass" {
            self.paused_for_limit = true;
            let over = pct.is_some_and(|p| p >= cfg.charge_limit_pct.saturating_add(2));
            ChargerTarget {
                input_limit_ua: 100_000,
                disable_charging: over,
            }
        } else {
            let limit = cfg.charge_limit_pct.clamp(50, 100);
            if limit < 100 {
                if pct.is_some_and(|p| p >= limit) {
                    self.paused_for_limit = true;
                } else if pct.is_some_and(|p| p <= limit.saturating_sub(3)) {
                    self.paused_for_limit = false;
                }
            } else {
                self.paused_for_limit = false;
            }
            if self.paused_for_limit {
                let strict_cut = pct.is_some_and(|p| p >= limit.saturating_add(2))
                    || raw.mv.is_some_and(|mv| mv >= 4340 && limit < 100);
                ChargerTarget {
                    input_limit_ua: 100_000,
                    disable_charging: strict_cut,
                }
            } else {
                let mut ma = match cfg.charge_mode.as_str() {
                    "eco" => cfg.charge_current_ma.clamp(500, 1000),
                    "fast" => 2000,
                    _ => cfg.charge_current_ma.clamp(500, 2000),
                };
                // Thermal battery protection: step down current when battery warms up so the
                // charger still supplies system power (VSYS) without overheating the cell.
                if raw.temp_dc.is_some_and(|t| t >= 410) {
                    ma = ma.min(500);
                } else if raw.temp_dc.is_some_and(|t| t >= 380) {
                    ma = ma.min(1000);
                }
                ChargerTarget {
                    input_limit_ua: i64::from(ma) * 1000,
                    disable_charging: false,
                }
            }
        };
        if self.last_charger != Some(target) {
            self.last_charger = Some(target);
            Some(target)
        } else {
            None
        }
    }
    /// Called every 10 s (rule 1).
    pub fn step(&mut self, raw: &RawBattery, cfg: &Cfg) -> Vec<Act> {
        let mut out = Vec::new();
        let discharging = is_discharging(effective_status(raw).as_deref());
        let mv = raw.mv.unwrap_or(0);
        // Rule 2: critical.
        if discharging && mv > 0 && mv < cfg.critical_batt_mv {
            self.below_critical += 1;
            if self.below_critical >= cfg.critical_readings.max(1) {
                out.push(Act::CriticalPoweroff { mv });
            }
        } else {
            self.below_critical = 0;
        }
        // Rule 3: low flag, report only on change.
        let low = discharging && mv > 0 && mv < cfg.low_batt_mv;
        if low != self.low {
            self.low = low;
            out.push(Act::LowChanged(low));
        }
        out
    }
}

#[cfg(test)]
mod tests;
