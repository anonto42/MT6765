# Graph Report - walpad.8g  (2026-11-10)

## Corpus Check
- 276 files · ~191,155 words
- Verdict: corpus is large enough that graph structure adds value.
- Unclassified: 40 file(s) not represented in the graph (top: .conf 16, (none) 11, .ttf 4)

## Summary
- 3471 nodes · 7474 edges · 249 communities (135 shown, 114 thin omitted)
- Extraction: 97% EXTRACTED · 3% INFERRED · 0% AMBIGUOUS · INFERRED: 189 edges (avg confidence: 0.86)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `3e1b3900`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- x.rs
- app/tests.rs
- mousepad.rs
- walpad-devd.py
- Shell
- _
- Rect
- src/components.rs
- Command
- dock.rs
- Target
- padui.c
- Daemon
- Console
- walpad-launcher.py
- Frame
- key_gestures.rs
- src/apps.rs
- tests/apps.rs
- touch_map.rs
- core_client.rs
- settings_screen.rs
- Model
- rotation.rs
- os
- screen_timeout.rs
- device_screen.rs
- vendor_up.py
- evdev.rs
- keyboard.rs
- panels.rs
- Icon
- text.rs
- time
- XError
- .new
- live.rs
- network.rs
- screens_snapshots.rs
- ErrorCode
- tests/components.rs
- Root
- sys
- parse.rs
- Fonts
- Wifi
- Brain
- .new
- input.rs
- build
- path
- hotspot_ipc.rs
- ipc.rs
- App
- Cmd
- chrome.rs
- display_dashboard.py
- run.sh
- Effect
- walpad-keys.py
- walpad-ctl/src/lib.rs
- logic/battery.rs
- Inputs
- src/tests.rs
- Canvas
- daemon.rs
- walpad-touchd.py
- status.rs
- gsensor.rs
- Outcome
- shadow.rs
- Hotspot
- power_key.rs
- walpad-x/src/lib.rs
- argparse
- wifi.rs
- art.rs
- SysKey
- ctrl.rs
- wpa/fake.rs
- manager.rs
- app.rs
- WiFi bring-up plan — WalPad 8G (MT6765)
- terminal_ui.py
- keys_py_test.py
- device_recon.py
- struct
- wifi_ipc.rs
- M9.1-M9.3 Log caps, safe mode, reproducible build
- root
- src/hotspot.rs
- config.rs
- Tint
- M7.1 Python-free image as an explicit build mode
- serial_pull.py
- dmlinear.c
- Plan: make the WalPad 8G a real device (browser, terminal, Wi-Fi, touch UI)
- home_snapshots.rs
- WalPad Shell UI spec: line-art theme (v2, 2026-11-10; changes go through the owner)
- Tnet
- super
- .configure
- GetKeys
- .on_tap
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
- codec.rs
- Phases
- 2026-08-18 — host-side recon; console access is the one blocker
- 2026-08-20 — CONNSYS stack fully initialises; wlan firmware start still fails
- 2026-11-07 / 08 — attempt to restore Android, root causes found, switch to a Linux touch UI
- walpad-shell
- measure_fbmirror.sh
- services_ctl.sh
- Project Memory (WalPad 8G)
- HotspotState
- Accessory
- 2026-08-16 — WalPad-Server USB gadget discovered; BROM mode reached
- 2026-08-20 (evening) — wlan0 EXISTS.  CONNSYS + firmware fully up.
- Status log
- host_session.sh
- m3_check.sh
- wifi_assoc.sh
- Agent Startup (WalPad 8G)
- WifiRadio
- 2026-08-20 — WiFi works end to end: `ssh root@192.168.0.100` over the built-in radio
- deploy.sh
- smoke.sh
- wifi_up.sh
- write_600
- BatterySource
- HotspotInternet
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
- core.md
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

## God Nodes (most connected - your core abstractions)
1. `Rect` - 122 edges
2. `_` - 86 edges
3. `Target` - 75 edges
4. `Painter` - 75 edges
5. `Root` - 70 edges
6. `Daemon` - 60 edges
7. `Icon` - 57 edges
8. `App` - 49 edges
9. `online()` - 46 edges
10. `Frame` - 43 edges

