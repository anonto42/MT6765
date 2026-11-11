use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatterySource {
    Gauge,
    VoltageEstimate,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenState {
    On,
    Dim,
    Off,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WifiRadio {
    Down,
    Starting,
    Up,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Battery {
    pub percent: Option<u8>,
    pub source: BatterySource,
    pub state: Option<String>,
    pub mv: Option<u32>,
    pub temp_c: Option<f32>,
    pub low: bool,
    /// Estimated minutes of use left while discharging (owner request 2026-11-11); None = unknown.
    #[serde(default)]
    pub time_left_min: Option<u32>,
    /// Estimated minutes to full while charging; None = unknown.
    #[serde(default)]
    pub time_to_full_min: Option<u32>,
    /// A time estimate is coming but there is not enough current data yet.
    #[serde(default)]
    pub time_calculating: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WifiStatus {
    pub radio: WifiRadio,
    pub state: Option<String>,
    pub ssid: Option<String>,
    pub ip: Option<String>,
    pub signal_dbm: Option<i32>,
    /// The owner's Wi-Fi switch (/var/lib/walpad/wifi-enabled; missing = on).
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotState {
    #[default]
    Off,
    Starting,
    On,
    Stopping,
    Failed,
}

/// Internet sharing of a running hotspot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotInternet {
    /// NAT to the uplink is set up.
    Shared,
    /// No uplink (the Wi-Fi client is off or not connected): local network only.
    NoUplink,
    /// iptables/NAT is not available: local network only.
    Unavailable,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotspotStatus {
    pub state: HotspotState,
    /// Why it failed; never contains the passphrase.
    pub reason: Option<String>,
    /// Configured name (None = never configured).
    pub ssid: Option<String>,
    /// A passphrase is stored (it is never reported).
    pub has_secret: bool,
    pub clients: Option<u32>,
    pub internet: Option<HotspotInternet>,
    /// The Wi-Fi client is held off while the hotspot runs (no concurrent STA+AP).
    pub exclusive: bool,
}

/// GPS receiver state (owner request 2026-11-11).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpsState {
    /// Switched off or not running.
    #[default]
    Off,
    /// `walpad-gps start`/`stop` is running.
    Starting,
    /// `walpad-gps stop` is running.
    Stopping,
    /// Running, no position yet (a cold start can take minutes under open sky).
    Searching,
    /// A recent position.
    Fix,
    /// The start failed (see `reason`).
    Failed,
}

/// GPS status. The position is private: it is shown to the owner and never logged. Coordinates are
/// rounded to 5 decimals (about a metre) and the accuracy to whole metres, for display only.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpsStatus {
    /// The owner's switch (/var/lib/walpad/gps-enabled; missing = off).
    pub enabled: bool,
    pub state: GpsState,
    /// Why it failed or cannot start (e.g. the Wi-Fi stack is down).
    pub reason: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub alt_m: Option<i32>,
    pub accuracy_m: Option<u32>,
    pub satellites_used: Option<u32>,
    pub satellites_view: Option<u32>,
    /// Seconds since the last position (None = none yet).
    pub age_s: Option<u32>,
    /// Signal-to-noise of the satellites in view (dB-Hz, 0 = not tracked), strongest first, at most 16.
    pub signals: Vec<u8>,
}

/// Which live system figures the header shows (owner request 2026-11-11, change `top-bar-stats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatsItems {
    pub cpu: bool,
    pub ram: bool,
    pub rom: bool,
    pub net: bool,
}

impl Default for StatsItems {
    /// CPU and RAM on, storage and network off.
    fn default() -> Self {
        StatsItems {
            cpu: true,
            ram: true,
            rom: false,
            net: false,
        }
    }
}

impl StatsItems {
    pub fn any(&self) -> bool {
        self.cpu || self.ram || self.rom || self.net
    }
}

/// Live system figures (owner request 2026-11-11). A figure is `None` while its item is off, before
/// its first sample, or when /proc or the filesystem could not be read: unavailable, never guessed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct StatsStatus {
    pub items: StatsItems,
    /// Busy share of all CPUs over the last interval, 0-100.
    pub cpu_pct: Option<u8>,
    pub ram_used_kb: Option<u64>,
    pub ram_total_kb: Option<u64>,
    /// The filesystem holding `/` (the userdata root).
    pub rom_used_b: Option<u64>,
    pub rom_total_b: Option<u64>,
    /// Bytes per second over wlan0, ccmni*, rndis0 and usb0 (never lo).
    pub net_down_bps: Option<u64>,
    pub net_up_bps: Option<u64>,
}

/// Bluetooth controller state (owner request 2026-11-10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BluetoothController {
    /// No controller: not supported, switched off or not started (see `reason`).
    #[default]
    Absent,
    /// `walpad-bluetooth start` is running.
    Starting,
    /// bluetoothd answers with a controller.
    Present,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BluetoothDevice {
    /// Upper-case `AA:BB:CC:DD:EE:FF`.
    pub address: String,
    /// Alias or name; the address when the device has none.
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub rssi: Option<i32>,
    /// BlueZ icon name (`input-keyboard`, `audio-headset`, `phone`, ...).
    pub icon: Option<String>,
    /// Class of device.
    pub class: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BluetoothStatus {
    pub controller: BluetoothController,
    /// Why there is no controller; never contains secrets.
    pub reason: Option<String>,
    /// The owner's switch (/var/lib/walpad/bluetooth-enabled; missing = on).
    pub enabled: bool,
    pub powered: bool,
    pub discovering: bool,
    /// A pairing is in progress for this address.
    pub pairing: Option<String>,
    pub devices: Vec<BluetoothDevice>,
}

impl Default for BluetoothStatus {
    fn default() -> Self {
        BluetoothStatus {
            controller: BluetoothController::Absent,
            reason: None,
            enabled: true,
            powered: false,
            discovering: false,
            pairing: None,
            devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VpnState {
    #[default]
    Off,
    Starting,
    On,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VpnKind {
    Wireguard,
    Openvpn,
}

/// An imported profile: the name only (keys and passwords never leave the 0600 file).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VpnProfile {
    pub name: String,
    pub kind: VpnKind,
}

/// VPN tunnel, kill switch, DNS and proxy. Contains no key, password or config content.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VpnStatus {
    pub state: VpnState,
    /// Why it failed or was reverted; never contains secrets.
    pub reason: Option<String>,
    /// The profile selected (or running).
    pub profile: Option<String>,
    pub kind: Option<VpnKind>,
    pub profiles: Vec<VpnProfile>,
    pub kill_switch: bool,
    /// User DNS servers (empty = from the tunnel or the network).
    pub dns: Vec<String>,
    /// HTTP proxy `http://host:port` (None = none).
    pub proxy: Option<String>,
    /// Seconds since the last WireGuard handshake (None: no handshake yet or not WireGuard).
    pub handshake_age_s: Option<u64>,
    pub rx_bytes: Option<u64>,
    pub tx_bytes: Option<u64>,
    /// Kill switch armed and not yet confirmed: seconds until the automatic revert.
    pub revert_in_s: Option<u32>,
}

/// The cellular modem (owner request 2026-11-10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModemState {
    /// No modem support on this system (walpad-modem missing).
    #[default]
    Absent,
    /// walpad-modem exists but the modem is not running (not started yet).
    Present,
    /// `walpad-modem start` is running.
    Booting,
    /// The AT channel answers.
    Ready,
    /// The modem did not come up (reason in `MobileStatus::reason`).
    Failed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimState {
    Ready,
    PinRequired,
    PukRequired,
    Absent,
    Busy,
    #[default]
    Unknown,
}

/// Network registration (best of CREG / CGREG / CEREG).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileReg {
    Home,
    Roaming,
    Searching,
    Denied,
    NotRegistered,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MobileData {
    #[default]
    Off,
    Connecting,
    On,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MobileStatus {
    pub modem: ModemState,
    /// Why the modem failed (never contains an identifier).
    pub reason: Option<String>,
    /// AT+CFUN: true = full functionality; None = not known yet.
    pub radio_on: Option<bool>,
    pub sim: SimState,
    pub operator: Option<String>,
    /// "2G" | "3G" | "4G" from the access technology of AT+COPS.
    pub tech: Option<String>,
    /// 0-4 bars and the dBm estimate from AT+CSQ; None = unknown.
    pub signal_bars: Option<u8>,
    pub signal_dbm: Option<i32>,
    pub registration: MobileReg,
    pub data: MobileData,
    pub data_reason: Option<String>,
    pub ip: Option<String>,
    /// Configured APN (/etc/walpad/mobile.conf); None = chosen by operator.
    pub apn: Option<String>,
    /// APN of the active / last data context.
    pub apn_in_use: Option<String>,
    /// Modem firmware (ATI).
    pub firmware: Option<String>,
}

/// The state of the one voice call (owner request 2026-11-11, `phone-dialer`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    #[default]
    Idle,
    /// Outgoing: dialling or ringing at the other end.
    Dialing,
    /// Incoming and not answered yet.
    Ringing,
    Active,
    Held,
    /// Over; `PhoneStatus::end_reason` says why. Shown for a few seconds, then `Idle`.
    Ended,
}

/// Calls and messages. The number and name of the party are private to the shell: never logged.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhoneStatus {
    /// Calls and messages can be used now (modem ready, SIM ready, radio on).
    pub available: bool,
    /// Why not (a short sentence), when `available` is false.
    pub reason: Option<String>,
    pub call: CallState,
    /// The current call came in (false = outgoing).
    pub incoming: bool,
    pub number: Option<String>,
    /// The contact's name, when the number is in the contacts.
    pub name: Option<String>,
    /// Seconds since the call was answered (0 before).
    pub elapsed_s: u64,
    pub muted: bool,
    pub speaker: bool,
    /// Why the call ended: `hangup`, `remote`, `busy`, `no_answer`, `failed`, `rejected`, `missed`.
    pub end_reason: Option<String>,
    /// Unread text messages.
    pub unread: u32,
    /// Call audio is experimental until verified on the device (see walpad-callaudio).
    pub audio: String,
}

/// Output volume (owner request 2026-11-10). `available` = something can change the volume (the
/// sound server or the codec mixer); until the first probe answers it is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStatus {
    pub volume: u8,
    pub muted: bool,
    pub available: bool,
}

impl Default for AudioStatus {
    fn default() -> Self {
        AudioStatus {
            volume: 60,
            muted: false,
            available: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hardware {
    pub touch: String,
    pub gsensor: String,
    pub audio: String,
    pub bluetooth: String,
    pub sd_card: String,
}

/// What the screen lock is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockKind {
    Pin,
    Password,
    /// 3x3 dot pattern, stored as the visited dots 0-8 (row-major) written as digits.
    Pattern,
}

/// The screen lock (owner request 2026-11-10). Never carries a secret or the hash.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LockStatus {
    /// A PIN / password / pattern is set.
    pub configured: bool,
    pub kind: Option<LockKind>,
    /// The tablet is locked now (the shell shows the lock screen).
    pub locked: bool,
    /// Seconds until attempts are accepted again after too many wrong secrets.
    pub retry_in_s: Option<u32>,
    /// Lock this long after the screen went off (0 = as soon as it turns off).
    pub auto_lock_s: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Status {
    pub battery: Battery,
    pub screen: ScreenState,
    pub brightness: u32,
    pub orientation: u16,
    pub rotation_locked: bool,
    pub lid_closed: bool,
    pub wifi: WifiStatus,
    #[serde(default)]
    pub hotspot: HotspotStatus,
    #[serde(default)]
    pub mobile: MobileStatus,
    /// GPS switch and position (owner request 2026-11-11).
    #[serde(default)]
    pub gps: GpsStatus,
    /// Header stats and their switches (owner request 2026-11-11).
    #[serde(default)]
    pub stats: StatsStatus,
    #[serde(default)]
    pub phone: PhoneStatus,
    #[serde(default)]
    pub bluetooth: BluetoothStatus,
    #[serde(default)]
    pub audio: AudioStatus,
    pub lock: LockStatus,
    /// VPN, DNS and proxy (owner request 2026-11-10, change vpn-and-dns).
    #[serde(default)]
    pub vpn: VpnStatus,
    pub apps: Vec<AppState>,
    pub hardware: Hardware,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppState {
    pub app: String,
    pub running: bool,
}

/// The pre-Rust /run/walpad/status.json, kept byte-compatible in field names and types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyStatus {
    pub time: u64,
    pub uptime_s: f64,
    pub battery: LegacyBattery,
    pub orientation: u16,
    pub rotation_locked: bool,
    pub screen: ScreenState,
    pub brightness: u32,
    pub lid_closed: bool,
    pub low_battery: bool,
    pub wifi: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct LegacyBattery {
    pub percent: i32,
    pub estimated: bool,
    pub status: String,
    pub mv: u32,
    pub temp_dC: String,
}

impl LegacyStatus {
    /// `wifi_state_raw` is the content of /run/walpad/wifi-state ("off" when missing).
    pub fn from_status(s: &Status, time: u64, uptime_s: f64, wifi_state_raw: &str) -> Self {
        LegacyStatus {
            time,
            uptime_s,
            battery: LegacyBattery {
                percent: s.battery.percent.map(i32::from).unwrap_or(-1),
                estimated: s.battery.source == BatterySource::VoltageEstimate,
                status: s.battery.state.clone().unwrap_or_else(|| "?".into()),
                mv: s.battery.mv.unwrap_or(0),
                temp_dC: s
                    .battery
                    .temp_c
                    .map(|t| format!("{}", (t * 10.0).round() as i32))
                    .unwrap_or_default(),
            },
            orientation: s.orientation,
            rotation_locked: s.rotation_locked,
            screen: s.screen,
            brightness: s.brightness,
            lid_closed: s.lid_closed,
            low_battery: s.battery.low,
            wifi: wifi_state_raw.to_string(),
        }
    }
}
