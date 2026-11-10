# Project Memory (WalPad 8G)

## Goal

Walton WalPad 8G (MediaTek MT6765, aarch64) as a standalone Linux tablet: touch, buttons,
Wi-Fi, browser, terminal, files, SIM. Android cannot be restored (no official boot images).
IchiBazar is no longer hosted here; do not restore Caddy/Cloudflare/Redis/Next/Go unless asked.

## Shape

- Boot: `boot_a` = `build/android-restore/boot_a-os.img` (ramdisk `config/boot/init-os` mounts
  `userdata` and runs `/sbin/walpad-init` in a **chroot**; kernel threads keep the ramdisk root).
- Root fs: Alpine 3.24 on `userdata`, built by `scripts/build_rootfs.sh`
  (`WALPAD_NO_PYTHON=1` = the image in use: Rust runtime, no Python).
- Rust workspace `crates/`: walpad-core (device daemon, IPC `/run/walpad/control.sock`),
  walpad-shell (touch UI, tiny-skia, X client), walpad-ui, walpad-proto, walpad-ctl, walpad-x.
- C tools: `tools/fbmirror` (Xvfb -> /dev/fb0), `tools/propshim[32]` (Android property shim),
  `tools/padui` (rescue UI), `tools/dmlinear`.
- Session: Xvfb 1280x1280 + openbox + walpad-shell; apps by id from `/etc/walpad/apps.json`
  (browser, terminal `st`, files `pcmanfm`).
- Plans/contracts: `docs/rust/CONTRACTS.md` (IPC), `docs/rust/UI.md`, `docs/rust/TASKS.md`,
  `docs/rust/FLASH-RUNBOOK.md`; history: `docs/99-status-log.md`.

## Device state (2026-11-10)

- Running the no-Python image; updates go over SSH (no reflash needed). IP changes with DHCP:
  `build/ssh/config` host `walpad` (separate known_hosts).
- Works: touch, Wi-Fi (firmware via /proc/2/root bind), time (NTP + RTC, Asia/Dhaka), SD card +
  USB disks (`walpad-storage`, exFAT/NTFS via FUSE), speaker (`walpad-audio`, asound.conf),
  terminal (devpts), system keyboard, mouse pad, wallpaper UI, Wi-Fi switch + hotspot code.
- Modem: `walpad-modem start` boots MD1; AT on `/dev/radio/pttycmd1`; SIM ready, registered
  (Grameenphone); PDP context gets an IP but ccmni traffic does not flow yet (NetAgent
  `AT+EIF ... ifst` confirm unknown).
- GPS: `walpad-gps start` runs mnld; positions not exported yet (mnl2hal socket protocol).
- Not possible on this kernel without new modules: Bluetooth (needs hci_vhci.ko), camera (HAL
  only); research and agents in `history/` handoffs.

## References (load when relevant)

- Hardware, partitions, input, display: `memory/hardware.md`
- Known issues and traps: `memory/known-issues.md`
- Settled decisions: `memory/decisions.md`
- Legacy SD-card/server flows (inactive): `memory/legacy.md`
- Daily history: `history/YYYY-MM-DD/`