## Surprising Connections (you probably didn't know these)
- `M6.1 process manager, M6.2 installed-only apps, M6.3 crash notice and return to the shell` --references--> `launch()`  [INFERRED]
  docs/rust/reports/M6.1-M6.3.md → config/rootfs/usr/local/bin/walpad-keys.py
- `M5.9-prep Quad tap launches the browser in both runtimes; shell capability advertised` --references--> `do()`  [INFERRED]
  docs/rust/reports/M5.9-prep.md → config/rootfs/usr/local/bin/walpad-keys.py
- `Wi-Fi switch and hotspot (owner request 2026-11-10)` --references--> `wifi()`  [INFERRED]
  docs/rust/CONTRACTS.md → config/rootfs/usr/local/bin/walpad-launcher.py
- `M4 Wi-Fi control through the supplicant socket` --references--> `wifi()`  [INFERRED]
  docs/rust/reports/M4.md → config/rootfs/usr/local/bin/walpad-launcher.py
- `M8.2 measurement: scripts/device/measure_fbmirror.sh` --references--> `screen_off()`  [INFERRED]
  docs/rust/reports/M8.md → scripts/power_button.py

## Import Cycles
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/chrome.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/device_screen.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/overlay.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/network.rs -> crates/walpad-shell/src/app.rs`
- 2-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/settings_screen.rs -> crates/walpad-shell/src/app.rs`
- 3-file cycle: `crates/walpad-shell/src/app.rs -> crates/walpad-shell/src/settings_screen.rs -> crates/walpad-shell/src/device_screen.rs -> crates/walpad-shell/src/app.rs`
- 3-file cycle: `crates/walpad-shell/src/present.rs -> crates/walpad-x/src/lib.rs -> crates/walpad-shell/src/window.rs -> crates/walpad-shell/src/present.rs`

## Communities (249 total, 114 thin omitted)

### Community 0 - "x.rs"
Cohesion: 0.05
Nodes (44): keysym(), parse_key_spec(), struts_pointer_keys_dock_against_real_xvfb(), XBridge, CHUNK_BYTES, converts_rgba_to_bgrx_for_a_sub_rect(), present(), rgba_to_bgrx() (+36 more)

### Community 1 - "app/tests.rs"
Cohesion: 0.09
Nodes (69): an_available_network_with_stored_credentials_connects_without_the_sheet(), app_crash_notice_shows_a_danger_toast_that_expires(), apps_panel_close_badge_closes_the_app_and_keeps_the_panel_open(), apps_screen_launches_closes_and_pins(), arming_one_item_then_pressing_another_does_not_fire(), background_layer_is_cached_not_redrawn_every_frame(), catalog(), centre() (+61 more)

### Community 2 - "mousepad.rs"
Cohesion: 0.06
Nodes (58): ACCEL, ARROW, BRIDGE_LIFT_MS, BUTTON_LEFT, BUTTON_RIGHT, button_row(), BUTTON_WHEEL_DOWN, BUTTON_WHEEL_UP (+50 more)

### Community 3 - "walpad-devd.py"
Cohesion: 0.06
Nodes (49): battery(), battery_guard(), command(), decide(), find_inputs(), load_cfg(), log(), main() (+41 more)

### Community 4 - "Shell"
Cohesion: 0.06
Nodes (32): read_clock_24h(), load_wm_classes(), beyond_slop(), down(), drag_scrolls_with_deltas_and_ends_on_release(), Gesture, Cancel, Drag (+24 more)

### Community 5 - "_"
Cohesion: 0.04
Nodes (56): SURFACE_BG, STRIP, _, ACCENT, BG, BLACK, BOX_DOCK, BOX_GRID (+48 more)

### Community 6 - "Rect"
Cohesion: 0.11
Nodes (14): paint_close_badge(), panel_caption(), panel_icon(), render_apps_panel(), Rect, banner_tint(), icon_for_box(), Painter (+6 more)

### Community 7 - "src/components.rs"
Cohesion: 0.05
Nodes (47): key_at(), ButtonKind, Danger, Primary, Secondary, ButtonState, Default, Disabled (+39 more)

