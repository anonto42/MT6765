use super::types::SheetPurpose;
use walpad_proto::LockKind;
use walpad_ui::components::KeyboardLayer;

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
            SheetPurpose::ClockTime => ("Set date & time", KeyboardLayer::Symbols),
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
                | SheetPurpose::ClockTime
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
            SheetPurpose::ClockTime => walpad_proto::valid_datetime(&self.secret),
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
            SheetPurpose::ClockTime => 19,
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
