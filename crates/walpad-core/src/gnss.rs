//! GPS switch and position (owner request 2026-11-11). Everything that touches the system is done
//! by the fixed script /usr/local/sbin/walpad-gps (start|stop|status), run as a child with a timeout
//! and polled from the daemon loop (never waited on); like the hotspot. The position itself comes
//! from /run/walpad/gnss.json, written by walpad-gnss (the stand-in for Android's GNSS HAL), read
//! here every couple of seconds. The switch is persisted in /var/lib/walpad/gps-enabled; a switched-on
//! GPS is started again after a boot (and retried while the Wi-Fi stack it shares is not up).
//!
//! Coordinates are private: they are only copied into the status reply for the owner (rounded for
//! display) and are never logged here.
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use walpad_hw::Root;
use walpad_proto::{GpsState, GpsStatus};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-gps";
pub const ENABLED_FILE: &str = "/var/lib/walpad/gps-enabled";
pub const STATE_FILE: &str = "/run/walpad/gnss.json";
const START_TIMEOUT: Duration = Duration::from_secs(40);
const STOP_TIMEOUT: Duration = Duration::from_secs(15);
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const READ_EVERY: Duration = Duration::from_secs(2);
const CHECK_EVERY: Duration = Duration::from_secs(30);
/// A failed (re)start is tried again this often while the switch is on.
const RETRY_EVERY: Duration = Duration::from_secs(60);
/// gnss.json not rewritten for this long: the daemon is gone or stuck.
const STALE_MS: i64 = 15_000;
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const MAX_JSON: u64 = 1 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Start,
    Stop,
    Status,
}

impl Verb {
    fn arg(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Status => "status",
        }
    }
}

struct Run {
    verb: Verb,
    child: Child,
    deadline: Instant,
}

#[derive(Debug, PartialEq, Eq)]
pub enum GnssError {
    Busy,
    Failed(String),
}

pub struct Gnss {
    root: Root,
    script: PathBuf,
    enabled_file: PathBuf,
    json: PathBuf,
    out: PathBuf,
    run: Option<Run>,
    st: GpsStatus,
    next_read: Instant,
    next_check: Instant,
    next_start: Instant,
    timeouts: [Duration; 3],
}

fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn clean_reason(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_string()
}

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Round for display: 5 decimals is about one metre.
fn round5(v: f64) -> f64 {
    (v * 1e5).round() / 1e5
}

/// The part of gnss.json the status needs; `None` = unreadable or not a gnss document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reading {
    pub state: String,
    pub reason: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<i32>,
    pub accuracy_m: Option<u32>,
    pub used: Option<u32>,
    pub view: Option<u32>,
    pub age_s: Option<u32>,
    pub signals: Vec<u8>,
    pub updated_ms: i64,
}

pub fn parse_reading(text: &str, now_ms: i64) -> Option<Reading> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let state = v.get("state")?.as_str()?.to_string();
    let updated_ms = v.get("updated").and_then(|u| u.as_i64()).unwrap_or(0);
    let mut r = Reading {
        state,
        reason: v
            .get("reason")
            .and_then(|x| x.as_str())
            .map(clean_reason)
            .filter(|s| !s.is_empty()),
        updated_ms,
        ..Reading::default()
    };
    if let Some(f) = v.get("fix").filter(|f| f.is_object()) {
        let lat = f.get("lat").and_then(|x| x.as_f64());
        let lon = f.get("lon").and_then(|x| x.as_f64());
        if let (Some(la), Some(lo)) = (lat, lon) {
            if (-90.0..=90.0).contains(&la) && (-180.0..=180.0).contains(&lo) {
                r.lat = Some(round5(la));
                r.lon = Some(round5(lo));
                r.alt_m = f
                    .get("alt")
                    .and_then(|x| x.as_f64())
                    .filter(|a| a.is_finite() && a.abs() < 100_000.0)
                    .map(|a| a.round() as i32);
                r.accuracy_m = f
                    .get("accuracy")
                    .and_then(|x| x.as_f64())
                    .filter(|a| a.is_finite() && *a >= 0.0 && *a < 1.0e6)
                    .map(|a| a.round() as u32);
                let age_ms = f.get("age_ms").and_then(|x| x.as_i64()).unwrap_or(0).max(0);
                // the file is rewritten every second; time since it was written counts too
                let since = if updated_ms > 0 {
                    (now_ms - updated_ms).max(0)
                } else {
                    0
                };
                r.age_s = Some(((age_ms + since) / 1000).min(u32::MAX as i64) as u32);
            }
        }
    }
    if let Some(s) = v.get("satellites") {
        r.view = s
            .get("view")
            .and_then(|x| x.as_u64())
            .map(|n| n.min(999) as u32);
        r.used = s
            .get("used")
            .and_then(|x| x.as_u64())
            .map(|n| n.min(999) as u32);
        if let Some(list) = s.get("list").and_then(|x| x.as_array()) {
            let mut sig: Vec<u8> = list
                .iter()
                .map(|e| e.get("snr").and_then(|x| x.as_u64()).unwrap_or(0).min(99) as u8)
                .collect();
            sig.sort_unstable_by(|a, b| b.cmp(a));
            sig.truncate(16);
            r.signals = sig;
        }
    }
    Some(r)
}

