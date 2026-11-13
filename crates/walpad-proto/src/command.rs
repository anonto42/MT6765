use crate::{ErrorCode, JackKind, LockKind, Secret};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const COMMAND_NAMES: &[&str] = &[
    "hello",
    "subscribe",
    "get_status",
    "screen",
    "set_brightness",
    "rotate",
    "wifi_scan",
    "wifi_list_saved",
    "wifi_connect",
    "wifi_disconnect",
    "wifi_forget",
    "wifi_enable",
    "hotspot",
    "hotspot_config",
    "gps",
    "stats_config",
    "mobile_radio",
    "mobile_data",
    "mobile_apn",
    "sim_pin",
    "mobile_info",
    "bluetooth_enable",
    "bluetooth_scan",
    "bluetooth_pair",
    "bluetooth_connect",
    "bluetooth_disconnect",
    "bluetooth_forget",
    "bluetooth_confirm",
    "vpn",
    "vpn_config",
    "vpn_confirm",
    "vpn_import",
    "vpn_remove",
    "vpn_files",
    "launch",
    "close_app",
    "new_instance",
    "windows",
    "focus_window",
    "close_window",
    "window_rect",
    "snap",
    "layout",
    "home",
    "back",
    "keyboard",
    "power",
    "restart_shell",
    "screenshot",
    "set_setting",
    "set_time",
    "open_map",
    "power_menu",
    "volume_set",
    "volume_delta",
    "mute",
    "volume_test",
    "jack_force",
    "lock_set",
    "lock_remove",
    "unlock",
    "lock_now",
    "lock_config",
    "pkg_catalogue",
    "pkg_install",
    "pkg_remove",
    "default_app",
    "phone_dial",
    "phone_answer",
    "phone_hangup",
    "phone_dtmf",
    "phone_mute",
    "phone_speaker",
    "sms_send",
    "sms_list",
    "sms_delete",
    "sms_read",
    "contact_list",
    "contact_save",
    "contact_delete",
    "contact_import",
    "call_log",
    "call_log_clear",
    "notifications",
    "notification_mark_read",
    "notifications_mark_all_read",
    "notification_clear",
    "notifications_clear_all",
];

/// Commands without arguments.
const NO_ARG_COMMANDS: &[&str] = &[
    "get_status",
    "wifi_scan",
    "wifi_list_saved",
    "wifi_disconnect",
    "windows",
    "home",
    "back",
    "restart_shell",
    "screenshot",
    "mobile_info",
    "volume_test",
    "lock_now",
    "pkg_catalogue",
    "vpn_confirm",
    "vpn_files",
    "phone_answer",
    "phone_hangup",
    "sms_list",
    "contact_list",
    "call_log",
    "call_log_clear",
    "notifications",
    "notifications_mark_all_read",
    "notifications_clear_all",
];

/// Roles of the default apps (`default_app`; /etc/walpad/defaults.json).
pub const DEFAULT_ROLES: &[&str] = &[
    "browser", "files", "terminal", "video", "music", "image", "pdf",
];

/// Settings accepted by `set_setting` (section 9 of the plan).
pub const ALLOWED_SETTINGS: &[&str] = &[
    "auto_rotate",
    "rotate_flip",
    "dim_after_s",
    "dim_level",
    "off_after_s",
    "lid_off",
    "low_batt_mv",
    "critical_batt_mv",
    "clock_24h",
    "theme",
    "wallpaper",
    "wallpaper_path",
    "pinned_apps",
    "touch_gestures",
    "multitouch",
    "key.vol_up.single",
    "key.vol_up.double",
    "key.vol_up.triple",
    "key.vol_up.quad",
    "key.vol_up.long",
    "key.vol_down.single",
    "key.vol_down.double",
    "key.vol_down.triple",
    "key.vol_down.quad",
    "key.vol_down.long",
    "key.both.press",
    "net_ip_mode",
    "net_ip",
    "net_mask",
    "net_gateway",
    "net_mac_mode",
    "net_mac",
    "net_hostname",
    "charge_mode",
    "charge_limit_pct",
    "charge_current_ma",
];

/// Safe hardware volume-button actions selectable in Settings > Input (`keys.json`).
pub const KEY_ACTIONS: &[&str] = &[
    "volume:+5",
    "volume:-5",
    "key:Up",
    "key:Down",
    "key:Prior",
    "key:Next",
    "ctl:brightness +40",
    "ctl:brightness -40",
    "ctl:rotate toggle",
    "home",
    "back",
    "keyboard",
    "powermenu",
    "screenshot",
    "launch:browser",
    "launch:terminal",
    "none",
];

