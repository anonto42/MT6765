#!/usr/bin/env python3
"""Push a file to the tablet over the serial console: base64 in small verified parts, reassembled and
checked (sha256) on the tablet.   scripts/tablet_put.py <local> <remote-path> [mode]
The console drops data on big bursts, so parts are ~2 KB and each one is verified (and retried)."""
import base64, hashlib, sys, time, serial
src, dst = sys.argv[1], sys.argv[2]; mode = sys.argv[3] if len(sys.argv) > 3 else "755"
data = open(src, "rb").read(); sha = hashlib.sha256(data).hexdigest()
PART = 2000
s = serial.Serial("/dev/ttyACM0", 115200, timeout=1, write_timeout=10)
def cmd(c, wait=0.15):
    s.write((c + "\n").encode()); time.sleep(wait)
def run(c, wait=1.2):
    s.reset_input_buffer(); cmd(c, wait); return s.read(4000).decode(errors="replace").replace("\r", "")
s.write(b"\x03\n"); time.sleep(0.5); s.read(2000)
parts = [data[i:i + PART] for i in range(0, len(data), PART)] or [b""]
cmd("rm -f /tmp/put.part.*", 0.3)
for n, part in enumerate(parts):
    ph = hashlib.sha256(part).hexdigest(); b64 = base64.b64encode(part).decode()
    for attempt in range(4):
        cmd("rm -f /tmp/p.b64", 0.2)
        for i in range(0, len(b64), 100): cmd("echo -n '%s' >> /tmp/p.b64" % b64[i:i + 100], 0.16)
        out = run("base64 -d /tmp/p.b64 > /tmp/put.part.%04d && sha256sum /tmp/put.part.%04d | cut -c1-64; echo PART_END" % (n, n), 1.5)
        if ph in out: break
        cmd("rm -f /tmp/put.part.%04d" % n, 0.2)
    else:
        print("part %d failed verification" % n); sys.exit(1)
out = run("cat /tmp/put.part.* > /tmp/put.bin && sha256sum /tmp/put.bin | cut -c1-64; rm -f /tmp/put.part.* /tmp/p.b64; echo ALL_END", 2.0)
ok = sha in out
print("sha verified on tablet:", ok)
if ok:
    cmd("cp /tmp/put.bin '%s' && chmod %s '%s' && rm -f /tmp/put.bin && echo INSTALLED" % (dst, mode, dst), 1.5)
else:
    print(out[-300:]); sys.exit(1)
