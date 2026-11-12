# Graph Report - walpad.8g  (2026-11-13)

## Corpus Check
- 506 files · ~456,225 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 55 file(s) not represented in the graph (top: .conf 23, (none) 12, .ttf 6)

## Summary
- 8428 nodes · 18306 edges · 519 communities (342 shown, 177 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 522 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3cf96bb4`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- x.rs
- tap
- mousepad.rs
- walpad-devd.py
- Gesture
- _
- Rect
- src/components.rs
- Command
- dock.rs
- Target
- padui.c
- Daemon
- Console
- walpad-wifi-ui.py
- Frame
- key_gestures.rs
- src/apps.rs
- tests/apps.rs
- touch_map.rs
- CoreEvent
- settings_screen.rs
- components
- rotation.rs
- os
- screen_timeout.rs
- Tone
- io
- evdev.rs
- keyboard.rs
- panels.rs
- Icon
- text.rs
- time
- modem_ipc.rs
- Canvas
- live.rs
- network.rs
- screens_snapshots.rs
- walpad-proto/src/lib.rs
- tests/components.rs
- atomic
- Sys
- parse.rs
- logic/gesture.rs
- Wifi
- Effect
- Topic
- wifi_finish.py
- ime.rs
- path
- hotspot_ipc.rs
- ipc.rs
- App
- Cmd
- chrome.rs
- display_dashboard.py
- run.sh
- pkg.rs
- walpad-keys.py
- walpad-ctl/src/lib.rs
- logic/battery.rs
- Inputs
- src/tests.rs
- bluetooth/mod.rs
- walpad-core/src/daemon.rs
- src/lock.rs
- status.rs
- gsensor.rs
- Outcome
- InputEvent
- src/hotspot.rs
- power_key.rs
- SheetPurpose
- argparse
- wifi.rs
- art.rs
- Root
- ctrl.rs
- wpa/fake.rs
- manager.rs
- app.rs
- WiFi bring-up plan — WalPad 8G (MT6765)
- logic/jack.rs
- keys_py_test.py
- notifications_ui.rs
- wire.rs
- serde_json
- M9.1-M9.3 Log caps, safe mode, reproducible build
- device_screen.rs
- modem/parse.rs
- src/notifications.rs
- recents.rs
- M7.1 Python-free image as an explicit build mode
- XWindow
- dmlinear.c
- Plan: make the WalPad 8G a real device (browser, terminal, Wi-Fi, touch UI)
- home_snapshots.rs
- WalPad Shell UI spec: line-art theme (v2, 2026-11-10; changes go through the owner)
- Tnet
- Model
- gnss.rs
- soc.rs
- Modem
- Flash runbook: first WalPad OS image with the Rust components
- mtk_stack_up.sh
- wpdprops32.c
- wpdprops.c
- Claude Code Context (WalPad 8G)
- psk.rs
- Purpose
- Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project
- m6_check.sh
- measure_firefox_mem.sh
- Gnss
- Phases
- 2026-08-18 — host-side recon; console access is the one blocker
- 2026-08-20 — CONNSYS stack fully initialises; wlan firmware start still fails
- 2026-11-07 / 08 — attempt to restore Android, root causes found, switch to a Linux touch UI
- walpad-shell
- measure_fbmirror.sh
- services_ctl.sh
- Project Memory (WalPad 8G)
- pdu.rs
- wmbar.rs
- 2026-08-16 — WalPad-Server USB gadget discovered; BROM mode reached
- 2026-08-20 (evening) — wlan0 EXISTS.  CONNSYS + firmware fully up.
- Status log
- host_session.sh
- m3_check.sh
- wifi_assoc.sh
- Agent Startup (WalPad 8G)
- seltools.rs
- 2026-08-20 — WiFi works end to end: `ssh root@192.168.0.100` over the built-in radio
- deploy.sh
- smoke.sh
- wifi_up.sh
- xbridge.rs
- init.rs
- XBridge
- m4_check.sh
- docker_boot.sh
- usb_utils.sh
- flash_os.sh
- net_diag.sh
- vps_boot.sh
- vps_setup.sh
- vps_stage.sh
- wifi_boot.sh
- .agents/README.md
- caveman.md
- decisions.md
- hardware.md
- known-issues.md
- legacy.md
- device-access.md
- device-safety.md
- graphify.md
- openspec-workflow.md
- shared/rules/README.md
- rust-migration.md
- vendor-daemons.md
- init.lua
- rootfs.md
- shell.md
- .claude/README.md
- apply.sh
- rootfs-nopython/README.md
- M0.1.md
- M0.2.md
- M1.1.md
- M1.2.md
- M1.3.md
- M1.4.md
- M1.5.md
- M1.6.md
- M1.7.md
- M2.1.md
- M2.2-M2.3.md
- M3.1.md
- M3.2-M3.3.md
- M5.1.md
- M5.10.md
- M5.2.md
- M5.3.md
- M5.4.md
- M5.5.md
- M5.6.md
- M5.7.md
- M5.8.md
- TASKS.md
- boot_kernel.sh
- build_rootfs.sh
- build_rust.sh
- build_ui_boot.sh
- charge_keeper.sh
- check.sh
- detect_mode.sh
- shadow_diff.sh script
- docker_setup.sh
- flash_partition.sh
- harvest_wifi_fw.sh
- host_recon.sh
- image_set_wifi.sh
- link_host.sh
- recovery_ap.sh
- rust.sh
- services_setup.sh
- setup_server.sh
- vps_migrate_userdata.sh
- vps_self_setup.sh
- vps_setup_device.sh
- wifi_authorize.sh
- wifi_find.sh
- wifi_password.sh
- wifi_watchdog.sh
- build_wifi_userspace.sh
- dmlinear/build.sh
- fbmirror/build.sh
- test.sh
- padui/build.sh
- propshim32/build.sh
- propshim/build.sh
- src/store.rs
- XError
- camacdk.c
- phone_ipc.rs
- pathbuf
- Vpn
- phone/mod.rs
- media_worker.rs
- src/bluetooth.rs
- edge.rs
- gps_screen.rs
- at.rs
- walpad-btbridge/src/lib.rs
- h4.rs
- bluetooth_ipc.rs
- command.rs
- phone/parse.rs
- Phone
- wm.rs
- state.rs
- src/volume.rs
- Shell
- walpad-touchd.py
- src/mobile.rs
- 4. Feature groups
- touch_gestures.rs
- call_screen.rs
- pkg_ipc.rs
- nmea.rs
- XConn
- modem/mod.rs
- src/media.rs
- vpn_screen.rs
- walpad-gnss/src/daemon.rs
- tests/daemon.rs
- Page
- logic/notifications.rs
- WalPad OS: tablet feature plan (2026-11-10)
- volume_ipc.rs
- Op
- vpn_ipc.rs
- lock_screen.rs
- walpad-launcher.py
- bluetooth/parse.rs
- phone/store.rs
- LockKind
- Fonts
- .x_try
- stats.rs
- gnss_ipc.rs
- lock_ipc.rs
- tests/bluetooth.rs
- phone_ui.rs
- src/vpn.rs
- super
- quick
- render
- LockScreen
- src/topbar.rs
- run
- arc
- launch
- tests/store.rs
- tests/mobile.rs
- PhoneModel
- Frozen contracts (do not change without an explicit card)
- CallTarget
- config.rs
- GpsStatus
- PhoneTarget
- fbmirror.c
- tests/lock.rs
- shell.rs
- Tab
- ADDED Requirements
- PairEvent
- SmsStore
- Camera through MediaTek ACDK (2026-11-10)
- Evidence
- Sock
- walpad-btbridge/src/main.rs
- Recv
- IDE / code editor: Lite XL chosen over Geany
- render
- MediaModel
- Section
- wall
- topbar_snapshots.rs
- logic.h
- render_sheet
- edit_snapshots.rs
- vpn_snapshots.rs
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- online
- C
- Stats
- tests/media.rs
- recents_snapshots.rs
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- ADDED Requirements
- .next_packet
- ContactStore
- CallAct
- Terminal: default shell with autosuggestions/completion (tabs already shipped)
- .new
- Kind
- media_snapshots.rs
- wmbar_snapshots.rs
- 2. How it was recovered (static analysis only)
- M5.11 Mobile network / SIM management (owner request 2026-11-10)
- M8 display efficiency (M8.1 done on the host; M8.2 and M8.3 are owner measurements)
- media_test.sh
- vpn.sh
- App switcher and navigation
- appdefs.rs
- Secret
- volume_snapshots.rs
- ADDED Requirements
- wpdprops64b.c
- Requirement: Headphone output route
- Multitasking: several terminals and PC-like windows
- ZT9101 USB Wi-Fi dongle driver for WalPad (station mode, monitor mode, injection)
- callaudio.sh
- jack.sh
- MobileReg
- .spawn
- StatItem
- input.rs
- Event
- .new
- Bluetooth with BlueZ (2026-11-10)
- Smooth battery percent and time left
- Bluetooth settings
- Clipboard and touch text editing
- Clock, alarms, calculator and notes
- Device event notifications
- Display and sound settings
- Headphone jack and microphone
- Security-testing mode (Kali chroot)
- Keyboard languages, Bangla and emoji
- Lock screen and screen lock
- Media apps, screenshots and screen recording
- Network shares, SFTP and storage manager
- Top bar stats
- VPN, proxy and DNS
- Wi-Fi monitor mode driver
- gps.sh
- assert_snapshot_in
- Wr
- ShellInput
- PhoneView
- Headphone jack and microphone (owner request 2026-11-12, change `headphone-mic-jack`)
- UI polish: remove box-in-box borders (owner request 2026-11-12)
- App catalogue and package manager
- ADDED Requirements
- Camera bring-up on WalPad OS
- Notification centre
- ADDED Requirements
- USB Wi-Fi dongle: find the chip and the driver (host-PC side flow)
- touch.lua
- AddrSource
- serial_pull.py
- Hit
- Value
- st
- camera-bringup/tasks.md
- State
- MobileData
- Notifications
- Battery percent and time left (owner request 2026-11-11)
- Requirement: Still photo from the built-in camera
- device-notifications/tasks.md
- top-bar-stats/tasks.md
- pkg_test.sh
- Media: photos, videos and music (owner request 2026-11-11, change `media-apps-capture`)
- Code editor app
- Thumb
- ClccStat
- GpsState
- Xvfb
- headphone-mic-jack/tasks.md
- SimState
- app-catalogue-manager/tasks.md
- app-switcher-navigation/tasks.md
- battery-soc-time-left/tasks.md
- bluetooth-ui/tasks.md
- clipboard-selection/tasks.md
- clock-alarms-productivity/tasks.md
- display-sound-settings/tasks.md
- kali-chroot-mode/tasks.md
- keyboard-languages-bangla/tasks.md
- launcher-home-screen/tasks.md
- lock-screen/tasks.md
- multitasking-windows/tasks.md
- network-file-sharing/tasks.md
- notification-centre/tasks.md
- vpn-and-dns/tasks.md
- LockAct
- M5.11.md
- app-catalogue-manager/README.md
- app-switcher-navigation/README.md
- bluetooth-ui/README.md
- clipboard-selection/README.md
- clock-alarms-productivity/README.md
- display-sound-settings/README.md
- headphone-mic-jack/README.md
- kali-chroot-mode/README.md
- keyboard-languages-bangla/README.md
- launcher-home-screen/README.md
- lock-screen/README.md
- media-apps-capture/README.md
- network-file-sharing/README.md
- notification-centre/README.md
- vpn-and-dns/README.md
- wifi-monitor-mode-driver/README.md
- zt9101-usb-wifi-driver/README.md
- build_bt_module.sh
- build_wifi_module.sh
- build_zt9101_module.sh
- camacdk/build.sh
- walpad-btbridge
- walpad-gnss
- Terminal tabs and shell autocomplete (owner request 2026-11-12)
- ADDED Requirements
- code-editor-app/tasks.md
- walpad-gnss/src/main.rs
- ADDED Requirements
- decode
- Rd<'a>
- App
- Location
- 5. Security-testing mode (Kali chroot)
- Verb
- codec.rs
- NotificationCategory
- ModemState
- terminal-tabs-and-autocomplete/tasks.md
- M5.9-prep Quad tap launches the browser in both runtimes; shell capability advertised
- VpnState
- .rejected
- Running
- Engine
- Running
- X_TURN

## God Nodes (most connected - your core abstractions)
1. `Rect` - 190 edges
2. `Target` - 170 edges
3. `XError` - 134 edges
4. `Root` - 131 edges
5. `Command` - 113 edges
6. `Cmd` - 108 edges
7. `online()` - 104 edges
8. `_` - 104 edges
9. `Daemon` - 86 edges
10. `Icon` - 81 edges

## Surprising Connections (you probably didn't know these)
- `Scenario: open a file` --references--> `launch()`  [INFERRED]
  openspec/changes/code-editor-app/specs/code-editor-app/spec.md → config/rootfs/usr/local/bin/walpad-keys.py
- `M5.9-prep Quad tap launches the browser in both runtimes; shell capability advertised` --references--> `do()`  [INFERRED]
  docs/rust/reports/M5.9-prep.md → config/rootfs/usr/local/bin/walpad-keys.py
- `Scenario: `wifi` is typed in search` --references--> `wifi()`  [INFERRED]
  openspec/changes/launcher-home-screen/specs/launcher/spec.md → config/rootfs/usr/local/bin/walpad-launcher.py
- `M8.2 measurement: scripts/device/measure_fbmirror.sh` --references--> `screen()`  [INFERRED]
  docs/rust/reports/M8.md → config/rootfs/usr/local/bin/walpad-devd.py
- ``launch` with a file (security model)` --references--> `launch()`  [INFERRED]
  docs/41-media.md → config/rootfs/usr/local/bin/walpad-keys.py

## Import Cycles
- 1-file cycle: `crates/walpad-core/src/jack.rs -> crates/walpad-core/src/jack.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/chrome.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/device_screen.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/network.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/settings_screen.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/gps_screen.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/overlay.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/vpn_screen.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/phone_ui.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/notifications_ui.rs -> crates/walpad-shell/src/app.rs`
- 3-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/settings_screen.rs -> crates/walpad-shell/src/device_screen.rs -> crates/walpad-shell/src/app.rs`
- 3-file cycle: `crates/walpad-shell/src/present.rs -> crates/walpad-x/src/lib.rs -> crates/walpad-shell/src/window.rs -> crates/walpad-shell/src/present.rs`

## Communities (519 total, 177 thin omitted)

### Community 0 - "x.rs"
Cohesion: 0.21
Nodes (36): a_shell_started_locked_covers_everything_grabs_and_takes_input(), activate_app(), active_fill(), app_window(), atom(), can_grab_pointer(), core_status(), count() (+28 more)

### Community 1 - "tap"
Cohesion: 0.13
Nodes (39): an_available_network_with_stored_credentials_connects_without_the_sheet(), apps_panel_close_badge_closes_the_app_and_keeps_the_panel_open(), apps_screen_launches_closes_and_pins(), clock_toggle_restart_shell_and_two_step_power_confirm(), close_reply_that_closed_nothing_is_reported(), disconnect_button_and_core_loss_reset_the_transient_state(), entering_network_asks_for_saved_networks_and_scan_is_single_flight(), go() (+31 more)

### Community 2 - "mousepad.rs"
Cohesion: 0.05
Nodes (63): ACCEL, ARROW, BRIDGE_LIFT_MS, BUTTON_LEFT, BUTTON_RIGHT, button_row(), BUTTON_WHEEL_DOWN, BUTTON_WHEEL_UP (+55 more)

### Community 3 - "walpad-devd.py"
Cohesion: 0.24
Nodes (18): battery(), battery_guard(), command(), decide(), find_inputs(), load_cfg(), log(), main() (+10 more)

### Community 4 - "Gesture"
Cohesion: 0.11
Nodes (27): beyond_slop(), down(), drag_scrolls_with_deltas_and_ends_on_release(), Gesture, Cancel, Drag, DragEnd, DragStart (+19 more)

### Community 5 - "_"
Cohesion: 0.04
Nodes (64): STRIP, DANGER_TINT, DANGER_TINT_PRESSED, _, ACCENT, BG, BLACK, BOX_DOCK (+56 more)

### Community 6 - "Rect"
Cohesion: 0.08
Nodes (15): dialog_rect(), render_pair(), Rect, banner_tint(), KeyBtn, page_rect(), Painter, split_charging() (+7 more)

### Community 7 - "src/components.rs"
Cohesion: 0.03
Nodes (72): Tap, Hide, Paste, Send, State, Accessory, Action, Chevron (+64 more)

### Community 8 - "Command"
Cohesion: 0.02
Nodes (87): Command, Back, BluetoothConfirm, BluetoothConnect, BluetoothDisconnect, BluetoothEnable, BluetoothForget, BluetoothPair (+79 more)

### Community 9 - "dock.rs"
Cohesion: 0.05
Nodes (41): app_for_class(), Atoms, AUTO_COLLAPSE_MS, button_at(), button_rects(), BUTTONS, command_for(), Dock (+33 more)

### Community 10 - "Target"
Cohesion: 0.02
Nodes (103): Target, AppTile, Avail, Back, BatteryCard, BtAction, BtAvail, BtCard (+95 more)

### Community 11 - "padui.c"
Cohesion: 0.15
Nodes (43): activate_sel(), atoi_(), battery_pct(), blit_all(), btn_count(), btn_get(), cmain_(), commit() (+35 more)

### Community 12 - "Daemon"
Cohesion: 0.09
Nodes (9): Client, Daemon, read_file(), wall_ms(), wifi_event_json(), write_atomic(), read_battery(), encode_line() (+1 more)

### Community 13 - "Console"
Cohesion: 0.11
Nodes (10): already_have(), collect(), main(), path_for(), save(), write_index(), Console, device_present() (+2 more)

### Community 14 - "walpad-wifi-ui.py"
Cohesion: 0.20
Nodes (5): App, Keyboard, scan_results(), status(), wcli()

### Community 15 - "Frame"
Cohesion: 0.06
Nodes (35): app_sheet_geom(), app_sheet_has_close_only_when_running(), AppSheet, dns_list(), key_at(), normalize_mac(), Overlay, App (+27 more)

### Community 16 - "key_gestures.rs"
Cohesion: 0.11
Nodes (32): Action, Back, Ctl, Home, Key, Keyboard, Launch, None (+24 more)

### Community 17 - "src/apps.rs"
Cohesion: 0.07
Nodes (33): app_that_disappears_from_disk_is_unlisted_again(), AppDef, Apps, apps_d_adds_apps_but_never_replaces_the_shipped_ones(), apps_get_the_system_proxy_in_their_environment(), bad_json_means_no_apps(), BIN_DIRS, closing_child_that_ignores_sigterm_is_killed_after_the_grace_period() (+25 more)

### Community 18 - "tests/apps.rs"
Cohesion: 0.12
Nodes (25): app_win(), atom(), Client, close_app_falls_back_to_sigterm_when_the_window_ignores_delete(), close_app_sends_wm_delete_window_to_an_app_we_did_not_start(), crash_publishes_a_notice_and_raises_home_but_clean_exit_and_close_do_not(), free_mode_clamps_moves_and_resizes_and_snaps(), geometry() (+17 more)

### Community 19 - "touch_map.rs"
Cohesion: 0.14
Nodes (27): Act, Move, Pair, Press, Release, Cfg, ev(), H (+19 more)

### Community 20 - "CoreEvent"
Cohesion: 0.12
Nodes (17): connects_gets_status_sends_commands_and_reconnects(), Core, CoreClient, CoreEvent, Apps, Audio, Connected, Disconnected (+9 more)

### Community 21 - "settings_screen.rs"
Cohesion: 0.11
Nodes (24): Ctx, find(), gather(), gather_reads_a_fake_root(), layout(), layout_top_level_and_subpage(), maintenance_has_the_dangerous_actions_as_danger_buttons(), page_rows() (+16 more)

### Community 22 - "components"
Cohesion: 0.10
Nodes (18): battery_card(), CELL_H, CELL_W, grid_is_centred_fits_and_wraps(), HomeLayout, layout(), overlaps(), render() (+10 more)

### Community 23 - "rotation.rs"
Cohesion: 0.10
Nodes (25): Act, DockKeyboard, SetStruts, WriteOrientation, Cfg, decide(), missing_sample_does_nothing(), Rotation (+17 more)

### Community 24 - "os"
Cohesion: 0.10
Nodes (19): ctl(), halt(), Menu, detect(), main(), main(), screen_off(), screen_on() (+11 more)

### Community 25 - "screen_timeout.rs"
Cohesion: 0.15
Nodes (19): Act, Backlight, Activity, NoWake, activity_resets_idle_timer(), Wakes, Brightness, Absolute (+11 more)

### Community 26 - "Tone"
Cohesion: 0.12
Nodes (21): hw_label(), Item, BrightnessCard, DimSeg, Gap, Info, Label, LidRow (+13 more)

### Community 27 - "io"
Cohesion: 0.08
Nodes (24): BadMetadata, _descriptor(), human(), parse(), parse_geometry(), parse_metadata(), _read_tables(), report() (+16 more)

### Community 28 - "evdev.rs"
Cohesion: 0.08
Nodes (25): ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_X, ABS_Y, BTN_TOUCH, ev(), EV_ABS, EV_KEY (+17 more)

### Community 29 - "keyboard.rs"
Cohesion: 0.06
Nodes (27): Atoms, bottom_strut(), BottomWindows, create_bottom_windows(), HEIGHT_PCT, inject(), keyboard_rect(), KeyMap (+19 more)

### Community 30 - "panels.rs"
Cohesion: 0.07
Nodes (43): apps_panel_rect(), apps_panel_sits_next_to_the_sidebar_at_the_bottom(), BADGE_HIT, BADGE_R, battery_row(), battery_time_text(), CAPTION_H, cell_box() (+35 more)

### Community 31 - "Icon"
Cohesion: 0.04
Nodes (56): build(), Canvas, draw_icon(), draw_icon_rotated(), every_icon_draws_something_inside_its_box(), Icon, .ALL, Apps (+48 more)

### Community 32 - "text.rs"
Cohesion: 0.10
Nodes (19): Align, Center, Left, Right, drawing_changes_pixels_only_inside_the_box(), ellipsis_fits_the_box(), Face, .ALL (+11 more)

### Community 33 - "time"
Cohesion: 0.14
Nodes (17): send_reboot(), usb_ids(), main(), reboot(), wait_gone(), attempt(), main(), stage() (+9 more)

### Community 34 - "modem_ipc.rs"
Cohesion: 0.12
Nodes (14): a_failed_boot_and_a_missing_modem_are_reported(), Conn, data_failure_is_reported_with_its_reason(), FAKE_SCRIPT, FakeModem, FakeState, fast(), IMEI (+6 more)

### Community 35 - "Canvas"
Cohesion: 0.10
Nodes (12): blit_and_crop_roundtrip(), Canvas, copy_rect_from_a_strip(), fills_and_reads_back(), origin_offsets_drawing_but_not_reads(), outlines_leave_the_inside_untouched(), paint(), rect_helpers() (+4 more)

### Community 36 - "live.rs"
Cohesion: 0.17
Nodes (17): append_events(), cover_closed_turns_screen_off_and_gsensor_rotates(), ev(), flags_decide_which_subsystem_acts(), long_power_opens_the_shell_menu_and_the_keys_are_routed_to_the_shell(), power_key_short_press_turns_screen_off_via_live_device(), put_device(), read() (+9 more)

### Community 37 - "network.rs"
Cohesion: 0.13
Nodes (25): bars_from_dbm(), CARD_H, client_off(), connected_card_and_failed_hint_take_space_only_when_present(), FORGET_W, hotspot_lines(), hotspot_on(), hotspot_status_lines() (+17 more)

### Community 38 - "screens_snapshots.rs"
Cohesion: 0.16
Nodes (39): app(), app_store_screens(), apps_options_sheet(), apps_panel_portrait_and_landscape(), bluetooth_busy_row_and_dialogs(), bluetooth_screen_states(), bt_on(), catalog() (+31 more)

### Community 39 - "walpad-proto/src/lib.rs"
Cohesion: 0.13
Nodes (9): ErrorBody, Event, parse_request(), PROTO_VERSION, Response, SOCKET_PATH, code(), code() (+1 more)

### Community 40 - "tests/components.rs"
Cohesion: 0.19
Nodes (17): assert_snapshot(), app_tiles_and_status_cards(), button_states(), controls(), dialogs_sheet_and_power_menu(), header_pill_and_icon_boxes(), keyboard_layers(), label() (+9 more)

### Community 41 - "atomic"
Cohesion: 0.09
Nodes (21): parse_voltage_mv(), PS, reads_tree_and_missing(), SOURCES, falls_back_to_extcon(), no_detector_is_none(), PRESENCE_NODES, read_jack() (+13 more)

### Community 42 - "Sys"
Cohesion: 0.09
Nodes (10): main(), table(), main(), versions(), cmd(), run(), _find_data_intf(), find_device() (+2 more)

### Community 43 - "parse.rs"
Cohesion: 0.12
Nodes (22): decode_escapes(), Event, Connected, Disconnected, Other, ScanResults, TempDisabled, list_status_signal_events() (+14 more)

### Community 44 - "logic/gesture.rs"
Cohesion: 0.07
Nodes (59): a_hold_shorter_than_500_ms_is_a_left_click(), a_late_or_distant_second_tap_is_a_separate_click(), a_touch_stops_a_fling_and_does_not_click(), BTN_LEFT, BTN_RIGHT, Cfg, CLICK, disabled_gestures_never_probe() (+51 more)

### Community 45 - "Wifi"
Cohesion: 0.18
Nodes (8): map(), Wifi, WifiError, Busy, Failed, Invalid, Unavailable, WifiLive

### Community 46 - "Effect"
Cohesion: 0.08
Nodes (32): a_short_power_press_that_turns_the_screen_off_locks_and_waking_does_not(), BATTERY_EVERY, Brain, cfg(), Effect, Action, Backlight, Button (+24 more)

### Community 47 - "Topic"
Cohesion: 0.33
Nodes (6): Topic, Apps, Input, Notice, Status, Wifi

### Community 48 - "wifi_finish.py"
Cohesion: 0.11
Nodes (20): Three bugs worth remembering, Tooling added, main(), parse_args(), read_core(), read_python(), main(), prepare() (+12 more)

### Community 49 - "ime.rs"
Cohesion: 0.06
Nodes (33): A, a_focus_that_does_not_stay_never_shows_the_keyboard(), Atoms, B, destroyed_context_and_vanished_client_hide(), FALLBACK_MS, focus(), focus_in_shows_and_focus_out_hides_after_the_debounce() (+25 more)

### Community 50 - "path"
Cohesion: 0.11
Nodes (12): GUARD_EVERY_MS, WM_CLASS_CLASS, WM_CLASS_INSTANCE, GUARD_EVERY_MS, WM_CLASS_CLASS, WM_CLASS_INSTANCE, golden(), password_and_pattern() (+4 more)

### Community 51 - "hotspot_ipc.rs"
Cohesion: 0.19
Nodes (8): Conn, FAKE_SCRIPT, hotspot_config_start_status_stop(), hotspot_failure_and_timeout_paths(), hotspot_without_script_fails_cleanly(), Rig, SECRET, wifi_switch_persists_and_hides_the_connection()

### Community 52 - "ipc.rs"
Cohesion: 0.23
Nodes (9): brightness_rotate_screen_and_errors(), Conn, Harness, hello_is_required_and_version_checked(), keyboard_command_reaches_the_shell_on_the_input_topic(), oversized_line_closes_connection_and_socket_mode_is_0600(), settings_launch_screenshot_power_and_restart_shell(), status_event_on_change_and_devd_fifo_and_legacy_json() (+1 more)

### Community 53 - "App"
Cohesion: 0.12
Nodes (3): App, AppEntry, WallTime

### Community 54 - "Cmd"
Cohesion: 0.03
Nodes (83): Cmd, Back, BtConfirm, BtConnect, BtDisconnect, BtEnable, BtForget, BtPair (+75 more)

### Community 55 - "chrome.rs"
Cohesion: 0.12
Nodes (25): active_target(), BELL_W, Cached, Chrome, ChromeKey, draw_item(), GAP, NAV_APPS (+17 more)

### Community 56 - "display_dashboard.py"
Cohesion: 0.11
Nodes (26): command(), commit_display(), cpu_percent(), endpoint(), hardware_status(), indicator(), load_psf(), main() (+18 more)

### Community 57 - "run.sh"
Cohesion: 0.10
Nodes (27): bad(), bt(), cam(), cam_env(), CAM_LOG, camrc(), chk(), conc_combo() (+19 more)

### Community 58 - "pkg.rs"
Cohesion: 0.07
Nodes (43): pkg_err(), APK_DB, app_entry_is_generated_from_the_catalogue(), cat_json(), CATALOGUE, catalogue_answer_lists_state_and_heals_a_missing_registration(), catalogue_drops_entries_that_could_reach_apk_wrongly(), check_space() (+35 more)

### Community 59 - "walpad-keys.py"
Cohesion: 0.28
Nodes (10): ctl(), do(), find(), Key, keyboard_toggle(), load(), log(), main() (+2 more)

### Community 61 - "walpad-ctl/src/lib.rs"
Cohesion: 0.16
Nodes (16): build_request(), key_held(), key_held_exit_status(), key_held_status(), legacy_line(), lock_clear(), read_secret(), rpc() (+8 more)

### Community 62 - "logic/battery.rs"
Cohesion: 0.16
Nodes (19): Act, CriticalPoweroff, LowChanged, Cfg, charger_online_means_charging_and_the_estimate_drops_the_charge_voltage(), charging_percent(), d4_consecutive_readings(), effective_status() (+11 more)

### Community 63 - "Inputs"
Cohesion: 0.19
Nodes (7): LiveState, Inputs, late_touch_device_is_picked_up(), put(), role_of(), Live, read()

### Community 64 - "src/tests.rs"
Cohesion: 0.07
Nodes (20): c_args(), hotspot_config_is_validated_and_redacted(), launch_takes_an_optional_validated_file_path(), legacy_status_has_old_field_set(), lock_commands_are_validated_and_secrets_redacted(), mobile_commands_are_validated_and_the_pin_redacted(), mobile_status_defaults_and_names(), old_status_without_bluetooth_parses() (+12 more)

### Community 65 - "bluetooth/mod.rs"
Cohesion: 0.07
Nodes (35): Bluetooth, BluetoothError, Busy, Failed, Refused, Cfg, CTL, ENABLED_FILE (+27 more)

### Community 66 - "walpad-core/src/daemon.rs"
Cohesion: 0.05
Nodes (29): APPS_X_EVERY, BATTERY_EVERY, bluetooth_err(), CLIENT_STALL, ExtClose, internal(), lock_err(), MAX_OUT (+21 more)

### Community 67 - "src/lock.rs"
Cohesion: 0.06
Nodes (42): a_damaged_file_counts_as_no_lock_never_as_locked_forever(), a_new_secret_must_fit_its_kind_and_changing_needs_the_current_one(), auto_lock_counts_from_screen_off_and_a_short_wake_does_not_lock(), BASE_DELAY_MS, BOOT_FILE, BURST, changing_or_removing_counts_wrong_current_secrets_too(), ct_eq() (+34 more)

### Community 68 - "status.rs"
Cohesion: 0.06
Nodes (40): kind_ext(), AppState, Battery, BatterySource, Gauge, Unknown, VoltageEstimate, Hardware (+32 more)

### Community 69 - "gsensor.rs"
Cohesion: 0.12
Nodes (7): BUF, GSENSOR_IOCTL, GsIoctl, .IS_MUTATING, parse_gsensor(), read_gsensor(), reads_fake_file_and_missing()

### Community 70 - "Outcome"
Cohesion: 0.17
Nodes (5): PowerMenuState, friendly(), App, Outcome, ReplyError

### Community 71 - "InputEvent"
Cohesion: 0.21
Nodes (14): cover_close_turns_screen_off(), ev(), idle_dims_then_touch_wakes(), lines(), lns(), new(), no_g(), power_short_press_toggles_screen_and_long_opens_menu() (+6 more)

### Community 73 - "src/hotspot.rs"
Cohesion: 0.09
Nodes (21): hotspot_err(), CONF, DEFAULT_SSID, field(), Hotspot, HotspotError, Busy, Failed (+13 more)

### Community 74 - "power_key.rs"
Cohesion: 0.21
Nodes (12): Act, ActivateMenuItem, OpenPowerMenu, ToggleScreen, WakeScreen, LONG_MS, PowerKey, rule1_long_press_opens_menu_once() (+4 more)

### Community 75 - "SheetPurpose"
Cohesion: 0.09
Nodes (22): SheetPurpose, HotspotName, HotspotPassword, LockCurrent, LockNew, LockRemove, LockRepeat, MobileApn (+14 more)

### Community 76 - "argparse"
Cohesion: 0.17
Nodes (11): detect_server(), interactive(), main(), one_shot(), read_all(), wait_for_server(), main(), mtk() (+3 more)

### Community 78 - "wifi.rs"
Cohesion: 0.32
Nodes (10): connect_wpa2_derives_psk_never_sends_the_passphrase_and_saves_with_mode_600(), Env, open_network_uses_key_mgmt_none_and_timeout_is_reported(), reconnects_when_the_supplicant_restarts(), saved_list_forget_disconnect_and_validation(), scan_is_single_flight_merged_sorted_and_utf8_safe(), secret(), until() (+2 more)

### Community 79 - "art.rs"
Cohesion: 0.20
Nodes (12): both_wallpapers_decode_at_panel_size_with_the_background_colour_on_top(), coffee_rect(), cup_path(), draw_coffee(), rotated_square(), strokes_stay_inside_the_rect(), the_cup_fits_both_orientations_and_keeps_its_aspect(), VIEW_H (+4 more)

### Community 80 - "Root"
Cohesion: 0.14
Nodes (22): resolve_open_path(), Root, BACKLIGHT, backlight_roundtrip_and_missing(), read_backlight(), read_trim(), uptime_s(), write_backlight() (+14 more)

### Community 81 - "ctrl.rs"
Cohesion: 0.26
Nodes (7): bind_to(), CtrlClient, CtrlError, Down, Fail, Timeout, REQUEST_TIMEOUT

### Community 82 - "wpa/fake.rs"
Cohesion: 0.23
Nodes (7): default_aps(), FakeAp, FakeSupplicant, Net, serve(), spawn(), unhex()

### Community 83 - "manager.rs"
Cohesion: 0.13
Nodes (13): CONNECT_POLL, CONNECT_TIMEOUT, Op, Connecting, Idle, Scanning, RECONNECT_EVERY, SCAN_TIMEOUT (+5 more)

### Community 84 - "app.rs"
Cohesion: 0.13
Nodes (12): MediaInfo, MediaThumbs, DEFAULT_PINNED, POWER_ARM_MS, POWER_MENU_MS, read_pinned(), shell_json(), SLIDER_SEND_EVERY_MS (+4 more)

### Community 85 - "WiFi bring-up plan — WalPad 8G (MT6765)"
Cohesion: 0.14
Nodes (13): Fallback while WiFi is still down, If route C turns out to be necessary, Measured (2026-08-19) — what the on-device survey found, Phase 0 — stop fighting sudo (5 min), Phase 1 — survey (one boot window), Phase 2 — make /vendor reachable (no 4 GB dump needed), Phase 3 — userspace, Phase 4 — connect (+5 more)

### Community 86 - "logic/jack.rs"
Cohesion: 0.12
Nodes (14): kind_name(), parse_kind(), effective(), mic_source_for(), MicSource, BuiltIn, HeadsetMic, Route (+6 more)

### Community 88 - "notifications_ui.rs"
Cohesion: 0.16
Nodes (15): Store, Notification, CLEAR_W, EMPTY_H, empty_list_has_no_rows_or_clear_button(), fonts(), layout(), n() (+7 more)

### Community 89 - "wire.rs"
Cohesion: 0.07
Nodes (28): ADDR_LEN, C_CLEANUP, C_DELETE_AIDING, C_INIT, C_INJECT_LOCATION, C_INJECT_TIME, C_POSITION_MODE, C_START (+20 more)

### Community 90 - "serde_json"
Cohesion: 0.12
Nodes (4): USAGE, Conn, scan_connect_forget_over_ipc(), CAPABILITIES

### Community 91 - "M9.1-M9.3 Log caps, safe mode, reproducible build"
Cohesion: 0.15
Nodes (12): Checks run, Deviations, Device checks pending (owner; nothing was run on the tablet), M9.1 log caps, M9.1-M9.3 Log caps, safe mode, reproducible build, M9.2 safe mode, M9.3 reproducible build, Measurements (+4 more)

### Community 92 - "device_screen.rs"
Cohesion: 0.16
Nodes (15): CRIT_BATT_OPTIONS, DIM_LEVEL_OPTIONS, DIM_OPTIONS, height(), info_row(), layout(), LOW_BATT_OPTIONS, OFF_OPTIONS (+7 more)

### Community 93 - "modem/parse.rs"
Cohesion: 0.18
Nodes (17): ati(), cfun(), cfun_and_cpin(), clean(), cme_sim(), cops(), cpin(), csq() (+9 more)

### Community 94 - "src/notifications.rs"
Cohesion: 0.22
Nodes (12): a_transition_fires_once_is_unread_and_bumps_the_badge(), CAPACITY, charging(), EVERY, first_tick_is_a_baseline_no_notification(), mark_read_clear_and_clear_all(), MAX_FILE, newest_first_and_ring_buffer_caps_at_100() (+4 more)

### Community 95 - "recents.rs"
Cohesion: 0.08
Nodes (33): a_refused_or_unanswered_close_brings_the_card_back(), a_short_upward_drag_springs_back(), Action, Close, Dismiss, Focus, Card, cards() (+25 more)

### Community 96 - "M7.1 Python-free image as an explicit build mode"
Cohesion: 0.17
Nodes (11): Capabilities and the guard (no Python to fall back to), Checks run, Deviations, Device checks pending (owner), M7.1 Python-free image as an explicit build mode, Measurements, Owner steps (nothing here was run by me), Rollback (+3 more)

### Community 97 - "XWindow"
Cohesion: 0.08
Nodes (12): XWindow, visible_area(), zone_rect(), Atoms, CAPTURE_EVERY, CAPTURE_FIRST, EDGE_CLASS, intern() (+4 more)

### Community 98 - "dmlinear.c"
Cohesion: 0.36
Nodes (9): cmain(), init_hdr(), num(), say(), sc(), scopy(), slen(), utoa() (+1 more)

### Community 99 - "Plan: make the WalPad 8G a real device (browser, terminal, Wi-Fi, touch UI)"
Cohesion: 0.18
Nodes (7): Codebase Graph, Codex Loader (WalPad 8G), Decision and honest expectations, Parallel track: official firmware (best outcome), Plan: make the WalPad 8G a real device (browser, terminal, Wi-Fi, touch UI), Progress (2026-11-08), Risks

### Community 100 - "home_snapshots.rs"
Cohesion: 0.42
Nodes (10): catalog(), golden(), home_landscape_connected(), home_landscape_offline(), home_portrait_charging_gauge_and_failed_radio(), home_portrait_connected(), home_portrait_core_offline(), home_portrait_low_battery_unknown_wifi_12h() (+2 more)

### Community 101 - "WalPad Shell UI spec: line-art theme (v2, 2026-11-10; changes go through the owner)"
Cohesion: 0.14
Nodes (13): Colour tokens (crates/walpad-ui/src/tokens.rs), Components and states, Destinations (how every screen is reached by touch), Frame (crates/walpad-ui/src/layout.rs), Interaction, Lock screen (owner request 2026-11-10, `lock_screen.rs` / `lock_window.rs`), Rendering and performance (the tablet renders in software on ARM), Screens and popovers (+5 more)

### Community 103 - "Model"
Cohesion: 0.22
Nodes (13): Model, label(), level(), muted(), Osd, OSD_MS, render_osd(), render_qs() (+5 more)

### Community 104 - "gnss.rs"
Cohesion: 0.11
Nodes (20): CHECK_EVERY, doc(), ENABLED_FILE, field(), MAX_JSON, parse_reading(), PATH, READ_EVERY (+12 more)

### Community 105 - "soc.rs"
Cohesion: 0.06
Nodes (44): a_working_gauge_wins(), absurd_times_are_hidden_and_garbage_current_means_unknown(), boot_seeds_from_the_persisted_value_when_it_agrees_with_the_voltage(), brightness_and_cpu_steps_do_not_make_the_percent_jump(), CAP, CAPACITY_FALLBACK_MAH, CAPACITY_MIN_HINT_MAH, charging_never_falls_and_full_forces_100() (+36 more)

### Community 106 - "Modem"
Cohesion: 0.08
Nodes (15): at_error_text(), field(), Busy, Failed, Invalid, Refused, Modem, ok_final() (+7 more)

### Community 107 - "Flash runbook: first WalPad OS image with the Rust components"
Cohesion: 0.20
Nodes (9): 0. What this image is (and is not), 1. Before you flash (host), 2. Flash (you run this), 3. First boot, 4. Turn the Rust parts on, one at a time, 5. What to send back, 6. Rollback (cheapest first), 7. Known risks and what is unverified (+1 more)

### Community 108 - "mtk_stack_up.sh"
Cohesion: 0.38
Nodes (9): die(), insmod_once(), launcher_serving(), launcher_up(), LD_LIBRARY_PATH, LD_PRELOAD, mounted(), say() (+1 more)

### Community 109 - "wpdprops32.c"
Cohesion: 0.47
Nodes (9): load(), property_get(), property_set(), s_close(), s_open(), s_read(), s_strlen(), s_write() (+1 more)

### Community 110 - "wpdprops.c"
Cohesion: 0.47
Nodes (9): load(), property_get(), property_set(), s_close(), s_open(), s_read(), s_strlen(), s_write() (+1 more)

### Community 111 - "Claude Code Context (WalPad 8G)"
Cohesion: 0.22
Nodes (8): Always-Loaded, Claude Code Context (WalPad 8G), Conditional: Docs, Conditional: Memory, Conditional: Rules (load only what matches), Conditional: Scopes, Daily History (Asia/Dhaka date, gitignored), Do Not

### Community 114 - "psk.rs"
Cohesion: 0.24
Nodes (5): hex(), derive(), psk_hex(), PskError, BadLength

### Community 115 - "Purpose"
Cohesion: 0.07
Nodes (28): Purpose, BtOp, BtPair, Close, Connect, DefaultApp, Forget, Gps (+20 more)

### Community 116 - "Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project"
Cohesion: 0.22
Nodes (9): Automation, How it actually works, Quick start, Repository layout, ⚠️ Safety, Setup, once, Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project, What's next (+1 more)

### Community 117 - "m6_check.sh"
Cohesion: 0.44
Nodes (7): bad(), chk(), listed(), ok(), running(), m6_check.sh script, waitfor()

### Community 118 - "measure_firefox_mem.sh"
Cohesion: 0.42
Nodes (7): cleanup(), mem_kb(), run_cfg(), measure_firefox_mem.sh script, stop_ff(), zram_off(), zram_on()

### Community 120 - "Gnss"
Cohesion: 0.19
Nodes (6): gnss_err(), Gnss, GnssError, Busy, Failed, Run

### Community 121 - "Phases"
Cohesion: 0.25
Nodes (8): Phase 0 - finish the framebuffer UI (in progress), Phase 1 - persistent rootfs on `userdata`, Phase 2 - Wi-Fi, Phase 3 - touch Wi-Fi settings and keyboard, Phase 4 - graphical session, browser, terminal, Phase 5 - the rest of the hardware, Phase 6 - hardening, Phases

### Community 122 - "2026-08-18 — host-side recon; console access is the one blocker"
Cohesion: 0.25
Nodes (8): 2026-08-18 — host-side recon; console access is the one blocker, 2026-08-19 — device-side WiFi survey complete, 2026-08-19 (later) — dropped the 4 GB dump; `/vendor` is a loop mount away, Next steps, The blocker: console permissions, The device is up and stable, What the USB descriptors reveal, Why the USB network never worked

### Community 123 - "2026-08-20 — CONNSYS stack fully initialises; wlan firmware start still fails"
Cohesion: 0.29
Nodes (7): 2026-08-20 — CONNSYS stack fully initialises; wlan firmware start still fails, Corrections to earlier entries, Do not do, Reached this boot, Remaining blocker, Solved, State at end of session

### Community 124 - "2026-11-07 / 08 — attempt to restore Android, root causes found, switch to a Linux touch UI"
Cohesion: 0.29
Nodes (7): 2026-11-07 / 08 — attempt to restore Android, root causes found, switch to a Linux touch UI, Android experiment (kept in `config/boot/init-android-experiment`), Current state of the tablet, Gotchas that cost time, How the tablet is reached now (all verified), Root causes found, Tools added

### Community 125 - "walpad-shell"
Cohesion: 0.43
Nodes (7): walpad-core, walpad-ctl, walpad-hw, walpad-proto, walpad-shell, walpad-ui, walpad-x

### Community 126 - "measure_fbmirror.sh"
Cohesion: 0.48
Nodes (5): measure(), screen_state(), set_screen(), measure_fbmirror.sh script, ticks()

### Community 127 - "services_ctl.sh"
Cohesion: 0.71
Nodes (6): load(), names(), running(), services_ctl.sh script, start_one(), stop_one()

### Community 128 - "Project Memory (WalPad 8G)"
Cohesion: 0.33
Nodes (5): Device state (2026-11-10), Goal, Project Memory (WalPad 8G), References (load when relevant), Shape

### Community 133 - "pdu.rs"
Cohesion: 0.08
Nodes (43): Alphabet, Gsm7, Ucs2, alphanumeric_sender_and_status_reports(), an_escape_pair_is_never_cut_between_parts(), bcd_decode(), bcd_encode(), concatenated_gsm7_decodes_back_in_order() (+35 more)

### Community 134 - "wmbar.rs"
Cohesion: 0.04
Nodes (63): SnapZone, Bottom, Full, Left, Right, Top, mode_name(), read_prefs() (+55 more)

### Community 135 - "2026-08-16 — WalPad-Server USB gadget discovered; BROM mode reached"
Cohesion: 0.33
Nodes (6): 2026-08-16 — WalPad-Server USB gadget discovered; BROM mode reached, Actions taken, Device is NOT in a plain MTK preloader, Next steps, The tablet is power-limited and boot-cycles, What responds / what doesn't

### Community 136 - "2026-08-20 (evening) — wlan0 EXISTS.  CONNSYS + firmware fully up."
Cohesion: 0.33
Nodes (6): 2026-08-20 (evening) — wlan0 EXISTS.  CONNSYS + firmware fully up., Bug 1 — module load order (the EMI base), Bug 2 — /dev/wmtWifi was a regular file, Debugging note for future work, Next blocker — no libnl for the supplicant, Result

### Community 137 - "Status log"
Cohesion: 0.33
Nodes (6): 2026-08-20 (later) — root cause of the firmware start failure: module load order, 2026-08-27 — boot persistence verified end-to-end, 2026-08-27 — internal userdata converted to 52.5 GiB Alpine VPS storage, 2026-11-06 - direction change and boot check, 2026-11-08 (later) — Alpine rootfs flashed, rotation flicker fix, Status log

### Community 138 - "host_session.sh"
Cohesion: 0.40
Nodes (4): DISPLAY, host_session.sh script, spawn(), WALPAD_ROOT

### Community 139 - "m3_check.sh"
Cohesion: 0.73
Nodes (5): bad(), chk(), conf(), ok(), m3_check.sh script

### Community 140 - "wifi_assoc.sh"
Cohesion: 0.47
Nodes (5): die(), LD_LIBRARY_PATH, LD_PRELOAD, say(), wifi_assoc.sh script

### Community 141 - "Agent Startup (WalPad 8G)"
Cohesion: 0.40
Nodes (4): Agent Startup (WalPad 8G), Hard Rules, Load, Work Loop

### Community 142 - "seltools.rs"
Cohesion: 0.04
Nodes (53): load_keymap(), active_class(), BAR_CLASS, bar_rect(), BarState, button_stroke(), button_strokes_are_the_ctrl_shortcuts_and_never_interrupt_a_terminal(), buttons() (+45 more)

### Community 143 - "2026-08-20 — WiFi works end to end: `ssh root@192.168.0.100` over the built-in radio"
Cohesion: 0.40
Nodes (5): 2026-08-20 — WiFi works end to end: `ssh root@192.168.0.100` over the built-in radio, Once wlan0 is up, stop using the serial console, Open, The libnl blocker, resolved, Two device-side gotchas

### Community 144 - "deploy.sh"
Cohesion: 0.80
Nodes (4): do_restart(), rput(), rsh(), deploy.sh script

### Community 145 - "smoke.sh"
Cohesion: 0.90
Nodes (4): bad(), chk(), ok(), smoke.sh script

### Community 146 - "wifi_up.sh"
Cohesion: 0.60
Nodes (4): die(), PATH, say(), wifi_up.sh script

### Community 148 - "xbridge.rs"
Cohesion: 0.11
Nodes (10): BACK_ALT_LEFT, back_spec(), keysym(), parse_key_spec(), TouchHit, App, None, Shell (+2 more)

### Community 149 - "init.rs"
Cohesion: 0.08
Nodes (35): ADDR, addr_is_valid(), address_comes_from_nvdata_first(), cfg(), corrupt_stored_address_is_replaced(), DEFAULT_ADDRS, DEFAULT_RADIO, DEFAULT_SLEEP (+27 more)

### Community 150 - "XBridge"
Cohesion: 0.13
Nodes (3): struts_pointer_keys_dock_against_real_xvfb(), XBridge, Window

### Community 151 - "m4_check.sh"
Cohesion: 0.83
Nodes (3): bad(), ok(), m4_check.sh script

### Community 152 - "docker_boot.sh"
Cohesion: 1.00
Nodes (3): mount_controller(), mountpoint(), docker_boot.sh script

### Community 249 - "src/store.rs"
Cohesion: 0.08
Nodes (35): Busy, catalogue_reply_is_parsed(), DEFAULT_RESERVE_KB, ERROR_H, fmt_size(), Item, Button, Error (+27 more)

### Community 250 - "XError"
Cohesion: 0.06
Nodes (14): CallWindow, pointer_events(), Atoms, ClipEvent, HistoryChanged, PrimaryChanged, ENTRY_MAX, Fetch (+6 more)

### Community 251 - "camacdk.c"
Cohesion: 0.14
Nodes (39): bypass_flag(), cam_main(), cap_cb(), cat_file(), clamp8(), cmd_capture(), cmd_list(), cmd_preview() (+31 more)

### Community 252 - "phone_ipc.rs"
Cohesion: 0.14
Nodes (20): a_busy_line_and_an_error_end_the_call_with_a_reason(), an_outgoing_call_runs_through_every_state(), AUDIO, Conn, contacts_are_saved_listed_imported_and_validated(), emergency_numbers_dial_without_a_sim_and_others_do_not(), FakeModem, FakeState (+12 more)

### Community 253 - "pathbuf"
Cohesion: 0.14
Nodes (10): DETECT_EVERY, FILE, Jack, new_reads_the_persisted_override_and_applies_once(), PATH, Run, SCRIPT, set_force_persists_and_clears() (+2 more)

### Community 254 - "Vpn"
Cohesion: 0.16
Nodes (16): write_600(), is_b64_key(), kind_of_name(), name_from_path(), parse_settings(), read_candidate(), render_settings(), Settings (+8 more)

### Community 255 - "phone/mod.rs"
Cohesion: 0.15
Nodes (23): a_cut_short_reply_does_not_end_the_call(), CALLAUDIO, clcc(), dial_failures_and_disconnect_causes(), dial_is_checked_and_gated(), EMERGENCY, fin(), Gate (+15 more)

### Community 256 - "media_worker.rs"
Cohesion: 0.11
Nodes (27): a_missing_helper_gives_no_thumbnail_instead_of_a_hang(), Event, Dropped, Info, Recent, Scanned, Thumb, fake_helper() (+19 more)

### Community 257 - "src/bluetooth.rs"
Cohesion: 0.11
Nodes (40): BluetoothDevice, BluetoothStatus, ACTION_W, available_devices(), bars_from_rssi(), bt(), BtLayout, BtRow (+32 more)

### Community 258 - "edge.rs"
Cohesion: 0.11
Nodes (28): a_tap_or_a_short_travel_in_the_zone_is_cancelled(), AREA, down(), EDGE_ZONE_PX, EdgeEvent, Cancelled, Home, Recents (+20 more)

### Community 259 - "gps_screen.rs"
Cohesion: 0.13
Nodes (20): bar_height(), busy(), chip(), FRESH_S, gps(), GpsLayout, LABEL_H, layout() (+12 more)

### Community 260 - "at.rs"
Cohesion: 0.10
Nodes (22): a_secret_command_never_prints(), AtCmd, AtError, Io, Open, Timeout, Final, Busy (+14 more)

### Community 261 - "walpad-btbridge/src/lib.rs"
Cohesion: 0.14
Nodes (25): Bridge, Bridge<C, H>, controller_command(), controller_command_waits_for_its_complete(), controller_hangup_is_reported(), controller_stream_is_reframed_one_packet_per_vhci_write(), create_hci(), create_hci_reads_the_index() (+17 more)

### Community 262 - "h4.rs"
Cohesion: 0.10
Nodes (27): ACL, bdaddr_and_command_complete(), CMD, command_is_not_a_controller_packet(), Deframer, drain(), EVENT, evt() (+19 more)

### Community 263 - "bluetooth_ipc.rs"
Cohesion: 0.14
Nodes (20): bad_addresses_never_reach_bluetoothctl(), Conn, dev(), disabled_switch_stays_off_at_startup(), failed_connect_gives_a_notice(), FAKE_CTL, FAKE_SCRIPT, KBD (+12 more)

### Community 264 - "command.rs"
Cohesion: 0.06
Nodes (42): ALLOWED_SETTINGS, BrightnessArgs, COMMAND_NAMES, DEFAULT_ROLES, KEY_ACTIONS, KeyboardMode, Hide, Show (+34 more)

### Community 265 - "phone/parse.rs"
Cohesion: 0.16
Nodes (13): fields(), Clcc, clcc_ignores_data_calls_and_noise(), clcc_lists_calls_of_every_state(), clean_number(), clip(), cmgl(), cmgl_pairs_headers_with_pdus() (+5 more)

### Community 266 - "Phone"
Cohesion: 0.12
Nodes (10): MobileError, AudioWant, Start, Stop, contacts_and_call_log_through_the_api(), Ended, is_emergency(), Phone (+2 more)

### Community 267 - "wm.rs"
Cohesion: 0.06
Nodes (54): LAYOUT_EVERY, load_layout(), WIN_CLOSE_GRACE, WinClose, a_lone_window_fills_the_work_area_and_a_second_one_takes_the_other_tile(), closed_windows_are_forgotten(), free_mode_floats_new_windows_and_keeps_them_inside(), hand_placed_windows_are_not_replaced() (+46 more)

### Community 268 - "state.rs"
Cohesion: 0.17
Nodes (17): bad_checksum_counts_but_changes_nothing(), Counters, Fix, FIX_STALE_MS, gsa_without_fix_clears_used(), json_shape(), loc(), LOC_PREFERRED_MS (+9 more)

### Community 269 - "src/volume.rs"
Cohesion: 0.10
Nodes (14): field(), FILE, Job, Apply, Get, parse_state(), PATH, PROBE_EVERY (+6 more)

### Community 271 - "walpad-touchd.py"
Cohesion: 0.24
Nodes (7): find_touch(), load_cal(), log(), main(), open_x(), orientation(), to_logical()

### Community 272 - "src/mobile.rs"
Cohesion: 0.16
Nodes (28): MobileStatus, data_line(), data_on(), data_switchable(), imei_value(), LABEL_H, layout(), layout_rows_are_touchable_and_do_not_overlap() (+20 more)

### Community 273 - "4. Feature groups"
Cohesion: 0.13
Nodes (15): 4.10 Phone features (SIM), 4.11 System services, 4.12 Developer features, 4.13 Automation (P4), 4.14 Security features of the OS itself, 4.1 Input and editing, 4.2 On-screen keyboard, 4.3 Gestures, navigation, windows (+7 more)

### Community 274 - "touch_gestures.rs"
Cohesion: 0.16
Nodes (17): APPS, double_tap_arrives_as_two_clicks_on_one_spot(), drag_in_a_scroll_app_is_wheel_events_and_never_a_button_1(), ev(), long_press_then_drag_is_a_button_1_drag_for_selecting_text(), mouse_apps_and_shell_windows_keep_the_mouse_drag(), NEXT, presses() (+9 more)

### Community 275 - "call_screen.rs"
Cohesion: 0.13
Nodes (16): PhoneStatus, BIG_D, buttons_are_big_and_inside_the_screen(), CallScreen, hit_test(), in_call_controls_toggle_and_send_tones(), KEY_GAP, KEY_H (+8 more)

### Community 276 - "pkg_ipc.rs"
Cohesion: 0.14
Nodes (13): a_failed_install_adds_no_entry_and_shows_the_error(), a_hanging_install_is_killed_at_the_deadline(), CATALOGUE, code(), Conn, default_apps_are_recorded_and_applied_to_mimeapps(), FAKE_APK, FAKE_HELPER (+5 more)

### Community 278 - "nmea.rs"
Cohesion: 0.10
Nodes (27): checksum_ok(), checksums(), coord(), days_from_civil(), gga(), gsv_and_gsa(), num(), parse() (+19 more)

### Community 279 - "XConn"
Cohesion: 0.07
Nodes (32): pixels(), pointer_to(), the_x_cursor_is_not_drawn_not_even_an_apps_own_cursor(), Xvfb, a_terminal_gets_only_copy_and_paste_and_paste_is_ctrl_shift_v(), atom(), click(), clipboard_list_pastes_the_tapped_entry() (+24 more)

### Community 280 - "modem/mod.rs"
Cohesion: 0.09
Nodes (21): CONF, data_needs_a_ready_modem_radio_and_sim(), job_notice(), JobKind, Init, Phone, Pin, Poll (+13 more)

### Community 281 - "src/media.rs"
Cohesion: 0.08
Nodes (28): AUDIO_EXT, GAP, IMAGE_EXT, LIST_ROW_H, MAX_DEPTH, MAX_ITEMS, MAX_RECENT, MAX_THUMBS (+20 more)

### Community 282 - "vpn_screen.rs"
Cohesion: 0.17
Nodes (24): CARD_H, card_lines(), dns_value(), editable(), home(), is_on(), KEEP_H, LABEL_H (+16 more)

### Community 283 - "walpad-gnss/src/daemon.rs"
Cohesion: 0.20
Nodes (10): boot_ms(), Config, Ctx, PUBLISH_EVERY_MS, RESEND_AFTER_MS, RESEND_MAX, run(), SANE_CLOCK_MS (+2 more)

### Community 284 - "tests/daemon.rs"
Cohesion: 0.16
Nodes (13): first_sv_blob_is_saved_for_analysis(), garbage_datagrams_are_ignored(), location(), N, nmea(), NonBlock, read_pty_lines(), Rig (+5 more)

### Community 285 - "Page"
Cohesion: 0.11
Nodes (19): CATEGORIES, Page, About, Apps, Bluetooth, Clock, Display, Gps (+11 more)

### Community 286 - "logic/notifications.rs"
Cohesion: 0.20
Nodes (11): a_sequence_of_snapshots_fires_only_on_transitions(), bluetooth_connect_and_disconnect(), cats(), charger_plug_and_unplug(), diff(), NewNotif, notif(), sd_card_and_usb_disk_mount_unmount() (+3 more)

### Community 287 - "WalPad OS: tablet feature plan (2026-11-10)"
Cohesion: 0.17
Nodes (11): 10. Open-source projects worth reusing, 11. Sources, 1. Scope, constraints, reading guide, 2. What the reference platforms provide (survey), 3. What WalPad has today (code read 2026-11-10), 6. Launcher and home screen, 7. Default app catalogue, 8. Phases (+3 more)

### Community 288 - "volume_ipc.rs"
Cohesion: 0.18
Nodes (9): a_hanging_script_is_killed(), a_sound_server_that_starts_later_gets_the_stored_level(), Conn, events_say_where_the_change_came_from(), FAKE_SCRIPT, no_script_or_no_sound_means_unavailable_but_the_level_is_kept(), Rig, test_tone_runs_the_script_once_at_a_time() (+1 more)

### Community 289 - "Op"
Cohesion: 0.08
Nodes (22): Op, Answer, Clcc, Dial, Dtmf, Hangup, Mute, Setup (+14 more)

### Community 290 - "vpn_ipc.rs"
Cohesion: 0.20
Nodes (9): a_failing_or_hanging_tunnel_is_reported_and_cleaned_up(), Conn, dns_and_proxy_are_stored_validated_and_applied(), FAKE_SCRIPT, import_validates_and_stores_the_profile_0600(), KEY, kill_switch_confirm_and_revert_are_reported(), on_and_off_run_the_script_and_report_the_tunnel() (+1 more)

### Community 291 - "lock_screen.rs"
Cohesion: 0.12
Nodes (21): a_short_pattern_is_refused_on_the_spot(), AREAS, canvas(), DOT_CAPTURE, five_wrong_pins_show_a_countdown_and_the_pad_is_dead_during_it(), Digit, KB_H, KEY_GAP (+13 more)

### Community 292 - "walpad-launcher.py"
Cohesion: 0.12
Nodes (13): battery(), Home, rd(), wifi(), Wi-Fi switch and hotspot (owner request 2026-11-10), M4 Wi-Fi control through the supplicant socket, Device risk, Launcher and home screen (+5 more)

### Community 293 - "bluetooth/parse.rs"
Cohesion: 0.17
Nodes (16): ansi_and_readline_noise_is_stripped(), clean_text(), devices_list(), failure(), friendly(), INFO_KEYBOARD, info_not_available_and_hostile_names(), info_unnamed_unpaired_old_rssi_format() (+8 more)

### Community 294 - "phone/store.rs"
Cohesion: 0.11
Nodes (19): call_log_is_capped(), CallLog, CallRec, CALLS_FILE, CallsFile, clip_name(), CONTACTS_FILE, contacts_save_lookup_delete_import() (+11 more)

### Community 295 - "LockKind"
Cohesion: 0.15
Nodes (12): LockKind, Password, Pattern, Pin, AUTO_LOCK_OPTIONS, kind_name(), lock_rows(), LockFlow (+4 more)

### Community 296 - "Fonts"
Cohesion: 0.12
Nodes (5): LockEvent, Submit, LockWindow, pointer_events(), Fonts

### Community 297 - ".x_try"
Cohesion: 0.17
Nodes (3): terminate_pid(), Daemon, class_matches()

### Community 298 - "stats.rs"
Cohesion: 0.14
Nodes (19): CONFIG_FILE, counted_iface(), cpu_line_and_delta(), cpu_pct(), CpuTimes, EVERY, MAX_FILE, MAX_GAP (+11 more)

### Community 299 - "gnss_ipc.rs"
Cohesion: 0.20
Nodes (8): a_hanging_start_is_busy_then_times_out(), a_stale_state_file_does_not_claim_a_fix(), a_switched_on_gps_starts_after_a_boot(), Conn, FAKE_SCRIPT, Rig, start_failure_is_reported_with_its_reason(), switch_start_position_and_stop()

### Community 300 - "lock_ipc.rs"
Cohesion: 0.21
Nodes (9): changing_and_removing_need_the_current_secret(), Conn, Core, Env, five_wrong_pins_show_a_countdown_and_a_right_pin_during_it_is_refused(), removing_lock_json_is_the_rescue_and_unlocks(), set_lock_screen_off_unlock_and_the_file_holds_no_pin(), the_tablet_comes_up_locked_after_a_reboot_but_not_after_a_core_restart() (+1 more)

### Community 301 - "tests/bluetooth.rs"
Cohesion: 0.19
Nodes (20): connect_disconnect_and_forget_from_my_devices(), dev(), failed_connect_is_a_toast_and_frees_the_row(), header_pill_shows_the_mark_only_while_powered(), KBD, keyboard_passkey_is_displayed(), notice(), off_controller_shows_only_the_state_card() (+12 more)

### Community 302 - "phone_ui.rs"
Cohesion: 0.09
Nodes (26): body_top(), CALL_H, contacts_filter_by_name_or_number(), Conversation, conversations_group_national_and_international_numbers(), DISPLAY_H, GRID_MAX_W, KEY_GAP (+18 more)

### Community 303 - "src/vpn.rs"
Cohesion: 0.10
Nodes (16): APPLY_TIMEOUT, CONF, DOWN_TIMEOUT, KEY, MAX_PROFILE, OVPN_FORBIDDEN, PATH, SCRIPT (+8 more)

### Community 304 - "super"
Cohesion: 0.13
Nodes (18): app_crash_notice_shows_a_danger_toast_that_expires(), render(), a_stats_only_change_redraws_the_header(), a_switch_sends_stats_config_with_the_opposite_value(), set_stats(), the_page_lists_four_switches_that_follow_the_status(), toggles(), audio_event() (+10 more)

### Community 305 - "quick"
Cohesion: 0.20
Nodes (14): a_failed_switch_request_is_a_toast(), copy_buttons_put_the_position_and_the_map_link_on_the_clipboard(), gps_churn_redraws_only_where_it_is_visible(), open_screen(), quick_settings_row_has_a_switch_and_opens_the_screen(), set_gps(), settings_has_a_gps_entry(), the_switch_is_dead_while_starting_and_while_offline() (+6 more)

### Community 306 - "render"
Cohesion: 0.19
Nodes (16): can_call(), fmt_when(), in_call(), key_box(), notice_line(), paint_badge(), phone(), Bubble (+8 more)

### Community 307 - "LockScreen"
Cohesion: 0.21
Nodes (6): drag(), LockScreen, Out, None, Redraw, Submit

### Community 308 - "src/topbar.rs"
Cohesion: 0.14
Nodes (14): ALL, BAR_H, Chip, chip_content(), DATE_MIN, GAP, has_figures(), Layout (+6 more)

### Community 309 - "run"
Cohesion: 0.25
Nodes (13): a_silent_controller_stops_the_sequence_after_one_timeout(), build_plan(), closed_controller_aborts_without_hanging(), fake_controller(), full_sequence_against_a_scripted_controller(), initialise(), InitReport, pair() (+5 more)

### Community 310 - "arc"
Cohesion: 0.19
Nodes (5): Conn, defaults_switch_persist_and_sample(), NET0, Rig, stored_switches_are_read_at_start_and_missing_sources_are_unavailable()

### Community 311 - "launch"
Cohesion: 0.18
Nodes (8): launch(), spawn(), `launch` with a file (security model), Multitasking and windows (owner request 2026-11-11, change `multitasking-windows`), M6.1 process manager, M6.2 installed-only apps, M6.3 crash notice and return to the shell, 1. Host, 1b. Media viewer (owner request 2026-11-11, docs/41-media.md), 2. Device (needs the tablet; no flashing)

### Community 312 - "tests/store.rs"
Cohesion: 0.29
Nodes (16): load_catalog(), catalog_and_settings_files(), app_list_follows_apps_d(), catalogue_data(), default_app_is_chosen_from_the_installed_candidates(), entry(), failed_install_shows_the_error_and_can_be_dismissed(), GIB (+8 more)

### Community 313 - "tests/mobile.rs"
Cohesion: 0.33
Nodes (8): apn_sheet_saves_and_imei_shows_on_request(), data_failure_and_header_bars(), notice(), quick_settings_row_opens_the_screen_and_the_switch_toggles_data(), registered(), set_mobile(), settings_has_a_mobile_network_entry(), sim_pin_sheet_sends_a_secret_and_waits_for_the_answer()

### Community 314 - "PhoneModel"
Cohesion: 0.19
Nodes (9): call_kind_text(), CallEntry, CallKind, In, Missed, Out, ContactDraft, ContactEntry (+1 more)

### Community 315 - "Frozen contracts (do not change without an explicit card)"
Cohesion: 0.11
Nodes (17): App environment (every launched app), App store (change app-catalogue-manager), Boot and process model, Config files (keep formats, add keys only), Display, Frozen contracts (do not change without an explicit card), GPS (owner request 2026-11-11; docs/43-gnss.md has the protocol and the experiments), Headphone jack / mic (owner request 2026-11-12, OpenSpec `headphone-mic-jack`) (+9 more)

### Community 317 - "CallTarget"
Cohesion: 0.12
Nodes (18): Call, CallState, Active, Dialing, Ended, Held, Idle, Ringing (+10 more)

### Community 318 - "config.rs"
Cohesion: 0.19
Nodes (11): devd_defaults_and_overrides(), DevdSettings, gestures_from_keys_json(), KEY_SLOTS, NET_MAC_MODES, NET_MASK_OPTIONS, next_key_action(), pretty_action() (+3 more)

### Community 319 - "GpsStatus"
Cohesion: 0.24
Nodes (9): GpsStatus, fix(), coordinates_and_link(), coords_text(), fix(), layout_has_the_position_block_only_with_a_position(), map_link(), st() (+1 more)

### Community 321 - "PhoneTarget"
Cohesion: 0.12
Nodes (17): PhoneTarget, AddContact, Backspace, Call, ClearLog, NewMessage, Reply, Search (+9 more)

### Community 322 - "fbmirror.c"
Cohesion: 0.23
Nodes (11): be32(), cmain(), map(), msleep(), read_ori(), say(), sc6(), slen() (+3 more)

### Community 324 - "tests/lock.rs"
Cohesion: 0.44
Nodes (8): changing_asks_for_the_current_secret_and_returns_to_it_when_wrong(), pin_set(), removing_needs_the_current_secret_and_lock_now_and_auto_lock_are_plain_commands(), security(), set_lock(), setting_a_pin_needs_it_twice_and_sends_a_secret_never_printed(), the_page_says_the_usb_recovery_shell_is_outside_the_lock(), sheet()

### Community 326 - "Tab"
Cohesion: 0.12
Nodes (15): Tab, .ALL, Apps, Bluetooth, Device, Gps, Home, Media (+7 more)

### Community 327 - "ADDED Requirements"
Cohesion: 0.12
Nodes (15): ADDED Requirements, Requirement: Authorised-use notice, Requirement: Honest hardware limits, Requirement: Mount handling, Requirement: Terminal and X11 tools, Requirement: Tool sets, Requirement: Verified install, Scenario: a GUI tool is started in kali-terminal (+7 more)

### Community 328 - "PairEvent"
Cohesion: 0.17
Nodes (14): all(), classify(), colored_prompt_and_overlong_input_do_not_break_the_parser(), PairEvent, Authorize, Confirm, Connected, ConnectFailed (+6 more)

### Community 329 - "SmsStore"
Cohesion: 0.33
Nodes (3): Sms, SmsFile, SmsStore

### Community 330 - "Camera through MediaTek ACDK (2026-11-10)"
Cohesion: 0.13
Nodes (14): 1. The C API of libacdk.so (exports), 2. Command numbers (ACDK_CMD_*), 3. Struct layouts, 4. Open flow, and why the overlay engine can be bypassed, 5. Capture path, preview and output files, 6. The recovered call sequence (what the factory does, and what camacdk does), 7. Environment (exactly what walpad-camera sets up), 8. What is uncertain (+6 more)

### Community 331 - "Evidence"
Cohesion: 0.13
Nodes (14): Device experiment list (SSH, safe order, nothing flashes or erases), E1. NetAgent has its own AT channels (confidence: certain), E2. URC grammar (certain), E3. Interface number = id % 100 (certain), E4. Interface bring-up order (high), E5. The confirmation (certain for the text, high for the necessity), E6. Capability sync at NetAgent start (high), E7. What the RIL itself does to activate (high for syntax, low for details) (+6 more)

### Community 332 - "Sock"
Cohesion: 0.18
Nodes (3): abstract_addr(), NmeaOut, Sock

### Community 333 - "walpad-btbridge/src/main.rs"
Cohesion: 0.18
Nodes (8): Args, log(), main(), on_signal(), open_timeout(), parse_args(), STOP, write_file()

### Community 334 - "Recv"
Cohesion: 0.25
Nodes (8): Done, Job, Recv, Done, Empty, Gone, Worker, Worker<K>

### Community 335 - "IDE / code editor: Lite XL chosen over Geany"
Cohesion: 0.20
Nodes (9): Decision: Lite XL, IDE / code editor: Lite XL chosen over Geany, Method, On-device verification steps (for the owner, after a rebuild + flash), `open_args`: why not the default `["--", "{path}"]`, Result: both are real prebuilt packages, The footprint comparison that actually matters here, What changed (+1 more)

### Community 336 - "render"
Cohesion: 0.21
Nodes (11): blit_at(), duration_badge(), fmt_duration(), fmt_size(), MediaTarget, Item, Refresh, Section (+3 more)

### Community 337 - "MediaModel"
Cohesion: 0.27
Nodes (5): Info, MediaModel, only_what_is_near_the_screen_is_asked_for_once(), sections_filter_by_kind_and_recent_keeps_its_order(), thumbnails_decode_scale_and_are_capped()

### Community 338 - "Section"
Cohesion: 0.18
Nodes (8): layout(), layout_grid_and_list_fit_both_orientations(), Section, .ALL, Music, Photos, Recent, Videos

### Community 339 - "wall"
Cohesion: 0.31
Nodes (9): catalog(), device_controls_do_nothing_while_the_core_is_down(), home_tiles_are_the_pinned_installed_apps(), switches_do_nothing_while_the_core_is_away(), low_battery_is_a_toast_once_not_a_persistent_banner(), offline_shows_the_banner_and_ignores_commands(), status(), the_menu_does_not_open_while_the_core_is_away() (+1 more)

### Community 340 - "topbar_snapshots.rs"
Cohesion: 0.43
Nodes (12): app(), GIB, golden(), header_all_four_items(), header_default_cpu_and_ram(), header_tight_portrait_12h_gps_mark_high_load(), header_unavailable_figures(), header_without_items_or_before_the_first_sample_is_unchanged() (+4 more)

### Community 341 - "logic.h"
Cohesion: 0.25
Nodes (11): Bluetooth (owner request 2026-11-10), What works now (M8.1, host-verified only), read_screen_off(), fb_parse_screen_off(), fb_status_off(), gate_poll_due(), gate_sleep_ms(), gate_update() (+3 more)

### Community 342 - "render_sheet"
Cohesion: 0.19
Nodes (12): PhoneSheet, ClearLog, Contact, DeleteContact, Message, Contact, SheetCall, pt() (+4 more)

### Community 343 - "edit_snapshots.rs"
Cohesion: 0.42
Nodes (8): bar(), clipboard_list_with_entries_and_empty(), fonts(), golden(), list(), sample(), selection_toolbar_pressed_button(), selection_toolbar_text_and_terminal()

### Community 344 - "vpn_snapshots.rs"
Cohesion: 0.37
Nodes (11): app(), connected(), golden(), profiles(), shot(), status(), vpn_connected_portrait_and_landscape(), vpn_empty_and_offline() (+3 more)

### Community 345 - "ADDED Requirements"
Cohesion: 0.15
Nodes (12): ADDED Requirements, Requirement: Default apps, Requirement: Install, Requirement: Space check, Requirement: Uninstall, Requirement: Untrusted packages, Scenario: a local .apk without a trusted signature is selected, Scenario: an install needs 3 GiB with 4 GiB free (+4 more)

### Community 346 - "ADDED Requirements"
Cohesion: 0.15
Nodes (12): ADDED Requirements, Requirement: Close from Recents, Requirement: Home and Back gestures, Requirement: No interference with apps, Requirement: Recents overview, Scenario: a card is swiped up off the strip, Scenario: a short swipe up from the bottom edge, Scenario: Back is pressed in Files (+4 more)

### Community 347 - "ADDED Requirements"
Cohesion: 0.15
Nodes (12): ADDED Requirements, Requirement: Alarms, Requirement: Calculator, Requirement: Notes, Requirement: Time change, Requirement: Timer and stopwatch, Scenario: 12*(3+4) is entered, Scenario: a 1 minute timer is started and the user opens Firefox (+4 more)

### Community 348 - "ADDED Requirements"
Cohesion: 0.15
Nodes (12): ADDED Requirements, Requirement: Locked at boot, Requirement: Locking and unlocking, Requirement: Rate limiting, Requirement: Rescue, Requirement: Setting a lock, Scenario: 5 wrong PINs are entered, Scenario: the correct PIN is entered (+4 more)

### Community 349 - "online"
Cohesion: 0.14
Nodes (22): arming_one_item_then_pressing_another_does_not_fire(), background_layer_is_cached_not_redrawn_every_frame(), centre(), device_tab_controls_send_the_right_commands(), hw_key(), long_power_opens_the_menu_with_cancel_focused_and_tells_the_core(), losing_the_core_closes_the_menu(), online() (+14 more)

### Community 350 - "C"
Cohesion: 0.35
Nodes (3): C, H, XimApp

### Community 352 - "Stats"
Cohesion: 0.24
Nodes (4): Stats, StatsItems, StatsStatus, ALL

### Community 353 - "tests/media.rs"
Cohesion: 0.38
Nodes (9): a_missing_player_or_core_is_a_toast_and_no_command(), a_refused_open_shows_the_core_message(), home_tile_and_apps_panel_tool_open_the_screen_and_start_a_scan(), it(), open_media(), scan_results_fill_the_sections_and_sections_switch_by_tap(), tapping_an_item_opens_it_in_the_matching_player_and_remembers_it(), the_visible_items_are_asked_for_through_tick_once() (+1 more)

### Community 354 - "recents_snapshots.rs"
Cohesion: 0.35
Nodes (8): cards(), fake_window(), fonts(), golden(), recents_card_dragged_up_and_empty(), recents_five_cards_scroll(), recents_three_cards_portrait_and_landscape(), shot()

### Community 355 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: File receive, Requirement: Honest capability, Requirement: Pairing, Requirement: Power and scan, Requirement: Wi-Fi isolation, Scenario: a Bluetooth keyboard is paired without hidp.ko, Scenario: a phone sends a photo (+3 more)

### Community 356 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Clipboard persistence, Requirement: History, Requirement: No persistence of secrets, Requirement: Touch edit popup, Scenario: 30 different items are copied, Scenario: log files are searched for a copied string, Scenario: text is copied in Firefox and Firefox is closed (+3 more)

### Community 357 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Audio safety, Requirement: Night light, Requirement: Text size, Requirement: Volume channels, Requirement: Wallpaper, Scenario: a picture is chosen, Scenario: Large is chosen (+3 more)

### Community 358 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Emoji, Requirement: Fonts, Requirement: Language switching, Requirement: Layout data, Requirement: Unicode commit, Scenario: a malformed bn file is installed, Scenario: an emoji is tapped (+3 more)

### Community 359 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: App drawer and search, Requirement: Folders, Requirement: Layout safety, Requirement: Orientation, Requirement: Widgets, Scenario: launcher.json is truncated, Scenario: the clock widget is added (+3 more)

### Community 360 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Audio safety, Requirement: Document and image viewing, Requirement: Screen recording, Requirement: Screenshot, Requirement: Video playback, Scenario: a 48 kHz file is played, Scenario: a 720p30 MP4 plays for 5 minutes (+3 more)

### Community 361 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Several terminals, Requirement: Struts are respected, Requirement: Window modes, Scenario: a second terminal, Scenario: closing one, Scenario: free windows stay inside, Scenario: keyboard up (+3 more)

### Community 362 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Credentials, Requirement: Network places, Requirement: SFTP access, Requirement: Storage screen, Requirement: Timeouts, Scenario: a PC runs sftp with the owner's key, Scenario: an SMB place is added with credentials (+3 more)

### Community 363 - "ADDED Requirements"
Cohesion: 0.17
Nodes (11): ADDED Requirements, Requirement: Clean teardown, Requirement: DNS and proxy, Requirement: Kill switch, Requirement: Secret handling, Requirement: WireGuard, Scenario: a DNS server is set, Scenario: a valid config is enabled (+3 more)

### Community 365 - ".next_packet"
Cohesion: 0.25
Nodes (8): check_single(), frame_len(), FrameError, Oversize, UnknownType, Next, Dropped, Packet

### Community 366 - "ContactStore"
Cohesion: 0.25
Nodes (5): digits(), Contact, ContactsFile, ContactStore, same_number()

### Community 367 - "CallAct"
Cohesion: 0.25
Nodes (10): CallAct, Answer, Dtmf, Hangup, Mute, Speaker, Out, Act (+2 more)

### Community 368 - "Terminal: default shell with autosuggestions/completion (tabs already shipped)"
Cohesion: 0.22
Nodes (8): Device risk, Open questions, Out of scope, Secrets impact, Terminal: default shell with autosuggestions/completion (tabs already shipped), Verification on the tablet (not done by the agent that wrote this; needs a rebuilt+flashed image), What changes, Why

### Community 370 - "Kind"
Cohesion: 0.18
Nodes (8): Item, Kind, Audio, Image, Video, player_for(), scan(), View

### Community 371 - "media_snapshots.rs"
Cohesion: 0.44
Nodes (10): app(), golden(), item(), library(), photos_grid_portrait_and_landscape(), scanning_and_empty_states(), shot(), status() (+2 more)

### Community 372 - "wmbar_snapshots.rs"
Cohesion: 0.40
Nodes (9): a_pressed_tab_and_a_pressed_snap_button(), fonts(), free_mode_title_bar_has_snap_buttons(), golden(), model(), shot(), six_tabs_in_portrait_use_numbers_and_lose_the_plus(), terminal_tabs_portrait_and_landscape() (+1 more)

### Community 373 - "2. How it was recovered (static analysis only)"
Cohesion: 0.18
Nodes (10): 1. The problem, 2.1 Transport (confidence: high), 2.2 Framing (confidence: high), 2.3 HAL -> mnld commands (the `hal2mnl_*` functions, 0x6020-0x76e8), 2.4 mnld -> HAL messages (confirmed from both sides), 2.5 What mnld needs (confidence: medium, from strings), 2. How it was recovered (static analysis only), 3. Implementation (+2 more)

### Community 374 - "M5.11 Mobile network / SIM management (owner request 2026-11-10)"
Cohesion: 0.18
Nodes (10): Checks run, Dependencies, Deviations, Device checks pending, Files changed, M5.11 Mobile network / SIM management (owner request 2026-11-10), Next card, Rollback (+2 more)

### Community 375 - "M8 display efficiency (M8.1 done on the host; M8.2 and M8.3 are owner measurements)"
Cohesion: 0.18
Nodes (10): Checks run, Deviations, Device checks pending (owner, in this order), M8.2 measurement: scripts/device/measure_fbmirror.sh, M8.3 measurement: scripts/device/measure_firefox_mem.sh, M8 display efficiency (M8.1 done on the host; M8.2 and M8.3 are owner measurements), Measurements, Next card (+2 more)

### Community 376 - "media_test.sh"
Cohesion: 0.29
Nodes (10): bad(), eq(), FAKE_CALLS, FAKE_DIR, ok(), media_test.sh script, th(), WALPAD_THUMB_DIR (+2 more)

### Community 377 - "vpn.sh"
Cohesion: 0.33
Nodes (7): bad(), conf(), eq(), ok(), reset(), vpn.sh script, vpn()

### Community 378 - "App switcher and navigation"
Cohesion: 0.20
Nodes (9): back(), Control socket `input` topic (core -> shell), App switcher and navigation, Device risk, Open questions, Out of scope, Verification, What changes (+1 more)

### Community 379 - "appdefs.rs"
Cohesion: 0.31
Nodes (6): APPS_D, apps_d_extends_but_never_shadows_apps_json(), APPS_JSON, entries(), entry_ok(), ids()

### Community 381 - "volume_snapshots.rs"
Cohesion: 0.47
Nodes (7): app(), golden(), quick_settings_volume_states(), settings_sound_page(), shot(), status(), volume_pill_for_a_key_press()

### Community 382 - "ADDED Requirements"
Cohesion: 0.20
Nodes (9): ADDED Requirements, Requirement: Bounds, Requirement: Do Not Disturb, Requirement: Notification service, Requirement: Shade, Scenario: 200 notifications are sent in a loop, Scenario: DND is on and a normal notification arrives, Scenario: `notify-send Hi body` runs (+1 more)

### Community 383 - "wpdprops64b.c"
Cohesion: 0.47
Nodes (9): load(), property_get(), property_set(), s_close(), s_open(), s_read(), s_strlen(), s_write() (+1 more)

### Community 384 - "Requirement: Headphone output route"
Cohesion: 0.22
Nodes (8): ADDED Requirements, Requirement: Headphone output route, Requirement: Microphone capture path, Scenario: built-in mic selected, Scenario: headset mic selected, Scenario: no live jack-insert detection, Scenario: owner clears the override, Scenario: owner forces headphone output

### Community 385 - "Multitasking: several terminals and PC-like windows"
Cohesion: 0.22
Nodes (8): Device risk, Multitasking: several terminals and PC-like windows, Open questions, Out of scope, Secrets impact, Verification on the tablet (not done by the agent that wrote this), What changes, Why

### Community 386 - "ZT9101 USB Wi-Fi dongle driver for WalPad (station mode, monitor mode, injection)"
Cohesion: 0.22
Nodes (8): Device risk, Open questions, Out of scope (unchanged), Status: done and verified on-device (2026-11-12), Verification (actual results, 2026-11-12), What changed, Why, ZT9101 USB Wi-Fi dongle driver for WalPad (station mode, monitor mode, injection)

### Community 387 - "callaudio.sh"
Cohesion: 0.39
Nodes (8): bad(), eq(), ok(), run(), callaudio.sh script, T_BAD, T_CALLS, WALPAD_CALLAUDIO_ROOT

### Community 388 - "jack.sh"
Cohesion: 0.39
Nodes (8): bad(), eq(), ok(), run(), jack.sh script, T_BAD, T_CALLS, WALPAD_AUDIO_ROOT

### Community 390 - "MobileReg"
Cohesion: 0.25
Nodes (8): combine_reg(), MobileReg, Denied, Home, NotRegistered, Roaming, Searching, Unknown

### Community 391 - ".spawn"
Cohesion: 0.16
Nodes (8): field(), Run, Verb, Apply, Confirm, Down, Status, Up

### Community 392 - "StatItem"
Cohesion: 0.25
Nodes (6): StatItem, .ALL, Cpu, Net, Ram, Rom

### Community 393 - "input.rs"
Cohesion: 0.25
Nodes (10): discover(), discovers_by_name_not_number(), ensure_node(), existing_node_is_left_alone(), find(), InputDevice, key_device_name(), key_held() (+2 more)

### Community 394 - "Event"
Cohesion: 0.09
Nodes (15): CursorHider, hide(), CHUNK_BYTES, converts_rgba_to_bgrx_for_a_sub_rect(), present(), rgba_to_bgrx(), rows_per_chunk(), TITLE (+7 more)

### Community 395 - ".new"
Cohesion: 0.20
Nodes (10): builders_match_the_hal_encoding(), cleanup(), delete_aiding(), garbage_is_rejected_not_panicked_on(), init(), inject_time(), position_mode(), Rd (+2 more)

### Community 396 - "Bluetooth with BlueZ (2026-11-10)"
Cohesion: 0.25
Nodes (7): Bluetooth with BlueZ (2026-11-10), /dev/stpbt protocol (from `drivers/misc/mediatek/connectivity/bt/mt66xx/wmt/stp_chrdev_bt.c`, matching the strings of the shipped `/vendor/lib/modules/bt_drv.ko`), Device experiments for "no device found" (2026-11-11, not run yet), Device test plan, How it fits together, Recovered vendor init sequence (static analysis, 2026-11-11), The module build (`scripts/build_bt_module.sh`)

### Community 397 - "Smooth battery percent and time left"
Cohesion: 0.25
Nodes (7): Device risk, Out of scope, Secrets impact, Smooth battery percent and time left, Verification, What changes, Why

### Community 398 - "Bluetooth settings"
Cohesion: 0.25
Nodes (7): Bluetooth settings, Device risk, Open questions, Out of scope, Verification, What changes, Why

### Community 399 - "Clipboard and touch text editing"
Cohesion: 0.25
Nodes (7): Clipboard and touch text editing, Device risk, Open questions, Out of scope, Verification, What changes, Why

### Community 400 - "Clock, alarms, calculator and notes"
Cohesion: 0.25
Nodes (7): Clock, alarms, calculator and notes, Device risk, Open questions, Out of scope, Verification, What changes, Why

### Community 401 - "Device event notifications"
Cohesion: 0.25
Nodes (7): Device event notifications, Device risk, Out of scope, Secrets impact, Verification, What changes, Why

### Community 402 - "Display and sound settings"
Cohesion: 0.25
Nodes (7): Device risk, Display and sound settings, Open questions, Out of scope, Verification, What changes, Why

### Community 403 - "Headphone jack and microphone"
Cohesion: 0.25
Nodes (7): Device risk, Headphone jack and microphone, Out of scope, Secrets impact, Verification, What changes, Why

### Community 404 - "Security-testing mode (Kali chroot)"
Cohesion: 0.25
Nodes (7): Device risk, Open questions, Out of scope, Security-testing mode (Kali chroot), Verification, What changes, Why

### Community 405 - "Keyboard languages, Bangla and emoji"
Cohesion: 0.25
Nodes (7): Device risk, Keyboard languages, Bangla and emoji, Open questions, Out of scope, Verification, What changes, Why

### Community 406 - "Lock screen and screen lock"
Cohesion: 0.25
Nodes (7): Device risk, Lock screen and screen lock, Open questions, Out of scope, Verification, What changes, Why

### Community 407 - "Media apps, screenshots and screen recording"
Cohesion: 0.25
Nodes (7): Device risk, Media apps, screenshots and screen recording, Open questions, Out of scope, Verification, What changes, Why

### Community 408 - "Network shares, SFTP and storage manager"
Cohesion: 0.25
Nodes (7): Device risk, Network shares, SFTP and storage manager, Open questions, Out of scope, Verification, What changes, Why

### Community 409 - "Top bar stats"
Cohesion: 0.25
Nodes (7): Device risk, Out of scope, Secrets impact, Top bar stats, Verification, What changes, Why

### Community 410 - "VPN, proxy and DNS"
Cohesion: 0.25
Nodes (7): Device risk, Open questions, Out of scope, Verification, VPN, proxy and DNS, What changes, Why

### Community 411 - "Wi-Fi monitor mode driver"
Cohesion: 0.25
Nodes (7): Device risk, Open questions, Out of scope, Verification, What changes, Why, Wi-Fi monitor mode driver

### Community 412 - "gps.sh"
Cohesion: 0.46
Nodes (5): bad(), eq(), gps(), ok(), gps.sh script

### Community 413 - "assert_snapshot_in"
Cohesion: 0.14
Nodes (16): bottom_shot(), dock_handle_collapsed_and_expanded(), dock_portrait_and_landscape(), dock_pressed_buttons(), golden(), shot(), golden(), keyboard_portrait_and_landscape() (+8 more)

### Community 417 - "ShellInput"
Cohesion: 0.29
Nodes (7): ShellInput, KeyboardHide, KeyboardShow, PowerMenuOpen, PowerShort, VolumeDown, VolumeUp

### Community 418 - "PhoneView"
Cohesion: 0.29
Nodes (6): PhoneView, .ALL, Contacts, Keypad, Messages, Recents

### Community 420 - "Headphone jack and microphone (owner request 2026-11-12, change `headphone-mic-jack`)"
Cohesion: 0.29
Nodes (6): Confirmed real (on the device, 2026-11-12), Does this tablet have a built-in microphone?, Headphone jack and microphone (owner request 2026-11-12, change `headphone-mic-jack`), Scope decision: no accdet insmod in this change, What the owner should test first (in this order, each with a timeout / a way to recover), What this change does

### Community 421 - "UI polish: remove box-in-box borders (owner request 2026-11-12)"
Cohesion: 0.29
Nodes (6): Spacing and typography audit, UI polish: remove box-in-box borders (owner request 2026-11-12), Verification, What changed, What the screenshots showed, What was kept, and why

### Community 422 - "App catalogue and package manager"
Cohesion: 0.29
Nodes (6): App catalogue and package manager, Device risk, Out of scope, Verification, What changes, Why

### Community 423 - "ADDED Requirements"
Cohesion: 0.29
Nodes (6): ADDED Requirements, Requirement: Calm battery percent, Requirement: Time left, Scenario: brightness is raised, Scenario: first minutes, Scenario: reboot

### Community 424 - "Camera bring-up on WalPad OS"
Cohesion: 0.29
Nodes (6): Camera bring-up on WalPad OS, Device risk, Out of scope, Verification, What changes, Why

### Community 425 - "Notification centre"
Cohesion: 0.29
Nodes (6): Device risk, Notification centre, Out of scope, Verification, What changes, Why

### Community 426 - "ADDED Requirements"
Cohesion: 0.29
Nodes (6): ADDED Requirements, Requirement: Live figures in the header, Requirement: Switches, Scenario: all switches off, Scenario: defaults, Scenario: small screen

### Community 427 - "USB Wi-Fi dongle: find the chip and the driver (host-PC side flow)"
Cohesion: 0.33
Nodes (5): Fast path (do this first next time, no VM needed), Linux driver result (2026-11-12, Kali VM kernel 7.2.15; the host was never touched), USB Wi-Fi dongle: find the chip and the driver (host-PC side flow), VM recipe (only if the catalog route fails), What was done this time (and what cost time)

### Community 428 - "touch.lua"
Cohesion: 0.47
Nodes (3): double(), now(), tap()

### Community 429 - "AddrSource"
Cohesion: 0.33
Nodes (5): AddrSource, Generated, Nvram, Override, Stored

### Community 430 - "serial_pull.py"
Cohesion: 0.33
Nodes (7): carve(), connect(), drain(), Lost, main(), pull(), server_window()

### Community 431 - "Hit"
Cohesion: 0.33
Nodes (5): Hit, Backspace, Eye, Key, Submit

### Community 432 - "Value"
Cohesion: 0.67
Nodes (5): json_parsers_are_tolerant(), parse_calls(), parse_contacts(), parse_sms(), s()

### Community 433 - "st"
Cohesion: 0.47
Nodes (6): chip_texts_and_unavailable_figures(), chips_never_overlap_the_clock_or_the_pill(), degrades_in_order_date_then_short_text_then_chips(), fonts(), nothing_is_drawn_before_the_first_sample_or_with_everything_off(), st()

### Community 434 - "camera-bringup/tasks.md"
Cohesion: 0.33
Nodes (5): 1. Probe (read-only), 2. Factory ACDK test (owner OK needed: possible nvdata writes, display takeover), 3. ACDK helper + camera app, 4. Device bring-up of the helper (docs/31-camera-acdk.md section 9), 5. Fallbacks

### Community 435 - "State"
Cohesion: 0.40
Nodes (4): State, Fix, Off, Searching

### Community 436 - "MobileData"
Cohesion: 0.19
Nodes (9): BluetoothController, Absent, Present, Starting, MobileData, Connecting, Failed, Off (+1 more)

### Community 438 - "Battery percent and time left (owner request 2026-11-11)"
Cohesion: 0.40
Nodes (4): Battery percent and time left (owner request 2026-11-11), Design (crates/walpad-core/src/logic/soc.rs), Limits, Research summary (short)

### Community 439 - "Requirement: Still photo from the built-in camera"
Cohesion: 0.40
Nodes (4): ADDED Requirements, Requirement: Still photo from the built-in camera, Scenario: Camera unavailable, Scenario: Capture from the camera app

### Community 440 - "device-notifications/tasks.md"
Cohesion: 0.40
Nodes (4): 1. Core and contract, 2. Shell, 3. Check, 4. Device (needs the tablet; no flashing; not done in this change)

### Community 441 - "top-bar-stats/tasks.md"
Cohesion: 0.40
Nodes (4): 1. Core and contract, 2. Shell, 3. Check, 4. Device (needs the tablet; no flashing)

### Community 442 - "pkg_test.sh"
Cohesion: 0.60
Nodes (3): pk(), reset_pkg(), pkg_test.sh script

### Community 444 - "Media: photos, videos and music (owner request 2026-11-11, change `media-apps-capture`)"
Cohesion: 0.25
Nodes (7): Hardware limits (honest), Install on a fresh image, Media: photos, videos and music (owner request 2026-11-11, change `media-apps-capture`), Player behaviour (mpv), The Media screen, `walpad-thumb`, What is where

### Community 445 - "Code editor app"
Cohesion: 0.25
Nodes (7): Code editor app, Device risk, Out of scope, Secrets impact, Verification, What changes, Why

### Community 446 - "Thumb"
Cohesion: 0.50
Nodes (4): Thumb, Missing, Pending, Ready

### Community 447 - "ClccStat"
Cohesion: 0.29
Nodes (7): ClccStat, Active, Alerting, Dialing, Held, Incoming, Waiting

### Community 448 - "GpsState"
Cohesion: 0.29
Nodes (7): GpsState, Failed, Fix, Off, Searching, Starting, Stopping

### Community 450 - "headphone-mic-jack/tasks.md"
Cohesion: 0.50
Nodes (3): 1. Research (read-only, device + kernel source), 2. Host, 3. Device (not part of this change; for the owner, in order)

### Community 451 - "SimState"
Cohesion: 0.29
Nodes (7): SimState, Absent, Busy, PinRequired, PukRequired, Ready, Unknown

### Community 467 - "LockAct"
Cohesion: 0.29
Nodes (7): LockAct, Change, CycleAuto, Now, Remove, SetPassword, SetPin

### Community 497 - "Terminal tabs and shell autocomplete (owner request 2026-11-12)"
Cohesion: 0.29
Nodes (6): 1. Terminal tabs: already shipped, no new code, 2. Shell: `fish` (autosuggestions + completion, zero config), Terminal tabs and shell autocomplete (owner request 2026-11-12), Unverified (needs the owner's real device, not done here), What changed, Why not `tabbed` (suckless)

### Community 498 - "ADDED Requirements"
Cohesion: 0.29
Nodes (6): ADDED Requirements, Requirement: Code editor app entry, Requirement: Package footprint evidence, Scenario: availability check, Scenario: launch from the Apps panel, Scenario: open a file

### Community 499 - "code-editor-app/tasks.md"
Cohesion: 0.29
Nodes (6): 1. Package availability (real evidence, not a guess), 2. Decision and docs, 3. Image, 4. UI, 5. Check, 6. Device (read-only only; no install, no flash — needs the owner's explicit approval first)

### Community 500 - "walpad-gnss/src/main.rs"
Cohesion: 0.28
Nodes (5): log(), main(), on_signal(), parse_args(), STOP

### Community 501 - "ADDED Requirements"
Cohesion: 0.29
Nodes (6): ADDED Requirements, Requirement: No second terminal-tabs mechanism is introduced, Requirement: Terminal sessions start an autosuggesting shell, Scenario: opening the terminal, Scenario: SSH and the Kali profile are unaffected, Scenario: tabbed is not packaged, and conflicts with the existing window bar

### Community 502 - "decode"
Cohesion: 0.36
Nodes (9): decode(), Msg, Capabilities, Location, Nmea, Other, RequestUtcTime, Status (+1 more)

### Community 503 - "Rd<'a>"
Cohesion: 0.47
Nodes (3): Rd<'a>, status_nmea_and_requests(), string_and_binary_encoding()

### Community 505 - "Location"
Cohesion: 0.29
Nodes (5): f32_at(), f64_at(), loc(), Location, location_roundtrip_and_flags()

### Community 506 - "5. Security-testing mode (Kali chroot)"
Cohesion: 0.33
Nodes (6): 5.1 Installing Kali, 5.2 Mounts and launching, 5.3 Package sets (inside the chroot; `apt install` after `apt update`), 5.4 X11 tools in the existing session, 5.5 Hardware limits and options, 5. Security-testing mode (Kali chroot)

### Community 507 - "Verb"
Cohesion: 0.40
Nodes (4): Verb, Start, Status, Stop

### Community 508 - "codec.rs"
Cohesion: 0.32
Nodes (4): LineDecoder, LineError, TooLong, MAX_LINE

### Community 509 - "NotificationCategory"
Cohesion: 0.25
Nodes (8): NotificationCategory, Bluetooth, Charging, Gps, Hotspot, MobileData, Storage, UsbDevice

### Community 510 - "ModemState"
Cohesion: 0.33
Nodes (6): ModemState, Absent, Booting, Failed, Present, Ready

### Community 511 - "terminal-tabs-and-autocomplete/tasks.md"
Cohesion: 0.50
Nodes (3): 1. Research (done, evidence in docs/46-terminal-tabs-and-autocomplete.md), 2. Host (config/shell-script/docs only, no Rust touched), 3. Device (needs the tablet; no flashing by this change)

### Community 513 - "VpnState"
Cohesion: 0.33
Nodes (6): VpnState, Failed, Off, On, Starting, Stopping

### Community 516 - "Engine"
Cohesion: 0.50
Nodes (4): Engine, Off, On, Unknown

## Knowledge Gaps
- **2166 isolated node(s):** `apply.sh script`, `walpad-btbridge`, `ACL`, `EVENT`, `VENDOR` (+2161 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 3171 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **177 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Target` connect `Target` to `tap`, `src/bluetooth.rs`, `gps_screen.rs`, `Rect`, `wmbar.rs`, `StatItem`, `Frame`, `src/mobile.rs`, `settings_screen.rs`, `components`, `src/media.rs`, `vpn_screen.rs`, `panels.rs`, `network.rs`, `phone_ui.rs`, `render`, `App`, `Cmd`, `chrome.rs`, `PhoneTarget`, `render`, `LockAct`, `app.rs`, `render_sheet`, `notifications_ui.rs`, `device_screen.rs`, `online`, `Model`, `Kind`, `src/store.rs`, `volume_snapshots.rs`?**
  _High betweenness centrality (0.059) - this node is a cross-community bridge._
- **What connects `apply.sh script`, `walpad-btbridge`, `ACL` to the rest of the system?**
  _2166 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `tap` be split into smaller, more focused modules?**
  _Cohesion score 0.12580943570767808 - nodes in this community are weakly interconnected._
- **Why does `_` connect `_` to `x.rs`, `src/bluetooth.rs`, `mousepad.rs`, `gps_screen.rs`, `Gesture`, `wmbar.rs`, `src/components.rs`, `dock.rs`, `seltools.rs`, `Frame`, `src/mobile.rs`, `call_screen.rs`, `settings_screen.rs`, `components`, `src/media.rs`, `vpn_screen.rs`, `panels.rs`, `Icon`, `text.rs`, `lock_screen.rs`, `Canvas`, `network.rs`, `tests/components.rs`, `phone_ui.rs`, `super`, `src/topbar.rs`, `chrome.rs`, `art.rs`, `app.rs`, `notifications_ui.rs`, `device_screen.rs`, `recents.rs`, `recents_snapshots.rs`, `Model`, `src/store.rs`?**
  _High betweenness centrality (0.042) - this node is a cross-community bridge._
- **Should `mousepad.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.050505050505050504 - nodes in this community are weakly interconnected._
- **Why does `Cmd` connect `Cmd` to `XWindow`, `Outcome`, `LockKind`, `StatItem`, `dock.rs`, `wmbar.rs`, `Shell`, `Purpose`, `app.rs`, `App`, `logic/jack.rs`, `Secret`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **Should `Gesture` be split into smaller, more focused modules?**
  _Cohesion score 0.11229946524064172 - nodes in this community are weakly interconnected._