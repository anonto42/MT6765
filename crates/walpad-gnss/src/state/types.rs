use crate::nmea::{SatInfo, Sys};
use std::collections::BTreeSet;

/// A fix older than this no longer counts as "fix" (the state falls back to searching).
pub const FIX_STALE_MS: u64 = 10_000;
/// Satellites not repeated in a GSV for this long are dropped.
pub const SAT_STALE_MS: u64 = 15_000;
/// An NMEA-derived fix is used only when the binary location message is this old.
pub(super) const LOC_PREFERRED_MS: u64 = 3_000;

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

/// Satellite ids a GSA sentence reported as used, with the time it arrived.
pub(super) type UsedSet = (BTreeSet<(Sys, u16)>, u64);

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
pub(super) enum Engine {
    Unknown,
    On,
    Off,
}
