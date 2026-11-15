# Device Porting Guide for WalPad OS

## 1. Overview
This guide describes the formal procedure for porting WalPad OS to a new tablet, smartphone, or embedded ARM64 device.

## 2. Porting Workflow

### Step 1: Device Discovery
Extract hardware identity and hardware components:
```bash
# On target device running stock firmware or recovery:
cat /proc/cpuinfo
cat /sys/firmware/devicetree/base/compatible
ls -la /sys/class/input/input*/name
ls -la /sys/class/leds/
ls -la /sys/class/power_supply/
cat /proc/asound/cards
```
Document SoC model, RAM size, display resolution, touchscreen controller, Wi-Fi chip, and battery sysfs interfaces.

### Step 2: Bootloader & Partitioning
- Determine bootloader unlock procedure (e.g. `fastboot flashing unlock`).
- Inspect partition table (A/B Android partition scheme vs. standard GPT).
- Identify boot partition (`boot_a` or `boot`) and rootfs target partition (`userdata` or `system`).

### Step 3: Kernel & Devicetree Enablement
- Obtain vendor kernel source or mainline Linux kernel with SoC devicetree.
- Ensure kernel has `CONFIG_VT`, `CONFIG_FB`, `CONFIG_INPUT_EVDEV`, `CONFIG_DEVTMPFS`.
- Package kernel and initramfs into target boot format (e.g. Android `boot.img` or U-Boot `uImage`).

### Step 4: Hardware Enablement
1. **Display & Backlight**: Verify `/dev/fb0` or DRM/KMS and verify backlight sysfs control.
2. **Touchscreen**: Verify `/sys/class/input/input*/name` and test events with `evtest`.
3. **Power & Battery**: Verify `/sys/class/power_supply/battery/capacity`.
4. **Audio**: Identify ALSA card ID and codec mixer controls.
5. **Network**: Verify Wi-Fi interface name (`wlan0`) and firmware loading.

### Step 5: Profile Creation
Create the platform configuration files:
```
platforms/<vendor>/<device-codename>/
├── device.toml
├── hardware.toml
├── display.toml
├── input.toml
├── network.toml
└── power.toml
```

### Step 6: Verification with `walpad-device-detect`
Run the detection tool on target device:
```bash
walpad-device-detect
walpad-device-detect --json
```
Verify that all subsystems report as detected and matched to your profile.

## 3. Hardware Acceptance Checklist

| Subsystem | Requirement | Pass Criteria |
| :--- | :--- | :--- |
| **CPU / Boot** | Cold boot & recovery | Boots into WalPad OS shell within 30s |
| **Display** | Resolution & brightness | Native resolution, backlight adjusts 0-255 |
| **Touch** | Accurate multi-touch | Single tap, drag, double tap work accurately |
| **Keys** | Power & Volume keys | Power toggles screen; volume changes audio level |
| **Wi-Fi** | Scanning & association | Scans SSIDs, connects, acquires DHCP IP |
| **Audio** | Speaker & headphone | YouTube / media audio plays cleanly without underruns |
| **Battery** | Fuel gauge & charging | Reads capacity percentage and charge state |
| **Suspend** | Deep idle | Powersave governor engaged, backlight off |
