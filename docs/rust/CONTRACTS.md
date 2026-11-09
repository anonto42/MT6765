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
- openbox with /etc/walpad/openbox-rc.xml (undecorated, maximised; matchbox-keyboard above).
- Backlight /sys/class/leds/lcd-backlight/brightness 0-255.

## /run/walpad/
- `orientation`: ASCII 0|90|180|270 (device daemon -> fbmirror ~160 ms poll, touch bridge).
- `status.json` (every 2 s, atomic rename): {time, uptime_s, battery{percent, estimated, status, mv, temp_dC}, orientation, rotation_locked, screen: on/dim/off, brightness, lid_closed, low_battery, wifi}.
- `devd.cmd` FIFO 0666: screen on/off/toggle, rotate auto/lock/toggle/0/90/180/270, brightness N/+N/-N, keyboard, reload.
- `wifi-state` (walpad-wifi, 4 s): `<wpa_state> <ipv4>` or started/failed.
- `xfb/Xvfb_screen0`: XWD, mmapped by fbmirror.
- Wi-Fi: socket dir /run/wpa_supplicant, iface wlan0, /etc/wpa_supplicant/wpa_supplicant.conf (600, update_config=1).

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
DISPLAY=:0 HOME=/root XDG_RUNTIME_DIR=/run/walpad LIBGL_ALWAYS_SOFTWARE=1 MOZ_DISABLE_CONTENT_SANDBOX=1 MOZ_DISABLE_GMP_SANDBOX=1 MOZ_DISABLE_RDD_SANDBOX=1 MOZ_DISABLE_SOCKET_PROCESS_SANDBOX=1 MOZ_WEBRENDER=0 MOZ_ACCELERATED=0 GTK_A11Y=none NO_AT_BRIDGE=1

## Known hardware status
Working: display, power/volume keys, cover sensor. Touch works, calibration inaccurate. Auto-rotation built, direction unverified. Battery gauge broken (-1), voltage estimate only. Wi-Fi built, unverified after cold boot. Browser/terminal built, unverified. Audio unverified. Bluetooth, suspend, SD, USB OTG not working/not attempted. Camera unsupported.
