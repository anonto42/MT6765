//! What the daemon knows about the receiver, folded from mnld's messages.  Pure logic with an
//! explicit monotonic clock so every rule is unit-testable.
use crate::nmea::{self, SatInfo, Sentence, Sys};
use crate::wire::{self, Location, Msg};
use std::collections::{BTreeMap, BTreeSet};

mod json;
#[cfg(test)]
mod tests;
mod types;

pub use json::to_json;
pub use types::{Counters, Fix, Sat, Snapshot, State, FIX_STALE_MS, SAT_STALE_MS};
use types::{Engine, UsedSet, LOC_PREFERRED_MS};

pub struct Tracker {
    started: bool,
    engine: Engine,
    fix: Option<(Fix, u64)>,
    loc_at: Option<u64>,
    sats: BTreeMap<(Sys, u16), (SatInfo, u64)>,
    used: BTreeMap<Option<Sys>, UsedSet>,
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
            Sentence::Gsv { sys, num, sats, .. } => {
                if num == 1 {
                    // a new cycle of this constellation starts: forget what it listed before
                    if let Some(sy) = sys {
                        self.sats.retain(|(s, _), _| *s != sy)
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
