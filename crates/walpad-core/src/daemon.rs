//! The core poll loop: IPC socket, devd.cmd FIFO and timers in one thread, no async runtime.
//! M1.4 scope: serve status/commands and keep status.json; autonomous hardware policy
//! (input-driven timeouts, battery shutdown, X bridge) arrives in M3 behind runtime.conf.
use crate::logic::{battery, rotation, screen_timeout as st};
use rustix::event::{poll, PollFd, PollFlags, Timespec};
use serde_json::json;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::{battery::read_battery, gsensor::read_gsensor, input, sysfs, Root};
use walpad_proto::{
    encode_line, parse_devd_line, parse_request, AppState, Command, ErrorCode, Event, Hardware,
    LegacyStatus, LineDecoder, Response, Status, Topic, WifiRadio, WifiStatus, PROTO_VERSION,
};

const STATUS_EVERY: Duration = Duration::from_secs(2);
const BATTERY_EVERY: Duration = Duration::from_secs(10);
const CLIENT_STALL: Duration = Duration::from_secs(5);
const MAX_OUT: usize = 256 * 1024;

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

pub struct Daemon {
    root: Root,
    started: Instant,
    screen: st::Screen,
    rotation: rotation::Rotation,
    guard: battery::Guard,
    batt_cfg: battery::Cfg,
    listener: UnixListener,
    fifo: Option<std::fs::File>,
    fifo_buf: Vec<u8>,
    clients: Vec<Client>,
    last_pushed: Option<Status>,
    next_status: Instant,
    next_battery: Instant,
    sock_path: PathBuf,
    shadow: Option<crate::shadow_live::Live>,
}

fn load_json<T: serde::de::DeserializeOwned + Default>(root: &Root, path: &str) -> T {
    std::fs::read_to_string(root.p(path))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
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
        let screen_cfg: st::Cfg = load_json(&root, "/etc/walpad/devd.json");
        let rot_cfg: rotation::Cfg = load_json(&root, "/etc/walpad/devd.json");
        let batt_cfg: battery::Cfg = load_json(&root, "/etc/walpad/devd.json");
        let backlight = sysfs::read_backlight(&root);
        let mut d = Daemon {
            screen: st::Screen::new(screen_cfg, backlight, 0),
            rotation: rotation::Rotation::new(rot_cfg),
            guard: battery::Guard::default(),
            batt_cfg,
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
            root,
        };
        let init = d.rotation.init();
        d.run_rotation(init);
        Ok(d)
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.sock_path
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    // ---- executors
    fn run_screen(&mut self, acts: Vec<st::Act>) {
        for a in acts {
            let st::Act::Backlight(v) = a;
            if let Err(e) = sysfs::write_backlight(&self.root, v) {
                eprintln!("walpad-core: backlight write failed: {e}");
            }
        }
    }
    fn run_rotation(&mut self, acts: Vec<rotation::Act>) {
        for a in acts {
            if let rotation::Act::WriteOrientation(o) = a {
                if let Err(e) =
                    std::fs::write(self.root.p("/run/walpad/orientation"), o.to_string())
                {
                    eprintln!("walpad-core: orientation write failed: {e}");
                }
            }
            // SetStruts / DockKeyboard need the X bridge (M3.2).
        }
    }

    // ---- model
    pub fn status(&self) -> Status {
        let raw = read_battery(&self.root);
        let wifi =
            parse_wifi_state(sysfs::read_trim(&self.root, "/run/walpad/wifi-state").as_deref());
        let devs = input::discover(&self.root);
        let ok = |b: bool| if b { "ok" } else { "unavailable" }.to_string();
        Status {
            battery: battery::model(&raw, self.guard.low()),
            screen: self.screen.state(),
            brightness: self.screen.brightness(),
            orientation: self.rotation.orientation(),
            rotation_locked: self.rotation.locked(),
            lid_closed: self.screen.lid_closed(),
            wifi,
            apps: Vec::<AppState>::new(),
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
        let tmp = self.root.p("/run/walpad/status.json.tmp");
        if let Ok(js) = serde_json::to_vec(&legacy) {
            if std::fs::write(&tmp, js).is_ok() {
                let _ = std::fs::rename(&tmp, &dst);
            }
        }
    }

    // ---- commands
    pub fn execute(&mut self, cmd: Command) -> Result<serde_json::Value, (ErrorCode, String)> {
        let now = self.now_ms();
        match cmd {
            Command::GetStatus => Ok(serde_json::to_value(self.status()).unwrap_or_default()),
            Command::Screen { mode } => {
                let a = self.screen.screen(mode, now);
                self.run_screen(a);
                Ok(json!({"screen": self.screen.state(), "brightness": self.screen.brightness()}))
            }
            Command::SetBrightness(args) => {
                let req = match (args.value, args.delta) {
                    (Some(v), _) => st::Brightness::Absolute(v),
                    (_, Some(d)) => st::Brightness::Delta(d),
                    _ => return Err((ErrorCode::InvalidArgs, "value or delta required".into())),
                };
                let a = self.screen.brightness_cmd(req);
                self.run_screen(a);
                Ok(json!({"brightness": self.screen.brightness()}))
            }
            Command::Rotate { mode } => {
                let a = self.rotation.command(mode);
                self.run_rotation(a);
                Ok(
                    json!({"orientation": self.rotation.orientation(), "rotation_locked": self.rotation.locked()}),
                )
            }
            Command::Power {
                confirmed: false, ..
            } => Err((ErrorCode::Refused, "power requires confirmed: true".into())),
            Command::Hello { .. } | Command::Subscribe { .. } => Ok(json!({})),
            other => Err((
                ErrorCode::Internal,
                format!("{} is not implemented in this build", cmd_name(&other)),
            )),
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

        let now = Instant::now();
        if now >= self.next_battery {
            self.next_battery = now + BATTERY_EVERY;
            let raw = read_battery(&self.root);
            for a in self.guard.step(&raw, &self.batt_cfg) {
                match a {
                    battery::Act::CriticalPoweroff { mv } => eprintln!(
                        "walpad-core: CRITICAL battery {mv} mV (shutdown not wired until M3.2)"
                    ),
                    battery::Act::LowChanged(l) => {
                        eprintln!("walpad-core: low battery flag -> {l}")
                    }
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

    /// Shadow mode: read the real inputs, log `WOULD ...`, write nothing (M2.2).
    pub fn enable_shadow(&mut self) {
        self.shadow = Some(crate::shadow_live::Live::new(&self.root));
    }

    pub fn run(&mut self, stop: &AtomicBool) {
        while !stop.load(Ordering::Relaxed) {
            if let Some(sh) = self.shadow.as_mut() {
                sh.pump(&self.root);
                self.step(Duration::from_millis(40));
            } else {
                self.step(Duration::from_millis(200));
            }
        }
        let _ = std::fs::remove_file(&self.sock_path);
    }
}

fn cmd_name(c: &Command) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.get("cmd").and_then(|x| x.as_str().map(String::from)))
        .unwrap_or_else(|| "command".into())
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
