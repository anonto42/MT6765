#!/usr/bin/env bash
# detect_mode.sh / probe_server.py companion.
# Reusable USB-mode helpers for the Makefile and scripts.
set -u

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mtk_venv="$repo/tools/mtk/.venv/bin/python"

usb_mode() {
  id=$(lsusb | grep -i "0e8d:" | awk '{print $6}' | head -1)
  case "$id" in
    0e8d:0003) echo "brom" ;;
    0e8d:2008) echo "server" ;;
    0e8d:2000|0e8d:2001|0e8d:20ff|0e8d:3000|0e8d:6000) echo "preloader" ;;
    *) echo "none" ;;
  esac
}

mtk_py() {
  "$mtk_venv" "$repo/tools/mtk/mtk.py" "$@"
}

"${1:-}" "${@:2}"