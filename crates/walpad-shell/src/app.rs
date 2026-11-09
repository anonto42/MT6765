//! Application state: what the shell shows and how taps, core events and replies change it.
//! Pure of I/O: the shell loop feeds it gestures and core events and sends the commands it returns.
use crate::apps_screen;
use crate::core_client::{CoreEvent, ReplyError};
use crate::gesture::Gesture;
use crate::home;
use crate::network::{self, bars_from_dbm, NetEntry, NetModel, SavedEntry};
use crate::overlay::{self, AppSheet, Overlay, PasswordSheet};
use serde_json::{json, Value};
use std::collections::HashMap;
use walpad_hw::Root;
use walpad_proto::{Secret, Status};
use walpad_ui::components::{Key, KeyboardLayer, Painter, Tab, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Canvas, Icon, Rect};

pub type Ms = u64;

const TOAST_MS: Ms = 3000;
const SPIN_EVERY_MS: Ms = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
    /// "AM"/"PM" in 12-hour mode.
    pub suffix: Option<&'static str>,
    /// e.g. "Monday, 09 November".
    pub date: String,
}

impl WallTime {
    pub fn new(hour: u8, minute: u8, date: &str, clock_24h: bool) -> WallTime {
        if clock_24h {
            WallTime {
                hour,
                minute,
                suffix: None,
                date: date.into(),
            }
        } else {
            let h12 = if hour.is_multiple_of(12) {
                12
            } else {
                hour % 12
            };
            WallTime {
                hour: h12,
                minute,
                suffix: Some(if hour >= 12 { "PM" } else { "AM" }),
                date: date.into(),
            }
        }
    }

    /// Local time now and the milliseconds until the next minute boundary.
    pub fn now(clock_24h: bool) -> (WallTime, u64) {
        let z = jiff::Zoned::now();
        let ms_in_minute = z.second() as u64 * 1000 + (z.subsec_nanosecond() as u64) / 1_000_000;
        let date = z.strftime("%A, %d %B").to_string();
        (
            WallTime::new(z.hour() as u8, z.minute() as u8, &date, clock_24h),
            60_000u64.saturating_sub(ms_in_minute).max(1),
        )
    }
}

/// An app known from /etc/walpad/apps.json.
#[derive(Debug, Clone, PartialEq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub icon: Icon,
}