### Community 8 - "Command"
Cohesion: 0.05
Nodes (44): ALLOWED_SETTINGS, BrightnessArgs, Command, CloseApp, GetStatus, Hello, Home, Hotspot (+36 more)

### Community 9 - "dock.rs"
Cohesion: 0.07
Nodes (23): XWindow, app_for_class(), Atoms, button_at(), button_rects(), BUTTONS, command_for(), Dock (+15 more)

### Community 10 - "Target"
Cohesion: 0.04
Nodes (46): Target, AppTile, Avail, Back, BatteryCard, ClockToggle, CloseApp, DialogCancel (+38 more)

### Community 11 - "padui.c"
Cohesion: 0.15
Nodes (43): activate_sel(), atoi_(), battery_pct(), blit_all(), btn_count(), btn_get(), cmain_(), commit() (+35 more)

### Community 12 - "Daemon"
Cohesion: 0.10
Nodes (5): Daemon, read_file(), write_atomic(), class_matches(), parse_devd_line()

### Community 13 - "Console"
Cohesion: 0.08
Nodes (18): Three bugs worth remembering, Tooling added, main(), prepare(), send(), unpack(), wrappers(), Console (+10 more)

### Community 14 - "walpad-launcher.py"
Cohesion: 0.07
Nodes (20): battery(), Home, rd(), wifi(), App, Keyboard, scan_results(), status() (+12 more)

### Community 15 - "Frame"
Cohesion: 0.08
Nodes (33): app_sheet_geom(), app_sheet_has_close_only_when_running(), AppSheet, Overlay, App, AppsPanel, Forget, Password (+25 more)

### Community 16 - "key_gestures.rs"
Cohesion: 0.11
Nodes (31): Action, Ctl, Home, Key, Keyboard, Launch, None, PowerMenu (+23 more)

### Community 17 - "src/apps.rs"
Cohesion: 0.09
Nodes (25): app_that_disappears_from_disk_is_unlisted_again(), AppDef, Apps, bad_json_means_no_apps(), BIN_DIRS, closing_child_that_ignores_sigterm_is_killed_after_the_grace_period(), CONTRACT_ENV, Exit (+17 more)

### Community 18 - "tests/apps.rs"
Cohesion: 0.12
Nodes (18): atom(), Client, close_app_falls_back_to_sigterm_when_the_window_ignores_delete(), close_app_sends_wm_delete_window_to_an_app_we_did_not_start(), crash_publishes_a_notice_and_raises_home_but_clean_exit_and_close_do_not(), kind(), NEXT, Running (+10 more)

### Community 19 - "touch_map.rs"
Cohesion: 0.10
Nodes (23): Act, Move, Press, Release, Cfg, ev(), H, LIFT_MS (+15 more)

### Community 20 - "core_client.rs"
Cohesion: 0.09
Nodes (24): ShellInput, KeyboardHide, KeyboardShow, PowerMenuOpen, PowerShort, VolumeDown, VolumeUp, connects_gets_status_sends_commands_and_reconnects() (+16 more)

### Community 21 - "settings_screen.rs"
Cohesion: 0.09
Nodes (33): CATEGORIES, Ctx, find(), gather(), gather_reads_a_fake_root(), layout(), layout_top_level_and_subpage(), maintenance_has_the_dangerous_actions_as_danger_buttons() (+25 more)

### Community 22 - "Model"
Cohesion: 0.07
Nodes (33): Model, Tile, pinned_items(), render_header(), battery_card(), CELL_H, CELL_W, grid_is_centred_fits_and_wraps() (+25 more)

### Community 23 - "rotation.rs"
Cohesion: 0.10
Nodes (25): Act, DockKeyboard, SetStruts, WriteOrientation, Cfg, decide(), missing_sample_does_nothing(), Rotation (+17 more)

### Community 24 - "os"
Cohesion: 0.11
Nodes (18): ctl(), halt(), Menu, main(), mtk(), detect(), main(), main() (+10 more)

### Community 25 - "screen_timeout.rs"
Cohesion: 0.15
Nodes (19): Act, Backlight, Activity, NoWake, activity_resets_idle_timer(), Wakes, Brightness, Absolute (+11 more)

### Community 26 - "device_screen.rs"
Cohesion: 0.10
Nodes (32): DIM_OPTIONS, height(), hw_label(), info_row(), Item, BrightnessCard, DimSeg, Gap (+24 more)

