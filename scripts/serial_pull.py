#!/usr/bin/env python3
"""
serial_pull.py — copy files off the tablet over the serial console.

Waits for a WalPad-Server window, then base64s each file across.  Small binaries
only: at 115200 baud this moves roughly 10 KB/s.

    serial_pull.py /vendor/bin/wmt_launcher /vendor/bin/wmt_loader
"""
import os
import sys
import time
import base64
import serial
import subprocess

PORT = "/dev/ttyACM0"
BAUD = 115200
OUTDIR = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                      "data", "vendor-bins")
B0 = "__WPD_B64_START__"
B1 = "__WPD_B64_END__"


class Lost(Exception):
    pass


def drain(s, seconds, echo=False):
    out = b""
    t0 = time.time()
    while time.time() - t0 < seconds:
        try:
            n = s.in_waiting
        except OSError:
            raise Lost("port went away")
        if n:
            chunk = s.read(n)
            out += chunk
            if echo:
                sys.stdout.write(chunk.decode("utf-8", "replace"))
                sys.stdout.flush()
        else:
            time.sleep(0.02)
    return out.decode("utf-8", "replace")


def server_window():
    try:
        return "0e8d:2008" in subprocess.run(
            ["lsusb"], capture_output=True, text=True, timeout=2).stdout
    except Exception:
        return False


def connect(timeout=3600):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if server_window() and os.path.exists(PORT):
            s = None
            try:
                s = serial.Serial(PORT, BAUD, timeout=0.2)
                s.reset_input_buffer()
                for _ in range(10):
                    s.write(b"\necho WPD_PING_OK\n")
                    s.flush()
                    if "WPD_PING_OK" in drain(s, 0.6):
                        return s
            except Exception:
                # a node that exists but answers EIO is a leftover from the
                # previous enumeration; drop it and wait for the real one
                pass
            if s is not None:
                try:
                    s.close()
                except Exception:
                    pass
        time.sleep(0.05)
    return None


def carve(text):
    """Pull the payload out, skipping the shell's echo of the command itself."""
    head = text.split(B1, 1)[0]
    if B0 not in head:
        raise ValueError("no start sentinel in reply")
    body = head.rsplit(B0, 1)[1]
    return "".join(body.split())


def pull(s, remote, local):
    size_txt = drain(s, 0.1)
    s.write(("echo %s; base64 %s; echo %s\n" % (B0, remote, B1)).encode())
    s.flush()
    text = ""
    t0 = time.time()
    while B1 not in text and time.time() - t0 < 600:
        text += drain(s, 1.0)
    blob = base64.b64decode(carve(text))
    with open(local, "wb") as f:
        f.write(blob)
    print("  %s -> %s (%d bytes)" % (remote, local, len(blob)))


def main():
    targets = sys.argv[1:]
    if not targets:
        print(__doc__)
        return 2
    os.makedirs(OUTDIR, exist_ok=True)
    print("waiting for a WalPad-Server window ...")
    s = connect()
    if s is None:
        print("timed out")
        return 1
    print("[%s] shell answering" % time.strftime("%H:%M:%S"))
    # refresh the on-device script, then bring /vendor back: the loop mount does
    # not survive a reboot
    here = os.path.dirname(os.path.abspath(__file__))
    s.write(b"mkdir -p /mnt/sd; grep -q ' /mnt/sd ' /proc/mounts || "
            b"mount /dev/mmcblk1p1 /mnt/sd\n")
    s.write(b"cat > /mnt/sd/mtk_stack_up.sh <<'__WPD_EOF__'\n")
    with open(os.path.join(here, "mtk_stack_up.sh")) as f:
        for line in f:
            s.write(line.rstrip("\n").encode() + b"\n")
            s.flush()
            time.sleep(0.004)
    s.write(b"__WPD_EOF__\n")
    s.flush()
    drain(s, 2.0)
    s.write(b"MTK_NO_POWERON=1 sh /mnt/sd/mtk_stack_up.sh 2>&1 | tail -20\n")
    s.flush()
    drain(s, 45.0, echo=True)
    for remote in targets:
        pull(s, remote, os.path.join(OUTDIR, os.path.basename(remote)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
