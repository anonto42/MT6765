#!/usr/bin/env python3
"""
wifi_now.py — sit on /dev/ttyACM0 and fire the whole WiFi bring-up the instant
the WalPad-Server window opens.

The tablet only presents its root shell during a 2008 window, so this polls for
the port at ~50 Hz and starts pushing within milliseconds of it appearing.
Everything it sends is idempotent, so re-running it is safe.
"""
import glob
import os
import sys
import time
import serial
import subprocess


class Lost(Exception):
    """The console vanished mid-transfer (window closed, or re-enumeration)."""

PORT = "/dev/ttyACM0"
BAUD = 115200
HERE = os.path.dirname(os.path.abspath(__file__))
STACK = os.path.join(HERE, "mtk_stack_up.sh")
CONF = os.path.join(os.path.dirname(HERE), "data", "wifi", "wifi.conf")
SHIM = os.path.join(os.path.dirname(HERE), "data", "wifi", "libwpdprops.so")


def server_window():
    """The shell only exists inside a 0e8d:2008 window."""
    try:
        out = subprocess.run(["lsusb"], capture_output=True, text=True,
                             timeout=2).stdout
    except Exception:
        return False
    return "0e8d:2008" in out


def connect(timeout=1800):
    """Wait for a window, then open the port and prove the shell is sane.

    Opening the node the instant it appears catches the device mid-enumeration:
    the first attempt after a re-enumeration comes back with every byte shifted
    by one, and the fd can die with EIO a moment later.  So handshake first and
    only return a port that echoes cleanly.
    """
    t0 = time.time()
    while time.time() - t0 < timeout:
        if not server_window():
            time.sleep(0.02)
            continue
        # The node index climbs with every re-enumeration (ttyACM0 becomes
        # ttyACM1, then ttyACM2...), and only one of the pair is a shell, so
        # probe whatever is present rather than trusting a fixed name.
        for port in sorted(glob.glob("/dev/ttyACM*")):
            try:
                s = serial.Serial(port, BAUD, timeout=0.2)
            except Exception:
                # a node that exists but answers EIO is a leftover from the
                # previous enumeration; wait for the real one
                continue
            try:
                s.reset_input_buffer()
                for _ in range(6):
                    s.write(b"\necho WPD_PING_OK\n")
                    s.flush()
                    if "WPD_PING_OK" in drain(s, 0.6, echo=False):
                        print("[console on %s]" % port)
                        return s
            except Exception:
                pass
            try:
                s.close()
            except Exception:
                pass
        time.sleep(0.1)
    return None


def send(s, line):
    s.write(line.encode() + b"\n")
    s.flush()


def drain(s, seconds, echo=True):
    out = b""
    t0 = time.time()
    while time.time() - t0 < seconds:
        try:
            waiting = s.in_waiting
        except OSError:
            raise Lost("serial port went away")
        if waiting:
            chunk = s.read(waiting)
            out += chunk
            if echo:
                sys.stdout.write(chunk.decode("utf-8", "replace"))
                sys.stdout.flush()
        else:
            time.sleep(0.02)
    return out.decode("utf-8", "replace")


def push_file(s, local, remote, mode="755"):
    """Write a file over the console with a quoted heredoc (no expansion)."""
    send(s, "mkdir -p %s" % os.path.dirname(remote))
    send(s, "cat > %s <<'__WPD_EOF__'" % remote)
    with open(local) as f:
        for line in f:
            s.write(line.rstrip("\n").encode() + b"\n")
            s.flush()
            time.sleep(0.004)
    send(s, "__WPD_EOF__")
    send(s, "chmod %s %s" % (mode, remote))
    drain(s, 1.5, echo=False)


def run_once():
    print("waiting for a WalPad-Server window ...")
    s = connect()
    if s is None:
        print("timed out")
        return 1
    print("[%s] shell answering, going" % time.strftime("%H:%M:%S"))

    # The SD controller enumerates several seconds into boot, well after the
    # console answers.  Mounting too early silently fails and every push below
    # lands in the ramdisk, only to be shadowed when the real card is mounted.
    send(s, "mkdir -p /mnt/sd; i=0; while [ ! -b /dev/mmcblk1p1 ] && [ $i -lt 40 ]; "
            "do sleep 1; i=$((i+1)); done; "
            "grep -q ' /mnt/sd ' /proc/mounts || mount /dev/mmcblk1p1 /mnt/sd; "
            "grep -q ' /mnt/sd ' /proc/mounts && echo SD_OK || echo SD_FAIL")
    out = drain(s, 50.0)
    if "SD_OK" not in out:
        print("!! SD card never mounted; not pushing into the ramdisk")
        return 1

    push_file(s, STACK, "/mnt/sd/mtk_stack_up.sh")
    push_file(s, CONF, "/mnt/sd/wifi/wifi.conf", mode="600")
    push_binary(s, SHIM, "/mnt/sd/wifi/libwpdprops.so")

    send(s, "sh /mnt/sd/mtk_stack_up.sh 2>&1")
    drain(s, 60.0)

    send(s, "echo '== netdevs'; ls /sys/class/net | tr '\\n' ' '; echo")
    drain(s, 5.0)
    return 0


def push_binary(s, local, remote, mode="644"):
    """Base64 a small binary across the console and decode it on the tablet."""
    import base64
    blob = base64.b64encode(open(local, "rb").read()).decode()
    send(s, "mkdir -p %s; : > %s.b64" % (os.path.dirname(remote), remote))
    # the tty is in canonical mode, so a line has to stay under ~4 KB
    for i in range(0, len(blob), 512):
        send(s, "echo -n %s >> %s.b64" % (blob[i:i + 512], remote))
        time.sleep(0.004)
    send(s, "base64 -d %s.b64 > %s && rm -f %s.b64 && chmod %s %s && "
            "echo PUSHED $(wc -c < %s)" % (remote, remote, remote, mode, remote, remote))
    print(drain(s, 8.0, echo=False).strip().splitlines()[-1] if True else "")


def main():
    while True:
        try:
            return run_once()
        except Lost as e:
            print("\n[!] %s — waiting for the next window" % e)
            time.sleep(1.0)


if __name__ == "__main__":
    sys.exit(main())