### Community 27 - "vendor_up.py"
Cohesion: 0.10
Nodes (16): _human(), NotSparse, parse_header(), report(), to_physical(), walk(), assemble(), assembly_plan() (+8 more)

### Community 28 - "evdev.rs"
Cohesion: 0.08
Nodes (26): ev(), ABS_MT_POSITION_X, ABS_MT_POSITION_Y, ABS_X, ABS_Y, BTN_TOUCH, ev(), EV_ABS (+18 more)

### Community 29 - "keyboard.rs"
Cohesion: 0.07
Nodes (25): bottom_strut(), HEIGHT_PCT, KeyMap, layout_fills_both_orientations_with_big_enough_keys(), Stroke, STRUT_CLASS, strut_counts_from_the_screen_edge(), strut_partial() (+17 more)

### Community 30 - "panels.rs"
Cohesion: 0.09
Nodes (30): apps_panel_rect(), apps_panel_sits_next_to_the_sidebar_at_the_bottom(), BADGE_HIT, BADGE_R, CAPTION_H, cell_box(), CELL_GAP, CELL_H (+22 more)

### Community 31 - "Icon"
Cohesion: 0.06
Nodes (31): Icon, .ALL, Apps, Back, Backspace, Battery, Bluetooth, Check (+23 more)

### Community 32 - "text.rs"
Cohesion: 0.12
Nodes (15): drawing_changes_pixels_only_inside_the_box(), ellipsis_fits_the_box(), Face, .ALL, Bold, ExtraLight, Mono, Regular (+7 more)

### Community 33 - "time"
Cohesion: 0.17
Nodes (15): main(), reboot(), wait_gone(), attempt(), main(), stage(), connect(), drain() (+7 more)

### Community 34 - "XError"
Cohesion: 0.16
Nodes (8): Atoms, BottomWindows, create_bottom_windows(), keyboard_rect(), SysKeyboard, connect(), XConn, XError

### Community 35 - ".new"
Cohesion: 0.12
Nodes (10): blit_and_crop_roundtrip(), copy_rect_from_a_strip(), fills_and_reads_back(), origin_offsets_drawing_but_not_reads(), outlines_leave_the_inside_untouched(), paint(), rect_helpers(), rounded_rect_corners_are_cut_and_border_shows() (+2 more)

### Community 36 - "live.rs"
Cohesion: 0.17
Nodes (16): append_events(), cover_closed_turns_screen_off_and_gsensor_rotates(), ev(), flags_decide_which_subsystem_acts(), long_power_opens_the_shell_menu_and_the_keys_are_routed_to_the_shell(), power_key_short_press_turns_screen_off_via_live_device(), put_device(), read() (+8 more)

### Community 37 - "network.rs"
Cohesion: 0.14
Nodes (25): bars_from_dbm(), CARD_H, client_off(), connected_card_and_failed_hint_take_space_only_when_present(), FORGET_W, hotspot_lines(), hotspot_on(), hotspot_status_lines() (+17 more)

### Community 38 - "screens_snapshots.rs"
Cohesion: 0.21
Nodes (27): app(), apps_options_sheet(), apps_panel_portrait_and_landscape(), catalog(), device_tab_portrait_landscape_offline(), forget_dialog_and_toast(), golden(), hotspot_on() (+19 more)

### Community 39 - "ErrorCode"
Cohesion: 0.10
Nodes (18): wifi_err(), ErrorBody, ErrorCode, Busy, Internal, InvalidArgs, NotHello, Refused (+10 more)

### Community 40 - "tests/components.rs"
Cohesion: 0.16
Nodes (19): assert_snapshot(), assert_snapshot_in(), golden_dir(), app_tiles_and_status_cards(), button_states(), controls(), dialogs_sheet_and_power_menu(), header_pill_and_icon_boxes() (+11 more)

### Community 41 - "Root"
Cohesion: 0.14
Nodes (20): Root, BACKLIGHT, backlight_roundtrip_and_missing(), read_backlight(), read_trim(), uptime_s(), write_backlight(), alpine_release() (+12 more)

