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
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{GpsState, GpsStatus};

mod reading;
#[cfg(test)]
mod tests;
mod tick;

pub use reading::{parse_reading, Reading};

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
pub(super) enum Verb {
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

pub(super) struct Run {
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
}
