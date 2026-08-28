#!/bin/bash
# Set root's password interactively, then persist only /etc/shadow on the SD.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
IP=${1:-}
[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)

echo "Setting the root SSH password on WalPad at $IP."
echo "The password is entered directly into the tablet's passwd program."
ssh -F /dev/null -t -o BatchMode=yes "root@$IP" passwd

ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    set -e
    mkdir -p /mnt/sd/wifi/ssh
    cp /etc/shadow /mnt/sd/wifi/ssh/shadow
    sync
    test -s /mnt/sd/wifi/ssh/shadow
'
echo "Password hash saved persistently on the SD card."
echo "Password login: ssh root@$IP"