### Community 42 - "sys"
Cohesion: 0.12
Nodes (10): main(), parse_args(), read_core(), read_python(), cmd(), run(), _find_data_intf(), find_device() (+2 more)

### Community 43 - "parse.rs"
Cohesion: 0.12
Nodes (22): decode_escapes(), Event, Connected, Disconnected, Other, ScanResults, TempDisabled, list_status_signal_events() (+14 more)

### Community 44 - "Fonts"
Cohesion: 0.16
Nodes (5): Marker, MousePad, paint_map(), paint_pad(), Fonts

### Community 45 - "Wifi"
Cohesion: 0.18
Nodes (8): map(), Wifi, WifiError, Busy, Failed, Invalid, Unavailable, WifiLive

### Community 46 - "Brain"
Cohesion: 0.16
Nodes (12): BATTERY_EVERY, Brain, cfg(), Cover, ShellKey, GESTURE_TICK, GSENSOR_EVERY, key() (+4 more)

### Community 47 - ".new"
Cohesion: 0.16
Nodes (9): Client, wifi_event_json(), encode_line(), Topic, Apps, Input, Notice, Status (+1 more)

### Community 48 - "input.rs"
Cohesion: 0.15
Nodes (17): reads_fake_file_and_missing(), discover(), discovers_by_name_not_number(), ensure_node(), existing_node_is_left_alone(), find(), InputDevice, key_device_name() (+9 more)

### Community 49 - "build"
Cohesion: 0.17
Nodes (7): build(), Canvas, draw_icon(), draw_icon_rotated(), every_icon_draws_something_inside_its_box(), icons_scale(), Pen

### Community 50 - "path"
Cohesion: 0.15
Nodes (12): dock_portrait_and_landscape(), dock_pressed_buttons(), golden(), shot(), golden(), keyboard_portrait_and_landscape(), keyboard_states(), shot() (+4 more)

### Community 51 - "hotspot_ipc.rs"
Cohesion: 0.18
Nodes (8): Conn, FAKE_SCRIPT, hotspot_config_start_status_stop(), hotspot_failure_and_timeout_paths(), hotspot_without_script_fails_cleanly(), Rig, SECRET, wifi_switch_persists_and_hides_the_connection()

### Community 52 - "ipc.rs"
Cohesion: 0.23
Nodes (9): brightness_rotate_screen_and_errors(), Conn, Harness, hello_is_required_and_version_checked(), keyboard_command_reaches_the_shell_on_the_input_topic(), oversized_line_closes_connection_and_socket_mode_is_0600(), settings_launch_screenshot_power_and_restart_shell(), status_event_on_change_and_devd_fifo_and_legacy_json() (+1 more)

### Community 53 - "App"
Cohesion: 0.18
Nodes (5): App, AppEntry, load_catalog(), catalog_and_settings_files(), WallTime

### Community 54 - "Cmd"
Cohesion: 0.09
Nodes (18): Cmd, HotspotConfig, KeyboardHide, KeyboardShow, KeyboardToggle, MousePadHide, MousePadShow, Power (+10 more)

### Community 55 - "chrome.rs"
Cohesion: 0.18
Nodes (18): Cached, Chrome, ChromeKey, draw_item(), GAP, NAV_APPS, NAV_HOME, NAV_SETTINGS (+10 more)

### Community 56 - "display_dashboard.py"
Cohesion: 0.19
Nodes (19): command(), commit_display(), cpu_percent(), endpoint(), hardware_status(), indicator(), load_psf(), main() (+11 more)

### Community 57 - "run.sh"
Cohesion: 0.13
Nodes (19): bad(), chk(), conc_combo(), eq(), excl_combo(), fakebin(), hs(), ok() (+11 more)

### Community 58 - "Effect"
Cohesion: 0.16
Nodes (11): Effect, Action, Backlight, CriticalPoweroff, KeyboardDock, LowBattery, Orientation, Pointer (+3 more)

### Community 59 - "walpad-keys.py"
Cohesion: 0.17
Nodes (14): ctl(), do(), find(), Key, keyboard_toggle(), launch(), load(), log() (+6 more)

### Community 61 - "walpad-ctl/src/lib.rs"
Cohesion: 0.17
Nodes (14): build_request(), key_held(), key_held_exit_status(), key_held_status(), legacy_line(), rpc(), rpc_wifi(), run() (+6 more)

