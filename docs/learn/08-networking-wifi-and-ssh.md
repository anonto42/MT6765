# 08 — Networking, Wi-Fi & SSH

Understanding basic Linux networking helps you connect your tablet to Wi-Fi, manage it from your PC over SSH, and use network tools in Kali.

---

## 1. Network Interfaces on Your Tablet

Run `ip a` (or `ip addr`) in Terminal to see your network interfaces:

- **`lo` (`127.0.0.1`)**: Loopback — the tablet talking to itself.
- **`wlan0`**: Main Wi-Fi client interface (connects to your router/hotspot).
- **`ap0`**: Wi-Fi hotspot interface (used when the tablet acts as an access point).
  - *Hardware limit:* On this MediaTek Wi-Fi chip, `wlan0` and `ap0` cannot be active at the same time.
- **`usb0` / `wlan1` (if plugged in)**: External USB Ethernet or USB Wi-Fi dongle via OTG.

---

## 2. Useful Networking Commands

```sh
ip a                    # Show IP addresses and interfaces
ip r                    # Show default gateway (route)
cat /etc/resolv.conf    # Show DNS servers
ping -c 3 1.1.1.1       # Test internet connectivity by IP
ping -c 3 google.com    # Test DNS + internet connectivity
ss -tulnp               # Show open listening ports (TCP/UDP)
```

---

## 3. Connecting from Your PC via SSH

When your tablet is on the same Wi-Fi network as your PC, you can control it from your PC's terminal (much faster than typing on-screen!).

In this repo, SSH is pre-configured in `build/ssh/config` under the host name **`walpad`**:

```sh
ssh -F build/ssh/config walpad
```

Once connected over SSH, you are in the same **Alpine Linux** environment as the tablet's normal **Terminal** app. You can also enter Kali from SSH with:
```sh
walpad-kali shell
```

---

**Next:** [09 — Kali Chroot & Safe Lab Practice](09-kali-chroot-and-safe-lab-use.md)
