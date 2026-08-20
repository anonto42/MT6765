#!/usr/bin/env python3
"""
serial_probe.py — shared engine for running command batches on the tablet.

The WalPad-Server boot window is short and unreliable, so every tool that
talks to the console needs the same three things:

  * wait for the 0e8d:2008 window to appear
  * frame each command with a sentinel, so output is read until the command
    actually finishes instead of guessing a --read-time
  * survive the window closing mid-batch, without losing what was collected

This module provides that once. Import it; don't reimplement it.
"""
import os
import sys
import time
import glob
import subprocess

try:
    import serial
except ImportError:
    print("pyserial missing - use the repo venv: tools/mtk/.venv/bin/python")
    raise

PORT = "/dev/ttyACM0"
BAUD = 115200
# The marker must never appear literally in the command that requests it.
# `stty -echo` does not always stick on this gadget, and when it does not, the
# console echoes the command straight back -- so a plain `echo __WPD_EOC_1__`
# is satisfied by its own echo and every command "succeeds" instantly.  printf
# joins the halves at run time: the echo shows two separate arguments, only the
# real output ever contains the joined token.
MARK = "__WPD_EOC_%s__"
MARK_CMD = "printf '%%s%%s\\n' '__WPD' '_EOC_%s__'"


def device_present():
    """True while the WalPad-Server (0e8d:2008) gadget is enumerated."""
    try:
        out = subprocess.run(["lsusb"], capture_output=True, text=True, timeout=2).stdout
        return "0e8d:2008" in out
    except Exception:
        return False


def wait_for_window(timeout=180, quiet=False):
    if not quiet:
        print("waiting for WalPad-Server window (up to %ds)..." % timeout)
    t0 = time.time()
    while time.time() - t0 < timeout:
        if device_present():
            if not quiet:
                print("[%s] window open" % time.strftime("%H:%M:%S"))
            return True
        time.sleep(0.2)
    if not quiet:
        print("timed out waiting for the 2008 window")
    return False


def strip_noise(text):
    """Drop the busybox prompt and the cursor-position escapes it emits."""
    text = text.replace("\x1b[6n", "").replace("\r\n", "\n")
    lines = [ln.rstrip() for ln in text.split("\n")]
    lines = [ln for ln in lines if ln.strip() not in ("", "/ #", "~ #", "#", ">")]
    return "\n".join(lines)


class WindowClosed(Exception):
    """Raised when the tablet drops off the bus mid-batch."""


