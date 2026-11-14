# Clock, alarms, calculator and notes

## Why

Owner request 2026-11-10. A tablet needs alarms and timers that fire reliably, a calculator and notes; none exist.

## What changes

- Core scheduler (timerfd based, persisted in /var/lib/walpad/alarms.json) for alarms, timers and stopwatch; fires a notification and a sound through the safe audio path.
- Shell screens: Clock (alarms list with repeat days, timer, stopwatch), Calculator (standard + scientific), Notes (Markdown files under ~/Notes, list and editor with the system keyboard).
- Apps registered in apps.json as shell-native apps (ids clock, calculator, notes).

## Out of scope

Calendar and contacts (later change), suspend-wake alarms, weather widget.

## Device risk

No flashing. Suspend is not used yet: alarms rely on the tablet being awake (screen off is fine); RTC wakealarm is an open question.

## Verification

On the tablet: set an alarm 2 minutes ahead with the screen off, confirm it rings and the notification offers Snooze and Dismiss; compute 12*(3+4); create a note, reboot, reopen.

## Open questions

- Does the RTC wakealarm work for later suspend support?
