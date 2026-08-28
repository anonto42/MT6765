#!/bin/bash
# Add another device's public key using a computer that already has access.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
KEY_FILE=${1:-}
IP=${2:-}

if [ -z "$KEY_FILE" ] || [ ! -f "$KEY_FILE" ]; then
    echo "usage: $0 /path/to/id_ed25519.pub [tablet-ip]" >&2
    exit 2
fi
grep -Eq '^ssh-(ed25519|rsa|ecdsa-[^ ]+) [A-Za-z0-9+/=]+([[:space:]]|$)' "$KEY_FILE" \
    || { echo "not a valid OpenSSH public-key file: $KEY_FILE" >&2; exit 2; }

if [ -z "$IP" ]; then
    IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)
fi

cat "$KEY_FILE" | ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    mkdir -p /mnt/sd/wifi/ssh /root/.ssh
    cat >> /mnt/sd/wifi/ssh/authorized_keys
    cp /mnt/sd/wifi/ssh/authorized_keys /root/.ssh/authorized_keys
    chown -R 0:0 /root
    chmod 700 /root /root/.ssh
    chmod 600 /root/.ssh/authorized_keys
'
echo "Key installed persistently. From the other device: ssh root@$IP"
