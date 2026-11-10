//! VPN, DNS and proxy (owner request 2026-11-10, change vpn-and-dns). Everything that touches the
//! network is done by the fixed script /usr/local/sbin/walpad-vpn (up|down|status|apply|confirm),
//! run as a child with a timeout and polled from the daemon loop (never waited on). The core owns
//! the non-secret settings in /etc/walpad/vpn.conf and the profile store /var/lib/walpad/vpn
//! (0700, files 0600). Profile content (keys, passwords) is validated on import and then only read
//! by the script's tunnel programs: it never reaches argv, a log, a reply or the status.
use crate::hotspot::write_600;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{
    valid_dns_server, valid_proxy, valid_vpn_name, VpnKind, VpnProfile, VpnState, VpnStatus,
};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-vpn";
pub const CONF: &str = "/etc/walpad/vpn.conf";
pub const STORE: &str = "/var/lib/walpad/vpn";
const UP_TIMEOUT: Duration = Duration::from_secs(90);
const DOWN_TIMEOUT: Duration = Duration::from_secs(30);
const STATUS_TIMEOUT: Duration = Duration::from_secs(8);
const APPLY_TIMEOUT: Duration = Duration::from_secs(15);
const STATUS_EVERY: Duration = Duration::from_secs(5);
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Largest profile accepted on import.
pub const MAX_PROFILE: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Up,
    Down,
    Status,
    Apply,
    Confirm,
}

impl Verb {
    fn arg(self) -> &'static str {
        match self {
            Verb::Up => "up",
            Verb::Down => "down",
            Verb::Status => "status",
            Verb::Apply => "apply",
            Verb::Confirm => "confirm",
        }
    }
    fn idx(self) -> usize {
        self as usize
    }
}

struct Run {
    verb: Verb,
    child: Child,
    deadline: Instant,
    /// A cleanup `down` after a failed/timed-out `up`: it must not overwrite the failure.
    cleanup: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VpnError {
    Busy,
    Invalid(String),
    Failed(String),
}

pub struct Vpn {
    root: Root,
    script: PathBuf,
    conf: PathBuf,
    store: PathBuf,
    out: PathBuf,
    run: Option<Run>,
    st: VpnStatus,
    /// Settings changed while a script ran: apply DNS/proxy when it is done.
    apply_pending: bool,
    next_status: Instant,
    timeouts: [Duration; 5],
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

fn kind_of_name(file: &str) -> Option<VpnKind> {
    if file.ends_with(".conf") {
        Some(VpnKind::Wireguard)
    } else if file.ends_with(".ovpn") {
        Some(VpnKind::Openvpn)
    } else {
        None
    }
}

fn kind_ext(k: VpnKind) -> &'static str {
    match k {
        VpnKind::Wireguard => "conf",
        VpnKind::Openvpn => "ovpn",
    }
}

/// Settings from /etc/walpad/vpn.conf.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Settings {
    pub profile: Option<String>,
    pub kill_switch: bool,
    pub dns: Vec<String>,
    pub proxy: Option<String>,
}

pub fn parse_settings(text: &str) -> Settings {
    let mut s = Settings::default();
    for l in text.lines() {
        let Some((k, v)) = l.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k {
            "profile" if valid_vpn_name(v) => s.profile = Some(v.to_string()),
            "kill_switch" => s.kill_switch = v == "1",
            "dns" => {
                s.dns = v
                    .split(',')
                    .map(str::trim)
                    .filter(|d| valid_dns_server(d))
                    .take(3)
                    .map(str::to_string)
                    .collect()
            }
            "proxy" if valid_proxy(v) => s.proxy = Some(v.to_string()),
            _ => {}
        }
    }
    s
}

fn render_settings(s: &Settings) -> String {
    format!(
        "# walpad-core vpn_config (owner request 2026-11-10); no secrets here\nprofile={}\nkill_switch={}\ndns={}\nproxy={}\n",
        s.profile.as_deref().unwrap_or(""),
        u8::from(s.kill_switch),
        s.dns.join(","),
        s.proxy.as_deref().unwrap_or("")
    )
}

// ---------------------------------------------------------------- import validation

