//! Battery policy (parity: walpad-devd.py battery()/battery_guard()).
use serde::Deserialize;
use walpad_hw::battery::RawBattery;
use walpad_proto::{Battery, BatterySource};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Cfg {
    pub low_batt_mv: u32,
    pub critical_batt_mv: u32,
    /// Consecutive readings below critical before shutdown (decision D4: starts at 1).
    pub critical_readings: u32,
}
impl Default for Cfg {
    fn default() -> Self {
        Cfg {
            low_batt_mv: 3550,
            critical_batt_mv: 3350,
            critical_readings: 1,
        }
    }
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

pub fn model(raw: &RawBattery, low: bool) -> Battery {
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
    Battery {
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
}

impl Guard {
    pub fn low(&self) -> bool {
        self.low
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
mod tests {
    use super::*;

    fn raw(cap: Option<i32>, status: &str, mv: Option<u32>) -> RawBattery {
        RawBattery {
            capacity: cap,
            status: Some(status.into()),
            mv,
            temp_dc: Some(340),
            ..Default::default()
        }
    }

    #[test]
    fn charger_online_means_charging_and_the_estimate_drops_the_charge_voltage() {
        let mut r = raw(Some(-1), "Discharging", Some(4336));
        r.charger_online = true;
        r.current_ua = Some(685_500);
        let b = model(&r, false);
        assert_eq!(b.state.as_deref(), Some("Charging"));
        // 4336 mV - 0.12 ohm * 685 mA = about 4254 mV -> 95 %
        assert_eq!(b.percent, Some(95));
        r.status = Some("Full".into());
        assert_eq!(model(&r, false).percent, Some(100));
        r.charger_online = false;
        r.status = Some("Charging".into());
        r.mv = Some(4400);
        r.current_ua = None;
        assert_eq!(
            model(&r, false).percent,
            Some(99),
            "never 100 % before Full"
        );
    }

    #[test]
    fn rule1_estimate_table() {
        for (cap, mv, want) in [
            (Some(80), Some(3000), (Some(80), BatterySource::Gauge)),
            (
                Some(-1),
                Some(3920),
                (Some(62), BatterySource::VoltageEstimate),
            ),
            (
                Some(-1),
                Some(3300),
                (Some(0), BatterySource::VoltageEstimate),
            ),
            (
                Some(-1),
                Some(3200),
                (Some(0), BatterySource::VoltageEstimate),
            ),
            (
                Some(-1),
                Some(4400),
                (Some(100), BatterySource::VoltageEstimate),
            ),
            (
                Some(101),
                Some(3600),
                (Some(30), BatterySource::VoltageEstimate),
            ),
            (Some(-1), Some(2500), (None, BatterySource::Unknown)),
            (Some(-1), None, (None, BatterySource::Unknown)),
            (None, Some(3920), (Some(62), BatterySource::VoltageEstimate)),
        ] {
            assert_eq!(percent(cap, mv), want, "cap {cap:?} mv {mv:?}");
        }
    }

    #[test]
    fn rule2_critical() {
        let cfg = Cfg::default();
        let mut g = Guard::default();
        assert!(g
            .step(&raw(Some(-1), "Discharging", Some(3349)), &cfg)
            .contains(&Act::CriticalPoweroff { mv: 3349 }));
        let mut g = Guard::default();
        assert!(!g
            .step(&raw(Some(-1), "Charging", Some(3000)), &cfg)
            .iter()
            .any(|a| matches!(a, Act::CriticalPoweroff { .. })));
        assert!(!g
            .step(&raw(Some(-1), "Discharging", Some(3350)), &cfg)
            .iter()
            .any(|a| matches!(a, Act::CriticalPoweroff { .. })));
        assert!(!g
            .step(&raw(Some(-1), "Discharging", Some(0)), &cfg)
            .iter()
            .any(|a| matches!(a, Act::CriticalPoweroff { .. })));
        assert!(!g
            .step(&raw(Some(-1), "Discharging", None), &cfg)
            .iter()
            .any(|a| matches!(a, Act::CriticalPoweroff { .. })));
    }

    #[test]
    fn rule2_status_case_insensitive() {
        assert!(is_discharging(Some("DISCHARGING")));
        assert!(is_discharging(Some("Discharging")));
        assert!(!is_discharging(Some("Charging")));
        assert!(!is_discharging(None));
    }

    #[test]
    fn d4_consecutive_readings() {
        let cfg = Cfg {
            critical_readings: 3,
            ..Cfg::default()
        };
        let mut g = Guard::default();
        let low = raw(Some(-1), "Discharging", Some(3300));
        let crit = |a: &[Act]| a.iter().any(|x| matches!(x, Act::CriticalPoweroff { .. }));
        assert!(!crit(&g.step(&low, &cfg)));
        assert!(!crit(&g.step(&low, &cfg)));
        // a recovery resets the count
        g.step(&raw(Some(-1), "Discharging", Some(3700)), &cfg);
        assert!(!crit(&g.step(&low, &cfg)));
        assert!(!crit(&g.step(&low, &cfg)));
        assert!(crit(&g.step(&low, &cfg)));
    }

    #[test]
    fn rule3_low_flag_logs_on_change_only() {
        let cfg = Cfg::default();
        let mut g = Guard::default();
        assert_eq!(
            g.step(&raw(Some(-1), "Discharging", Some(3500)), &cfg),
            vec![Act::LowChanged(true)]
        );
        assert!(g
            .step(&raw(Some(-1), "Discharging", Some(3500)), &cfg)
            .is_empty());
        assert!(g.low());
        assert_eq!(
            g.step(&raw(Some(-1), "Charging", Some(3500)), &cfg),
            vec![Act::LowChanged(false)]
        );
    }

    #[test]
    fn model_marks_source_and_temperature() {
        let b = model(&raw(Some(-1), "Discharging", Some(3920)), false);
        assert_eq!(b.source, BatterySource::VoltageEstimate);
        assert_eq!(b.percent, Some(62));
        assert_eq!(b.temp_c, Some(34.0));
        let u = model(&RawBattery::default(), false);
        assert_eq!((u.percent, u.source), (None, BatterySource::Unknown));
    }
}
