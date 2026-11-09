//! The core poll loop: IPC socket, devd.cmd FIFO, input devices and timers in one thread, no async
//! runtime. The `Brain` decides; this module executes (sysfs writes, X requests, spawns).
use crate::apps::Apps;
use crate::brain::{Brain, Effect};
use crate::inputs::Inputs;
use crate::logic::{battery, key_gestures::Action, screen_timeout as st, touch_map as tm};
use crate::wpa::manager::{Wifi, WifiError, WifiEvent};
use crate::wpa::parse::{hex, ssid_display, unhex};
use crate::xbridge::{XBridge, XError};
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
    LegacyStatus, LineDecoder, PowerAction, Response, ShellInput, Status, Topic, WifiRadio,
    WifiStatus, PROTO_VERSION,
};

const STATUS_EVERY: Duration = Duration::from_secs(2);
const BATTERY_EVERY: Duration = Duration::from_secs(10);
const CLIENT_STALL: Duration = Duration::from_secs(5);
const X_RETRY: Duration = Duration::from_secs(2);
const X_PING_EVERY: Duration = Duration::from_secs(5);
const MAX_OUT: usize = 256 * 1024;
const KEYBOARD_DOCK_DELAY_MS: u64 = 1500;
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
    xb: Option<XBridge>,
    x_retry: Instant,
    x_ping: Instant,
    dock_at: Option<u64>,
    /// Until when (ms) the shell's power menu owns the volume and power keys.
    shell_menu_until: Option<u64>,
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
            xb: None,
            x_retry: now,
            x_ping: now,
            dock_at: None,
            shell_menu_until: None,
            root,
        };
        // orientation is 0 at startup and the file is always written (struts need X: later)
        let init = d.brain.rotation.init();
        let mut fx = Vec::new();
        Brain::rotation_effects(init, &mut fx);
        d.apply(fx, true);
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
        if let Err(e) = r {
            eprintln!("walpad-core: cannot start {exe}: {e}");
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
                comm.trim().starts_with(prefix).then_some(pid)
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

    fn keyboard(&mut self, mode: KeyboardMode) -> &'static str {
        let running = !self.pids_by_comm("matchbox-keyb").is_empty();
        let show = match mode {
            KeyboardMode::Show => true,
            KeyboardMode::Hide => false,
            KeyboardMode::Toggle => !running,
        };
        if show && !running {
            self.spawn_simple("matchbox-keyboard", &[]);
            self.dock_at = Some(self.now_ms() + KEYBOARD_DOCK_DELAY_MS);
            "shown"
        } else if !show && running {
            self.terminate("matchbox-keyb");
            "hidden"
        } else if show {
            "shown"
        } else {
            "hidden"
        }
    }

    fn launch(&mut self, id: &str) -> Result<&'static str, (ErrorCode, String)> {
        let Some(def) = self.apps.def(id).cloned() else {
            return Err((ErrorCode::InvalidArgs, format!("unknown app {id:?}")));
        };
        if !self.apps.installed(&def, &self.root) {
            return Err((ErrorCode::Refused, format!("{id} is not installed")));
        }
        if def.single_instance {
            if !def.wm_class.is_empty()
                && self.x_try(|b| b.focus_class(&def.wm_class)) == Some(true)
            {
                return Ok("focused");
            }
            if self.apps.running(id) {
                return Ok("already_running");
            }
        }
        self.apps
            .spawn(id, &self.root)
            .map_err(|e| (ErrorCode::Internal, format!("{e:?}")))?;
        Ok("launched")
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
        let virt = if key == "clock_24h" || key == "pinned_apps" {
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
            Action::Launch(id) => match self.launch(&id) {
                Ok(how) => eprintln!("walpad-core: launch {id}: {how}"),
                Err((_, m)) => eprintln!("walpad-core: launch {id}: {m}"),
            },
            Action::Run(c) => eprintln!("walpad-core: run:{c} rejected (use launch:<app-id>)"),
            Action::Home => {
                self.x_try(|b| b.focus_title("WalPad Home"));
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
                let arg = if v >= 0 {
                    format!("{v}%+")
                } else {
                    format!("{}%-", -v)
                };
                self.spawn_simple("amixer", &["-q", "sset", "Master", &arg]);
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
                }
                Effect::KeyboardDock(o) => {
                    self.x_try(|b| b.dock_keyboard(o).map(|_| ()));
                }
                Effect::PowerMenuOpen => self.open_power_menu(),
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
                Effect::Action(a) => self.run_action(a),
            }
        }
    }

    // ---------------------------------------------------------------- model
    pub fn status(&self) -> Status {
        let raw = read_battery(&self.root);
        let mut wifi =
            parse_wifi_state(sysfs::read_trim(&self.root, "/run/walpad/wifi-state").as_deref());
        let live = self.wifi.live();
        wifi.ssid = live.ssid.clone();
        wifi.signal_dbm = live.signal_dbm;
        if wifi.ip.is_none() {
            wifi.ip = live.ip.clone();
        }
        let devs = input::discover(&self.root);
        let ok = |b: bool| if b { "ok" } else { "unavailable" }.to_string();
        Status {
            battery: battery::model(&raw, self.brain.guard.low()),
            screen: self.brain.screen.state(),
            brightness: self.brain.screen.brightness(),
            orientation: self.brain.rotation.orientation(),
            rotation_locked: self.brain.rotation.locked(),
            lid_closed: self.brain.screen.lid_closed(),
            wifi,
            apps: self.apps.status(&self.root),
            hardware: Hardware {
                touch: ok(input::find(&devs, input::is_touch).is_some()),
                gsensor: ok(read_gsensor(&self.root).is_some()),
                audio: "unverified".into(),
                bluetooth: "unavailable".into(),
                sd_card: "unavailable".into(),
            },
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
            Command::Launch { app } => self.launch(&app).map(|how| json!({"result": how})),
            Command::CloseApp { app } => {
                if self.apps.def(&app).is_none() {
                    return Err((ErrorCode::InvalidArgs, format!("unknown app {app:?}")));
                }
                // only apps this core started can be closed (it owns their process)
                Ok(json!({"closed": self.apps.close(&app)}))
            }
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
            Command::PowerMenu { open } => {
                self.shell_menu_until = open.then(|| now + SHELL_MENU_MAX_MS);
                Ok(json!({"open": open}))
            }
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
        let mut fx = Vec::new();
        for (role, ev) in evs {
            fx.extend(self.brain.on_event(role, &ev, now));
        }
        let root = self.root.clone();
        let mut g = || read_gsensor(&root);
        let mut b = || read_battery(&root);
        fx.extend(self.brain.tick(now, &mut g, &mut b));
        self.apply(fx, false);
        if let Some(t) = self.dock_at {
            if now >= t {
                self.dock_at = None;
                let o = self.brain.orientation();
                self.x_try(|b| b.dock_keyboard(o).map(|_| ()));
            }
        }
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

        if !self.apps.reap().is_empty() {
            did = true;
        }
        for ev in self.wifi.tick(Instant::now()) {
            let line = encode_line(&Event::new("wifi", wifi_event_json(&ev)));
            for c in &mut self.clients {
                if c.hello && c.topics.contains(&Topic::Wifi) {
                    c.queue(line.clone());
                }
            }
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
    };
    match raw.map(str::trim) {
        None | Some("") => none(WifiRadio::Down),
        Some("failed") => none(WifiRadio::Failed),
        Some("started") => none(WifiRadio::Starting),
        Some(s) => {
            let mut it = s.split_whitespace();
            WifiStatus {
                radio: WifiRadio::Up,
                state: it.next().map(String::from),
                ssid: None,
                ip: it.next().map(String::from),
                signal_dbm: None,
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