### Community 62 - "logic/battery.rs"
Cohesion: 0.17
Nodes (14): Act, CriticalPoweroff, LowChanged, Cfg, d4_consecutive_readings(), Guard, is_discharging(), model() (+6 more)

### Community 63 - "Inputs"
Cohesion: 0.17
Nodes (10): Role, Hall, Key, Touch, Inputs, late_touch_device_is_picked_up(), put(), role_of() (+2 more)

### Community 64 - "src/tests.rs"
Cohesion: 0.13
Nodes (11): c_args(), hotspot_config_is_validated_and_redacted(), legacy_status_has_old_field_set(), parses_basic_commands(), power_defaults_to_unconfirmed(), req(), sample_status(), secret_is_redacted_everywhere() (+3 more)

### Community 66 - "daemon.rs"
Cohesion: 0.10
Nodes (13): APPS_X_EVERY, BATTERY_EVERY, CLIENT_STALL, ExtClose, internal(), LiveState, MAX_OUT, open_fifo() (+5 more)

### Community 67 - "walpad-touchd.py"
Cohesion: 0.17
Nodes (9): find_touch(), load_cal(), log(), main(), open_x(), orientation(), to_logical(), send_reboot() (+1 more)

### Community 68 - "status.rs"
Cohesion: 0.16
Nodes (14): parse_wifi_state(), wifi_state_parsing(), AppState, Battery, Hardware, HotspotStatus, LegacyBattery, LegacyStatus (+6 more)

### Community 69 - "gsensor.rs"
Cohesion: 0.12
Nodes (6): BUF, GSENSOR_IOCTL, GsIoctl, .IS_MUTATING, parse_gsensor(), read_gsensor()

### Community 70 - "Outcome"
Cohesion: 0.23
Nodes (4): Hotspot, Launch, PowerMenuState, Outcome

### Community 71 - "shadow.rs"
Cohesion: 0.27
Nodes (12): cover_close_turns_screen_off(), idle_dims_then_touch_wakes(), lines(), lns(), new(), no_g(), power_short_press_toggles_screen_and_long_opens_menu(), replay() (+4 more)

### Community 73 - "Hotspot"
Cohesion: 0.21
Nodes (7): field(), Hotspot, Run, Verb, Start, Status, Stop

### Community 74 - "power_key.rs"
Cohesion: 0.21
Nodes (12): Act, ActivateMenuItem, OpenPowerMenu, ToggleScreen, WakeScreen, LONG_MS, PowerKey, rule1_long_press_opens_menu_once() (+4 more)

### Community 75 - "walpad-x/src/lib.rs"
Cohesion: 0.17
Nodes (4): TimedStream, wait_any(), wait_readable(), X_TIMEOUT_S

### Community 76 - "argparse"
Cohesion: 0.23
Nodes (9): detect_server(), interactive(), main(), one_shot(), read_all(), wait_for_server(), main(), push() (+1 more)

### Community 78 - "wifi.rs"
Cohesion: 0.32
Nodes (10): connect_wpa2_derives_psk_never_sends_the_passphrase_and_saves_with_mode_600(), Env, open_network_uses_key_mgmt_none_and_timeout_is_reported(), reconnects_when_the_supplicant_restarts(), saved_list_forget_disconnect_and_validation(), scan_is_single_flight_merged_sorted_and_utf8_safe(), secret(), until() (+2 more)

### Community 79 - "art.rs"
Cohesion: 0.19
Nodes (13): both_wallpapers_decode_at_panel_size_with_the_background_colour_on_top(), coffee_rect(), cup_path(), draw_coffee(), rotated_square(), strokes_stay_inside_the_rect(), the_cup_fits_both_orientations_and_keeps_its_aspect(), VIEW_H (+5 more)

### Community 80 - "SysKey"
Cohesion: 0.13
Nodes (16): SYS_ROWS, SysKey, Backspace, Char, Ctrl, Down, Enter, Esc (+8 more)

### Community 81 - "ctrl.rs"
Cohesion: 0.26
Nodes (7): bind_to(), CtrlClient, CtrlError, Down, Fail, Timeout, REQUEST_TIMEOUT

