#!/bin/bash
# Host orchestrator: download verified ARM64 payloads and provision the tablet.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
CACHE="$ROOT/data/vps"
ALPINE_VER=${ALPINE_VER:-3.24.1}
TAILSCALE_VER=${TAILSCALE_VER:-1.102.3}
ALPINE="alpine-minirootfs-${ALPINE_VER}-aarch64.tar.gz"
TAILSCALE="tailscale_${TAILSCALE_VER}_arm64.tgz"
ALPINE_URL="https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/aarch64/$ALPINE"
TAILSCALE_URL="https://pkgs.tailscale.com/stable/$TAILSCALE"
IP=${WPD_IP:-}

mkdir -p "$CACHE"
fetch() {
    local url=$1 out=$2
    [ -s "$out" ] || curl -fL --retry 3 -o "$out" "$url"
    curl -fL --retry 3 -o "$out.sha256" "$url.sha256"
    expected=$(awk '{print $1; exit}' "$out.sha256")
    actual=$(sha256sum "$out" | awk '{print $1}')
    [ "$actual" = "$expected" ] \
        || { echo "checksum mismatch for $out" >&2; exit 1; }
    echo "$(basename "$out"): OK"
}
fetch "$ALPINE_URL" "$CACHE/$ALPINE"
fetch "$TAILSCALE_URL" "$CACHE/$TAILSCALE"

[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)
echo "Provisioning WalPad at $IP"

# The server ramdisk has no time service and this tablet booted with its RTC in
# 2010. TLS correctly rejects current Alpine/Tailscale certificates until the
# clock is sane. Seed it from the provisioning host and persist to the RTC.
NOW=$(date +%s)
ssh -F /dev/null -o BatchMode=yes "root@$IP" \
    "date -s '@$NOW'; hwclock -w 2>/dev/null || true"

tar -C "$ROOT/scripts" -cf - vps_boot.sh vps_setup_device.sh wifi_assoc.sh wifi_watchdog.sh \
    | ssh -F /dev/null -o BatchMode=yes "root@$IP" '
        mkdir -p /mnt/sd/vps
        tar -C /mnt/sd/vps -xf -
        chmod 755 /mnt/sd/vps/vps_boot.sh /mnt/sd/vps/vps_setup_device.sh
        cp /mnt/sd/vps/vps_boot.sh /mnt/sd/vps/boot.sh
        cp /mnt/sd/vps/wifi_assoc.sh /mnt/sd/wifi_assoc.sh
    '

tar -C "$CACHE" -cf - "$ALPINE" "$TAILSCALE" \
    | ssh -F /dev/null -o BatchMode=yes "root@$IP" '
        tar -C /mnt/sd/vps -xf -
        mv /mnt/sd/vps/alpine-minirootfs-*-aarch64.tar.gz \
           /mnt/sd/vps/alpine-minirootfs-aarch64.tar.gz
        mkdir -p /mnt/sd/vps/tailscale
        tar -xzf /mnt/sd/vps/tailscale_*_arm64.tgz -C /mnt/sd/vps
        cp /mnt/sd/vps/tailscale_*_arm64/tailscale* /mnt/sd/vps/tailscale/
        chmod 755 /mnt/sd/vps/tailscale/tailscale*
    '

ssh -F /dev/null -o BatchMode=yes "root@$IP" \
    "VPS_SIZE_MB=${VPS_SIZE_MB:-3072} sh /mnt/sd/vps/vps_setup_device.sh"

echo
echo "Starting Tailscale. Open the authentication URL it prints:"
ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    sh /mnt/sd/vps/boot.sh
    /mnt/sd/vps/tailscale/tailscale --socket=/tmp/tailscaled.sock \
        up --hostname=walpad --netfilter-mode=off
'
