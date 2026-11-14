//! VPN, DNS and proxy (owner request 2026-11-10, change vpn-and-dns). Everything that touches the
//! network is done by the fixed script /usr/local/sbin/walpad-vpn (up|down|status|apply|confirm),
//! run as a child with a timeout and polled from the daemon loop (never waited on). The core owns
//! the non-secret settings in /etc/walpad/vpn.conf and the profile store /var/lib/walpad/vpn
//! (0700, files 0600). Profile content (keys, passwords) is validated on import and then only read
//! by the script's tunnel programs: it never reaches argv, a log, a reply or the status.

mod config;
mod profiles;
mod runner;

#[cfg(test)]
mod tests;

pub use config::{name_from_path, parse_settings, validate_openvpn, validate_wireguard, Settings};

use crate::hotspot::write_600;
use config::render_settings;
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{valid_dns_server, valid_proxy, VpnState, VpnStatus};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-vpn";
pub const CONF: &str = "/etc/walpad/vpn.conf";
pub const STORE: &str = "/var/lib/walpad/vpn";
pub(crate) const UP_TIMEOUT: Duration = Duration::from_secs(90);
pub(crate) const DOWN_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
pub(crate) const APPLY_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const STATUS_EVERY: Duration = Duration::from_secs(5);
/// Largest profile accepted on import.
pub const MAX_PROFILE: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    Up,
    Down,
    Status,
    Apply,
    Confirm,
}

impl Verb {
    pub(crate) fn arg(self) -> &'static str {
        match self {
            Verb::Up => "up",
            Verb::Down => "down",
            Verb::Status => "status",
            Verb::Apply => "apply",
            Verb::Confirm => "confirm",
        }
    }
    pub(crate) fn idx(self) -> usize {
        self as usize
    }
}

pub(crate) struct Run {
    pub(crate) verb: Verb,
    pub(crate) child: Child,
    pub(crate) deadline: Instant,
    /// A cleanup `down` after a failed/timed-out `up`: it must not overwrite the failure.
    pub(crate) cleanup: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VpnError {
    Busy,
    Invalid(String),
    Failed(String),
}

pub struct Vpn {
    pub(crate) root: Root,
    pub(crate) script: PathBuf,
    pub(crate) conf: PathBuf,
    pub(crate) store: PathBuf,
    pub(crate) out: PathBuf,
    pub(crate) run: Option<Run>,
    pub(crate) st: VpnStatus,
    /// Settings changed while a script ran: apply DNS/proxy when it is done.
    pub(crate) apply_pending: bool,
    pub(crate) next_status: Instant,
    pub(crate) timeouts: [Duration; 5],
}

impl Vpn {
    pub fn new(root: &Root) -> Self {
        let mut v = Vpn {
            root: root.clone(),
            script: root.p(SCRIPT),
            conf: root.p(CONF),
            store: root.p(STORE),
            out: root.p("/run/walpad/vpn-out"),
            run: None,
            st: VpnStatus::default(),
            apply_pending: false,
            next_status: Instant::now(),
            timeouts: [
                UP_TIMEOUT,
                DOWN_TIMEOUT,
                STATUS_TIMEOUT,
                APPLY_TIMEOUT,
                APPLY_TIMEOUT,
            ],
        };
        v.load();
        v
    }

    /// Shorter timeouts for tests: up, down, status.
    pub fn set_timeouts(&mut self, up: Duration, down: Duration, status: Duration) {
        self.timeouts = [up, down, status, APPLY_TIMEOUT, APPLY_TIMEOUT];
    }

    pub(crate) fn settings(&self) -> Settings {
        parse_settings(&std::fs::read_to_string(&self.conf).unwrap_or_default())
    }

    pub(crate) fn load(&mut self) {
        let s = self.settings();
        self.st.kill_switch = s.kill_switch;
        self.st.dns = s.dns;
        self.st.proxy = s.proxy;
        self.st.profiles = self.list_profiles();
        self.st.profile = s
            .profile
            .filter(|p| self.st.profiles.iter().any(|x| &x.name == p));
        self.st.kind = self
            .st
            .profile
            .as_ref()
            .and_then(|p| self.st.profiles.iter().find(|x| &x.name == p))
            .map(|x| x.kind);
    }

