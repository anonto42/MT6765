#!/usr/bin/env python3
"""
probe_server.py — catch the 'WalPad-Server' (0e8d:2008) boot window and probe it fast.

The WalPad 8G cycles BROM(0003) -> preloader(2000) -> WalPad-Server(2008) -> off
when the battery is too low to hold a full boot.  The 2008 window is short, so
this script watches USB in a tight loop and the instant the gadget appears it:

  * reads the USB descriptor (is it a Linux gadget or an MTK preloader?)
  * if root: brings up the RNDIS/ECM links, ARP-scans for the device's IP,
    tries the common gadget addresses, and probes common server ports
  * reads both /dev/ttyACM consoles for output

Run as root for the network/serial probing:
    sudo python3 scripts/probe_server.py

Keeps running until Ctrl-C; prints one probe result per 2008 window.
"""
import os
import sys
import time
import subprocess
import socket

VID = 0x0E8D
PIDS = {0x0003: "BROM", 0x2000: "preloader", 0x2001: "preloader", 0x2008: "WalPad-Server", 0x20FF: "preloader"}
RNDIS_IF = "enp10s0f3u1i6"
ECM_IF = "enp10s0f3u1i4"
CANDIDATE_IPS = ["192.168.42.129", "192.168.42.1", "192.168.42.2", "172.16.42.1", "192.168.0.1", "10.0.0.1"]
HOST_IP = "192.168.42.100/24"
PORTS = [22, 80, 443, 23, 53, 8080, 9999, 445, 2049]


def run(cmd, timeout=3):
    try:
        return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout).stdout
    except Exception:
        return ""


def probe_network():
    results = []
    if os.geteuid() != 0:
        return ["(non-root: skip network probe)"]
    for iface in (RNDIS_IF, ECM_IF):
        run(f"ip link set {iface} up")
        run(f"ip addr flush dev {iface}")
        run(f"ip addr add {HOST_IP} dev {iface}")
        found = []
        for ip in CANDIDATE_IPS:
            run(f"ping -c1 -W1 {ip} >/dev/null 2>&1")
            neighbors = run(f"ip neigh show dev {iface}")
            if ip in neighbors:
                found.append(ip)
        results.append(f"{iface}: reachable={found}")
        for ip in found:
            for p in PORTS:
                try:
                    s = socket.create_connection((ip, p), timeout=1)
                    s.close()
                    results.append(f"  {ip}:{p} OPEN")
                except Exception:
                    pass
        run(f"ip addr flush dev {iface}")
    return results


def probe_serial():
    results = []
    if os.geteuid() != 0:
        return ["(non-root: skip serial probe)"]
    for port in ("/dev/ttyACM0", "/dev/ttyACM1"):
        try:
            import serial
            s = serial.Serial(port, 115200, timeout=1, write_timeout=1)
            time.sleep(0.2)
            s.reset_input_buffer()
            s.write(b"\r\n")
            s.flush()
            time.sleep(0.8)
            data = s.read(512)
            s.close()
            results.append(f"{port}: {'response ' + data[:64].hex() if data else '(silent)'}")
        except Exception as e:
            results.append(f"{port}: error {e}")
    return results


def current_usb():
    try:
        import usb.core
        devs = usb.core.find(find_all=True)
        for d in devs:
            if d.idVendor == VID:
                return d.idProduct, d.product, d.manufacturer
    except Exception:
        pass
    try:
        out = run("lsusb | grep -i 0e8d")
        if "0e8d:2008" in out:
            return 0x2008, None, None
        if "0e8d:2000" in out:
            return 0x2000, None, None
        if "0e8d:0003" in out:
            return 0x0003, None, None
    except Exception:
        pass
    return None, None, None


def main():
    print("Watching for WalPad-Server (2008) windows. Ctrl-C to stop.")
    prev = None
    last_trigger = 0
    while True:
        pid, product, mfr = current_usb()
        if pid != prev:
            name = PIDS.get(pid, "?")
            print(f"[{time.strftime('%H:%M:%S')}] {name} ({pid and hex(pid)})")
            prev = pid
        if pid == 0x2008 and time.time() - last_trigger > 5:
            last_trigger = time.time()
            print(f"[{time.strftime('%H:%M:%S')}] --- probing 2008 window ---")
            print("  descriptor:", product, "/", mfr)
            for line in probe_network():
                print("  " + line)
            for line in probe_serial():
                print("  " + line)
            print(f"[{time.strftime('%H:%M:%S')}] --- probe done ---")
        time.sleep(0.1)


if __name__ == "__main__":
    main()