pub fn load_catalog(root: &Root) -> Vec<AppEntry> {
    let Some(list) = std::fs::read_to_string(root.p("/etc/walpad/apps.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    else {
        return Vec::new();
    };
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e["id"].as_str()?.to_string();
                    let name = e["name"].as_str().unwrap_or(&id).to_string();
                    let icon =
                        Icon::from_name(e["icon"].as_str().unwrap_or("")).unwrap_or(Icon::Apps);
                    Some(AppEntry { id, name, icon })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn shell_json(root: &Root) -> Value {
    std::fs::read_to_string(root.p("/etc/walpad/shell.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null)
}

pub fn read_clock_24h(root: &Root) -> bool {
    shell_json(root)["clock_24h"].as_bool().unwrap_or(true)
}

pub const DEFAULT_PINNED: [&str; 4] = ["browser", "terminal", "files", "settings"];

pub fn read_pinned(root: &Root) -> Vec<String> {
    shell_json(root)["pinned_apps"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_else(|| DEFAULT_PINNED.iter().map(|s| s.to_string()).collect())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub icon: Icon,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    // navigation + Home
    Nav(usize),
    WifiCard,
    BatteryCard,
    Tile(usize),
    Slider,
    Rotation,
    Keyboard,
    ScreenOff,
    // Apps
    AppTile(usize),
    // Network
    Scan,
    Disconnect,
    Avail(usize),
    Saved(usize),
    Forget(usize),
    // overlays
    Eye,
    Key(usize),
    SheetCancel,
    SheetConnect,
    CloseApp,
    PinToggle,
    DialogCancel,
    DialogConfirm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    SetBrightness(u8),
    RotateToggle,
    KeyboardToggle,
    ScreenOff,
    Launch(String),
    CloseApp(String),
    SetPinned(Vec<String>),
    WifiScan,
    WifiListSaved,
    /// `secret` is never printed: `Secret`'s Debug is `***`.
    WifiConnect {
        ssid_hex: Option<String>,
        secret: Option<Secret>,
        saved_id: Option<u32>,
    },
    WifiDisconnect,
    WifiForget(u32),
}

/// What a request was for, to interpret its reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    SavedList,
    Scan,
    Connect,
    Forget,
    Close,
    Other,
}

impl Cmd {
    /// (command, args) for the core's IPC.
    pub fn request(&self) -> (&'static str, Value) {
        match self {
            Cmd::SetBrightness(v) => ("set_brightness", json!({"value": v})),
            Cmd::RotateToggle => ("rotate", json!({"mode": "toggle"})),
            Cmd::KeyboardToggle => ("keyboard", json!({"mode": "toggle"})),
            Cmd::ScreenOff => ("screen", json!({"mode": "off"})),
            Cmd::Launch(id) => ("launch", json!({"app": id})),
            Cmd::CloseApp(id) => ("close_app", json!({"app": id})),
            Cmd::SetPinned(ids) => ("set_setting", json!({"key": "pinned_apps", "value": ids})),
            Cmd::WifiScan => ("wifi_scan", Value::Null),
            Cmd::WifiListSaved => ("wifi_list_saved", Value::Null),
            Cmd::WifiConnect {
                ssid_hex,
                secret,
                saved_id,
            } => {
                let mut a = serde_json::Map::new();
                if let Some(h) = ssid_hex {
                    a.insert("ssid".into(), json!(h));
                }
                if let Some(s) = secret {
                    // the one place the secret is serialized: onto the control socket, never a log
                    a.insert(
                        "secret".into(),
                        serde_json::to_value(s).unwrap_or(Value::Null),
                    );
                }
                if let Some(id) = saved_id {
                    a.insert("saved_id".into(), json!(id));
                }
                ("wifi_connect", Value::Object(a))
            }
            Cmd::WifiDisconnect => ("wifi_disconnect", Value::Null),
            Cmd::WifiForget(id) => ("wifi_forget", json!({"saved_id": id})),
        }
    }

    pub fn purpose(&self) -> Purpose {
        match self {
            Cmd::WifiListSaved => Purpose::SavedList,
            Cmd::WifiScan => Purpose::Scan,
            Cmd::WifiConnect { .. } => Purpose::Connect,
            Cmd::WifiForget(_) => Purpose::Forget,
            Cmd::CloseApp(_) => Purpose::Close,
            _ => Purpose::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub text: String,
    pub tone: Tone,
    pub until: Ms,
}

/// Everything the screens draw from.
pub struct Model {
    pub status: Option<Status>,
    pub connected: bool,
    /// Home tiles (pinned, installed; at most four).
    pub tiles: Vec<Tile>,
    /// Apps screen: every installed app plus the Settings shortcut.
    pub apps: Vec<Tile>,
    pub pinned: Vec<String>,
    pub wall: WallTime,
    pub pressed: Option<Target>,
    pub slider_active: bool,
    pub slider_value: f32,
    pub net: NetModel,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast>,
    pub spinner_deg: u16,
}

impl Model {
    pub fn blank(wall: WallTime) -> Model {
        Model {
            status: None,
            connected: false,
            tiles: Vec::new(),
            apps: Vec::new(),
            pinned: DEFAULT_PINNED.iter().map(|s| s.to_string()).collect(),
            wall,
            pressed: None,
            slider_active: false,
            slider_value: 0.0,
            net: NetModel::default(),
            overlay: None,
            toast: None,
            spinner_deg: 0,
        }
    }

    /// Slider position 0..=1: the dragged value while dragging, else the core's brightness.
    pub fn brightness_value(&self) -> f32 {
        if self.slider_active {
            self.slider_value
        } else {
            self.status
                .as_ref()
                .map_or(0.0, |s| s.brightness as f32 / 255.0)
        }
    }

    fn spinning(&self) -> bool {
        self.net.scanning
            || self.net.connecting.is_some()
            || matches!(&self.overlay, Some(Overlay::Password(s)) if s.connecting)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub dirty: bool,
    pub cmds: Vec<Cmd>,
}

impl Outcome {
    fn cmd(&mut self, c: Cmd) {
        self.cmds.push(c);
    }
}

const SLIDER_SEND_EVERY_MS: Ms = 80;

pub struct App {
    pub frame: Frame,
    pub active: Tab,
    pub model: Model,
    catalog: Vec<AppEntry>,
    /// Hit areas in screen coordinates (nav bar, Home, status).
    hits: Vec<(Rect, Target)>,
    /// Hit areas of scrollable screens, in content coordinates.
    content_hits: Vec<(Rect, Target)>,
    overlay_hits: Vec<(Rect, Target)>,
    pub scroll: i32,
    content_h: i32,
    view: Rect,
    last_slider_send: Ms,
    last_spin: Ms,
    pending: HashMap<u64, Purpose>,
    now: Ms,
}

fn native_brightness(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round().clamp(1.0, 255.0) as u8
}

fn friendly(e: &ReplyError) -> String {
    match e.code.as_str() {
        "internal" if e.msg.contains("unavailable") => "Wi-Fi control is unavailable".into(),
        _ => e.msg.clone(),
    }
}

impl App {
    pub fn new(w: i32, h: i32, catalog: Vec<AppEntry>, wall: WallTime) -> App {
        let frame = Frame::new(w, h);
        let mut a = App {
            frame,
            active: Tab::Home,
            model: Model::blank(wall),
            catalog,
            hits: Vec::new(),
            content_hits: Vec::new(),
            overlay_hits: Vec::new(),
            scroll: 0,
            content_h: 0,
            view: frame.content,
            last_slider_send: 0,
            last_spin: 0,
            pending: HashMap::new(),
            now: 0,
        };
        a.rebuild_tiles();
        a
    }

    pub fn with_pinned(mut self, pinned: Vec<String>) -> App {
        self.model.pinned = pinned;
        self.rebuild_tiles();
        self
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.frame = Frame::new(w, h);
        self.hits.clear();
        self.content_hits.clear();
        self.overlay_hits.clear();
        self.scroll = 0;
    }

    pub fn set_wall(&mut self, wall: WallTime) -> bool {
        let changed = self.model.wall != wall;
        self.model.wall = wall;
        changed
    }

    /// Installed apps (what the core reports) in catalog order, then the Settings shortcut; the Home
    /// tiles are the pinned ones among them (at most four).
    fn rebuild_tiles(&mut self) {
        let mut apps = Vec::new();
        if let Some(st) = &self.model.status {
            for e in self.catalog.iter().filter(|e| e.id != "wifi") {
                if let Some(a) = st.apps.iter().find(|a| a.app == e.id) {
                    apps.push(Tile {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        icon: e.icon,
                        running: a.running,
                    });
                }
            }
        }
        apps.push(Tile {
            id: "settings".into(),
            name: "Settings".into(),
            icon: Icon::Settings,
            running: false,
        });
        let tiles = self
            .model
            .pinned
            .iter()
            .filter_map(|id| apps.iter().find(|t| &t.id == id).cloned())
            .take(4)
            .collect();
        self.model.apps = apps;
        self.model.tiles = tiles;
    }

    /// Remember which request was sent with which id (to interpret its reply).
    pub fn note_sent(&mut self, id: u64, cmd: &Cmd) {
        self.pending.insert(id, cmd.purpose());
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.model.toast = Some(Toast {
            text: text.into(),
            tone,
            until: self.now + TOAST_MS,
        });
    }

    fn go(&mut self, tab: Tab, out: &mut Outcome) {
        if tab != self.active {
            self.active = tab;
            self.scroll = 0;
            self.content_hits.clear();
        }
        if tab == Tab::Network && self.model.connected {
            out.cmd(Cmd::WifiListSaved);
        }
        out.dirty = true;
    }

    // ------------------------------------------------------------------ core events
    pub fn on_core(&mut self, ev: &CoreEvent, now: Ms) -> Outcome {
        self.now = now;
        let mut out = Outcome::default();
        match ev {
            CoreEvent::Connected => {
                self.model.connected = true;
                if self.active == Tab::Network {
                    out.cmd(Cmd::WifiListSaved);
                }
                out.dirty = true;
            }
            CoreEvent::Disconnected => {
                self.model.connected = false;
                self.model.status = None;
                self.model.net.scanning = false;
                self.model.net.connecting = None;
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.connecting = false;
                }
                self.pending.clear();
                self.rebuild_tiles();
                out.dirty = true;
            }
            CoreEvent::Status(s) => {
                self.model.status = Some((**s).clone());
                self.model.connected = true;
                self.rebuild_tiles();
                out.dirty = true;
            }
            CoreEvent::Wifi(v) => self.on_wifi_event(v, &mut out),
            CoreEvent::Reply {
                id,
                ok,
                data,
                error,
            } => self.on_reply(*id, *ok, data, error.as_ref(), &mut out),
        }
        out
    }

    fn on_wifi_event(&mut self, v: &Value, out: &mut Outcome) {
        match v["kind"].as_str() {
            Some("scan") => {
                self.model.net.results = v["networks"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|n| {
                                Some(NetEntry {
                                    ssid_hex: n["ssid_hex"].as_str()?.to_string(),
                                    ssid: n["ssid"].as_str()?.to_string(),
                                    bars: bars_from_dbm(
                                        n["signal_dbm"].as_i64().unwrap_or(-90) as i32
                                    ),
                                    secured: n["security"].as_str() != Some("open"),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                self.model.net.scanning = false;
                self.model.net.scanned = true;
                self.model.net.error = None;
                out.dirty = true;
            }
            Some("connect") => {
                let ssid = v["ssid"].as_str().unwrap_or("").to_string();
                let hex = v["ssid_hex"].as_str().unwrap_or("").to_string();
                self.model.net.connecting = None;
                if v["ok"] == true {
                    self.toast(format!("Connected to {ssid}"), Tone::Info);
                    if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.ssid_hex == hex)
                    {
                        self.model.overlay = None;
                    }
                    out.cmd(Cmd::WifiListSaved);
                } else {
                    let text = match v["error"].as_str() {
                        Some("wrong_password") => "Wrong password",
                        Some("timeout") => "Timed out after 25 s",
                        _ => "Could not connect",
                    };
                    match &mut self.model.overlay {
                        Some(Overlay::Password(s)) if s.ssid_hex == hex => {
                            s.connecting = false;
                            s.error = Some(text.into());
                        }
                        _ => self.toast(format!("{text}: {ssid}"), Tone::Danger),
                    }
                }
                out.dirty = true;
            }
            Some("disconnected") => {
                self.toast("Wi-Fi disconnected", Tone::Warning);
                out.dirty = true;
            }
            _ => {}
        }
    }

    fn on_reply(
        &mut self,
        id: u64,
        ok: bool,
        data: &Value,
        error: Option<&ReplyError>,
        out: &mut Outcome,
    ) {
        let purpose = self.pending.remove(&id).unwrap_or(Purpose::Other);
        out.dirty = true;
        match (purpose, ok) {
            (Purpose::SavedList, true) => {
                self.model.net.saved = data
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|s| {
                                Some(SavedEntry {
                                    id: s["id"].as_u64()? as u32,
                                    ssid: s["ssid"].as_str()?.to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (Purpose::Scan, false) => {
                let e = error.cloned().unwrap_or(ReplyError {
                    code: "error".into(),
                    msg: "scan failed".into(),
                });
                if e.code != "busy" {
                    self.model.net.scanning = false;
                    self.model.net.error = Some(friendly(&e));
                }
            }
            (Purpose::Connect, false) => {
                self.model.net.connecting = None;
                let msg = error
                    .map(friendly)
                    .unwrap_or_else(|| "Could not connect".into());
                match &mut self.model.overlay {
                    Some(Overlay::Password(s)) => {
                        s.connecting = false;
                        s.error = Some(msg);
                    }
                    _ => self.toast(msg, Tone::Danger),
                }
            }
            (Purpose::Forget, true) => out.cmd(Cmd::WifiListSaved),
            (Purpose::Close, true) => {
                if data["closed"] == false {
                    self.toast("Could not close the app", Tone::Warning);
                }
            }
            (_, false) => {
                if let Some(e) = error {
                    if e.code != "busy" {
                        self.toast(friendly(e), Tone::Danger);
                    }
                }
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------ time
    /// Advance toast expiry and the spinner. Returns true when a redraw is needed.
    pub fn tick(&mut self, now: Ms) -> bool {
        self.now = now;
        let mut dirty = false;
        if self.model.toast.as_ref().is_some_and(|t| now >= t.until) {
            self.model.toast = None;
            dirty = true;
        }
        if self.model.spinning() && now.saturating_sub(self.last_spin) >= SPIN_EVERY_MS {
            self.last_spin = now;
            self.model.spinner_deg = (self.model.spinner_deg + 30) % 360;
            dirty = true;
        }
        dirty
    }

    /// Absolute time at which `tick` next has something to do.
    pub fn next_deadline(&self) -> Option<Ms> {
        let spin = self
            .model
            .spinning()
            .then_some(self.last_spin + SPIN_EVERY_MS);
        let toast = self.model.toast.as_ref().map(|t| t.until);
        [spin, toast].into_iter().flatten().min()
    }

    // ------------------------------------------------------------------ banners
    /// Persistent banner under the status bar (core down has priority over low battery).
    pub fn banner(&self) -> Option<(&'static str, Tone)> {
        if !self.model.connected {
            return Some(("System service not responding", Tone::Danger));
        }
        let st = self.model.status.as_ref()?;
        let charging = st
            .battery
            .state
            .as_deref()
            .is_some_and(|s| s.to_ascii_lowercase().starts_with("charg"));
        (st.battery.low && !charging)
            .then_some(("Battery low - connect the charger", Tone::Warning))
    }

    fn banner_h(&self) -> i32 {
        if self.banner().is_some() {
            56
        } else {
            0
        }
    }

    fn scrolls(&self) -> bool {
        matches!(
            self.active,
            Tab::Apps | Tab::Network | Tab::Device | Tab::Settings
        )
    }

    // ------------------------------------------------------------------ hit testing
    fn hit(&self, x: i32, y: i32) -> Option<Target> {
        if self.model.overlay.is_some() {
            return self
                .overlay_hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, t)| *t);
        }
        if self.scrolls() && self.view.contains(x, y) {
            let cy = y - self.view.y + self.scroll;
            if let Some((_, t)) = self
                .content_hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, cy))
            {
                return Some(*t);
            }
        }
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, t)| *t)
    }

    fn slider_value_at(&self, x: i32) -> f32 {
        let track = self
            .hits
            .iter()
            .find(|(_, t)| *t == Target::Slider)
            .map(|(r, _)| r.inset(SP_4))
            .unwrap_or(Rect::new(0, 0, 1, 1));
        ((x - track.x) as f32 / track.w.max(1) as f32).clamp(0.0, 1.0)
    }

    // ------------------------------------------------------------------ gestures
    pub fn on_gesture(&mut self, g: &Gesture, now: Ms) -> Outcome {
        self.now = now;
        let mut out = Outcome::default();
        match *g {
            Gesture::Pressed { x, y } => {
                let t = self.hit(x, y);
                self.model.pressed = t;
                if let (Some(Target::Key(i)), Some(Overlay::Password(s))) =
                    (t, &mut self.model.overlay)
                {
                    s.pressed_key = Some(i);
                }
                if t == Some(Target::Slider) && self.model.connected {
                    self.model.slider_active = true;
                    self.model.slider_value = self.slider_value_at(x);
                }
                out.dirty = t.is_some();
            }
            Gesture::Cancel => {
                out.dirty = self.model.pressed.take().is_some();
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    out.dirty |= s.pressed_key.take().is_some();
                }
            }
            Gesture::Tap { x, y } => {
                let t = self.hit(x, y);
                self.model.pressed = None;
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.pressed_key = None;
                }
                let was_slider = std::mem::take(&mut self.model.slider_active);
                out.dirty = true;
                match t {
                    Some(t) => self.on_tap(t, x, was_slider, &mut out),
                    None => out.dirty = was_slider || out.dirty,
                }
            }
            Gesture::LongPress { x, y } => {
                self.model.slider_active = false;
                out.dirty = self.model.pressed.take().is_some();
                if self.model.overlay.is_none() {
                    let id = match self.hit(x, y) {
                        Some(Target::Tile(i)) => self.model.tiles.get(i).map(|t| t.id.clone()),
                        Some(Target::AppTile(i)) => self.model.apps.get(i).map(|t| t.id.clone()),
                        _ => None,
                    };
                    if let Some(id) = id {
                        if let Some(t) = self.model.apps.iter().find(|t| t.id == id) {
                            let sheet = AppSheet {
                                id: id.clone(),
                                name: t.name.clone(),
                                running: t.running,
                                pinned: self.model.pinned.contains(&id),
                            };
                            self.model.overlay = Some(Overlay::App(sheet));
                            out.dirty = true;
                        }
                    }
                }
            }
            Gesture::DragStart { .. } => {}
            Gesture::Drag { dx, dy } => {
                if self.model.slider_active {
                    let w = self
                        .hits
                        .iter()
                        .find(|(_, t)| *t == Target::Slider)
                        .map_or(1, |(r, _)| r.inset(SP_4).w.max(1));
                    self.model.slider_value =
                        (self.model.slider_value + dx as f32 / w as f32).clamp(0.0, 1.0);
                    out.dirty = true;
                    if now.saturating_sub(self.last_slider_send) >= SLIDER_SEND_EVERY_MS {
                        self.last_slider_send = now;
                        out.cmd(Cmd::SetBrightness(native_brightness(
                            self.model.slider_value,
                        )));
                    }
                } else if self.scrolls() && self.model.overlay.is_none() {
                    let max = (self.content_h - self.view.h).max(0);
                    let s = (self.scroll - dy).clamp(0, max);
                    if s != self.scroll {
                        self.scroll = s;
                        out.dirty = true;
                    }
                }
            }
            Gesture::DragEnd => {
                if std::mem::take(&mut self.model.slider_active) {
                    out.cmd(Cmd::SetBrightness(native_brightness(
                        self.model.slider_value,
                    )));
                    out.dirty = true;
                }
            }
        }
        out
    }

    fn activate(&mut self, id: &str, out: &mut Outcome) {
        if id == "settings" {
            self.go(Tab::Settings, out);
        } else if self.model.connected {
            out.cmd(Cmd::Launch(id.to_string()));
        }
    }

    fn on_tap(&mut self, t: Target, x: i32, was_slider: bool, out: &mut Outcome) {
        let interactive = self.model.connected;
        match t {
            Target::Nav(i) => self.go(Tab::ALL[i], out),
            Target::WifiCard => self.go(Tab::Network, out),
            Target::BatteryCard => self.go(Tab::Device, out),
            Target::Tile(i) => {
                if let Some(id) = self.model.tiles.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            Target::AppTile(i) => {
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            Target::Slider if interactive => {
                let v = if was_slider {
                    self.model.slider_value
                } else {
                    self.slider_value_at(x)
                };
                out.cmd(Cmd::SetBrightness(native_brightness(v)));
            }
            Target::Rotation if interactive => out.cmd(Cmd::RotateToggle),
            Target::Keyboard if interactive => out.cmd(Cmd::KeyboardToggle),
            Target::ScreenOff if interactive => out.cmd(Cmd::ScreenOff),
            Target::Scan if interactive && !self.model.net.scanning => {
                self.model.net.scanning = true;
                self.model.net.error = None;
                out.cmd(Cmd::WifiScan);
            }
            Target::Disconnect if interactive => out.cmd(Cmd::WifiDisconnect),
            Target::Avail(i) if interactive => self.on_available(i, out),
            Target::Saved(i) if interactive => {
                if let Some(s) = self.model.net.saved.get(i).cloned() {
                    let already =
                        network::is_connected(&self.model.status) == Some(s.ssid.as_str());
                    if !already && self.model.net.connecting.is_none() {
                        out.cmd(Cmd::WifiConnect {
                            ssid_hex: None,
                            secret: None,
                            saved_id: Some(s.id),
                        });
                    }
                }
            }
            Target::Forget(i) => {
                if let Some(s) = self.model.net.saved.get(i) {
                    self.model.overlay = Some(Overlay::Forget {
                        id: s.id,
                        ssid: s.ssid.clone(),
                    });
                }
            }
            // ---- overlays
            Target::SheetCancel => self.close_overlay(),
            Target::DialogCancel => self.close_overlay(),
            Target::DialogConfirm => {
                if let Some(Overlay::Forget { id, .. }) = self.model.overlay.take() {
                    out.cmd(Cmd::WifiForget(id));
                }
            }
            Target::Eye => {
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.shown = !s.shown;
                }
            }
            Target::Key(i) => self.on_key(i, out),
            Target::SheetConnect => self.try_connect(out),
            Target::CloseApp => {
                if let Some(Overlay::App(s)) = self.model.overlay.take() {
                    out.cmd(Cmd::CloseApp(s.id));
                }
            }
            Target::PinToggle => {
                if let Some(Overlay::App(s)) = self.model.overlay.take() {
                    if s.pinned {
                        self.model.pinned.retain(|p| p != &s.id);
                    } else if !self.model.pinned.contains(&s.id) {
                        self.model.pinned.push(s.id.clone());
                    }
                    self.rebuild_tiles();
                    out.cmd(Cmd::SetPinned(self.model.pinned.clone()));
                }
            }
            _ => {}
        }
    }

    fn close_overlay(&mut self) {
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            s.secret.clear();
        }
        self.model.net.connecting = None;
        self.model.overlay = None;
    }

    fn on_available(&mut self, i: usize, out: &mut Outcome) {
        let Some(n) = self.model.net.results.get(i).cloned() else {
            return;
        };
        if network::is_connected(&self.model.status) == Some(n.ssid.as_str())
            || self.model.net.connecting.is_some()
        {
            return;
        }
        if let Some(saved) = self.model.net.saved.iter().find(|s| s.ssid == n.ssid) {
            // credentials are already stored: just select it
            self.model.net.connecting = Some(n.ssid_hex.clone());
            out.cmd(Cmd::WifiConnect {
                ssid_hex: None,
                secret: None,
                saved_id: Some(saved.id),
            });
        } else if n.secured {
            self.model.overlay = Some(Overlay::Password(PasswordSheet::new(n.ssid_hex, n.ssid)));
        } else {
            self.model.net.connecting = Some(n.ssid_hex.clone());
            out.cmd(Cmd::WifiConnect {
                ssid_hex: Some(n.ssid_hex),
                secret: None,
                saved_id: None,
            });
        }
    }

    fn try_connect(&mut self, out: &mut Outcome) {
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            if s.valid() && !s.connecting {
                s.connecting = true;
                s.error = None;
                self.model.net.connecting = Some(s.ssid_hex.clone());
                out.cmd(Cmd::WifiConnect {
                    ssid_hex: Some(s.ssid_hex.clone()),
                    secret: Some(Secret::new(s.secret.clone())),
                    saved_id: None,
                });
            }
        }
    }

    fn on_key(&mut self, idx: usize, out: &mut Outcome) {
        let frame = self.frame;
        let Some(Overlay::Password(s)) = &mut self.model.overlay else {
            return;
        };
        let Some(key) = overlay::key_at(&frame, s.layer, idx) else {
            return;
        };
        match key {
            Key::Char(c) => {
                if s.secret.len() < 64 {
                    s.secret.push(c);
                    s.error = None;
                }
                if s.shift {
                    s.shift = false;
                    s.layer = KeyboardLayer::Lower;
                }
            }
            Key::Space => {
                if s.secret.len() < 64 {
                    s.secret.push(' ');
                    s.error = None;
                }
            }
            Key::Backspace => {
                s.secret.pop();
                s.error = None;
            }
            Key::Shift => {
                s.shift = s.layer != KeyboardLayer::Upper;
                s.layer = if s.shift {
                    KeyboardLayer::Upper
                } else {
                    KeyboardLayer::Lower
                };
            }
            Key::Layer => {
                s.shift = false;
                s.layer = if s.layer == KeyboardLayer::Symbols {
                    KeyboardLayer::Lower
                } else {
                    KeyboardLayer::Symbols
                };
            }
            Key::Enter => self.try_connect(out),
        }
    }

    // ------------------------------------------------------------------ rendering
    pub fn render(&mut self, p: &mut Painter) {
        p.cv.fill(BG);
        let (ssid, pct, est, low) = match &self.model.status {
            Some(s) => (
                home::wifi_name(s),
                s.battery.percent,
                s.battery.source == walpad_proto::BatterySource::VoltageEstimate,
                s.battery.low,
            ),
            None => (None, None, false, false),
        };
        p.status_bar(self.frame.status, ssid.as_deref(), pct, est, low);
        let banner = self.banner();
        let banner_h = self.banner_h();
        self.hits.clear();
        self.content_hits.clear();
        self.view = Rect::new(
            0,
            self.frame.content.y + banner_h,
            self.frame.w,
            self.frame.content.h - banner_h,
        );
        if self.active == Tab::Home {
            self.hits = home::render(p, &self.frame, &self.model, banner_h);
        } else {
            self.render_scrolled(p);
        }
        if let Some((text, tone)) = banner {
            p.banner(
                Rect::new(0, self.frame.status.bottom(), self.frame.w, 48),
                text,
                tone,
            );
        }
        for (i, r) in p
            .bottom_nav(self.frame.nav, self.active)
            .into_iter()
            .enumerate()
        {
            self.hits.push((r, Target::Nav(i)));
        }
        self.overlay_hits.clear();
        let deg = self.model.spinner_deg;
        self.overlay_hits = match &self.model.overlay {
            Some(Overlay::Password(s)) => overlay::render_password(p, &self.frame, s, deg),
            Some(Overlay::App(s)) => overlay::render_app_sheet(p, &self.frame, s),
            Some(Overlay::Forget { ssid, .. }) => overlay::render_forget(p, &self.frame, ssid),
            None => Vec::new(),
        };
        if let Some(t) = &self.model.toast {
            p.toast(Rect::new(0, 0, self.frame.w, self.frame.h), &t.text, t.tone);
        }
    }

    /// Screens taller than the view are drawn off-screen at full height and the visible slice is copied.
    fn render_scrolled(&mut self, p: &mut Painter) {
        let frame = self.frame;
        let total = match self.active {
            Tab::Apps => apps_screen::layout(&frame, self.model.apps.len()).total,
            Tab::Network => network::layout(&frame, &self.model.status, &self.model.net).total,
            _ => 0,
        }
        .max(self.view.h);
        self.content_h = total;
        self.scroll = self.scroll.clamp(0, (total - self.view.h).max(0));
        let Some(mut off) = Canvas::new(frame.w as u32, total as u32, BG) else {
            return;
        };
        {
            let mut op = Painter {
                cv: &mut off,
                fonts: &mut *p.fonts,
            };
            let m = &self.model;
            self.content_hits = match self.active {
                Tab::Apps => apps_screen::render(&mut op, &frame, &m.apps, m.pressed),
                Tab::Network => network::render(
                    &mut op,
                    &frame,
                    &m.status,
                    &m.net,
                    m.connected,
                    m.pressed,
                    m.spinner_deg,
                ),
                other => {
                    let c = frame.padded();
                    op.screen_title(other.label(), Rect::new(c.x, SP_4, c.w, 40));
                    op.text(
                        "Coming in the next card",
                        walpad_ui::Style::new(
                            walpad_ui::Face::Regular,
                            SIZE_SECONDARY,
                            TEXT_SECONDARY,
                        ),
                        Rect::new(c.x, 64, c.w, 24),
                        walpad_ui::Align::Left,
                    );
                    Vec::new()
                }
            };
        }
        if let Some(slice) = off.crop(Rect::new(0, self.scroll, frame.w, self.view.h)) {
            p.cv.blit(&slice, 0, self.view.y);
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
