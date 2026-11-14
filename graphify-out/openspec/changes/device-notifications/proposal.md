# Device event notifications

## Why

Owner request 2026-11-12: "i want a notification type system that it will notify every events
like charging, any otg or other thinks was connected".

Not the same as the existing `openspec/changes/notification-centre` proposal: that one is a
general org.freedesktop.Notifications D-Bus service for apps (banners, DND, per-app mute) and is
still unimplemented. This change is smaller and does not touch it: a history of device/status
events the daemon itself already observes (or can observe with a small, new sysfs reader), shown
as a bell + badge + plain list. If the D-Bus change is built later, it can feed the same store
(or be merged with it); out of scope here.

## What changes

- walpad-core: a bounded ring buffer (100) of notifications `{id, ts_ms, category, title, body,
  read}`, generated from status transitions the daemon already tracks (battery charging,
  Bluetooth connect/disconnect, mobile data up/down, hotspot client count) plus two new, minimal
  sysfs readers in walpad-hw (`usb::attached_devices`, `usb::removable_mounts`) for USB/OTG device
  attach-detach and SD-card/USB-disk mount-remove, since nothing tracked those before. Persisted to
  `/var/lib/walpad/notifications.json` (atomic rename), so it survives at least a shell/core
  restart (in practice a reboot too: `/var/lib/walpad` is on the userdata partition, same as other
  saved state there).
- New IPC: `notifications` (list, newest first), `notification_mark_read {id}`,
  `notifications_mark_all_read`, `notification_clear {id}`, `notifications_clear_all`;
  `status.notifications.unread_count` for the badge; a `notice` event per new notification.
- walpad-shell: a bell icon in the header next to the Wi-Fi/battery marks with an unread badge
  (`phone_ui::paint_badge`, the same one the Phone tile uses), opening a new `Tab::Notifications`
  screen: a plain list, newest first, tap a row to mark it read, a Clear all button.
- walpad-ui: `Icon::Bell`.

## Out of scope

Day-grouping, swipe-to-clear, heads-up banners, Do Not Disturb, a generic app-facing notification
API (org.freedesktop.Notifications: see `notification-centre`), notification sounds. GPS and
hotspot-client notifications are included because the daemon already tracks those states, but are
not the owner's literal examples; charging and USB/OTG are the two the owner named.

## Device risk

None: no flashing, no partitions, no boot files. Two new sysfs readers
(`/sys/bus/usb/devices`, `/proc/mounts`), both read-only and Option-returning.

## Secrets impact

None: a notification body never carries an SSID, IMEI, phone number or secret (titles/bodies come
only from Bluetooth device name/address, a USB product name, a mount path, or a client count,
none of which this project treats as a secret).

## Verification

Host: pure diff-logic unit tests (sequences of synthetic snapshots), store persistence/ring-buffer/
corrupt-file tests, walpad-hw sysfs reader tests, IPC parse tests, shell snapshot test for the new
screen and the badge; `scripts/check.sh`. On the tablet (not done in this change): plug/unplug the
charger and a USB drive, pair a Bluetooth device, toggle mobile data, confirm each shows up and the
badge/unread count tracks it.
