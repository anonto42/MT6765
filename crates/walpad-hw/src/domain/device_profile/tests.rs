use super::*;
use crate::Root;

#[test]
fn default_profiles_are_valid() {
    let walpad = DeviceProfile::default_walpad_8g();
    assert_eq!(walpad.id, "walton-walpad-8g");
    assert_eq!(walpad.tier, 1);
    assert_eq!(walpad.display.width, 800);
    assert_eq!(walpad.display.height, 1280);
    assert_eq!(walpad.capabilities.gnss, Capability::Supported);
    assert_eq!(walpad.capabilities.camera, Capability::Unsupported);

    let generic = DeviceProfile::generic_arm64();
    assert_eq!(generic.id, "arm64-generic");
    assert_eq!(generic.tier, 3);
    assert_eq!(generic.display.width, 1280);
    assert_eq!(generic.capabilities.gnss, Capability::Unknown);
}

#[test]
fn parse_custom_toml_profile() {
    let sample = r#"
[device]
id = "test-tab-10"
name = "Test Tablet 10"
manufacturer = "ExampleCorp"
model = "Tab 10"
codename = "tab10"
architecture = "aarch64"
soc = "unisoc-t618"
tier = 2

[display]
width = 1200
height = 1920
orientation = "landscape"
backlight = "/sys/class/backlight/panel/brightness"

[input]
backend = "linux-evdev"
touch_name_patterns = ["goodix-ts", "touchscreen"]
power_key_name = "gpio-keys"
volume_keys_name = "gpio-keys"

[network]
backend = "generic-nl80211"
wifi_device = "wlan0"
bt_device = "hci0"

[power]
backend = "linux-sysfs"
battery_capacity = "/sys/class/power_supply/bat/capacity"
battery_status = "/sys/class/power_supply/bat/status"

[audio]
codec = "rt5651"
audio_script = "/usr/local/sbin/tab-audio"

[capabilities]
camera = "supported"
gnss = "unsupported"
modem = "unsupported"
accelerometer = "supported"
bluetooth = "supported"
"#;

    let p = DeviceProfile::parse_toml(sample).expect("parse failed");
    assert_eq!(p.id, "test-tab-10");
    assert_eq!(p.manufacturer, "ExampleCorp");
    assert_eq!(p.soc, "unisoc-t618");
    assert_eq!(p.tier, 2);
    assert_eq!(p.display.width, 1200);
    assert_eq!(p.display.height, 1920);
    assert_eq!(p.display.orientation, "landscape");
    assert_eq!(
        p.display.backlight_path,
        "/sys/class/backlight/panel/brightness"
    );
    assert_eq!(
        p.input.touch_name_patterns,
        vec!["goodix-ts", "touchscreen"]
    );
    assert_eq!(p.capabilities.camera, Capability::Supported);
    assert_eq!(p.capabilities.gnss, Capability::Unsupported);
    assert_eq!(p.capabilities.bluetooth, Capability::Supported);
}

#[test]
fn detect_profile_from_root() {
    let td = crate::testutil::tmp();
    let root = Root::new(&td);

    // Empty root falls back to generic arm64
    let detected = DeviceProfile::detect(&root);
    assert_eq!(detected.id, "arm64-generic");

    // Root with MT6765 cpuinfo detects walpad-8g
    crate::testutil::put(&root, "/proc/cpuinfo", "Hardware : MT6765\n");
    let detected = DeviceProfile::detect(&root);
    assert_eq!(detected.id, "walton-walpad-8g");

    // Root with explicit device.toml overrides everything
    crate::testutil::put(
        &root,
        "/etc/walpad/device.toml",
        "[device]\nid = \"custom-override\"\ntier = 2\n",
    );
    let detected = DeviceProfile::detect(&root);
    assert_eq!(detected.id, "custom-override");
    assert_eq!(detected.tier, 2);
}

#[test]
fn real_platform_files_parse() {
    let walton = DeviceProfile::load_from_file(Path::new("platforms/walton/walpad-8g/device.toml"))
        .or_else(|_| {
            DeviceProfile::load_from_file(Path::new("../../platforms/walton/walpad-8g/device.toml"))
        })
        .expect("walton profile parse failed");
    assert_eq!(walton.id, "walton-walpad-8g");
    assert_eq!(walton.tier, 1);

    let arm64 = DeviceProfile::load_from_file(Path::new("platforms/arm64-generic/device.toml"))
        .or_else(|_| {
            DeviceProfile::load_from_file(Path::new("../../platforms/arm64-generic/device.toml"))
        })
        .expect("generic profile parse failed");
    assert_eq!(arm64.id, "arm64-generic");
    assert_eq!(arm64.tier, 3);

    let samsung =
        DeviceProfile::load_from_file(Path::new("platforms/samsung/example-tablet/device.toml"))
            .or_else(|_| {
                DeviceProfile::load_from_file(Path::new(
                    "../../platforms/samsung/example-tablet/device.toml",
                ))
            })
            .expect("samsung profile parse failed");
    assert_eq!(samsung.id, "samsung-example-tablet");

    let lenovo =
        DeviceProfile::load_from_file(Path::new("platforms/lenovo/example-tablet/device.toml"))
            .or_else(|_| {
                DeviceProfile::load_from_file(Path::new(
                    "../../platforms/lenovo/example-tablet/device.toml",
                ))
            })
            .expect("lenovo profile parse failed");
    assert_eq!(lenovo.id, "lenovo-example-tablet");
}
