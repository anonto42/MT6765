# WalPad OS Device Bundles

## Purpose
A **Device Bundle** contains all hardware-specific assets required to boot and run WalPad OS on a specific hardware platform:
- Target Linux kernel (`boot.img` or `zImage` / `Image.gz`)
- Device Tree Blobs (`.dtb`) and Device Tree Overlays (`.dtbo`)
- Proprietary firmware files and Wi-Fi/BT calibration records
- Target partition table mapping (`partitions.json`)
- Device profile (`device.toml`, `display.toml`, `input.toml`, `power.toml`)
- Model-specific initramfs scripts

## Bundle Layout
```
device-bundle/
├── bundle.toml       # Top-level bundle metadata & component paths
├── boot/             # Kernel binary and bootloader configuration
├── devicetree/       # Compiled DTB and overlays
├── firmware/         # Vendor firmware blobs
└── overlays/         # Rootfs file injections (e.g. /etc/walpad/device.toml)
```

## Creating a New Device Bundle
1. Create directory `images/device-bundles/<vendor>-<model>/`
2. Add `bundle.toml` specifying the kernel format, bootloader type, and target partitions.
3. Link or bundle target kernel image and firmware files.
4. Reference the device profile from `platforms/<vendor>/<model>/device.toml`.
