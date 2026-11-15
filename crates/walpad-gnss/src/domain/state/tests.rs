use super::*;
use crate::nmea::with_checksum;
use crate::wire::{LOC_HAS_ACCURACY, LOC_HAS_ALTITUDE, LOC_HAS_LAT_LONG};
use serde_json::Value;

fn loc(lat: f64, lon: f64) -> Msg {
    Msg::Location(Location {
        flags: LOC_HAS_LAT_LONG | LOC_HAS_ALTITUDE | LOC_HAS_ACCURACY,
        lat,
        lon,
        alt: 10.0,
        speed: 0.0,
        bearing: 0.0,
        accuracy: Some(5.0),
        time_ms: 1_700_000_000_000,
    })
}

#[test]
fn off_until_started_then_searching_then_fix_then_stale() {
    let mut t = Tracker::new();
    assert_eq!(t.snapshot(0).state, State::Off);
    t.set_started(true);
    assert_eq!(t.snapshot(0).state, State::Searching);
    t.on_msg(&Msg::Status(wire::ST_ENGINE_ON), 100);
    assert_eq!(t.snapshot(100).engine, "on");
    t.on_msg(&loc(23.5, 90.5), 1_000);
    let s = t.snapshot(2_000);
    assert_eq!(s.state, State::Fix);
    assert_eq!(s.fix.unwrap().accuracy, Some(5.0));
    assert_eq!(s.fix_age_ms, Some(1_000));
    // the fix goes stale but stays known (last fix shown with its age)
    let s = t.snapshot(1_000 + FIX_STALE_MS + 1);
    assert_eq!(s.state, State::Searching);
    assert!(s.fix.is_some());
    // stopping clears the satellites and reports off
    t.set_started(false);
    let s = t.snapshot(20_000);
    assert_eq!(s.state, State::Off);
    assert_eq!(s.engine, "off");
}

#[test]
fn position_without_flag_is_not_a_fix() {
    let mut t = Tracker::new();
    t.set_started(true);
    t.on_msg(
        &Msg::Location(Location {
            flags: 0,
            lat: 1.0,
            lon: 1.0,
            alt: 0.0,
            speed: 0.0,
            bearing: 0.0,
            accuracy: None,
            time_ms: 0,
        }),
        1,
    );
    assert_eq!(t.snapshot(2).state, State::Searching);
    assert!(t.snapshot(2).fix.is_none());
}

fn nm(t: &mut Tracker, body: &str, now: u64) {
    t.on_msg(
        &Msg::Nmea {
            time_ms: 0,
            text: with_checksum(body),
        },
        now,
    );
}

#[test]
fn satellites_from_gsv_and_gsa() {
    let mut t = Tracker::new();
    t.set_started(true);
    nm(
        &mut t,
        "GPGSV,2,1,05,03,40,100,31,04,15,270,22,06,01,010,,13,60,292,40",
        1000,
    );
    nm(&mut t, "GPGSV,2,2,05,20,30,030,18", 1000);
    nm(&mut t, "GLGSV,1,1,01,70,50,200,25", 1000);
    nm(&mut t, "GNGSA,A,3,03,13,20,,,,,,,,,1.5,1.0,1.1,1", 1000);
    nm(&mut t, "GNGSA,A,3,70,,,,,,,,,,,,1.5,1.0,1.1,2", 1000);
    let s = t.snapshot(1500);
    assert_eq!(s.view, 6);
    assert_eq!(s.used, 4);
    let g3 = s.sats.iter().find(|x| x.info.id == 3).unwrap();
    assert!(g3.used && g3.info.snr == Some(31));
    let g6 = s.sats.iter().find(|x| x.info.id == 6).unwrap();
    assert!(!g6.used && g6.info.snr.is_none());
    assert!(s.sats.iter().any(|x| x.sys == Sys::Glonass && x.used));
    // a new GPS cycle replaces the old list; GLONASS stays
    nm(&mut t, "GPGSV,1,1,01,09,10,010,20", 2000);
    let s = t.snapshot(2100);
    assert_eq!(s.view, 2);
    // stale satellites are dropped
    let s = t.snapshot(2000 + SAT_STALE_MS + 1);
    assert_eq!(s.view, 0);
    assert_eq!(s.used, 0);
}

