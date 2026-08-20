#!/usr/bin/env bash
# charge_keeper.sh — hold the WalPad 8G in a charging state (KPOC / preloader
# trickle) and report status, for how ever long you want.
#
# The flat-battery blocker means the tablet cannot complete a normal boot.  While
# it cycles BROM -> preloader -> WalPad-Server -> off, plugging it to a strong
# supply (or a kernel in Power-Off-Charging mode) slowly raises the battery.
# This script just babysits: it logs the mode transitions and their timestamps.
#
# Usage:
#   bash scripts/charge_keeper.sh [minutes]     # default: 60
#   bash scripts/charge_keeper.sh 3600          # keep watching for an hour
set -eu
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
minutes="${1:-60}"
end=$(( $(date +%s) + minutes * 60 ))

prev=""
while [ "$(date +%s)" -lt "$end" ]; do
  id=$(lsusb | grep -i "0e8d:" | awk '{print $6}' | head -1)
  case "$id" in
    0e8d:0003) mode="BROM" ;;
    0e8d:2008) mode="WalPad-Server" ;;
    0e8d:2000|0e8d:2001|0e8d:20ff|0e8d:3000|0e8d:6000) mode="Preloader($id)" ;;
    "") mode="off/unplugged" ;;
    *) mode="other:$id" ;;
  esac
  if [ "$mode" != "$prev" ]; then
    echo "[$(date +%F_%H:%M:%S)] $mode"
    prev="$mode"
  fi
  sleep 5
done
echo "charge_keeper done after ${minutes}m"