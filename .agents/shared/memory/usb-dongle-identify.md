# USB Wi-Fi dongle: find the chip and the driver (host-PC side flow)

Scope: a dongle on the owner's Linux PC (not the tablet). Worked example 2026-11-11: Speedlink 300 Mbps
"MIW3100", `2310:1a2b` -> `2310:9086`, chip **ZTOP ZT9101**. Reusable for any "detected, no wlan" dongle.

## Fast path (do this first next time, no VM needed)

1. `lsusb`, `lsusb -v -d VID:PID`, `lsblk`. Interface class 8 + a 2 MB `sr0` ("CDROM--AUTORUN-") =
   driver-CD dongle: it must be mode-switched before it is a Wi-Fi device.
2. Switch at plug-in with a udev rule (`ACTION=="add", ATTR{idVendor}, ATTR{idProduct}, RUN+=usb_modeswitch -c
   /etc/usb_modeswitch.d/VID:PID -v .. -p ..`) and `StandardEject=1`. Stock rules only match the real Realtek
   `0bda:1a2b`; a clone vendor ID needs its own rule. `lsusb` then shows the real ID (here `2310:9086`).
3. Look the post-switch ID up in the **Microsoft Update Catalog** from Linux:
   `curl -sL "https://www.catalog.update.microsoft.com/Search.aspx?q=usb%5Cvid_XXXX%26pid_YYYY"` -> `goToDetails("GUID")`;
   `POST DownloadDialog.aspx` with `updateIDs=[{"size":0,"languages":"","uidInfo":"GUID","updateID":"GUID"}]` -> .cab URL.
   Extract with `7z x`. The `.inf` (plain text) names the chip and lists the VID/PID. Provider/model = chipset maker.
4. Search `<chip name> linux driver github`. Check the repo's USB ID table for the exact VID:PID, firmware shipped,
   kernel range. Read the installer before running it as root.

## What was done this time (and what cost time)

- Dead ends: web search of the model, the EAN (prefix 894 = Bangladesh GS1: local importer, no database hit),
  the vendor ID (not listed). The CD itself is unreadable on Linux (usb-storage hangs) and in the VM ("insert a disc").
- Wrong guess: Realtek RTL8188EU. Patched `rtl8xxxu` (needs matching `linux-headers`, `make -C`, module in `updates/`)
  bound but failed "failed to parse EFuse"; every register read returned one constant (0xfa075492). Constant reads
  across addresses/requests = the device does not speak the Realtek vendor protocol. Do not iterate fops variants.
- Root probe via usbfs ioctl (USBDEVFS_CONTROL 0xC0185500, vendor reads only) fingerprints protocols safely.
- Worked: Windows 10 VM (QEMU/KVM, `win10.qcow2` 25 GB, retail 22H2 ISO), dongle hot-added AFTER Windows was up,
  already switched to Wi-Fi mode by the host udev rule. Device Manager: "802.11n NIC" under Other devices.
  `Get-PnpDevice -PresentOnly` gave the hardware IDs; the Windows Update COM search returned the driver:
  `(New-Object -ComObject Microsoft.Update.Session).CreateUpdateSearcher().Search("IsInstalled=0 and Type='Driver'")`
  -> `Updates | % {Title, DriverHardwareID, DriverManufacturer}` = "ZTOP Microelectronics Corporation - Net - 15.56.1.960".
- Result: ZT9101 (ZTOP). Linux drivers (out of tree): dobbsbp/ztop-wifi-driver (Debian 13, kernel 6.12, ID table has
  0x2310:0x9086, firmware in `driver/fw`, DKMS), RadianceADM/zt9101-linux (kernels 6.13+, also lists 2310:9086),
  alexmorshu/ZT9101_vendor_linux_driver, phaylali/omniversify-usb-wireless-drivers. Windows package kept in
  `/home/md/Code/zt9101-wifi/` (`windows-package/` .cab + extracted `x/`, clone `dobbsbp/`), outside the VM. Built under that folder; not installed yet.

## VM recipe (only if the catalog route fails)

