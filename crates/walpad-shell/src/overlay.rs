//! Modal overlays: the Wi-Fi password sheet (with the on-screen keyboard), the app options sheet,
//! the forget-network and power dialogs, the power menu, and the two popovers (quick settings and
//! the Apps panel, drawn by `panels`). Geometry is pure so hit areas are testable.
use crate::app::Target;
use walpad_proto::LockKind;
use walpad_ui::components::{keyboard_layout, ButtonKind, ButtonState, KeyboardLayer, Painter};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::Rect;

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct PasswordSheet {
    pub ssid_hex: String,
    /// Sheet title: the network name (Wi-Fi) or "Hotspot name" / "Hotspot password".
    pub ssid: String,
    /// The text being typed (a password, or the hotspot name).
    pub secret: String,
    pub shown: bool,
    pub layer: KeyboardLayer,
    /// One-shot shift: the next letter is upper case.
    pub shift: bool,
    pub error: Option<String>,
    /// Waiting for the core (connecting / saving).
    pub connecting: bool,
    pub pressed_key: Option<usize>,
    pub purpose: SheetPurpose,
}

impl PasswordSheet {
    pub fn new(ssid_hex: String, ssid: String) -> Self {
        PasswordSheet {
            ssid_hex,
            ssid,
            secret: String::new(),
            shown: false,
            layer: KeyboardLayer::Lower,
            shift: false,
            error: None,
            connecting: false,
            pressed_key: None,
            purpose: SheetPurpose::WifiJoin,
        }
    }
    /// The hotspot name or password sheet; `initial` pre-fills the field (the name only).
    pub fn hotspot(purpose: SheetPurpose, initial: &str) -> Self {
        let title = match purpose {
            SheetPurpose::HotspotName => "Hotspot name",
            _ => "Hotspot password",
        };
        PasswordSheet {
            secret: initial.to_string(),
            shown: purpose == SheetPurpose::HotspotName,
            purpose,
            ..PasswordSheet::new(String::new(), title.into())
        }
    }
    /// A screen-lock sheet (`purpose` must be one of the four lock purposes); a PIN opens on the
    /// digits layer.
    pub fn lock(purpose: SheetPurpose) -> Self {
        let (title, kind) = match purpose {
            SheetPurpose::LockCurrent(k) | SheetPurpose::LockRemove(k) => ("Current", k),
            SheetPurpose::LockNew(k) => ("New", k),
            SheetPurpose::LockRepeat(k) => ("Repeat new", k),
            _ => ("Screen lock", LockKind::Pin),
        };
        let what = if kind == LockKind::Pin {
            "PIN"
        } else {
            "password"
        };
        PasswordSheet {
            layer: if kind == LockKind::Pin {
                KeyboardLayer::Symbols
            } else {
                KeyboardLayer::Lower
            },
            purpose,
            ..PasswordSheet::new(String::new(), format!("{title} {what}"))
        }
    }
    /// The sheet belongs to the screen-lock flow.
    pub fn is_lock(&self) -> bool {
        matches!(
            self.purpose,
            SheetPurpose::LockCurrent(_)
                | SheetPurpose::LockRemove(_)
                | SheetPurpose::LockNew(_)
                | SheetPurpose::LockRepeat(_)
        )
    }
    /// The APN or SIM PIN sheet. The PIN sheet opens on the digits layer.
    pub fn mobile(purpose: SheetPurpose, initial: &str) -> Self {
        let (title, layer) = match purpose {
            SheetPurpose::SimPin => ("SIM PIN", KeyboardLayer::Symbols),
            _ => ("Access point (APN)", KeyboardLayer::Lower),
        };
        PasswordSheet {
            secret: initial.to_string(),
            shown: purpose == SheetPurpose::MobileApn,
            layer,
            purpose,
            ..PasswordSheet::new(String::new(), title.into())
        }
    }
    /// A text sheet of the Phone screen; `title` names the field, `initial` pre-fills it. Number
    /// fields open on the digits layer.
    pub fn phone(purpose: SheetPurpose, title: &str, initial: &str) -> Self {
        let layer = match purpose {
            SheetPurpose::PhoneContactNumber | SheetPurpose::PhoneNewNumber => {
                KeyboardLayer::Symbols
            }
            _ => KeyboardLayer::Lower,
        };
        PasswordSheet {
            secret: initial.to_string(),
            shown: true,
            layer,
            purpose,
            ..PasswordSheet::new(String::new(), title.into())
        }
    }
    /// The sheet belongs to the Phone screen (search, contacts, messages).
    pub fn is_phone(&self) -> bool {
        matches!(
            self.purpose,
            SheetPurpose::PhoneSearch
                | SheetPurpose::PhoneContactName
                | SheetPurpose::PhoneContactNumber
                | SheetPurpose::PhoneCompose
                | SheetPurpose::PhoneNewNumber
        )
    }
    /// The DNS or proxy sheet of the VPN screen.
    pub fn vpn(purpose: SheetPurpose, initial: &str) -> Self {
        let title = match purpose {
            SheetPurpose::VpnDns => "DNS servers",
            _ => "HTTP proxy",
        };
        PasswordSheet {
            secret: initial.to_string(),
            shown: true,
            purpose,
            ..PasswordSheet::new(String::new(), title.into())
        }
    }
    pub fn is_vpn(&self) -> bool {
        matches!(self.purpose, SheetPurpose::VpnDns | SheetPurpose::VpnProxy)
    }
    /// Manual network configuration sheet (SSID, IP, Gateway, MAC, Hostname).
    pub fn net_config(purpose: SheetPurpose, initial: &str) -> Self {
        let (title, layer) = match purpose {
            SheetPurpose::WifiManualSsid => ("Join hidden network (SSID)", KeyboardLayer::Lower),
            SheetPurpose::NetIp => ("IPv4 address", KeyboardLayer::Symbols),
            SheetPurpose::NetGateway => ("Gateway / Router IP", KeyboardLayer::Symbols),
            SheetPurpose::NetMac => ("MAC address", KeyboardLayer::Lower),
            _ => ("Device hostname", KeyboardLayer::Lower),
        };
        PasswordSheet {
            secret: initial.to_string(),
            shown: true,
            layer,
            purpose,
            ..PasswordSheet::new(String::new(), title.into())
        }
    }
    pub fn is_net_config(&self) -> bool {
        matches!(
            self.purpose,
            SheetPurpose::WifiManualSsid
                | SheetPurpose::NetIp
                | SheetPurpose::NetGateway
                | SheetPurpose::NetMac
                | SheetPurpose::NetHostname
                | SheetPurpose::WallpaperPath
        )
    }
    /// The sheet sets something on the mobile modem.
    pub fn is_mobile(&self) -> bool {
        matches!(self.purpose, SheetPurpose::MobileApn | SheetPurpose::SimPin)
    }
    /// WPA2: 8-63 characters, or a 64-digit hex key; the hotspot takes 8-63 printable ASCII and a
    /// name of 1-32 bytes.
    pub fn valid(&self) -> bool {
        match self.purpose {
            SheetPurpose::WifiJoin => {
                let n = self.secret.len();
                (8..=63).contains(&n)
                    || (n == 64 && self.secret.bytes().all(|b| b.is_ascii_hexdigit()))
            }
            SheetPurpose::HotspotName => walpad_proto::valid_hotspot_ssid(&self.secret),
            SheetPurpose::HotspotPassword => walpad_proto::valid_hotspot_secret(&self.secret),
            SheetPurpose::MobileApn => walpad_proto::valid_apn(&self.secret),
            SheetPurpose::SimPin => walpad_proto::valid_sim_pin(&self.secret),
            SheetPurpose::VpnDns => {
                let l = dns_list(&self.secret);
                l.len() <= 3 && l.iter().all(|d| walpad_proto::valid_dns_server(d))
            }
            SheetPurpose::VpnProxy => {
                self.secret.is_empty() || walpad_proto::valid_proxy(&self.secret)
            }
            SheetPurpose::LockCurrent(_) | SheetPurpose::LockRemove(_) => {
                walpad_proto::valid_attempt(&self.secret)
            }
            SheetPurpose::LockNew(k) | SheetPurpose::LockRepeat(k) => {
                walpad_proto::valid_lock_secret(k, &self.secret)
            }
            SheetPurpose::PhoneSearch => true,
            SheetPurpose::PhoneContactName => walpad_proto::valid_contact_name(&self.secret),
            SheetPurpose::PhoneContactNumber | SheetPurpose::PhoneNewNumber => {
                walpad_proto::valid_sms_number(&self.secret)
            }
            SheetPurpose::PhoneCompose => walpad_proto::valid_sms_text(&self.secret),
            SheetPurpose::WifiManualSsid => walpad_proto::valid_hotspot_ssid(&self.secret),
            SheetPurpose::NetIp | SheetPurpose::NetGateway => {
                self.secret.is_empty() || walpad_proto::valid_dns_server(&self.secret)
            }
            SheetPurpose::NetMac => {
                self.secret.is_empty()
                    || walpad_proto::valid_bt_address(&normalize_mac(&self.secret))
            }
            SheetPurpose::NetHostname => {
                self.secret.is_empty() || walpad_proto::valid_hostname(&self.secret)
            }
            SheetPurpose::WallpaperPath => {
                self.secret.is_empty()
                    || (self.secret.starts_with('/') && !self.secret.contains(".."))
            }
        }
    }
    /// Longest text the field takes (bytes).
    pub fn max_len(&self) -> usize {
        match self.purpose {
            SheetPurpose::WifiJoin => 64,
            SheetPurpose::HotspotName => 32,
            SheetPurpose::HotspotPassword => 63,
            SheetPurpose::MobileApn => 62,
            SheetPurpose::SimPin => 8,
            SheetPurpose::VpnDns => 80,
            SheetPurpose::VpnProxy => 100,
            SheetPurpose::LockCurrent(_)
            | SheetPurpose::LockRemove(_)
            | SheetPurpose::LockNew(_)
            | SheetPurpose::LockRepeat(_) => 64,
            SheetPurpose::PhoneSearch => 40,
            SheetPurpose::PhoneContactName => 96,
            SheetPurpose::PhoneContactNumber | SheetPurpose::PhoneNewNumber => 21,
            SheetPurpose::PhoneCompose => 1200,
            SheetPurpose::WifiManualSsid => 32,
            SheetPurpose::NetIp | SheetPurpose::NetGateway => 45,
            SheetPurpose::NetMac => 17,
            SheetPurpose::NetHostname => 63,
            SheetPurpose::WallpaperPath => 256,
        }
    }
}

