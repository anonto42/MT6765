//! Wire commands, request envelope, and field validation for the IPC protocol.
mod envelope;
mod names;
mod settings;
mod validate;
mod validators;

pub use envelope::{
    BrightnessArgs, KeyboardMode, LaunchArgs, PowerAction, RawRequest, RotateMode, ScreenMode,
    ShellInput, Topic,
};
pub use names::{ALLOWED_SETTINGS, COMMAND_NAMES, DEFAULT_ROLES, KEY_ACTIONS};
pub use settings::valid_datetime;
pub use validators::{
    lock_secret_rule, same_number, valid_apn, valid_app_id, valid_attempt, valid_bt_address,
    valid_contact_name, valid_dial_number, valid_dns_server, valid_dtmf, valid_hostname,
    valid_hotspot_secret, valid_hotspot_ssid, valid_lock_secret, valid_open_path, valid_proxy,
    valid_sim_pin, valid_sms_number, valid_sms_text, valid_store_id, valid_vcard_path,
    valid_vpn_import_path, valid_vpn_name, OPEN_ROOTS,
};

use crate::{JackKind, LockKind, Secret};
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    /// USSD codes (`*121#`, `#...#`) are routed to `AT+CUSD`. Private: never logged.
    PhoneDial {
        number: Secret,
    },
    /// Send a USSD code (`AT+CUSD=1,"<code>",15`). Private: never logged.
    PhoneUssd {
        code: Secret,
    },
    /// Cancel/dismiss the active USSD session (`AT+CUSD=2`).
    PhoneUssdCancel,
    /// Load contacts from the SIM phonebook (`AT+CPBS="SM"` / `AT+CPBR`).
    PhoneSimContacts,
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
