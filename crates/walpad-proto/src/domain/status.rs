use serde::{Deserialize, Serialize};

mod connectivity;
mod power;
mod system;

pub use connectivity::*;
pub use power::*;
pub use system::*;

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
    /// Headphone jack / mic route (owner request 2026-11-12, change headphone-mic-jack).
    #[serde(default)]
    pub jack: JackStatus,
    pub lock: LockStatus,
    /// VPN, DNS and proxy (owner request 2026-11-10, change vpn-and-dns).
    #[serde(default)]
    pub vpn: VpnStatus,
    pub apps: Vec<AppState>,
    pub hardware: Hardware,
    /// Badge for the notification centre (owner request 2026-11-12).
    #[serde(default)]
    pub notifications: NotificationsStatus,
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
