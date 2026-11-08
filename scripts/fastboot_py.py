#!/usr/bin/env python3
"""Minimal fastboot client (pyusb). Usage: fb.py CMD [CMD...]   e.g. fb.py getvar:all"""
import sys, usb.core, usb.util
VID, PID = 0x0e8d, 0x201c
dev = usb.core.find(idVendor=VID, idProduct=PID)
if dev is None: sys.exit("no fastboot device")
try:
    if dev.is_kernel_driver_active(0): dev.detach_kernel_driver(0)
except Exception: pass
cfg = dev.get_active_configuration()
intf = None
for i in cfg:
    if i.bInterfaceClass == 0xff and i.bInterfaceSubClass == 0x42 and i.bInterfaceProtocol == 0x03: intf = i
intf = intf or cfg[(0, 0)]
ep_out = usb.util.find_descriptor(intf, custom_match=lambda e: usb.util.endpoint_direction(e.bEndpointAddress) == usb.util.ENDPOINT_OUT)
ep_in = usb.util.find_descriptor(intf, custom_match=lambda e: usb.util.endpoint_direction(e.bEndpointAddress) == usb.util.ENDPOINT_IN)
def cmd(c, timeout=10000):
    ep_out.write(c.encode(), timeout)
    out = []
    while True:
        r = bytes(ep_in.read(512, timeout)).decode(errors="replace")
        t, body = r[:4], r[4:]
        if t == "INFO": out.append(body)
        elif t in ("OKAY", "FAIL"): out.append("%s %s" % (t, body)); return out
        elif t == "DATA": out.append("DATA " + body); return out
        else: out.append(r); return out
for c in sys.argv[1:]:
    print(">>", c)
    try:
        for l in cmd(c): print("  ", l)
    except Exception as e: print("   ERR", e)
