//! Application state: what the shell shows and how taps, core events and replies change it.
//! Pure of I/O: the shell loop feeds it gestures and core events and sends the commands it returns.
use crate::bluetooth::{self, PairState};
use crate::chrome::{self, Chrome, ChromeKey, NAV_APPS};
use crate::config::DevdSettings;
use crate::core_client::{CoreEvent, ReplyError};
use crate::device_screen;
use crate::gesture::Gesture;
use crate::home;
use crate::mobile::{self, MobileInfo};
use crate::network::{self, bars_from_dbm, NetEntry, NetModel, SavedEntry};
mod lock_flow;

use crate::overlay::{self, AppSheet, Overlay, PasswordSheet, SheetPurpose};
use crate::panels;
use crate::settings_screen::{self, Page, SysInfo};
use crate::store::{self, StoreTarget, StoreView};
use crate::volume_ui::{self, Osd};
use crate::vpn_screen;
use serde_json::{json, Value};
use std::collections::HashMap;
use walpad_hw::Root;
use walpad_proto::{Secret, ShellInput, Status};
use walpad_ui::components::{Key, KeyboardLayer, Painter, Tab, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Icon, Rect};

pub use lock_flow::{lock_rows, LockAct, LockFlow};

pub type Ms = u64;

const TOAST_MS: Ms = 3000;
const POWER_ARM_MS: Ms = 3000;
/// The power menu closes itself this long after it opened.
const POWER_MENU_MS: Ms = 20_000;
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
    // apps.json plus the apps the app store registered in apps.d
    walpad_proto::appdefs::entries(
        &root.p(walpad_proto::appdefs::APPS_JSON),
        &root.p(walpad_proto::appdefs::APPS_D),
    )
    .iter()
    .filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let name = e["name"].as_str().unwrap_or(&id).to_string();
        let icon = Icon::from_name(e["icon"].as_str().unwrap_or("")).unwrap_or(Icon::Apps);
        Some(AppEntry { id, name, icon })
    })
    .collect()
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
    // sidebar: Nav(i) = Tab::ALL[i] (Home, the Apps panel toggle, Settings); Tile(i) = pinned app
    Nav(usize),
    Tile(usize),
    // header pill and the Quick Settings dropdown
    QuickSettings,
    WifiCard,
    BatteryCard,
    Slider,
    /// Volume slider (Quick Settings and Settings > Sound), mute button / row, test tone button.
    VolumeSlider,
    MuteToggle,
    TestTone,
    Rotation,
    ScreenOff,
    // Home grid (index into `Model::apps`)
    AppTile(usize),
    // Apps panel: a running app (index into `Model::apps`), its close badge, a tool (index into
    // `panels::TOOLS`)
    PanelActive(usize),
    PanelClose(usize),
    PanelTool(usize),
    /// Tap outside a popover: closes it.
    Dismiss,
    /// Inside a popover but not on a control: nothing happens.
    PanelBody,
    // Wi-Fi switch and hotspot (Network screen and Quick Settings; owner request 2026-11-10)
    WifiSwitch,
    HotspotSwitch,
    /// Quick Settings Hotspot row (opens Network).
    HotspotCard,
    HotspotName,
    HotspotPassword,
    // Mobile network (owner request 2026-11-10): the Quick Settings row, the screen's controls
    // VPN and DNS (owner request 2026-11-10)
    VpnSwitch,
    VpnKill,
    VpnKeep,
    VpnProfile,
    VpnDns,
    VpnProxy,
    /// Import candidate (index into `Model::vpn_files`).
    VpnImport(usize),
    MobileCard,
    MobileRadio,
    MobileDataSwitch,
    MobileApn,
    SimPinEntry,
    ImeiShow,
    // Bluetooth (owner request 2026-11-10): the Quick Settings row, the screen's controls and the
    // pairing dialog; indices are into `bluetooth::my_devices` / `available_devices`
    BtCard,
    BtSwitch,
    BtScan,
    BtAction(usize),
    BtForget(usize),
    BtAvail(usize),
    BtPairAccept,
    BtPairReject,
    // Network
    Scan,
    Disconnect,
    Avail(usize),
    Saved(usize),
    Forget(usize),
    // Device
    Rot(usize),
    Dim(usize),
    Off(usize),
    LidToggle,
    // Settings
    Page(Page),
    Back,
    ClockToggle,
    RestartShell,
    Reboot,
    Shutdown,
    /// Settings > Maintenance: open the power menu.
    PowerMenu,
    // overlays
    Eye,
    Key(usize),
    SheetCancel,
    SheetConnect,
    CloseApp,
    PinToggle,
    DialogCancel,
    DialogConfirm,
    /// Power menu item (index into `overlay::POWER_ITEMS`).
    PowerItem(usize),
    /// Settings > Security: the screen lock rows (OpenSpec `lock-screen`).
    Lock(LockAct),
    /// App store screen (change app-catalogue-manager).
    Store(StoreTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    SetBrightness(u8),
    /// `volume_set` percent 0-100, `mute`, `volume_test` (owner request 2026-11-10).
    SetVolume(u8),
    Mute(bool),
    TestTone,
    RotateToggle,
    /// Toggle / show / hide the shell's system keyboard. Handled by the shell loop itself; the
    /// resulting state is reported to the core as `keyboard show|hide`.
    KeyboardToggle,
    KeyboardShow,
    KeyboardHide,
    /// Show / hide the shell's mouse pad (mousepad.rs). Handled by the shell loop itself, never
    /// sent to the core (the core has no mouse pad command).
    MousePadShow,
    MousePadHide,
    /// Show the clipboard history list (seltools.rs). Shell-local like the mouse pad.
    ClipboardShow,
    ScreenOff,
    /// The Back control (dock button): the core sends Alt+Left or Esc to the focused window.
    Back,
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
    /// The Wi-Fi client switch.
    WifiEnable(bool),
    Hotspot(bool),
    /// `secret` is never printed: `Secret`'s Debug is `***`.
    HotspotConfig {
        ssid: Option<String>,
        secret: Option<Secret>,
    },
    Vpn(bool),
    /// Keep the kill switch (`vpn_confirm`).
    VpnKeep,
    VpnConfig {
        profile: Option<String>,
        kill_switch: Option<bool>,
        dns: Option<Vec<String>>,
        proxy: Option<String>,
    },
    VpnImport(String),
    VpnFiles,
    MobileRadio(bool),
    MobileData(bool),
    MobileApn(String),
    /// `pin` is never printed: `Secret`'s Debug is `***`.
    SimPin(Secret),
    MobileInfo,
    BtEnable(bool),
    BtScan(bool),
    BtPair(String),
    BtConnect(String),
    BtDisconnect(String),
    BtForget(String),
    BtConfirm {
        address: String,
        accept: bool,
    },
    /// Screen lock (OpenSpec `lock-screen`): secrets are never printed (`Secret`'s Debug is `***`).
    LockSet {
        kind: walpad_proto::LockKind,
        secret: Secret,
        current: Option<Secret>,
    },
    LockRemove(Secret),
    LockNow,
    LockConfig(u32),
    /// App store: the catalogue, install / uninstall by catalogue id, default app per role.
    PkgCatalogue,
    PkgInstall(String),
    PkgRemove(String),
    DefaultApp {
        role: String,
        app: String,
    },
    /// `rotate` mode: auto, 0, 90, 180, 270.
    Rotate(&'static str),
    /// `set_setting` with an allowlisted key.
    Setting(&'static str, Value),
    RestartShell,
    Power {
        poweroff: bool,
    },
    /// Tell the core the power menu opened/closed (it routes the volume and power keys to us
    /// while it is open).
    PowerMenuState(bool),
}

/// What a request was for, to interpret its reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    SavedList,
    Scan,
    Connect,
    Forget,
    Close,
    Setting,
    HotspotConfig,
    MobileApn,
    SimPin,
    MobileInfo,
    BtPair,
    BtOp,
    Lock,
    PkgCatalogue,
    PkgAction,
    DefaultApp,
    /// A VPN command whose failure is shown as a toast.
    Vpn,
    VpnSheet,
    VpnImport,
    VpnFiles,
    Other,
}

impl Cmd {
    /// (command, args) for the core's IPC.
    pub fn request(&self) -> (&'static str, Value) {
        match self {
            Cmd::SetBrightness(v) => ("set_brightness", json!({"value": v})),
            Cmd::SetVolume(v) => ("volume_set", json!({"percent": v})),
            Cmd::Mute(on) => ("mute", json!({"on": on})),
            Cmd::TestTone => ("volume_test", Value::Null),
            Cmd::RotateToggle => ("rotate", json!({"mode": "toggle"})),
            Cmd::KeyboardToggle => ("keyboard", json!({"mode": "toggle"})),
            Cmd::KeyboardShow => ("keyboard", json!({"mode": "show"})),
            Cmd::KeyboardHide => ("keyboard", json!({"mode": "hide"})),
            // shell-local: `Shell::send` applies these and never puts them on the socket
            Cmd::MousePadShow => ("mouse_pad", json!({"mode": "show"})),
            Cmd::MousePadHide => ("mouse_pad", json!({"mode": "hide"})),
            Cmd::ClipboardShow => ("clipboard", json!({"mode": "show"})),
            Cmd::ScreenOff => ("screen", json!({"mode": "off"})),
            Cmd::Back => ("back", Value::Null),
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
            Cmd::WifiEnable(on) => ("wifi_enable", json!({"on": on})),
            Cmd::Hotspot(on) => ("hotspot", json!({"on": on})),
            Cmd::HotspotConfig { ssid, secret } => {
                let mut a = serde_json::Map::new();
                if let Some(n) = ssid {
                    a.insert("ssid".into(), json!(n));
                }
                if let Some(s) = secret {
                    // serialized onto the control socket only, never a log
                    a.insert(
                        "secret".into(),
                        serde_json::to_value(s).unwrap_or(Value::Null),
                    );
                }
                ("hotspot_config", Value::Object(a))
            }
            Cmd::Vpn(on) => ("vpn", json!({"on": on})),
            Cmd::VpnKeep => ("vpn_confirm", Value::Null),
            Cmd::VpnConfig {
                profile,
                kill_switch,
                dns,
                proxy,
            } => {
                let mut a = serde_json::Map::new();
                if let Some(p) = profile {
                    a.insert("profile".into(), json!(p));
                }
                if let Some(k) = kill_switch {
                    a.insert("kill_switch".into(), json!(k));
                }
                if let Some(d) = dns {
                    a.insert("dns".into(), json!(d));
                }
                if let Some(p) = proxy {
                    a.insert("proxy".into(), json!(p));
                }
                ("vpn_config", Value::Object(a))
            }
            Cmd::VpnImport(path) => ("vpn_import", json!({"path": path})),
            Cmd::VpnFiles => ("vpn_files", Value::Null),
            Cmd::MobileRadio(on) => ("mobile_radio", json!({"on": on})),
            Cmd::MobileData(on) => ("mobile_data", json!({"on": on})),
            Cmd::MobileApn(a) => ("mobile_apn", json!({"apn": a})),
            // serialized onto the control socket only, never a log
            Cmd::SimPin(pin) => (
                "sim_pin",
                json!({"pin": serde_json::to_value(pin).unwrap_or(Value::Null)}),
            ),
            Cmd::MobileInfo => ("mobile_info", Value::Null),
            Cmd::BtEnable(on) => ("bluetooth_enable", json!({"on": on})),
            Cmd::BtScan(on) => ("bluetooth_scan", json!({"on": on})),
            Cmd::BtPair(a) => ("bluetooth_pair", json!({"address": a})),
            Cmd::BtConnect(a) => ("bluetooth_connect", json!({"address": a})),
            Cmd::BtDisconnect(a) => ("bluetooth_disconnect", json!({"address": a})),
            Cmd::BtForget(a) => ("bluetooth_forget", json!({"address": a})),
            Cmd::BtConfirm { address, accept } => (
                "bluetooth_confirm",
                json!({"address": address, "accept": accept}),
            ),
            // serialized onto the control socket only, never a log
            Cmd::LockSet {
                kind,
                secret,
                current,
            } => {
                let mut a = json!({
                    "kind": kind,
                    "secret": serde_json::to_value(secret).unwrap_or(Value::Null),
                });
                if let Some(c) = current {
                    a["current"] = serde_json::to_value(c).unwrap_or(Value::Null);
                }
                ("lock_set", a)
            }
            Cmd::LockRemove(c) => (
                "lock_remove",
                json!({"current": serde_json::to_value(c).unwrap_or(Value::Null)}),
            ),
            Cmd::LockNow => ("lock_now", Value::Null),
            Cmd::LockConfig(s) => ("lock_config", json!({"auto_lock_s": s})),
            Cmd::PkgCatalogue => ("pkg_catalogue", Value::Null),
            Cmd::PkgInstall(id) => ("pkg_install", json!({"id": id})),
            Cmd::PkgRemove(id) => ("pkg_remove", json!({"id": id})),
            Cmd::DefaultApp { role, app } => ("default_app", json!({"role": role, "app": app})),
            Cmd::Rotate(m) => ("rotate", json!({"mode": m})),
            Cmd::Setting(k, v) => ("set_setting", json!({"key": k, "value": v})),
            Cmd::RestartShell => ("restart_shell", Value::Null),
            Cmd::PowerMenuState(open) => ("power_menu", json!({"open": open})),
            Cmd::Power { poweroff } => (
                "power",
                json!({"action": if *poweroff { "poweroff" } else { "reboot" }, "confirmed": true}),
            ),
        }
    }

    pub fn purpose(&self) -> Purpose {
        match self {
            Cmd::WifiListSaved => Purpose::SavedList,
            Cmd::WifiScan => Purpose::Scan,
            Cmd::WifiConnect { .. } => Purpose::Connect,
            Cmd::WifiForget(_) => Purpose::Forget,
            Cmd::HotspotConfig { .. } => Purpose::HotspotConfig,
            Cmd::MobileApn(_) => Purpose::MobileApn,
            Cmd::SimPin(_) => Purpose::SimPin,
            Cmd::MobileInfo => Purpose::MobileInfo,
            Cmd::Vpn(_) | Cmd::VpnKeep => Purpose::Vpn,
            Cmd::VpnConfig { dns, proxy, .. } if dns.is_some() || proxy.is_some() => {
                Purpose::VpnSheet
            }
            Cmd::VpnConfig { .. } => Purpose::Vpn,
            Cmd::VpnImport(_) => Purpose::VpnImport,
            Cmd::VpnFiles => Purpose::VpnFiles,
            Cmd::BtPair(_) => Purpose::BtPair,
            Cmd::BtConnect(_) | Cmd::BtDisconnect(_) | Cmd::BtForget(_) => Purpose::BtOp,
            Cmd::LockSet { .. } | Cmd::LockRemove(_) | Cmd::LockNow | Cmd::LockConfig(_) => {
                Purpose::Lock
            }
            Cmd::PkgCatalogue => Purpose::PkgCatalogue,
            Cmd::PkgInstall(_) | Cmd::PkgRemove(_) => Purpose::PkgAction,
            Cmd::DefaultApp { .. } => Purpose::DefaultApp,
            Cmd::CloseApp(_) => Purpose::Close,
            Cmd::Setting(..) | Cmd::SetPinned(_) => Purpose::Setting,
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
    /// Pinned apps (installed; at most four): the sidebar shows them.
    pub tiles: Vec<Tile>,
    /// Every installed app plus the Settings shortcut: the Home grid and the Apps panel.
    pub apps: Vec<Tile>,
    pub pinned: Vec<String>,
    pub wall: WallTime,
    pub pressed: Option<Target>,
    pub slider_active: bool,
    pub slider_value: f32,
    /// Which slider is being dragged: `Target::Slider` (brightness) or `Target::VolumeSlider`.
    pub slider_target: Target,
    /// The volume pill for a hardware key (None = hidden).
    pub osd: Option<Osd>,
    pub net: NetModel,
    pub overlay: Option<Overlay>,
    pub toast: Option<Toast>,
    pub spinner_deg: u16,
    pub devd: DevdSettings,
    pub clock_24h: bool,
    pub sys: SysInfo,
    /// Open Settings subpage (None = the category list).
    pub settings_page: Option<Page>,
    /// IMEI / ICCID from `mobile_info` (asked for when "Show" is tapped; never logged).
    pub mobile_info: Option<MobileInfo>,
    pub imei_shown: bool,
    /// Importable VPN profiles the core found (VPN screen).
    pub vpn_files: Vec<crate::vpn_screen::VpnFile>,
    /// Address of the Bluetooth connect / pair request in progress.
    pub bt_busy: Option<String>,
    /// The open "set / change / remove the screen lock" sequence (Settings > Security).
    pub lock_flow: Option<LockFlow>,
    /// App store screen state.
    pub store: store::StoreModel,
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
            slider_target: Target::Slider,
            osd: None,
            net: NetModel::default(),
            overlay: None,
            toast: None,
            spinner_deg: 0,
            devd: DevdSettings::default(),
            clock_24h: true,
            sys: SysInfo::default(),
            settings_page: None,
            mobile_info: None,
            imei_shown: false,
            vpn_files: Vec::new(),
            bt_busy: None,
            lock_flow: None,
            store: store::StoreModel::default(),
        }
    }

    /// Slider position 0..=1: the dragged value while dragging, else the core's brightness.
    pub fn brightness_value(&self) -> f32 {
        if self.slider_active && self.slider_target == Target::Slider {
            self.slider_value
        } else {
            self.status
                .as_ref()
                .map_or(0.0, |s| s.brightness as f32 / 255.0)
        }
    }

    fn armed_until(&self) -> Option<Ms> {
        match &self.overlay {
            Some(Overlay::PowerConfirm { armed_until, .. }) => *armed_until,
            Some(Overlay::PowerMenu { armed, .. }) => armed.map(|(_, t)| t),
            _ => None,
        }
    }

    fn spinning(&self) -> bool {
        self.net.scanning
            || self.net.connecting.is_some()
            || self.bt_busy.is_some()
            || bluetooth::scanning(&self.status)
            || matches!(&self.overlay, Some(Overlay::Password(s)) if s.connecting)
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub dirty: bool,
    pub cmds: Vec<Cmd>,
    /// The shell should re-read the configuration files and system facts.
    pub refresh: bool,
}

impl Outcome {
    fn cmd(&mut self, c: Cmd) {
        self.cmds.push(c);
    }
}

const SLIDER_SEND_EVERY_MS: Ms = 80;

pub struct App {
    pub frame: Frame,
    /// Cached background layer (charcoal, watermark, sidebar at rest).
    pub chrome: Chrome,
    pub active: Tab,
    pub model: Model,
    catalog: Vec<AppEntry>,
    /// Hit areas in screen coordinates (sidebar, header pill, Home grid).
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
    /// Commands produced by time (tick), drained by the event loop with `take_cmds`.
    queued: Vec<Cmd>,
}

fn percent(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 100.0).round() as u8
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
            chrome: Chrome::default(),
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
            queued: Vec::new(),
        };
        a.rebuild_tiles();
        a
    }

    /// The app definitions changed (an app was installed or removed): rebuild the tiles.
    pub fn set_catalog(&mut self, catalog: Vec<AppEntry>) -> bool {
        let changed = self.catalog != catalog;
        self.catalog = catalog;
        self.rebuild_tiles();
        changed
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

    /// New configuration read from the files (devd.json, shell.json). Returns true to redraw.
    pub fn set_config(&mut self, devd: DevdSettings, clock_24h: bool) -> bool {
        let changed = self.model.devd != devd || self.model.clock_24h != clock_24h;
        self.model.devd = devd;
        self.model.clock_24h = clock_24h;
        changed
    }

    pub fn set_sysinfo(&mut self, sys: SysInfo) -> bool {
        let changed = self.model.sys != sys;
        self.model.sys = sys;
        changed
    }

    /// Remember which request was sent with which id (to interpret its reply).
    pub fn note_sent(&mut self, id: u64, cmd: &Cmd) {
        self.pending.insert(id, cmd.purpose());
    }

    /// A warning toast from outside the model (the Recents strip).
    pub fn notice(&mut self, text: impl Into<String>) {
        self.toast(text, Tone::Warning);
    }

    fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.model.toast = Some(Toast {
            text: text.into(),
            tone,
            until: self.now + TOAST_MS,
        });
    }

    fn go(&mut self, tab: Tab, out: &mut Outcome) {
        if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
            self.model.overlay = None;
        }
        // the Apps destination is the Home grid (the panel is a popover over any screen)
        let tab = if tab == Tab::Apps { Tab::Home } else { tab };
        if tab != self.active {
            self.active = tab;
            self.scroll = 0;
            self.content_hits.clear();
            self.model.settings_page = None;
            self.model.imei_shown = false;
            self.model.store.view = StoreView::Installed;
            self.model.store.armed = None;
        }
        if matches!(tab, Tab::Device | Tab::Settings) {
            out.refresh = true;
        }
        if tab == Tab::Network && self.model.connected {
            out.cmd(Cmd::WifiListSaved);
        }
        if tab == Tab::Vpn && self.model.connected {
            out.cmd(Cmd::VpnFiles);
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
                self.model.bt_busy = None;
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.connecting = false;
                }
                if matches!(self.model.overlay, Some(Overlay::BtPair { .. })) {
                    self.model.overlay = None;
                }
                self.pending.clear();
                if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
                    self.model.overlay = None;
                }
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
            CoreEvent::Notice(v)
                if v["kind"].as_str().is_some_and(|k| k.starts_with("mobile_")) =>
            {
                self.on_mobile_notice(v, &mut out)
            }
            CoreEvent::Notice(v)
                if v["kind"]
                    .as_str()
                    .is_some_and(|k| k.starts_with("bluetooth")) =>
            {
                self.on_bt_notice(v, &mut out)
            }
            CoreEvent::Notice(v) if v["kind"].as_str().is_some_and(|k| k.starts_with("pkg_")) => {
                self.on_pkg_notice(v, &mut out)
            }
            CoreEvent::Notice(v) => {
                if v["kind"] == "app_crashed" {
                    let name = v["name"].as_str().or(v["app"].as_str()).unwrap_or("An app");
                    self.toast(format!("{name} closed unexpectedly"), Tone::Danger);
                    out.dirty = true;
                }
            }
            CoreEvent::Input(k) => self.on_hw_key(*k, &mut out),
            // the volume pill for a hardware key (a slider drag needs no extra feedback)
            CoreEvent::Audio(v) if v["source"] == "key" => {
                self.model.osd = Some(Osd {
                    volume: v["volume"].as_u64().unwrap_or(0).min(100) as u8,
                    muted: v["muted"] == true,
                    available: v["available"] == true,
                    until: now + volume_ui::OSD_MS,
                });
                out.dirty = true;
            }
            CoreEvent::Audio(_) => {}
            // app lifecycle events drive the system keyboard (shell loop), nothing to draw here
            CoreEvent::Apps(_) => {}
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
            Some("vpn") => {
                match v["state"].as_str() {
                    Some("on") => self.toast("VPN connected", Tone::Info),
                    Some("failed") => {
                        let why = v["reason"].as_str().unwrap_or("unknown error");
                        self.toast(format!("VPN: {why}"), Tone::Danger)
                    }
                    _ => {}
                }
                out.dirty = true;
            }
            Some("hotspot") => {
                match v["state"].as_str() {
                    Some("on") if v["exclusive"] == true => {
                        self.toast("Hotspot on, Wi-Fi client paused", Tone::Warning)
                    }
                    Some("on") => self.toast("Hotspot on", Tone::Info),
                    Some("failed") => {
                        let why = v["reason"].as_str().unwrap_or("unknown error");
                        self.toast(format!("Hotspot failed: {why}"), Tone::Danger)
                    }
                    _ => {}
                }
                out.dirty = true;
            }
            _ => {}
        }
    }

    /// Progress and result of an install or uninstall (change app-catalogue-manager).
    fn on_pkg_notice(&mut self, v: &Value, out: &mut Outcome) {
        out.dirty = true;
        let id = v["id"].as_str().unwrap_or("").to_string();
        let op = v["op"].as_str().unwrap_or("install").to_string();
        match v["kind"].as_str() {
            Some("pkg_progress") => {
                self.model.store.busy = Some(store::Busy {
                    op,
                    id,
                    percent: v["percent"].as_u64().unwrap_or(0).min(100) as u8,
                    text: v["text"].as_str().unwrap_or("").to_string(),
                });
            }
            Some("pkg_done") => {
                let name = self.model.store.name_of(&id);
                self.model.store.busy = None;
                if v["ok"] == true {
                    self.model.store.error = None;
                    let verb = if op == "remove" {
                        "uninstalled"
                    } else {
                        "installed"
                    };
                    self.toast(format!("{name} {verb}"), Tone::Info);
                } else {
                    let why = v["error"].as_str().unwrap_or("unknown error");
                    self.model.store.error = Some(format!("{name}: {why}"));
                    self.toast(
                        format!(
                            "Could not {} {name}",
                            if op == "remove" {
                                "uninstall"
                            } else {
                                "install"
                            }
                        ),
                        Tone::Danger,
                    );
                }
                out.cmd(Cmd::PkgCatalogue);
                // the app list changed: re-read apps.json / apps.d
                out.refresh = true;
            }
            _ => {}
        }
    }

    /// Results of radio / PIN / data requests (owner request 2026-11-10).
    fn on_mobile_notice(&mut self, v: &Value, out: &mut Outcome) {
        out.dirty = true;
        let err = v["error"].as_str().unwrap_or("unknown error").to_string();
        match v["kind"].as_str() {
            Some("mobile_pin") => {
                let sheet = matches!(&self.model.overlay,
                    Some(Overlay::Password(s)) if s.purpose == SheetPurpose::SimPin);
                if v["ok"] == true {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast("SIM unlocked", Tone::Info);
                } else if let (true, Some(Overlay::Password(s))) = (sheet, &mut self.model.overlay)
                {
                    s.connecting = false;
                    s.secret.clear();
                    s.error = Some(err);
                } else {
                    self.toast(format!("SIM PIN: {err}"), Tone::Danger);
                }
            }
            Some("mobile_radio") if v["ok"] != true => {
                self.toast(format!("Mobile radio: {err}"), Tone::Danger)
            }
            Some("mobile_data") => match v["state"].as_str() {
                Some("on") => self.toast("Mobile data on", Tone::Info),
                Some("failed") => {
                    let why = v["reason"].as_str().unwrap_or("unknown error");
                    self.toast(format!("Mobile data failed: {why}"), Tone::Danger)
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn bt_name(&self, address: &str) -> String {
        self.model
            .status
            .as_ref()
            .and_then(|s| s.bluetooth.devices.iter().find(|d| d.address == address))
            .map(|d| d.name.clone())
            .unwrap_or_else(|| address.to_string())
    }

    /// Pairing and connect results (owner request 2026-11-10).
    fn on_bt_notice(&mut self, v: &Value, out: &mut Outcome) {
        out.dirty = true;
        let addr = v["address"].as_str().unwrap_or("").to_string();
        let name = self.bt_name(&addr);
        let err = v["error"].as_str().unwrap_or("unknown error").to_string();
        match v["kind"].as_str() {
            Some("bluetooth_pair") => {
                let mine = matches!(&self.model.overlay,
                    Some(Overlay::BtPair { address, .. }) if *address == addr);
                let set = |s: &mut Self, state: PairState| {
                    s.model.overlay = Some(Overlay::BtPair {
                        address: addr.clone(),
                        name: name.clone(),
                        state,
                    })
                };
                let code = v["passkey"].as_str().unwrap_or("").to_string();
                match v["state"].as_str() {
                    Some("confirm") => set(self, PairState::Confirm(code)),
                    Some("passkey") => set(self, PairState::Display(code)),
                    Some("ok") => {
                        self.model.bt_busy = None;
                        if mine {
                            self.model.overlay = None;
                        }
                        self.toast(format!("Paired with {name}"), Tone::Info);
                    }
                    Some("failed") => {
                        self.model.bt_busy = None;
                        if mine {
                            set(self, PairState::Failed(err));
                        } else {
                            self.toast(format!("Pairing failed: {err}"), Tone::Danger);
                        }
                    }
                    _ => {}
                }
            }
            Some("bluetooth") => {
                if self
                    .model
                    .bt_busy
                    .as_deref()
                    .is_none_or(|b| b == addr || addr.is_empty())
                {
                    self.model.bt_busy = None;
                }
                let op = v["op"].as_str().unwrap_or("");
                if v["ok"] == true {
                    if op == "connect" {
                        self.toast(format!("Connected to {name}"), Tone::Info);
                    }
                } else {
                    let what = match op {
                        "connect" => format!("Could not connect to {name}"),
                        "disconnect" => format!("Could not disconnect {name}"),
                        "forget" => format!("Could not forget {name}"),
                        _ => "Bluetooth".to_string(),
                    };
                    self.toast(format!("{what}: {err}"), Tone::Danger);
                }
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
            (Purpose::BtPair | Purpose::BtOp, false) => {
                self.model.bt_busy = None;
                if purpose == Purpose::BtPair
                    && matches!(self.model.overlay, Some(Overlay::BtPair { .. }))
                {
                    self.model.overlay = None;
                }
                if let Some(e) = error {
                    self.toast(friendly(e), Tone::Danger);
                }
            }
            (Purpose::HotspotConfig, ok) => {
                let sheet = matches!(&self.model.overlay,
                    Some(Overlay::Password(s)) if s.purpose != SheetPurpose::WifiJoin);
                if ok {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast(
                        if data["restarting"] == true {
                            "Saved, restarting the hotspot"
                        } else {
                            "Hotspot settings saved"
                        },
                        Tone::Info,
                    );
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    match &mut self.model.overlay {
                        Some(Overlay::Password(s)) if sheet => {
                            s.connecting = false;
                            s.error = Some(msg);
                        }
                        _ => self.toast(msg, Tone::Danger),
                    }
                }
            }
            (Purpose::Lock, ok) => self.on_lock_reply(ok, error, out),
            (Purpose::MobileInfo, true) => {
                self.model.mobile_info = Some(MobileInfo {
                    imei: data["imei"].as_str().map(String::from),
                    iccid: data["iccid"].as_str().map(String::from),
                });
            }
            (Purpose::MobileApn | Purpose::SimPin, ok) => {
                let sheet =
                    matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_mobile());
                if ok && purpose == Purpose::MobileApn {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast(
                        if data["reconnect"] == true {
                            "APN saved: turn mobile data off and on to use it"
                        } else {
                            "APN saved"
                        },
                        Tone::Info,
                    );
                } else if !ok {
                    // PIN accepted for checking: the sheet waits for the `mobile_pin` notice
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    match &mut self.model.overlay {
                        Some(Overlay::Password(s)) if sheet => {
                            s.connecting = false;
                            s.error = Some(msg);
                        }
                        _ => self.toast(msg, Tone::Danger),
                    }
                }
            }
            (Purpose::PkgCatalogue, true) => self.model.store.apply_catalogue(data),
            (Purpose::PkgCatalogue, false) => {
                self.model.store.error =
                    Some(error.map_or_else(|| "Cannot read the catalogue".into(), friendly));
            }
            (Purpose::PkgAction, false) => {
                // refused (no space, built in, busy ...): nothing changed, say why
                self.model.store.busy = None;
                self.model.store.error =
                    Some(error.map_or_else(|| "The request failed".into(), friendly));
            }
            (Purpose::PkgAction, true) => {}
            (Purpose::DefaultApp, true) => out.cmd(Cmd::PkgCatalogue),
            (Purpose::DefaultApp, false) => {
                self.model.store.error =
                    Some(error.map_or_else(|| "Could not change the default app".into(), friendly));
            }
            (Purpose::VpnFiles, true) => {
                self.model.vpn_files = data["files"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|f| {
                                Some(crate::vpn_screen::VpnFile {
                                    name: f["name"].as_str()?.to_string(),
                                    path: f["path"].as_str()?.to_string(),
                                })
                            })
                            .take(crate::vpn_screen::MAX_FILES)
                            .collect()
                    })
                    .unwrap_or_default();
            }
            (Purpose::VpnImport, ok) => {
                if ok {
                    self.toast("VPN profile imported", Tone::Info);
                    out.cmd(Cmd::VpnFiles);
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Import failed".into());
                    self.toast(msg, Tone::Danger);
                }
            }
            (Purpose::Vpn, false) => {
                let msg = error
                    .map(friendly)
                    .unwrap_or_else(|| "VPN request failed".into());
                self.toast(msg, Tone::Danger);
            }
            (Purpose::VpnSheet, ok) => {
                let sheet = matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_vpn());
                if ok {
                    if sheet {
                        self.close_overlay();
                    }
                    self.toast("Saved", Tone::Info);
                } else {
                    let msg = error
                        .map(friendly)
                        .unwrap_or_else(|| "Could not save".into());
                    match &mut self.model.overlay {
                        Some(Overlay::Password(s)) if sheet => {
                            s.connecting = false;
                            s.error = Some(msg);
                        }
                        _ => self.toast(msg, Tone::Danger),
                    }
                }
            }
            (Purpose::Setting, ok) => {
                // the core wrote (or refused) the file: show what is really configured
                out.refresh = true;
                if let (false, Some(e)) = (ok, error) {
                    self.toast(friendly(e), Tone::Danger);
                }
            }
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
        if self.model.osd.is_some_and(|o| now >= o.until) {
            self.model.osd = None;
            dirty = true;
        }
        if self.armed_until().is_some_and(|t| now >= t) {
            match &mut self.model.overlay {
                Some(Overlay::PowerConfirm { armed_until, .. }) => *armed_until = None,
                Some(Overlay::PowerMenu { armed, .. }) => *armed = None,
                _ => {}
            }
            dirty = true;
        }
        if matches!(&self.model.overlay, Some(Overlay::PowerMenu { until, .. }) if now >= *until) {
            self.close_power_menu();
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
        let menu = match &self.model.overlay {
            Some(Overlay::PowerMenu { until, .. }) => Some(*until),
            _ => None,
        };
        let osd = self.model.osd.map(|o| o.until);
        [spin, toast, osd, self.armed_until(), menu]
            .into_iter()
            .flatten()
            .min()
    }

    // ------------------------------------------------------------------ banners
    fn armed_until(&self) -> Option<Ms> {
        self.model.armed_until()
    }

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
            Tab::Network | Tab::Device | Tab::Settings | Tab::Mobile | Tab::Bluetooth | Tab::Vpn
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

    /// The brightness slider's rectangle (Quick Settings: overlay hits; Device: content hits). x is
    /// never scrolled.
    fn slider_rect(&self) -> Option<Rect> {
        self.overlay_hits
            .iter()
            .chain(self.hits.iter())
            .chain(self.content_hits.iter())
            .find(|(_, t)| *t == self.model.slider_target)
            .map(|(r, _)| *r)
    }

    fn slider_value_at(&self, x: i32) -> f32 {
        let track = self
            .slider_rect()
            .map(|r| r.inset(SP_4))
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
                let slider_ok = match t {
                    Some(Target::Slider) => self.model.connected,
                    Some(Target::VolumeSlider) => volume_ui::usable(&self.model),
                    _ => false,
                };
                if slider_ok {
                    self.model.slider_target = t.unwrap_or(Target::Slider);
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
                let panel = matches!(self.model.overlay, Some(Overlay::AppsPanel));
                if self.model.overlay.is_none() || panel {
                    let id = match self.hit(x, y) {
                        Some(Target::Tile(i)) => self.model.tiles.get(i).map(|t| t.id.clone()),
                        Some(Target::AppTile(i) | Target::PanelActive(i)) => {
                            self.model.apps.get(i).map(|t| t.id.clone())
                        }
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
                    let w = self.slider_rect().map_or(1, |r| r.inset(SP_4).w.max(1));
                    self.model.slider_value =
                        (self.model.slider_value + dx as f32 / w as f32).clamp(0.0, 1.0);
                    out.dirty = true;
                    if now.saturating_sub(self.last_slider_send) >= SLIDER_SEND_EVERY_MS {
                        self.last_slider_send = now;
                        out.cmd(self.slider_cmd());
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
                    out.cmd(self.slider_cmd());
                    out.dirty = true;
                }
            }
        }
        out
    }

    /// The command for the slider being dragged (its value is in `slider_value`).
    fn slider_cmd(&self) -> Cmd {
        match self.model.slider_target {
            Target::VolumeSlider => Cmd::SetVolume(percent(self.model.slider_value)),
            _ => Cmd::SetBrightness(native_brightness(self.model.slider_value)),
        }
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
        if !matches!(t, Target::Store(StoreTarget::Remove(_))) {
            self.model.store.armed = None;
        }
        match t {
            Target::Nav(NAV_APPS) => self.model.overlay = Some(Overlay::AppsPanel),
            Target::Nav(i) => self.go(Tab::ALL[i], out),
            Target::QuickSettings => self.model.overlay = Some(Overlay::QuickSettings),
            Target::Dismiss => {
                if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
                    self.model.overlay = None;
                }
            }
            Target::PanelBody => {}
            Target::PanelActive(i) => {
                self.model.overlay = None;
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    self.activate(&id, out);
                }
            }
            // the close badge: the panel stays open (the app leaves "Active apps" with the next
            // status), so several apps can be closed in a row
            Target::PanelClose(i) if interactive => {
                if let Some(id) = self.model.apps.get(i).map(|t| t.id.clone()) {
                    out.cmd(Cmd::CloseApp(id));
                }
            }
            Target::PanelTool(i) => {
                if let Some(tool) = panels::TOOLS.get(i) {
                    self.model.overlay = None;
                    match tool.action {
                        panels::ToolAction::MousePad => out.cmd(Cmd::MousePadShow),
                        panels::ToolAction::Clipboard => out.cmd(Cmd::ClipboardShow),
                        panels::ToolAction::AppStore => {
                            self.go(Tab::Settings, out);
                            self.model.settings_page = Some(Page::Apps);
                            self.model.store.view = StoreView::Browse;
                            out.cmd(Cmd::PkgCatalogue);
                        }
                    }
                }
            }
            Target::Page(Page::Vpn) => self.go(Tab::Vpn, out),
            Target::VpnSwitch if interactive && vpn_screen::switchable(&self.model.status) => {
                out.cmd(Cmd::Vpn(!vpn_screen::is_on(&self.model.status)))
            }
            Target::VpnKill if interactive && vpn_screen::editable(&self.model.status) => {
                let ks = self
                    .model
                    .status
                    .as_ref()
                    .is_some_and(|s| s.vpn.kill_switch);
                out.cmd(Cmd::VpnConfig {
                    profile: None,
                    kill_switch: Some(!ks),
                    dns: None,
                    proxy: None,
                })
            }
            Target::VpnKeep if interactive => out.cmd(Cmd::VpnKeep),
            Target::VpnProfile if interactive && vpn_screen::editable(&self.model.status) => {
                if let Some(s) = &self.model.status {
                    let names: Vec<&String> = s.vpn.profiles.iter().map(|p| &p.name).collect();
                    let cur = names
                        .iter()
                        .position(|n| Some(*n) == s.vpn.profile.as_ref());
                    if let Some(next) = names.get(cur.map_or(0, |i| (i + 1) % names.len().max(1))) {
                        out.cmd(Cmd::VpnConfig {
                            profile: Some((*next).clone()),
                            kill_switch: None,
                            dns: None,
                            proxy: None,
                        });
                    }
                }
            }
            Target::VpnImport(i) if interactive => {
                if let Some(f) = self.model.vpn_files.get(i) {
                    out.cmd(Cmd::VpnImport(f.path.clone()));
                }
            }
            Target::VpnDns | Target::VpnProxy if interactive => {
                let dns = matches!(t, Target::VpnDns);
                let cur = self.model.status.as_ref().map(|s| {
                    if dns {
                        s.vpn.dns.join(", ")
                    } else {
                        s.vpn.proxy.clone().unwrap_or_default()
                    }
                });
                self.model.overlay = Some(Overlay::Password(PasswordSheet::vpn(
                    if dns {
                        SheetPurpose::VpnDns
                    } else {
                        SheetPurpose::VpnProxy
                    },
                    &cur.unwrap_or_default(),
                )));
            }
            Target::WifiCard | Target::HotspotCard => self.go(Tab::Network, out),
            Target::MobileCard | Target::Page(Page::Mobile) => self.go(Tab::Mobile, out),
            Target::BtCard | Target::Page(Page::Bluetooth) => self.go(Tab::Bluetooth, out),
            Target::BtSwitch if interactive && bluetooth::switchable(&self.model.status) => {
                out.cmd(Cmd::BtEnable(!bluetooth::switch_on(&self.model.status)))
            }
            Target::BtScan if interactive && bluetooth::powered(&self.model.status) => {
                out.cmd(Cmd::BtScan(!bluetooth::scanning(&self.model.status)))
            }
            Target::BtAction(i) if interactive && self.model.bt_busy.is_none() => {
                let d = bluetooth::my_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.connected));
                if let Some((address, connected)) = d {
                    self.model.bt_busy = Some(address.clone());
                    out.cmd(if connected {
                        Cmd::BtDisconnect(address)
                    } else {
                        Cmd::BtConnect(address)
                    });
                }
            }
            Target::BtForget(i) if interactive => {
                let d = bluetooth::my_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.name.clone()));
                if let Some((address, name)) = d {
                    self.model.overlay = Some(Overlay::BtForget { address, name });
                }
            }
            Target::BtAvail(i) if interactive && self.model.bt_busy.is_none() => {
                let d = bluetooth::available_devices(&self.model.status)
                    .get(i)
                    .map(|d| (d.address.clone(), d.name.clone()));
                if let Some((address, name)) = d {
                    self.model.bt_busy = Some(address.clone());
                    self.model.overlay = Some(Overlay::BtPair {
                        address: address.clone(),
                        name,
                        state: PairState::Waiting,
                    });
                    out.cmd(Cmd::BtPair(address));
                }
            }
            Target::BtPairAccept | Target::BtPairReject => {
                let accept = t == Target::BtPairAccept;
                if let Some(Overlay::BtPair { address, name, .. }) = self.model.overlay.take() {
                    out.cmd(Cmd::BtConfirm {
                        address: address.clone(),
                        accept,
                    });
                    if accept {
                        self.model.overlay = Some(Overlay::BtPair {
                            address,
                            name,
                            state: PairState::Waiting,
                        });
                    } else {
                        self.model.bt_busy = None;
                    }
                }
            }
            Target::MobileRadio if interactive && mobile::radio_switchable(&self.model.status) => {
                out.cmd(Cmd::MobileRadio(!mobile::radio_on(&self.model.status)))
            }
            Target::MobileDataSwitch
                if interactive && mobile::data_switchable(&self.model.status) =>
            {
                out.cmd(Cmd::MobileData(!mobile::data_on(&self.model.status)))
            }
            Target::MobileApn if interactive => {
                let apn = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| s.mobile.apn.clone())
                    .unwrap_or_default();
                self.model.overlay = Some(Overlay::Password(PasswordSheet::mobile(
                    SheetPurpose::MobileApn,
                    &apn,
                )));
            }
            Target::SimPinEntry if interactive && mobile::pin_needed(&self.model.status) => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::mobile(
                    SheetPurpose::SimPin,
                    "",
                )));
            }
            Target::ImeiShow if interactive => {
                self.model.imei_shown = !self.model.imei_shown;
                if self.model.imei_shown && self.model.mobile_info.is_none() {
                    out.cmd(Cmd::MobileInfo);
                }
            }
            Target::WifiSwitch if interactive && self.model.status.is_some() => {
                out.cmd(Cmd::WifiEnable(!network::wifi_enabled(&self.model.status)))
            }
            Target::HotspotSwitch if interactive && self.model.status.is_some() => {
                self.hotspot_switch(out)
            }
            Target::HotspotName if interactive => {
                let name = self
                    .model
                    .status
                    .as_ref()
                    .and_then(|s| s.hotspot.ssid.clone())
                    .unwrap_or_else(|| "WalPad".into());
                self.model.overlay = Some(Overlay::Password(PasswordSheet::hotspot(
                    SheetPurpose::HotspotName,
                    &name,
                )));
            }
            Target::HotspotPassword if interactive => {
                self.model.overlay = Some(Overlay::Password(PasswordSheet::hotspot(
                    SheetPurpose::HotspotPassword,
                    "",
                )));
            }
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
                    self.model.slider_target = Target::Slider;
                    self.slider_value_at(x)
                };
                out.cmd(Cmd::SetBrightness(native_brightness(v)));
            }
            Target::VolumeSlider if volume_ui::usable(&self.model) => {
                let v = if was_slider {
                    self.model.slider_value
                } else {
                    self.model.slider_target = Target::VolumeSlider;
                    self.slider_value_at(x)
                };
                out.cmd(Cmd::SetVolume(percent(v)));
            }
            Target::MuteToggle if volume_ui::usable(&self.model) => {
                out.cmd(Cmd::Mute(!volume_ui::muted(&self.model)))
            }
            Target::TestTone if interactive => out.cmd(Cmd::TestTone),
            Target::Rotation if interactive => out.cmd(Cmd::RotateToggle),
            Target::ScreenOff if interactive => out.cmd(Cmd::ScreenOff),
            Target::Scan
                if interactive
                    && !self.model.net.scanning
                    && !network::client_off(&self.model.status) =>
            {
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
            // ---- Device
            Target::Rot(i) if interactive => {
                if let Some((_, mode)) = device_screen::ROTATION_ROWS.get(i) {
                    out.cmd(Cmd::Rotate(mode));
                }
            }
            Target::Dim(i) if interactive => {
                if let Some((v, _)) = device_screen::DIM_OPTIONS.get(i) {
                    out.cmd(Cmd::Setting("dim_after_s", json!(v)));
                }
            }
            Target::Off(i) if interactive => {
                if let Some((v, _)) = device_screen::OFF_OPTIONS.get(i) {
                    out.cmd(Cmd::Setting("off_after_s", json!(v)));
                }
            }
            Target::LidToggle if interactive => {
                out.cmd(Cmd::Setting("lid_off", json!(!self.model.devd.lid_off)))
            }
            // ---- Settings
            Target::Page(Page::Display) => self.go(Tab::Device, out),
            Target::Page(pg) => {
                self.model.settings_page = Some(pg);
                if pg == Page::Apps {
                    self.model.store.view = StoreView::Installed;
                    out.cmd(Cmd::PkgCatalogue);
                }
                self.scroll = 0;
                self.content_hits.clear();
                out.refresh = true;
            }
            Target::Back
                if self.model.settings_page == Some(Page::Apps)
                    && self.model.store.view != StoreView::Installed =>
            {
                self.model.store.view = StoreView::Installed;
                self.scroll = 0;
                self.content_hits.clear();
            }
            Target::Store(t) if interactive => self.on_store_tap(t, out),
            Target::Back => {
                self.model.settings_page = match self.model.settings_page {
                    Some(Page::Logs) => Some(Page::Maintenance),
                    _ => None,
                };
                self.scroll = 0;
                self.content_hits.clear();
            }
            Target::ClockToggle if interactive => {
                out.cmd(Cmd::Setting("clock_24h", json!(!self.model.clock_24h)))
            }
            Target::RestartShell if interactive => {
                out.cmd(Cmd::RestartShell);
                self.toast("Restarting the shell", Tone::Info);
            }
            Target::Reboot => {
                self.model.overlay = Some(Overlay::PowerConfirm {
                    poweroff: false,
                    armed_until: None,
                })
            }
            Target::Shutdown => {
                self.model.overlay = Some(Overlay::PowerConfirm {
                    poweroff: true,
                    armed_until: None,
                })
            }
            Target::PowerMenu => self.open_power_menu(out),
            Target::PowerItem(i) => self.power_item(i, out),
            Target::Lock(a) if interactive => self.on_lock_tap(a, out),
            // ---- overlays
            Target::SheetCancel => self.close_overlay(),
            Target::DialogCancel => self.close_overlay(),
            Target::DialogConfirm => match self.model.overlay.take() {
                Some(Overlay::Forget { id, .. }) => out.cmd(Cmd::WifiForget(id)),
                Some(Overlay::BtForget { address, .. }) => out.cmd(Cmd::BtForget(address)),
                // two taps: the first arms the button for 3 s ("Tap again to shut down")
                Some(Overlay::PowerConfirm {
                    poweroff,
                    armed_until,
                }) => {
                    if armed_until.is_some_and(|t| self.now < t) {
                        if self.model.connected {
                            out.cmd(Cmd::Power { poweroff });
                        }
                    } else {
                        self.model.overlay = Some(Overlay::PowerConfirm {
                            poweroff,
                            armed_until: Some(self.now + POWER_ARM_MS),
                        });
                    }
                }
                other => self.model.overlay = other,
            },
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

    fn on_store_tap(&mut self, t: StoreTarget, out: &mut Outcome) {
        let st = &mut self.model.store;
        match t {
            StoreTarget::DismissError => st.error = None,
            StoreTarget::Browse => {
                st.view = StoreView::Browse;
                self.scroll = 0;
                self.content_hits.clear();
            }
            StoreTarget::Role(i) => {
                st.view = StoreView::Role(i);
                self.scroll = 0;
                self.content_hits.clear();
            }
            StoreTarget::PickDefault(i) => {
                if let (StoreView::Role(ri), Some(e)) = (st.view, st.entries.get(i)) {
                    if let Some(role) = walpad_proto::DEFAULT_ROLES.get(ri) {
                        out.cmd(Cmd::DefaultApp {
                            role: role.to_string(),
                            app: e.id.clone(),
                        });
                    }
                }
            }
            StoreTarget::Install(i) => {
                let Some(e) = st.entries.get(i).cloned() else {
                    return;
                };
                if st.busy.is_some() {
                    return;
                }
                if let Some(why) = st.space_problem(&e) {
                    // nothing is sent: explain it here, nothing changes
                    st.error = Some(format!("{}: {why}", e.name));
                    return;
                }
                st.error = None;
                st.busy = Some(store::Busy {
                    op: "install".into(),
                    id: e.id.clone(),
                    percent: 0,
                    text: "Starting the installation".into(),
                });
                out.cmd(Cmd::PkgInstall(e.id));
            }
            StoreTarget::Remove(i) => {
                let Some(e) = st.entries.get(i).cloned() else {
                    return;
                };
                if st.busy.is_some() || e.system {
                    return;
                }
                if st.armed != Some(i) {
                    st.armed = Some(i);
                    return;
                }
                st.armed = None;
                st.error = None;
                st.busy = Some(store::Busy {
                    op: "remove".into(),
                    id: e.id.clone(),
                    percent: 0,
                    text: "Starting the removal".into(),
                });
                out.cmd(Cmd::PkgRemove(e.id));
            }
        }
    }

    /// Rows of the open Settings subpage (empty for the category list).
    fn settings_rows(&self) -> Vec<settings_screen::Row> {
        match self.model.settings_page {
            None => Vec::new(),
            Some(Page::Sound) => volume_ui::sound_rows(&self.model),
            Some(pg) => settings_screen::page_rows(
                pg,
                &settings_screen::Ctx {
                    sys: &self.model.sys,
                    status: &self.model.status,
                    devd: &self.model.devd,
                    clock_24h: self.model.clock_24h,
                },
            ),
        }
    }

    /// Commands the clock produced (the power menu closing itself); the loop sends them.
    pub fn take_cmds(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.queued)
    }

    /// Open the power menu with Cancel focused and tell the core to route the hardware keys here.
    fn open_power_menu(&mut self, out: &mut Outcome) {
        if !self.model.connected {
            return;
        }
        self.close_overlay();
        self.model.overlay = Some(Overlay::PowerMenu {
            focus: overlay::POWER_CANCEL,
            armed: None,
            until: self.now + POWER_MENU_MS,
        });
        out.cmd(Cmd::PowerMenuState(true));
        out.dirty = true;
    }

    /// Close the menu from the clock (no `Outcome` to put the command in).
    fn close_power_menu(&mut self) {
        if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
            self.model.overlay = None;
            self.queued.push(Cmd::PowerMenuState(false));
        }
    }

    /// Hardware keys from the core: only the power menu uses them.
    fn on_hw_key(&mut self, k: ShellInput, out: &mut Outcome) {
        if k == ShellInput::PowerMenuOpen {
            if !matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
                self.open_power_menu(out);
            }
            return;
        }
        let Some(Overlay::PowerMenu { focus, .. }) = &mut self.model.overlay else {
            return;
        };
        let n = overlay::POWER_ITEMS.len();
        match k {
            ShellInput::VolumeDown => *focus = (*focus + 1) % n,
            ShellInput::VolumeUp => *focus = (*focus + n - 1) % n,
            ShellInput::PowerShort => {
                let i = *focus;
                self.power_item(i, out);
            }
            // the system keyboard is the shell loop's (keyboard.rs)
            ShellInput::PowerMenuOpen | ShellInput::KeyboardShow | ShellInput::KeyboardHide => {
                return
            }
        }
        out.dirty = true;
    }

    /// Activate a power menu item: Screen off and Cancel act at once, Restart and Shut down need
    /// a second tap within 3 s.
    fn power_item(&mut self, i: usize, out: &mut Outcome) {
        let Some(Overlay::PowerMenu { focus, armed, .. }) = &mut self.model.overlay else {
            return;
        };
        let Some((_, two_step)) = overlay::POWER_ITEMS.get(i).copied() else {
            return;
        };
        *focus = i;
        out.dirty = true;
        if two_step && !armed.is_some_and(|(a, t)| a == i && self.now < t) {
            *armed = Some((i, self.now + POWER_ARM_MS));
            return;
        }
        self.model.overlay = None;
        out.cmd(Cmd::PowerMenuState(false));
        match i {
            0 if self.model.connected => out.cmd(Cmd::ScreenOff),
            1 | 2 if self.model.connected => out.cmd(Cmd::Power { poweroff: i == 2 }),
            _ => {}
        }
    }

    fn close_overlay(&mut self) {
        if matches!(self.model.overlay, Some(Overlay::PowerMenu { .. })) {
            self.close_power_menu();
            return;
        }
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            s.secret.clear();
        }
        self.model.net.connecting = None;
        self.model.lock_flow = None;
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

    /// Hotspot switch: off when it runs or starts; on needs a password (opens the sheet first).
    fn hotspot_switch(&mut self, out: &mut Outcome) {
        let Some(has_secret) = self.model.status.as_ref().map(|s| s.hotspot.has_secret) else {
            return;
        };
        if network::hotspot_on(&self.model.status) {
            out.cmd(Cmd::Hotspot(false));
        } else if !has_secret {
            if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
                self.go(Tab::Network, out);
            }
            let mut s = PasswordSheet::hotspot(SheetPurpose::HotspotPassword, "");
            s.error = Some("Set a password first".into());
            self.model.overlay = Some(Overlay::Password(s));
        } else {
            out.cmd(Cmd::Hotspot(true));
        }
    }

    fn try_connect(&mut self, out: &mut Outcome) {
        if matches!(&self.model.overlay, Some(Overlay::Password(s)) if s.is_lock()) {
            return self.lock_submit(out);
        }
        if let Some(Overlay::Password(s)) = &mut self.model.overlay {
            if s.is_vpn() {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    out.cmd(if s.purpose == SheetPurpose::VpnDns {
                        Cmd::VpnConfig {
                            profile: None,
                            kill_switch: None,
                            dns: Some(overlay::dns_list(&s.secret)),
                            proxy: None,
                        }
                    } else {
                        Cmd::VpnConfig {
                            profile: None,
                            kill_switch: None,
                            dns: None,
                            proxy: Some(s.secret.clone()),
                        }
                    });
                } else if !s.valid() {
                    s.error = Some(
                        if s.purpose == SheetPurpose::VpnDns {
                            "Up to 3 IP addresses, e.g. 1.1.1.1"
                        } else {
                            "Use http://host:port, no user name or password"
                        }
                        .into(),
                    );
                }
                return;
            }
            if s.is_mobile() {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    out.cmd(if s.purpose == SheetPurpose::SimPin {
                        Cmd::SimPin(Secret::new(s.secret.clone()))
                    } else {
                        Cmd::MobileApn(s.secret.clone())
                    });
                } else if !s.valid() {
                    s.error = Some(
                        if s.purpose == SheetPurpose::SimPin {
                            "The PIN has 4-8 digits"
                        } else {
                            "Use letters, digits, . - _ (up to 62)"
                        }
                        .into(),
                    );
                }
                return;
            }
            if s.purpose != SheetPurpose::WifiJoin {
                if s.valid() && !s.connecting {
                    s.connecting = true;
                    s.error = None;
                    let text = s.secret.clone();
                    out.cmd(if s.purpose == SheetPurpose::HotspotName {
                        Cmd::HotspotConfig {
                            ssid: Some(text),
                            secret: None,
                        }
                    } else {
                        Cmd::HotspotConfig {
                            ssid: None,
                            secret: Some(Secret::new(text)),
                        }
                    });
                }
                return;
            }
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
                if s.secret.len() + c.len_utf8() <= s.max_len() {
                    s.secret.push(c);
                    s.error = None;
                }
                if s.shift {
                    s.shift = false;
                    s.layer = KeyboardLayer::Lower;
                }
            }
            Key::Space => {
                if s.secret.len() < s.max_len() {
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
    /// One frame: cached background (memcpy), pressed sidebar button, the screen, header, banner,
    /// overlays, toast. Scrolled screens draw straight into the frame through the canvas origin.
    pub fn render(&mut self, p: &mut Painter) {
        let frame = self.frame;
        let panel_open = matches!(self.model.overlay, Some(Overlay::AppsPanel));
        let sidebar =
            chrome::sidebar_layout(&frame, &chrome::pinned_items(&self.model), panel_open);
        let key = ChromeKey {
            w: frame.w,
            h: frame.h,
            sidebar: sidebar.clone(),
            active: chrome::active_target(self.active, panel_open),
            page: self.scrolls(),
        };
        self.chrome.paint(p, &frame, key);
        self.chrome.render_pressed(p, &sidebar, self.model.pressed);
        self.hits.clear();
        self.content_hits.clear();
        for it in &sidebar.items {
            self.hits.push((it.hit, it.target));
        }

        let banner = self.banner();
        let banner_h = self.banner_h();
        self.view = Rect::new(
            frame.content.x,
            frame.content.y + banner_h,
            frame.content.w,
            frame.content.h - banner_h,
        );
        if self.scrolls() {
            self.render_scrolled(p);
            // scrolled rows may have run up under the header and banner: restore that strip
            self.chrome.restore(
                p.cv,
                Rect::new(frame.header.x, 0, frame.header.w, frame.header.h + banner_h),
            );
        } else {
            let home = home::render(p, &frame, &self.model, banner_h);
            self.hits.extend(home);
        }
        let qs_open = matches!(self.model.overlay, Some(Overlay::QuickSettings));
        let pill = chrome::render_header(p, &frame, &self.model, qs_open);
        self.hits.push((pill, Target::QuickSettings));
        if let Some((text, tone)) = banner {
            p.banner(
                Rect::new(
                    frame.content.x + SP_4,
                    frame.content.y + SP_1,
                    frame.content.w - 2 * SP_4,
                    48,
                ),
                text,
                tone,
            );
        }

        let deg = self.model.spinner_deg;
        self.overlay_hits = match &self.model.overlay {
            Some(Overlay::Password(s)) => overlay::render_password(p, &frame, s, deg),
            Some(Overlay::App(s)) => overlay::render_app_sheet(p, &frame, s),
            Some(Overlay::Forget { ssid, .. }) => overlay::render_forget(p, &frame, ssid),
            Some(Overlay::BtPair { name, state, .. }) => {
                bluetooth::render_pair(p, &frame, name, state, deg)
            }
            Some(Overlay::BtForget { name, .. }) => bluetooth::render_forget(p, &frame, name),
            Some(Overlay::PowerConfirm {
                poweroff,
                armed_until,
            }) => overlay::render_power_confirm(p, &frame, *poweroff, armed_until.is_some()),
            Some(Overlay::PowerMenu { focus, armed, .. }) => {
                overlay::render_power_menu(p, &frame, *focus, armed.map(|(i, _)| i))
            }
            Some(Overlay::QuickSettings) => panels::render_quick_settings(p, &frame, &self.model),
            Some(Overlay::AppsPanel) => panels::render_apps_panel(p, &frame, &self.model),
            None => Vec::new(),
        };
        if let Some(t) = &self.model.toast {
            p.toast(frame.content, &t.text, t.tone);
        }
        if let Some(o) = &self.model.osd {
            volume_ui::render_osd(p, frame.content, o);
        }
    }

    /// Scrolled screens: the whole layout is walked in content coordinates with the canvas origin
    /// at (0, view.y - scroll); primitives skip what falls outside the frame. No off-screen copy.
    fn render_scrolled(&mut self, p: &mut Painter) {
        let frame = self.frame;
        let total = match self.active {
            Tab::Vpn => {
                vpn_screen::layout(&frame, &self.model.status, self.model.vpn_files.len()).total
            }
            Tab::Network => network::layout(&frame, &self.model.status, &self.model.net).total,
            Tab::Device => device_screen::layout(&frame, &self.model.status, &self.model.devd).1,
            Tab::Settings if self.model.settings_page == Some(Page::Apps) => {
                store::layout(&frame, &self.model.store).total
            }
            Tab::Settings => {
                settings_screen::layout(&frame, self.model.settings_page, &self.settings_rows())
                    .total
            }
            Tab::Mobile => mobile::layout(&frame, &self.model.status).total,
            Tab::Bluetooth => bluetooth::layout(&frame, &self.model.status).total,
            _ => 0,
        }
        .max(self.view.h);
        self.content_h = total;
        self.scroll = self.scroll.clamp(0, (total - self.view.h).max(0));
        p.cv.set_origin(0, self.view.y - self.scroll);
        let m = &self.model;
        self.content_hits = match self.active {
            Tab::Network => network::render(
                p,
                &frame,
                &m.status,
                &m.net,
                m.connected,
                m.pressed,
                m.spinner_deg,
            ),
            Tab::Device => device_screen::render(
                p,
                &frame,
                &m.status,
                &m.devd,
                m.connected,
                m.brightness_value(),
                m.slider_active,
                m.pressed,
            ),
            Tab::Settings if m.settings_page == Some(Page::Apps) => {
                store::render(p, &frame, &m.store, m.connected, m.pressed)
            }
            Tab::Settings => {
                let rows = self.settings_rows();
                settings_screen::render(p, &frame, m.settings_page, &rows, m.connected, m.pressed)
            }
            Tab::Mobile => mobile::render(
                p,
                &frame,
                &mobile::View {
                    st: &m.status,
                    info: &m.mobile_info,
                    imei_shown: m.imei_shown,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Bluetooth => bluetooth::render(
                p,
                &frame,
                &bluetooth::View {
                    st: &m.status,
                    online: m.connected,
                    pressed: m.pressed,
                    spinner_deg: m.spinner_deg,
                    busy: m.bt_busy.as_deref(),
                },
            ),
            Tab::Vpn => vpn_screen::render(
                p,
                &frame,
                &vpn_screen::View {
                    st: &m.status,
                    files: &m.vpn_files,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Home | Tab::Apps => Vec::new(),
        };
        p.cv.set_origin(0, 0);
    }
}

#[cfg(test)]
pub(crate) mod tests;
