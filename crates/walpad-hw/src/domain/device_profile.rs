//! Device profile specification and loading.
use super::capability::Capability;
use crate::Root;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub width: u32,
    pub height: u32,
    pub orientation: String,
    pub backlight_path: String,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 1280,
            orientation: "portrait".into(),
            backlight_path: "/sys/class/leds/lcd-backlight/brightness".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfig {
    pub backend: String,
    pub touch_name_patterns: Vec<String>,
    pub power_key_name: String,
    pub volume_keys_name: String,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            backend: "linux-evdev".into(),
            touch_name_patterns: vec!["mtk-tpd".into(), "gsl3670".into(), "touchscreen".into()],
            power_key_name: "mtk-pmic-keys".into(),
            volume_keys_name: "mtk-kpd".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub wifi_backend: String,
    pub wifi_device: String,
    pub bt_device: String,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            wifi_backend: "mediatek-connsys".into(),
            wifi_device: "wlan0".into(),
            bt_device: "hci0".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PowerConfig {
    pub backend: String,
    pub battery_capacity_path: String,
    pub battery_status_path: String,
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            backend: "linux-sysfs".into(),
            battery_capacity_path: "/sys/class/power_supply/battery/capacity".into(),
            battery_status_path: "/sys/class/power_supply/battery/status".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioConfig {
    pub codec: String,
    pub audio_script: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            codec: "mt6357".into(),
            audio_script: "/usr/local/sbin/walpad-audio".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeviceCapabilities {
    pub camera: Capability,
    pub gnss: Capability,
    pub modem: Capability,
    pub accelerometer: Capability,
    pub bluetooth: Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceProfile {
    pub id: String,
    pub name: String,
    pub manufacturer: String,
    pub model: String,
    pub codename: String,
    pub architecture: String,
    pub soc: String,
    pub tier: u8,
    pub display: DisplayConfig,
    pub input: InputConfig,
    pub network: NetworkConfig,
    pub power: PowerConfig,
    pub audio: AudioConfig,
    pub capabilities: DeviceCapabilities,
}

impl DeviceProfile {
    /// Walton WalPad 8G reference profile (Tier 1).
    pub fn default_walpad_8g() -> Self {
        Self {
            id: "walton-walpad-8g".into(),
            name: "Walton WalPad 8G".into(),
            manufacturer: "Walton".into(),
            model: "WalPad 8G".into(),
            codename: "walpad8g".into(),
            architecture: "aarch64".into(),
            soc: "mediatek-mt6765".into(),
            tier: 1,
            display: DisplayConfig::default(),
            input: InputConfig::default(),
            network: NetworkConfig::default(),
            power: PowerConfig::default(),
            audio: AudioConfig::default(),
            capabilities: DeviceCapabilities {
                camera: Capability::Unsupported,
                gnss: Capability::Supported,
                modem: Capability::Supported,
                accelerometer: Capability::Supported,
                bluetooth: Capability::Unsupported,
            },
        }
    }

    /// Generic aarch64 fallback profile (Tier 3).
    pub fn generic_arm64() -> Self {
        Self {
            id: "arm64-generic".into(),
            name: "Generic ARM64 Device".into(),
            manufacturer: "Generic".into(),
            model: "ARM64".into(),
            codename: "generic-arm64".into(),
            architecture: "aarch64".into(),
            soc: "generic".into(),
            tier: 3,
            display: DisplayConfig {
                width: 1280,
                height: 800,
                orientation: "landscape".into(),
                backlight_path: "/sys/class/backlight".into(),
            },
            input: InputConfig {
                backend: "linux-evdev".into(),
                touch_name_patterns: vec!["touchscreen".into()],
                power_key_name: "power".into(),
                volume_keys_name: "keys".into(),
            },
            network: NetworkConfig {
                wifi_backend: "generic-nl80211".into(),
                wifi_device: "wlan0".into(),
                bt_device: "hci0".into(),
            },
            power: PowerConfig {
                backend: "linux-sysfs".into(),
                battery_capacity_path: "/sys/class/power_supply/battery/capacity".into(),
                battery_status_path: "/sys/class/power_supply/battery/status".into(),
            },
            audio: AudioConfig {
                codec: "generic-alsa".into(),
                audio_script: "".into(),
            },
            capabilities: DeviceCapabilities {
                camera: Capability::Unknown,
                gnss: Capability::Unknown,
                modem: Capability::Unknown,
                accelerometer: Capability::Unknown,
                bluetooth: Capability::Unknown,
            },
        }
    }

    /// Parse a simple device.toml content string.
    pub fn parse_toml(content: &str) -> Result<Self, String> {
        let mut p = Self::default_walpad_8g();
        let mut cur_sec = "";

        for raw_line in content.lines() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(sec) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                cur_sec = sec.trim();
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let k = k.trim();
            let v = v.trim();
            p.apply_field(cur_sec, k, v);
        }
        Ok(p)
    }

    fn apply_field(&mut self, section: &str, k: &str, v: &str) {
        let clean = v.trim_matches('"').trim_matches('\'');
        match section {
            "device" => match k {
                "id" => self.id = clean.into(),
                "name" => self.name = clean.into(),
                "manufacturer" => self.manufacturer = clean.into(),
                "model" => self.model = clean.into(),
                "codename" => self.codename = clean.into(),
                "architecture" => self.architecture = clean.into(),
                "soc" => self.soc = clean.into(),
                "tier" => self.tier = clean.parse().unwrap_or(self.tier),
                _ => {}
            },
            "display" => match k {
                "width" => self.display.width = clean.parse().unwrap_or(self.display.width),
                "height" => self.display.height = clean.parse().unwrap_or(self.display.height),
                "orientation" => self.display.orientation = clean.into(),
                "backlight" | "backlight_path" => self.display.backlight_path = clean.into(),
                _ => {}
            },
            "input" => match k {
                "backend" => self.input.backend = clean.into(),
                "power_key_name" => self.input.power_key_name = clean.into(),
                "volume_keys_name" => self.input.volume_keys_name = clean.into(),
                "touch_name_patterns" => {
                    self.input.touch_name_patterns = v
                        .trim_matches(|c| c == '[' || c == ']')
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            },
            "network" => match k {
                "wifi_backend" | "backend" => self.network.wifi_backend = clean.into(),
                "wifi_device" => self.network.wifi_device = clean.into(),
                "bt_device" => self.network.bt_device = clean.into(),
                _ => {}
            },
            "power" => match k {
                "backend" => self.power.backend = clean.into(),
                "battery_capacity" | "battery_capacity_path" => {
                    self.power.battery_capacity_path = clean.into();
                }
                "battery_status" | "battery_status_path" => {
                    self.power.battery_status_path = clean.into();
                }
                _ => {}
            },
            "audio" => match k {
                "codec" => self.audio.codec = clean.into(),
                "audio_script" => self.audio.audio_script = clean.into(),
                _ => {}
            },
            "capabilities" => {
                let cap = Capability::from_name(clean);
                match k {
                    "camera" => self.capabilities.camera = cap,
                    "gnss" => self.capabilities.gnss = cap,
                    "modem" => self.capabilities.modem = cap,
                    "accelerometer" => self.capabilities.accelerometer = cap,
                    "bluetooth" => self.capabilities.bluetooth = cap,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Load device profile from a path.
    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse_toml(&content)
    }

    /// Detect device profile from root prefix or fall back safely.
    pub fn detect(root: &Root) -> Self {
        // Priority 1: explicitly configured profile at /etc/walpad/device.toml
        let explicit = root.p("/etc/walpad/device.toml");
        if explicit.exists() {
            if let Ok(p) = Self::load_from_file(&explicit) {
                return p;
            }
        }

        // Priority 2: devicetree compatible check for mt6765
        let dt = root.p("/sys/firmware/devicetree/base/compatible");
        if let Ok(comp) = std::fs::read_to_string(dt) {
            if comp.contains("mt6765") || comp.contains("walpad") {
                return Self::default_walpad_8g();
            }
        }

        // Priority 3: check /proc/cpuinfo for Hardware : MT6765
        let cpuinfo = root.p("/proc/cpuinfo");
        if let Ok(info) = std::fs::read_to_string(cpuinfo) {
            if info.contains("MT6765") || info.contains("mt6765") {
                return Self::default_walpad_8g();
            }
        }

        Self::generic_arm64()
    }
}

#[cfg(test)]
mod tests;