### Community 82 - "wpa/fake.rs"
Cohesion: 0.21
Nodes (8): default_aps(), FakeAp, FakeSupplicant, Net, serve(), spawn(), hex(), unhex()

### Community 83 - "manager.rs"
Cohesion: 0.14
Nodes (13): CONNECT_POLL, CONNECT_TIMEOUT, Op, Connecting, Idle, Scanning, RECONNECT_EVERY, SCAN_TIMEOUT (+5 more)

### Community 84 - "app.rs"
Cohesion: 0.23
Nodes (8): DEFAULT_PINNED, friendly(), POWER_ARM_MS, POWER_MENU_MS, SLIDER_SEND_EVERY_MS, SPIN_EVERY_MS, Toast, TOAST_MS

### Community 85 - "WiFi bring-up plan — WalPad 8G (MT6765)"
Cohesion: 0.14
Nodes (13): Fallback while WiFi is still down, If route C turns out to be necessary, Measured (2026-08-19) — what the on-device survey found, Phase 0 — stop fighting sudo (5 min), Phase 1 — survey (one boot window), Phase 2 — make /vendor reachable (no 4 GB dump needed), Phase 3 — userspace, Phase 4 — connect (+5 more)

### Community 86 - "terminal_ui.py"
Cohesion: 0.22
Nodes (7): box(), commit(), font_load(), main(), mapxy(), px(), txt()

### Community 88 - "device_recon.py"
Cohesion: 0.26
Nodes (8): already_have(), collect(), main(), path_for(), save(), write_index(), device_present(), wait_for_window()

### Community 89 - "struct"
Cohesion: 0.27
Nodes (8): BadMetadata, _descriptor(), human(), parse(), parse_geometry(), parse_metadata(), _read_tables(), report()

### Community 91 - "M9.1-M9.3 Log caps, safe mode, reproducible build"
Cohesion: 0.15
Nodes (12): Checks run, Deviations, Device checks pending (owner; nothing was run on the tablet), M9.1 log caps, M9.1-M9.3 Log caps, safe mode, reproducible build, M9.2 safe mode, M9.3 reproducible build, Measurements (+4 more)

### Community 93 - "src/hotspot.rs"
Cohesion: 0.17
Nodes (8): CONF, DEFAULT_SSID, PATH, SCRIPT, START_TIMEOUT, STATUS_EVERY, STATUS_TIMEOUT, STOP_TIMEOUT

### Community 94 - "config.rs"
Cohesion: 0.27
Nodes (7): devd_defaults_and_overrides(), DevdSettings, gestures_from_keys_json(), pretty_action(), read_devd(), read_gestures(), root_with()

### Community 95 - "Tint"
Cohesion: 0.17
Nodes (7): DANGER_TINT, DANGER_TINT_PRESSED, PRESSED_TINT, SURFACE, SURFACE_ICON, SURFACE_STRONG, Tint

### Community 96 - "M7.1 Python-free image as an explicit build mode"
Cohesion: 0.17
Nodes (11): Capabilities and the guard (no Python to fall back to), Checks run, Deviations, Device checks pending (owner), M7.1 Python-free image as an explicit build mode, Measurements, Owner steps (nothing here was run by me), Rollback (+3 more)

### Community 97 - "serial_pull.py"
Cohesion: 0.30
Nodes (7): carve(), connect(), drain(), Lost, main(), pull(), server_window()

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
Cohesion: 0.18
Nodes (10): Colour tokens (crates/walpad-ui/src/tokens.rs), Components and states, Destinations (how every screen is reached by touch), Frame (crates/walpad-ui/src/layout.rs), Interaction, Rendering and performance (the tablet renders in software on ARM), Screens and popovers, Shape (+2 more)

### Community 103 - "super"
Cohesion: 0.27
Nodes (4): parse_voltage_mv(), PS, read_battery(), reads_tree_and_missing()

### Community 104 - ".configure"
Cohesion: 0.31
Nodes (6): hotspot_err(), HotspotError, Busy, Failed, Invalid, parse_conf()

