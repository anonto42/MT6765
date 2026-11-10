//! Hotspot (owner request 2026-11-10). Everything that touches the system is done by the fixed
//! script /usr/local/sbin/walpad-hotspot (start|stop|status), run as a child with a timeout and
//! polled from the daemon loop (never waited on). The core owns /etc/walpad/hotspot.conf (0600):
//! `ssid2=<hex>` and `wpa_passphrase=<secret>` lines, which the script copies file to file into the
//! hostapd configuration. The passphrase never reaches argv, a log or a reply.
use crate::wpa::parse::{hex, unhex};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{
    valid_hotspot_secret, valid_hotspot_ssid, HotspotInternet, HotspotState, HotspotStatus, Secret,
};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-hotspot";
pub const CONF: &str = "/etc/walpad/hotspot.conf";
/// Name used when only a password was configured.
pub const DEFAULT_SSID: &str = "WalPad";
const START_TIMEOUT: Duration = Duration::from_secs(45);
const STOP_TIMEOUT: Duration = Duration::from_secs(20);
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const STATUS_EVERY: Duration = Duration::from_secs(10);
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

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
    /// A cleanup `stop` after a failed/timed-out start: it must not overwrite the failure.
    cleanup: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HotspotError {
    Busy,
    Invalid(String),
    Failed(String),
}

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

/// One `key=value` line of the script's output.
fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

/// Short, printable reason text from the script (never trusted to be tidy).
fn clean_reason(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(120).collect();
    t.trim().to_string()
}

/// (ssid, has_secret) from the config file content.
pub fn parse_conf(text: &str) -> (Option<String>, bool) {
    let mut ssid = None;
    let mut secret = false;
    for l in text.lines() {
        if let Some(h) = l.strip_prefix("ssid2=") {
            ssid = unhex(h.trim()).map(|b| String::from_utf8_lossy(&b).into_owned());
        } else if l.starts_with("wpa_passphrase=") && l.len() > "wpa_passphrase=".len() {
            secret = true;
        }
    }
    (ssid, secret)
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

    fn apply_output(&mut self, out: &str) {
        self.st.clients = field(out, "clients").and_then(|c| c.parse().ok());
        self.st.internet = match field(out, "internet") {
            Some("shared") => Some(HotspotInternet::Shared),
            Some("no_uplink") => Some(HotspotInternet::NoUplink),
            Some("unavailable") => Some(HotspotInternet::Unavailable),
            _ => None,
        };
        self.st.exclusive = field(out, "exclusive") == Some("1");
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match (r.verb, r.cleanup) {
            (Verb::Stop, true) => {
                // keep the failure that caused the cleanup; the client is resumed by the script
                self.st.exclusive = false;
                self.st.clients = None;
                self.st.internet = None;
            }
            (Verb::Start, _) => {
                if code == Some(0) && state == Some("on") {
                    self.apply_output(&out);
                    self.st.state = HotspotState::On;
                    self.st.reason = None;
                    self.next_status = now + STATUS_EVERY;
                } else {
                    self.apply_output(&out);
                    self.st.state = HotspotState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-hotspot failed (exit {c})"),
                        None => "walpad-hotspot was killed".into(),
                    }));
                    self.st.clients = None;
                }
            }
            (Verb::Stop, false) => {
                self.apply_output(&out);
                self.st.clients = None;
                self.st.internet = None;
                if code == Some(0) {
                    self.st.state = HotspotState::Off;
                    self.st.reason = None;
                    if std::mem::take(&mut self.restart) {
                        if let Err(e) = self.spawn(Verb::Start, now, false) {
                            self.st.state = HotspotState::Failed;
                            self.st.reason = Some(e);
                        } else {
                            self.st.state = HotspotState::Starting;
                        }
                    }
                } else {
                    self.restart = false;
                    self.st.state = HotspotState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not stop the hotspot".into()));
                }
            }
            (Verb::Status, _) => {
                if code != Some(0) {
                    return;
                }
                self.apply_output(&out);
                match state {
                    Some("on") => self.st.state = HotspotState::On,
                    Some("failed") => {
                        self.st.state = HotspotState::Failed;
                        self.st.reason = reason;
                        self.st.clients = None;
                    }
                    Some("off") | None => {
                        self.st.state = HotspotState::Off;
                        self.st.clients = None;
                    }
                    Some(_) => {}
                }
                self.next_status = now + STATUS_EVERY;
            }
        }
    }

    /// Advance the running script. Returns true when the visible status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st.clone();
        if let Some(mut r) = self.run.take() {
            match r.child.try_wait() {
                Ok(Some(es)) => self.finish(r, es.code(), now),
                Ok(None) if now >= r.deadline => {
                    let verb = r.verb;
                    Self::kill(r);
                    match verb {
                        Verb::Status => self.next_status = now + STATUS_EVERY,
                        Verb::Start | Verb::Stop => {
                            self.restart = false;
                            self.st.state = HotspotState::Failed;
                            self.st.reason =
                                Some(format!("walpad-hotspot {} timed out", verb.arg()));
                            self.st.clients = None;
                            if verb == Verb::Start {
                                // tear down whatever the start left behind
                                let _ = self.spawn(Verb::Stop, now, true);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.st.state == HotspotState::On
            && now >= self.next_status
            && self.spawn(Verb::Status, now, false).is_err()
        {
            self.next_status = now + STATUS_EVERY;
        }
        self.st != before
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conf_parsing() {
        assert_eq!(parse_conf(""), (None, false));
        assert_eq!(
            parse_conf("# x\nssid2=57616c506164\nwpa_passphrase=abcdefgh\n"),
            (Some("WalPad".into()), true)
        );
        assert_eq!(parse_conf("wpa_passphrase=\n"), (None, false));
    }

    #[test]
    fn output_fields_and_reasons() {
        let out = "state=failed\nreason=hostapd is not installed\x07\nclients=3\n";
        assert_eq!(field(out, "state"), Some("failed"));
        assert_eq!(field(out, "clients"), Some("3"));
        assert_eq!(field(out, "channel"), None);
        assert_eq!(
            clean_reason(field(out, "reason").unwrap()),
            "hostapd is not installed"
        );
        assert_eq!(clean_reason(&"x".repeat(500)).len(), 120);
    }
}
