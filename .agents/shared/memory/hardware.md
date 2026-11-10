# Hardware

- SoC MT6765 (Helio P35), aarch64; kernel 4.19.127 (Android clang 11), config
  `data/kernel/walpad-4.19.127.config`, source `tools/kernel-mt6765-4.19`.
  No USER_NS, SYSVIPC, CGROUP_DEVICE/PIDS: no Docker/Podman, no local PostgreSQL.
  Has binder(fs), ION, V4L2 core, MTK camera/ISP drivers, MODVERSIONS=y, BT=y (no HCI bridges).
- eMMC `mmcblk0`: userdata = p44 (ext4, resized to ~53 GiB on first boot), super = p40
  (Android logical partitions mounted ro via dmlinear: system_a, vendor_a, product_a),
  NV: nvram p15, nvdata p6, nvcfg p5, protect1 p9, protect2 p10, proinfo p14, persist p12.
  NV backups: `firmware/MT6765/backups/modem-20261110/` (+ tablet `/mnt/sd/walpad-backup`).
- SD card `mmcblk1`; USB OTG host via musb (role switches on the ID pin).
- Panel `/dev/fb0` 800x1280, 32 bpp BGRA, 3 pages; present with sysfs pan `0,1280` then `0,0`;
  wake once with blank 1 then 0 (repeated blanking resets touch). Backlight
  `/sys/class/leds/lcd-backlight/brightness`.
- Input (find by name, never by event number): HALL_DEV, mtk-kpd (volume), mtk-pmic-keys (power),
  kxd (accelerometer), mtk-tpd (Silead GSL touch, registers late).
- Audio: legacy mt-soc driver, card 0 `mt-snd-card`; only 44.1 kHz with 25 ms/100 ms
  period/buffer is safe (other settings hung the tablet; watchdog reboot).
- Wi-Fi: MTK CONNSYS gen4m (wlan0, ap0, p2p0); interface combos do not allow AP + STA together.
- Modem: CCCI, LK-loaded MD1 `1_ulwtg_n`, firmware MOLY.LR12A.R3.MP.V175.4, dual-SIM capable.
- Recovery addresses on USB: 192.168.42.1 / 192.168.43.1 (host side needs root; host->tablet
  USB ethernet never transmitted). Unauthenticated root telnetd on port 23 on USB only.
