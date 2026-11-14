# Top bar stats

## Why

Owner request 2026-11-11: "on the top bar on the screen on the row of date time i want to see the cpu, ram, rom, network use and also which things i want to see and not see those will control on settings".

## What changes

- walpad-core samples CPU use (/proc/stat), RAM (/proc/meminfo), storage of `/` (statvfs) and network throughput (/proc/net/dev: wlan0, ccmni*, rndis0, usb0, never lo) every 2 s, only while at least one item is on. Every read is Option: unreadable = unavailable, never a guess.
- `status.stats` carries the four switches and the figures; command `stats_config {cpu?, ram?, rom?, net?}` changes the switches, persisted in /var/lib/walpad/statusbar.json. Default: CPU and RAM on, storage and network off.
- walpad-shell draws the enabled items as compact text with mini bars between the clock and the Wi-Fi/battery pill, in portrait and landscape, dropping the date and then the bars before anything overlaps.
- Settings > Top bar items: four switches.
- walpad-ctl: `stats cpu|ram|rom|net on|off`.

## Out of scope

Per-process figures, history graphs, temperature, per-interface breakdown, a Quick Settings entry.

## Device risk

None: no flashing, no partitions, no boot files. Reads /proc and one statvfs call; the config file is one small JSON file.

## Secrets impact

None.

## Verification

Host: parser, daemon IPC and shell snapshot tests (portrait and landscape goldens), `scripts/check.sh`. On the tablet (not done yet): turn each item on in Settings > Top bar items, compare with `top`, `free`, `df /` and a download; check the row stays clear of the clock and the pill in both orientations.
