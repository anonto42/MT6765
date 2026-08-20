#!/usr/bin/env python3
"""
wifi_loop.py -- unattended bring-up: push, try, reboot, repeat.

A failed /dev/wmtWifi power-on leaves the chip latched in "chip reset and chip
no response" and usually takes both ACM endpoints with it, so every attempt
needs a fresh kernel.  Rebooting *immediately* after a failure -- while the
console still answers -- keeps that recovery in software.

Usage:  wifi_loop.py [attempts]        (default 3)
"""
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from wifi_now import connect, send, drain, push_file, push_binary, STACK, CONF, SHIM
import reboot_now


def stage(s):
    """Mount the SD card and refresh the three files we drive everything from."""
    send(s, "mkdir -p /mnt/sd; i=0; "
            "while [ ! -b /dev/mmcblk1p1 ] && [ $i -lt 40 ]; do sleep 1; i=$((i+1)); done; "
            "grep -q ' /mnt/sd ' /proc/mounts || mount /dev/mmcblk1p1 /mnt/sd; "
            "grep -q ' /mnt/sd ' /proc/mounts && echo SD_OK || echo SD_FAIL")
    if "SD_OK" not in drain(s, 50.0):
        return False
    push_file(s, STACK, "/mnt/sd/mtk_stack_up.sh")
    push_file(s, CONF, "/mnt/sd/wifi/wifi.conf", mode="600")
    push_binary(s, SHIM, "/mnt/sd/wifi/libwpdprops.so")
    return True


def attempt(s):
    """Run the stack script.  True once wlan0 exists."""
    send(s, "sh /mnt/sd/mtk_stack_up.sh 2>&1; echo STACK_DONE")
    print(drain(s, 120.0))
    # The console echoes the command back, so the marker must not be a literal
    # in the command itself -- otherwise it matches whatever the result was.
    send(s, "ls /sys/class/net | grep -qx wlan0; echo WLANCHK=$?")
    return "WLANCHK=0" in drain(s, 15.0, echo=False)


def main():
    attempts = int(sys.argv[1]) if len(sys.argv) > 1 else 3

    for n in range(1, attempts + 1):
        print("\n===== attempt %d/%d =====" % (n, attempts))
        s = connect(timeout=300)
        if s is None:
            print("!! no console -- needs a physical power-cycle")
            return 1

        if not stage(s):
            print("!! SD card never mounted")
            return 1

        if attempt(s):
            print("\n*** wlan0 is up ***")
            return 0

        if n == attempts:
            print("\n!! out of attempts; leaving the console alive for diagnosis")
            return 1

        # Reboot now, before the wedge sets in, so the next attempt is unattended.
        print("\n-- failed; rebooting for a clean chip --")
        reboot_now.reboot(s)
        reboot_now.wait_gone()
        time.sleep(2)

    return 1


if __name__ == "__main__":
    sys.exit(main())