#[derive(Debug, Clone, Deserialize)]
pub struct RawRequest {
    pub v: u32,
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Status,
    Wifi,
    Apps,
    Notice,
    Input,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenMode {
    On,
    Off,
    Toggle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotateMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "lock")]
    Lock,
    #[serde(rename = "toggle")]
    Toggle,
    #[serde(rename = "0")]
    R0,
    #[serde(rename = "90")]
    R90,
    #[serde(rename = "180")]
    R180,
    #[serde(rename = "270")]
    R270,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardMode {
    Show,
    Hide,
    Toggle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Poweroff,
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessArgs {
    #[serde(default)]
    pub value: Option<i32>,
    #[serde(default)]
    pub delta: Option<i32>,
}

/// Optional argument of `launch`: one file to open.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LaunchArgs {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", content = "args", rename_all = "snake_case")]
pub enum Command {
    Hello {
        client: String,
        proto: u32,
    },
    Subscribe {
        topics: Vec<Topic>,
    },
    GetStatus,
    Screen {
        mode: ScreenMode,
    },
    SetBrightness(BrightnessArgs),
    Rotate {
        mode: RotateMode,
    },
    WifiScan,
    WifiListSaved,
    /// `ssid` is the SSID bytes as hex.
    WifiConnect {
        #[serde(default)]
        ssid: Option<String>,
        #[serde(default)]
        secret: Option<Secret>,
        #[serde(default)]
        saved_id: Option<u32>,
    },
    WifiDisconnect,
    WifiForget {
        saved_id: u32,
    },
    /// Wi-Fi client on/off at the network level (owner request 2026-11-10). Off stops the
    /// supplicant and takes wlan0 down; the chip stays powered (a power cycle latches it).
    WifiEnable {
        on: bool,
    },
    /// Hotspot on/off (owner request 2026-11-10): runs /usr/local/sbin/walpad-hotspot.
    Hotspot {
        on: bool,
    },
    /// Hotspot name (UTF-8, 1-32 bytes) and/or WPA2 passphrase (8-63 printable ASCII). Stored
    /// mode 0600; the passphrase is never logged, never in argv, never reported back.
    HotspotConfig {
        #[serde(default)]
        ssid: Option<String>,
        #[serde(default)]
        secret: Option<Secret>,
    },
    /// GPS on/off (owner request 2026-11-11): runs /usr/local/sbin/walpad-gps (mnld + walpad-gnss).
    /// Persisted in /var/lib/walpad/gps-enabled.
    Gps {
        on: bool,
    },
    /// Which header stats are shown (owner request 2026-11-11): any subset of the four switches;
    /// the rest keep their value. Persisted in /var/lib/walpad/statusbar.json.
    StatsConfig {
        #[serde(default)]
        cpu: Option<bool>,
        #[serde(default)]
        ram: Option<bool>,
        #[serde(default)]
        rom: Option<bool>,
        #[serde(default)]
        net: Option<bool>,
    },
    /// Mobile radio on/off (owner request 2026-11-10): AT+CFUN=1/0 over the modem's AT channel;
    /// off is the airplane mode. Persisted in /var/lib/walpad/mobile-radio.
    MobileRadio {
        on: bool,
    },
    /// Mobile data on/off: runs `/usr/local/sbin/walpad-modem data on|off` (fixed path, only the
    /// boolean comes from the client).
    MobileData {
        on: bool,
    },
    /// APN for mobile data (1-62 characters [A-Za-z0-9.-_]); stored in /etc/walpad/mobile.conf.
    MobileApn {
        apn: String,
    },
    /// SIM PIN (4-8 digits). Secret: never logged, never in argv; written to the AT channel only.
    SimPin {
        pin: Secret,
    },
    /// Modem identity on request (IMEI, ICCID, firmware): never in the status topic.
    MobileInfo,
    /// Bluetooth switch (owner request 2026-11-10): persists /var/lib/walpad/bluetooth-enabled and
    /// runs `walpad-bluetooth start|stop`; on also powers the controller.
    BluetoothEnable {
        on: bool,
    },
    /// Discovery on/off; bounded: stops by itself after 30 s.
    BluetoothScan {
        on: bool,
    },
    /// Pair (then trust and connect) a device; `address` is `AA:BB:CC:DD:EE:FF`.
    BluetoothPair {
        address: String,
    },
    BluetoothConnect {
        address: String,
    },
    BluetoothDisconnect {
        address: String,
    },
    /// Unpair and remove the device.
    BluetoothForget {
        address: String,
    },
    /// Answer a pairing passkey confirmation (`notice` kind `bluetooth_pair`, state `confirm`).
    BluetoothConfirm {
        address: String,
        accept: bool,
    },
    /// VPN on/off (owner request 2026-11-10): runs /usr/local/sbin/walpad-vpn with the profile
    /// selected by `vpn_config`.
    Vpn {
        on: bool,
    },
    /// Selected profile, kill switch, DNS servers (empty list clears) and HTTP proxy (empty string
    /// clears). Profile and kill switch cannot change while the tunnel is up.
    VpnConfig {
        #[serde(default)]
        profile: Option<String>,
        #[serde(default)]
        kill_switch: Option<bool>,
        #[serde(default)]
        dns: Option<Vec<String>>,
        #[serde(default)]
        proxy: Option<String>,
    },
    /// Keep the kill switch after turning the VPN on (otherwise it reverts after the timeout).
    VpnConfirm,
    /// Import a WireGuard `.conf` or OpenVPN `.ovpn` file from a user folder into the 0600 store.
    VpnImport {
        path: String,
        #[serde(default)]
        name: Option<String>,
    },
    VpnRemove {
        name: String,
    },
    /// Importable files found in the user folders (names and paths, never content).
    VpnFiles,
    /// Start an app by id. `args.path` (media viewer, change media-apps-capture) opens one file in
    /// an app that declares `"open": true`; the core re-validates the path (allowed roots,
    /// no symlink escape) and builds the argv itself.
    Launch {
        app: String,
        #[serde(default)]
        args: Option<LaunchArgs>,
    },
    /// Stop an app that the core started.
    CloseApp {
        app: String,
    },
    /// Start one more instance of an app with `max_instances` > 1 (the terminal); owner request
    /// 2026-11-11, change multitasking-windows.
    NewInstance {
        app: String,
    },
    /// List the app windows on screen (id, app, title, geometry, focused).
    Windows,
    FocusWindow {
        window: u32,
    },
    /// Close one window (WM_DELETE_WINDOW, SIGTERM of its pid after the grace period).
    CloseWindow {
        window: u32,
    },
    /// Move/resize one window; the core clamps it to the work area and the minimum size.
    WindowRect {
        window: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    },
    /// Put one window in a half or the whole work area.
    Snap {
        window: u32,
        zone: crate::wm::SnapZone,
    },
    /// Read (no argument) or change the window layout: mode, split ratio (33, 50, 67), `swap`
    /// exchanges the two tiles.
    Layout {
        #[serde(default)]
        mode: Option<crate::wm::LayoutMode>,
        #[serde(default)]
        ratio: Option<u8>,
        #[serde(default)]
        swap: bool,
    },
    Home,
    /// The Back control: Alt+Left to the focused window of the browser or the file manager, Esc
    /// to any other (answers `{"sent": "alt+Left"|"Escape"|null}`).
    Back,
    Keyboard {
        mode: KeyboardMode,
    },
    Power {
        action: PowerAction,
        #[serde(default)]
        confirmed: bool,
    },
    RestartShell,
    Screenshot,
    SetSetting {
        key: String,
        value: Value,
    },
    /// Set the system clock manually (`YYYY-MM-DD HH:MM` or `YYYY-MM-DD HH:MM:SS` in local time)
    /// and persist to the hardware RTC.
    SetTime {
        datetime: String,
    },
    /// Open OpenStreetMap in the browser (at the given WGS84 coordinates, or the map home).
    OpenMap {
        #[serde(default)]
        lat: Option<f64>,
        #[serde(default)]
        lon: Option<f64>,
    },
    /// The shell tells the core its power menu opened or closed (the core routes the volume and
    /// power keys to the shell while it is open). `open: true` from a client also opens it.
    PowerMenu {
        open: bool,
    },
    /// Output volume 0-100 % (owner request 2026-11-10): software volume of the sound server,
    /// the codec's gain only when no sound server runs. Persisted by the core.
    VolumeSet {
        percent: i32,
    },
    /// Relative volume change, -100..100 percentage points (clamped to 0-100 in the result).
    VolumeDelta {
        delta: i32,
    },
    /// Mute / unmute (the volume is kept).
    Mute {
        on: bool,
    },
    /// Play the 1 s test tone through the default sink (`walpad-volume test`, bounded).
    VolumeTest,
    /// Headphone jack route / mic source override (owner request 2026-11-12, change
    /// `headphone-mic-jack`). `None` clears the override (back to speaker + built-in mic): this
    /// kernel build has no live jack detection, so this is the only way to switch route until a
    /// future change loads the accdet module (docs/45-headphone-mic.md).
    JackForce {
        #[serde(default)]
        route: Option<JackKind>,
    },
    /// Set or change the screen lock (owner request 2026-11-10, `lock-screen`). `current` is the
    /// present secret and is required when a lock is already set. Secrets are never logged, never
    /// in argv; the core stores only a salted PBKDF2 hash.
    LockSet {
        kind: LockKind,
        secret: Secret,
        #[serde(default)]
        current: Option<Secret>,
    },
    /// Remove the screen lock; needs the current secret.
    LockRemove {
        current: Secret,
    },
    /// Try to unlock. Answers `{unlocked, retry_in_s}`; a wrong secret is not a protocol error.
    Unlock {
        secret: Secret,
    },
    /// Lock at once.
    LockNow,
    /// Auto-lock delay in seconds after the screen has been off (0 = at once).
    LockConfig {
        auto_lock_s: u32,
    },
    /// App store (change `app-catalogue-manager`): the shipped catalogue with install state, free
    /// space and the running job. Never takes arguments.
    PkgCatalogue,
    /// Install a catalogue entry by id (never a package name): runs /usr/local/sbin/walpad-pkg.
    PkgInstall {
        id: String,
    },
    /// Uninstall a catalogue entry by id (never a system package).
    PkgRemove {
        id: String,
    },
    /// Choose the app that opens a role's files (`role` from `DEFAULT_ROLES`, `app` a catalogue id).
    DefaultApp {
        role: String,
        app: String,
    },
    /// Place a voice call (owner request 2026-11-11, `phone-dialer`): `ATD<number>;` over the modem's
    /// AT channel. `number` is 1-32 characters of `0-9 + * # ,` (emergency numbers included).
    /// Private: never logged.
    PhoneDial {
        number: Secret,
    },
    /// Answer the ringing call (`ATA`).
    PhoneAnswer,
    /// End the call, or reject the ringing one (`AT+CHUP`).
    PhoneHangup,
    /// Send one DTMF tone during a call (`AT+VTS`): `0-9 * #`.
    PhoneDtmf {
        digit: String,
    },
    /// Mute the microphone during a call (`AT+CMUT`).
    PhoneMute {
        on: bool,
    },
    /// Speaker (true) or earpiece route of the call audio (experimental: `walpad-callaudio`).
    PhoneSpeaker {
        on: bool,
    },
    /// Send a text message (SMS, PDU mode, GSM 7-bit or UCS2 for Bangla). Private: never logged.
    SmsSend {
        number: Secret,
        text: Secret,
    },
    /// All stored messages (oldest first), answers `{messages: [...], unread}`.
    SmsList,
    /// Delete one stored message by id.
    SmsDelete {
        id: u64,
    },
    /// Mark every message of the conversation with `number` as read.
    SmsRead {
        number: Secret,
    },
    /// All contacts, sorted by name.
    ContactList,
    /// Create (no `id`) or change a contact: a name and 1-5 numbers. Answers `{id}`.
    ContactSave {
        #[serde(default)]
        id: Option<String>,
        name: String,
        numbers: Vec<Secret>,
    },
    ContactDelete {
        id: String,
    },
    /// Import a vCard (.vcf) file from a user folder or removable media.
    ContactImport {
        path: String,
    },
    /// The call log (oldest first).
    CallLog,
    CallLogClear,
    /// Notification centre (owner request 2026-11-12, change `device-notifications`): the bounded
    /// history, newest first.
    Notifications,
    NotificationMarkRead {
        id: u64,
    },
    NotificationsMarkAllRead,
    NotificationClear {
        id: u64,
    },
    NotificationsClearAll,
}

/// Hardware-key events published on the `input` topic for the shell (never touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShellInput {
    /// Long power press: open the power menu.
    PowerMenuOpen,
    VolumeUp,
    VolumeDown,
    /// Short power press while the power menu is open: activate the focused item.
    PowerShort,
    /// Show / hide the shell's system keyboard (the `keyboard` command, the `keyboard` key
    /// action, Home). The core resolves `toggle` against the state the shell last reported.
    KeyboardShow,
    KeyboardHide,
}