/// Keys of a WireGuard config that run commands or change the host: refused.
const WG_FORBIDDEN: &[&str] = &["preup", "postup", "predown", "postdown"];
/// OpenVPN directives that run programs, include other files or write outside the store: refused.
const OVPN_FORBIDDEN: &[&str] = &[
    "up",
    "down",
    "route-up",
    "route-pre-down",
    "tls-verify",
    "auth-user-pass-verify",
    "client-connect",
    "client-disconnect",
    "learn-address",
    "ipchange",
    "plugin",
    "script-security",
    "setenv-safe",
    "config",
    "management",
    "writepid",
    "log",
    "log-append",
    "cd",
    "chroot",
    "daemon",
];

fn is_b64_key(v: &str) -> bool {
    v.len() == 44
        && v.ends_with('=')
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
}

/// Check a WireGuard config. The error text never contains config content.
pub fn validate_wireguard(text: &str) -> Result<(), String> {
    let (mut section, mut has_iface, mut has_peer) = (String::new(), false, false);
    let (mut key, mut addr, mut pubkey, mut endpoint, mut allowed) =
        (false, false, false, false, false);
    for raw in text.lines() {
        let l = raw.split('#').next().unwrap_or("").trim();
        if l.is_empty() {
            continue;
        }
        if let Some(h) = l.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
            section = h.trim().to_ascii_lowercase();
            match section.as_str() {
                "interface" => has_iface = true,
                "peer" => has_peer = true,
                _ => return Err("unknown section in the WireGuard config".into()),
            }
            continue;
        }
        let Some((k, v)) = l.split_once('=') else {
            return Err("a line of the WireGuard config is not key = value".into());
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim());
        if WG_FORBIDDEN.contains(&k.as_str()) {
            return Err("the profile runs scripts (PreUp/PostUp): not allowed".into());
        }
        match (section.as_str(), k.as_str()) {
            ("interface", "privatekey") => {
                if !is_b64_key(v) {
                    return Err("PrivateKey is not a WireGuard key".into());
                }
                key = true;
            }
            ("interface", "address") => addr = !v.is_empty(),
            ("peer", "publickey") => {
                if !is_b64_key(v) {
                    return Err("PublicKey is not a WireGuard key".into());
                }
                pubkey = true;
            }
            ("peer", "endpoint") => endpoint = v.contains(':'),
            ("peer", "allowedips") => allowed = !v.is_empty(),
            _ => {}
        }
    }
    if !has_iface || !has_peer {
        return Err("a WireGuard config needs [Interface] and [Peer]".into());
    }
    if !key {
        return Err("the config has no PrivateKey".into());
    }
    if !addr {
        return Err("the config has no Address".into());
    }
    if !(pubkey && endpoint && allowed) {
        return Err("the peer needs PublicKey, Endpoint (host:port) and AllowedIPs".into());
    }
    Ok(())
}

/// Check an OpenVPN config (inline `<ca>` style blocks are skipped).
pub fn validate_openvpn(text: &str) -> Result<(), String> {
    let (mut remote, mut inline) = (false, false);
    for raw in text.lines() {
        let l = raw.trim();
        if inline {
            if l.starts_with("</") {
                inline = false;
            }
            continue;
        }
        if l.is_empty() || l.starts_with('#') || l.starts_with(';') {
            continue;
        }
        if l.starts_with('<') {
            inline = !l.contains("</");
            continue;
        }
        let mut it = l.split_whitespace();
        let d = it.next().unwrap_or("").trim_start_matches("--");
        if OVPN_FORBIDDEN.contains(&d) {
            return Err("the profile uses a directive that runs programs or changes files".into());
        }
        if d == "auth-user-pass" && it.next().is_none() {
            return Err(
                "the profile asks for a password interactively: embed it or drop it".into(),
            );
        }
        if d == "remote" && it.next().is_some() {
            remote = true;
        }
    }
    if inline {
        return Err("an inline block is not closed".into());
    }
    if !remote {
        return Err("the profile has no remote server".into());
    }
    Ok(())
}

/// A profile name from a file name: stem, invalid characters to `-`, at most 24.
pub fn name_from_path(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vpn");
    let mut n: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '-'
            }
        })
        .take(24)
        .collect();
    while n.starts_with('.') {
        n.remove(0);
    }
    if n.is_empty() {
        "vpn".into()
    } else {
        n
    }
}

