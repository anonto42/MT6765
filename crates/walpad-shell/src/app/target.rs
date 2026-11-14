//! Hit-testing targets and dock/home tile models.

use crate::media::MediaTarget;
use crate::phone_ui::PhoneTarget;
use crate::settings_screen::Page;
use crate::store::StoreTarget;
use crate::topbar::StatItem;
use walpad_ui::Icon;

use super::lock_flow::LockAct;

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
    JackHeadphones,
    JackHeadsetMic,
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
    // GPS (owner request 2026-11-11): the Quick Settings row, the screen's switch and Copy buttons
    GpsCard,
    GpsSwitch,
    GpsCopyCoords,
    GpsCopyLink,
    GpsOpenMap,
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
    WifiManualSsid,
    NetMacMode,
    NetMacEdit,
    NetIpMode,
    NetIpEdit,
    NetMaskCycle,
    NetGatewayEdit,
    NetHostnameEdit,
    // Device
    Rot(usize),
    RotateFlipToggle,
    Dim(usize),
    Off(usize),
    LidToggle,
    ThemeToggle,
    Wallpaper(usize),
    WallpaperPathEdit,
    PowerDimCycle,
    PowerDimLevelCycle,
    PowerOffCycle,
    LowBattCycle,
    CritBattCycle,
    ChargeModeCycle,
    ChargeLimitCycle,
    ChargeCurrentCycle,
    TouchGesturesToggle,
    MultitouchToggle,
    KeyGesture(usize),
    // Settings
    Page(Page),
    Back,
    ClockToggle,
    ClockSetTime,
    /// Settings > Top bar items: one live figure on or off.
    Stat(StatItem),
    /// Settings > Input > Windows (change multitasking-windows).
    WindowsMode,
    WindowsRatio,
    WindowsSwap,
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
    /// Media screen (change media-apps-capture).
    Media(MediaTarget),
    /// Phone screen and its sheets (owner request 2026-11-11, change phone-dialer).
    Phone(PhoneTarget),
    /// Notification centre (owner request 2026-11-12, change `device-notifications`): the bell
    /// icon in the header, a row (index into `Model::notifications`), Clear all.
    NotificationsCard,
    NotificationRow(usize),
    NotificationsClearAll,
}
