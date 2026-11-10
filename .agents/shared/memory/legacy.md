# Legacy flows (inactive, kept as reference)

- SD-card Wi-Fi boot: `scripts/wifi_boot.sh`, `wifi_assoc.sh`, `wifi_watchdog.sh` (needs the SD card
  layout under `/mnt/sd/wifi`), `scripts/vps_boot.sh` and the service supervisor in
  `config/services/` (dashboard, power-button, retired IchiBazar services with AUTOSTART=0).
- The Alpine VPS on `userdata` they used was erased; the public dashboard checks would show
  DEGRADED. The leftover Cloudflare tunnel token (if any) is a secret: delete, never print.
- Custom kernel `build/persistence/boot-container-kernel-ramboot.img` needs a BROM/test-point boot.
