//! Human-readable and JSON reporting for hardware detection.
use crate::probe::HardwareAudit;
use serde::{Deserialize, Serialize};
use walpad_hw::domain::device_profile::DeviceProfile;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectionReport {
    pub profile: DeviceProfile,
    pub audit: HardwareAudit,
    pub tier_label: String,
}

pub fn tier_label(tier: u8) -> &'static str {
    match tier {
        1 => "Tier 1 — Fully Supported Reference",
        2 => "Tier 2 — Basic Operation (Known Limitations)",
        3 => "Tier 3 — Experimental / Generic",
        _ => "Unknown Tier",
    }
}

pub fn build_report(audit: HardwareAudit, profile: DeviceProfile) -> DetectionReport {
    let label = tier_label(profile.tier).to_string();
    DetectionReport {
        profile,
        audit,
        tier_label: label,
    }
}

pub fn format_human_report(report: &DetectionReport) -> String {
    let a = &report.audit;
    let p = &report.profile;

    let soc_str = a
        .soc_model
        .as_deref()
        .or_else(|| (!p.soc.is_empty()).then_some(p.soc.as_str()))
        .unwrap_or("Unknown");

    let mut out = String::new();
    out.push_str("====================================================\n");
    out.push_str("          WALPAD OS HARDWARE DETECTOR\n");
    out.push_str("====================================================\n\n");

    out.push_str("DEVICE IDENTITY:\n");
    out.push_str(&format!("  Profile ID:      {}\n", p.id));
    out.push_str(&format!(
        "  Device Model:    {} ({})\n",
        p.name, p.manufacturer
    ));
    out.push_str(&format!("  Architecture:    {}\n", a.architecture));
    out.push_str(&format!("  Processor / SoC: {}\n", soc_str));
    out.push_str(&format!("  Kernel Version:  {}\n", a.kernel_version));
    out.push_str(&format!("  Platform Status: {}\n\n", report.tier_label));

    out.push_str("PROBED HARDWARE STATUS:\n");
    let bl_status = match &a.display_backlight {
        Some(path) => format!("Detected ({path})"),
        None => "Not Found / Unsupported".into(),
    };
    out.push_str(&format!("  Display:         {bl_status}\n"));

    let touch_status = if a.touch_detected {
        "Detected (evdev touch controller)"
    } else {
        "No evdev touchscreen found"
    };
    out.push_str(&format!("  Touchscreen:     {touch_status}\n"));

    let wifi_status = if a.wifi_interfaces.is_empty() {
        "None active".to_string()
    } else {
        format!("Detected ({})", a.wifi_interfaces.join(", "))
    };
    out.push_str(&format!("  Wi-Fi:           {wifi_status}\n"));

    let bt_status = if a.bluetooth_controllers.is_empty() {
        "None detected".to_string()
    } else {
        format!("Detected ({})", a.bluetooth_controllers.join(", "))
    };
    out.push_str(&format!("  Bluetooth:       {bt_status}\n"));

    let bat_status = match (a.battery_capacity, &a.battery_status) {
        (Some(c), Some(s)) => format!("Detected ({c}%, {s})"),
        (Some(c), None) => format!("Detected ({c}%)"),
        _ => "None / AC-only".into(),
    };
    out.push_str(&format!("  Battery:         {bat_status}\n"));

    let audio_status = if a.audio_cards.is_empty() {
        "None found".to_string()
    } else {
        format!("Detected ({})", a.audio_cards.join(", "))
    };

    out.push_str(&format!("  Audio:           {audio_status}\n"));

    let accel_status = if a.accelerometer_detected {
        "Detected"
    } else {
        "None"
    };
    out.push_str(&format!("  Accelerometer:   {accel_status}\n"));

    let gnss_status = if a.gnss_devices.is_empty() {
        "None".to_string()
    } else {
        format!("Detected ({})", a.gnss_devices.join(", "))
    };
    out.push_str(&format!("  GNSS / GPS:      {gnss_status}\n"));

    let modem_status = if a.modem_devices.is_empty() {
        "None".to_string()
    } else {
        format!("Detected ({})", a.modem_devices.join(", "))
    };
    out.push_str(&format!("  Modem / 4G:      {modem_status}\n\n"));

    out.push_str("CAPABILITY FLAGS:\n");
    out.push_str(&format!(
        "  GNSS:            {}\n",
        p.capabilities.gnss.as_str()
    ));
    out.push_str(&format!(
        "  Cellular Modem:  {}\n",
        p.capabilities.modem.as_str()
    ));
    out.push_str(&format!(
        "  Camera:          {}\n",
        p.capabilities.camera.as_str()
    ));
    out.push_str(&format!(
        "  Bluetooth:       {}\n",
        p.capabilities.bluetooth.as_str()
    ));
    out.push_str(&format!(
        "  Accelerometer:   {}\n",
        p.capabilities.accelerometer.as_str()
    ));

    out.push_str("====================================================\n");
    out
}

pub fn format_json(report: &DetectionReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests;
