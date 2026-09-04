#!/usr/bin/env bash
# boot_kernel.sh — RAM-boot the stock kernel via fastboot (does NOT flash).
# The device must already be in fastboot mode (Vol-Down + Power, or via LK).
# Uses platform-tools fastboot; falls back to PATH fastboot.
set -eu
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [ -x "$repo/tools/platform-tools/fastboot" ]; then
  FB="$repo/tools/platform-tools/fastboot"
elif command -v fastboot >/dev/null 2>&1; then
  FB="fastboot"
elif command -v nix >/dev/null 2>&1; then
  # Keep the host setup sudo-free; Nix supplies a current Android fastboot.
  exec nix shell nixpkgs#android-tools -c "$0" "$@"
else
  FB="${FASTBOOT:-fastboot}"
fi

IMG="${1:-$repo/firmware/MT6765/boot_a.img}"
if [ ! -f "$IMG" ]; then
  echo "boot image not found: $IMG" >&2
  echo "usage: $0 [path/to/boot.img]" >&2
  exit 1
fi

echo "Waiting for device in fastboot mode..."
"$FB" wait-for-device
echo "RAM-booting $IMG (device will not be modified)..."
"$FB" boot "$IMG"
