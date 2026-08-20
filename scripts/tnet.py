#!/usr/bin/env python3
"""
tnet.py — run a command batch on the tablet over telnet (WiFi), not serial.

Once wlan0 has an address this is strictly better than the CDC-ACM console:
it does not wedge, it does not re-enumerate, and it is orders of magnitude
faster.  The firmware's telnetd hands out an unauthenticated root busybox
shell, so there is nothing to log in to.

Same sentinel discipline as serial_probe: the marker is assembled by printf at
run time so the shell's own echo of the command can never satisfy it.

Usage:  tnet.py [host] 'command' ['command' ...]
"""
import socket
import sys
import time

HOST = "192.168.0.100"
PORT = 23
IAC, DONT, WONT = 255, 254, 252


class Tnet:
    def __init__(self, host=HOST, port=PORT, timeout=10.0):
        self.s = socket.create_connection((host, port), timeout=timeout)
        self.s.settimeout(0.5)
        self._n = 0
        self._greet()

    def _greet(self):
        """Refuse every telnet option, then wait for the shell prompt."""
        t0 = time.time()
        while time.time() - t0 < 5:
            try:
                data = self.s.recv(4096)
            except socket.timeout:
                break
            if not data:
                break
            reply = bytearray()
            i = 0
            while i < len(data):
                if data[i] == IAC and i + 2 < len(data):
                    # WILL/DO -> WONT/DONT: we negotiate nothing
                    reply += bytes([IAC, DONT if data[i + 1] in (251, 253) else WONT,
                                    data[i + 2]])
                    i += 3
                else:
                    i += 1
            if reply:
                self.s.sendall(bytes(reply))

    def run(self, cmd, timeout=60.0):
        self._n += 1
        mark = "__TNET_EOC_%d__" % self._n
        self.s.sendall(("%s\nprintf '%%s%%s\\n' '__TNET' '_EOC_%d__'\n"
                        % (cmd, self._n)).encode())
        buf = b""
        t0 = time.time()
        while time.time() - t0 < timeout:
            try:
                chunk = self.s.recv(65536)
            except socket.timeout:
                continue
            if not chunk:
                break
            buf += chunk
            if mark.encode() in buf:
                break
        text = buf.split(mark.encode())[0].decode(errors="replace")
        text = text.replace("\x1b[6n", "").replace("\r\n", "\n")
        # telnetd leaves echo on, so drop the command the shell echoed back and
        # the marker's own printf line -- neither is output.
        keep = []
        for ln in text.split("\n"):
            st = ln.strip()
            if st in ("", "/ #", "~ #", "#", ">"):
                continue
            if st.startswith("printf '%s%s") or st == cmd.strip():
                continue
            if st.startswith("/ # printf '%s%s"):
                continue
            keep.append(ln)
        return "\n".join(keep)

    def close(self):
        try:
            self.s.close()
        except Exception:
            pass

    def __enter__(self):
        return self

    def __exit__(self, *a):
        self.close()
        return False


def main():
    args = sys.argv[1:]
    host = HOST
    if args and ("." in args[0] and " " not in args[0]):
        host, args = args[0], args[1:]
    if not args:
        args = ["uname -a", "ip -4 addr show wlan0 | grep inet"]
    with Tnet(host) as t:
        for cmd in args:
            print("--- %s" % cmd)
            print(t.run(cmd))
    return 0


if __name__ == "__main__":
    sys.exit(main())
