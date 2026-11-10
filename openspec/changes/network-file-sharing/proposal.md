# Network shares, SFTP and storage manager

## Why

Owner request 2026-11-10. Files only sees local storage; users expect Samba/NFS shares, SFTP and a storage screen.

## What changes

- Files: smb:// via gvfs (gvfs, gvfs-smb, samba-client) and NFS mounts (nfs-utils) with a Network places screen to add servers; credentials in credential files (0600), never argv.
- Kernel client check first: if CONFIG_CIFS is absent use FUSE/gvfs only and say so.
- SFTP: openssh-sftp-server so `sftp`/SFTP clients work against the tablet; Settings > Developer toggle for SSH (key-only).
- Optional Samba share of ~/Share (off by default, LAN only, password, firewall rule).
- Settings > Storage: internal/SD/USB usage, Eject, Format (confirm), free-up suggestions.

## Out of scope

USB gadget mass storage/MTP, Wi-Fi Direct, casting, KDE Connect.

## Device risk

Mounts remote filesystems: all operations need timeouts so a dead server cannot hang Files or the shell; format actions are destructive and need two confirmations. No flashing.

## Verification

On the tablet: add a Samba share, copy a file both ways, unplug the server and confirm the UI recovers in under 30 s; `sftp` into the tablet from a PC; eject a USB stick.

## Open questions

- Is CIFS/NFS client built into this kernel? Check /proc/filesystems on the tablet.
