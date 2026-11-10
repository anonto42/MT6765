use crate::{ErrorCode, Secret};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const COMMAND_NAMES: &[&str] = &[
    "hello",
    "subscribe",
    "get_status",
    "screen",
    "set_brightness",
    "rotate",
    "wifi_scan",
    "wifi_list_saved",
    "wifi_connect",
    "wifi_disconnect",
    "wifi_forget",
    "wifi_enable",
    "hotspot",
    "hotspot_config",
    "launch",
    "close_app",
    "home",
    "keyboard",
    "power",
    "restart_shell",
    "screenshot",
    "set_setting",
    "power_menu",
];

/// Commands without arguments.
const NO_ARG_COMMANDS: &[&str] = &[
    "get_status",
    "wifi_scan",
    "wifi_list_saved",
    "wifi_disconnect",
    "home",
    "restart_shell",
    "screenshot",
];

/// Settings accepted by `set_setting` (section 9 of the plan).
pub const ALLOWED_SETTINGS: &[&str] = &[
    "auto_rotate",
    "rotate_flip",
    "dim_after_s",
    "off_after_s",
    "lid_off",
    "clock_24h",
    "pinned_apps",
];

#[derive(Debug, Clone, Deserialize)]
pub struct RawRequest {
    pub v: u32,
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Topic {
    Status,
    Wifi,
    Apps,
    Notice,
    Input,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScreenMode {
    On,
    Off,
    Toggle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RotateMode {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "lock")]
    Lock,
    #[serde(rename = "toggle")]
    Toggle,
    #[serde(rename = "0")]
    R0,
    #[serde(rename = "90")]
    R90,
    #[serde(rename = "180")]
    R180,
    #[serde(rename = "270")]
    R270,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardMode {
    Show,
    Hide,
    Toggle,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    Poweroff,
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrightnessArgs {
    #[serde(default)]
    pub value: Option<i32>,
    #[serde(default)]
    pub delta: Option<i32>,
}

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
    Launch {
        app: String,
    },
    /// Stop an app that the core started.
    CloseApp {
        app: String,
    },
    Home,
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
    /// The shell tells the core its power menu opened or closed (the core routes the volume and
    /// power keys to the shell while it is open). `open: true` from a client also opens it.
    PowerMenu {
        open: bool,
    },
}

/// Hardware-key events published on the `input` topic for the shell (never touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ShellInput {
    /// Long power press: open the power menu.
    PowerMenuOpen,
    VolumeUp,
    VolumeDown,
    /// Short power press while the power menu is open: activate the focused item.
    PowerShort,
    /// Show / hide the shell's system keyboard (the `keyboard` command, the `keyboard` key
    /// action, Home). The core resolves `toggle` against the state the shell last reported.
    KeyboardShow,
    KeyboardHide,
}

impl Command {
    /// Build from an envelope, distinguishing unknown commands from bad arguments.
    pub fn from_raw(raw: &RawRequest) -> Result<Command, (ErrorCode, String)> {
        if !COMMAND_NAMES.contains(&raw.cmd.as_str()) {
            return Err((
                ErrorCode::UnknownCommand,
                format!("unknown command {:?}", raw.cmd),
            ));
        }
        let mut obj = serde_json::Map::new();
        obj.insert("cmd".into(), Value::String(raw.cmd.clone()));
        // commands that take no arguments ignore a stray `args` (unknown fields are ignored)
        if NO_ARG_COMMANDS.contains(&raw.cmd.as_str()) {
            // nothing to insert
        } else if !raw.args.is_null() {
            obj.insert("args".into(), raw.args.clone());
        } else if matches!(raw.cmd.as_str(), "set_brightness") {
            obj.insert("args".into(), Value::Object(Default::default()));
        }
        let cmd: Command = serde_json::from_value(Value::Object(obj))
            .map_err(|e| (ErrorCode::InvalidArgs, format!("bad args: {e}")))?;
        cmd.validate().map_err(|m| (ErrorCode::InvalidArgs, m))?;
        Ok(cmd)
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Command::Hello { client, .. } if client.is_empty() || client.len() > 64 => {
                Err("client must be 1-64 chars".into())
            }
            Command::SetBrightness(a) => match (a.value, a.delta) {
                (Some(v), None) if (0..=255).contains(&v) => Ok(()),
                (None, Some(d)) if (-255..=255).contains(&d) => Ok(()),
                (Some(_), None) => Err("value must be 0-255".into()),
                (None, Some(_)) => Err("delta must be -255..255".into()),
                _ => Err("give exactly one of value or delta".into()),
            },
            Command::WifiConnect {
                ssid,
                secret,
                saved_id,
            } => {
                match (ssid, saved_id) {
                    (Some(h), None) => {
                        if h.is_empty()
                            || h.len() > 64
                            || h.len() % 2 != 0
                            || !h.bytes().all(|b| b.is_ascii_hexdigit())
                        {
                            return Err("ssid must be 1-32 bytes as hex".into());
                        }
                    }
                    (None, Some(_)) => {}
                    _ => return Err("give exactly one of ssid or saved_id".into()),
                }
                if let Some(s) = secret {
                    let n = s.expose().len();
                    if !(8..=63).contains(&n) && n != 64 {
                        return Err("secret length is not valid".into());
                    }
                }
                Ok(())
            }
            Command::HotspotConfig { ssid, secret } => {
                if ssid.is_none() && secret.is_none() {
                    return Err("give ssid and/or secret".into());
                }
                if ssid.as_deref().is_some_and(|n| !valid_hotspot_ssid(n)) {
                    return Err("ssid must be 1-32 bytes without control characters".into());
                }
                if secret
                    .as_ref()
                    .is_some_and(|s| !valid_hotspot_secret(s.expose()))
                {
                    return Err("secret must be 8-63 printable ASCII characters".into());
                }
                Ok(())
            }
            Command::Launch { app } | Command::CloseApp { app } => {
                if valid_app_id(app) {
                    Ok(())
                } else {
                    Err("app must be an id like \"browser\"".into())
                }
            }
            Command::SetSetting { key, value } => validate_setting(key, value),
            _ => Ok(()),
        }
    }
}

/// Hotspot name: 1-32 bytes of UTF-8 without control characters.
pub fn valid_hotspot_ssid(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && !s.chars().any(char::is_control)
}

/// Hotspot WPA2 passphrase: 8-63 printable ASCII characters (what hostapd's wpa_passphrase takes).
pub fn valid_hotspot_secret(s: &str) -> bool {
    (8..=63).contains(&s.len()) && s.bytes().all(|b| (0x20..=0x7e).contains(&b))
}

/// App ids are short lower-case slugs ("browser", "system-monitor").
pub fn valid_app_id(app: &str) -> bool {
    !app.is_empty()
        && app.len() <= 32
        && app
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn validate_setting(key: &str, v: &Value) -> Result<(), String> {
    if !ALLOWED_SETTINGS.contains(&key) {
        return Err(format!("setting {key:?} is not allowed"));
    }
    match key {
        "pinned_apps" => match v.as_array() {
            Some(a) if a.len() <= 8 && a.iter().all(|x| x.as_str().is_some_and(valid_app_id)) => {
                Ok(())
            }
            _ => Err("value must be a list of at most 8 app ids".into()),
        },
        "auto_rotate" | "rotate_flip" | "lid_off" | "clock_24h" => v
            .is_boolean()
            .then_some(())
            .ok_or_else(|| "value must be a boolean".to_string()),
        _ => match v.as_u64() {
            Some(n) if n <= 3600 => Ok(()),
            _ => Err("value must be an integer 0-3600".into()),
        },
    }
}
