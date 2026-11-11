"""Fake modem AT channels for the walpad-modem data tests (host only).

Creates pseudo-terminals and links their slaves next to argv[1] (like /dev/radio/pttycmd1):
  pttycmd1 (argv[1])  main AT channel
  pttynwcmd           MediaTek NetAgent command channel   (not with FAKE_AT_NO_NW=1)
  pttynwurc           MediaTek NetAgent URC channel       (not with FAKE_AT_NO_NW=1)
Answers AT commands and appends every command to argv[2] (netagent ones prefixed "NW:").
Env: FAKE_AT_CGACT_FAIL=1  data activation fails
     FAKE_AT_NO_URC=1      AT+CGACT=1,1 emits no "+EIF ... ifup" URC
     FAKE_AT_IFST_FAIL=1   the netagent channel refuses the ifst confirmation (+CME ERROR: 50)
Runs until killed.
"""
import os
import pty
import select
import sys
import tty

link, log = sys.argv[1], sys.argv[2]
d = os.path.dirname(link)


def make(path):
    master, slave = pty.openpty()
    tty.setraw(slave)
    if os.path.lexists(path):
        os.unlink(path)
    os.symlink(os.ttyname(slave), path)
    return master, slave  # keep the slave open: the pty must survive a reader closing it


main_m, _s1 = make(link)
nw = os.environ.get("FAKE_AT_NO_NW") != "1"
if nw:
    cmd_m, _s2 = make(os.path.join(d, "pttynwcmd"))
    urc_m, _s3 = make(os.path.join(d, "pttynwurc"))

REPLIES = {
    "AT+COPS=3,0": "",
    "AT+COPS?": '+COPS: 0,0,"Grameenphone",7',
    "AT+EDALLOW=1": "",
    "AT+CGACT=0,1": "",
    "AT+CGPADDR=1": '+CGPADDR: 1,"10.20.30.40"',
    "AT+CGCONTRDP=1": '+CGCONTRDP: 1,5,"gpinternet","10.20.30.40.255.255.255.0","10.20.30.1","8.8.8.8","8.8.4.4"',
}


def urc(text):
    if nw:
        os.write(urc_m, ("\r\n" + text + "\r\n").encode())


def answer_main(cmd):
    if cmd == "AT+CGACT=1,1" and os.environ.get("FAKE_AT_CGACT_FAIL") == "1":
        return "\r\n+CME ERROR: 148\r\n"
    if cmd.startswith("AT+CGDCONT=1,"):
        return "\r\nOK\r\n"
    if cmd == "AT+CGACT=1,1":
        if os.environ.get("FAKE_AT_NO_URC") != "1":
            urc("+EIF: 302, ifup, 1, 10.20.30.40, 0")
            urc("+EIF: 302, mtu, 1428")
        return "\r\n+EIND: 64\r\nOK\r\n"
    if cmd == "AT+CGACT=0,1":
        urc("+EIF: 302, ifdown, 1")
    if cmd in REPLIES:
        body = REPLIES[cmd]
        return ("\r\n%s\r\n\r\nOK\r\n" % body) if body else "\r\nOK\r\n"
    return "\r\nERROR\r\n"


def answer_nw(cmd):
    if os.environ.get("FAKE_AT_IFST_FAIL") == "1" and cmd.startswith("AT+EIF="):
        return "\r\n+CME ERROR: 50\r\n"
    if cmd in ('AT+EIF=302, "ifst", "up", 1', 'AT+EIF=302, "ifst", "down", 1', "AT+EIFCONFIG=3"):
        return "\r\nOK\r\n"
    return "\r\n+CME ERROR: 50\r\n"


chans = {main_m: ("", answer_main, b"")}
if nw:
    chans[cmd_m] = ("NW:", answer_nw, b"")
masters = list(chans)
bufs = {m: b"" for m in masters}
while True:
    r, _, _ = select.select(masters, [], [], 1.0)
    for m in r:
        try:
            bufs[m] += os.read(m, 1024)
        except OSError:
            continue
        prefix, fn, _ = chans[m]
        while b"\r" in bufs[m]:
            line, bufs[m] = bufs[m].split(b"\r", 1)
            cmd = line.decode(errors="replace").strip()
            if not cmd:
                continue
            with open(log, "a") as f:
                f.write(prefix + cmd + "\n")
            os.write(m, fn(cmd).encode())
