#!/bin/bash
# Find this WalPad on the local /24 by its persistent SSH host key.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
KEY_FILE="$ROOT/data/wifi/tablet-host-key.pub"
IP_ONLY=0

if [ "${1:-}" = "--ip-only" ]; then IP_ONLY=1; shift; fi
PREFIX=${1:-}

if [ ! -s "$KEY_FILE" ]; then
    echo "missing tablet identity: $KEY_FILE" >&2
    exit 2
fi
EXPECTED=$(awk '{print $1 " " $2; exit}' "$KEY_FILE")

if [ -z "$PREFIX" ]; then
    SRC=$(ip -4 route get 1.1.1.1 2>/dev/null | sed -n 's/.* src \([0-9.]*\).*/\1/p' | head -1)
    [ -n "$SRC" ] || { echo "cannot determine this device's LAN address" >&2; exit 2; }
    PREFIX=${SRC%.*}
fi
PREFIX=${PREFIX%.}

case "$PREFIX" in
    *.*.*) ;;
    *) echo "usage: $0 [--ip-only] [192.168.0]" >&2; exit 2 ;;
esac

TMP=$(mktemp -d /tmp/walpad-find.XXXXXX) || exit 2
trap 'rm -rf "$TMP"' EXIT INT TERM

# Run in small batches: a dead address otherwise costs one timeout each.
for start in $(seq 1 32 254); do
    end=$((start + 31)); [ "$end" -gt 254 ] && end=254
    for n in $(seq "$start" "$end"); do
        ip="$PREFIX.$n"
        (
            got=$(ssh-keyscan -T 1 -t ed25519 "$ip" 2>/dev/null \
                | awk '$2 == "ssh-ed25519" {print $2 " " $3; exit}')
            [ "$got" = "$EXPECTED" ] && printf '%s\n' "$ip" > "$TMP/$n"
        ) &
    done
    wait
done

FOUND=$(cat "$TMP"/* 2>/dev/null | head -1)
if [ -z "$FOUND" ]; then
    [ "$IP_ONLY" -eq 1 ] || echo "WalPad is not reachable on $PREFIX.0/24"
    exit 1
fi

if [ "$IP_ONLY" -eq 1 ]; then
    echo "$FOUND"
else
    echo "WalPad is online at $FOUND"
    echo "ssh root@$FOUND"
fi