- `qemu-system-x86_64 -enable-kvm -machine q35 -cpu host -smp 4 -m 4096`, IDE disk, `-device qemu-xhci -device usb-tablet`,
  `-nic user,model=e1000e,ipv6=off`, `-monitor unix:mon.sock,server,nowait`. Needs group `kvm` and a udev rule
  (`ATTR{idVendor}=="2310", MODE="0666"`) so the user can open the dongle.
- Do NOT pass the dongle at boot while it is in CD mode: Windows Setup hangs on the fake CD. Attach live:
  `device_add usb-host,vendorid=0x2310,id=dongle` (match by vendor so the ID change after a switch still attaches).
- Screens: `screendump f.ppm` over the monitor, then `convert f.ppm f.png`. Keys: `sendkey`.
- The disc left in the VM is the Windows ISO (D:); its `setup.exe` is not the dongle driver.

## Linux driver result (2026-11-12, Kali VM kernel 7.2.15; the host was never touched)

- Use RadianceADM/zt9101-linux (kernels 6.13-7.2, lists 2310:9086); dobbsbp is for 6.12. DKMS install works.
- Normal Wi-Fi OK (WPA2, ping/DNS/TCP). Monitor mode AND packet injection (deauth) both work after a 10-part
  patch; full writeup and the patch itself: `/home/md/Code/zt9101-wifi/README.md` and
  `/home/md/Code/zt9101-wifi/patches/radianceadm-zt9101-monitor-and-injection.patch`.
- The injection blocker was NOT in the TX code itself: the vendor driver ties the kernel netif TX queue state to
  "associated to an AP" and never re-enables it for monitor mode, so dev_queue_xmit's qdisc silently drops every
  injected frame before ndo_start_xmit ever runs -- a raw socket's send() still reports success regardless, since
  that only confirms qdisc enqueue. Found only by putting a raw `printk` at the very first line of the driver's
  ndo_start_xmit and watching `journalctl -k -f` live during a send: zero hits proved the function itself was never
  entered, which took several rounds to pin down because `ip -s link show` TX stats are USELESS here -- the driver
  overrides `.ndo_get_stats` to report internal tx_info/rx_info counters, ignoring the stock net_device.stats
  entirely, so updating the latter (an easy first instinct) looks like it does nothing either way.
- Verified for real: aireplay-ng deauth against my own router+client got 64/64 over-the-air ACKs, and a concurrent
  tcpdump on the same interface showed the client actually doing a full reconnect twice during the attack. Then went
  further: captured a real WPA handshake (deauth-forced reconnect) and cracked it with aircrack-ng against a wordlist
  containing the real (not recorded here) passphrase -> KEY FOUND, matching it exactly. Full chain confirmed:
  monitor RX -> injection-forced handshake -> standard aircrack-ng tooling, all correct byte-for-byte (MIC verifies).
- `airmon-ng start` does NOT work for this driver (prints a bogus "already enabled" message, leaves interface in
  managed mode) -- its mac80211-era heuristics misfire on this from-scratch cfg80211 driver. Use manual `iw`/`ip link`
  commands instead (reliable throughout). A capture mixing several real stations' handshakes can also confuse
  aircrack-ng's own per-BSSID tracking -- isolate one station's frames with tshark (`-F pcap`, not the pcapng
  default) before cracking if the whole-file result looks wrong. airodump-ng's stdout redirected to a log file
  fills a tmpfs /tmp fast (600+MB per ~30s run from its own ncurses redraw) -- don't log it to a file.
- Side finding on the owner's real network, unrelated to this work: a device (60:01:94:3d:6d:f3, likely the
  "WLED-AP" controller) is stuck in an endless Msg1/Msg2 handshake retry loop against "Loff" -- looks like a wrong
  cached Wi-Fi password on that device. Worth telling the owner.
- Kali VM recipe: official `kali-linux-*-qemu-amd64.7z` (verify SHA256), overlay qcow2, hostfwd 2222->22, DNS 8.8.8.8
  in the guest, `apt install linux-image-amd64 linux-headers-amd64 dkms` (image kernel's headers are gone), then SSH
  with a dedicated key. Remote shell is zsh: unmatched globs abort `&&` chains. Never put the Wi-Fi passphrase in argv:
  pipe it to `wpa_passphrase` over stdin and keep only the PSK hash (mode 600).

