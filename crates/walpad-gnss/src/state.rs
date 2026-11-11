//! What the daemon knows about the receiver, folded from mnld's messages.  Pure logic with an
//! explicit monotonic clock so every rule is unit-testable.
use crate::nmea::{self, SatInfo, Sentence, Sys};
use crate::wire::{self, Location, Msg};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// A fix older than this no longer counts as "fix" (the state falls back to searching).
pub const FIX_STALE_MS: u64 = 10_000;
/// Satellites not repeated in a GSV for this long are dropped.
pub const SAT_STALE_MS: u64 = 15_000;
/// An NMEA-derived fix is used only when the binary location message is this old.
const LOC_PREFERRED_MS: u64 = 3_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Off,
    Searching,
    Fix,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Off => "off",
            State::Searching => "searching",
            State::Fix => "fix",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {
    pub lat: f64,
    pub lon: f64,
    pub alt: Option<f64>,
    pub speed: Option<f32>,
    pub bearing: Option<f32>,
    pub accuracy: Option<f32>,
    /// UTC ms when the receiver knows it.
    pub time_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sat {
    pub sys: Sys,
    pub info: SatInfo,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub state: State,
    /// `on` / `off` / `unknown`: the engine state mnld last reported.
    pub engine: &'static str,
    pub fix: Option<Fix>,
    /// Milliseconds since the last fix (None = never).
    pub fix_age_ms: Option<u64>,
    pub sats: Vec<Sat>,
    pub view: usize,
    pub used: usize,
    pub reason: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counters {
    pub location: u32,
    pub status: u32,
    pub sv: u32,
    pub nmea: u32,
    pub bad_nmea: u32,
    pub utc_requests: u32,
    pub other: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Engine {
    Unknown,
    On,
    Off,
}

pub struct Tracker {
    started: bool,
    engine: Engine,
    fix: Option<(Fix, u64)>,
    loc_at: Option<u64>,
    sats: BTreeMap<(Sys, u16), (SatInfo, u64)>,
    used: BTreeMap<Option<Sys>, (BTreeSet<(Sys, u16)>, u64)>,
    gga_used: Option<(u8, u64)>,
    /// A previous RMC (position, course, speed, time) waits here for its GGA (altitude).
    rmc: Option<(Fix, u64)>,
    pub reason: Option<String>,
    pub counters: Counters,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker::new()
    }
}

impl Tracker {
    pub fn new() -> Tracker {
        Tracker {
            started: false,
            engine: Engine::Unknown,
            fix: None,
            loc_at: None,
            sats: BTreeMap::new(),
            used: BTreeMap::new(),
            gga_used: None,
            rmc: None,
            reason: None,
            counters: Counters::default(),
        }
    }

    /// The daemon asked mnld to run (true) / stopped it (false).
    pub fn set_started(&mut self, on: bool) {
        self.started = on;
        if !on {
            self.engine = Engine::Off;
            self.sats.clear();
            self.used.clear();
            self.gga_used = None;
        } else {
            self.engine = Engine::Unknown;
        }
    }

    pub fn on_msg(&mut self, m: &Msg, now: u64) {
        match m {
            Msg::Location(l) => {
                self.counters.location += 1;
                self.on_location(l, now);
            }
            Msg::Status(s) => {
                self.counters.status += 1;
                match *s {
                    wire::ST_ENGINE_ON | wire::ST_SESSION_BEGIN => self.engine = Engine::On,
                    wire::ST_ENGINE_OFF | wire::ST_NONE => self.engine = Engine::Off,
                    _ => {}
                }
            }
            Msg::Sv(_) => self.counters.sv += 1,
            Msg::Nmea { text, .. } => {
                self.counters.nmea += 1;
                self.on_nmea(text, now);
            }
            Msg::RequestUtcTime => self.counters.utc_requests += 1,
            Msg::Capabilities(_) | Msg::Other(_) => self.counters.other += 1,
        }
    }

    fn on_location(&mut self, l: &Location, now: u64) {
        if !l.has_position() {
            return;
        }
        self.loc_at = Some(now);
        let f = Fix {
            lat: l.lat,
            lon: l.lon,
            alt: (l.flags & wire::LOC_HAS_ALTITUDE != 0 && l.alt.is_finite()).then_some(l.alt),
            speed: (l.flags & wire::LOC_HAS_SPEED != 0 && l.speed.is_finite()).then_some(l.speed),
            bearing: (l.flags & wire::LOC_HAS_BEARING != 0 && l.bearing.is_finite())
                .then_some(l.bearing),
            accuracy: l.accuracy,
            time_ms: (l.time_ms > 0).then_some(l.time_ms),
        };
        self.fix = Some((f, now));
    }

    fn loc_is_fresh(&self, now: u64) -> bool {
        self.loc_at
            .is_some_and(|t| now.saturating_sub(t) <= LOC_PREFERRED_MS)
    }

    pub fn on_nmea(&mut self, line: &str, now: u64) {
        let Some(s) = nmea::parse(line) else {
            if line.starts_with('$') && !nmea::checksum_ok(line) {
                self.counters.bad_nmea += 1;
            }
            return;
        };
        match s {
            Sentence::Gga {
                lat,
                lon,
                quality,
                used,
                alt,
            } => {
                if let Some(u) = used {
                    self.gga_used = Some((u, now));
                }
                if quality > 0 && !self.loc_is_fresh(now) {
                    if let (Some(lat), Some(lon)) = (lat, lon) {
                        let base = self.rmc.filter(|(_, t)| now.saturating_sub(*t) < 2_000);
                        let mut f = base.map(|(f, _)| f).unwrap_or(Fix {
                            lat,
                            lon,
                            alt: None,
                            speed: None,
                            bearing: None,
                            accuracy: None,
                            time_ms: None,
                        });
                        f.lat = lat;
                        f.lon = lon;
                        f.alt = alt;
                        self.fix = Some((f, now));
                    }
                }
            }
            Sentence::Rmc {
                valid,
                lat,
                lon,
                speed_mps,
                course,
                time_ms,
            } => {
                if !valid || self.loc_is_fresh(now) {
                    return;
                }
                if let (Some(lat), Some(lon)) = (lat, lon) {
                    let keep_alt = self.fix.and_then(|(f, _)| f.alt);
                    let f = Fix {
                        lat,
                        lon,
                        alt: keep_alt,
                        speed: speed_mps,
                        bearing: course,
                        accuracy: None,
                        time_ms,
                    };
                    self.rmc = Some((f, now));
                    self.fix = Some((f, now));
                }
            }
            Sentence::Gsv {
                sys,
                num,
                sats,
                ..
            } => {
                if num == 1 {
                    // a new cycle of this constellation starts: forget what it listed before
                    match sys {
                        Some(sy) => self.sats.retain(|(s, _), _| *s != sy),
                        None => {}
                    }
                }
                for si in sats {
                    let s = match sys {
                        Some(s) => s,
                        None => nmea::sys_of_id(si.id),
                    };
                    self.sats.insert((s, si.id), (si, now));
                }
            }
            Sentence::Gsa { sys, fix, ids } => {
                let set: BTreeSet<(Sys, u16)> = if fix < 2 {
                    BTreeSet::new()
                } else {
                    ids.iter()
                        .map(|id| (sys.unwrap_or_else(|| nmea::sys_of_id(*id)), *id))
                        .collect()
                };
                self.used.insert(sys, (set, now));
            }
        }
    }

    pub fn snapshot(&mut self, now: u64) -> Snapshot {
        self.sats
            .retain(|_, (_, t)| now.saturating_sub(*t) <= SAT_STALE_MS);
        self.used
            .retain(|_, (_, t)| now.saturating_sub(*t) <= SAT_STALE_MS);
        let used_set: BTreeSet<(Sys, u16)> = self
            .used
            .values()
            .flat_map(|(s, _)| s.iter().copied())
            .collect();
        let sats: Vec<Sat> = self
            .sats
            .iter()
            .map(|((sys, id), (info, _))| Sat {
                sys: *sys,
                info: *info,
                used: used_set.contains(&(*sys, *id)),
            })
            .collect();
        let gga = self
            .gga_used
            .filter(|(_, t)| now.saturating_sub(*t) <= SAT_STALE_MS)
            .map_or(0, |(u, _)| u as usize);
        let fresh = self
            .fix
            .is_some_and(|(_, t)| now.saturating_sub(t) <= FIX_STALE_MS);
        let state = if !self.started {
            State::Off
        } else if fresh {
            State::Fix
        } else {
            State::Searching
        };
        Snapshot {
            state,
            engine: match self.engine {
                Engine::Unknown => "unknown",
                Engine::On => "on",
                Engine::Off => "off",
            },
            fix: self.fix.map(|(f, _)| f),
            fix_age_ms: self.fix.map(|(_, t)| now.saturating_sub(t)),
            view: sats.len(),
            used: used_set.len().max(if fresh { gga } else { 0 }),
            sats,
            reason: self.reason.clone(),
        }
    }
}

/// The published document /run/walpad/gnss.json.  Coordinates are private: the daemon never logs
/// this text, and the file is for walpad-core and local tools.
pub fn to_json(s: &Snapshot, wall_ms: i64, c: &Counters) -> String {
    let fix = s.fix.map(|f| {
        json!({
            "lat": f.lat, "lon": f.lon, "alt": f.alt, "speed": f.speed,
            "bearing": f.bearing, "accuracy": f.accuracy, "time": f.time_ms,
            "age_ms": s.fix_age_ms,
        })
    });
    let list: Vec<Value> = s
        .sats
        .iter()
        .map(|t| {
            json!({
                "sys": t.sys.name(), "id": t.info.id, "snr": t.info.snr,
                "elev": t.info.elev, "az": t.info.az, "used": t.used,
            })
        })
        .collect();
    let v = json!({
        "v": 1,
        "state": s.state.name(),
        "engine": s.engine,
        "reason": s.reason,
        "updated": wall_ms,
        "fix": fix,
        "satellites": {"view": s.view, "used": s.used, "list": list},
        "counters": {
            "location": c.location, "status": c.status, "sv": c.sv, "nmea": c.nmea,
            "bad_nmea": c.bad_nmea, "utc_requests": c.utc_requests, "other": c.other,
        },
    });
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nmea::with_checksum;
    use crate::wire::{LOC_HAS_ACCURACY, LOC_HAS_ALTITUDE, LOC_HAS_LAT_LONG};

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
        nm(&mut t, "GPGSV,2,1,05,03,40,100,31,04,15,270,22,06,01,010,,13,60,292,40", 1000);
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
        nm(&mut t, "GPRMC,123519,A,2330.000,N,09030.000,E,010.0,090.0,230394,,", 1000);
        nm(&mut t, "GPGGA,123519,2330.000,N,09030.000,E,1,07,0.9,12.5,M,0,M,,", 1000);
        let s = t.snapshot(1500);
        assert_eq!(s.state, State::Fix);
        let f = s.fix.unwrap();
        assert!((f.lat - 23.5).abs() < 1e-9 && (f.lon - 90.5).abs() < 1e-9);
        assert_eq!(f.alt, Some(12.5));
        assert!(f.speed.unwrap() > 5.0);
        assert_eq!(s.used, 7); // GGA count when no GSA arrived
        // a binary location now wins over NMEA for the next 3 s
        t.on_msg(&loc(24.0, 91.0), 2000);
        nm(&mut t, "GPRMC,123520,A,2330.000,N,09030.000,E,010.0,090.0,230394,,", 2500);
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
}
