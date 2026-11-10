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