    pub fn status(&self) -> &VpnStatus {
        &self.st
    }

    /// `http://host:port` for the environment of launched apps.
    pub fn proxy(&self) -> Option<&str> {
        self.st.proxy.as_deref()
    }

    pub fn busy(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| r.verb != Verb::Status || r.cleanup)
    }

    /// Startup: the tunnel may outlive a core restart, and the DNS and proxy settings are
    /// applied again (a reboot clears /run, a DHCP client may have rewritten resolv.conf).
    pub fn startup(&mut self, now: Instant) {
        let _ = self.spawn(Verb::Apply, now, false);
    }

    /// `vpn {on}`. Returns the state right after the request.
    pub fn set_on(&mut self, on: bool, now: Instant) -> Result<VpnState, VpnError> {
        if self.busy() {
            return Err(VpnError::Busy);
        }
        if on {
            if self.st.profile.is_none() {
                return Err(VpnError::Invalid(
                    "import and select a VPN profile first".into(),
                ));
            }
            if matches!(self.st.state, VpnState::On | VpnState::Starting) {
                return Ok(self.st.state);
            }
            self.spawn(Verb::Up, now, false).map_err(VpnError::Failed)?;
            self.st.state = VpnState::Starting;
        } else {
            if self.st.state == VpnState::Off && self.run.is_none() {
                return Ok(VpnState::Off);
            }
            self.spawn(Verb::Down, now, false)
                .map_err(VpnError::Failed)?;
            self.st.state = VpnState::Stopping;
        }
        self.st.reason = None;
        Ok(self.st.state)
    }

    /// `vpn_confirm`: keep the kill switch.
    pub fn confirm(&mut self, now: Instant) -> Result<(), VpnError> {
        if self.busy() {
            return Err(VpnError::Busy);
        }
        self.spawn(Verb::Confirm, now, false)
            .map_err(VpnError::Failed)?;
        self.st.revert_in_s = None;
        Ok(())
    }

    /// `vpn_config`. Profile and kill switch only while the tunnel is off; DNS and proxy any time.
    pub fn configure(
        &mut self,
        profile: Option<&str>,
        kill_switch: Option<bool>,
        dns: Option<&[String]>,
        proxy: Option<&str>,
        now: Instant,
    ) -> Result<(), VpnError> {
        if profile.is_some() || kill_switch.is_some() {
            if self.st.state != VpnState::Off && self.st.state != VpnState::Failed {
                return Err(VpnError::Invalid("turn the VPN off first".into()));
            }
            if self.busy() {
                return Err(VpnError::Busy);
            }
        }
        if let Some(p) = profile {
            if !self.st.profiles.iter().any(|x| x.name == p) {
                return Err(VpnError::Invalid("no such VPN profile".into()));
            }
        }
        if dns.is_some_and(|d| d.len() > 3 || d.iter().any(|s| !valid_dns_server(s)))
            || proxy.is_some_and(|p| !p.is_empty() && !valid_proxy(p))
        {
            return Err(VpnError::Invalid("invalid DNS or proxy".into()));
        }
        let mut s = self.settings();
        if let Some(p) = profile {
            s.profile = Some(p.to_string());
        }
        if let Some(k) = kill_switch {
            s.kill_switch = k;
        }
        if let Some(d) = dns {
            s.dns = d.to_vec();
        }
        if let Some(p) = proxy {
            s.proxy = (!p.is_empty()).then(|| p.to_string());
        }
        write_600(&self.conf, render_settings(&s).as_bytes())
            .map_err(|e| VpnError::Failed(format!("cannot store the VPN settings: {e}")))?;
        self.load();
        if dns.is_some() || proxy.is_some() {
            if self.busy() {
                self.apply_pending = true;
            } else {
                self.spawn(Verb::Apply, now, false)
                    .map_err(VpnError::Failed)?;
            }
        }
        Ok(())
    }
}
