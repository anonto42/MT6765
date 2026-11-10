# Display and sound settings

## Why

Owner request 2026-11-10. Settings only has brightness and rotation; there is no volume UI, wallpaper chooser, text size or night light.

## What changes

- Settings > Display: wallpaper chooser (built-ins and ~/Pictures), text size (small/normal/large) as a UI scale factor in walpad-ui tokens, light/dark token sets, night light (warm overlay with schedule), brightness schedule (no light sensor).
- Settings > Sound: Media, Alarm, Notification volumes using three softvol PCMs that all feed the existing single 44.1 kHz dmix; volume keys show a volume bubble; mute/Normal/DND mode.
- Quick Settings: night light and mute tiles.

## Out of scope

Per-app volume (needs PulseAudio/PipeWire), vibration, ringtones from SMS (later), auto-brightness from a sensor.

## Device risk

Audio: the legacy driver hangs the tablet at other rates or buffers; ALL playback SHALL stay at 44.1 kHz with 25 ms/100 ms. Test with a timeout and a watchdog-safe soak. No flashing.

## Verification

On the tablet: choose Large text and check no clipping in Settings, Wi-Fi and Mobile; change the wallpaper; play a wav through each volume channel; run a 60 s overlapping-sounds soak with `timeout`.

## Open questions

- How to render night light cheaply without a compositor (overlay alpha vs fbmirror LUT)?
