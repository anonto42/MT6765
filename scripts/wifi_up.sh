#!/bin/sh
# wifi_up.sh — run ON THE TABLET (via serial console).
#
# Brings the WiFi interface up and joins a network.
#   1. load the connectivity driver (module or built-in)
#   2. ask the MTK CONNSYS subsystem to create wlan0  (/dev/wmtWifi)
#   3. associate with wpa_supplicant
#   4. get an address with udhcpc
#
# Credentials come from /mnt/sd/wifi/wifi.conf:
#     SSID="MyNetwork"
#     PSK="mypassword"
# (leave PSK empty for an open network)
#
# Usage: sh /mnt/sd/wifi_up.sh

SD=/mnt/sd
W=$SD/wifi
IFACE=wlan0

export PATH="$W/sbin:/sbin:/bin:/usr/sbin:/usr/bin:$PATH"

say() { echo "[wifi] $*"; }
die() { echo "[wifi] !! $*"; exit 1; }

[ -f "$W/wifi.conf" ] || die "missing $W/wifi.conf (set SSID= and PSK=)"
. "$W/wifi.conf"
[ -n "${SSID:-}" ] || die "SSID not set in $W/wifi.conf"

# --- 1. driver -------------------------------------------------------------
if [ ! -d /sys/module/cfg80211 ]; then
    say "cfg80211 not loaded, trying modules from $W/modules"
    # order matters: platform/connectivity core first, wlan driver last
    for m in conninfra.ko wmt_drv.ko wmt_chrdev_wifi.ko connfem.ko \
             cfg80211.ko mac80211.ko \
             wlan_drv_gen4m.ko wlan_drv_gen4m_6765.ko wlan_mt6765.ko wlan.ko; do
        [ -f "$W/modules/$m" ] || continue
        insmod "$W/modules/$m" 2>&1 | grep -v 'File exists' && say "insmod $m"
    done
else
    say "cfg80211 already present"
fi

# --- 1b. firmware -----------------------------------------------------------
# The kernel was booted with firmware_class.path=/vendor/firmware, but /vendor
# lives inside the `super` dynamic partition and nothing mounts it.  Without it
# the driver can't load the WiFi RAM code, which is why no wlan0 ever appears.
# scripts/vendor_up.py mounts it from the host; re-check here so this script is
# also useful standalone.
FWPATH=$(cat /sys/module/firmware_class/parameters/path 2>/dev/null)
[ -n "$FWPATH" ] || FWPATH=/lib/firmware
if [ ! -d "$FWPATH" ]; then
    say "!! firmware path $FWPATH does not exist - /vendor is not mounted"
    say "   run this from the host first:  make vendor-up"
else
    say "firmware path $FWPATH present ($(ls "$FWPATH" 2>/dev/null | wc -l) entries)"
fi

# A built-in driver that already failed its firmware load at boot will not retry
# on its own.  Unbind/rebind the platform device so it probes again, now that
# the firmware is actually reachable.
WIFIDEV=18000000.wifi
DRVLINK=/sys/bus/platform/devices/$WIFIDEV/driver
if [ ! -e /sys/class/net/$IFACE ] && [ -L "$DRVLINK" ]; then
    DRV=$(readlink -f "$DRVLINK")
    say "re-probing $WIFIDEV via $(basename "$DRV")"
    echo "$WIFIDEV" > "$DRV/unbind" 2>/dev/null
    sleep 1
    echo "$WIFIDEV" > "$DRV/bind"   2>/dev/null
    sleep 2
fi

# --- 2. ask CONNSYS to create the interface --------------------------------
if [ -e /dev/wmtWifi ]; then
    say "enabling WiFi via /dev/wmtWifi"
    echo 1 > /dev/wmtWifi 2>/dev/null || say "write to /dev/wmtWifi failed"
    i=0
    while [ $i -lt 15 ]; do
        [ -e "/sys/class/net/$IFACE" ] && break
        sleep 1; i=$((i+1))
    done
fi

if [ ! -e "/sys/class/net/$IFACE" ]; then
    say "interfaces present: $(ls /sys/class/net/ | tr '\n' ' ')"
    # some builds name it wlan1 / wlp* — take the first wireless-capable one
    for n in /sys/class/net/*/wireless /sys/class/net/*/phy80211; do
        [ -e "$n" ] || continue
        IFACE=$(basename "$(dirname "$n")")
        say "using discovered wireless interface: $IFACE"
        break
    done