impl Gnss {
    pub fn new(root: &Root) -> Self {
        let now = Instant::now();
        let mut g = Gnss {
            root: root.clone(),
            script: root.p(SCRIPT),
            enabled_file: root.p(ENABLED_FILE),
            json: root.p(STATE_FILE),
            out: root.p("/run/walpad/gps-out"),
            run: None,
            st: GpsStatus::default(),
            next_read: now,
            next_check: now,
            next_start: now,
            timeouts: [START_TIMEOUT, STOP_TIMEOUT, STATUS_TIMEOUT],
        };
        g.st.enabled = std::fs::read_to_string(&g.enabled_file)
            .map(|t| t.trim() == "1")
            .unwrap_or(false);
        g
    }

    /// Shorter timeouts for tests: start, stop, status.
    pub fn set_timeouts(&mut self, start: Duration, stop: Duration, status: Duration) {
        self.timeouts = [start, stop, status];
    }

    pub fn status(&self) -> &GpsStatus {
        &self.st
    }

    pub fn busy(&self) -> bool {
        self.run.as_ref().is_some_and(|r| r.verb != Verb::Status)
    }

    fn running_state(&self) -> bool {
        matches!(self.st.state, GpsState::Searching | GpsState::Fix)
    }

    fn spawn(&mut self, verb: Verb, now: Instant) -> Result<(), String> {
        if let Some(r) = self.run.take() {
            Self::kill(r);
        }
        let out = std::fs::File::create(&self.out).map_err(|e| format!("output file: {e}"))?;
        let mut cmd = Command::new(&self.script);
        cmd.arg(verb.arg())
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(Stdio::null());
        if self.root.base() != std::path::Path::new("/") {
            cmd.env("WALPAD_GPS_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-gps is not installed".to_string(),
            _ => format!("cannot run walpad-gps: {e}"),
        })?;
        self.run = Some(Run {
            verb,
            child,
            deadline: now + self.timeouts[verb as usize],
        });
        Ok(())
    }

