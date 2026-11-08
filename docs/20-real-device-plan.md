# Plan: make the WalPad 8G a real device (browser, terminal, Wi-Fi, touch UI)

Written 2026-11-08.  Status of the tablet and how we got here: `docs/99-status-log.md`.

## Decision and honest expectations

Android could not be restored without the official Walton boot images (see the status log), so
this plan turns the tablet into a **Linux tablet**: a touch launcher, a web browser, a terminal and
Wi-Fi settings, all driven by touch and the hardware keys.  It will *not* run Android apps.

Hard limits that come from the hardware and the stock 4.19.127 kernel
(`data/kernel/walpad-4.19.127.config`):

| Area | Fact | Consequence |
|---|---|---|
| Display | legacy `MTK_FB` framebuffer (`/dev/fb0`, 800x1280, BGRA, 3 pages); DRM is enabled but there is no MediaTek KMS driver | **X11 + xf86-video-fbdev**; no Wayland |
| GPU | PowerVR kernel driver only; user-space blobs are Android-only | software rendering; a browser will be slow-ish but usable |
| Touch | `TOUCHSCREEN_MTK_GSL3670_T805_HD` on `/dev/input/event4` | works through evdev/libinput once the panel is awake |
| Sandboxing | `USER_NS` and `SYSVIPC` are off; PID/NET/UTS namespaces, cgroups, overlayfs, FUSE, zram, seccomp are on | browsers must run with their sandbox disabled; X must not use MIT-SHM |
| Wi-Fi / Bluetooth | MediaTek CONNSYS 6765 (`MTK_COMBO_WIFI`, `MTK_BTIF`) | needs the vendor modules/firmware/launcher; the repo already has a working bring-up |
| Audio | ALSA/ASoC present, MediaTek audio needs vendor setup | to be verified on the device |
| Camera | MTK ISP drivers only, needs the Android camera HAL | **not planned** |
| Storage | `userdata` (p44, ~53 GB, erased) is free for a rootfs; `super` holds repaired Android system/vendor/product | `vendor_a` supplies Wi-Fi modules and firmware |

Things that need the user: confirm the Linux path; physically reseat the SD card (it is not detected);
type Wi-Fi credentials on the tablet itself (never stored in the repo).

## Phases

Each phase ends with a check that can be repeated.  Flash only with the procedure in `AGENTS.md`
(preloader window, read back and compare hashes).  `boot_b` stays untouched as the fallback.

### Phase 0 - finish the framebuffer UI (in progress)
- Fix touch (the debug strip on screen counts events per input device; read it after a test).
  Candidates: panel needs a different wake sequence, events come from another node, or the
  controller firmware (`gsl_load_fw`) has not finished.
- Battery: gauge reports -1; keep the voltage estimate; investigate `fuel gauge` calibration later.
- **Done when:** tiles respond to touch, finger painting works, volume/power keys behave.

### Phase 1 - persistent rootfs on `userdata`
- Build an Alpine **aarch64** rootfs on the host (`apk --arch aarch64 --root ... --no-scripts` in
  an x86 Alpine container, or an arm64 container if binfmt is available), pack it into an ext4 image
  made with features the 4.19 kernel supports, write it to p44 with mtkclient, grow it on the device.
- New boot ramdisk `/init`: mount `userdata`, bind `/system` `/vendor` `/product` from `super`
  with `tools/dmlinear` (proven), `switch_root` into Alpine; keep USB gadget (serial + ethernet) and
  an SSH/telnet path for debugging; fall back to the current ramdisk on any failure.
- zram swap + a small swapfile for the browser.
- **Done when:** the tablet boots into Alpine, `padui` runs from it, files survive a reboot.

### Phase 2 - Wi-Fi
- Reuse `scripts/mtk_stack_up.sh`, `scripts/wifi_assoc.sh`, `tools/propshim` and the musl
  `wpa_supplicant` bundle; modules and firmware now come from the mounted `vendor_a` instead of the SD card.
- Credentials live in `/etc/wpa_supplicant/` on `userdata` (mode 600).
- **Done when:** after a cold boot the tablet associates on its own and `ping 1.1.1.1` works.

### Phase 3 - touch Wi-Fi settings and keyboard
- Settings screen: scan (`wpa_cli scan_results`), pick a network, type the password on an
  on-screen keyboard, connect, show signal and IP.  First version inside `padui` (framebuffer, no
  X needed); later a Tk/GTK version inside the X session.
- **Done when:** a new network can be joined with touch only, and it reconnects after reboot.

### Phase 4 - graphical session, browser, terminal
- X11: `xorg-server` + `xf86-video-fbdev` + `xf86-input-libinput` (touch), no MIT-SHM, `matchbox-window-manager`
  (single-window, touch friendly), an on-screen keyboard (`matchbox-keyboard` or `onboard`).
- Browser: **Firefox ESR** first (`MOZ_DISABLE_CONTENT_SANDBOX=1`); measure RAM/CPU; fallback NetSurf
  for light pages.  Chromium only if Firefox is too heavy (needs `--no-sandbox`).
- Terminal: `st` or `xterm` with the on-screen keyboard; keep dropbear for remote access.
- Launcher: a fullscreen home with tiles (Browser, Terminal, Wi-Fi, Settings) and a status bar
  (battery, Wi-Fi, time); hardware keys: power = screen off/on, volume = brightness/volume, a daemon maps them.
- **Done when:** the tablet opens a website over Wi-Fi with touch only and a shell can be typed in.

### Phase 5 - the rest of the hardware
- Audio (`/dev/snd`, mixer, speaker/headphone), rotation from the accelerometer, battery/charging
  indicators, backlight policy and screen timeout, suspend (`PM_SLEEP` is on, wake sources to test),
  Bluetooth through the CONNSYS stack, SD card, USB OTG, Wi-Fi power saving.
- **Done when:** each item is either working or documented as unsupported.

### Phase 6 - hardening
- Hardware watchdog kept alive by a supervisor, restart of the UI on crash, a safe-mode boot to
  the current ramdisk (hold a key at boot), documented recovery steps, reproducible builds
  (`scripts/build_ui_boot.sh` and a new rootfs build script), backups of every flashed partition.

## Parallel track: official firmware (best outcome)
A real Walton boot image (`boot.img`, `vendor_boot.img`, `vbmeta*.img`, `dtbo.img`) would make the
repaired `super` boot Android, giving a real browser, apps, camera and audio.  If Walton support (or another
WalPad 8G owner) can provide the build `WALPAD8G_V2 ... mp1V915`, flashing it supersedes phases 1-5.
Keep asking while the Linux path proceeds; nothing in the Linux plan blocks it (`super` is already repaired).

## Risks
- Browser speed without GPU acceleration on Cortex-A53; may need a lighter browser.
- Wi-Fi bring-up is fragile (CONNSYS init order, no Android property service); it worked before,
  so reuse the exact sequence in the status log.
- Touch wake quirk: blanking the panel repeatedly resets the controller; blank once only.
- Flashing mistakes: always read back and compare; one mtkclient process at a time.
- `userdata` was erased; there is no copy of the old VPS.