### Community 106 - ".on_tap"
Cohesion: 0.22
Nodes (7): CloseApp, Rotate, SetBrightness, SetPinned, WifiEnable, WifiForget, native_brightness()

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
Cohesion: 0.28
Nodes (4): derive(), psk_hex(), PskError, BadLength

### Community 115 - "Purpose"
Cohesion: 0.22
Nodes (9): Purpose, Close, Connect, Forget, HotspotConfig, Other, SavedList, Scan (+1 more)

### Community 116 - "Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project"
Cohesion: 0.22
Nodes (9): Automation, How it actually works, Quick start, Repository layout, ⚠️ Safety, Setup, once, Walton WalPad 8G — MT6765 Recovery & Low-Level Learning Project, What's next (+1 more)

### Community 117 - "m6_check.sh"
Cohesion: 0.44
Nodes (7): bad(), chk(), listed(), ok(), running(), m6_check.sh script, waitfor()

### Community 118 - "measure_firefox_mem.sh"
Cohesion: 0.42
Nodes (7): cleanup(), mem_kb(), run_cfg(), measure_firefox_mem.sh script, stop_ff(), zram_off(), zram_on()

### Community 120 - "codec.rs"
Cohesion: 0.32
Nodes (4): LineDecoder, LineError, TooLong, MAX_LINE

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

### Community 133 - "HotspotState"
Cohesion: 0.33
Nodes (6): HotspotState, Failed, Off, On, Starting, Stopping

### Community 134 - "Accessory"
Cohesion: 0.33
Nodes (6): Accessory, Action, Chevron, None, Toggle, Value

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

### Community 142 - "WifiRadio"
Cohesion: 0.40
Nodes (5): WifiRadio, Down, Failed, Starting, Up

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

### Community 149 - "BatterySource"
Cohesion: 0.50
Nodes (4): BatterySource, Gauge, Unknown, VoltageEstimate

### Community 150 - "HotspotInternet"
Cohesion: 0.50
Nodes (4): HotspotInternet, NoUplink, Shared, Unavailable

### Community 151 - "m4_check.sh"
Cohesion: 0.83
Nodes (3): bad(), ok(), m4_check.sh script

### Community 152 - "docker_boot.sh"
Cohesion: 1.00
Nodes (3): mount_controller(), mountpoint(), docker_boot.sh script

## Knowledge Gaps
- **774 isolated node(s):** `apply.sh script`, `CONTRACT_ENV`, `BIN_DIRS`, `Clean`, `Closed` (+769 more)
  These have ≤1 connection - possible missing edges or undocumented components. (Counts symbols only; 1290 node(s) total have ≤1 connection when file, concept and rationale nodes are included.)
- **114 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `_` connect `_` to `x.rs`, `mousepad.rs`, `Shell`, `src/components.rs`, `dock.rs`, `Frame`, `settings_screen.rs`, `Model`, `device_screen.rs`, `panels.rs`, `text.rs`, `.new`, `network.rs`, `tests/components.rs`, `build`, `chrome.rs`, `art.rs`, `app.rs`, `Tint`, `super`?**
  _High betweenness centrality (0.055) - this node is a cross-community bridge._
- **What connects `apply.sh script`, `CONTRACT_ENV`, `BIN_DIRS` to the rest of the system?**
  _774 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `x.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05232323232323232 - nodes in this community are weakly interconnected._
- **Why does `Root` connect `Root` to `Shell`, `dock.rs`, `Daemon`, `src/apps.rs`, `settings_screen.rs`, `Wifi`, `.new`, `input.rs`, `App`, `Cmd`, `walpad-ctl/src/lib.rs`, `Inputs`, `daemon.rs`, `gsensor.rs`, `Hotspot`, `manager.rs`, `app.rs`, `src/hotspot.rs`, `config.rs`, `super`?**
  _High betweenness centrality (0.045) - this node is a cross-community bridge._
- **Should `app/tests.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.08521529659741042 - nodes in this community are weakly interconnected._
- **Why does `Daemon` connect `Daemon` to `x.rs`, `daemon.rs`, `live.rs`, `Root`, `Brain`, `.new`, `Inputs`?**
  _High betweenness centrality (0.041) - this node is a cross-community bridge._
- **Should `mousepad.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.0624048706240487 - nodes in this community are weakly interconnected._