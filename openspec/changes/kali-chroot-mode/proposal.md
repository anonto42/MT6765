# Security-testing mode (Kali chroot)

## Why

Owner request 2026-11-10: use the tablet as a general-purpose Linux machine for penetration-testing practice on the owner's OWN equipment, labs and CTFs, with a Kali userland. A first prototype `walpad-kali` exists; this makes it a supported feature.

## What changes

- Harden and finish `walpad-kali`: install the NetHunter minimal arm64 rootfs into /opt/kali with checksum verification (or a debootstrap tarball built on a PC and verified), first-run setup, metapackage helper (`walpad-kali tools top10|headless|<group>`), status, safe unmount.
- Bind mounts for /dev, /dev/pts, /proc, /sys, /tmp (X11 socket), /run/walpad (ro), SD/USB under /mnt and a shared folder; DNS refreshed on network change.
- Terminal profile apps.json entries `kali-terminal` (st running walpad-kali shell) and a launcher for X11 tools with DISPLAY and XIM variables set.
- Apps-panel Tools entry and a Settings page `Security testing` with disk use, install/remove, a first-run notice (own equipment and authorised targets only), egress firewall profile.
- Document hardware limits in the UI: built-in Wi-Fi has no monitor mode; USB Wi-Fi adapter, HID gadget and USB serial need out-of-tree modules (separate follow-up changes).

## Out of scope

Attack procedures or scripts, custom kernel modules (USB Wi-Fi, usbserial, HID gadget tool walpad-usbmode: follow-up changes), GPU cracking, a full Kali desktop.

## Device risk

Writes only under /opt/kali; no flashing, no kernel modules. A chroot shares the kernel: root inside can affect the host (documented). The mode never starts at boot. Unmount must never `rm -rf` through a live bind mount (known risk): umount first, verify with /proc/mounts.

## Verification

On the tablet: `walpad-kali install` verifies the checksum, `walpad-kali setup`, `walpad-kali run nmap --version`, open `kali-terminal`, run a GUI tool (e.g. a small X11 app) and see it in the session, `walpad-kali umount` and check /proc/mounts is clean.

## Open questions

- Does PostgreSQL (msfdb) run in the chroot without SysV IPC? Does apt need extra network groups? Both unknown until tried.
