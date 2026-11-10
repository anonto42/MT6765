# Notification centre

## Why

Owner request 2026-11-10. Apps, alarms, downloads and the battery have no way to notify except private toasts; apps expect org.freedesktop.Notifications.

## What changes

- The shell runs a session D-Bus (dbus-daemon --session started by walpad-session) and implements org.freedesktop.Notifications with zbus (pure Rust): Notify, CloseNotification, GetCapabilities, GetServerInformation, NotificationClosed, ActionInvoked.
- Heads-up banner (5 s), notification shade opened by pulling down on the header, grouping per app, swipe to dismiss, Clear all.
- Do Not Disturb toggle in Quick Settings (state in /var/lib/walpad/dnd); per-app mute list; urgency critical bypasses DND.
- `walpad-ctl notify <app> <title> <body>` for scripts and the core.

## Out of scope

Notification sounds (see display-sound-settings), lock-screen notification privacy (lock-screen change), push services.

## Device risk

No flashing or reboot. A new long-running D-Bus session process; bounded queue (100) so a noisy app cannot exhaust memory.

## Verification

`ssh walpad 'notify-send Hello World'` shows a banner; open the shade; tap it; enable DND and repeat.