fi
# Last resort: a wiphy can be registered with no netdev attached to it (this
# board boots in exactly that state - phy0 exists, wlan0 does not).  If so, ask
# cfg80211 to create the interface directly.
if [ ! -e "/sys/class/net/$IFACE" ] && command -v iw >/dev/null 2>&1; then
    for phy in /sys/class/ieee80211/*; do
        [ -d "$phy" ] || continue
        PHY=$(basename "$phy")
        say "no netdev on $PHY - creating one with iw"
        iw phy "$PHY" interface add "$IFACE" type managed 2>&1 && sleep 1
        [ -e "/sys/class/net/$IFACE" ] && break
    done
fi

[ -e "/sys/class/net/$IFACE" ] || die "no wireless interface — see docs/10-wifi-plan.md, phase 1"

ip link set "$IFACE" up 2>/dev/null || ifconfig "$IFACE" up
say "$IFACE is up"

# --- 3. associate ----------------------------------------------------------
if command -v wpa_supplicant >/dev/null 2>&1; then
    CONF=$W/wpa_supplicant.conf
    if [ -n "${PSK:-}" ]; then
        if command -v wpa_passphrase >/dev/null 2>&1; then
            wpa_passphrase "$SSID" "$PSK" > "$CONF"
        else
            printf 'network={\n\tssid="%s"\n\tpsk="%s"\n}\n' "$SSID" "$PSK" > "$CONF"
        fi
    else
        printf 'network={\n\tssid="%s"\n\tkey_mgmt=NONE\n}\n' "$SSID" > "$CONF"
    fi
    sed -i '1i ctrl_interface=/var/run/wpa_supplicant\nupdate_config=1' "$CONF" 2>/dev/null
    mkdir -p /var/run/wpa_supplicant

    killall wpa_supplicant 2>/dev/null
    wpa_supplicant -B -i "$IFACE" -c "$CONF" -D nl80211 \
        || wpa_supplicant -B -i "$IFACE" -c "$CONF" -D wext \
        || die "wpa_supplicant failed to start"
    say "wpa_supplicant started, waiting for association"
    i=0
    while [ $i -lt 20 ]; do
        if command -v iw >/dev/null 2>&1; then
            iw dev "$IFACE" link 2>/dev/null | grep -q 'Connected to' && break
        else
            [ "$(cat /sys/class/net/$IFACE/operstate 2>/dev/null)" = "up" ] && break
        fi
        sleep 1; i=$((i+1))
    done
elif command -v iw >/dev/null 2>&1 && [ -z "${PSK:-}" ]; then
    say "no wpa_supplicant — joining open network with iw"
    iw dev "$IFACE" connect "$SSID" || die "iw connect failed"
else
    die "no wpa_supplicant on this system (WPA network needs it) — see plan phase 3"
fi

# --- 4. address ------------------------------------------------------------
say "requesting DHCP lease"
killall udhcpc 2>/dev/null
udhcpc -i "$IFACE" -n -q -t 8 2>&1 | tail -5 || say "udhcpc failed"

[ -s /etc/resolv.conf ] || printf 'nameserver 1.1.1.1\nnameserver 8.8.8.8\n' > /etc/resolv.conf

# --- report ----------------------------------------------------------------
echo ""
say "=== result ==="
ip addr show "$IFACE" 2>/dev/null || ifconfig "$IFACE"
ip route 2>/dev/null || route -n
echo ""
say "connectivity test:"
ping -c 2 -W 3 1.1.1.1 2>&1 | tail -3

# --- 5. make the tablet reachable over WiFi --------------------------------
# The whole point of this exercise: SSH to the server without the USB tether.
IP=$(ip -4 addr show "$IFACE" 2>/dev/null | sed -n 's/.*inet \([0-9.]*\).*/\1/p' | head -1)
if [ -n "$IP" ]; then
    mkdir -p /etc/dropbear
    [ -f /etc/dropbear/dropbear_rsa_host_key ] || \
        dropbearkey -t rsa -f /etc/dropbear/dropbear_rsa_host_key >/dev/null 2>&1
    [ -f /etc/dropbear/dropbear_ed25519_host_key ] || \
        dropbearkey -t ed25519 -f /etc/dropbear/dropbear_ed25519_host_key >/dev/null 2>&1
    killall dropbear 2>/dev/null
    dropbear -R -B -p 22 2>&1 | tail -2
    say "=== reachable at:  ssh root@$IP"
else
    say "no IPv4 address on $IFACE - DHCP did not complete"
fi
