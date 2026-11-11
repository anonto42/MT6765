//! The core poll loop: IPC socket, devd.cmd FIFO, input devices and timers in one thread, no async
//! runtime. The `Brain` decides; this module executes (sysfs writes, X requests, spawns).
use crate::apps::{terminate_pid, Apps, Exit, LaunchError};
use crate::bluetooth::{Bluetooth, BluetoothError};
use crate::brain::{Brain, Effect};
use crate::gnss::{Gnss, GnssError};
use crate::hotspot::{Hotspot, HotspotError};
use crate::inputs::Inputs;
use crate::lock::{Lock, LockError, Unlock};
use crate::logic::{battery, key_gestures::Action, screen_timeout as st, touch_map as tm};
use crate::modem::{MobileError, Modem};
use crate::pkg::{Op, Pkg, PkgError};
use crate::stats::Stats;
use crate::volume::Volume;
use crate::vpn::{Vpn, VpnError};
use crate::wpa::manager::{Wifi, WifiError, WifiEvent};
use crate::wpa::parse::{hex, ssid_display, unhex};
use crate::xbridge::{class_matches, XBridge, XError};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::{battery::read_battery, gsensor::read_gsensor, input, sysfs, Root};
use walpad_proto::{
    encode_line, parse_devd_line, parse_request, Command, ErrorCode, Event, Hardware, KeyboardMode,
    LegacyStatus, LineDecoder, PowerAction, Response, ScreenMode, ScreenState, ShellInput, Status,
    Topic, WifiRadio, WifiStatus, PROTO_VERSION,
};

mod windows;

const STATUS_EVERY: Duration = Duration::from_secs(2);
const BATTERY_EVERY: Duration = Duration::from_secs(10);
const CLIENT_STALL: Duration = Duration::from_secs(5);
const X_RETRY: Duration = Duration::from_secs(2);
const X_PING_EVERY: Duration = Duration::from_secs(5);
const APPS_X_EVERY: Duration = Duration::from_millis(700);
const MAX_OUT: usize = 256 * 1024;
/// Safety net: the shell closes its power menu after 20 s and says so (`power_menu` closed); if it
/// never does (crash), the core stops routing keys to it after this long.
const SHELL_MENU_MAX_MS: u64 = 25_000;

type CmdResult = Result<Value, (ErrorCode, String)>;

struct Client {
    stream: UnixStream,
    dec: LineDecoder,
    hello: bool,
    topics: Vec<Topic>,
    out: Vec<u8>,
    stalled_since: Option<Instant>,
    dead: bool,
    sync_pending: bool,
}

impl Client {
    fn queue(&mut self, bytes: Vec<u8>) {
        if self.out.len() + bytes.len() > MAX_OUT {
            self.dead = true;
            return;
        }
        self.out.extend(bytes);
        self.flush();
    }
    fn flush(&mut self) {
        while !self.out.is_empty() {
            match self.stream.write(&self.out) {
                Ok(0) => {
                    self.dead = true;
                    return;
                }
                Ok(n) => {
                    self.out.drain(..n);
                    self.stalled_since = None;
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.stalled_since.get_or_insert_with(Instant::now);
                    return;
                }
                Err(_) => {
                    self.dead = true;
                    return;
                }
            }
        }
    }
}

/// Which subsystems this core runs live (flags come from walpad-runtime via the init script).
struct LiveState {
    inputs: Inputs,
    device: bool,
    input: bool,
}

pub struct Daemon {
    root: Root,
    started: Instant,
    brain: Brain,
    listener: UnixListener,
    fifo: Option<std::fs::File>,
    fifo_buf: Vec<u8>,
    clients: Vec<Client>,
    last_pushed: Option<Status>,
    next_status: Instant,
    next_battery: Instant,
    sock_path: PathBuf,
    shadow: Option<crate::shadow_live::Live>,
    live: Option<LiveState>,
    apps: Apps,
    wifi: Wifi,
    hotspot: Hotspot,
    gnss: Gnss,
    stats: Stats,
    bluetooth: Bluetooth,
    pkg: Pkg,
    vpn: Vpn,
    modem: Modem,
    volume: Volume,
    lock: Lock,
    xb: Option<XBridge>,
    x_retry: Instant,
    x_ping: Instant,
    /// The shell's system keyboard is up (what the shell last reported or we last asked for).
    keyboard_shown: bool,
    apps_x_at: Instant,
    ext_close: Vec<ExtClose>,
    close_grace: Duration,
    /// Helper processes started by `spawn_simple` (keyboard, mixer, ...), reaped in `app_tick`.
    helpers: std::cell::RefCell<Vec<std::process::Child>>,
    /// Until when (ms) the shell's power menu owns the volume and power keys.
    shell_menu_until: Option<u64>,
    /// Window layout mode and the engine behind it (change multitasking-windows).
    layout: crate::layout::Layout,
    layout_at: Instant,
    win_closes: Vec<windows::WinClose>,
}

/// Close app for an app we did not start: WM_DELETE_WINDOW sent, SIGTERM of its pid at `deadline`
/// if its window is still there.
struct ExtClose {
    app: String,
    deadline: Instant,
}

fn read_file(root: &Root, p: &str) -> String {
    std::fs::read_to_string(root.p(p)).unwrap_or_default()
}

fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Milliseconds since the epoch: the screen lock's clock (its rate-limit state is persisted).
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn lock_err(e: LockError) -> (ErrorCode, String) {
    match e {
        LockError::WrongSecret { retry_in_s: None } => {
            (ErrorCode::Refused, "wrong PIN or password".into())
        }
        LockError::WrongSecret {
            retry_in_s: Some(s),
        }
        | LockError::Wait { retry_in_s: s } => (
            ErrorCode::Refused,
            format!("too many wrong attempts, wait {s} s"),
        ),
        LockError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        LockError::NotSet => (ErrorCode::Refused, "no screen lock is set".into()),
        LockError::Io(m) => (ErrorCode::Internal, m),
    }
}

fn internal<E: std::fmt::Display>(e: E) -> (ErrorCode, String) {
    (ErrorCode::Internal, e.to_string())
}

