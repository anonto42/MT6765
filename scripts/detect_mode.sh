#!/usr/bin/env bash
# detect_mode.sh — watch the WalPad 8G USB mode live (BROM / preloader / WalPad-Server).
# Usage: bash scripts/detect_mode.sh     (Ctrl-C to stop)
# Modes:
#   0e8d:0003  BROM           -> mask ROM, mtkclient can talk + flash
#   0e8d:2000/2001  preloader -> MTK preloader (flashable)
#   0e8d:2008  WalPad-Server  -> custom Linux USB gadget (serial + RNDIS/ECM)
#   (none)     device off / unplugged
# On a flat battery the tablet cycles BROM -> preloader -> server -> off.

prev=""
echo "Watching for MediaTek USB mode changes on the WalPad 8G..."
echo "To force BROM: unplug, hold Vol Up + Power, plug USB, release."
echo
while true; do
  id=$(lsusb | grep -i "0e8d:" | awk '{print $6}' | head -1)
  case "$id" in
    0e8d:0003) mode="BROM (0e8d:0003) - mtkclient can flash" ;;
    0e8d:2008) mode="WalPad-Server (0e8d:2008) - Linux USB gadget" ;;
    0e8d:2000|0e8d:2001|0e8d:20ff|0e8d:3000|0e8d:6000) mode="Preloader ($id) - flashable" ;;
    "") mode="NO DEVICE (off/unplugged)" ;;
    *) mode="other: $id" ;;
  esac
  if [ "$mode" != "$prev" ]; then
    echo "[$(date +%H:%M:%S)] $mode"
    prev="$mode"
  fi
  sleep 1
done