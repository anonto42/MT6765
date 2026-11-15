use super::*;
use crate::probe::HardwareAudit;
use walpad_hw::domain::device_profile::DeviceProfile;

fn mock_audit() -> HardwareAudit {
    HardwareAudit {
        architecture: "aarch64".into(),
        kernel_version: "4.19.127".into(),
        soc_model: Some("MediaTek MT6765".into()),
        devicetree_compatible: Some("walton,walpad8g".into()),
        display_backlight: Some("/sys/class/leds/lcd-backlight/brightness".into()),
        framebuffer: Some("/dev/fb0".into()),
        battery_capacity: Some(88),
        battery_status: Some("Charging".into()),
        input_devices: vec!["mtk-tpd".into(), "mtk-kpd".into()],
        touch_detected: true,
        wifi_interfaces: vec!["wlan0".into()],
        bluetooth_controllers: vec!["hci0".into()],
        audio_cards: vec!["0 [mt6357]".into()],
        modem_devices: vec!["/dev/radio/pttycmd1".into()],
        gnss_devices: vec!["/dev/strmon".into()],
        accelerometer_detected: true,
    }
}

#[test]
fn report_human_formatting_includes_hardware_lines() {
    let audit = mock_audit();
    let profile = DeviceProfile::default_walpad_8g();
    let report = build_report(audit, profile);

    let text = format_human_report(&report);
    assert!(text.contains("WALPAD OS HARDWARE DETECTOR"));
    assert!(text.contains("walton-walpad-8g"));
    assert!(text.contains("Tier 1 — Fully Supported Reference"));
    assert!(text.contains("MediaTek MT6765"));
    assert!(text.contains("88%, Charging"));
    assert!(text.contains("wlan0"));
    assert!(text.contains("hci0"));
    assert!(text.contains("mt6357"));
}

#[test]
fn report_json_formatting_is_valid_json() {
    let audit = mock_audit();
    let profile = DeviceProfile::default_walpad_8g();
    let report = build_report(audit, profile);

    let json_str = format_json(&report).expect("json formatting failed");
    let val: serde_json::Value = serde_json::from_str(&json_str).expect("invalid json");

    assert_eq!(val["profile"]["id"], "walton-walpad-8g");
    assert_eq!(val["audit"]["battery_capacity"], 88);
    assert_eq!(val["tier_label"], "Tier 1 — Fully Supported Reference");
}
