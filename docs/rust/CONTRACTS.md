# Frozen contracts (do not change without an explicit card)

## Boot and process model
- `config/boot/init-os` is PID 1: pets /dev/watchdog every 10 s, USB gadget, telnetd bound to 192.168.42.1/192.168.43.1 only, mounts mmcblk0p44 at /newroot, runs `/sbin/walpad-init` in a chroot. /tmp and /run are tmpfs.
- Health: no `/run/walpad-ok` 150 s after chroot start -> ramdisk kills Xvfb fbmirror openbox python3 st xterm firefox-esr and starts padui on /dev/fb0. The Rust shell must create `/run/walpad-ok` after its first frame. Adding walpad-shell/walpad-core to the kill list needs an approved boot image change.
- /system, /vendor, /product are read-only from super (mmcblk0p40) via dmlinear; Wi-Fi needs them.

## Display
- Panel /dev/fb0 800x1280, 32 bpp B,G,R,A, 3 pages; present by writing `0,1280` / `0,0` to /sys/class/graphics/fb0/pan. Wake panel once per boot (blank 1 then 0); repeated blanking resets touch.
- Xvfb :0 -screen 0 1280x1280x24 -fbdir /run/walpad/xfb -nolisten tcp -ac. No MIT-SHM (no SYSVIPC).
- Visible: 0/180 deg x 0-799,y 0-1279; 90/270 deg x 0-1279,y 0-799.
- Struts (window `walpad-strut`): portrait right 480 -> `[0,480,0,0, 0,0,0,1279, 0,0,0,0]`; landscape bottom 480 -> `[0,0,0,480, 0,0,0,0, 0,0,0,1279]`.
- openbox with /etc/walpad/openbox-rc.xml (undecorated, maximised; matchbox-keyboard above: Python runtime only, the Rust runtime uses the shell's system keyboard).
- Backlight /sys/class/leds/lcd-backlight/brightness 0-255.

## /run/walpad/
- `orientation`: ASCII 0|90|180|270 (device daemon -> fbmirror ~160 ms poll, touch bridge).
- `status.json` (every 2 s, atomic rename): {time, uptime_s, battery{percent, estimated, status, mv, temp_dC}, orientation, rotation_locked, screen: on/dim/off, brightness, lid_closed, low_battery, wifi}.
- `devd.cmd` FIFO 0666: screen on/off/toggle, rotate auto/lock/toggle/0/90/180/270, brightness N/+N/-N, keyboard, reload.
- `wifi-state` (walpad-wifi, 4 s): `<wpa_state> <ipv4>` or started/failed; `disabled` (Wi-Fi switch off) or `hotspot` (client paused for an exclusive hotspot) (owner request 2026-11-10).
- `hotspot-state` (walpad-hotspot): `key=value` lines state (off|starting|on|failed), reason, iface, channel, internet (shared|no_uplink|unavailable), exclusive (0|1). `hotspot-exclusive`: present while the Wi-Fi client is held off for the hotspot (walpad-wifi does not run the supplicant). `hostapd.conf` / `udhcpd.conf` / `*.pid`: walpad-hotspot's, mode 0600.
- `xfb/Xvfb_screen0`: XWD, mmapped by fbmirror.
- Wi-Fi: socket dir /run/wpa_supplicant, iface wlan0, /etc/wpa_supplicant/wpa_supplicant.conf (600, update_config=1).

## Control socket `input` topic (core -> shell)
- Events `{"kind": ...}`: `power_menu_open`, `volume_up`, `volume_down`, `power_short`; `keyboard_show`, `keyboard_hide` (owner request 2026-11-10: the shell's system keyboard replaces matchbox-keyboard).
- `keyboard {mode: show|hide|toggle}` publishes `keyboard_show|keyboard_hide` (toggle is resolved against the state the shell last reported with `keyboard show|hide`) and answers `{"keyboard": "shown"|"hidden"|"unavailable"}` (unavailable = no client subscribed to `input`). The core never spawns matchbox-keyboard and no longer opens a keyboard on `launch terminal`; the shell shows it when a text input gets the focus (XIM, below), and on the `apps` event `started` for app `terminal` only as a fallback (3 s without any XIM connect/focus, or at once when its XIM server is not running).
- System keyboard windows (shell): `walpad-keyboard` (override-redirect, full width, bottom 30 % of the visible area: 384 portrait, 240 landscape; WM_HINTS input=false; keys injected with XTEST) + managed 1x1 `walpad-keyboard-strut` (`_NET_WM_WINDOW_TYPE_DOCK`, bottom strut incl. the root below the panel: 384 / 720). The system dock sits directly above it while both are up.
- Input method (owner request 2026-11-10, the dynamic keyboard): the shell runs an XIM server `@server=walpad` (selection owner + `XIM_SERVERS` on the root; transport version 0.0, `_XIM_MOREDATA` reassembled) on its own X connection. It offers only `XIMPreeditNothing|XIMStatusNothing` (and the None variants), sets every IC's forward/synchronous event masks to 0 and never consumes an event, so apps keep their key handling. SetICFocus shows the keyboard 150 ms later if the focus stayed; UnsetICFocus, DestroyIC, Close/Disconnect or the destruction of the client's XIM window hide it 250 ms after the last focused IC went away. The minimise key hides it until the next SetICFocus; the dock button, the core's `keyboard` command and Home stay manual. Apps get `XMODIFIERS=@im=walpad GTK_IM_MODULE=xim QT_IM_MODULE=xim`; walpad-session builds GTK's `immodules.cache` when missing (apk ran with --no-scripts).
- System dock windows (shell, collapsible since owner request 2026-11-10): `walpad-dock-handle` (override-redirect, 56x28, SHAPE-rounded top corners, WM_HINTS input=false) mapped while a non-shell window is `_NET_ACTIVE_WINDOW`, at the right end (8 px margin) of the bottom edge of the visible area or directly above the keyboard / mouse pad, with no strut. A tap on it maps `walpad-dock` (override-redirect strip, full width, 56 high) with the handle on top of it and maps the managed 1x1 `walpad-dock-strut` (bottom strut over strip + keyboard/pad: 56 / 440 portrait, 536 / 776 landscape); an action, 5 s without a tap or a second tap on the handle unmaps strip and strut again.

## Wi-Fi switch and hotspot (owner request 2026-11-10)
- The chip is never powered off/on after boot (a failed power-on latches until reboot): Wi-Fi off is network level only. walpad-wifi always brings the stack up at boot; its loop then honours the switch.
- `/var/lib/walpad/wifi-enabled` (persistent): `0` = client off, anything else or missing = on. Off: the loop terminates wpa_supplicant, stops DHCP, flushes and downs wlan0 (wlan0 stays up while an exclusive hotspot runs); on: it restarts the supplicant, which auto-connects to saved networks. Checked every second.
- `wifi_enable {on: bool}` writes the file (atomic) and, for off, sends DISCONNECT at once; answers `{enabled, held_by_hotspot}`. `status.wifi.enabled` reflects the file; while off or paused `wifi.ssid/ip/signal_dbm` are null.
- `/etc/walpad/hotspot.conf` (0600, owned by walpad-core): `ssid2=<hex>` and `wpa_passphrase=<8-63 printable ASCII>`. `hotspot_config {ssid?: string (1-32 bytes, no control chars), secret?: string}` (at least one; a missing name defaults to `WalPad`; the secret is never logged, never in argv, never answered) answers `{ssid, has_secret, restarting}`; a running hotspot restarts with the new settings.
- `hotspot {on: bool}` runs `/usr/local/sbin/walpad-hotspot start|stop` (fixed path, empty environment + PATH, own process group; timeouts start 45 s, stop 20 s, status 8 s; a timed-out start is killed and followed by a cleanup stop); `on` without a stored password is `invalid_args`; `busy` while a start/stop runs. Answers `{state}` at once and finishes with a `wifi` event `{kind: "hotspot", state, reason, ssid, has_secret, clients, internet, exclusive}`. While on, `status` runs every 10 s (client count). At core start one `status` picks up a running hotspot.
- `status.hotspot`: `{state: off|starting|on|stopping|failed, reason, ssid, has_secret, clients, internet: shared|no_uplink|unavailable|null, exclusive}`.
- walpad-hotspot: hostapd (nl80211, WPA2-PSK CCMP) on `ap0` (created with `iw dev wlan0 interface add ap0 type __ap` if missing), 192.168.50.1/24, busybox udhcpd .10-.60, NAT with legacy iptables (chain `WALPAD_HS`, MASQUERADE to the default-route interface, ip_forward 1 only while on). STA+AP only when `iw list` has a combination with managed and AP together (then the AP uses the client's channel); otherwise the client is paused (`hotspot-exclusive`) and resumes on stop/failure. No uplink or no iptables = hotspot without internet (`internet` says which).

## Mobile network / SIM (owner request 2026-11-10)
- AT channel `/dev/radio/pttycmd1` (gsm0710muxd pty, one opener at a time): walpad-core opens it per job (O_NONBLOCK, raw termios) from one worker thread, every command <= 5 s; no AT job while a `walpad-modem` child runs. Unknown URCs (`+EIND: 64`) are ignored.
- Core start: no `/usr/local/sbin/walpad-modem` = `modem: absent`. Channel present = ready. Otherwise, once per boot (`/run/walpad/modem-start-attempted`), `walpad-modem start` (120 s, empty env + PATH, own process group); afterwards the channel is looked for every 10 s.
- Ready: `ATE0`, `AT+CMEE=1`, `ATI` (firmware), `AT+CGSN` (IMEI, memory only), one `walpad-modem data status` (10 s). Poll (10 s radio on, 30 s off, at once on a SIM change): `AT+CFUN?`, `AT+CPIN?`, `AT+CCID` (once per SIM), and with radio on + SIM ready `AT+CSQ`, `AT+CREG?`, `AT+CGREG?`, `AT+CEREG?`, `AT+COPS=3,0`, `AT+COPS?`.
- `/var/lib/walpad/mobile-radio` (persistent, 0600): `0` = radio off (airplane), else on; applied with AT+CFUN=1/0 after every boot. `/etc/walpad/mobile.conf` (0600, owned by walpad-core): `apn=<1-62 of A-Za-z0-9.-_>`.
- `status.mobile`: `{modem: absent|present|booting|ready|failed, reason, radio_on: bool|null, sim: ready|pin_required|puk_required|absent|busy|unknown, operator, tech: "2G"|"3G"|"4G"|null, signal_bars: 0-4|null, signal_dbm (-113 + 2*CSQ), registration: home|roaming|searching|denied|not_registered|unknown, data: off|connecting|on|failed, data_reason, ip, apn (configured; null = operator default), apn_in_use, firmware}`. Never IMEI/ICCID/PIN.
- Commands: `mobile_radio {on}` -> `{on, applied}` (refused without modem support); `mobile_data {on}` -> `{data}` (refused unless ready, radio on and SIM ready for on; busy while switching; runs `/usr/local/sbin/walpad-modem data on|off`, 40 s, only the boolean from the client); `mobile_apn {apn}` -> `{apn, reconnect}`; `sim_pin {pin: 4-8 digits}` -> `{accepted}` (refused unless the SIM asks for a PIN; the PIN goes only to the AT channel as `AT+CPIN="<pin>"`, never logged or in argv); `mobile_info` -> `{modem, firmware, imei, iccid}` (identifiers on request only).
- Results arrive on the `notice` topic: `{kind: "mobile_pin", ok, error}` ("Wrong PIN" for CME 16), `{kind: "mobile_radio", on, ok, error}`, `{kind: "mobile_data", state, reason, ip}`.
- `walpad-modem data on|off|status` prints `state=on|off|failed`, `reason`, `ip`, `apn` (kept in /run/walpad/mobile-data). on: APN from mobile.conf or by operator (Grameenphone gpinternet, Robi/Airtel internet, Banglalink blweb, Teletalk wap, else internet), `AT+EDALLOW=1`, `AT+CGDCONT=1,"IP","<apn>"`, `AT+CGACT=1,1`, `AT+CGPADDR=1`, `AT+CGCONTRDP=1`, then `/usr/local/sbin/walpad-modem-link up <ip> <dns1> <dns2>`; without that hook: `state=failed reason=data link not supported yet` (context left active). off: hook `down`, `AT+CGACT=0,1`.

## Input (discover by /sys/class/input/input*/name, never by event number; mknod missing nodes; no EVIOCGRAB)
- HALL_DEV: EV_SW 1 = closed. mtk-kpd: KEY_VOLUMEDOWN 114, KEY_VOLUMEUP 115. mtk-pmic-keys: KEY_POWER 116. kxd-kxd: unused.
- mtk-tpd (also gsl3680, touchscreen): ABS_MT_POSITION_X 53 / ABS_X 0, _Y 54 / 1, BTN_TOUCH 0x14A, protocol A, no slots.
- input_event aarch64 = 24 bytes: tv_sec i64, tv_usec i64, type u16, code u16, value i32.
- Accelerometer: /dev/gsensor, ioctl 0x80048503, 128-byte buffer, NUL-terminated ASCII "x y z" (hex, signed 32-bit).
- Battery: /sys/class/power_supply/battery/{capacity,status,voltage_now,temp}; capacity reads -1; voltage_now in uV (/1000 when >100000); temp in tenths of C.

## Config files (keep formats, add keys only)
- /etc/walpad/devd.json: auto_rotate, rotate_flip, tilt_min 3000, tilt_ratio 1.35, rotate_samples 3, dim_after_s 90, dim_level 20, off_after_s 240, lid_off, low_batt_mv 3550, critical_batt_mv 3350, default_brightness 120.
- /etc/walpad/keys.json: tap_window_ms 350, long_ms 600, chord_ms 160, repeat_ms 140; per button single/double/triple/quad/long/hold_repeat, both.press. Actions: key:<keysym>, ctl:<cmd>, home, keyboard, screenshot, volume:+-N, powermenu, none. New `launch:<app-id>`; `run:` accepted only while Python is the runtime.
- /etc/walpad/touch.json: swap, flipx, flipy, xmin, xmax, ymin, ymax.
- New /etc/walpad/runtime.conf: WALPAD_DEVICE|INPUT|SHELL=python|rust, WALPAD_SHADOW=0|1.
- New /etc/walpad/apps.json: [{id, name, icon, exec[], env{}, single_instance, wm_class}].

## App environment (every launched app)
DISPLAY=:0 HOME=/root XDG_RUNTIME_DIR=/run/walpad LIBGL_ALWAYS_SOFTWARE=1 MOZ_DISABLE_CONTENT_SANDBOX=1 MOZ_DISABLE_GMP_SANDBOX=1 MOZ_DISABLE_RDD_SANDBOX=1 MOZ_DISABLE_SOCKET_PROCESS_SANDBOX=1 MOZ_WEBRENDER=0 MOZ_ACCELERATED=0 GTK_A11Y=none NO_AT_BRIDGE=1 XMODIFIERS=@im=walpad GTK_IM_MODULE=xim QT_IM_MODULE=xim (the last three: owner request 2026-11-10, the shell is the input method)

## Known hardware status
Working: display, power/volume keys, cover sensor. Touch works, calibration inaccurate. Auto-rotation built, direction unverified. Battery gauge broken (-1), voltage estimate only. Wi-Fi built, unverified after cold boot. Browser/terminal built, unverified. Audio unverified. Bluetooth, suspend, SD, USB OTG not working/not attempted. Camera unsupported.
