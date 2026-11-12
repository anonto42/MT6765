# Decisions (settled; do not re-debate without the owner)

- 2026-11-10: the Python runtime is dropped on the device (no-Python image); Rust core/shell are
  the runtime. Python stays in the source tree for reference only.
- 2026-11-10: UI = the owner's line-art design (left sidebar, header pill, quick settings, apps
  panel with close badges and Tools) on the owner's coffee wallpaper; translucent component fills.
- 2026-11-10: system keyboard and mouse pad drawn by the shell (override-redirect strips), keys and
  clicks through XTEST; matchbox-keyboard is not used.
- 2026-11-10: hotspot pauses the Wi-Fi client (chip cannot run AP + STA).
- 2026-11-10: modem NV partitions were backed up before the vendor modem daemons were allowed to
  write to them (owner approved "do what you want for sim").
- Camera: no shortcut; only via the Android camera HAL (Halium-style) or a USB webcam module.
- 2026-11-12: only the ZT9101 USB Wi-Fi dongle has a driver built/verified for WalPad so far
  (`openspec/changes/zt9101-usb-wifi-driver/`, `scripts/build_zt9101_module.sh`,
  `scripts/walpad-usbmode`). If the owner buys a different USB Wi-Fi adapter (e.g. an
  RTL8812BU/RTL8811CU/other chipset for dual-band or stronger monitor/injection support), it is
  NOT expected to work out of the box -- it needs its own from-scratch build-and-verify pass,
  same shape as the ZT9101 one (and possibly harder: see `memory/known-issues.md`'s ZT9101 entry
  for why "well-supported on modern Kali kernels" does not imply easy to port to this device's
  frozen 4.19.127 kernel). Do this only when the owner actually has the hardware in hand and asks
  for it; get the exact chip ID (same lsusb/VID:PID method as `memory/usb-dongle-identify.md`)
  and its driver source first, then assess the real porting effort before starting, don't assume
  it will be quick. Once a `.ko` is built and verified, add it to `scripts/walpad-usbmode` as a
  new `PROFILE_*` block (the script is already structured for more than one adapter).
