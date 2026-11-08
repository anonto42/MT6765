# WalPad 8G Context

## Project goal (updated 2026-11-08)

Use the Walton WalPad 8G as a **real standalone device**: touch display, hardware buttons, Wi-Fi,
a web browser and a terminal.  Android could not be restored without the official Walton boot
images, so the path is a Linux tablet; the plan and its phases are in
`docs/20-real-device-plan.md`, and the full history and root causes are in `docs/99-status-log.md`.

**IchiBazar is no longer hosted on this tablet.**  Do not deploy it or restore the Caddy/Cloudflare/
Redis/Next.js/Go stack unless the user asks.

## Current state (2026-11-08)

- `boot_a` = BusyBox/dropbear server ramdisk + `padui` (framebuffer touch UI).  `boot_b` = unmodified
  server ramdisk (fallback).  `super` = repaired stock Android system/vendor/product (not booted).
- Works: display, hardware buttons.  Not working: touch (debugging), battery gauge (`-1`, estimated
  from voltage).  `userdata` and `metadata` were erased; the Alpine VPS and its services are gone;
  the SD card is not detected.
- Build the image with `scripts/build_ui_boot.sh firmware/MT6765/backups/boot_a-original.img <out>`;
  `tools/padui` and `tools/dmlinear` are static aarch64 C built by `build.sh` in a throwaway
  Alpine docker container.

## Flashing and recovery (read before touching the tablet)

- **Never flash or erase a partition without explicit user approval.**  Always read back and compare
  hashes after a write: a write after a stale session can report success without landing.
- mtkclient works in **preloader mode, no preloader file needed**: start ONE `tools/mtk/mtk.py`
  command, then unplug/replug the USB cable with no buttons held (or hold power 15 s while the tablet
  runs).  The window stays open for several commands; `mtk.py reset` leaves it.  Never run two
  mtkclient processes at once.
- Fastboot (`0e8d:201c`, bootloader unlocked) is reachable; there is no `fastboot` binary, use
  `scripts/fastboot_py.py`.
- Backups: `firmware/MT6765/backups/` (`boot_a-original.img` is the server image, not Android;
  `boot_a-pre-android-restore.img`), `build/android-restore/super-sparse-as-found.img` (the original `super`).
- Serial console wedges after large transfers and the host side of USB ethernet never transmits; use
  mtkclient reads for anything big.

## Hardware and network

- SoC/architecture: MediaTek MT6765, ARM64 (`aarch64`).
- Stock kernel: 4.19.127; it lacks `CONFIG_SYSVIPC`,
  `CONFIG_CGROUP_DEVICE`, `CONFIG_CGROUP_PIDS`, and `CONFIG_USER_NS`.
- Internal persistent storage: `/dev/mmcblk0p44`, mounted at `/mnt/vps`.
- SD boot/payload storage: `/dev/mmcblk1p1`, mounted at `/mnt/sd`.
- Wi-Fi interface: `wlan0`; DHCP address changes. Use `make wifi-find` or
  `bash scripts/wifi_find.sh --ip-only` to discover it.
- USB control addresses on the tablet are normally `192.168.42.1` and
  `192.168.43.1`; host-side USB addresses require administrator privileges.

## Input and display

- Panel: `/dev/fb0`, 800x1280, 32 bpp memory order B,G,R,A, three pages (`PAGE = 800*1280*4`);
  present with sysfs `pan` `0,1280` then `0,0`; wake the panel once with `blank` 1 then 0
  (repeated blanking resets the touch controller).  Backlight: `/sys/class/leds/lcd-backlight/brightness`.
- Input map: event0 HALL_DEV, event1 mtk-kpd (volume keys), event2 mtk-pmic-keys (power), event3 kxd-kxd,
  event4 mtk-tpd (Silead GSL3670 touch, native 800x1280, ABS_MT_POSITION_X/Y).
- Only one framebuffer writer at a time (`padui`, or the legacy `display_dashboard.py` / `terminal_ui.py`).
- Kernel facts: no USER_NS, no SYSVIPC, no GPU user space, no camera HAL; evdev, uinput, ALSA, cfg80211,
  overlayfs, FUSE, zram and cgroups are enabled (`data/kernel/walpad-4.19.127.config`).

## Legacy boot flow (NOT active: needs the SD card and the erased `userdata`)

Kept as reference for phases 1-2 of `docs/20-real-device-plan.md`.

`scripts/wifi_boot.sh` mounts the SD card, restores SSH identity, starts the
MediaTek Wi-Fi stack, associates using `/mnt/sd/wifi/wifi.conf`, obtains DHCP,
and starts Dropbear SSH. `scripts/wifi_assoc.sh` then starts the persistent boot
script. `scripts/vps_boot.sh` mounts `/mnt/vps`, repairs the service bundle,
and starts `/opt/walpad/services` after every reboot or power-on.

`scripts/wifi_watchdog.sh` is installed on the SD card and retries association
and DHCP approximately every 30 seconds when Wi-Fi disappears. Keep this
watchdog dependency-free and avoid starting duplicate supervisors during a
reconnect.

## Legacy native service stack (NOT active: the Alpine VPS on `userdata` was erased)

Service definitions live in `config/services/` and deploy to
`/mnt/vps/etc/walpad/services.d/`. The supervisor is
`/mnt/vps/opt/walpad/services`.

Active services: `dashboard` (default framebuffer owner), `power-button`,
`demo`, and recovery services. `terminal` is opt-in (`AUTOSTART=0`).

Retired (config kept, `AUTOSTART=0`): `caddy`, `cloudflared`, `redis`,
`ecommerce-frontend`, `ecommerce-backend`. These only served IchiBazar.
Re-enable by removing the `AUTOSTART=0` line and redeploying.

Note: the public dashboard health checks (Caddy / Go API) in
`scripts/display_dashboard.py` will show DEGRADED while those services are
retired; the dashboard should be reworked to show device status instead.

## Kernel and container limitations

The stock kernel cannot run PostgreSQL locally because shared memory support is
disabled. Podman/Docker also fail with cgroup and user-namespace errors. Do not
claim local containers work unless the custom kernel has actually been booted.
The custom kernel image is `build/persistence/boot-container-kernel-ramboot.img`;
booting it currently requires a hardware BROM/test-point procedure. Never flash
or erase partitions without explicit user approval.

## Safety rules

- Preserve unrelated worktree changes; do not reset or force-push.
- Never flash or erase partitions without explicit user approval.
- Do not assume Docker/Podman on the stock kernel.
- Treat SSH passwords, Wi-Fi credentials, and any leftover Cloudflare tunnel
  token (`/mnt/vps/etc/cloudflared/tunnel-token`) as sensitive; never print or
  commit them. Consider deleting the token now that the tunnel is retired.
- The firmware runs an unauthenticated root telnetd on port 23; treat the LAN
  as exposed until it is closed.
- Prefer reversible changes and verify the tablet remains reachable after every
  service, Wi-Fi, or boot modification.
