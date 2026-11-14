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
