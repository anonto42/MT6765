//! Hardware capability definitions and backend abstraction traits.
use serde::{Deserialize, Serialize};

/// Status of a given hardware subsystem on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// The capability is verified and supported by drivers and configuration.
    Supported,
    /// The capability is absent, locked, or known not to work on this platform.
    Unsupported,
    /// The capability status has not been probed or configured.
    #[default]
    Unknown,
}

impl Capability {
    pub fn is_supported(self) -> bool {
        matches!(self, Capability::Supported)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Capability::Supported => "supported",
            Capability::Unsupported => "unsupported",
            Capability::Unknown => "unknown",
        }
    }

    pub fn from_name(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "supported" | "yes" | "true" | "1" => Capability::Supported,
            "unsupported" | "no" | "false" | "0" | "disabled" => Capability::Unsupported,
            _ => Capability::Unknown,
        }
    }
}

/// Abstract display control backend.
pub trait DisplayBackend {
    fn get_brightness(&self) -> Option<u32>;
    fn set_brightness(&self, val: u8) -> std::io::Result<()>;
    fn dimensions(&self) -> (u32, u32);
    fn orientation(&self) -> &str;
}

/// Abstract battery and power supply query backend.
pub trait BatteryBackend {
    fn read_capacity(&self) -> Option<u8>;
    fn read_status(&self) -> Option<String>;
    fn is_charging(&self) -> bool;
}

/// Abstract input device matching backend.
pub trait InputBackend {
    fn is_touch_device(&self, name: &str) -> bool;
    fn key_device_name(&self, code: u16) -> Option<&'static str>;
}

/// Abstract audio routing backend.
pub trait AudioBackend {
    fn speaker_route(&self) -> &str;
    fn is_headphone_connected(&self) -> Option<bool>;
}

/// Abstract network interface backend.
pub trait NetworkBackend {
    fn wifi_interface(&self) -> Option<&str>;
    fn bluetooth_controller(&self) -> Option<&str>;
}

/// Abstract power and CPU frequency scaling backend.
pub trait PowerBackend {
    fn set_governor(&self, governor: &str) -> usize;
    fn uptime_seconds(&self) -> Option<f64>;
}

#[cfg(test)]
mod tests;
