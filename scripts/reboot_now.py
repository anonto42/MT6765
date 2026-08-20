#!/usr/bin/env python3
"""
reboot_now.py -- reboot the tablet from its own console, no hands needed.

The WiFi bring-up wedges both CDC-ACM endpoints whenever a power-on attempt
fails, and once wedged the only recovery is physical.  So the trick is to never
get there: reboot from the shell *while it still answers*, which costs nothing
and returns a clean kernel a few seconds later.

Exit codes:  0 rebooted and back,  1 no console to reboot from,  2 never came back.
"""
import sys
import time

sys.path.insert(0, __file__.rsplit("/", 1)[0])
from wifi_now import connect, send, server_window


def reboot(s):
    """Ask for an immediate reset.

    Plain `reboot` is a busybox applet that signals init; this firmware's init
    may not answer, so fall through to sysrq, which resets from inside the
    kernel and cannot be refused.  Neither command replies -- the console dies
    mid-write -- so nothing here waits for output.
    """
    send(s, "sync")
    time.sleep(0.5)
    send(s, "reboot -f 2>/dev/null || "
            "{ echo 1 > /proc/sys/kernel/sysrq; echo b > /proc/sysrq-trigger; }")
    time.sleep(0.5)
    try:
        s.close()
    except Exception:
        pass


def wait_gone(timeout=30):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if not server_window():
            return True
        time.sleep(0.2)
    return False


def main():
    print("connecting ...")
    s = connect(timeout=60)
    if s is None:
        print("!! no console -- the shell is already wedged, "
              "this one needs a physical power-cycle")
        return 1

    print("rebooting")
    reboot(s)

    if wait_gone():
        print("device dropped off the bus")
    else:
        # A warm reset can be quick enough that the window never looks closed.
        print("never saw it drop (fast reset?), waiting for the shell anyway")

    s = connect(timeout=180)
    if s is None:
        print("!! did not come back")
        return 2
    print("back up")
    return 0


if __name__ == "__main__":
    sys.exit(main())