impl Command {
    /// Build from an envelope, distinguishing unknown commands from bad arguments.
    pub fn from_raw(raw: &RawRequest) -> Result<Command, (ErrorCode, String)> {
        if !COMMAND_NAMES.contains(&raw.cmd.as_str()) {
            return Err((
                ErrorCode::UnknownCommand,
                format!("unknown command {:?}", raw.cmd),
            ));
        }
        let mut obj = serde_json::Map::new();
        obj.insert("cmd".into(), Value::String(raw.cmd.clone()));
        // commands that take no arguments ignore a stray `args` (unknown fields are ignored)
        if NO_ARG_COMMANDS.contains(&raw.cmd.as_str()) {
            // nothing to insert
        } else if !raw.args.is_null() {
            obj.insert("args".into(), raw.args.clone());
        } else if matches!(raw.cmd.as_str(), "set_brightness" | "layout" | "jack_force") {
            obj.insert("args".into(), Value::Object(Default::default()));
        }
        let cmd: Command = serde_json::from_value(Value::Object(obj))
            .map_err(|e| (ErrorCode::InvalidArgs, format!("bad args: {e}")))?;
        cmd.validate().map_err(|m| (ErrorCode::InvalidArgs, m))?;
        Ok(cmd)
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Command::Hello { client, .. } if client.is_empty() || client.len() > 64 => {
                Err("client must be 1-64 chars".into())
            }
            Command::SetBrightness(a) => match (a.value, a.delta) {
                (Some(v), None) if (0..=255).contains(&v) => Ok(()),
                (None, Some(d)) if (-255..=255).contains(&d) => Ok(()),
                (Some(_), None) => Err("value must be 0-255".into()),
                (None, Some(_)) => Err("delta must be -255..255".into()),
                _ => Err("give exactly one of value or delta".into()),
            },
            Command::WifiConnect {
                ssid,
                secret,
                saved_id,
            } => {
                match (ssid, saved_id) {
                    (Some(h), None) => {
                        if h.is_empty()
                            || h.len() > 64
                            || h.len() % 2 != 0
                            || !h.bytes().all(|b| b.is_ascii_hexdigit())
                        {
                            return Err("ssid must be 1-32 bytes as hex".into());
                        }
                    }
                    (None, Some(_)) => {}
                    _ => return Err("give exactly one of ssid or saved_id".into()),
                }
                if let Some(s) = secret {
                    let n = s.expose().len();
                    if !(8..=63).contains(&n) && n != 64 {
                        return Err("secret length is not valid".into());
                    }
                }
                Ok(())
            }
            Command::HotspotConfig { ssid, secret } => {
                if ssid.is_none() && secret.is_none() {
                    return Err("give ssid and/or secret".into());
                }
                if ssid.as_deref().is_some_and(|n| !valid_hotspot_ssid(n)) {
                    return Err("ssid must be 1-32 bytes without control characters".into());
                }
                if secret
                    .as_ref()
                    .is_some_and(|s| !valid_hotspot_secret(s.expose()))
                {
                    return Err("secret must be 8-63 printable ASCII characters".into());
                }
                Ok(())
            }
            Command::BluetoothPair { address }
            | Command::BluetoothConnect { address }
            | Command::BluetoothDisconnect { address }
            | Command::BluetoothForget { address }
            | Command::BluetoothConfirm { address, .. }
                if !valid_bt_address(address) =>
            {
                Err("address must be like AA:BB:CC:DD:EE:FF".into())
            }
            Command::VolumeSet { percent } if !(0..=100).contains(percent) => {
                Err("percent must be 0-100".into())
            }
            Command::VolumeDelta { delta } if !(-100..=100).contains(delta) => {
                Err("delta must be -100..100".into())
            }
            Command::VpnConfig {
                profile,
                kill_switch,
                dns,
                proxy,
            } => {
                if profile.is_none() && kill_switch.is_none() && dns.is_none() && proxy.is_none() {
                    return Err("give profile, kill_switch, dns and/or proxy".into());
                }
                if profile.as_deref().is_some_and(|n| !valid_vpn_name(n)) {
                    return Err("profile must be 1-24 characters of A-Z a-z 0-9 . - _".into());
                }
                if let Some(d) = dns {
                    if d.len() > 3 || d.iter().any(|s| !valid_dns_server(s)) {
                        return Err("dns must be up to 3 IPv4 or IPv6 addresses".into());
                    }
                }
                if proxy
                    .as_deref()
                    .is_some_and(|p| !p.is_empty() && !valid_proxy(p))
                {
                    return Err("proxy must look like http://host:port".into());
                }
                Ok(())
            }
            Command::VpnImport { path, name } => {
                if !valid_vpn_import_path(path) {
                    return Err("path must be a .conf or .ovpn file in a user folder".into());
                }
                if name.as_deref().is_some_and(|n| !valid_vpn_name(n)) {
                    return Err("name must be 1-24 characters of A-Z a-z 0-9 . - _".into());
                }
                Ok(())
            }
            Command::VpnRemove { name } if !valid_vpn_name(name) => {
                Err("name must be 1-24 characters of A-Z a-z 0-9 . - _".into())
            }
            Command::MobileApn { apn } if !valid_apn(apn) => {
                Err("apn must be 1-62 characters of A-Z a-z 0-9 . - _".into())
            }
            Command::SimPin { pin } if !valid_sim_pin(pin.expose()) => {
                Err("pin must be 4-8 digits".into())
            }
            Command::Launch { app, args } => {
                if !valid_app_id(app) {
                    Err("app must be an id like \"browser\"".into())
                } else if let Some(a) = args {
                    if valid_open_path(&a.path) {
                        Ok(())
                    } else {
                        Err("path must be an absolute file path in /root, /mnt, /home or /tmp/Downloads".into())
                    }
                } else {
                    Ok(())
                }
            }
            Command::CloseApp { app } => {
                if valid_app_id(app) {
                    Ok(())
                } else {
                    Err("app must be an id like \"browser\"".into())
                }
            }
            Command::NewInstance { app } if !valid_app_id(app) => {
                Err("app must be an id like \"terminal\"".into())
            }
            Command::FocusWindow { window }
            | Command::CloseWindow { window }
            | Command::WindowRect { window, .. }
            | Command::Snap { window, .. }
                if *window == 0 =>
            {
                Err("window must be a window id from `windows`".into())
            }
            Command::WindowRect { w, h, x, y, .. }
                if !(1..=8192).contains(w)
                    || !(1..=8192).contains(h)
                    || !(-8192..=8192).contains(x)
                    || !(-8192..=8192).contains(y) =>
            {
                Err("x, y, w, h must be within 8192".into())
            }
            Command::Layout { ratio: Some(r), .. } if !crate::wm::valid_ratio(*r) => {
                Err("ratio must be 33, 50 or 67".into())
            }
            Command::PkgInstall { id } | Command::PkgRemove { id } => {
                if valid_app_id(id) && !id.starts_with(['-', '_']) {
                    Ok(())
                } else {
                    Err("id must be a catalogue id like \"mousepad\"".into())
                }
            }
            Command::DefaultApp { role, app } => {
                if !DEFAULT_ROLES.contains(&role.as_str()) {
                    Err("role is not one of the default app roles".into())
                } else if !valid_app_id(app) || app.starts_with(['-', '_']) {
                    Err("app must be a catalogue id like \"evince\"".into())
                } else {
                    Ok(())
                }
            }
            Command::PhoneDial { number } if !valid_dial_number(number.expose()) => {
                Err("number must be 1-32 characters of 0-9 + * # ,".into())
            }
            Command::PhoneDtmf { digit } if !valid_dtmf(digit) => {
                Err("digit must be one of 0-9 * #".into())
            }
            Command::SmsSend { number, text } => {
                if !valid_sms_number(number.expose()) {
                    Err("number must be 3-20 digits, optionally starting with +".into())
                } else if !valid_sms_text(text.expose()) {
                    Err("text must be 1-1000 characters without control characters".into())
                } else {
                    Ok(())
                }
            }
            Command::SmsRead { number } if !valid_sms_number(number.expose()) => {
                Err("number must be 3-20 digits, optionally starting with +".into())
            }
            Command::ContactSave { id, name, numbers } => {
                if id.as_deref().is_some_and(|i| !valid_store_id(i)) {
                    Err("id is not valid".into())
                } else if !valid_contact_name(name) {
                    Err("name must be 1-64 characters without control characters".into())
                } else if numbers.is_empty()
                    || numbers.len() > 5
                    || numbers.iter().any(|n| !valid_sms_number(n.expose()))
                {
                    Err("give 1-5 numbers of 3-20 digits, optionally starting with +".into())
                } else {
                    Ok(())
                }
            }
            Command::ContactDelete { id } if !valid_store_id(id) => Err("id is not valid".into()),
            Command::ContactImport { path } if !valid_vcard_path(path) => {
                Err("path must be a .vcf file in a user folder".into())
            }
            Command::SetSetting { key, value } => validate_setting(key, value),
            Command::SetTime { datetime } if !valid_datetime(datetime) => {
                Err("datetime must be YYYY-MM-DD HH:MM or YYYY-MM-DD HH:MM:SS".into())
            }
            Command::OpenMap { lat, lon }
                if lat.is_some() != lon.is_some()
                    || lat.is_some_and(|v| !v.is_finite() || !(-90.0..=90.0).contains(&v))
                    || lon.is_some_and(|v| !v.is_finite() || !(-180.0..=180.0).contains(&v)) =>
            {
                Err("lat must be -90..90 and lon -180..180".into())
            }
            Command::StatsConfig {
                cpu: None,
                ram: None,
                rom: None,
                net: None,
            } => Err("give at least one of cpu, ram, rom, net".into()),
            Command::LockSet {
                kind,
                secret,
                current,
            } => {
                if !valid_lock_secret(*kind, secret.expose()) {
                    return Err(lock_secret_rule(*kind).into());
                }
                if current.as_ref().is_some_and(|c| !valid_attempt(c.expose())) {
                    return Err("current secret is not valid".into());
                }
                Ok(())
            }
            Command::LockRemove { current: s } | Command::Unlock { secret: s }
                if !valid_attempt(s.expose()) =>
            {
                Err("secret must be 1-128 characters".into())
            }
            Command::LockConfig { auto_lock_s } if *auto_lock_s > 3600 => {
                Err("auto_lock_s must be 0-3600".into())
            }
            _ => Ok(()),
        }
    }
}

