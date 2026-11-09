#!/usr/bin/env bash
# Put ONE Wi-Fi network into a built rootfs image WITHOUT rebuilding it, so the tablet joins Wi-Fi (and
# SSH works) on its first boot after flashing.  The passphrase is read from stdin (never from argv), turned
# into the 64-hex WPA2 key here, and only the derived key is written into the image.
#   printf '%s\n' 'PASSPHRASE' | scripts/image_set_wifi.sh build/rootfs-candidate/rootfs.ext4 'SSID'
# Never modifies build/rootfs/rootfs.ext4 (the last flashed image: your rollback copy).
set -euo pipefail
cd "$(dirname "$0")/.."
IMG=${1:?usage: printf '%s\n' PASSPHRASE | $0 <rootfs.ext4> <SSID>}; SSID=${2:?SSID}
[ -f "$IMG" ] && [ ! -L "$IMG" ] || { echo "not a regular file: $IMG" >&2; exit 2; }
[ -b "$IMG" ] && { echo "refusing a block device" >&2; exit 2; }
if [ "$(realpath "$IMG")" = "$(realpath build/rootfs/rootfs.ext4 2>/dev/null || echo /nonexistent)" ]; then
  echo "refusing to modify build/rootfs/rootfs.ext4: it is the last flashed image (your rollback copy)" >&2; exit 2
fi
for t in debugfs e2fsck python3; do command -v $t >/dev/null || { echo "missing tool: $t" >&2; exit 2; }; done
IFS= read -r PASS || true
[ "${#PASS}" -ge 8 ] && [ "${#PASS}" -le 63 ] || { echo "passphrase must be 8-63 characters (read from stdin)" >&2; exit 2; }
[ "${#SSID}" -ge 1 ] && [ "${#SSID}" -le 32 ] || { echo "SSID must be 1-32 bytes" >&2; exit 2; }

umask 077
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
# derive the WPA2 key and write the config; the passphrase goes through stdin only
printf '%s' "$PASS" | python3 -I -c '
import sys, hashlib
ssid = sys.argv[1].encode()
pw = sys.stdin.buffer.read()
key = hashlib.pbkdf2_hmac("sha1", pw, ssid, 4096, 32).hex()
sys.stdout.write("ctrl_interface=/run/wpa_supplicant\nctrl_interface_group=0\nupdate_config=1\ncountry=US\n\nnetwork={\n\tssid=%s\n\t%s=%s\n\tkey_mgmt=WPA-PSK\n}\n" % (ssid.hex(), "psk", key))
' "$SSID" > "$TMP/wpa.conf"

# Two separate debugfs sessions: removing and re-creating the same name in ONE session leaves the
# free-space summary inconsistent (e2fsck complains); separate sessions are clean.
P=/etc/wpa_supplicant/wpa_supplicant.conf
# (debugfs `mkdir` on an existing directory corrupts it: only create it when it is really missing)
if debugfs -R "stat /etc/wpa_supplicant" "$IMG" 2>/dev/null | grep -q "Type: directory"; then
  printf '%s\n' "rm $P" > "$TMP/cmds1"
else
  printf '%s\n' "mkdir /etc/wpa_supplicant" > "$TMP/cmds1"
fi
debugfs -w -f "$TMP/cmds1" "$IMG" > "$TMP/out1" 2>&1 || true
printf '%s\n' "write $TMP/wpa.conf $P" "sif $P mode 0100600" "sif $P uid 0" "sif $P gid 0" > "$TMP/cmds2"
debugfs -w -f "$TMP/cmds2" "$IMG" > "$TMP/out2" 2>&1 || true

# verify: the file is there, has the SSID (hex) and a 64-hex key, and does NOT contain the passphrase
debugfs -R "cat /etc/wpa_supplicant/wpa_supplicant.conf" "$IMG" > "$TMP/check" 2>/dev/null
HEX=$(printf '%s' "$SSID" | od -An -tx1 | tr -d ' \n')
grep -q "ssid=$HEX" "$TMP/check" || { echo "FAILED: SSID not found in the image's wpa_supplicant.conf" >&2; exit 1; }
grep -Eq '^	psk=[0-9a-f]{64}$' "$TMP/check" || { echo "FAILED: derived key not found in the image" >&2; exit 1; }
if grep -qF -- "$PASS" "$TMP/check"; then echo "FAILED: the passphrase itself is in the image config" >&2; exit 1; fi
MODE=$(debugfs -R "stat /etc/wpa_supplicant/wpa_supplicant.conf" "$IMG" 2>/dev/null | sed -n 's/^.*Mode: *\([0-7]*\).*$/\1/p' | head -1)
[ "$MODE" = "0600" ] || { echo "FAILED: config mode is $MODE, expected 0600" >&2; exit 1; }
e2fsck -fn "$IMG" > "$TMP/fsck" 2>&1 || { echo "FAILED: filesystem check after patching:" >&2; tail -5 "$TMP/fsck" >&2; exit 1; }
echo "OK: '$SSID' added to $IMG (WPA2 key derived; passphrase not stored; mode 0600; filesystem clean)"
