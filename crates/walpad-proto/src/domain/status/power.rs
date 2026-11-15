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
