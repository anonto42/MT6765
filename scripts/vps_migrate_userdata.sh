#!/bin/sh
# DESTRUCTIVE: turn Android userdata into the persistent Alpine VPS filesystem.
set -e

DEV=/dev/mmcblk0p44
SD=/mnt/sd
VPS=$SD/vps
OLD=/mnt/vps
NEW=/mnt/vps-new

grep -q '^PARTNAME=userdata$' /sys/class/block/mmcblk0p44/uevent \
    || { echo "refusing: $DEV is not GPT partition userdata"; exit 1; }
grep -q ' /mnt/vps ' /proc/mounts \
    || { echo "refusing: existing Alpine source is not mounted"; exit 1; }
[ -f "$OLD/etc/alpine-release" ] \
    || { echo "refusing: source is not an Alpine root"; exit 1; }

echo "Formatting $DEV (userdata) as ext4..."
mke2fs -F -L walpad-vps -m 1 "$DEV"
mkdir -p "$NEW"
mount -t ext4 "$DEV" "$NEW"

# Remove transient host mounts so they are not recursively copied.
umount "$OLD/dev" 2>/dev/null || true
umount "$OLD/sys" 2>/dev/null || true
umount "$OLD/proc" 2>/dev/null || true

echo "Migrating Alpine to internal eMMC..."
tar -C "$OLD" -cf - . | tar -C "$NEW" -xf -
sync
[ -f "$NEW/etc/alpine-release" ] || { echo "migration verification failed"; exit 1; }

umount "$NEW"
umount "$OLD"
for l in /dev/loop*; do
    losetup "$l" 2>/dev/null | grep -q "$VPS/rootfs.ext4" && losetup -d "$l"
done

touch "$VPS/use-internal-userdata"
sh "$VPS/boot.sh"
grep -q "$DEV $OLD ext4" /proc/mounts
echo "Internal VPS migration complete."
df -h "$OLD"
