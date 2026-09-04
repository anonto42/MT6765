# WalPad 8G Infrastructure Context

This repository brings up a Walton WalPad 8G tablet as a small native VPS.
It contains the recovery ramdisk, Wi-Fi userspace, persistent Alpine payload,
display dashboard, service supervisor, and deployment helpers. The companion
application repository is `../e_commerce` (IchiBazar).

## Hardware and network

- SoC/architecture: MediaTek MT6765, ARM64 (`aarch64`).
- Stock kernel: 4.19.127; it lacks `CONFIG_SYSVIPC`,
  `CONFIG_CGROUP_DEVICE`, `CONFIG_CGROUP_PIDS`, and `CONFIG_USER_NS`.
- Internal persistent VPS storage: `/dev/mmcblk0p44`, mounted at `/mnt/vps`.
- SD boot/payload storage: `/dev/mmcblk1p1`, mounted at `/mnt/sd`.
- Wi-Fi interface: `wlan0`; DHCP address changes. Use `make wifi-find` or
  `bash scripts/wifi_find.sh --ip-only` to discover it.
- USB control addresses on the tablet are normally `192.168.42.1` and
  `192.168.43.1`; host-side USB addresses require administrator privileges.

## Persistent boot flow

`scripts/wifi_boot.sh` mounts the SD card, restores SSH identity, starts the
MediaTek Wi-Fi stack, associates using `/mnt/sd/wifi/wifi.conf`, obtains DHCP,
and starts Dropbear SSH. `scripts/wifi_assoc.sh` then starts the persistent VPS
boot script. `scripts/vps_boot.sh` mounts `/mnt/vps`, repairs the service bundle,
and starts `/opt/walpad/services` after every reboot or power-on.

`scripts/wifi_watchdog.sh` is installed on the SD card and retries association
and DHCP approximately every 30 seconds when Wi-Fi disappears. Keep this
watchdog dependency-free and avoid starting duplicate VPS supervisors during a
reconnect.

The display dashboard is the default framebuffer owner and refreshes metrics
continuously. The interactive terminal UI must be selected explicitly; never
run dashboard and terminal framebuffer writers simultaneously.

## Native service stack

Service definitions live in `config/services/` and deploy to
`/mnt/vps/etc/walpad/services.d/`. The supervisor is
`/mnt/vps/opt/walpad/services`.

Production services:

- `caddy`: public HTTP entry point on port 80.
- `ecommerce-frontend`: Next.js on port 3001.
- `ecommerce-backend`: Go API on port 8080.
- `cloudflared`: direct tablet Cloudflare Tunnel connector.
- `dashboard`, `demo`, `power-button`, and recovery services.

Caddy routes `/` to Next.js and `/api/*` plus `/ws` to the Go API. Verify both
the web and API routes after changes:

    curl -I https://ichibazar.com/
    curl https://ichibazar.com/api/v1/geo/locale

## Database and container limitations

PostgreSQL and Redis run on the development host, not inside the tablet:

- PostgreSQL: host `192.168.0.111:5435`, database `ylop_db`.
- Redis: host ports configured in the tablet backend `.env`.

The stock kernel cannot run PostgreSQL locally because shared memory support is
disabled. Podman/Docker also fail with cgroup and user-namespace errors. Do not
claim local containers or local PostgreSQL work unless the custom kernel has
actually been booted. The custom kernel image is
`build/persistence/boot-container-kernel-ramboot.img`; booting it currently
requires a hardware BROM/test-point procedure. Never flash or erase partitions
without explicit user approval.

## Cloudflare Tunnel

`cloudflared` is installed as an ARM64 binary at
`/mnt/vps/usr/local/bin/cloudflared`. Its token is stored mode 600 at
`/mnt/vps/etc/cloudflared/tunnel-token`; never print, commit, or copy the token
into source control. The published route must be:

    ichibazar.com -> http://127.0.0.1:80

Only the tablet replica should be active. A stale Debian/amd64 connector caused
502 responses and was disabled on the host.

## Application deployment

The IchiBazar repository contains `scripts/deploy-tablet.sh` and a versioned
`.githooks/post-push` hook. A deployment transfers source while preserving the
tablet backend `.env`, builds Next.js on the tablet, cross-compiles the Go API
for ARM64, transfers the binary, and restarts native services. Manual example:

    TABLET_IP=192.168.0.112 bash ../e_commerce/scripts/deploy-tablet.sh

When modifying deployment scripts, test source transfer, frontend build,
backend restart, and both public URLs. The tablet's persistent secrets and
database settings must survive source updates.

## Safety rules

- Preserve unrelated worktree changes; do not reset or force-push.
- Do not use Docker/Podman assumptions for the tablet stock kernel.
- Do not expose PostgreSQL, Redis, or API port 8080 directly to the Internet.
- Treat SSH passwords, JWT secrets, database credentials, and Cloudflare tokens
  as sensitive.
- Prefer reversible changes and verify the tablet remains reachable after every
  service, Wi-Fi, or boot modification.
