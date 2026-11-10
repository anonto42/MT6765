"""Fake modem AT channel for the walpad-modem data tests (host only).

Creates a pseudo-terminal, links its slave to argv[1] (like /dev/radio/pttycmd1), answers AT
commands and appends every command to argv[2].  Env FAKE_AT_CGACT_FAIL=1: data activation fails.
Runs until killed.
"""
import os
import pty
import select
import sys
import tty

link, log = sys.argv[1], sys.argv[2]
master, slave = pty.openpty()
tty.setraw(slave)
if os.path.lexists(link):
    os.unlink(link)
os.symlink(os.ttyname(slave), link)

REPLIES = {
    "AT+COPS=3,0": "",
    "AT+COPS?": '+COPS: 0,0,"Grameenphone",7',
    "AT+EDALLOW=1": "",
    "AT+CGACT=0,1": "",
    "AT+CGPADDR=1": '+CGPADDR: 1,"10.20.30.40"',
    "AT+CGCONTRDP=1": '+CGCONTRDP: 1,5,"gpinternet","10.20.30.40.255.255.255.0","10.20.30.1","8.8.8.8","8.8.4.4"',
}


def answer(cmd):
    if cmd == "AT+CGACT=1,1" and os.environ.get("FAKE_AT_CGACT_FAIL") == "1":
        return "\r\n+CME ERROR: 148\r\n"
    if cmd.startswith("AT+CGDCONT=1,"):
        return "\r\nOK\r\n"
    if cmd == "AT+CGACT=1,1":
        return "\r\n+EIND: 64\r\nOK\r\n"
    if cmd in REPLIES:
        body = REPLIES[cmd]
        return ("\r\n%s\r\n\r\nOK\r\n" % body) if body else "\r\nOK\r\n"
    return "\r\nERROR\r\n"


buf = b""
while True:
    r, _, _ = select.select([master], [], [], 1.0)
    if not r:
        continue
    try:
        buf += os.read(master, 1024)
    except OSError:
        continue
    while b"\r" in buf:
        line, buf = buf.split(b"\r", 1)
        cmd = line.decode(errors="replace").strip()
        if not cmd:
            continue
        with open(log, "a") as f:
            f.write(cmd + "\n")
        os.write(master, answer(cmd).encode())
