#!/bin/sh
# harvest_wifi_fw.sh — run ON THE TABLET (via serial console).
#
# Pulls the WiFi driver module + firmware blobs off the stock Android vendor
# partition onto the SD card, so the server ramdisk can load them.
#
# Why this works: only /data is FBE-encrypted on this device. /vendor and
# /system are plain ext4 and mount read-only just fine from the ramdisk.
#
# Usage: sh /mnt/sd/harvest_wifi_fw.sh

SD=/mnt/sd
OUT=$SD/wifi
V=/mnt/vendor

set -u
mkdir -p "$OUT/firmware" "$OUT/modules" "$OUT/bin" "$V"

echo "=== WalPad 8G WiFi harvest ==="

# --- locate the vendor partition -------------------------------------------
VDEV=""
for cand in /dev/block/by-name/vendor_a /dev/block/by-name/vendor \
            /dev/block/by-name/vendor_b /dev/block/by-name/super; do
    [ -e "$cand" ] && VDEV="$cand" && break
done

if [ -z "$VDEV" ]; then
    echo "!! no vendor partition found under /dev/block/by-name/"
    echo "   available:"; ls /dev/block/by-name/ 2>/dev/null
    echo "   -> dump it from the host instead (see docs/10-wifi-plan.md)"
    exit 1
fi
echo "vendor device: $VDEV"

# super = dynamic partitions (Android 10+); vendor lives inside as an LV.
case "$VDEV" in
    */super)
        echo "!! this is a 'super' (dynamic partition) image — the ramdisk cannot"
        echo "   map logical partitions without dmsetup/lpdump."
        echo "   -> dump 'super' from the host and extract vendor there:"
        echo "      sudo python3 scripts/instant_catch.py r super super.img"
        exit 2
        ;;
esac

mount -t ext4 -o ro "$VDEV" "$V" 2>/dev/null || mount -o ro "$VDEV" "$V"
if [ $? -ne 0 ]; then
    echo "!! mount failed (ext4 support present? check /proc/filesystems)"
    exit 3
fi
echo "mounted $VDEV -> $V"

# --- firmware ---------------------------------------------------------------
echo "[1/3] copying firmware blobs"
for d in "$V/firmware" "$V/etc/firmware" "$V/lib/firmware"; do
    [ -d "$d" ] || continue
    echo "  from $d"
    # WiFi/BT connectivity blobs for MTK CONNSYS / MT76xx combo parts
    find "$d" \( -iname 'WIFI_RAM_CODE*' -o -iname '*mt66*' -o -iname '*mt76*' \
              -o -iname 'wifi*.bin' -o -iname '*.cfg' -o -iname 'WIFI_*' \
              -o -iname 'soc*_ram_code*' -o -iname 'BT_RAM_CODE*' \) \
        -exec cp -f {} "$OUT/firmware/" \; 2>/dev/null
done
echo "  -> $(ls "$OUT/firmware" 2>/dev/null | wc -l) files"

# --- kernel modules ---------------------------------------------------------
echo "[2/3] copying kernel modules"
for d in "$V/lib/modules" "$V/modules"; do
    [ -d "$d" ] || continue
    echo "  from $d"
    find "$d" -name '*.ko' -exec cp -f {} "$OUT/modules/" \; 2>/dev/null
done
echo "  -> $(ls "$OUT/modules" 2>/dev/null | wc -l) modules"
ls "$OUT/modules" 2>/dev/null | grep -iE 'wlan|wmt|conn|wifi' || echo "  (no obviously-wifi module names)"

# --- vendor userspace helpers (reference only; bionic-linked) ---------------
echo "[3/3] copying vendor wifi helpers (reference)"
for b in wmt_loader stp_uart_launcher wpa_supplicant hostapd; do
    for p in "$V/bin/$b" "$V/bin/hw/$b" "$V/xbin/$b"; do
        [ -f "$p" ] && cp -f "$p" "$OUT/bin/" 2>/dev/null && echo "  $p"
    done
done

# --- record the exact kernel version the modules were built for ------------
{
    echo "harvested: $(date 2>/dev/null)"
    echo "vendor dev: $VDEV"
    echo "running kernel: $(uname -r)"
    echo "--- module vermagic ---"
    for m in "$OUT"/modules/*.ko; do
        [ -f "$m" ] || continue
        echo "$(basename "$m"): $(strings "$m" 2>/dev/null | grep -m1 vermagic)"
    done
} > "$OUT/HARVEST.txt" 2>/dev/null
cat "$OUT/HARVEST.txt"

umount "$V" 2>/dev/null
sync
echo ""
echo "=== done. contents now on the SD card at $OUT ==="
echo "Next: compare 'running kernel' vs module vermagic above."
echo "  same  -> modules will insmod; run  sh /mnt/sd/wifi_up.sh"
echo "  differ -> driver must be built into the server kernel instead"