    fn kill(mut r: Run) {
        if let Some(pid) = rustix::process::Pid::from_raw(r.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    }

    /// Startup: if the switch is on, bring the GPS up (it may also still be running from before a
    /// core restart: `start` is idempotent).
    pub fn probe(&mut self, now: Instant) {
        self.next_start = now;
        if self.st.enabled && self.run.is_none() {
            self.next_start = now + RETRY_EVERY;
            if self.spawn(Verb::Start, now).is_ok() {
                self.st.state = GpsState::Starting;
            }
        } else if self.run.is_none() {
            let _ = self.spawn(Verb::Status, now);
        }
    }

    /// `gps {on}`: persist the switch, then start or stop. Returns the state right after.
    pub fn set_on(&mut self, on: bool, now: Instant) -> Result<GpsState, GnssError> {
        if self.busy() {
            return Err(GnssError::Busy);
        }
        crate::hotspot::write_600(&self.enabled_file, if on { b"1\n" } else { b"0\n" })
            .map_err(|e| GnssError::Failed(format!("cannot store the GPS switch: {e}")))?;
        self.st.enabled = on;
        if on {
            if self.running_state() {
                return Ok(self.st.state);
            }
            self.spawn(Verb::Start, now).map_err(GnssError::Failed)?;
            self.st.state = GpsState::Starting;
            self.next_start = now + RETRY_EVERY;
        } else {
            if self.st.state == GpsState::Off && self.run.is_none() {
                return Ok(GpsState::Off);
            }
            self.spawn(Verb::Stop, now).map_err(GnssError::Failed)?;
            self.st.state = GpsState::Stopping;
        }
        self.st.reason = None;
        Ok(self.st.state)
    }

    fn clear_position(&mut self) {
        let keep = (self.st.enabled, self.st.state, self.st.reason.take());
        self.st = GpsStatus {
            enabled: keep.0,
            state: keep.1,
            reason: keep.2,
            ..GpsStatus::default()
        };
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match r.verb {
            Verb::Start => {
                if code == Some(0) && state == Some("on") {
                    self.st.state = GpsState::Searching;
                    self.st.reason = None;
                    self.next_read = now;
                    self.next_check = now + CHECK_EVERY;
                } else {
                    self.clear_position();
                    self.st.state = GpsState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-gps failed (exit {c})"),
                        None => "walpad-gps was killed".into(),
                    }));
                }
            }
            Verb::Stop => {
                if code == Some(0) {
                    self.clear_position();
                    self.st.state = GpsState::Off;
                } else {
                    self.st.state = GpsState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not stop the GPS".into()));
                }
            }
            Verb::Status => {
                if code != Some(0) {
                    return;
                }
                let on = state == Some("on");
                if on && !self.running_state() && self.st.enabled {
                    self.st.state = GpsState::Searching;
                    self.st.reason = None;
                } else if !on && self.running_state() {
                    // died behind our back: report it, the retry brings it up again
                    self.clear_position();
                    self.st.state = GpsState::Failed;
                    self.st.reason = Some("the GPS stopped unexpectedly".into());
                    self.next_start = now;
                } else if !on && self.st.state == GpsState::Off {
                    // nothing to do
                }
                self.next_check = now + CHECK_EVERY;
            }
        }
    }

    fn read_state(&mut self, now: Instant) {
        self.next_read = now + READ_EVERY;
        let Ok(meta) = std::fs::metadata(&self.json) else {
            return;
        };
        if meta.len() > MAX_JSON {
            return;
        }
        let Ok(text) = std::fs::read_to_string(&self.json) else {
            return;
        };
        let wall = wall_ms();
        let Some(r) = parse_reading(&text, wall) else {
            return;
        };
        let stale = r.updated_ms > 0 && wall - r.updated_ms > STALE_MS;
        let state = match (r.state.as_str(), stale) {
            (_, true) | ("off", _) => GpsState::Searching,
            ("fix", false) => GpsState::Fix,
            _ => GpsState::Searching,
        };
        let enabled = self.st.enabled;
        self.st = GpsStatus {
            enabled,
            state,
            reason: r.reason,
            lat: r.lat,
            lon: r.lon,
            alt_m: r.alt_m,
            accuracy_m: r.accuracy_m,
            satellites_used: r.used,
            satellites_view: r.view,
            age_s: r.age_s,
            signals: r.signals,
        };
        if stale {
            // the daemon stopped writing: ask the script whether it is still alive
            self.next_check = now;
        }
    }

    /// Advance the running script, read the position. Returns true when the visible status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st.clone();
        if let Some(mut r) = self.run.take() {
            match r.child.try_wait() {
                Ok(Some(es)) => self.finish(r, es.code(), now),
                Ok(None) if now >= r.deadline => {
                    let verb = r.verb;
                    Self::kill(r);
                    match verb {
                        Verb::Status => self.next_check = now + CHECK_EVERY,
                        Verb::Start | Verb::Stop => {
                            self.clear_position();
                            self.st.state = GpsState::Failed;
                            self.st.reason = Some(format!("walpad-gps {} timed out", verb.arg()));
                            if verb == Verb::Start {
                                let _ = self.spawn(Verb::Stop, now);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.running_state() {
            if now >= self.next_check && self.spawn(Verb::Status, now).is_err() {
                self.next_check = now + CHECK_EVERY;
            }
        } else if self.st.enabled
            && matches!(self.st.state, GpsState::Off | GpsState::Failed)
            && now >= self.next_start
        {
            // the switch is on but nothing runs (boot ordering, a crash): try again
            self.next_start = now + RETRY_EVERY;
            if self.spawn(Verb::Start, now).is_ok() {
                self.st.state = GpsState::Starting;
            }
        }
        if self.running_state()
            && self.run.as_ref().is_none_or(|r| r.verb == Verb::Status)
            && now >= self.next_read
        {
            self.read_state(now);
        }
        self.st != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(state: &str, fix: bool, updated: i64) -> String {
        json!({
            "v": 1, "state": state, "updated": updated,
            "fix": if fix { json!({"lat": 23.7512345, "lon": 90.4, "alt": 12.4, "accuracy": 6.6,
                                   "age_ms": 3000}) } else { json!(null) },
            "satellites": {"view": 3, "used": 2, "list": [
                {"snr": 20}, {"snr": null}, {"snr": 41}]},
        })
        .to_string()
    }

    #[test]
    fn reading_rounds_for_display_and_orders_signals() {
        let r = parse_reading(&doc("fix", true, 1_000_000), 1_002_000).unwrap();
        assert_eq!(r.state, "fix");
        assert_eq!(r.lat, Some(23.75123));
        assert_eq!(r.lon, Some(90.4));
        assert_eq!(r.alt_m, Some(12));
        assert_eq!(r.accuracy_m, Some(7));
        // 3000 ms at the fix + 2000 ms since the file was written
        assert_eq!(r.age_s, Some(5));
        assert_eq!((r.view, r.used), (Some(3), Some(2)));
        assert_eq!(r.signals, vec![41, 20, 0]);
    }

    #[test]
    fn reading_without_fix_or_with_bad_numbers() {
        let r = parse_reading(&doc("searching", false, 5), 5).unwrap();
        assert!(r.lat.is_none() && r.age_s.is_none());
        let bad = json!({"state":"fix","fix":{"lat":123.0,"lon":1.0}}).to_string();
        assert!(parse_reading(&bad, 0).unwrap().lat.is_none());
        assert!(parse_reading("not json", 0).is_none());
        assert!(parse_reading("{}", 0).is_none());
        assert!(parse_reading("[1]", 0).is_none());
    }

    #[test]
    fn script_fields() {
        let out = "[gps] x\nstate=failed\nreason=turn Wi-Fi on first\x07\n";
        assert_eq!(field(out, "state"), Some("failed"));
        assert_eq!(
            clean_reason(field(out, "reason").unwrap()),
            "turn Wi-Fi on first"
        );
        assert_eq!(clean_reason(&"y".repeat(400)).len(), 120);
    }
}
