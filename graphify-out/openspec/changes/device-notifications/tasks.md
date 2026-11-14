## 1. Core and contract
- [x] 1.1 walpad-proto: `Notification`, `NotificationCategory`, `NotificationsStatus`,
      `status.notifications`, the five commands; CONTRACTS.md
- [x] 1.2 walpad-hw `usb` module: `attached_devices`, `removable_mounts` (new; nothing tracked
      USB/OTG or removable storage before)
- [x] 1.3 walpad-core `logic::notifications::diff` (pure) + `notifications::Notifications` (ring
      buffer, persistence, IPC glue), wired into the daemon's poll loop
- [x] 1.4 Unit tests: diff sequences, store persistence/ring-buffer/corrupt-file, usb readers, IPC
      parsing

## 2. Shell
- [x] 2.1 `Icon::Bell`, `Tab::Notifications`
- [x] 2.2 Bell + badge in the header, minimal notification-centre screen (list, tap to mark read,
      Clear all)
- [ ] 2.3 Snapshot golden for the new screen and the badge

## 3. Check
- [ ] 3.1 `scripts/check.sh` prints ALL CHECKS PASSED

## 4. Device (needs the tablet; no flashing; not done in this change)
- [ ] 4.1 Plug/unplug the charger and a USB drive, pair Bluetooth, toggle mobile data: each shows
      up and the badge tracks it
