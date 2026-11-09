# Rust migration task cards (status: todo | doing | done | blocked)

Gates: M0 SSH deploy + baseline + fixtures; M1 `scripts/check.sh` passes, tablet unchanged; M2 30 min real use, zero unexplained shadow divergences; M3 three cold boots with rust device+input, rollback tested; M4 new network joined via walpad-ctl, walpad-wifi untouched; M5 UI checklist passes; M6 touch-only browse/type/home/recover; M7 rootfs builds without python3.
[DEVICE] = owner runs a script on the tablet.

| Card | Task | Status |
|---|---|---|
| M0.1 [DEVICE] | scripts/device/smoke.sh (processes, walpad-ok, wifi-state, ping, dropbear :22, telnetd scope, status.json age) + owner manual checks | partial (script written; owner must run it) |
| M0.2 | scripts/deploy.sh with --dry-run, --rollback (hash-verified, current/previous symlinks) | partial (host-tested; see reports/M0.2.md) |
| M0.3 [DEVICE] | scripts/device/baseline.sh -> docs/rust/baseline.md | todo |
| M0.4 [DEVICE] | scripts/device/record_fixtures.sh -> tests/fixtures/ | todo |
| M1.1 | Workspace, toolchain pin, profiles, lints, cargo config, build_rust.sh, check.sh | done |
| M1.2 | walpad-proto: envelope, commands, status model, NDJSON 64 KiB, Secret | done |
| M1.3 | walpad-hw: WALPAD_ROOT, input discovery, evdev, gsensor, backlight, power supply, fakes | done |
| M1.4 | walpad-core skeleton: poll loop, IPC, get_status, status.json writer, --fake | done |
| M1.5 | walpad-ctl | done |
| M1.6 | scripts/dev/host_session.sh | done |
| M1.7 | build_rootfs.sh installs binaries, runtime.conf (python), apps.json | done |
| M2.1 | Logic modules (battery, screen_timeout, power_key, rotation, touch_map, key_gestures) | done |
| M2.2 | Shadow mode | done |
| M2.3 | shadow_diff.sh | done |
| M2.4 [DEVICE] | Owner scripted session + diff report | todo (see reports/M2.2-M2.3.md) |
| M3.1 | runtime.conf handling + fallback | done (host; see reports/M3.1.md) |
| M3.2 | device subsystem live | done (host; see reports/M3.2-M3.3.md) |
| M3.3 | input subsystem live | done (host) |
| M3.4 [DEVICE] | m3_check.sh + owner checklist | script written; owner runs it after flash |
| M4.1 | wpa ctrl client + fake supplicant | done |
| M4.2 | scan (single flight, escapes, merge, security) | done |
| M4.3 | connect/forget/disconnect (PBKDF2 PSK, mode 600) | done |
| M4.4 [DEVICE] | join, forget, reboot test | script written (scripts/device/m4_check.sh); owner runs it |
| M5.1 | walpad-ui: tokens, canvas, text, icons, PNG presenter, snapshot harness | done |
| M5.2 | components, every state (13 goldens) | done |
| M5.3 | X11 presenter, gestures, window props, resize, walpad-ok | done (host) |
| M5.4 | Home + core client + offline banner (6 goldens, full-stack test) | done (host) |
| M5.5 | Apps + Network + password sheet (17 goldens, click-through test) | done (host) |
| M5.6 | Device + Settings screens (19 goldens, round-trip test) | done (host) |
| M5.7 | Power menu overlay + hardware-key navigation (input topic) | done (host; see reports/M5.7.md) |
| M5.8 | System dock (own X window, shown over foreign apps) | done (host; see reports/M5.8.md) |
| M5.9 [DEVICE] | UI checklist on the tablet; `walpad-shell --capabilities` already prints `shell` (reports/M5.9-prep.md) | owner runs it |
| M6.1-M6.3 | process manager, installed-only apps, crash notice | done (host; see reports/M6.1-M6.3.md) |
| M6.4 [DEVICE] | gate checklist: scripts/device/m6_check.sh | owner runs it |
| M7.1 | Python-free image: WALPAD_NO_PYTHON=1 build mode + padui rescue (opt-in; default image unchanged) | done (host; see reports/M7.1.md) |
| M7.2 [DEVICE] | re-run baseline, compare | owner runs it |
| M8.1 | fbmirror pauses while the screen is off | done (host; never run on the tablet; see reports/M8.md) |
| M8.2 [DEVICE] | decide from scripts/device/measure_fbmirror.sh | owner measures |
| M8.3 [DEVICE] | Firefox memory: scripts/device/measure_firefox_mem.sh | owner measures |
| M9.1-M9.5 | Log caps, safe mode, reproducible build, security review, failure matrix | todo |

Owner decisions (defaults apply unless changed here): D1 pure-Rust renderer (tiny-skia+fontdue); D2 Files tile only when installed; D3 system dock yes; D4 port one-reading critical rule first, then 3 readings in own commit; D5 defer boot image changes; D6 keep keys.json gestures, add volume: only after audio verified.

Full card text: see the v2 plan sections 5-11 (behaviour parity spec, IPC v1, toolchain, UI) -- to be copied into docs/rust/ as each card needs them.
