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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hardware {
    pub touch: String,
    pub gsensor: String,
    pub audio: String,
    pub bluetooth: String,
    pub sd_card: String,
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