/// DNS servers typed in the sheet: split on commas and spaces.
pub fn dns_list(s: &str) -> Vec<String> {
    s.split([',', ' '])
        .filter(|d| !d.is_empty())
        .map(str::to_string)
        .collect()
}

/// Normalize a typed MAC address (`aabbccddeeff`, `aa-bb-cc-dd-ee-ff`, or `aa:bb:cc:dd:ee:ff`) to
/// upper-case `AA:BB:CC:DD:EE:FF`.
pub fn normalize_mac(s: &str) -> String {
    let s = s.trim();
    if s.len() == 12 && s.bytes().all(|b| b.is_ascii_hexdigit()) {
        let up = s.to_ascii_uppercase();
        format!(
            "{}:{}:{}:{}:{}:{}",
            &up[0..2],
            &up[2..4],
            &up[4..6],
            &up[6..8],
            &up[8..10],
            &up[10..12]
        )
    } else {
        s.replace('-', ":").to_ascii_uppercase()
    }
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
    Password(PasswordSheet),
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

const SHEET_W: i32 = 800;
pub const PASSWORD_SHEET_H: i32 = 524;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordGeom {
    pub sheet: Rect,
    pub field: Rect,
    pub keyboard: Rect,
    pub cancel: Rect,
    pub connect: Rect,
}

pub fn password_geom(frame: &Frame) -> PasswordGeom {
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new(
        (frame.w - w) / 2,
        frame.h - PASSWORD_SHEET_H,
        w,
        PASSWORD_SHEET_H,
    );
    let content = Rect::new(
        sheet.x + SP_5,
        sheet.y + 68,
        sheet.w - 2 * SP_5,
        sheet.h - 68 - SP_5,
    );
    let field = Rect::new(content.x, content.y, content.w, 56);
    let keyboard = Rect::new(sheet.x, field.bottom() + 34, sheet.w, 274);
    let bw = (content.w - SP_3) / 2;
    let by = keyboard.bottom() + SP_3;
    PasswordGeom {
        sheet,
        field,
        keyboard,
        cancel: Rect::new(content.x, by, bw, BUTTON_H),
        connect: Rect::new(content.x + bw + SP_3, by, bw, BUTTON_H),
    }
}

pub fn render_password(
    p: &mut Painter,
    frame: &Frame,
    s: &PasswordSheet,
    spinner_deg: u16,
) -> Vec<(Rect, Target)> {
    let g = password_geom(frame);
    let mut hits = Vec::new();
    p.sheet(g.sheet, &s.ssid);
    let (placeholder, secret_field, confirm) = match s.purpose {
        SheetPurpose::WifiJoin => ("Password", true, "Connect"),
        SheetPurpose::HotspotName => ("Name", false, "Save"),
        SheetPurpose::HotspotPassword => ("8-63 characters", true, "Save"),
        SheetPurpose::MobileApn => ("e.g. gpinternet", false, "Save"),
        SheetPurpose::SimPin => ("4-8 digits", true, "Unlock"),
        SheetPurpose::VpnDns => ("e.g. 1.1.1.1, 9.9.9.9 (empty: automatic)", false, "Save"),
        SheetPurpose::VpnProxy => ("http://host:port (empty: none)", false, "Save"),
        SheetPurpose::LockCurrent(_) => ("Current secret", true, "Next"),
        SheetPurpose::LockRemove(_) => ("Current secret", true, "Remove lock"),
        SheetPurpose::LockNew(LockKind::Pin) => ("4-12 digits", true, "Next"),
        SheetPurpose::LockNew(_) => ("4-64 characters", true, "Next"),
        SheetPurpose::LockRepeat(_) => ("Type it again", true, "Save"),
        SheetPurpose::PhoneSearch => ("Name or number", false, "Search"),
        SheetPurpose::PhoneContactName => ("Name", false, "Next"),
        SheetPurpose::PhoneContactNumber => ("e.g. 01712345678", false, "Save"),
        SheetPurpose::PhoneCompose => ("Message", false, "Send"),
        SheetPurpose::PhoneNewNumber => ("Number to write to", false, "Next"),
        SheetPurpose::WifiManualSsid => ("Network name (SSID)", false, "Next"),
        SheetPurpose::NetIp => ("e.g. 192.168.0.100 (empty: automatic)", false, "Save"),
        SheetPurpose::NetGateway => ("e.g. 192.168.0.1 (empty: automatic)", false, "Save"),
        SheetPurpose::NetMac => ("AA:BB:CC:DD:EE:FF (empty: hardware)", false, "Save"),
        SheetPurpose::NetHostname => ("e.g. walpad (empty: default)", false, "Save"),
        SheetPurpose::WallpaperPath => ("e.g. /root/wallpaper.png (empty: default)", false, "Save"),
    };
    if let Some(eye) = p.text_field(
        g.field,
        &s.secret,
        placeholder,
        secret_field,
        s.shown,
        true,
        s.error.as_deref(),
    ) {
        hits.push((eye, Target::Eye));
    }
    let keys = p.keyboard(g.keyboard, s.layer, s.pressed_key);
    for (i, k) in keys.iter().enumerate() {
        hits.push((k.rect, Target::Key(i)));
    }
    p.button(
        g.cancel,
        "Cancel",
        ButtonKind::Secondary,
        ButtonState::Default,
        None,
    );
    let cs = if s.connecting {
        ButtonState::Loading(spinner_deg)
    } else if s.valid() {
        ButtonState::Default
    } else {
        ButtonState::Disabled
    };
    p.button(g.connect, confirm, ButtonKind::Primary, cs, None);
    hits.push((g.cancel, Target::SheetCancel));
    hits.push((g.connect, Target::SheetConnect));
    hits
}

pub fn app_sheet_geom(frame: &Frame, running: bool) -> (Rect, Vec<(Rect, Target)>) {
    let n = if running { 3 } else { 2 };
    let h = 68 + n * (BUTTON_H + SP_3) + SP_4;
    let w = SHEET_W.min(frame.w);
    let sheet = Rect::new((frame.w - w) / 2, frame.h - h, w, h);
    let mut y = sheet.y + 68;
    let mut btns = Vec::new();
    let mut add = |t: Target, y: &mut i32| {
        btns.push((
            Rect::new(sheet.x + SP_5, *y, sheet.w - 2 * SP_5, BUTTON_H),
            t,
        ));
        *y += BUTTON_H + SP_3;
    };
    if running {
        add(Target::CloseApp, &mut y);
    }
    add(Target::PinToggle, &mut y);
    add(Target::SheetCancel, &mut y);
    (sheet, btns)
}

pub fn render_app_sheet(p: &mut Painter, frame: &Frame, s: &AppSheet) -> Vec<(Rect, Target)> {
    let (sheet, btns) = app_sheet_geom(frame, s.running);
    p.sheet(sheet, &s.name);
    for (r, t) in &btns {
        match t {
            Target::CloseApp => p.button(
                *r,
                "Close app",
                ButtonKind::Danger,
                ButtonState::Default,
                None,
            ),
            Target::PinToggle => p.button(
                *r,
                if s.pinned {
                    "Unpin from the sidebar"
                } else {
                    "Pin to the sidebar"
                },
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            ),
            _ => p.button(
                *r,
                "Cancel",
                ButtonKind::Secondary,
                ButtonState::Default,
                None,
            ),
        }
    }
    btns
}

pub fn render_forget(p: &mut Painter, frame: &Frame, ssid: &str) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (cancel, confirm) = p.dialog(
        screen,
        "Forget network?",
        &format!("{ssid} will be removed from the saved networks."),
        "Cancel",
        "Forget",
        "Forget",
        true,
        false,
    );
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}

