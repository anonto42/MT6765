# 07 — Hardware, Devices & Daemons

In Linux, **"everything is a file."** Hardware devices show up as special files in `/dev` and status/control knobs in `/sys`.

---

## 1. Key Hardware Devices on WalPad 8G

| Hardware | Where It Lives | Notes |
| :--- | :--- | :--- |
| **Screen (Framebuffer)** | `/dev/fb0` | `800x1280` panel, 32-bit BGRA; `fbmirror` copies the `1280x1280` `Xvfb` screen onto it |
| **Backlight** | `/sys/class/leds/lcd-backlight/brightness` | Controls screen brightness |
| **Touchscreen** | `/dev/input/event*` (`mtk-tpd`) | Silead GSL touch controller (always matched by name, not event number) |
| **Buttons & Sensors** | `/dev/input/event*` | `mtk-kpd` (volume), `mtk-pmic-keys` (power), `HALL_DEV`, `kxd` (accelerometer) |
| **Audio** | ALSA card 0 (`mt-snd-card`) | Managed by `walpad-audio` (44.1 kHz, 25 ms/100 ms period/buffer) |
| **Wi-Fi** | `wlan0`, `ap0`, `p2p0` | MediaTek CONNSYS (`gen4m`); cannot run client (`wlan0`) + hotspot (`ap0`) at the exact same time |
| **Cellular Modem** | `/dev/radio/pttycmd1` (CCCI) | `walpad-modem` boots MD1 and talks AT commands |

---

## 2. How `walpad-core` & `walpad-shell` Work Together

- **`walpad-core`** (written in Rust) runs in the background as the central system daemon.
  - It reads hardware state (battery, brightness, Wi-Fi, storage, modem).
  - It launches, tracks, and closes apps from `/etc/walpad/apps.json`.
  - It listens for commands on a Unix socket: **`/run/walpad/control.sock`**.
- **`walpad-shell`** (written in Rust) is the touch UI you see on screen.
  - It talks to `walpad-core` over `/run/walpad/control.sock` to show status, launch apps, toggle Wi-Fi, or adjust brightness.
- **`walpad-ctl`** is a command-line tool that can also talk to `walpad-core` over that same socket.

---

## Try It on Your Tablet

```sh
cat /sys/class/leds/lcd-backlight/brightness   # Read current brightness
cat /proc/bus/input/devices                    # List touch, buttons, sensors
ls -la /run/walpad/                            # See control.sock
```

---

**Next:** [08 — Networking, Wi-Fi & SSH](08-networking-wifi-and-ssh.md)
