//! Hotspot (owner request 2026-11-10). Everything that touches the system is done by the fixed
//! script /usr/local/sbin/walpad-hotspot (start|stop|status), run as a child with a timeout and
//! polled from the daemon loop (never waited on). The core owns /etc/walpad/hotspot.conf (0600):
//! `ssid2=<hex>` and `wpa_passphrase=<secret>` lines, which the script copies file to file into the
//! hostapd configuration. The passphrase never reaches argv, a log or a reply.
use crate::wpa::parse::hex;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{valid_hotspot_secret, valid_hotspot_ssid, HotspotState, HotspotStatus, Secret};

mod parse;
#[cfg(test)]
mod tests;
mod tick;

pub use parse::{parse_conf, HotspotError};
use parse::{Run, Verb};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-hotspot";
pub const CONF: &str = "/etc/walpad/hotspot.conf";
/// Name used when only a password was configured.
pub const DEFAULT_SSID: &str = "WalPad";
const START_TIMEOUT: Duration = Duration::from_secs(45);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const STATUS_EVERY: Duration = Duration::from_secs(10);
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

pub struct Hotspot {
    root: Root,
    script: PathBuf,
    conf: PathBuf,
    out: PathBuf,
    run: Option<Run>,
    st: HotspotStatus,
    /// Start again once the running stop finishes (new settings while on).
    restart: bool,
    next_status: Instant,
    timeouts: [Duration; 3],
}

impl Hotspot {
    pub fn new(root: &Root) -> Self {
        let mut h = Hotspot {
            root: root.clone(),
            script: root.p(SCRIPT),
            conf: root.p(CONF),
            out: root.p("/run/walpad/hotspot-out"),
            run: None,
            st: HotspotStatus::default(),
            restart: false,
            next_status: Instant::now(),
            timeouts: [START_TIMEOUT, STOP_TIMEOUT, STATUS_TIMEOUT],
        };
        h.load_conf();
        h
    }

    /// Shorter timeouts for tests: start, stop, status.
    pub fn set_timeouts(&mut self, start: Duration, stop: Duration, status: Duration) {
        self.timeouts = [start, stop, status];
    }

    fn load_conf(&mut self) {
        let text = std::fs::read_to_string(&self.conf).unwrap_or_default();
        let (ssid, has) = parse_conf(&text);
        self.st.ssid = ssid;
        self.st.has_secret = has;
    }

    pub fn status(&self) -> &HotspotStatus {
        &self.st
    }

    pub fn busy(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.verb != Verb::Status || r.cleanup)
    }

    fn spawn(&mut self, verb: Verb, now: Instant, cleanup: bool) -> Result<(), String> {
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
            cmd.env("WALPAD_HS_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-hotspot is not installed".to_string(),
            _ => format!("cannot run walpad-hotspot: {e}"),
        })?;
        let t = self.timeouts[verb as usize];
        self.run = Some(Run {
            verb,
            child,
            deadline: now + t,
            cleanup,
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

    /// Ask for the current state (startup: the hotspot may outlive a core restart).
    pub fn probe(&mut self, now: Instant) {
        if self.run.is_none() {
            let _ = self.spawn(Verb::Status, now, false);
        }
    }

    /// `hotspot {on}`. Returns the state right after the request.
    pub fn set_on(&mut self, on: bool, now: Instant) -> Result<HotspotState, HotspotError> {
        if self.busy() {
            return Err(HotspotError::Busy);
        }
        self.restart = false;
        if on {
            if !self.st.has_secret {
                return Err(HotspotError::Invalid("set a hotspot password first".into()));
            }
            if self.st.state == HotspotState::On {
                return Ok(HotspotState::On);
            }
            self.spawn(Verb::Start, now, false)
                .map_err(HotspotError::Failed)?;
            self.st.state = HotspotState::Starting;
        } else {
            if self.st.state == HotspotState::Off && self.run.is_none() {
                return Ok(HotspotState::Off);
            }
            self.spawn(Verb::Stop, now, false)
                .map_err(HotspotError::Failed)?;
            self.st.state = HotspotState::Stopping;
        }
        self.st.reason = None;
        Ok(self.st.state)
    }

    /// `hotspot_config`: store name and/or passphrase (0600, atomic). A running hotspot restarts
    /// with the new settings; returns true when that restart was started.
    pub fn configure(
        &mut self,
        ssid: Option<&str>,
        secret: Option<&Secret>,
        now: Instant,
    ) -> Result<bool, HotspotError> {
        if ssid.is_some_and(|s| !valid_hotspot_ssid(s)) {
            return Err(HotspotError::Invalid(
                "name must be 1-32 bytes without control characters".into(),
            ));
        }
        if secret.is_some_and(|s| !valid_hotspot_secret(s.expose())) {
            return Err(HotspotError::Invalid(
                "password must be 8-63 printable ASCII characters".into(),
            ));
        }
        if self.busy() {
            return Err(HotspotError::Busy);
        }
        let old = std::fs::read_to_string(&self.conf).unwrap_or_default();
        let old_secret = old
            .lines()
            .find(|l| l.starts_with("wpa_passphrase="))
            .map(str::to_string);
        let (old_ssid, _) = parse_conf(&old);
        let name = ssid
            .map(str::to_string)
            .or(old_ssid)
            .unwrap_or_else(|| DEFAULT_SSID.to_string());
        let mut text = format!(
            "# walpad-core hotspot_config (owner request 2026-11-10); mode 0600, do not log\nssid2={}\n",
            hex(name.as_bytes())
        );
        match (secret, old_secret) {
            (Some(s), _) => text.push_str(&format!("wpa_passphrase={}\n", s.expose())),
            (None, Some(l)) => {
                text.push_str(&l);
                text.push('\n');
            }
            (None, None) => {}
        }
        write_600(&self.conf, text.as_bytes())
            .map_err(|e| HotspotError::Failed(format!("cannot store the hotspot settings: {e}")))?;
        self.load_conf();
        if self.st.state == HotspotState::On {
            self.spawn(Verb::Stop, now, false)
                .map_err(HotspotError::Failed)?;
            self.st.state = HotspotState::Stopping;
            self.restart = true;
            return Ok(true);
        }
        Ok(false)
    }
}

pub(crate) fn write_600(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
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
        .mode(0o600)
        .open(&tmp)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(bytes)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}