#[test]
fn gsa_without_fix_clears_used() {
    let mut t = Tracker::new();
    t.set_started(true);
    nm(&mut t, "GPGSA,A,3,03,13,,,,,,,,,,,1.5,1.0,1.1", 10);
    assert_eq!(t.snapshot(20).used, 2);
    nm(&mut t, "GPGSA,A,1,,,,,,,,,,,,,,,", 30);
    assert_eq!(t.snapshot(40).used, 0);
}

#[test]
fn nmea_fallback_fix_only_when_binary_location_is_missing() {
    let mut t = Tracker::new();
    t.set_started(true);
    nm(
        &mut t,
        "GPRMC,123519,A,2330.000,N,09030.000,E,010.0,090.0,230394,,",
        1000,
    );
    nm(
        &mut t,
        "GPGGA,123519,2330.000,N,09030.000,E,1,07,0.9,12.5,M,0,M,,",
        1000,
    );
    let s = t.snapshot(1500);
    assert_eq!(s.state, State::Fix);
    let f = s.fix.unwrap();
    assert!((f.lat - 23.5).abs() < 1e-9 && (f.lon - 90.5).abs() < 1e-9);
    assert_eq!(f.alt, Some(12.5));
    assert!(f.speed.unwrap() > 5.0);
    assert_eq!(s.used, 7); // GGA count when no GSA arrived
                           // a binary location now wins over NMEA for the next 3 s
    t.on_msg(&loc(24.0, 91.0), 2000);
    nm(
        &mut t,
        "GPRMC,123520,A,2330.000,N,09030.000,E,010.0,090.0,230394,,",
        2500,
    );
    assert_eq!(t.snapshot(2600).fix.unwrap().lat, 24.0);
    // a void RMC never makes a fix
    let mut t = Tracker::new();
    t.set_started(true);
    nm(&mut t, "GPRMC,123519,V,,,,,,,230394,,", 10);
    assert!(t.snapshot(20).fix.is_none());
}

#[test]
fn bad_checksum_counts_but_changes_nothing() {
    let mut t = Tracker::new();
    t.set_started(true);
    t.on_msg(
        &Msg::Nmea {
            time_ms: 0,
            text: "$GPRMC,123519,A,2330.000,N,09030.000,E,1,2,230394,,*00".into(),
        },
        5,
    );
    assert_eq!(t.counters.bad_nmea, 1);
    assert!(t.snapshot(6).fix.is_none());
}

#[test]
fn json_shape() {
    let mut t = Tracker::new();
    t.set_started(true);
    t.on_msg(&loc(23.5, 90.5), 100);
    nm(&mut t, "GPGSV,1,1,01,03,40,100,31", 100);
    let s = t.snapshot(200);
    let v: Value = serde_json::from_str(&to_json(&s, 1_700_000_000_000, &t.counters)).unwrap();
    assert_eq!(v["state"], "fix");
    assert_eq!(v["fix"]["lat"], 23.5);
    assert_eq!(v["fix"]["accuracy"], 5.0);
    assert_eq!(v["fix"]["age_ms"], 100);
    assert_eq!(v["satellites"]["view"], 1);
    assert_eq!(v["satellites"]["list"][0]["snr"], 31);
    assert_eq!(v["updated"], 1_700_000_000_000u64);
    // off: no fix key content
    let mut t = Tracker::new();
    let v: Value = serde_json::from_str(&to_json(&t.snapshot(0), 5, &t.counters.clone())).unwrap();
    assert_eq!(v["state"], "off");
    assert!(v["fix"].is_null());
}
