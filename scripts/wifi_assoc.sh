#!/bin/sh
# wifi_assoc.sh -- run ON THE TABLET, after mtk_stack_up.sh has produced wlan0.
#
# Associates with the network in /mnt/sd/wifi/wifi.conf and gets a lease.
#
# Two supplicants are possible and the musl one wins whenever it is present.
# Android's /vendor/bin/hw/wpa_supplicant cannot be linked at all here --
#     CANNOT LINK EXECUTABLE: library "libnl.so" not found
# because libnl ships in the product/system_ext images, which this firmware
# does not carry.  The bundle in data/wifi-userspace brings its own libnl-3,
# so push_wifi_userspace.py drops working wrappers in $W/sbin and we use those.
# The vendor path is kept only as a fallback for a device that does have libnl.

SD=/mnt/sd
W=$SD/wifi
SYS=/system/system
LINKER=$SYS/bin/bootstrap/linker64
IFACE=wlan0

export LD_LIBRARY_PATH=$SYS/lib64/bootstrap:/vendor/lib64:$SYS/lib64
export LD_PRELOAD=$W/libwpdprops.so

if [ -x $W/sbin/wpa_supplicant ]; then
    # The wrappers invoke the musl loader themselves and must NOT inherit the
    # bionic preload -- the shim is an Android-only .so and would abort them.
    SUP="env -u LD_PRELOAD -u LD_LIBRARY_PATH $W/sbin/wpa_supplicant"
    CLI="env -u LD_PRELOAD -u LD_LIBRARY_PATH $W/sbin/wpa_cli"
    SUPBIN=$W/sbin/wpa_supplicant
else
    SUP="$LINKER /vendor/bin/hw/wpa_supplicant"
    CLI="$LINKER /vendor/bin/wpa_cli"
    SUPBIN=/vendor/bin/hw/wpa_supplicant
fi

say() { echo "[wifi] $*"; }
die() { echo "[wifi] !! $*"; exit 1; }

[ -e /sys/class/net/$IFACE ] || die "$IFACE does not exist -- run mtk_stack_up.sh first"
[ -f $W/wifi.conf ] || die "missing $W/wifi.conf"
. $W/wifi.conf
[ -n "${SSID:-}" ] || die "SSID not set"
[ -x $SUPBIN ] || die "no supplicant at $SUPBIN"

ip link set $IFACE up 2>/dev/null || ifconfig $IFACE up

# --- config ----------------------------------------------------------------
mkdir -p /tmp/wpa
if [ -n "${PSK:-}" ]; then
    cat > /tmp/wpa.conf <<CONF
ctrl_interface=/tmp/wpa
update_config=1
network={
	ssid="$SSID"
	psk="$PSK"
	key_mgmt=WPA-PSK
}
CONF
else
    cat > /tmp/wpa.conf <<CONF
ctrl_interface=/tmp/wpa
update_config=1
network={
	ssid="$SSID"
	key_mgmt=NONE
}
CONF
fi
chmod 600 /tmp/wpa.conf

# --- supplicant ------------------------------------------------------------
# No killall in this busybox, and the process shows up as "linker64" anyway.
for p in $(ps | grep -v grep | grep '[w]pa_supplicant' | awk '{print $1}'); do
    kill $p 2>/dev/null
done
sleep 1

say "using $SUPBIN"
setsid $SUP -i $IFACE -c /tmp/wpa.conf -Dnl80211 \
    > /tmp/wpa.log 2>&1 < /dev/null &
say "supplicant started, waiting for association"

i=0
state=
while [ $i -lt 40 ]; do
    sleep 1
    state=$($CLI -p /tmp/wpa -i $IFACE status 2>/dev/null \
            | sed -n 's/^wpa_state=//p')
    case "$state" in
        COMPLETED) break ;;
    esac
    i=$((i+1))
done
[ "$state" = "COMPLETED" ] || {
    say "did not associate (last state: ${state:-none})"
    say "---- supplicant log ----"
    tail -25 /tmp/wpa.log
    die "association failed"
}
say "associated with $SSID"

# --- address ---------------------------------------------------------------
# busybox udhcpc does nothing without a script to apply the lease.
mkdir -p /tmp/udhcpc
cat > /tmp/udhcpc/simple.script <<'SCR'
#!/bin/sh
case "$1" in
    bound|renew)
        ip addr flush dev $interface 2>/dev/null
        ip addr add $ip/${mask:-24} dev $interface
        ip link set $interface up
        [ -n "$router" ] && ip route add default via ${router%% *} dev $interface 2>/dev/null
        : > /etc/resolv.conf
        for d in $dns; do echo "nameserver $d" >> /etc/resolv.conf; done
        ;;
esac
exit 0
SCR
chmod +x /tmp/udhcpc/simple.script

for p in $(ps | grep -v grep | grep '[u]dhcpc' | awk '{print $1}'); do
    kill $p 2>/dev/null
done
say "requesting DHCP lease"
udhcpc -i $IFACE -n -q -t 8 -s /tmp/udhcpc/simple.script 2>&1 | tail -5

IP=$(ip -4 addr show $IFACE 2>/dev/null | sed -n 's/.*inet \([0-9.]*\).*/\1/p' | head -1)
[ -n "$IP" ] || die "no IPv4 address -- DHCP did not complete"

say "=== $IFACE has $IP ==="
ip route 2>/dev/null | head -5
ping -c 2 -W 3 ${GW:-1.1.1.1} 2>&1 | tail -3

# --- ssh --------------------------------------------------------------------
# The point of all of this: a shell that does not need the USB tether.
#
# /root ships owned by uid 1000 -- the ramdisk was built by a non-root user --
# and dropbear refuses an authorized_keys file the logging-in user does not own,
# failing with nothing but "Permission denied (publickey,password)" on the
# client.  Fix the ownership before starting the daemon, not after.
if [ -f /root/.ssh/authorized_keys ]; then
    chown -R 0:0 /root
    chmod 700 /root /root/.ssh
    chmod 600 /root/.ssh/authorized_keys
fi

if ! ps | grep -v grep | grep -q '[d]ropbear -R'; then
    mkdir -p /etc/dropbear
    dropbear -R -E -p 22 > /tmp/dropbear.log 2>&1
    sleep 1
fi
ps | grep -v grep | grep -q '[d]ropbear' \
    && say "sshd listening -- ssh root@$IP" \
    || say "dropbear did not start, see /tmp/dropbear.log"
