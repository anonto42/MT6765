use serde::{Deserialize, Serialize};

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

/// USSD session state (`AT+CUSD`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UssdStatus {
    /// `running` | `done` | `failed`
    pub state: String,
    /// Decoded network reply or short error message.
    pub text: Option<String>,
    /// True when the network requested a menu reply (`+CUSD: 1,...`).
    pub interactive: bool,
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
    /// Latest USSD request/response (`AT+CUSD`), if any.
    pub ussd: Option<UssdStatus>,
}
