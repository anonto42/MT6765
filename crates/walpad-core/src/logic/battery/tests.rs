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
    let b = model(&r, false, None);
    assert_eq!(b.state.as_deref(), Some("Charging"));
    // 4336 mV - 0.12 ohm * 685 mA = about 4254 mV -> 95 %
    assert_eq!(b.percent, Some(95));
    r.status = Some("Full".into());
    assert_eq!(model(&r, false, None).percent, Some(100));
    r.charger_online = false;
    r.status = Some("Charging".into());
    r.mv = Some(4400);
    r.current_ua = None;
    assert_eq!(
        model(&r, false, None).percent,
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
fn model_uses_the_estimator_output_for_the_percent_and_times() {
    let r = raw(Some(-1), "Discharging", Some(3920));
    let e = soc::Output {
        percent: Some(47),
        time_left_min: Some(300),
        ..Default::default()
    };
    let b = model(&r, false, Some(&e));
    assert_eq!((b.percent, b.time_left_min), (Some(47), Some(300)));
    assert_eq!(b.source, BatterySource::VoltageEstimate);
    // a working gauge is never replaced
    let b = model(&raw(Some(80), "Discharging", Some(3920)), false, Some(&e));
    assert_eq!((b.percent, b.source), (Some(80), BatterySource::Gauge));
    // charger online forces the Charging direction for the estimator
    let mut c = raw(Some(-1), "Discharging", Some(4100));
    c.charger_online = true;
    assert_eq!(soc_input(&c, &Cfg::default()).dir, soc::Dir::Charging);
}

#[test]
fn model_marks_source_and_temperature() {
    let b = model(&raw(Some(-1), "Discharging", Some(3920)), false, None);
    assert_eq!(b.source, BatterySource::VoltageEstimate);
    assert_eq!(b.percent, Some(62));
    assert_eq!(b.temp_c, Some(34.0));
    let u = model(&RawBattery::default(), false, None);
    assert_eq!((u.percent, u.source), (None, BatterySource::Unknown));
}

#[test]
fn charger_policy_smart_limit_bypass_and_thermal() {
    let mut g = Guard::default();
    let mut cfg = Cfg::default(); // smart, 85%, 1500 mA
    let mut r = raw(Some(-1), "Charging", Some(4000));
    r.charger_online = true;
    // 60% at 34.0 C -> 1500 mA power-path + charge
    assert_eq!(
        g.charger_step(&r, Some(60), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 1_500_000,
            disable_charging: false,
        })
    );
    // Unchanged on next poll -> None
    assert_eq!(g.charger_step(&r, Some(60), &cfg), None);
    // Warm battery (39.0 C) -> steps down to 1000 mA
    r.temp_dc = Some(390);
    assert_eq!(
        g.charger_step(&r, Some(60), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 1_000_000,
            disable_charging: false,
        })
    );
    r.temp_dc = Some(340);
    // Reaches 85% limit -> enters 100 mA power-path hold (charger runs device, battery protected)
    assert_eq!(
        g.charger_step(&r, Some(85), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 100_000,
            disable_charging: false,
        })
    );
    // Creeps to 87% -> strict cut
    assert_eq!(
        g.charger_step(&r, Some(87), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 100_000,
            disable_charging: true,
        })
    );
    // Drops to 82% -> resumes normal 1500 mA charging
    assert_eq!(
        g.charger_step(&r, Some(82), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 1_500_000,
            disable_charging: false,
        })
    );
    // Bypass mode at 50% -> holds 100 mA power-path
    cfg.charge_mode = "bypass".into();
    assert_eq!(
        g.charger_step(&r, Some(50), &cfg),
        Some(ChargerTarget {
            input_limit_ua: 100_000,
            disable_charging: false,
        })
    );
}
