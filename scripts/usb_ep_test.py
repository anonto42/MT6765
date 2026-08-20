#!/usr/bin/env python3
"""Raw USB bulk-OUT test against the WalPad-Server gadget.

Determines whether the device's gadget services the ethernet OUT
endpoints (ECM EP 0x03, RNDIS EP 0x04) by writing a probe frame with a
short timeout. A timeout = device NAKs (RX never queued); success =
device accepted the USB packet (bug is higher in its net stack).

Does NOT touch the console interfaces (iface 0/1) so the serial shell
survives. Detaches + reattaches the ethernet kernel drivers around the
test.
"""

import sys
import time

import usb.core
import usb.util

VID = 0x0E8D
PID = 0x2008

# interface index -> (label, OUT endpoint bEndpointAddress, data alt-setting)
TARGETS = {
    3: ("ACM GS1 data (ttyGS1)", 0x02, 0),
    5: ("ECM data (usb0)", 0x03, 1),
    7: ("RNDIS data (rndis0)", 0x04, 1),
}

PROBE = bytes(range(64))
BIG = bytes([0xA5]) * 1514


def find_device():
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("no WalPad-Server device found (0e8d:2008)")
        return None
    print(f"device: bus {dev.bus} addr {dev.address}")
    return dev


def _find_data_intf(cfg, iface):
    # interface appears twice (alt settings); pick the one with the bulk OUT EP
    for d in cfg:
        if d.bInterfaceNumber == iface:
            ep = usb.util.find_descriptor(d, bmAttributes=usb.ENDPOINT_TYPE_BULK)
            if ep is not None:
                return d
    return usb.util.find_descriptor(cfg, bInterfaceNumber=iface)


def test_ep(dev, iface, label, ep_out, altsetting):
    cfg = dev.get_active_configuration()
    intf = _find_data_intf(cfg, iface)
    if intf is None:
        print(f"  iface {iface} not found")
        return
    claimed = False
    try:
        if dev.is_kernel_driver_active(iface):
            dev.detach_kernel_driver(iface)
        usb.util.claim_interface(dev, iface)
        claimed = True
        if altsetting:
            dev.set_interface_altsetting(iface, altsetting)
        intf = _find_data_intf(cfg, iface)
        ep = usb.util.find_descriptor(intf, bmAttributes=usb.ENDPOINT_TYPE_BULK,
                                      bEndpointAddress=ep_out)
        if ep is None:
            print(f"  {label}: OUT EP 0x{ep_out:02x} not found")
            return
        for name, payload in (("64B", PROBE), ("1514B", BIG)):
            t0 = time.monotonic()
            try:
                n = ep.write(payload, timeout=2500)
                dt = time.monotonic() - t0
                print(f"  {label}: OUT EP 0x{ep_out:02x} {name} wrote {n} bytes in {dt:.3f}s  -> ACCEPTED")
            except usb.core.USBTimeoutError:
                print(f"  {label}: OUT EP 0x{ep_out:02x} {name} TIMEOUT after 2.5s  -> NAK'd")
            except usb.core.USBError as e:
                print(f"  {label}: OUT EP 0x{ep_out:02x} {name} USBError: {e}")
    finally:
        if claimed:
            try:
                usb.util.dispose_resources(dev)
            except Exception:
                pass
            try:
                usb.util.release_interface(dev, intf)
            except Exception:
                pass
        for n in (3, 5, 7):
            try:
                if not dev.is_kernel_driver_active(n):
                    dev.attach_kernel_driver(n)
            except Exception:
                pass


def main():
    dev = find_device()
    if dev is None:
        sys.exit(1)
    try:
        dev.set_configuration()
    except usb.core.USBError as e:
        print(f"set_configuration: {e}")
    for iface, (label, ep_out, altsetting) in TARGETS.items():
        test_ep(dev, iface, label, ep_out, altsetting)
    print("done (drivers reattached where possible)")


if __name__ == "__main__":
    main()