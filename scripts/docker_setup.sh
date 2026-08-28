#!/bin/bash
# Install and test the restricted Docker profile supported by the tablet kernel.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
IP=${WPD_IP:-}
[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)

tar -C "$ROOT/scripts" -cf - docker_boot.sh vps_boot.sh \
    | ssh -F /dev/null -o BatchMode=yes "root@$IP" '
        mkdir -p /tmp/walpad-docker-setup
        tar -C /tmp/walpad-docker-setup -xf -
        cp /tmp/walpad-docker-setup/docker_boot.sh /mnt/vps/opt/walpad/docker_boot.sh
        cp /tmp/walpad-docker-setup/vps_boot.sh /mnt/sd/vps/boot.sh
        chmod 755 /mnt/vps/opt/walpad/docker_boot.sh
        chmod 755 /mnt/sd/vps/boot.sh
    '

set +e
ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    set -e
    chroot /mnt/vps /sbin/apk add docker docker-cli-compose iptables
    mkdir -p /mnt/vps/etc/docker
    cat > /mnt/vps/etc/docker/daemon.json <<"EOF"
{
  "data-root": "/var/lib/docker",
  "storage-driver": "overlay2",
  "bridge": "none",
  "iptables": false,
  "ip6tables": false,
  "ip-forward": false,
  "ip-masq": false,
  "exec-opts": ["native.cgroupdriver=cgroupfs"],
  "log-driver": "json-file",
  "log-opts": {"max-size": "10m", "max-file": "3"}
}
EOF
    if ! awk '\''$1 == "devices" && $4 == 1 { found=1 } END { exit !found }'\'' /proc/cgroups; then
        mkdir -p /mnt/vps/var/lib/walpad
        echo "BLOCKED: kernel lacks CONFIG_CGROUP_DEVICE; Docker packages and configuration are installed." \
            | tee /mnt/vps/var/lib/walpad/docker-blocked
        echo "Next device-side stage: build and safely boot a compatible kernel."
        exit 78
    fi
    sh /mnt/vps/opt/walpad/docker_boot.sh
    chroot /mnt/vps /usr/bin/docker info
    chroot /mnt/vps /usr/bin/docker run --rm --network=host --ipc=host \
        --cgroupns=host alpine:3.22 echo WALPAD_DOCKER_OK
'
rc=$?
set -e

if [ "$rc" -eq 78 ]; then
    echo "Docker setup is staged, but activation is waiting for a compatible kernel."
    exit 0
fi
[ "$rc" -eq 0 ] || exit "$rc"

echo "Docker kernel compatibility test passed."
