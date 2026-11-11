//! The daemon loop: take the GNSS HAL's place on mnld's sockets, start a positioning session,
//! fold the messages into a [`Tracker`] and publish /run/walpad/gnss.json atomically.
use crate::io::{NmeaOut, Sock};
use crate::state::{to_json, Tracker};
use crate::wire::{self, Msg};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// 2024-01-01: an earlier wall clock means "not set yet" (no battery-backed RTC value, no NTP).
const SANE_CLOCK_MS: i64 = 1_704_067_200_000;
const PUBLISH_EVERY_MS: u64 = 1_000;
/// Nothing at all heard from mnld this long after a start: send the session commands again.
const RESEND_AFTER_MS: u64 = 15_000;
const RESEND_MAX: u32 = 6;

#[derive(Debug, Clone)]
pub struct Config {
    pub hal_name: String,
    pub mnld_name: String,
    pub state_path: PathBuf,
    pub nmea_link: Option<PathBuf>,
    /// Where the first raw satellite blob (message 0x67) is saved for offline analysis.
    pub sv_dump: Option<PathBuf>,
    pub interval_ms: i32,
    /// false = only listen (a second consumer, or tests that start the session themselves).
    pub send_start: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hal_name: wire::HAL_NAME.into(),
            mnld_name: wire::MNLD_NAME.into(),
            state_path: "/run/walpad/gnss.json".into(),
            nmea_link: Some("/run/walpad/nmea".into()),
            sv_dump: Some("/run/walpad/gnss-sv.bin".into()),
            interval_ms: 1000,
            send_start: true,
        }
    }
}

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Atomic 0644 write (tmp + rename in the same directory).
pub fn write_atomic(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(&tmp)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    f.write_all(text.as_bytes())?;
    drop(f);
    std::fs::rename(&tmp, path)
}

/// Monotonic milliseconds for `inject_time`'s reference stamp.
fn boot_ms() -> i64 {
    // SAFETY: clock_gettime with a valid out-pointer.
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: see above.
    unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    ts.tv_sec * 1000 + ts.tv_nsec / 1_000_000
}

struct Ctx<'a> {
    cfg: &'a Config,
    sock: Sock,
    log: &'a dyn Fn(&str),
    tracker: Tracker,
    nmea: Option<NmeaOut>,
    sv_saved: bool,
    time_sent: bool,
}

impl Ctx<'_> {
    fn send(&self, dg: &[u8]) -> std::io::Result<()> {
        self.sock.send_to(&self.cfg.mnld_name, dg)
    }

    fn inject_time(&mut self) {
        let w = wall_ms();
        if w < SANE_CLOCK_MS {
            return;
        }
        if self.send(&wire::inject_time(w, boot_ms(), 5000)).is_ok() {
            self.time_sent = true;
            (self.log)("time injected into mnld");
        }
    }

    /// init + position mode + start (+ time).
    fn start_session(&mut self) -> std::io::Result<()> {
        self.send(&wire::init())?;
        self.send(&wire::position_mode(self.cfg.interval_ms))?;
        self.send(&wire::start())?;
        self.tracker.set_started(true);
        self.tracker.reason = None;
        self.time_sent = false;
        self.inject_time();
        Ok(())
    }

    fn handle(&mut self, dg: &[u8], now: u64) {
        let Some(m) = wire::decode(dg) else {
            return;
        };
        match &m {
            Msg::Nmea { text, .. } => {
                if let Some(n) = &self.nmea {
                    n.write_line(text);
                }
            }
            Msg::Sv(b) if !self.sv_saved => {
                if let Some(p) = &self.cfg.sv_dump {
                    // satellite data only (no coordinates): kept for working out the layout
                    let _ = std::fs::write(p, b);
                }
                self.sv_saved = true;
            }
            Msg::RequestUtcTime => self.inject_time(),
            Msg::Status(s) => (self.log)(&format!("mnld status {s}")),
            _ => {}
        }
        self.tracker.on_msg(&m, now);
    }

    fn publish(&mut self, now: u64) {
        let snap = self.tracker.snapshot(now);
        let text = to_json(&snap, wall_ms(), &self.tracker.counters);
        if let Err(e) = write_atomic(&self.cfg.state_path, &text) {
            (self.log)(&format!("cannot write the state file: {e}"));
        }
    }
}

/// Run until `stop` is set. Returns Err only when the socket cannot be taken.
pub fn run(cfg: &Config, stop: &AtomicBool, log: &dyn Fn(&str)) -> std::io::Result<()> {
    let sock = Sock::bind(&cfg.hal_name)?;
    let nmea = match &cfg.nmea_link {
        Some(l) => match NmeaOut::open(l) {
            Ok(n) => Some(n),
            Err(e) => {
                log(&format!("no NMEA pty: {e}"));
                None
            }
        },
        None => None,
    };
    let mut c = Ctx {
        cfg,
        sock,
        log,
        tracker: Tracker::new(),
        nmea,
        sv_saved: false,
        time_sent: false,
    };
    let t0 = Instant::now();
    let mono = || t0.elapsed().as_millis() as u64;
    let mut started = false;
    let mut next_try = 0u64;
    let mut started_at = 0u64;
    let mut heard = false;
    let mut resends = 0u32;
    let mut last_pub = 0u64;
    let mut buf = vec![0u8; 0x10000];
    c.publish(0);
    log("listening as the GNSS HAL");
    while !stop.load(Ordering::SeqCst) {
        let now = mono();
        if cfg.send_start && !started && now >= next_try {
            match c.start_session() {
                Ok(()) => {
                    started = true;
                    started_at = now;
                    heard = false;
                    log("session commands sent to mnld");
                }
                Err(e) => {
                    c.tracker.reason = Some(format!("mnld does not answer ({e})"));
                    next_try = now + 1000;
                }
            }
        } else if !cfg.send_start && !started {
            started = true;
            c.tracker.set_started(true);
        }
        if cfg.send_start
            && started
            && !heard
            && resends < RESEND_MAX
            && now.saturating_sub(started_at) >= RESEND_AFTER_MS
        {
            resends += 1;
            started_at = now;
            log("mnld is silent: sending the session commands again");
            let _ = c.start_session();
        }
        if started && !c.time_sent && cfg.send_start {
            c.inject_time();
        }
        match c.sock.recv(&mut buf, 250) {
            Ok(Some(n)) => {
                heard = true;
                let now = mono();
                c.handle(&buf[..n], now);
            }
            Ok(None) => {}
            Err(e) => {
                log(&format!("receive failed: {e}"));
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        let now = mono();
        if now.saturating_sub(last_pub) >= PUBLISH_EVERY_MS {
            last_pub = now;
            c.publish(now);
        }
    }
    if cfg.send_start && started {
        let _ = c.send(&wire::stop());
        let _ = c.send(&wire::cleanup());
        log("stop sent to mnld");
    }
    c.tracker.set_started(false);
    c.publish(mono());
    drop(c.nmea.take());
    Ok(())
}
