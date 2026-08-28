#!/bin/sh
# Start Docker in the Alpine chroot using only capabilities this Android kernel has.
ROOT=/mnt/vps
LOG=$ROOT/var/log/dockerd.log

mountpoint() { grep -q " $1 " /proc/mounts; }

mkdir -p /sys/fs/cgroup
mountpoint /sys/fs/cgroup || mount -t tmpfs -o mode=755 cgroup_root /sys/fs/cgroup

mount_controller() {
    name=$1 opts=$2
    dir=/sys/fs/cgroup/$name
    mkdir -p "$dir"
    mountpoint "$dir" || mount -t cgroup -o "$opts" "$name" "$dir" 2>/dev/null || true
}

mount_controller cpuset cpuset
mount_controller cpu cpu,cpuacct
mount_controller memory memory
mount_controller blkio blkio
mount_controller freezer freezer
mount_controller schedtune schedtune

# Moby/runc require the devices controller to enforce the OCI device allowlist.
# The stock WalPad kernel was built without CONFIG_CGROUP_DEVICE.  Do not leave
# a doomed dockerd running at every boot; a compatible kernel must be installed
# before this script can safely continue.
if ! awk '$1 == "devices" && $4 == 1 { found=1 } END { exit !found }' /proc/cgroups; then
    mkdir -p "$ROOT/var/lib/walpad"
    printf '%s\n' 'BLOCKED: kernel lacks CONFIG_CGROUP_DEVICE; Docker was not started.' \
        >"$ROOT/var/lib/walpad/docker-blocked"
    printf '%s\n' 'Docker blocked: devices cgroup controller is unavailable.' >>"$LOG"
    exit 78
fi

rm -f "$ROOT/var/lib/walpad/docker-blocked"

mkdir -p "$ROOT/etc/docker" "$ROOT/var/lib/docker" "$ROOT/var/log" "$ROOT/run"
[ -f "$ROOT/etc/docker/daemon.json" ] || exit 1

if ! ps | grep -v grep | grep -q '[d]ockerd'; then
    setsid chroot "$ROOT" /usr/bin/dockerd --config-file=/etc/docker/daemon.json \
        >>"$LOG" 2>&1 </dev/null &
fi

i=0
while [ "$i" -lt 30 ]; do
    [ -S "$ROOT/run/docker.sock" ] && exit 0
    sleep 1
    i=$((i + 1))
done
exit 1
