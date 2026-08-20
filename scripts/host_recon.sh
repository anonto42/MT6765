#!/usr/bin/env bash
# host_recon.sh — everything the HOST can learn about the tablet, no root, no
# serial access needed. Safe to run any time the device is plugged in.
# Output: data/host/
set -u
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
D="$REPO/data/host"
mkdir -p "$D"
TS="$(date -Is)"

echo "=== host-side recon -> $D ==="

# --- USB descriptors --------------------------------------------------------
{
  echo "# Host-side USB inventory of WalPad-Server  ($TS)"
  echo; echo "## lsusb"; lsusb | grep -i 0e8d || echo "(device not connected)"
  echo; echo "## lsusb -v"; lsusb -v -d 0e8d:2008 2>&1
} > "$D/usb-descriptors.txt"
echo "  usb-descriptors.txt"

# --- sysfs interface tree ---------------------------------------------------
USBDEV=""
for d in /sys/bus/usb/devices/*/; do
  [ -f "$d/idVendor" ] || continue
  if [ "$(cat "$d/idVendor" 2>/dev/null)" = "0e8d" ] &&
     [ "$(cat "$d/idProduct" 2>/dev/null)" = "2008" ]; then USBDEV="$d"; break; fi
done
{
  echo "# sysfs view of the WalPad-Server gadget  ($TS)"
  echo "node: ${USBDEV:-<not found>}"
  [ -n "$USBDEV" ] || exit 0
  echo; echo "## device attributes"
  for a in idVendor idProduct bcdDevice manufacturer product serial version speed \
           maxchild bNumInterfaces bNumConfigurations bmAttributes bMaxPower devnum busnum; do
    printf '%-20s %s\n' "$a" "$(cat "$USBDEV/$a" 2>/dev/null)"
  done
  echo; echo "## interfaces"
  for i in "$USBDEV"*:*/; do
    [ -d "$i" ] || continue
    echo "--- $(basename "$i")"
    for a in bInterfaceNumber bInterfaceClass bInterfaceSubClass bInterfaceProtocol \
             bNumEndpoints interface; do
      printf '  %-22s %s\n' "$a" "$(cat "$i/$a" 2>/dev/null)"
    done
    echo "  driver:                $(basename "$(readlink -f "$i/driver" 2>/dev/null)" 2>/dev/null)"
    echo "  endpoints:             $(ls -d "$i"/ep_* 2>/dev/null | xargs -n1 basename 2>/dev/null | tr '\n' ' ')"
    echo "  tty:                   $(ls "$i/tty" 2>/dev/null | tr '\n' ' ')"
    echo "  net:                   $(ls "$i/net" 2>/dev/null | tr '\n' ' ')"
  done
} > "$D/usb-sysfs.txt" 2>&1
echo "  usb-sysfs.txt"

# --- USB network link state -------------------------------------------------
{
  echo "# USB network link state  ($TS)"
  for n in $(ls /sys/class/net/); do
    drv="$(basename "$(readlink -f "/sys/class/net/$n/device/driver" 2>/dev/null)" 2>/dev/null)"
    case "$drv" in cdc_ether|rndis_host|cdc_ncm|cdc_eem) ;; *) continue ;; esac
    echo "=== $n  (driver: $drv) ==="
    for a in address operstate carrier mtu flags speed; do
      printf '  %-10s %s\n' "$a" "$(cat "/sys/class/net/$n/$a" 2>/dev/null)"
    done
    echo "  stats: rx_pkt=$(cat "/sys/class/net/$n/statistics/rx_packets" 2>/dev/null)" \
         "tx_pkt=$(cat "/sys/class/net/$n/statistics/tx_packets" 2>/dev/null)" \
         "rx_b=$(cat "/sys/class/net/$n/statistics/rx_bytes" 2>/dev/null)" \
         "tx_b=$(cat "/sys/class/net/$n/statistics/tx_bytes" 2>/dev/null)"
    echo "  addrs:"; ip -o addr show dev "$n" 2>/dev/null | sed 's/^/    /'
  done
  echo; echo "=== host routes ==="; ip route 2>/dev/null
  echo; echo "=== neighbours ==="; ip neigh 2>/dev/null
} > "$D/usb-net.txt" 2>&1
echo "  usb-net.txt"

# --- device access / permissions -------------------------------------------
{
  echo "# console access check  ($TS)"
  ls -l /dev/ttyACM* 2>&1
  echo
  for d in /dev/ttyACM0 /dev/ttyACM1; do
    [ -e "$d" ] || continue
    if [ -r "$d" ] && [ -w "$d" ]; then echo "$d: read+write OK (no sudo needed)"
    else echo "$d: NO ACCESS - run 'make udev' then replug"; fi
    getfacl "$d" 2>/dev/null | sed 's/^/    /'
  done
  echo; echo "user groups: $(id -nG)"
  echo "udev rule installed: $([ -f /etc/udev/rules.d/99-walpad.rules ] && echo yes || echo NO)"
} > "$D/access.txt" 2>&1
echo "  access.txt"

# --- kernel messages, if readable ------------------------------------------
{
  echo "# host kernel log for the tablet  ($TS)"
  (journalctl -k --since "-2 hours" 2>/dev/null || dmesg 2>/dev/null) \
    | grep -iE 'usb 3-|cdc_acm|cdc_ether|rndis|0e8d|ttyACM' | tail -60 \
    || echo "(kernel log not readable as this user - dmesg_restrict is on)"
} > "$D/host-dmesg.txt" 2>&1
echo "  host-dmesg.txt"

echo "=== done: $(ls "$D" | wc -l) files in data/host/ ==="
