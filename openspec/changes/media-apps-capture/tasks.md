## 1. Host
- [x] 1.1 Catalogue entries and mimeapps tests (mpv Media player with video+music roles, gpicview, media-tools; `open` field; defaults.json; tests/shell/media_test.sh, pkg.rs tests)
- [ ] 1.2 Screenshot/recording command construction tests
- [x] 1.3 `scripts/check.sh` passes (see the report of the media viewer change)

## 1b. Media viewer (owner request 2026-11-11, docs/41-media.md)
- [x] 1b.1 `launch` with `args.path`: proto validation, `resolve_open_path` (roots, `..`, symlink, option injection, missing file), apps `open`/`open_args`, replace-on-reopen
- [x] 1b.2 mpv config (`root/.config/mpv`: mpv.conf, input.conf, scripts/touch.lua) with large OSC and tap / double-tap gestures
- [x] 1b.3 Photo viewer gpicview in the image (apps.json, build_rootfs.sh); catalogue alternatives imv, ristretto, gthumb declare `open`
- [x] 1b.4 `walpad-thumb` (bounded, one job at a time, cache, 100 MB limit) with fake-tool tests
- [x] 1b.5 Media screen (Home tile, Apps panel tool): Photos/Videos grids, Music/Recent lists, background scan and thumbnails, recent list; shell tests and goldens
- [x] 1b.6 File associations through defaults.json roles and the generated mimeapps.list (PCManFM single tap, Firefox downloads via GIO)
- [x] 1b.7 Hardware limits documented (docs/41-media.md)
- [ ] 1b.8 Not yet decided: PDF viewer wiring (evince/zathura catalogue entries exist, no Media section)

## 2. Device (needs the tablet; no flashing)
- [ ] 2.1 Playback and PDF checks, soak with timeout
- [ ] 2.1a Install mpv and the thumbnail tools from the app store; Media screen shows thumbnails; photo opens in gpicview (pinch zoom, toolbar); video opens in mpv with sound at 44.1 kHz; tap pauses, double tap seeks; Back leaves
- [ ] 2.1b Measure dropped frames for a 720p30 and a 1080p30 H.264 file and record them in docs/41-media.md
- [ ] 2.1c Confirm the mpv double-click event order and that `ctrl+wheel` zooms in gpicview
- [ ] 2.2 Recording CPU and temperature observation
- [ ] 2.3 Screenshot chord via keys.json