fn wifi_err(e: WifiError) -> (ErrorCode, String) {
    match e {
        WifiError::Busy => (ErrorCode::Busy, "another Wi-Fi operation is running".into()),
        WifiError::Unavailable => (
            ErrorCode::Internal,
            "Wi-Fi control is unavailable (wpa_supplicant socket)".into(),
        ),
        WifiError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        WifiError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn bluetooth_err(e: BluetoothError) -> (ErrorCode, String) {
    match e {
        BluetoothError::Busy(m) => (ErrorCode::Busy, m),
        BluetoothError::Refused(m) => (ErrorCode::Refused, m),
        BluetoothError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn mobile_err(e: MobileError) -> (ErrorCode, String) {
    match e {
        MobileError::Busy(m) => (ErrorCode::Busy, m),
        MobileError::Refused(m) => (ErrorCode::Refused, m),
        MobileError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        MobileError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn pkg_err(e: PkgError) -> (ErrorCode, String) {
    match e {
        PkgError::Busy => (
            ErrorCode::Busy,
            "another install or uninstall is running".into(),
        ),
        PkgError::Unknown(m) => (ErrorCode::InvalidArgs, m),
        PkgError::Refused(m) => (ErrorCode::Refused, m),
        PkgError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn gnss_err(e: GnssError) -> (ErrorCode, String) {
    match e {
        GnssError::Busy => (ErrorCode::Busy, "the GPS is starting or stopping".into()),
        GnssError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn hotspot_err(e: HotspotError) -> (ErrorCode, String) {
    match e {
        HotspotError::Busy => (
            ErrorCode::Busy,
            "the hotspot is starting or stopping".into(),
        ),
        HotspotError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        HotspotError::Failed(m) => (ErrorCode::Internal, m),
    }
}

fn vpn_err(e: VpnError) -> (ErrorCode, String) {
    match e {
        VpnError::Busy => (ErrorCode::Busy, "the VPN is starting or stopping".into()),
        VpnError::Invalid(m) => (ErrorCode::InvalidArgs, m),
        VpnError::Failed(m) => (ErrorCode::Internal, m),
    }
}

/// The owner's Wi-Fi switch (owner request 2026-11-10); read by walpad-wifi's loop.
pub const WIFI_ENABLED_FILE: &str = "/var/lib/walpad/wifi-enabled";

fn wifi_event_json(e: &WifiEvent) -> Value {
    match e {
        WifiEvent::Scan { networks } => json!({
            "kind": "scan",
            "networks": networks.iter().map(|n| json!({
                "ssid_hex": hex(&n.ssid), "ssid": ssid_display(&n.ssid), "signal_dbm": n.signal_dbm,
                "security": n.security.as_str(), "freq_mhz": n.freq_mhz,
            })).collect::<Vec<_>>(),
        }),
        WifiEvent::Connect { ok, error, ssid } => {
            json!({"kind": "connect", "ok": ok, "error": error, "ssid_hex": hex(ssid), "ssid": ssid_display(ssid)})
        }
        WifiEvent::Disconnected => json!({"kind": "disconnected"}),
    }
}

impl Daemon {
    pub fn new(root: Root) -> std::io::Result<Self> {
        std::fs::create_dir_all(root.p("/run/walpad"))?;
        let sock_path = root.p(walpad_proto::SOCKET_PATH);
        let _ = std::fs::remove_file(&sock_path);
        let listener = UnixListener::bind(&sock_path)?;
        listener.set_nonblocking(true)?;
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&sock_path, std::fs::Permissions::from_mode(0o600))?;
        }
        let fifo = open_fifo(&root)
            .map_err(|e| eprintln!("walpad-core: devd.cmd unavailable: {e}"))
            .ok();
        let now = Instant::now();
        let brain = Brain::new(
            &read_file(&root, "/etc/walpad/devd.json"),
            &read_file(&root, "/etc/walpad/keys.json"),
            &read_file(&root, "/etc/walpad/touch.json"),
            sysfs::read_backlight(&root),
            0,
        );
        let apps = Apps::load(&root);
        let mut d = Daemon {
            brain,
            listener,
            fifo,
            fifo_buf: Vec::new(),
            clients: Vec::new(),
            last_pushed: None,
            next_status: now,
            next_battery: now,
            started: now,
            sock_path,
            shadow: None,
            live: None,
            apps,
            wifi: Wifi::new(&root),
            hotspot: Hotspot::new(&root),
            gnss: Gnss::new(&root),
            stats: Stats::new(&root),
            bluetooth: Bluetooth::new(&root),
            pkg: Pkg::new(&root),
            vpn: Vpn::new(&root),
            modem: Modem::new(&root),
            volume: Volume::new(&root, now),
            lock: Lock::new(&root, wall_ms()),
            xb: None,
            x_retry: now,
            x_ping: now,
            keyboard_shown: false,
            apps_x_at: now,
            ext_close: Vec::new(),
            close_grace: Duration::from_secs(3),
            helpers: Default::default(),
            shell_menu_until: None,
            layout: windows::load_layout(&root),
            layout_at: now,
            win_closes: Vec::new(),
            root,
        };
        // orientation is 0 at startup and the file is always written (struts need X: later)
        let init = d.brain.rotation.init();
        let mut fx = Vec::new();
        Brain::rotation_effects(init, &mut fx);
        d.apply(fx, true);
        // mimeapps.list follows what is installed (app store, change app-catalogue-manager)
        let _ = d.pkg.apply_defaults();
        // a hotspot can outlive a core restart: pick up its real state
        d.hotspot.probe(now);
        // GPS (owner request 2026-11-11): a switched-on GPS comes up again after a boot
        d.gnss.probe(now);
        // VPN: pick up a running tunnel, apply the stored DNS and proxy (owner request 2026-11-10)
        d.vpn.startup(now);
        let px = d.vpn.proxy().map(str::to_string);
        d.apps.set_proxy(px.as_deref());
        // the modem: pick up a running one, or boot it once per boot (owner request 2026-11-10)
        d.modem.startup(now);
        Ok(d)
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.sock_path
    }
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }
    fn real_root(&self) -> bool {
        self.root.base() == std::path::Path::new("/")
    }

    /// Shadow mode: read the real inputs, log `WOULD ...`, write nothing by policy (M2.2).
    pub fn enable_shadow(&mut self) {
        self.shadow = Some(crate::shadow_live::Live::new(&self.root));
    }

    /// Live mode: run the device and/or input subsystem from the real inputs (M3.2/M3.3).
    pub fn enable_live(&mut self, device: bool, input: bool) {
        if !(device || input) {
            return;
        }
        self.live = Some(LiveState {
            inputs: Inputs::new(&self.root, "walpad-core"),
            device,
            input,
        });
        let init = self.brain.rotation.init();
        let mut fx = Vec::new();
        Brain::rotation_effects(init, &mut fx);
        self.apply(fx, true);
    }

    // ---------------------------------------------------------------- X bridge
    fn x_connect(&mut self) {
        if self.xb.is_some() || Instant::now() < self.x_retry {
            return;
        }
        match XBridge::connect(&self.root.p("/tmp/.X11-unix/X0")) {
            Ok(mut b) => {
                // X (re)started: restore struts and keyboard placement for the current orientation
                let o = self.brain.orientation();
                if let Err(e) = b.set_struts(o).and_then(|_| b.dock_keyboard(o).map(|_| ())) {
                    eprintln!("walpad-core: X resync failed: {}", e.0);
                    self.x_retry = Instant::now() + X_RETRY;
                } else {
                    eprintln!("walpad-core: connected to X");
                    self.xb = Some(b);
                }
            }
            Err(_) => self.x_retry = Instant::now() + X_RETRY,
        }
    }

    /// Run an X operation; on failure drop the bridge (it reconnects with backoff).
    fn x_try<T>(&mut self, f: impl FnOnce(&mut XBridge) -> Result<T, XError>) -> Option<T> {
        self.x_connect();
        let b = self.xb.as_mut()?;
        match f(b) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("walpad-core: X error: {}", e.0);
                if let Some(mut b) = self.xb.take() {
                    b.release_if_down();
                }
                self.x_retry = Instant::now() + X_RETRY;
                None
            }
        }
    }

    fn x_maintain(&mut self) {
        if self.xb.is_none() {
            self.x_connect();
        } else if Instant::now() >= self.x_ping {
            self.x_ping = Instant::now() + X_PING_EVERY;
            self.x_try(|b| b.ping());
        }
    }

    // ---------------------------------------------------------------- executor
    fn spawn_simple(&self, exe: &str, args: &[&str]) {
        let r = std::process::Command::new(exe)
            .args(args)
            .env("DISPLAY", ":0")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match r {
            Ok(child) => self.helpers.borrow_mut().push(child),
            Err(e) => eprintln!("walpad-core: cannot start {exe}: {e}"),
        }
    }

    fn halt(&self, cmd: &str) {
        if !self.real_root() {
            eprintln!("walpad-core: (test root) would run `{cmd} -f`");
            return;
        }
        rustix::fs::sync();
        self.spawn_simple(cmd, &["-f"]);
    }

    fn pids_by_comm(&self, prefix: &str) -> Vec<i32> {
        let Ok(rd) = std::fs::read_dir(self.root.p("/proc")) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|e| {
                let pid: i32 = e.file_name().to_str()?.parse().ok()?;
                let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
                // a zombie (state Z, after the comm's closing parenthesis) is not running: its
                // parent has not reaped it yet (orphans of an earlier core land on the ramdisk's
                // PID 1, which never reaps)
                let stat = std::fs::read_to_string(e.path().join("stat")).unwrap_or_default();
                let state = stat.rsplit_once(") ").and_then(|(_, r)| r.chars().next());
                (comm.trim().starts_with(prefix) && state != Some('Z')).then_some(pid)
            })
            .collect()
    }

    fn terminate(&self, prefix: &str) -> usize {
        let mut n = 0;
        for pid in self.pids_by_comm(prefix) {
            if let Some(p) = rustix::process::Pid::from_raw(pid) {
                if rustix::process::kill_process(p, rustix::process::Signal::TERM).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    /// The on-screen keyboard is the shell's system keyboard (owner request 2026-11-10: replaces
    /// matchbox-keyboard): ask the shell over the `input` topic. The shell reports its own changes
    /// (dock button, minimise key, Home) with `keyboard show|hide`, so `toggle` resolves against
    /// its real state. "unavailable" when no shell listens.
    fn keyboard(&mut self, mode: KeyboardMode) -> &'static str {
        if !self.shell_listening() {
            return "unavailable";
        }
        let show = match mode {
            KeyboardMode::Show => true,
            KeyboardMode::Hide => false,
            KeyboardMode::Toggle => !self.keyboard_shown,
        };
        self.keyboard_shown = show;
        self.publish_input(if show {
            ShellInput::KeyboardShow
        } else {
            ShellInput::KeyboardHide
        });
        if show {
            "shown"
        } else {
            "hidden"
        }
    }

    fn launch(
        &mut self,
        id: &str,
        path: Option<&str>,
    ) -> Result<&'static str, (ErrorCode, String)> {
        let Some(def) = self.apps.def(id).cloned() else {
            return Err((ErrorCode::InvalidArgs, format!("unknown app {id:?}")));
        };
        if !self.apps.installed(&def, &self.root) {
            return Err((ErrorCode::Refused, format!("{id} is not installed")));
        }
        if self.pkg.busy_with(id) {
            return Err((
                ErrorCode::Busy,
                format!("{id} is being installed or removed"),
            ));
        }
        // a file to open: resolved against the allowed roots here, argv built by the core
        let file = match path {
            None => None,
            Some(p) => {
                if !def.open {
                    return Err((ErrorCode::InvalidArgs, format!("{id} does not open files")));
                }
                Some(
                    crate::apps::resolve_open_path(&self.root, p)
                        .map_err(|m| (ErrorCode::InvalidArgs, m))?,
                )
            }
        };
        if let Some(f) = &file {
            // a player that is already open switches to the new file: close it, start again when
            // it has gone (app_tick)
            if self.apps.running(id) && self.apps.owns(id) {
                self.apps.set_pending_open(id, f.clone());
                self.close_app(id);
                return Ok("replacing");
            }
        } else if def.single_instance || def.max_instances > 1 {
            // one window of the app is enough: bring it to the front (`new_instance` adds more)
            if !def.wm_class.is_empty()
                && self.x_try(|b| b.focus_class(&def.wm_class)) == Some(true)
            {
                return Ok("focused");
            }
            if self.apps.running(id) {
                return Ok("already_running");
            }
        }
        let pid = self
            .apps
            .spawn_open(id, &self.root, file.as_deref())
            .map_err(|e| match e {
                LaunchError::Closing => (
                    ErrorCode::Busy,
                    format!("{id} is still closing, try again in a moment"),
                ),
                LaunchError::Limit => (ErrorCode::Refused, format!("{id} is at its window limit")),
                e => (ErrorCode::Internal, format!("{e:?}")),
            })?;
        self.publish(
            Topic::Apps,
            "apps",
            json!({"kind": "started", "app": id, "pid": pid}),
        );
        Ok("launched")
    }

    /// Send an event to every subscriber of `topic`.
    fn publish(&mut self, topic: Topic, event: &str, data: Value) {
        let line = encode_line(&Event::new(event, data));
        for c in &mut self.clients {
            if c.hello && c.topics.contains(&topic) {
                c.queue(line.clone());
            }
        }
    }

    /// Test hook: cheaper PBKDF2 for the screen lock's new secrets.
    pub fn lock_mut(&mut self) -> &mut Lock {
        &mut self.lock
    }

    /// Test hook: shorter VPN script timeouts.
    pub fn vpn_mut(&mut self) -> &mut Vpn {
        &mut self.vpn
    }

    /// `wifi` event `vpn` with the current VPN status (state changes, failures, reverts).
    fn publish_vpn(&mut self) {
        let data = match serde_json::to_value(self.vpn.status()) {
            Ok(Value::Object(mut m)) => {
                // `kind` names the event; the tunnel type moves to `tunnel`
                let t = m.remove("kind").unwrap_or(Value::Null);
                m.insert("tunnel".into(), t);
                m.insert("kind".into(), json!("vpn"));
                Value::Object(m)
            }
            _ => return,
        };
        self.publish(Topic::Wifi, "wifi", data);
    }

    /// Test hook: a shorter stats sampling interval.
    pub fn stats_mut(&mut self) -> &mut Stats {
        &mut self.stats
    }
    /// Test hook: shorter GPS script timeouts.
    pub fn gnss_mut(&mut self) -> &mut Gnss {
        &mut self.gnss
    }
    /// Test hook: shorter hotspot script timeouts.
    pub fn hotspot_mut(&mut self) -> &mut Hotspot {
        &mut self.hotspot
    }
    /// Test hook: shorter Bluetooth limits.
    pub fn bluetooth_mut(&mut self) -> &mut Bluetooth {
        &mut self.bluetooth
    }

    /// Test hook: walpad-volume timeout.
    pub fn volume_mut(&mut self) -> &mut Volume {
        &mut self.volume
    }

    /// Test hook: app store reserve and job timeouts.
    pub fn pkg_mut(&mut self) -> &mut Pkg {
        &mut self.pkg
    }

    /// Test hook: modem timeouts and poll periods.
    pub fn modem_mut(&mut self) -> &mut Modem {
        &mut self.modem
    }

    /// Test hook: how long a closing app gets before it is killed.
    pub fn set_close_grace(&mut self, d: Duration) {
        self.close_grace = d;
        self.apps.set_close_grace(d);
    }

    /// Close app (M6.1): our own child gets SIGTERM (SIGKILL after the grace period); an app
    /// somebody else started gets WM_DELETE_WINDOW and, if its window survives the grace period,
    /// SIGTERM of its _NET_WM_PID. Returns how it was done, None if nothing is open.
    fn close_app(&mut self, id: &str) -> Option<&'static str> {
        if self.apps.request_close(id, Instant::now()) {
            return Some("terminate");
        }
        if self.apps.owns(id) {
            return None; // already closing
        }
        let class = self.apps.def(id)?.wm_class.clone();
        if class.is_empty() {
            return None;
        }
        let wins: Vec<_> = self
            .x_try(|b| b.windows())?
            .into_iter()
            .filter(|w| class_matches(&w.class, &class))
            .collect();
        if wins.is_empty() {
            return None;
        }
        let mut deadline = Instant::now() + self.close_grace;
        for w in &wins {
            if w.deletable {
                self.x_try(|b| b.close_window(w.id));
            } else {
                // cannot be asked politely: go straight to the signal
                deadline = Instant::now();
            }
        }
        if !self.ext_close.iter().any(|c| c.app == id) {
            self.ext_close.push(ExtClose {
                app: id.to_string(),
                deadline,
            });
        }
        Some("window_close")
    }

    /// Housekeeping for apps: reap children and publish their exit, look for app windows on
    /// screen, finish pending external closes. Returns true when something changed.
    fn app_tick(&mut self) -> bool {
        // reap finished helpers so they do not stay zombies
        self.helpers
            .borrow_mut()
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
        let now = Instant::now();
        let mut did = false;
        let mut reopen_list: Vec<(String, String)> = Vec::new();
        for (id, exit) in self.apps.reap(now) {
            did = true;
            if let Some(p) = self.apps.take_pending_open(&id) {
                reopen_list.push((id.clone(), p));
            }
            let name = self
                .apps
                .def(&id)
                .map(|d| {
                    if d.name.is_empty() {
                        id.clone()
                    } else {
                        d.name.clone()
                    }
                })
                .unwrap_or_else(|| id.clone());
            // another window of the same app is still open: the app keeps running
            let others = self.apps.running(&id);
            match exit {
                Exit::Clean | Exit::Closed if others => self.publish(
                    Topic::Apps,
                    "apps",
                    json!({"kind": "instance_closed", "app": id}),
                ),
                Exit::Clean | Exit::Closed => self.publish(
                    Topic::Apps,
                    "apps",
                    json!({"kind": "exited", "app": id, "closed": exit == Exit::Closed}),
                ),
                Exit::Crashed { code, signal } => {
                    eprintln!("walpad-core: app {id} crashed (code {code:?}, signal {signal:?})");
                    self.publish(
                        Topic::Apps,
                        "apps",
                        json!({"kind": "crashed", "app": id, "code": code, "signal": signal}),
                    );
                    self.publish(
                        Topic::Notice,
                        "notice",
                        json!({"kind": "app_crashed", "app": id, "name": name}),
                    );
                    // back to the shell so the user is not left on a blank screen
                    if !others {
                        self.x_try(|b| b.focus_title("WalPad Home"));
                    }
                }
            }
        }
        for (id, path) in reopen_list {
            if let Err((_, m)) = self.launch(&id, Some(&path)) {
                eprintln!("walpad-core: reopen {id}: {m}");
            }
        }
        if now >= self.apps_x_at {
            self.apps_x_at = now + APPS_X_EVERY;
            did |= self.scan_app_windows();
        }
        did |= self.finish_ext_closes(now);
        did |= self.windows_tick(now);
        did
    }

    /// Which apps have a window on screen (also apps started by hand or by the Python launcher).
    fn scan_app_windows(&mut self) -> bool {
        if !self.apps.watches_windows() {
            return false;
        }
        let Some(wins) = self.x_try(|b| b.windows()) else {
            return false; // X is away: keep what we knew
        };
        let seen: std::collections::HashSet<String> = self
            .apps
            .defs()
            .iter()
            .filter(|d| self.apps.installed(d, &self.root))
            .filter(|d| wins.iter().any(|w| class_matches(&w.class, &d.wm_class)))
            .map(|d| d.id.clone())
            .collect();
        let changes = self.apps.set_ext(seen);
        let did = !changes.is_empty();
        for (id, up) in changes {
            let kind = if up { "started" } else { "exited" };
            self.publish(
                Topic::Apps,
                "apps",
                json!({"kind": kind, "app": id, "external": true}),
            );
        }
        did
    }

    fn finish_ext_closes(&mut self, now: Instant) -> bool {
        let due: Vec<String> = self
            .ext_close
            .iter()
            .filter(|c| now >= c.deadline)
            .map(|c| c.app.clone())
            .collect();
        if due.is_empty() {
            return false;
        }
        self.ext_close.retain(|c| now < c.deadline);
        for id in due {
            let class = self
                .apps
                .def(&id)
                .map(|d| d.wm_class.clone())
                .unwrap_or_default();
            let wins = self.x_try(|b| b.windows()).unwrap_or_default();
            for w in wins.iter().filter(|w| class_matches(&w.class, &class)) {
                match w.pid {
                    Some(pid) if terminate_pid(pid, false) => {
                        eprintln!(
                            "walpad-core: {id} ignored WM_DELETE_WINDOW, sent SIGTERM to {pid}"
                        )
                    }
                    _ => eprintln!("walpad-core: {id} did not close and has no usable pid"),
                }
            }
        }
        true
    }

    fn screenshot(&self) -> CmdResult {
        let src = self.root.p("/run/walpad/xfb/Xvfb_screen0");
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let virt = format!("/root/screenshots/shot-{t}.xwd");
        let dst = self.root.p(&virt);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(internal)?;
        }
        std::fs::copy(src, &dst).map_err(|e| (ErrorCode::Internal, format!("screenshot: {e}")))?;
        Ok(json!({"path": virt}))
    }

    fn reload_cfg(&mut self) {
        self.brain.reload(
            &read_file(&self.root, "/etc/walpad/devd.json"),
            &read_file(&self.root, "/etc/walpad/keys.json"),
            &read_file(&self.root, "/etc/walpad/touch.json"),
        );
    }

    fn set_setting(&mut self, key: &str, value: Value) -> CmdResult {
        let virt = if matches!(
            key,
            "clock_24h" | "pinned_apps" | "windows_mode" | "split_ratio" | "split_swap"
        ) {
            "/etc/walpad/shell.json"
        } else {
            "/etc/walpad/devd.json"
        };
        let path = self.root.p(virt);
        let mut obj: serde_json::Map<String, Value> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        obj.insert(key.to_string(), value.clone());
        let bytes = serde_json::to_vec_pretty(&obj).map_err(internal)?;
        write_atomic(&path, &bytes)
            .map_err(|e| (ErrorCode::Internal, format!("cannot write {virt}: {e}")))?;
        self.reload_cfg();
        Ok(json!({"key": key, "value": value}))
    }

    /// A shell client has subscribed to the `input` topic.
    fn shell_listening(&self) -> bool {
        self.clients
            .iter()
            .any(|c| c.hello && c.topics.contains(&Topic::Input))
    }

    /// The shell's power menu is open (it told us, or we opened it, and it is not stale).
    fn shell_menu_active(&self) -> bool {
        self.shell_menu_until.is_some_and(|t| self.now_ms() < t) && self.shell_listening()
    }

    fn publish_input(&mut self, k: ShellInput) {
        let ev = encode_line(&Event::new(
            "input",
            serde_json::to_value(k).unwrap_or_default(),
        ));
        for c in &mut self.clients {
            if c.hello && c.topics.contains(&Topic::Input) {
                c.queue(ev.clone());
            }
        }
    }

    /// Long power press / `powermenu` key action: the shell draws the menu when it listens for
    /// hardware keys, otherwise the Tk fallback runs.
    fn open_power_menu(&mut self) {
        if self.shell_listening() {
            self.shell_menu_until = Some(self.now_ms() + SHELL_MENU_MAX_MS);
            self.publish_input(ShellInput::PowerMenuOpen);
        } else {
            self.spawn_simple("walpad-powermenu.py", &[]);
        }
    }

    fn run_action(&mut self, a: Action) {
        // a locked tablet takes no key shortcut that reaches an app or the shell
        if self.lock.locked()
            && matches!(
                a,
                Action::Key(_)
                    | Action::Launch(_)
                    | Action::Run(_)
                    | Action::Home
                    | Action::Keyboard
                    | Action::PowerMenu
                    | Action::Screenshot
            )
        {
            return;
        }
        match a {
            Action::None => {}
            Action::Key(spec) => {
                self.x_try(|b| b.send_key(&spec));
            }
            Action::Ctl(line) => match parse_devd_line(&line) {
                Ok(Some(cmd)) => {
                    let device_live = self.live.as_ref().is_some_and(|l| l.device);
                    if device_live {
                        if let Err((_, m)) = self.execute(cmd) {
                            eprintln!("walpad-core: ctl:{line}: {m}");
                        }
                    } else if let Err(e) = std::fs::OpenOptions::new()
                        .write(true)
                        .open(self.root.p("/run/walpad/devd.cmd"))
                        .and_then(|mut f| writeln!(f, "{line}"))
                    {
                        eprintln!("walpad-core: devd.cmd not reachable: {e}");
                    }
                }
                Ok(None) => self.reload_cfg(),
                Err(e) => eprintln!("walpad-core: bad ctl action {line:?}: {e}"),
            },
            Action::Launch(id) => match self.launch(&id, None) {
                Ok(how) => eprintln!("walpad-core: launch {id}: {how}"),
                Err((_, m)) => eprintln!("walpad-core: launch {id}: {m}"),
            },
            Action::Run(c) => eprintln!("walpad-core: run:{c} rejected (use launch:<app-id>)"),
            Action::Home => {
                self.x_try(|b| b.focus_title("WalPad Home"));
                if self.keyboard_shown {
                    self.keyboard(KeyboardMode::Hide);
                }
            }
            Action::Back => {
                self.x_try(|b| b.back());
            }
            Action::Keyboard => {
                self.keyboard(KeyboardMode::Toggle);
            }
            Action::PowerMenu => self.open_power_menu(),
            Action::Screenshot => {
                if let Err((_, m)) = self.screenshot() {
                    eprintln!("walpad-core: {m}");
                }
            }
            Action::Volume(v) => {
                let now = Instant::now();
                let a = self.volume.delta(v, now);
                self.publish_audio("key", a);
            }
            Action::Unknown(s) => eprintln!("walpad-core: unknown key action {s:?}"),
        }
    }

    /// Execute effects. `forced` = explicit client request (always applied); otherwise only the
    /// subsystems this core runs live.
    fn apply(&mut self, effects: Vec<Effect>, forced: bool) {
        let (dev, inp) = match &self.live {
            Some(l) => (l.device, l.input),
            None => (false, false),
        };
        for e in effects {
            if !forced && ((e.is_device() && !dev) || (!e.is_device() && !inp)) {
                continue;
            }
            match e {
                Effect::Backlight(v) => {
                    if let Err(err) = sysfs::write_backlight(&self.root, v) {
                        eprintln!("walpad-core: backlight write failed: {err}");
                    }
                }
                Effect::Orientation(o) => {
                    if let Err(err) =
                        std::fs::write(self.root.p("/run/walpad/orientation"), o.to_string())
                    {
                        eprintln!("walpad-core: orientation write failed: {err}");
                    }
                }
                Effect::Struts(o) => {
                    self.x_try(|b| b.set_struts(o));
                    // tile again for the new orientation
                    self.layout_at = Instant::now();
                }
                Effect::KeyboardDock(o) => {
                    self.x_try(|b| b.dock_keyboard(o).map(|_| ()));
                }
                Effect::PowerMenuOpen if self.lock.locked() => {}
                Effect::PowerMenuOpen => self.open_power_menu(),
                Effect::LockNow => {
                    self.lock.lock_now();
                }
                Effect::PowerMenuActivate => {
                    if self.shell_menu_active() {
                        self.publish_input(ShellInput::PowerShort);
                    } else {
                        self.x_try(|b| b.send_key("Return"));
                    }
                }
                Effect::ShellKey(k) => self.publish_input(k),
                Effect::Cover(c) => {
                    eprintln!("walpad-core: cover {}", if c { "closed" } else { "open" })
                }
                Effect::LowBattery(l) => eprintln!("walpad-core: low battery flag -> {l}"),
                Effect::CriticalPoweroff(mv) => {
                    eprintln!("walpad-core: CRITICAL battery {mv} mV: safe power-off");
                    self.halt("poweroff");
                }
                Effect::Pointer(tm::Act::Move(x, y)) => {
                    self.x_try(|b| b.pointer_move(x, y));
                }
                Effect::Pointer(tm::Act::Press) => {
                    self.x_try(|b| b.button(true));
                }
                Effect::Pointer(tm::Act::Release) => {
                    self.x_try(|b| b.button(false));
                }
                Effect::Pointer(tm::Act::Pair(..)) => {}
                Effect::Button(n, d) => {
                    self.x_try(|b| b.button_n(n, d));
                }
                Effect::Wheel { button, ctrl } => {
                    self.x_try(|b| b.wheel(button, ctrl));
                }
                Effect::Action(a) => self.run_action(a),
            }
        }
    }

    // ---------------------------------------------------------------- model
    pub fn status(&self) -> Status {
        let raw = read_battery(&self.root);
        let mut wifi =
            parse_wifi_state(sysfs::read_trim(&self.root, "/run/walpad/wifi-state").as_deref());
        wifi.enabled = self.wifi_enabled();
        // a stale supplicant reading must not show a connection while the client is off
        let client_off =
            !wifi.enabled || matches!(wifi.state.as_deref(), Some("disabled" | "hotspot"));
        if !client_off {
            let live = self.wifi.live();
            wifi.ssid = live.ssid.clone();
            wifi.signal_dbm = live.signal_dbm;
            if wifi.ip.is_none() {
                wifi.ip = live.ip.clone();
            }
        }
        let devs = input::discover(&self.root);
        let bt = self.bluetooth.status();
        let ok = |b: bool| if b { "ok" } else { "unavailable" }.to_string();
        Status {
            battery: battery::model(&raw, self.brain.guard.low()),
            screen: self.brain.screen.state(),
            brightness: self.brain.screen.brightness(),
            orientation: self.brain.rotation.orientation(),
            rotation_locked: self.brain.rotation.locked(),
            lid_closed: self.brain.screen.lid_closed(),
            wifi,
            hotspot: self.hotspot.status().clone(),
            vpn: self.vpn.status().clone(),
            mobile: self.modem.status().clone(),
            phone: self.modem.phone_status(Instant::now()),
            gps: self.gnss.status().clone(),
            stats: self.stats.status().clone(),
            bluetooth: bt.clone(),
            audio: self.volume.status(),
            lock: self.lock.status(wall_ms()),
            apps: self.apps.status(&self.root),
            hardware: Hardware {
                touch: ok(input::find(&devs, input::is_touch).is_some()),
                gsensor: ok(read_gsensor(&self.root).is_some()),
                audio: "unverified".into(),
                bluetooth: if bt.controller == walpad_proto::BluetoothController::Present {
                    "ok"
                } else {
                    "unavailable"
                }
                .into(),
                sd_card: "unavailable".into(),
            },
        }
    }

    /// The Wi-Fi switch: anything but "0" in the state file (or no file) is on.
    fn wifi_enabled(&self) -> bool {
        sysfs::read_trim(&self.root, WIFI_ENABLED_FILE).as_deref() != Some("0")
    }

    /// `wifi_enable`: persist the switch; walpad-wifi's loop stops or restarts the supplicant
    /// within about a second. Off also drops the current association at once.
    fn wifi_enable(&mut self, on: bool) -> CmdResult {
        let p = self.root.p(WIFI_ENABLED_FILE);
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(internal)?;
        }
        write_atomic(&p, if on { b"1\n" } else { b"0\n" }).map_err(internal)?;
        if !on && self.wifi.connected() {
            let _ = self.wifi.disconnect();
        }
        eprintln!(
            "walpad-core: Wi-Fi switch {}",
            if on { "on" } else { "off" }
        );
        Ok(json!({
            "enabled": on,
            // the client stays paused while an exclusive hotspot runs
            "held_by_hotspot": on && self.hotspot.status().exclusive,
        }))
    }

    /// `audio` event on the status topic: `{volume, muted, available, source: "key"|"ipc"}`. The
    /// shell draws its volume pill for `key` (a slider drag needs no extra feedback).
    fn publish_audio(&mut self, source: &str, a: walpad_proto::AudioStatus) {
        self.publish(
            Topic::Status,
            "audio",
            json!({"volume": a.volume, "muted": a.muted, "available": a.available, "source": source}),
        );
    }

    /// `wifi` event `hotspot` with the current hotspot status (state changes, failures).
    fn publish_hotspot(&mut self) {
        let data = match serde_json::to_value(self.hotspot.status()) {
            Ok(Value::Object(mut m)) => {
                m.insert("kind".into(), json!("hotspot"));
                Value::Object(m)
            }
            _ => return,
        };
        self.publish(Topic::Wifi, "wifi", data);
    }

    /// `gps` event on the status topic with the current GPS status (owner request 2026-11-11).
    /// The position is private: it goes only to subscribed clients, never to a log.
    fn publish_gps(&mut self) {
        if let Ok(v) = serde_json::to_value(self.gnss.status()) {
            self.publish(Topic::Status, "gps", v);
        }
    }

    fn write_status_json(&self, s: &Status) {
        let wifi_raw =
            sysfs::read_trim(&self.root, "/run/walpad/wifi-state").unwrap_or_else(|| "off".into());
        let time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let up = sysfs::uptime_s(&self.root).unwrap_or(0.0);
        let legacy = LegacyStatus::from_status(s, time, up, &wifi_raw);
        let dst = self.root.p("/run/walpad/status.json");
        if let Ok(js) = serde_json::to_vec(&legacy) {
            let _ = write_atomic(&dst, &js);
        }
    }

    // ---------------------------------------------------------------- commands
    pub fn execute(&mut self, cmd: Command) -> CmdResult {
        let now = self.now_ms();
        match cmd {
            Command::GetStatus => Ok(serde_json::to_value(self.status()).unwrap_or_default()),
            Command::Screen { mode } => {
                let a = self.brain.screen.screen(mode, now);
                let mut fx = Vec::new();
                Brain::screen_effects(a, &mut fx);
                self.apply(fx, true);
                // switching the screen off by hand locks at once (like the short power press)
                if mode != ScreenMode::On && self.brain.screen.state() == ScreenState::Off {
                    self.lock.lock_now();
                }
                Ok(
                    json!({"screen": self.brain.screen.state(), "brightness": self.brain.screen.brightness()}),
                )
            }
            Command::SetBrightness(args) => {
                let req = match (args.value, args.delta) {
                    (Some(v), _) => st::Brightness::Absolute(v),
                    (_, Some(d)) => st::Brightness::Delta(d),
                    _ => return Err((ErrorCode::InvalidArgs, "value or delta required".into())),
                };
                let a = self.brain.screen.brightness_cmd(req);
                let mut fx = Vec::new();
                Brain::screen_effects(a, &mut fx);
                self.apply(fx, true);
                Ok(json!({"brightness": self.brain.screen.brightness()}))
            }
            Command::VolumeSet { percent } => {
                let a = self.volume.set_volume(percent, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::VolumeDelta { delta } => {
                let a = self.volume.delta(delta, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::Mute { on } => {
                let a = self.volume.set_mute(on, Instant::now());
                self.publish_audio("ipc", a);
                Ok(json!(a))
            }
            Command::VolumeTest => match self.volume.test_tone(Instant::now()) {
                Ok(true) => Ok(json!({"playing": true})),
                Ok(false) => Err((ErrorCode::Busy, "the test tone is still playing".into())),
                Err(m) => Err((ErrorCode::Internal, m)),
            },
            Command::Rotate { mode } => {
                let a = self.brain.rotation.command(mode);
                let mut fx = Vec::new();
                Brain::rotation_effects(a, &mut fx);
                self.apply(fx, true);
                Ok(
                    json!({"orientation": self.brain.rotation.orientation(), "rotation_locked": self.brain.rotation.locked()}),
                )
            }
            Command::Keyboard { mode } => Ok(json!({"keyboard": self.keyboard(mode)})),
            Command::Home => {
                let ok = self
                    .x_try(|b| b.focus_title("WalPad Home"))
                    .unwrap_or(false);
                Ok(json!({"focused": ok}))
            }
            Command::Back => {
                let sent = self.x_try(|b| b.back());
                Ok(json!({"sent": sent}))
            }
            Command::Launch { app, args } => self
                .launch(&app, args.as_ref().map(|a| a.path.as_str()))
                .map(|how| json!({"result": how})),
            Command::CloseApp { app } => {
                if self.apps.def(&app).is_none() {
                    return Err((ErrorCode::InvalidArgs, format!("unknown app {app:?}")));
                }
                let how = self.close_app(&app);
                Ok(json!({"closed": how.is_some(), "how": how}))
            }
            Command::NewInstance { app } => self.new_instance(&app),
            Command::Windows => self.windows_cmd(),
            Command::FocusWindow { window } => self.focus_window(window),
            Command::CloseWindow { window } => self.close_window_cmd(window),
            Command::WindowRect { window, x, y, w, h } => {
                self.window_rect_cmd(window, walpad_proto::wm::WRect::new(x, y, w, h))
            }
            Command::Snap { window, zone } => self.snap_cmd(window, zone),
            Command::Layout { mode, ratio, swap } => self.layout_cmd(mode, ratio, swap),
            Command::RestartShell => Ok(json!({"terminated": self.terminate("walpad-shell")})),
            Command::Screenshot => self.screenshot(),
            Command::SetSetting { key, value } => self.set_setting(&key, value),
            Command::Power {
                confirmed: false, ..
            } => Err((ErrorCode::Refused, "power requires confirmed: true".into())),
            Command::Power { action, .. } => {
                self.halt(match action {
                    PowerAction::Poweroff => "poweroff",
                    PowerAction::Reboot => "reboot",
                });
                Ok(json!({"action": action}))
            }
            Command::WifiScan => self.wifi.scan(Instant::now()).map(|_| json!({"accepted": true})).map_err(wifi_err),
            Command::WifiListSaved => self
                .wifi
                .list_saved()
                .map(|v| Value::Array(v.iter().map(|s| json!({"id": s.id, "ssid": ssid_display(&s.ssid), "ssid_hex": hex(&s.ssid)})).collect()))
                .map_err(wifi_err),
            Command::WifiConnect { ssid, secret, saved_id } => {
                let bytes = match ssid.as_deref() {
                    Some(h) => unhex(h).ok_or((ErrorCode::InvalidArgs, "ssid must be hex".to_string()))?,
                    None => Vec::new(),
                };
                self.wifi.connect(bytes, secret.as_ref(), saved_id, Instant::now()).map(|_| json!({"accepted": true})).map_err(wifi_err)
            }
            Command::WifiDisconnect => self.wifi.disconnect().map(|_| json!({})).map_err(wifi_err),
            Command::WifiForget { saved_id } => self.wifi.forget(saved_id).map(|_| json!({})).map_err(wifi_err),
            Command::WifiEnable { on } => self.wifi_enable(on),
            Command::Hotspot { on } => {
                let st = self.hotspot.set_on(on, Instant::now()).map_err(hotspot_err)?;
                self.publish_hotspot();
                Ok(json!({"state": st}))
            }
            Command::HotspotConfig { ssid, secret } => {
                let restarting = self
                    .hotspot
                    .configure(ssid.as_deref(), secret.as_ref(), Instant::now())
                    .map_err(hotspot_err)?;
                let h = self.hotspot.status();
                let reply = json!({"ssid": h.ssid, "has_secret": h.has_secret, "restarting": restarting});
                if restarting {
                    self.publish_hotspot();
                }
                Ok(reply)
            }
            Command::StatsConfig { cpu, ram, rom, net } => {
                let it = self
                    .stats
                    .configure(cpu, ram, rom, net, Instant::now())
                    .map_err(|m| (ErrorCode::Internal, m))?;
                Ok(json!({"items": it}))
            }
            Command::Gps { on } => {
                let st = self.gnss.set_on(on, Instant::now()).map_err(gnss_err)?;
                self.publish_gps();
                Ok(json!({"enabled": on, "state": st}))
            }
            Command::Vpn { on } => {
                let st = self.vpn.set_on(on, Instant::now()).map_err(vpn_err)?;
                self.publish_vpn();
                Ok(json!({"state": st}))
            }
            Command::VpnConfig { profile, kill_switch, dns, proxy } => {
                self.vpn
                    .configure(profile.as_deref(), kill_switch, dns.as_deref(), proxy.as_deref(), Instant::now())
                    .map_err(vpn_err)?;
                let px = self.vpn.proxy().map(str::to_string);
                self.apps.set_proxy(px.as_deref());
                let v = self.vpn.status();
                Ok(json!({"profile": v.profile, "kill_switch": v.kill_switch, "dns": v.dns, "proxy": v.proxy}))
            }
            Command::VpnConfirm => {
                self.vpn.confirm(Instant::now()).map_err(vpn_err)?;
                Ok(json!({}))
            }
            Command::VpnImport { path, name } => {
                let p = self.vpn.import(&path, name.as_deref()).map_err(vpn_err)?;
                eprintln!("walpad-core: VPN profile imported");
                Ok(json!({"name": p.name, "kind": p.kind}))
            }
            Command::VpnRemove { name } => {
                self.vpn.remove(&name).map_err(vpn_err)?;
                Ok(json!({}))
            }
            Command::VpnFiles => {
                let files: Vec<Value> = self
                    .vpn
                    .candidates()
                    .into_iter()
                    .map(|(name, path, kind)| json!({"name": name, "path": path, "kind": kind}))
                    .collect();
                Ok(json!({"files": files}))
            }
            Command::MobileRadio { on } => self.modem.set_radio(on, Instant::now()).map_err(mobile_err),
            Command::MobileData { on } => self.modem.set_data(on, Instant::now()).map_err(mobile_err),
            Command::MobileApn { apn } => self.modem.set_apn(&apn).map_err(mobile_err),
            Command::SimPin { pin } => self.modem.sim_pin(&pin, Instant::now()).map_err(mobile_err),
            Command::MobileInfo => Ok(self.modem.info()),
            Command::BluetoothEnable { on } => self.bluetooth.set_enabled(on).map_err(bluetooth_err),
            Command::BluetoothScan { on } => self.bluetooth.set_scan(on).map_err(bluetooth_err),
            Command::BluetoothPair { address } => self.bluetooth.pair(&address).map_err(bluetooth_err),
            Command::BluetoothConnect { address } => self.bluetooth.connect(&address).map_err(bluetooth_err),
            Command::BluetoothDisconnect { address } => {
                self.bluetooth.disconnect(&address).map_err(bluetooth_err)
            }
            Command::BluetoothForget { address } => self.bluetooth.forget(&address).map_err(bluetooth_err),
            Command::BluetoothConfirm { address, accept } => {
                self.bluetooth.confirm(&address, accept).map_err(bluetooth_err)
            }
            // phone (owner request 2026-11-11): numbers and texts never reach a log
            Command::PhoneDial { number } => self
                .modem
                .phone_op(Instant::now(), |p, n| p.dial(number.expose(), n))
                .map_err(mobile_err),
            Command::PhoneAnswer => self.modem.phone_op(Instant::now(), |p, n| p.answer(n)).map_err(mobile_err),
            Command::PhoneHangup => self.modem.phone_op(Instant::now(), |p, n| p.hangup(n)).map_err(mobile_err),
            Command::PhoneDtmf { digit } => {
                let c = digit.chars().next().unwrap_or(' ');
                self.modem.phone_op(Instant::now(), |p, n| p.dtmf(c, n)).map_err(mobile_err)
            }
            Command::PhoneMute { on } => self.modem.phone_op(Instant::now(), |p, n| p.mute(on, n)).map_err(mobile_err),
            Command::PhoneSpeaker { on } => self.modem.phone_op(Instant::now(), |p, n| p.speaker(on, n)).map_err(mobile_err),
            Command::SmsSend { number, text } => self
                .modem
                .phone_op(Instant::now(), |p, _| p.sms_send(number.expose(), text.expose()))
                .map_err(mobile_err),
            Command::SmsList => Ok(self.modem.phone().sms_list()),
            Command::SmsDelete { id } => self.modem.phone_mut().sms_delete(id).map_err(mobile_err),
            Command::SmsRead { number } => self.modem.phone_mut().sms_read(number.expose()).map_err(mobile_err),
            Command::ContactList => Ok(self.modem.phone().contact_list()),
            Command::ContactSave { id, name, numbers } => {
                let nums = numbers.iter().map(|n| n.expose().to_string()).collect();
                self.modem.phone_mut().contact_save(id.as_deref(), &name, nums).map_err(mobile_err)
            }
            Command::ContactDelete { id } => self.modem.phone_mut().contact_delete(&id).map_err(mobile_err),
            Command::ContactImport { path } => {
                let p = self.root.p(&path);
                self.modem.phone_mut().contact_import(&p).map_err(mobile_err)
            }
            Command::CallLog => Ok(self.modem.phone().call_log()),
            Command::CallLogClear => self.modem.phone_mut().call_log_clear().map_err(mobile_err),
            Command::PkgCatalogue => Ok(self.pkg.catalogue_json()),
            Command::PkgInstall { id } => {
                self.pkg.start(Op::Install, &id, Instant::now()).map_err(pkg_err)?;
                Ok(json!({"accepted": true, "id": id}))
            }
            Command::PkgRemove { id } => {
                self.pkg.start(Op::Remove, &id, Instant::now()).map_err(pkg_err)?;
                // an app that runs is closed first: its window goes, then the package
                let closed = self.close_app(&id);
                Ok(json!({"accepted": true, "id": id, "closed": closed.is_some()}))
            }
            Command::DefaultApp { role, app } => {
                self.pkg.set_default(&role, &app).map_err(pkg_err)?;
                Ok(json!({"role": role, "app": app}))
            }
            Command::PowerMenu { open } => {
                self.shell_menu_until = open.then(|| now + SHELL_MENU_MAX_MS);
                Ok(json!({"open": open}))
            }
            Command::LockSet {
                kind,
                secret,
                current,
            } => self
                .lock
                .set(
                    kind,
                    secret.expose(),
                    current.as_ref().map(|c| c.expose()),
                    wall_ms(),
                )
                .map(|_| {
                    eprintln!("walpad-core: screen lock set ({kind:?})");
                    json!({"kind": kind})
                })
                .map_err(lock_err),
            Command::LockRemove { current } => self
                .lock
                .remove(current.expose(), wall_ms())
                .map(|_| {
                    eprintln!("walpad-core: screen lock removed");
                    json!({})
                })
                .map_err(lock_err),
            Command::Unlock { secret } => {
                Ok(match self.lock.unlock(secret.expose(), wall_ms()) {
                    Unlock::Unlocked => json!({"unlocked": true}),
                    Unlock::Wrong { attempts_left } => {
                        json!({"unlocked": false, "attempts_left": attempts_left})
                    }
                    Unlock::Wait { retry_in_s } => {
                        json!({"unlocked": false, "retry_in_s": retry_in_s})
                    }
                })
            }
            Command::LockNow => Ok(
                json!({"locked": self.lock.lock_now(), "configured": self.lock.configured()}),
            ),
            Command::LockConfig { auto_lock_s } => self
                .lock
                .set_auto_lock(auto_lock_s)
                .map(|_| json!({"auto_lock_s": auto_lock_s}))
                .map_err(lock_err),
            Command::Hello { .. } | Command::Subscribe { .. } => Ok(json!({})),
        }
    }

    fn handle_line(&mut self, ci: usize, line: &[u8]) {
        let resp = match parse_request(line) {
            Err(r) => r,
            Ok((id, cmd)) => {
                let hello = self.clients[ci].hello;
                match cmd {
                    Command::Hello { proto, .. } => {
                        if proto != PROTO_VERSION {
                            Response::err(id, ErrorCode::Version, "unsupported protocol version")
                        } else {
                            self.clients[ci].hello = true;
                            Response::ok(
                                id,
                                json!({"proto": PROTO_VERSION, "server": "walpad-core"}),
                            )
                        }
                    }
                    _ if !hello => {
                        Response::err(id, ErrorCode::NotHello, "first request must be hello")
                    }
                    Command::Subscribe { topics } => {
                        self.clients[ci].topics = topics;
                        self.clients[ci].sync_pending = true;
                        Response::ok(id, json!({}))
                    }
                    cmd => match self.execute(cmd) {
                        Ok(v) => Response::ok(id, v),
                        Err((c, m)) => Response::err(id, c, m),
                    },
                }
            }
        };
        self.clients[ci].queue(encode_line(&resp));
        // a new status subscriber starts in sync
        if resp.ok
            && self.clients[ci].topics.contains(&Topic::Status)
            && self.clients[ci].sync_pending
        {
            self.clients[ci].sync_pending = false;
            let ev = Event::new(
                "status",
                serde_json::to_value(self.status()).unwrap_or_default(),
            );
            self.clients[ci].queue(encode_line(&ev));
        }
    }

    fn handle_fifo(&mut self) {
        let Some(f) = self.fifo.as_mut() else { return };
        let mut buf = [0u8; 512];
        loop {
            match f.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => self.fifo_buf.extend_from_slice(&buf[..n]),
                Err(_) => break,
            }
        }
        while let Some(pos) = self.fifo_buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.fifo_buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line).trim().to_string();
            match parse_devd_line(&line) {
                Ok(Some(cmd)) => {
                    if let Err((_, m)) = self.execute(cmd) {
                        eprintln!("walpad-core: devd.cmd {line:?}: {m}");
                    }
                }
                Ok(None) if line == "reload" => self.reload_cfg(),
                Ok(None) => {}
                Err(e) => eprintln!("walpad-core: devd.cmd {line:?}: {e}"),
            }
        }
    }

    fn push_status_if_changed(&mut self) {
        let s = self.status();
        if self.last_pushed.as_ref() != Some(&s) {
            let ev = encode_line(&Event::new(
                "status",
                serde_json::to_value(&s).unwrap_or_default(),
            ));
            for c in &mut self.clients {
                if c.hello && c.topics.contains(&Topic::Status) {
                    c.queue(ev.clone());
                }
            }
            self.last_pushed = Some(s);
        }
    }

    /// Poke fbmirror through its FIFO so it leaves its idle polling at once and the reaction to a
    /// touch reaches the panel without the idle delay. Never blocks: non-blocking open/write, every
    /// error (no fbmirror, FIFO full) is ignored.
    fn wake_display(&self) {
        use rustix::fs::{Mode, OFlags};
        if let Ok(fd) = rustix::fs::open(
            self.root.p("/run/walpad/fbwake"),
            OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            let _ = rustix::io::write(&fd, &[1u8]);
        }
    }

    /// Feed real input and timers to the Brain and execute what it decides (live mode).
    fn pump_live(&mut self) {
        let now = self.now_ms();
        let shell_menu = self.shell_menu_active();
        self.brain.shell_keys = shell_menu;
        self.brain.menu_open = shell_menu || self.root.p("/run/walpad/powermenu-open").exists();
        let evs = match self.live.as_mut() {
            Some(l) => l.inputs.poll(&self.root),
            None => return,
        };
        if !evs.is_empty() {
            self.wake_display();
        }
        let mut fx = Vec::new();
        {
            // one X query per touch: shell windows and `touch: mouse` apps keep the mouse
            // behaviour, everything else gets the touchscreen gestures
            let xb = self.xb.as_ref();
            let apps = &self.apps;
            let mut probe = |x: i32, y: i32| match xb.and_then(|b| b.touch_hit(x, y).ok()) {
                Some(crate::xbridge::TouchHit::App(class)) => apps.touch_mode(&class),
                _ => crate::logic::gesture::Mode::Mouse,
            };
            for (role, ev) in evs {
                fx.extend(self.brain.on_event_probe(role, &ev, now, &mut probe));
            }
        }
        let root = self.root.clone();
        let mut g = || read_gsensor(&root);
        let mut b = || read_battery(&root);
        fx.extend(self.brain.tick(now, &mut g, &mut b));
        self.apply(fx, false);
        self.x_maintain();
    }

    /// One iteration: wait for I/O (bounded), then service everything that is ready.
    pub fn step(&mut self, max_wait: Duration) {
        enum Slot {
            Listener,
            Fifo,
            Client(usize),
        }
        let mut slots = Vec::new();
        let ready: Vec<(usize, bool, bool)>;
        {
            let mut fds = Vec::new();
            fds.push(PollFd::new(&self.listener, PollFlags::IN));
            slots.push(Slot::Listener);
            if let Some(f) = &self.fifo {
                fds.push(PollFd::new(f, PollFlags::IN));
                slots.push(Slot::Fifo);
            }
            for (i, c) in self.clients.iter().enumerate() {
                let mut fl = PollFlags::IN;
                if !c.out.is_empty() {
                    fl |= PollFlags::OUT;
                }
                fds.push(PollFd::new(&c.stream, fl));
                slots.push(Slot::Client(i));
            }
            let ms = max_wait.as_millis().min(1000) as i64;
            let t = Timespec {
                tv_sec: ms / 1000,
                tv_nsec: (ms % 1000) * 1_000_000,
            };
            let _ = poll(&mut fds, Some(&t));
            ready = fds
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let r = f.revents();
                    (
                        i,
                        r.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR),
                        r.contains(PollFlags::OUT),
                    )
                })
                .collect();
        }
        let mut did = false;
        for (i, readable, writable) in ready {
            match slots[i] {
                Slot::Listener if readable => {
                    while let Ok((s, _)) = self.listener.accept() {
                        let _ = s.set_nonblocking(true);
                        self.clients.push(Client {
                            stream: s,
                            dec: LineDecoder::new(),
                            hello: false,
                            topics: vec![],
                            out: vec![],
                            stalled_since: None,
                            dead: false,
                            sync_pending: false,
                        });
                    }
                }
                Slot::Fifo if readable => {
                    self.handle_fifo();
                    did = true;
                }
                Slot::Client(ci) => {
                    if writable {
                        self.clients[ci].flush();
                    }
                    if readable {
                        let mut buf = [0u8; 4096];
                        let mut lines = Vec::new();
                        loop {
                            match self.clients[ci].stream.read(&mut buf) {
                                Ok(0) => {
                                    self.clients[ci].dead = true;
                                    break;
                                }
                                Ok(n) => match self.clients[ci].dec.feed(&buf[..n]) {
                                    Ok(l) => lines.extend(l),
                                    Err(_) => {
                                        self.clients[ci].dead = true;
                                        break;
                                    }
                                },
                                Err(_) => break,
                            }
                        }
                        for l in lines {
                            self.handle_line(ci, &l);
                            did = true;
                        }
                    }
                }
                _ => {}
            }
        }
        // drop dead or stalled clients (a client that does not read for 5 s is disconnected)
        let now = Instant::now();
        self.clients.retain(|c| {
            !c.dead
                && c.stalled_since
                    .is_none_or(|t| now.duration_since(t) < CLIENT_STALL)
        });

        did |= self.app_tick();
        let wall = wall_ms();
        self.lock
            .screen(self.brain.screen.state() == ScreenState::Off, wall);
        did |= self.lock.tick(wall);
        for ev in self.wifi.tick(Instant::now()) {
            let line = encode_line(&Event::new("wifi", wifi_event_json(&ev)));
            for c in &mut self.clients {
                if c.hello && c.topics.contains(&Topic::Wifi) {
                    c.queue(line.clone());
                }
            }
            did = true;
        }
        if self.hotspot.tick(Instant::now()) {
            self.publish_hotspot();
            did = true;
        }
        // header stats (owner request 2026-11-11): sampled only while an item is on
        did |= self.stats.tick(Instant::now());
        if self.gnss.tick(Instant::now()) {
            self.publish_gps();
            did = true;
        }
        did |= self.bluetooth.tick(Instant::now());
        for n in self.bluetooth.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        if self.pkg.tick(Instant::now()) {
            // an app was installed or removed: the allowlist follows
            self.apps.reload(&self.root);
            did = true;
        }
        for n in self.pkg.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        if self.vpn.tick(Instant::now()) {
            self.publish_vpn();
            did = true;
        }
        did |= self.modem.tick(Instant::now());
        did |= self.volume.tick(Instant::now());
        for n in self.modem.take_notices() {
            self.publish(Topic::Notice, "notice", n);
            did = true;
        }
        let now = Instant::now();
        // outside live device mode the battery guard only logs (the Python daemon owns shutdown)
        if self.live.as_ref().is_none_or(|l| !l.device) && now >= self.next_battery {
            self.next_battery = now + BATTERY_EVERY;
            let raw = read_battery(&self.root);
            for a in self.brain.guard.step(&raw, &self.brain.batt_cfg) {
                match a {
                    battery::Act::CriticalPoweroff { mv } => eprintln!("walpad-core: CRITICAL battery {mv} mV (not acting: device subsystem is not live)"),
                    battery::Act::LowChanged(l) => eprintln!("walpad-core: low battery flag -> {l}"),
                }
            }
            did = true;
        }
        if did || now >= self.next_status {
            let s = self.status();
            if now >= self.next_status {
                self.next_status = now + STATUS_EVERY;
                self.write_status_json(&s);
            }
            self.push_status_if_changed();
        }
    }

    pub fn run(&mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            if let Some(sh) = self.shadow.as_mut() {
                sh.pump(&self.root);
                self.step(Duration::from_millis(40));
            } else if self.live.is_some() {
                self.pump_live();
                let wait = self.brain.poll_interval_ms().clamp(10, 40);
                self.step(Duration::from_millis(wait));
            } else {
                self.step(Duration::from_millis(200));
            }
        }
        if let Some(mut b) = self.xb.take() {
            b.release_if_down();
        }
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

fn open_fifo(root: &Root) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let p = root.p("/run/walpad/devd.cmd");
    if !p.exists() {
        rustix::fs::mknodat(
            rustix::fs::CWD,
            &p,
            rustix::fs::FileType::Fifo,
            rustix::fs::Mode::from_raw_mode(0o666),
            0,
        )?;
    }
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(&p)?;
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o666))?;
    Ok(f)
}