pub fn render_power_confirm(
    p: &mut Painter,
    frame: &Frame,
    poweroff: bool,
    armed: bool,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    let (title, body, label, again) = if poweroff {
        (
            "Shut down?",
            "The tablet will power off. Press the power button to start it again.",
            "Shut down",
            "Tap again to shut down",
        )
    } else {
        (
            "Restart tablet?",
            "The tablet will restart. Open apps will be closed.",
            "Restart",
            "Tap again to restart",
        )
    };
    let (cancel, confirm) = p.dialog(screen, title, body, "Cancel", label, again, true, armed);
    vec![
        (cancel, Target::DialogCancel),
        (confirm, Target::DialogConfirm),
    ]
}

pub fn render_power_menu(
    p: &mut Painter,
    frame: &Frame,
    focus: usize,
    armed: Option<usize>,
) -> Vec<(Rect, Target)> {
    let screen = Rect::new(0, 0, frame.w, frame.h);
    p.power_menu(screen, &POWER_ITEMS, focus, armed)
        .into_iter()
        .enumerate()
        .map(|(i, r)| (r, Target::PowerItem(i)))
        .collect()
}

/// Key index -> key for the current keyboard (same geometry the renderer uses).
pub fn key_at(
    frame: &Frame,
    layer: KeyboardLayer,
    idx: usize,
) -> Option<walpad_ui::components::Key> {
    keyboard_layout(password_geom(frame).keyboard, layer)
        .get(idx)
        .map(|k| k.key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_sheet_fits_both_orientations_and_targets_are_big_enough() {
        for (w, h) in [(800, 1280), (1280, 800)] {
            let g = password_geom(&Frame::new(w, h));
            assert!(
                g.sheet.x >= 0 && g.sheet.right() <= w && g.sheet.y >= 0 && g.sheet.bottom() == h,
                "{w}x{h}: {:?}",
                g.sheet
            );
            assert!(g.field.h >= TOUCH_MIN && g.cancel.h >= TOUCH_MIN && g.connect.h >= TOUCH_MIN);
            assert!(
                g.field.bottom() < g.keyboard.y
                    && g.keyboard.bottom() < g.cancel.y
                    && g.cancel.bottom() <= g.sheet.bottom()
            );
            assert!(g.cancel.right() < g.connect.x);
        }
    }

    #[test]
    fn secret_validity_follows_wpa2_rules() {
        let mut s = PasswordSheet::new("00".into(), "Home".into());
        for (v, ok) in [
            ("", false),
            ("1234567", false),
            ("12345678", true),
            (&"a".repeat(63), true),
            (&"a".repeat(64), true),
            (&"g".repeat(64), false),
            (&"a".repeat(65), false),
        ] {
            s.secret = v.to_string();
            assert_eq!(s.valid(), ok, "{} chars", v.len());
        }
    }

    #[test]
    fn app_sheet_has_close_only_when_running() {
        let f = Frame::new(800, 1280);
        let t = |r| {
            app_sheet_geom(&f, r)
                .1
                .into_iter()
                .map(|(_, t)| t)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            t(true),
            [Target::CloseApp, Target::PinToggle, Target::SheetCancel]
        );
        assert_eq!(t(false), [Target::PinToggle, Target::SheetCancel]);
        let (sheet, btns) = app_sheet_geom(&f, true);
        assert!(btns
            .iter()
            .all(|(r, _)| r.h >= TOUCH_MIN && r.y >= sheet.y && r.bottom() <= sheet.bottom()));
    }
}