/// Bluetooth device address: six hex pairs separated by colons (nothing else reaches bluetoothctl).
pub fn valid_bt_address(s: &str) -> bool {
    s.len() == 17
        && s.bytes().enumerate().all(|(i, b)| {
            if i % 3 == 2 {
                b == b':'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

/// Hotspot name: 1-32 bytes of UTF-8 without control characters.
pub fn valid_hotspot_ssid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && !s.chars().any(char::is_control)
}

/// Hotspot WPA2 passphrase: 8-63 printable ASCII characters (what hostapd's wpa_passphrase takes).
pub fn valid_hotspot_secret(s: &str) -> bool {
    (8..=63).contains(&s.len()) && s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// VPN profile name: 1-24 of A-Z a-z 0-9 . - _, not starting with a dot (it is a file name).
pub fn valid_vpn_name(s: &str) -> bool {
    (1..=24).contains(&s.len())
        && !s.starts_with('.')
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// A DNS server: an IPv4 or IPv6 address literal (no names: resolv.conf takes addresses).
pub fn valid_dns_server(s: &str) -> bool {
    s.parse::<std::net::IpAddr>().is_ok()
}

/// HTTP proxy `http://host:port`: no credentials (they would sit in a file and in every app's
/// environment), host of letters, digits, `.`, `-` or an IPv6 literal in brackets, port 1-65535.
pub fn valid_proxy(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("http://") else {
        return false;
    };
    let Some((host, port)) = rest.rsplit_once(':') else {
        return false;
    };
    let host_ok = if let Some(v6) = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')) {
        v6.parse::<std::net::Ipv6Addr>().is_ok()
    } else {
        !host.is_empty()
            && host.len() <= 253
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    };
    host_ok && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok_and(|p| p > 0)
}

/// Where `vpn_import` may read from: user folders and removable media, a `.conf` or `.ovpn` file,
/// no `..` component.
pub fn valid_vpn_import_path(s: &str) -> bool {
    const ROOTS: &[&str] = &[
        "/root/",
        "/home/",
        "/media/",
        "/mnt/",
        "/tmp/",
        "/var/lib/walpad/import/",
    ];
    s.len() <= 255
        && ROOTS.iter().any(|r| s.starts_with(r))
        && (s.ends_with(".conf") || s.ends_with(".ovpn"))
        && !s.split('/').any(|c| c == "..")
        && !s.chars().any(char::is_control)
}

/// Folders a `launch` file argument may come from (checked again by the core after resolving
/// symlinks).
pub const OPEN_ROOTS: &[&str] = &["/root", "/mnt", "/home", "/tmp/Downloads"];

/// Syntax of the `launch` file argument: absolute, inside an allowed root, no `.`/`..`/empty/hidden
/// component, no control characters or NUL, never option-looking, at most 1024 bytes. The core
/// additionally resolves symlinks (`apps::resolve_open_path`).
pub fn valid_open_path(s: &str) -> bool {
    if s.is_empty() || s.len() > 1024 || s.starts_with('-') || s.chars().any(char::is_control) {
        return false;
    }
    let Some(rest) = s.strip_prefix('/') else {
        return false;
    };
    if !rest
        .split('/')
        .all(|c| !c.is_empty() && c != "." && c != ".." && !c.starts_with(['.', '-']))
    {
        return false;
    }
    OPEN_ROOTS
        .iter()
        .any(|r| s.strip_prefix(r).is_some_and(|t| t.starts_with('/')))
}

/// Access point name: 1-62 characters of A-Z a-z 0-9 . - _ (safe in an AT command and a file).
pub fn valid_apn(s: &str) -> bool {
    (1..=62).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// Do two numbers name the same party? `01711234567` and `+8801711234567` do: one is the suffix
/// of the other (digits only), with at least 7 digits in common; short numbers match exactly.
pub fn same_number(a: &str, b: &str) -> bool {
    let digits = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
    let (a, b) = (digits(a), digits(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (long, short) = if a.len() >= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    short.len() >= 7 && long.ends_with(short.as_str())
}

/// A number to dial: 1-32 of `0-9 + * # ,` (a `,` is a pause). Nothing else reaches the modem.
pub fn valid_dial_number(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'*' | b'#' | b','))
}

/// One DTMF digit: 0-9, `*` or `#`.
pub fn valid_dtmf(s: &str) -> bool {
    s.len() == 1
        && s.bytes()
            .all(|b| b.is_ascii_digit() || b == b'*' || b == b'#')
}

/// An SMS destination or a contact number: 3-20 digits, an optional leading `+`.
pub fn valid_sms_number(s: &str) -> bool {
    let d = s.strip_prefix('+').unwrap_or(s);
    (3..=20).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit())
}

/// Message text: 1-1000 characters, no control characters except line breaks.
pub fn valid_sms_text(s: &str) -> bool {
    let n = s.chars().count();
    (1..=1000).contains(&n) && !s.chars().any(|c| c.is_control() && c != '\n')
}

pub fn valid_contact_name(s: &str) -> bool {
    (1..=64).contains(&s.chars().count()) && !s.chars().any(char::is_control)
}

/// Ids of stored contacts: 1-24 of `a-z 0-9 -`.
pub fn valid_store_id(s: &str) -> bool {
    (1..=24).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Where `contact_import` may read from: user folders and removable media, a `.vcf` file.
pub fn valid_vcard_path(s: &str) -> bool {
    const ROOTS: &[&str] = &["/root/", "/home/", "/media/", "/mnt/", "/tmp/"];
    s.len() <= 255
        && ROOTS.iter().any(|r| s.starts_with(r))
        && s.to_ascii_lowercase().ends_with(".vcf")
        && !s.split('/').any(|c| c == "..")
        && !s.chars().any(char::is_control)
}

/// SIM PIN: 4-8 ASCII digits.
pub fn valid_sim_pin(s: &str) -> bool {
    (4..=8).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

/// Rule text for a lock secret kind (shown to the user as is).
pub fn lock_secret_rule(kind: LockKind) -> &'static str {
    match kind {
        LockKind::Pin => "PIN must be 4-12 digits",
        LockKind::Password => "password must be 4-64 printable characters",
        LockKind::Pattern => "pattern must connect 4-9 different dots",
    }
}

/// A new lock secret: PIN 4-12 digits, password 4-64 printable characters (no control
/// characters), pattern 4-9 different dots of the 3x3 grid written as the digits 0-8.
pub fn valid_lock_secret(kind: LockKind, s: &str) -> bool {
    match kind {
        LockKind::Pin => (4..=12).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit()),
        LockKind::Password => {
            (4..=64).contains(&s.chars().count()) && !s.chars().any(char::is_control)
        }
        LockKind::Pattern => {
            (4..=9).contains(&s.len())
                && s.bytes().all(|b| (b'0'..=b'8').contains(&b))
                && s.bytes()
                    .enumerate()
                    .all(|(i, b)| !s.as_bytes()[..i].contains(&b))
        }
    }
}

/// A secret offered for verification: any 1-128 bytes (the stored kind decides what it can match).
pub fn valid_attempt(s: &str) -> bool {
    (1..=128).contains(&s.len())
}

/// App ids are short lower-case slugs ("browser", "system-monitor").
pub fn valid_app_id(app: &str) -> bool {
    !app.is_empty()
        && app.len() <= 32
        && app
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Device hostname: 1-63 characters of A-Z a-z 0-9 -, not starting or ending with `-`.
pub fn valid_hostname(s: &str) -> bool {
    (1..=63).contains(&s.len())
        && !s.starts_with('-')
        && !s.ends_with('-')
        && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn validate_setting(key: &str, v: &Value) -> Result<(), String> {
    if !ALLOWED_SETTINGS.contains(&key) {
        return Err(format!("setting {key:?} is not allowed"));
    }
    match key {
        "pinned_apps" => match v.as_array() {
            Some(a) if a.len() <= 8 && a.iter().all(|x| x.as_str().is_some_and(valid_app_id)) => {
                Ok(())
            }
            _ => Err("value must be a list of at most 8 app ids".into()),
        },
        "auto_rotate" | "rotate_flip" | "lid_off" | "clock_24h" | "touch_gestures"
        | "multitouch" => v
            .is_boolean()
            .then_some(())
            .ok_or_else(|| "value must be a boolean".to_string()),
        "dim_level" => match v.as_u64() {
            Some(n) if (1..=255).contains(&n) => Ok(()),
            _ => Err("value must be an integer 1-255".into()),
        },
        "low_batt_mv" => match v.as_u64() {
            Some(n) if (3300..=4000).contains(&n) => Ok(()),
            _ => Err("value must be an integer 3300-4000".into()),
        },
        "critical_batt_mv" => match v.as_u64() {
            Some(n) if (3100..=3800).contains(&n) => Ok(()),
            _ => Err("value must be an integer 3100-3800".into()),
        },
        "net_ip_mode" => match v.as_str() {
            Some("dhcp" | "manual") => Ok(()),
            _ => Err("value must be \"dhcp\" or \"manual\"".into()),
        },
        "net_ip" | "net_gateway" => match v.as_str() {
            Some(s) if s.is_empty() || valid_dns_server(s) => Ok(()),
            _ => Err("value must be an IPv4/IPv6 address or empty".into()),
        },
        "net_mask" => match v.as_str() {
            Some("255.255.255.0" | "255.255.0.0" | "255.255.255.128" | "255.0.0.0") => Ok(()),
            _ => Err("value must be an allowed subnet mask".into()),
        },
        "net_mac_mode" => match v.as_str() {
            Some("hardware" | "random" | "custom") => Ok(()),
            _ => Err("value must be \"hardware\", \"random\" or \"custom\"".into()),
        },
        "net_mac" => match v.as_str() {
            Some(s) if s.is_empty() || valid_bt_address(s) => Ok(()),
            _ => Err("value must be AA:BB:CC:DD:EE:FF or empty".into()),
        },
        "net_hostname" => match v.as_str() {
            Some(s) if s.is_empty() || valid_hostname(s) => Ok(()),
            _ => Err("value must be a hostname of 1-63 letters, digits or -".into()),
        },
        "theme" => match v.as_str() {
            Some("dark" | "light") => Ok(()),
            _ => Err("value must be \"dark\" or \"light\"".into()),
        },
        "wallpaper" => match v.as_str() {
            Some("default" | "live_coffee" | "live_aurora" | "live_stars" | "custom") => Ok(()),
            _ => Err(
                "value must be \"default\", \"live_coffee\", \"live_aurora\", \"live_stars\", or \"custom\""
                    .into(),
            ),
        },
        "wallpaper_path" => match v.as_str() {
            Some("") => Ok(()),
            Some(s) if s.len() <= 512 && s.starts_with('/') && !s.contains("..") => Ok(()),
            _ => Err("value must be empty or an absolute path without '..'".into()),
        },
        "charge_mode" => match v.as_str() {
            Some("smart" | "bypass" | "eco" | "fast") => Ok(()),
            _ => Err("value must be \"smart\", \"bypass\", \"eco\" or \"fast\"".into()),
        },
        "charge_limit_pct" => match v.as_u64() {
            Some(n) if (50..=100).contains(&n) => Ok(()),
            _ => Err("value must be an integer 50-100".into()),
        },
        "charge_current_ma" => match v.as_u64() {
            Some(500 | 1000 | 1500 | 2000) => Ok(()),
            _ => Err("value must be 500, 1000, 1500 or 2000".into()),
        },
        k if k.starts_with("key.") => match v.as_str() {
            Some(s) if KEY_ACTIONS.contains(&s) => Ok(()),
            _ => Err("value must be an allowed key action".into()),
        },
        _ => match v.as_u64() {
            Some(n) if n <= 3600 => Ok(()),
            _ => Err("value must be an integer 0-3600".into()),
        },
    }
}

/// Validate a manual local date & time string (`YYYY-MM-DD HH:MM` or `YYYY-MM-DD HH:MM:SS`).
pub fn valid_datetime(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 16 && b.len() != 19 {
        return false;
    }
    if b[4] != b'-' || b[7] != b'-' || b[10] != b' ' || b[13] != b':' {
        return false;
    }
    if b.len() == 19 && b[16] != b':' {
        return false;
    }
    let parse_u = |from: usize, to: usize| -> Option<u32> {
        let slice = s.get(from..to)?;
        if !slice.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        slice.parse().ok()
    };
    let (Some(y), Some(m), Some(d), Some(hh), Some(mm)) = (
        parse_u(0, 4),
        parse_u(5, 7),
        parse_u(8, 10),
        parse_u(11, 13),
        parse_u(14, 16),
    ) else {
        return false;
    };
    let ss = if b.len() == 19 {
        match parse_u(17, 19) {
            Some(v) => v,
            None => return false,
        }
    } else {
        0
    };
    if !(2000..=2099).contains(&y) || !(1..=12).contains(&m) || hh > 23 || mm > 59 || ss > 59 {
        return false;
    }
    let leap = y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    let max_d = match m {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    (1..=max_d).contains(&d)
}
