#!/bin/sh
# Mount and start the persistent Alpine/Tailscale VPS payload from the SD card.
SD=/mnt/sd
VPS=$SD/vps
ROOT=/mnt/vps
LOG=$VPS/boot.log
DATA_DEV=/dev/mmcblk0p44

mkdir -p "$ROOT" "$VPS"
exec >>"$LOG" 2>&1
echo "=== VPS boot $(date 2>/dev/null) ==="

mounted() { grep -q " $1 " /proc/mounts; }

if ! mounted "$ROOT"; then
    if [ -f "$VPS/use-internal-userdata" ]; then
        mount -t ext4 "$DATA_DEV" "$ROOT" || exit 1
    else
        LOOP=
        for l in /dev/loop*; do
            [ -b "$l" ] || continue
            losetup "$l" 2>/dev/null | grep -q "$VPS/rootfs.ext4" && LOOP=$l && break
        done
        [ -n "$LOOP" ] || LOOP=$(losetup -f)
        losetup "$LOOP" 2>/dev/null | grep -q "$VPS/rootfs.ext4" \
            || losetup "$LOOP" "$VPS/rootfs.ext4" || exit 1
        mount -t ext4 "$LOOP" "$ROOT" || exit 1
    fi
fi

mkdir -p "$ROOT/proc" "$ROOT/sys" "$ROOT/dev" "$ROOT/dev/pts" "$ROOT/dev/shm" "$ROOT/run"
mounted "$ROOT/proc" || mount -t proc proc "$ROOT/proc"
mounted "$ROOT/sys" || mount --bind /sys "$ROOT/sys"
mounted "$ROOT/dev" || mount --bind /dev "$ROOT/dev"
mounted "$ROOT/dev/shm" || mount -t tmpfs -o mode=1777,nosuid,nodev shm "$ROOT/dev/shm"
cp /etc/resolv.conf "$ROOT/etc/resolv.conf"

# The recovery userspace creates lo but leaves it down and without an address.
# Native services use loopback so backends are not exposed directly on WiFi.
ip link set lo up 2>/dev/null || ifconfig lo up 2>/dev/null || true
ip addr add 127.0.0.1/8 dev lo 2>/dev/null || \
    ifconfig lo 127.0.0.1 netmask 255.0.0.0 2>/dev/null || true

# Docker is optional until `make docker-setup` installs and validates it.
[ -x "$ROOT/opt/walpad/docker_boot.sh" ] && \
    sh "$ROOT/opt/walpad/docker_boot.sh" >>"$VPS/docker-boot.log" 2>&1

# User-managed services. Keep failures from preventing remote access.
[ -x "$ROOT/opt/walpad/start.sh" ] && \
    chroot "$ROOT" /opt/walpad/start.sh >>"$VPS/services.log" 2>&1 &

if [ -x "$VPS/tailscale/tailscaled" ]; then
    if ! ps | grep -v grep | grep -q '[t]ailscaled'; then
        setsid "$VPS/tailscale/tailscaled" \
            --state="$VPS/tailscale.state" \
            --socket=/tmp/tailscaled.sock \
            --tun=tailscale0 --port=41641 \
            >>"$VPS/tailscaled.log" 2>&1 </dev/null &
        sleep 3
    fi
fi
