//! Shell commands, request purposes, and gesture/event outcomes.

use crate::topbar::StatItem;
use serde_json::Value;
use walpad_proto::Secret;

#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    SetBrightness(u8),
    /// `volume_set` percent 0-100, `mute`, `volume_test` (owner request 2026-11-10).
    SetVolume(u8),
    Mute(bool),
    TestTone,
    JackForce(Option<walpad_proto::JackKind>),
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
    /// Open one file in an app that declares `open` (the core validates the path).
    LaunchFile {
        app: String,
        path: String,
    },
    /// Media screen work for the shell's background thread (never sent to the core): scan the
    /// folders, make thumbnails, read media facts, remember an opened file.
    MediaScan,
    MediaThumbs(Vec<String>),
    MediaInfo(Vec<String>),
    MediaRemember(String),
    CloseApp(String),
    /// Window bar and Settings > Input (change multitasking-windows): one more window of an
    /// app, focus / close / place one window by id, the layout mode.
    NewInstance(String),
    FocusWindow(u32),
    CloseWindow(u32),
    WindowRect {
        window: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    Snap {
        window: u32,
        zone: &'static str,
    },
    Layout {
        mode: Option<&'static str>,
        ratio: Option<u8>,
        swap: bool,
    },
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
    /// The GPS switch (`gps`).
    Gps(bool),
    /// Put text on the clipboard (shell-local; the text is a position: never logged).
    CopyText(String),
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
    /// Phone (owner request 2026-11-11): numbers and texts are `Secret`s, so a Debug print of a
    /// command can never leak them.
    PhoneDial(Secret),
    PhoneUssdCancel,
    PhoneSimContacts,
    SmsSend {
        number: Secret,
        text: Secret,
    },
    SmsList,
    SmsDelete(u64),
    SmsRead(Secret),
    ContactList,
    ContactSave {
        id: Option<String>,
        name: String,
        number: Secret,
    },
    ContactDelete(String),
    CallLog,
    CallLogClear,
    /// Notification centre (owner request 2026-11-12, change `device-notifications`).
    Notifications,
    NotificationMarkRead(u64),
    NotificationsMarkAllRead,
    NotificationsClearAll,
    /// `rotate` mode: auto, 0, 90, 180, 270.
    Rotate(&'static str),
    /// `stats_config`: show or hide one header figure.
    Stats(StatItem, bool),
    /// `set_setting` with an allowlisted key.
    Setting(&'static str, Value),
    /// `set_time`: set the system clock manually (`YYYY-MM-DD HH:MM`).
    SetTime(String),
    /// `open_map`: open OpenStreetMap in the browser at optional `(lat, lon)`.
    OpenMap(Option<(f64, f64)>),
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
    /// Phone data (owner request 2026-11-11): what the reply fills in.
    PhoneSms,
    PhoneContacts,
    PhoneCalls,
    /// A send / delete / save whose success refreshes the lists and whose failure is a toast.
    PhoneAction,
    /// A VPN command whose failure is shown as a toast.
    Vpn,
    /// The GPS switch: a failure is shown as a toast.
    Gps,
    VpnSheet,
    VpnImport,
    VpnFiles,
    /// Notification centre list fetch (owner request 2026-11-12, change `device-notifications`).
    Notifications,
    Other,
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub dirty: bool,
    pub cmds: Vec<Cmd>,
    /// The shell should re-read the configuration files and system facts.
    pub refresh: bool,
}

impl Outcome {
    pub(super) fn cmd(&mut self, c: Cmd) {
        self.cmds.push(c);
    }
}