/// `<wpa_state> <ipv4>` | `started` | `failed` | missing.
pub fn parse_wifi_state(raw: Option<&str>) -> WifiStatus {
    let none = |radio| WifiStatus {
        radio,
        state: None,
        ssid: None,
        ip: None,
        signal_dbm: None,
        enabled: true,
    };
    match raw.map(str::trim) {
        None | Some("") => none(WifiRadio::Down),
        Some("failed") => none(WifiRadio::Failed),
        Some("started") => none(WifiRadio::Starting),
        // client off by the switch / paused for an exclusive hotspot (owner request 2026-11-10)
        Some(s @ ("disabled" | "hotspot")) => WifiStatus {
            state: Some(s.to_string()),
            ..none(WifiRadio::Down)
        },
        Some(s) => {
            let mut it = s.split_whitespace();
            WifiStatus {
                radio: WifiRadio::Up,
                state: it.next().map(String::from),
                ssid: None,
                ip: it.next().map(String::from),
                signal_dbm: None,
                enabled: true,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_state_parsing() {
        assert_eq!(parse_wifi_state(None).radio, WifiRadio::Down);
        assert_eq!(parse_wifi_state(Some("failed\n")).radio, WifiRadio::Failed);
        assert_eq!(parse_wifi_state(Some("started")).radio, WifiRadio::Starting);
        let w = parse_wifi_state(Some("COMPLETED 192.168.0.100"));
        assert_eq!(
            (w.radio, w.state.as_deref(), w.ip.as_deref()),
            (WifiRadio::Up, Some("COMPLETED"), Some("192.168.0.100"))
        );
        assert_eq!(parse_wifi_state(Some("SCANNING")).ip, None);
    }
}
