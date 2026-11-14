# Launcher and home screen

## Why

Owner request 2026-11-10. Home is a static grid with no folders, search, widgets or wallpaper choice.

## What changes

- Home page with native widgets (clock/date, battery, next alarm, media controls, shortcuts) and an app drawer (A-Z) with a search field covering apps, settings entries and notes.
- Folders: long-press lift and drop one icon on another; folder popover; layout in /var/lib/walpad/launcher.json; icon reordering.
- Edit mode (long-press on empty space): widgets, wallpaper, home settings.
- Badge counts from the notification centre.

## Out of scope

Third-party widgets, weather (needs network fetch; later), file search index, icon packs.

## Device risk

No flashing or reboot. launcher.json must be written atomically and a corrupt file falls back to the default grid.

## Verification

On the tablet: create a folder from two apps, reboot, still there; search `wifi` opens Network; add and remove the clock widget; rotate.

## Open questions

- Should widgets resize in this change or later?
