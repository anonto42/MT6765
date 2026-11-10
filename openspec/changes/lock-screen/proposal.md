# Lock screen and screen lock

## Why

Owner request 2026-11-10. A tablet that leaves the house needs a PIN or password; today there is none and the Security page only reports exposure.

## What changes

- Core stores a salted PBKDF2-HMAC-SHA256 (or argon2) hash in /var/lib/walpad/lock.json (0600); verification only in the core; attempts are rate limited (5 tries, then 30 s doubling).
- Shell lock window: fullscreen override-redirect with keyboard and pointer grab, clock, PIN pad, password through the system keyboard; optional 3x3 pattern.
- Auto-lock after the screen has been off for N seconds (default immediately); short power press locks; boot comes up locked when a lock is set.
- Settings > Security: set/change/remove lock (current secret required), auto-lock delay.

## Out of scope

Full-disk encryption, screen pinning, multi-user, biometrics. The recovery telnetd on USB (frozen boot image) is outside the lock and the UI SHALL say so.

## Device risk

No flashing or reboot needed. A bug can lock the owner out: a rescue path (SSH `walpad-ctl lock_clear`, USB recovery) is part of the tasks.

## Verification

On the tablet: set PIN, press power, wake, try a wrong PIN (delay), right PIN; reboot and confirm it boots locked; remove the lock with the rescue command over SSH.

## Open questions

- Does XGrabKeyboard stay effective across openbox focus changes on this X setup? Verify on the tablet.
