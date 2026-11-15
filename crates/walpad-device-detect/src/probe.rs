//! Hardware discovery and inspection routines.
use serde::{Deserialize, Serialize};
use walpad_hw::Root;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareAudit {
    pub architecture: String,
    pub kernel_version: String,
    pub soc_model: Option<String>,
    pub devicetree_compatible: Option<String>,
    pub display_backlight: Option<String>,
    pub framebuffer: Option<String>,
    pub battery_capacity: Option<u8>,
    pub battery_status: Option<String>,
    pub input_devices: Vec<String>,
    pub touch_detected: bool,
    pub wifi_interfaces: Vec<String>,
    pub bluetooth_controllers: Vec<String>,
    pub audio_cards: Vec<String>,
    pub modem_devices: Vec<String>,
    pub gnss_devices: Vec<String>,
    pub accelerometer_detected: bool,
}

pub fn probe_all(root: &Root) -> HardwareAudit {
    let (architecture, soc_model) = probe_cpu(root);
    let devicetree_compatible = probe_dt(root);
    let kernel_version = probe_kernel(root);
    let (display_backlight, framebuffer) = probe_display(root);
    let (battery_capacity, battery_status) = probe_battery(root);
    let (input_devices, touch_detected) = probe_inputs(root);
    let (wifi_interfaces, bluetooth_controllers) = probe_network(root);
    let audio_cards = probe_audio(root);
    let (modem_devices, gnss_devices) = probe_cellular_gnss(root);
    let accelerometer_detected = probe_accelerometer(root);

    HardwareAudit {
        architecture,
        kernel_version,
        soc_model,
        devicetree_compatible,
        display_backlight,
        framebuffer,
        battery_capacity,
        battery_status,
        input_devices,
        touch_detected,
        wifi_interfaces,
        bluetooth_controllers,
        audio_cards,
        modem_devices,
        gnss_devices,
        accelerometer_detected,
    }
}

fn probe_cpu(root: &Root) -> (String, Option<String>) {
    let arch = std::env::consts::ARCH.to_string();
    let cpuinfo_path = root.p("/proc/cpuinfo");
    let Ok(content) = std::fs::read_to_string(cpuinfo_path) else {
        return (arch, None);
    };

    let mut soc = None;
    for line in content.lines() {
        if line.starts_with("Hardware") || line.starts_with("model name") {
            if let Some((_, val)) = line.split_once(':') {
                soc = Some(val.trim().to_string());
                break;
            }
        }
    }
    (arch, soc)
}

fn probe_dt(root: &Root) -> Option<String> {
    let p = root.p("/sys/firmware/devicetree/base/compatible");
    if let Ok(c) = std::fs::read_to_string(p) {
        return Some(c.trim_matches('\0').trim().to_string());
    }
    let p2 = root.p("/proc/device-tree/compatible");
    std::fs::read_to_string(p2)
        .ok()
        .map(|s| s.trim_matches('\0').trim().to_string())
}

fn probe_kernel(root: &Root) -> String {
    let ver_path = root.p("/proc/version");
    if let Ok(c) = std::fs::read_to_string(ver_path) {
        if let Some(first) = c.split_whitespace().nth(2) {
            return first.to_string();
        }
    }
    "unknown".into()
}

fn probe_display(root: &Root) -> (Option<String>, Option<String>) {
    let mut bl = None;
    let leds_dir = root.p("/sys/class/leds");
    if let Ok(rd) = std::fs::read_dir(leds_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.contains("backlight") {
                bl = Some(format!("/sys/class/leds/{name}/brightness"));
                break;
            }
        }
    }
    if bl.is_none() {
        let bl_dir = root.p("/sys/class/backlight");
        if let Ok(rd) = std::fs::read_dir(bl_dir) {
            if let Some(entry) = rd.flatten().next() {
                let name = entry.file_name().to_string_lossy().to_string();
                bl = Some(format!("/sys/class/backlight/{name}/brightness"));
            }
        }
    }

    let fb = if root.p("/dev/fb0").exists() || root.p("/sys/class/graphics/fb0").exists() {
        Some("/dev/fb0".into())
    } else {
        None
    };

    (bl, fb)
}

fn probe_battery(root: &Root) -> (Option<u8>, Option<String>) {
    let cap_path = root.p("/sys/class/power_supply/battery/capacity");
    let cap = std::fs::read_to_string(cap_path)
        .ok()
        .and_then(|s| s.trim().parse::<u8>().ok());

    let stat_path = root.p("/sys/class/power_supply/battery/status");
    let stat = std::fs::read_to_string(stat_path)
        .ok()
        .map(|s| s.trim().to_string());

    (cap, stat)
}

fn probe_inputs(root: &Root) -> (Vec<String>, bool) {
    let devs = walpad_hw::input::discover(root);
    let mut names = Vec::new();
    let mut touch = false;
    for d in devs {
        if walpad_hw::input::is_touch(&d.name) {
            touch = true;
        }
        names.push(d.name);
    }
    (names, touch)
}

fn probe_network(root: &Root) -> (Vec<String>, Vec<String>) {
    let mut wifi = Vec::new();
    let net_dir = root.p("/sys/class/net");
    if let Ok(rd) = std::fs::read_dir(net_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("wlan") || name.starts_with("mlan") {
                wifi.push(name);
            }
        }
    }

    let mut bt = Vec::new();
    let bt_dir = root.p("/sys/class/bluetooth");
    if let Ok(rd) = std::fs::read_dir(bt_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("hci") {
                bt.push(name);
            }
        }
    }

    (wifi, bt)
}

fn probe_audio(root: &Root) -> Vec<String> {
    let mut cards = Vec::new();
    let cards_path = root.p("/proc/asound/cards");
    if let Ok(content) = std::fs::read_to_string(cards_path) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with(|c: char| c.is_ascii_digit()) && trimmed.contains('[') {
                cards.push(trimmed.to_string());
            }
        }
    }
    cards
}

fn probe_cellular_gnss(root: &Root) -> (Vec<String>, Vec<String>) {
    let mut modem = Vec::new();
    let mut gnss = Vec::new();

    let radio_dir = root.p("/dev/radio");
    if let Ok(rd) = std::fs::read_dir(radio_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            modem.push(format!("/dev/radio/{name}"));
        }
    }
    if root.p("/dev/ccmni0").exists() {
        modem.push("/dev/ccmni0".into());
    }

    let strmon = root.p("/dev/strmon");
    if strmon.exists() {
        gnss.push("/dev/strmon".into());
    }
    let gps = root.p("/dev/gps");
    if gps.exists() {
        gnss.push("/dev/gps".into());
    }

    (modem, gnss)
}

fn probe_accelerometer(root: &Root) -> bool {
    let iio = root.p("/sys/bus/iio/devices");
    if let Ok(rd) = std::fs::read_dir(iio) {
        for entry in rd.flatten() {
            let name_path = entry.path().join("name");
            if let Ok(name) = std::fs::read_to_string(name_path) {
                let n = name.to_ascii_lowercase();
                if n.contains("accel") || n.contains("mc34") || n.contains("gsensor") {
                    return true;
                }
            }
        }
    }
    root.p("/dev/gsensor").exists() || root.p("/sys/class/misc/m_acc_misc").exists()
}

#[cfg(test)]
mod tests;
