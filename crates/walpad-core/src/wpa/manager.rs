//! Wi-Fi manager: single-flight scan, connect/forget/disconnect and live status over the supplicant
//! control socket. Non-blocking from the daemon's point of view: `scan` and `connect` return at once
//! and finish through `WifiEvent`s produced by `tick`.
use super::ctrl::{CtrlClient, CtrlError};
use super::parse::{self, Saved};
use super::psk::{self, PskError};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::Secret;

const SCAN_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(25);
const RECONNECT_EVERY: Duration = Duration::from_secs(2);
const STATUS_EVERY: Duration = Duration::from_secs(4);
const CONNECT_POLL: Duration = Duration::from_millis(250);

mod poll;
mod types;

use types::{map, Op};
pub use types::{WifiError, WifiEvent, WifiLive};

pub struct Wifi {
    ctrl: Option<CtrlClient>,
    server: PathBuf,
    run_dir: PathBuf,
    conf: PathBuf,
    op: Op,
    next_reconnect: Instant,
    next_status: Instant,
    live: WifiLive,
    connect_timeout: Duration,
    reconnect_every: Duration,
}

impl Wifi {
    pub fn new(root: &Root) -> Self {
        let now = Instant::now();
        Wifi {
            ctrl: None,
            server: root.p("/run/wpa_supplicant/wlan0"),
            run_dir: root.p("/run/walpad"),
            conf: root.p("/etc/wpa_supplicant/wpa_supplicant.conf"),
            op: Op::Idle,
            next_reconnect: now,
            next_status: now,
            live: WifiLive::default(),
            connect_timeout: CONNECT_TIMEOUT,
            reconnect_every: RECONNECT_EVERY,
        }
    }

    /// Shorter timings for tests.
    pub fn set_timing(&mut self, connect_timeout: Duration, reconnect_every: Duration) {
        self.connect_timeout = connect_timeout;
        self.reconnect_every = reconnect_every;
    }

    pub fn live(&self) -> &WifiLive {
        &self.live
    }
    pub fn connected(&self) -> bool {
        self.ctrl.is_some()
    }
    pub fn busy(&self) -> bool {
        !matches!(self.op, Op::Idle)
    }

    /// Run a request; a transport failure drops the client so `tick` reconnects.
    fn req(&mut self, cmd: &str) -> Result<String, WifiError> {
        let Some(c) = self.ctrl.as_ref() else {
            return Err(WifiError::Unavailable);
        };
        match c.request(cmd) {
            Ok(t) => Ok(t),
            Err(e @ CtrlError::Fail(_)) => Err(map(e)),
            Err(e) => {
                self.ctrl = None;
                self.op = Op::Idle;
                self.live = WifiLive::default();
                Err(map(e))
            }
        }
    }

    fn ensure_mode_600(&self) {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(&self.conf) {
            if m.permissions().mode() & 0o077 != 0 {
                let _ =
                    std::fs::set_permissions(&self.conf, std::fs::Permissions::from_mode(0o600));
            }
        }
    }

    fn save_config(&mut self) {
        let _ = self.req("SAVE_CONFIG");
        self.ensure_mode_600();
    }

    pub fn list_saved(&mut self) -> Result<Vec<Saved>, WifiError> {
        Ok(parse::parse_list_networks(&self.req("LIST_NETWORKS")?))
    }

    // ---------------------------------------------------------------- commands
    /// Single flight: a second call while scanning (or connecting) is `Busy`.
    pub fn scan(&mut self, now: Instant) -> Result<(), WifiError> {
        if self.busy() {
            return Err(WifiError::Busy);
        }
        self.req("SCAN").or_else(|e| match e {
            // the supplicant is already scanning: results will arrive as an event
            WifiError::Failed(m) if m.contains("BUSY") => Ok(String::new()),
            e => Err(e),
        })?;
        self.op = Op::Scanning {
            deadline: now + SCAN_TIMEOUT,
        };
        Ok(())
    }

    pub fn connect(
        &mut self,
        ssid: Vec<u8>,
        secret: Option<&Secret>,
        saved_id: Option<u32>,
        now: Instant,
    ) -> Result<(), WifiError> {
        if self.busy() {
            return Err(WifiError::Busy);
        }
        let saved = self.list_saved()?;
        let (id, ssid, was_new) = match saved_id {
            Some(id) => {
                let s = saved
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| WifiError::Invalid("unknown saved network".into()))?;
                (id, s.ssid.clone(), false)
            }
            None => {
                if ssid.is_empty() || ssid.len() > 32 {
                    return Err(WifiError::Invalid("ssid must be 1-32 bytes".into()));
                }
                // validate the secret before touching the supplicant
                let psk = match secret {
                    Some(s) => Some(psk::psk_hex(s.expose(), &ssid).map_err(|e| {
                        WifiError::Invalid(match e {
                            PskError::BadLength => {
                                "passphrase must be 8-63 characters or a 64-digit hex key".into()
                            }
                        })
                    })?),
                    None => None,
                };
                let existing = saved.iter().find(|s| s.ssid == ssid).map(|s| s.id);
                let (id, was_new) = match existing {
                    Some(id) => (id, false),
                    None => {
                        let t = self.req("ADD_NETWORK")?;
                        (
                            t.trim()
                                .parse::<u32>()
                                .map_err(|_| WifiError::Failed("ADD_NETWORK".into()))?,
                            true,
                        )
                    }
                };
                let r = (|| {
                    self.req(&format!("SET_NETWORK {id} ssid {}", parse::hex(&ssid)))?;
                    match &psk {
                        Some(k) => self.req(&format!("SET_NETWORK {id} psk {k}"))?,
                        None => self.req(&format!("SET_NETWORK {id} key_mgmt NONE"))?,
                    };
                    Ok::<(), WifiError>(())
                })();
                if let Err(e) = r {
                    if was_new {
                        let _ = self.req(&format!("REMOVE_NETWORK {id}"));
                    }
                    return Err(e);
                }
                (id, ssid, was_new)
            }
        };
        let others: Vec<u32> = saved.iter().map(|s| s.id).filter(|&i| i != id).collect();
        if let Err(e) = self.req(&format!("SELECT_NETWORK {id}")) {
            if was_new {
                let _ = self.req(&format!("REMOVE_NETWORK {id}"));
            }
            return Err(e);
        }
        self.op = Op::Connecting {
            id,
            ssid,
            deadline: now + self.connect_timeout,
            was_new,
            others,
            next_poll: now + CONNECT_POLL,
        };
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), WifiError> {
        self.req("DISCONNECT").map(|_| ())
    }

    pub fn forget(&mut self, id: u32) -> Result<(), WifiError> {
        self.req(&format!("REMOVE_NETWORK {id}"))?;
        self.save_config();
        Ok(())
    }

    // ---------------------------------------------------------------- progress
}
