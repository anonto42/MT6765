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
  `/home/md/vms/ztop/` (.cab, extracted `x/`, clone `dobbsbp/`), outside the VM. Not built or installed yet.

## VM recipe (only if the catalog route fails)

- `qemu-system-x86_64 -enable-kvm -machine q35 -cpu host -smp 4 -m 4096`, IDE disk, `-device qemu-xhci -device usb-tablet`,
  `-nic user,model=e1000e,ipv6=off`, `-monitor unix:mon.sock,server,nowait`. Needs group `kvm` and a udev rule
  (`ATTR{idVendor}=="2310", MODE="0666"`) so the user can open the dongle.
- Do NOT pass the dongle at boot while it is in CD mode: Windows Setup hangs on the fake CD. Attach live:
  `device_add usb-host,vendorid=0x2310,id=dongle` (match by vendor so the ID change after a switch still attaches).
- Screens: `screendump f.ppm` over the monitor, then `convert f.ppm f.png`. Keys: `sendkey`.
- The disc left in the VM is the Windows ISO (D:); its `setup.exe` is not the dongle driver.
