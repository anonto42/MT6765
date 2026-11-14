use walpad_proto::LockKind;

/// What the text sheet (field + keyboard) edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetPurpose {
    /// Join the Wi-Fi network `ssid` with a password.
    WifiJoin,
    /// The hotspot name (shown in clear, 1-32 bytes).
    HotspotName,
    /// The hotspot WPA2 password (8-63 printable ASCII).
    HotspotPassword,
    /// The mobile data APN (shown in clear, 1-62 of A-Z a-z 0-9 . - _; owner request 2026-11-10).
    MobileApn,
    /// The SIM PIN (4-8 digits, hidden; sent to the core as a secret).
    SimPin,
    /// DNS servers, comma separated (shown in clear; empty = automatic).
    VpnDns,
    /// HTTP proxy `http://host:port` (shown in clear; empty = none).
    VpnProxy,
    /// Screen lock (OpenSpec `lock-screen`): the present secret before changing it, the one
    /// before removing the lock, the new one and its repeat. The kind says what is typed.
    LockCurrent(LockKind),
    LockRemove(LockKind),
    LockNew(LockKind),
    LockRepeat(LockKind),
    /// Phone (owner request 2026-11-11): contact search, contact name, contact number, a message
    /// and the number of a new conversation. Typed text stays in the sheet until it is sent.
    PhoneSearch,
    PhoneContactName,
    PhoneContactNumber,
    PhoneCompose,
    PhoneNewNumber,
    /// Manual Wi-Fi network SSID entry and manual network parameters (IP, Gateway, MAC, Hostname).
    WifiManualSsid,
    NetIp,
    NetGateway,
    NetMac,
    NetHostname,
    WallpaperPath,
    ClockTime,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppSheet {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    Password(super::PasswordSheet),
    App(AppSheet),
    Forget {
        id: u32,
        ssid: String,
    },
    /// Pairing in progress (owner request 2026-11-10): waiting, passkey confirmation, failure.
    BtPair {
        address: String,
        name: String,
        state: crate::bluetooth::PairState,
    },
    /// Confirm forgetting a paired Bluetooth device.
    BtForget {
        address: String,
        name: String,
    },
    /// Restart / shut down: the first confirm tap arms the button for 3 s.
    PowerConfirm {
        poweroff: bool,
        armed_until: Option<u64>,
    },
    /// The power menu (long power press / Settings): `focus` is the hardware-key focus, `armed`
    /// the Restart/Shut down item whose first tap happened and when that arming ends, `until`
    /// when the sheet closes itself.
    PowerMenu {
        focus: usize,
        armed: Option<(usize, u64)>,
        until: u64,
    },
    /// The dropdown under the header pill (Wi-Fi, Bluetooth, battery, brightness, quick toggles).
    /// A tap outside closes it.
    QuickSettings,
    /// The Apps panel next to the sidebar (running and installed apps). A tap outside closes it.
    AppsPanel,
    /// A Phone sheet: contact actions, message actions, confirmations.
    Phone(crate::phone_ui::PhoneSheet),
}

impl Overlay {
    /// Light popovers close on a tap outside; the other overlays are modal.
    pub fn is_popover(&self) -> bool {
        matches!(self, Overlay::QuickSettings | Overlay::AppsPanel)
    }
}

/// Items of the power menu: (label, needs two taps).
pub const POWER_ITEMS: [(&str, bool); 4] = [
    ("Screen off", false),
    ("Restart", true),
    ("Shut down", true),
    ("Cancel", false),
];
/// Index of Cancel: the focus when the menu opens.
pub const POWER_CANCEL: usize = 3;
