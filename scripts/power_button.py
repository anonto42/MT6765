#!/usr/bin/env python3
"""Use the WalPad PMIC power key for LCD toggle and clean shutdown."""

import argparse
import os
import struct
import subprocess
import time

EVENT = struct.Struct("llHHI")
EV_KEY = 1
KEY_POWER = 116
SCREEN_OFF = "/run/walpad-screen-off"
BLANK = "/sys/class/graphics/fb0/blank"
BRIGHTNESS = "/sys/class/leds/lcd-backlight/brightness"


def write(path, value):
    with open(path, "w", encoding="ascii") as handle:
        handle.write(str(value))


def screen_off():
    open(SCREEN_OFF, "a", encoding="ascii").close()
    write(BLANK, 1)
    write(BRIGHTNESS, 0)


def screen_on(brightness):
    try:
        os.unlink(SCREEN_OFF)
    except FileNotFoundError:
        pass
    write(BRIGHTNESS, brightness)
    write(BLANK, 0)


def toggle(brightness):
    if os.path.exists(SCREEN_OFF):
        screen_on(brightness)
    else:
        screen_off()


def shutdown(dry_run):
    screen_off()
    if dry_run:
        print("long press: shutdown requested", flush=True)
        return
    subprocess.run(["sync"], check=False)
    subprocess.run(["/sbin/poweroff", "-f"], check=False)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--device", default="/dev/input/event2")
    parser.add_argument("--long-press", type=float, default=3.0)
    parser.add_argument("--brightness", type=int, default=80)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    pressed_at = None
    with open(args.device, "rb", buffering=0) as device:
        while True:
            data = device.read(EVENT.size)
            if len(data) != EVENT.size:
                time.sleep(0.1)
                continue
            _, _, event_type, code, value = EVENT.unpack(data)
            if event_type != EV_KEY or code != KEY_POWER:
                continue
            if value == 1:
                pressed_at = time.monotonic()
            elif value == 0 and pressed_at is not None:
                duration = time.monotonic() - pressed_at
                pressed_at = None
                if duration >= args.long_press:
                    shutdown(args.dry_run)
                else:
                    toggle(args.brightness)


if __name__ == "__main__":
    main()
