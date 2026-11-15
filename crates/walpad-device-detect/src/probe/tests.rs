use super::*;
use walpad_hw::Root;

#[test]
fn probe_empty_root_returns_safe_defaults() {
    let td = std::env::temp_dir().join(format!("test-detect-probe-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&td);
    let _ = std::fs::create_dir_all(&td);
    let root = Root::new(&td);

    let audit = probe_all(&root);
    assert!(!audit.architecture.is_empty());
    assert_eq!(audit.kernel_version, "unknown");
    assert_eq!(audit.soc_model, None);
    assert_eq!(audit.battery_capacity, None);
    assert!(!audit.touch_detected);
}

#[test]
fn probe_populates_detected_nodes() {
    let td = std::env::temp_dir().join(format!("test-detect-probe-nodes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&td);
    let _ = std::fs::create_dir_all(&td);
    let root = Root::new(&td);

    // Mock CPU
    let proc_dir = root.p("/proc");
    let _ = std::fs::create_dir_all(&proc_dir);
    let _ = std::fs::write(
        proc_dir.join("cpuinfo"),
        "Processor: AArch64\nHardware : MT6765\n",
    );
    let _ = std::fs::write(
        proc_dir.join("version"),
        "Linux version 4.19.127 (builder@walton) #1 SMP PREEMPT\n",
    );

    // Mock Battery
    let bat_dir = root.p("/sys/class/power_supply/battery");
    let _ = std::fs::create_dir_all(&bat_dir);
    let _ = std::fs::write(bat_dir.join("capacity"), "85\n");
    let _ = std::fs::write(bat_dir.join("status"), "Discharging\n");

    // Mock Backlight
    let bl_dir = root.p("/sys/class/leds/lcd-backlight");
    let _ = std::fs::create_dir_all(&bl_dir);

    let audit = probe_all(&root);
    assert_eq!(audit.soc_model.as_deref(), Some("MT6765"));
    assert_eq!(audit.kernel_version, "4.19.127");
    assert_eq!(audit.battery_capacity, Some(85));
    assert_eq!(audit.battery_status.as_deref(), Some("Discharging"));
    assert_eq!(
        audit.display_backlight.as_deref(),
        Some("/sys/class/leds/lcd-backlight/brightness")
    );
}
