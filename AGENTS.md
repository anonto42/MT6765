# WalPad 8G Context

## Project goal (updated 2026-11-08)

Use the Walton WalPad 8G as a **real standalone device**: touch display, hardware buttons, Wi-Fi,
a web browser and a terminal.  Android could not be restored without the official Walton boot
images, so the path is a Linux tablet; the plan and its phases are in
`docs/20-real-device-plan.md`, and the full history and root causes are in `docs/99-status-log.md`.

**IchiBazar is no longer hosted on this tablet.**  Do not deploy it or restore the Caddy/Cloudflare/
Redis/Next.js/Go stack unless the user asks.

## Current state (2026-11-08)

- `userdata` = **Alpine aarch64 rootfs** (`build/rootfs/rootfs.ext4`, built by `scripts/build_rootfs.sh`):
  Xvfb + openbox session, Firefox ESR, terminal, Wi-Fi UI, `walpad-devd` (auto-rotation, screen
  timeout, lid sensor, battery guard, power menu), `walpad-keys` (volume/power gestures),
  `walpad-touchd` (touch -> X), `fbmirror` (Xvfb -> `/dev/fb0`, rotation aware).
- `boot_a` = `boot_a-os.img` (ramdisk `config/boot/init-os` mounts `userdata` and switch_roots into
  Alpine; the old `padui` key UI is the fallback if the session is unhealthy).  `boot_b` = unmodified
  server ramdisk (fallback).  `super` = repaired stock Android (not booted).  `metadata` erased.
- Last flash 2026-11-08 (`scripts/flash_os.sh`, read-back verified): rootfs with the `fbmirror` fix
  that no longer blanks the panel on rotation.  **Unverified on the device:** that the rotation
  flicker is gone, Wi-Fi, and rotation direction (`rotate_flip` in `/etc/walpad/devd.json`).
- Touch works but is not accurate (optimise later).  Battery gauge reads `-1` (voltage estimate).
  SD card not detected.  Android cannot be restored without official Walton boot images.
- Flash both partitions with `scripts/flash_os.sh <rootfs.ext4> <boot_a image>` (explicit user approval
  and one cable replug).  To change a single file, patch the image with `debugfs -w` instead of a full
  rebuild.  `tools/padui`, `tools/fbmirror`, `tools/dmlinear` are static aarch64 C, built by their
  `build.sh` in a throwaway Alpine docker container.

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

## Rust migration rules (plan v2, 2026-11-09)

Plan docs: `docs/rust/CONTRACTS.md` (frozen interfaces), `docs/rust/TASKS.md` (cards), `docs/rust/UI.md`.

Hard rules
1. One task card per session. Do not start the next card until its done-when checks pass and the report is written.
2. Do not change anything in `docs/rust/CONTRACTS.md` unless the card says so explicitly.
3. Never run `mtk.py`, `flash_os.sh`, `flash_partition.sh`, `fastboot_py.py` or write to a block device. Prepare the command; the owner runs it.
4. Never edit `config/boot/*`, `tools/propshim`, `scripts/mtk_stack_up.sh` or the CONNSYS section of `walpad-wifi`.
5. Python stays the default runtime until a Rust component passes its shadow-mode diff and device checklist. Switching is a one-line change in `/etc/walpad/runtime.conf`, never a deletion.
6. No secret in logs, argv, fixtures or commits. Wi-Fi passphrases go through the wpa_supplicant control socket only; log calls that could carry one use `redact()`.
7. No arbitrary command execution. Apps launch by ID from `/etc/walpad/apps.json`.
8. Every hardware read returns Option/Result; missing sensors are "unavailable", never guessed. The battery estimate is allowed only tagged `source: "voltage_estimate"`.
9. Every blocking operation (spawn, socket, sysfs write, X request) has a timeout.
10. New dependencies need a one-line justification in the report and must be pure Rust (no -sys crates) unless the card allows it.

Stop and ask the owner when: a done-when check needs the tablet ([DEVICE] cards: write the script, give the command, wait for output); a contract looks wrong; real code contradicts the plan (record it, code wins); a fix would touch the boot image, partitions or vendor files.

Session protocol: read plan sections + the card + listed files; state the plan in <=5 lines; implement (<~600 line diffs); run `scripts/check.sh`; write `docs/rust/reports/<card-id>.md` (Status, Files changed, What works, Checks run, Device checks pending, Measurements, Deviations, Rollback, Next card) and update `docs/rust/TASKS.md`.
