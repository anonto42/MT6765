#!/bin/sh
# Idempotent on-tablet provisioning called by scripts/vps_setup.sh.
set -e

SD=/mnt/sd
VPS=$SD/vps
ROOT=/mnt/vps
SIZE_MB=${VPS_SIZE_MB:-3072}
ALPINE_TAR=$VPS/alpine-minirootfs-aarch64.tar.gz

mkdir -p "$VPS" "$ROOT"
[ -s "$ALPINE_TAR" ] || { echo "missing $ALPINE_TAR"; exit 1; }

if [ ! -f "$VPS/use-internal-userdata" ]; then
    if [ ! -s "$VPS/rootfs.ext4" ]; then
        echo "Creating ${SIZE_MB} MiB ext4 VPS disk (one-time)..."
        dd if=/dev/zero of="$VPS/rootfs.ext4" bs=1M count="$SIZE_MB"
    fi
    if [ ! -f "$VPS/rootfs.formatted" ]; then
        echo "Formatting VPS disk as ext4..."
        mke2fs -F -L walpad-vps -m 1 "$VPS/rootfs.ext4"
        touch "$VPS/rootfs.formatted"
    fi
fi

sh "$VPS/boot.sh"

if [ ! -f "$ROOT/etc/alpine-release" ]; then
    echo "Extracting Alpine root filesystem..."
    tar -xzf "$ALPINE_TAR" -C "$ROOT"
fi

mkdir -p "$ROOT/opt/walpad" "$ROOT/etc/apk"
cat > "$ROOT/etc/apk/repositories" <<'EOF'
https://dl-cdn.alpinelinux.org/alpine/v3.24/main
https://dl-cdn.alpinelinux.org/alpine/v3.24/community
EOF
cp /etc/resolv.conf "$ROOT/etc/resolv.conf"

cat > "$ROOT/opt/walpad/start.sh" <<'EOF'
#!/bin/sh
# Add persistent application startup commands below. Use absolute paths.
# Example: /usr/bin/python3 /srv/app/server.py >>/var/log/app.log 2>&1 &
exit 0
EOF
chmod 755 "$ROOT/opt/walpad/start.sh"

echo "Installing the Alpine VPS baseline..."
chroot "$ROOT" /sbin/apk update
chroot "$ROOT" /sbin/apk add ca-certificates bash curl git nano openssh-client \
    python3 py3-pip nodejs npm tmux docker docker-cli-compose iptables

echo "Alpine $(cat "$ROOT/etc/alpine-release") is ready at $ROOT"
echo "Enter it from the host with: make vps-shell"
