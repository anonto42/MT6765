#!/bin/sh
# wifi_boot.sh -- early-boot launcher embedded in the WalPad server ramdisk.
#
# Keep this file small and dependency-free.  The large userspace bundle,
# credentials, vendor/system images, and load-order-sensitive WiFi scripts live
# on the persistent SD card.  /init starts this in the background after mdev.

SD=/mnt/sd
W=$SD/wifi
LOG=$W/boot.log

klog() { echo "WALPAD-WIFI: $*" > /dev/kmsg 2>/dev/null; }

# The SD controller appears several seconds after PID 1.  Never create files
# under an unmounted /mnt/sd: they would land in the volatile ramdisk and then
# be hidden by the real card.
i=0
while [ ! -b /dev/mmcblk1p1 ] && [ "$i" -lt 60 ]; do
    /bin/busybox mdev -s 2>/dev/null
    sleep 1
    i=$((i + 1))
done
[ -b /dev/mmcblk1p1 ] || { klog "SD card did not appear"; exit 1; }

mkdir -p "$SD"
grep -q " $SD " /proc/mounts || mount /dev/mmcblk1p1 "$SD" 2>/dev/null
grep -q " $SD " /proc/mounts || { klog "could not mount SD card"; exit 1; }
mkdir -p "$W" "$W/ssh" /root/.ssh /etc/dropbear /var/run

exec >>"$LOG" 2>&1
echo
echo "=== boot $(date 2>/dev/null) uptime=$(cut -d' ' -f1 /proc/uptime) ==="
klog "SD mounted; starting WiFi"

# Restore identity into the volatile root before starting the network daemon.
if [ -s "$W/ssh/authorized_keys" ]; then
    cp "$W/ssh/authorized_keys" /root/.ssh/authorized_keys
fi
chown -R 0:0 /root
chmod 700 /root /root/.ssh
[ ! -f /root/.ssh/authorized_keys ] || chmod 600 /root/.ssh/authorized_keys

for type in rsa ed25519; do
    key=dropbear_${type}_host_key
    if [ -s "$W/ssh/$key" ]; then
        cp "$W/ssh/$key" "/etc/dropbear/$key"
    else
        dropbearkey -t "$type" -f "/etc/dropbear/$key" >/dev/null 2>&1 || continue
        cp "/etc/dropbear/$key" "$W/ssh/$key"
    fi
    chmod 600 "/etc/dropbear/$key"
done

[ -x "$SD/mtk_stack_up.sh" ] || { echo "missing $SD/mtk_stack_up.sh"; exit 1; }
[ -x "$SD/wifi_assoc.sh" ] || { echo "missing $SD/wifi_assoc.sh"; exit 1; }

sh "$SD/mtk_stack_up.sh" || { klog "CONNSYS stack failed; see $LOG"; exit 1; }
sh "$SD/wifi_assoc.sh" || { klog "association failed; see $LOG"; exit 1; }

# wifi_assoc starts dropbear on current images.  Ensure it is listening with
# the persistent keys even if an older SD-card copy of that script is used.
if ! netstat -ltn 2>/dev/null | grep -q ':22 '; then
    dropbear -r /etc/dropbear/dropbear_rsa_host_key \
        -r /etc/dropbear/dropbear_ed25519_host_key -E -p 22
fi

IP=$(ip -4 addr show wlan0 2>/dev/null | sed -n 's/.*inet \([0-9.]*\).*/\1/p' | head -1)
klog "ready at ${IP:-no-address}"
echo "=== WiFi ready at ${IP:-no-address} ==="