class Console:
    """A framed command session on the tablet's serial console."""

    def __init__(self, port=PORT, baud=BAUD):
        self.port = port
        self.baud = baud
        self.s = None
        self._n = 0

    def open(self, wait=True, timeout=180):
        if wait and not device_present() and not wait_for_window(timeout):
            return False
        # The gadget exposes two CDC-ACM endpoints and only one of them is a
        # shell, and the node index climbs with every re-enumeration -- ttyACM0
        # becomes ttyACM1, then ttyACM2.  So probe whatever is present instead
        # of trusting a fixed name, and accept only a port that answers.
        #
        # write_timeout is what makes this safe: after a failed WiFi power-on
        # the tablet stops draining its endpoint, and a write with no timeout
        # blocks forever inside the kernel with no way out.
        # Keep sweeping.  The 2008 window opening and the ttyACM nodes becoming
        # usable are seconds apart: the gadget enumerates, then the ACM nodes
        # appear, then the far-side shell actually starts reading them.  A single
        # pass at window-open time reliably finds nothing.
        deadline = time.time() + max(30, timeout)
        while time.time() < deadline:
            try:
                if self._sweep():
                    return True
            except PermissionError:
                return False
        print("no answering console on %s after %ds"
              % (", ".join(self._candidates()) or "no /dev/ttyACM*", max(30, timeout)))
        return False

    def _sweep(self):
        """One pass over every ACM node; True if one of them answered.

        This has to be FAST -- a pass used to take ~7s, most of it sleeps and a
        3s listen per port, which is far too coarse to tell "not up yet" from
        "not coming up" and too slow to catch the console early in a boot.  A
        pass is now ~0.75s: nothing sleeps, the write timeout is short, and the
        listen is a fraction of a second.  The loop just runs again.  All the
        setup work (Ctrl-C, stty -echo) happens *after* a port answers, where it
        costs nothing.
        """
        for port in self._candidates():
            try:
                s = serial.Serial(port, self.baud, timeout=0.1, write_timeout=0.4)
            except Exception as e:
                if "Permission" in str(e):
                    print("open %s failed: %s" % (port, e))
                    print("hint: run `make udev` once, then replug - avoids needing sudo")
                    raise PermissionError(port)
                continue
            answered = False
            try:
                s.reset_input_buffer()
                s.write(b"\nprintf '%s%s\\n' __WPD _HELLO__\n")
                s.flush()
                answered = self._hears(s, "__WPD_HELLO__", timeout=0.4)
            except Exception:
                answered = False
            if answered:
                self._settle(s)
                self.port = port
                self.s = s
                return True
            self._discard(s)
        return False

    @staticmethod
    def _discard(s):
        """Close a port that did not answer, without blocking on it.

        A wedged endpoint has undelivered bytes sitting in the kernel's TX
        buffer, and close() waits for those to drain -- forever, since nothing
        on the far side is reading.  Drop them first.
        """
        for step in (s.cancel_write, s.reset_output_buffer, s.reset_input_buffer,
                     s.close):
            try:
                step()
            except Exception:
                pass

    def _settle(self, s):
        """Quiet a port that has just answered, before any real command runs."""
        try:
            # Ctrl-C first: a previous run that died mid-chunk can leave the
            # shell at a `> ` continuation prompt, where every later command is
            # swallowed as the tail of an unterminated quoted string.
            s.write(b"\x03\n")
            s.flush()
            time.sleep(0.2)
            s.reset_input_buffer()
            # echo off, so output isn't doubled back at us
            s.write(b"stty -echo 2>/dev/null\n")
            s.flush()
            time.sleep(0.3)
            s.reset_input_buffer()
        except Exception:
            pass
        # restore the working timeouts the command loop expects
        s.timeout = 1
        s.write_timeout = 5

    def _candidates(self):
        found = sorted(glob.glob("/dev/ttyACM*"))
        # an explicit port stays first in line, but is not the only thing tried
        return [self.port] + [p for p in found if p != self.port]

    @staticmethod
    def _hears(s, token, timeout=3.0):
        buf = b""
        t0 = time.time()
        while time.time() - t0 < timeout:
            try:
                buf += s.read(max(1, s.in_waiting))
            except Exception:
                return False
            if token.encode() in buf:
                return True
            time.sleep(0.05)
        return False

    def run(self, cmd, timeout=10.0):
        """Run one command. Returns (output, completed). Raises WindowClosed."""
        self._n += 1
        marker = MARK % self._n
        try:
            self.s.write(("%s\n%s\n" % (cmd, MARK_CMD % self._n)).encode())
            self.s.flush()
        except Exception:
            raise WindowClosed("write failed - device likely dropped off the bus")

        buf = b""
        mb = marker.encode()
        t0 = time.time()
        while time.time() - t0 < timeout:
            try:
                waiting = self.s.in_waiting
            except Exception:
                raise WindowClosed("read failed - device likely dropped off the bus")
            if waiting:
                buf += self.s.read(waiting)
                if mb in buf:
                    return strip_noise(buf.split(mb)[0].decode(errors="replace")), True
            else:
                time.sleep(0.02)
        return strip_noise(buf.decode(errors="replace")), False

    def close(self):
        if self.s:
            try:
                self.s.close()
            except Exception:
                pass
            self.s = None

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.close()
        return False


def repo_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
