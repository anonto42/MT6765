# Media apps, screenshots and screen recording

## Why

Owner request 2026-11-10. The tablet has no video, music, image or PDF app, and screenshot/recording are missing.

## What changes

- Install and register via the catalogue: mpv (video, music), ristretto or imv (images), evince (PDF), with apps.json entries and file associations.
- mpv profile sw-fast (no hwdec, framedrop, x11 output) and audio output only through the existing 44.1 kHz ALSA PCM; touch OSC script.
- Screenshot: shell action saves PNG to ~/Pictures/Screenshots with a notification; hardware chord from keys.json.
- Screen recording: ffmpeg x11grab at 15 fps, libx264 ultrafast, started and stopped from a Quick Settings tile, no audio.

## Out of scope

Hardware video decode (no vendor decode user space), camera app (see camera-bringup), music library UI, office suite.

## Device risk

No flashing. Audio: SHALL NOT change ALSA rates (see display-sound-settings). Recording is CPU heavy: cap at 15 fps and stop below 15 % free RAM.

## Verification

On the tablet: play a 720p30 H.264 file for 5 minutes (`mpv --vo=x11 --hwdec=no`), check dropped frames; open a PDF and an image folder; take a screenshot; record 10 s and play it back.

## Open questions

- Can 1080p H.264 play at all in software on 8 cores? Measure and record in the change.