/// Read a candidate file: a regular file (no symlink), small, UTF-8.
fn read_candidate(path: &Path) -> Result<String, String> {
    let md = std::fs::symlink_metadata(path).map_err(|_| "cannot read the file".to_string())?;
    if !md.file_type().is_file() {
        return Err("not a regular file".into());
    }
    if md.len() > MAX_PROFILE {
        return Err("the file is too large for a VPN profile".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "cannot read the file".to_string())?;
    String::from_utf8(bytes).map_err(|_| "the file is not text".to_string())
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

    fn settings(&self) -> Settings {
        parse_settings(&std::fs::read_to_string(&self.conf).unwrap_or_default())
    }

    fn load(&mut self) {
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

    fn list_profiles(&self) -> Vec<VpnProfile> {
        let mut v: Vec<VpnProfile> = std::fs::read_dir(&self.store)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let f = e.file_name().to_string_lossy().into_owned();
                let kind = kind_of_name(&f)?;
                let name = f.rsplit_once('.')?.0.to_string();
                valid_vpn_name(&name).then_some(VpnProfile { name, kind })
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
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
        if self.root.base() != Path::new("/") {
            cmd.env("WALPAD_VPN_ROOT", self.root.base());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-vpn is not installed".to_string(),
            _ => format!("cannot run walpad-vpn: {e}"),
        })?;
        self.run = Some(Run {
            verb,
            child,
            deadline: now + self.timeouts[verb.idx()],
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

    /// `vpn_import`: validate and store a profile (0600 file in the 0700 store).
    pub fn import(&mut self, path: &str, name: Option<&str>) -> Result<VpnProfile, VpnError> {
        let kind =
            kind_of_name(path).ok_or_else(|| VpnError::Invalid("not a profile file".into()))?;
        let name = name.map_or_else(|| name_from_path(path), str::to_string);
        if !valid_vpn_name(&name) {
            return Err(VpnError::Invalid("invalid profile name".into()));
        }
        let text = read_candidate(&self.root.p(path)).map_err(VpnError::Invalid)?;
        match kind {
            VpnKind::Wireguard => validate_wireguard(&text),
            VpnKind::Openvpn => validate_openvpn(&text),
        }
        .map_err(VpnError::Invalid)?;
        if self.st.profiles.iter().any(|p| p.name == name) {
            return Err(VpnError::Invalid(
                "a profile with that name exists: remove it first".into(),
            ));
        }
        self.ensure_store()
            .map_err(|e| VpnError::Failed(format!("cannot create the profile store: {e}")))?;
        let dest = self.store.join(format!("{name}.{}", kind_ext(kind)));
        write_600(&dest, text.as_bytes())
            .map_err(|e| VpnError::Failed(format!("cannot store the profile: {e}")))?;
        self.load();
        // the first profile becomes the selected one
        if self.st.profile.is_none() {
            let mut s = self.settings();
            s.profile = Some(name.clone());
            let _ = write_600(&self.conf, render_settings(&s).as_bytes());
            self.load();
        }
        Ok(VpnProfile { name, kind })
    }

    fn ensure_store(&self) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&self.store)?;
        std::fs::set_permissions(&self.store, std::fs::Permissions::from_mode(0o700))
    }

    /// `vpn_remove`: not the running profile.
    pub fn remove(&mut self, name: &str) -> Result<(), VpnError> {
        if self.st.state != VpnState::Off && self.st.profile.as_deref() == Some(name) {
            return Err(VpnError::Invalid("turn the VPN off first".into()));
        }
        let p = self
            .st
            .profiles
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| VpnError::Invalid("no such VPN profile".into()))?;
        std::fs::remove_file(self.store.join(format!("{name}.{}", kind_ext(p.kind))))
            .map_err(|e| VpnError::Failed(format!("cannot remove the profile: {e}")))?;
        if self.st.profile.as_deref() == Some(name) {
            let mut s = self.settings();
            s.profile = None;
            let _ = write_600(&self.conf, render_settings(&s).as_bytes());
        }
        self.load();
        Ok(())
    }

    /// Importable profile files in the user folders: (name, path, kind). Only files that pass
    /// the import validation are listed.
    pub fn candidates(&self) -> Vec<(String, String, VpnKind)> {
        let mut dirs: Vec<String> = vec![
            "/root/Downloads".into(),
            "/root/Documents".into(),
            "/var/lib/walpad/import".into(),
        ];
        for base in ["/media", "/mnt"] {
            for e in std::fs::read_dir(self.root.p(base))
                .into_iter()
                .flatten()
                .flatten()
            {
                dirs.push(format!("{base}/{}", e.file_name().to_string_lossy()));
            }
        }
        let mut out = Vec::new();
        for d in dirs {
            for e in std::fs::read_dir(self.root.p(&d))
                .into_iter()
                .flatten()
                .flatten()
            {
                let f = e.file_name().to_string_lossy().into_owned();
                let Some(kind) = kind_of_name(&f) else {
                    continue;
                };
                let path = format!("{d}/{f}");
                if !walpad_proto::valid_vpn_import_path(&path) {
                    continue;
                }
                let Ok(text) = read_candidate(&self.root.p(&path)) else {
                    continue;
                };
                let ok = match kind {
                    VpnKind::Wireguard => validate_wireguard(&text),
                    VpnKind::Openvpn => validate_openvpn(&text),
                };
                if ok.is_ok() {
                    out.push((name_from_path(&path), path, kind));
                }
                if out.len() >= 30 {
                    return out;
                }
            }
        }
        out
    }

    fn apply_output(&mut self, out: &str) {
        self.st.handshake_age_s = field(out, "handshake_age").and_then(|v| v.parse().ok());
        self.st.rx_bytes = field(out, "rx").and_then(|v| v.parse().ok());
        self.st.tx_bytes = field(out, "tx").and_then(|v| v.parse().ok());
        self.st.revert_in_s = field(out, "revert_in").and_then(|v| v.parse().ok());
        if let Some(k) = field(out, "kind") {
            self.st.kind = match k {
                "wireguard" => Some(VpnKind::Wireguard),
                "openvpn" => Some(VpnKind::Openvpn),
                _ => self.st.kind,
            };
        }
    }

    fn clear_live(&mut self) {
        self.st.handshake_age_s = None;
        self.st.rx_bytes = None;
        self.st.tx_bytes = None;
        self.st.revert_in_s = None;
    }

    fn finish(&mut self, r: Run, code: Option<i32>, now: Instant) {
        let out = std::fs::read_to_string(&self.out).unwrap_or_default();
        let reason = field(&out, "reason").map(clean_reason);
        let state = field(&out, "state");
        match (r.verb, r.cleanup) {
            (Verb::Down, true) => self.clear_live(),
            (Verb::Up, _) => {
                if code == Some(0) && state == Some("on") {
                    self.apply_output(&out);
                    self.st.state = VpnState::On;
                    self.st.reason = None;
                    self.next_status = now + STATUS_EVERY;
                } else {
                    self.clear_live();
                    self.st.state = VpnState::Failed;
                    self.st.reason = Some(reason.unwrap_or_else(|| match code {
                        Some(c) => format!("walpad-vpn failed (exit {c})"),
                        None => "walpad-vpn was killed".into(),
                    }));
                }
            }
            (Verb::Down, false) => {
                self.clear_live();
                if code == Some(0) {
                    self.st.state = VpnState::Off;
                    self.st.reason = None;
                } else {
                    self.st.state = VpnState::Failed;
                    self.st.reason =
                        Some(reason.unwrap_or_else(|| "could not turn the VPN off".into()));
                }
            }
            (Verb::Confirm, _) => {
                if code != Some(0) {
                    self.st.reason = reason;
                }
            }
            (Verb::Apply, _) => {
                // a tunnel that survived a core restart shows up in the next status poll
                if matches!(self.st.state, VpnState::Off) {
                    let _ = self.spawn(Verb::Status, now, false);
                }
            }
            (Verb::Status, _) => {
                // the script exits 1 for state=failed: the state field decides, not the exit code
                let Some(state) = state else { return };
                match state {
                    "on" => {
                        self.apply_output(&out);
                        self.st.state = VpnState::On;
                        self.st.reason = None;
                    }
                    "starting" => {
                        self.apply_output(&out);
                        self.st.state = VpnState::Starting;
                        self.st.reason = reason;
                    }
                    "failed" => {
                        self.clear_live();
                        self.st.state = VpnState::Failed;
                        self.st.reason = reason;
                    }
                    "off" => {
                        self.clear_live();
                        if matches!(self.st.state, VpnState::On | VpnState::Starting) {
                            self.st.state = VpnState::Off;
                        }
                    }
                    _ => {}
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
                        Verb::Status | Verb::Apply | Verb::Confirm => {
                            self.next_status = now + STATUS_EVERY
                        }
                        Verb::Up | Verb::Down => {
                            self.clear_live();
                            self.st.state = VpnState::Failed;
                            self.st.reason = Some(format!("walpad-vpn {} timed out", verb.arg()));
                            if verb == Verb::Up {
                                // tear down whatever the start left behind (routes, kill switch)
                                let _ = self.spawn(Verb::Down, now, true);
                            }
                        }
                    }
                }
                Ok(None) => self.run = Some(r),
                Err(_) => Self::kill(r),
            }
        } else if self.apply_pending {
            self.apply_pending = false;
            let _ = self.spawn(Verb::Apply, now, false);
        } else if matches!(self.st.state, VpnState::On | VpnState::Starting)
            && now >= self.next_status
            && self.spawn(Verb::Status, now, false).is_err()
        {
            self.next_status = now + STATUS_EVERY;
        }
        self.st != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

    fn wg(extra: &str) -> String {
        format!(
            "[Interface]\nPrivateKey = {KEY}\nAddress = 10.8.0.2/32\nDNS = 10.8.0.1\n{extra}\n[Peer]\nPublicKey = {KEY}\nEndpoint = vpn.example.net:51820\nAllowedIPs = 0.0.0.0/0\n"
        )
    }

    #[test]
    fn wireguard_validation() {
        assert_eq!(validate_wireguard(&wg("")), Ok(()));
        assert!(validate_wireguard(&wg("PostUp = touch /x")).is_err());
        assert!(validate_wireguard(&wg("preup=x")).is_err());
        assert!(validate_wireguard("[Interface]\nPrivateKey = short\n").is_err());
        assert!(validate_wireguard("").is_err());
        assert!(validate_wireguard(&wg("").replace("Endpoint", "Endpoin")).is_err());
        assert!(validate_wireguard(&wg("").replace("Address", "Adr")).is_err());
        assert!(validate_wireguard("[Foo]\nx = y\n").is_err());
        // an error never quotes the content
        let bad = wg("").replace(KEY, "notakey-SECRETVALUE");
        assert!(!validate_wireguard(&bad)
            .unwrap_err()
            .contains("SECRETVALUE"));
    }

    #[test]
    fn openvpn_validation() {
        let ok = "client\nremote 198.51.100.7 1194\n<ca>\nup not-a-directive\n</ca>\nauth-user-pass creds.txt\n";
        assert_eq!(validate_openvpn(ok), Ok(()));
        assert!(validate_openvpn("client\n").is_err());
        assert!(validate_openvpn("remote h 1\nup /tmp/x.sh\n").is_err());
        assert!(validate_openvpn("remote h 1\n--script-security 2\n").is_err());
        assert!(validate_openvpn("remote h 1\nplugin foo.so\n").is_err());
        assert!(validate_openvpn("remote h 1\nauth-user-pass\n").is_err());
        assert!(validate_openvpn("remote h 1\nconfig other.ovpn\n").is_err());
        assert!(validate_openvpn("remote h 1\n<ca>\n").is_err());
    }

    #[test]
    fn settings_roundtrip_and_validation() {
        let s = Settings {
            profile: Some("home".into()),
            kill_switch: true,
            dns: vec!["1.1.1.1".into(), "2606:4700::1111".into()],
            proxy: Some("http://10.0.0.2:3128".into()),
        };
        assert_eq!(parse_settings(&render_settings(&s)), s);
        let bad = parse_settings(
            "profile=../x\ndns=evil;rm,1.1.1.1\nproxy=http://u:p@h:1\nkill_switch=0\n",
        );
        assert_eq!(
            bad,
            Settings {
                profile: None,
                kill_switch: false,
                dns: vec!["1.1.1.1".into()],
                proxy: None
            }
        );
    }

    #[test]
    fn names_from_paths() {
        assert_eq!(
            name_from_path("/root/Downloads/my vpn (1).conf"),
            "my-vpn--1-"
        );
        assert_eq!(name_from_path("/root/.hidden.ovpn"), "hidden");
        assert!(!name_from_path("/root/").is_empty());
        assert_eq!(
            name_from_path(&format!("/r/{}.conf", "a".repeat(40))).len(),
            24
        );
    }

    #[test]
    fn output_fields() {
        let out = "state=failed\nreason=no handshake\x07\nrx=10\n";
        assert_eq!(field(out, "state"), Some("failed"));
        assert_eq!(field(out, "rx"), Some("10"));
        assert_eq!(field(out, "tx"), None);
        assert_eq!(clean_reason(field(out, "reason").unwrap()), "no handshake");
    }
}
