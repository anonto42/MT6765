#!/bin/sh
# mtk_stack_up.sh — run ON THE TABLET.  Brings the MediaTek CONNSYS WiFi stack
# up from a cold boot, as far as wlan0.  Association is wifi_up.sh's job.
#
# ORDER IS LOAD-BEARING.  wmt_launcher must be listening on /dev/stpwmt before
# anything writes to /dev/wmtWifi: on power-on the kernel asks *userspace* for
# the ROM patch, and a request nobody answers wedges the driver's single
# command buffer permanently —
#     wmt_ctrl_ul_cmd: cmd buf is occupied by (srh_rom_patch)
# There is no software recovery from that.  `rmmod wmt_drv` deadlocks in the
# kernel and takes the serial console down with it; only a power-cycle helps.
#
# The vendor blobs are Android bionic binaries, so they are invoked through the
# bootstrap linker (the normal /system/bin/linker64 is an APEX symlink that
# cannot resolve without the runtime APEX mounted).
#
# Usage: sh /mnt/sd/mtk_stack_up.sh

SD=/mnt/sd
SYS=/system/system
LINKER=$SYS/bin/bootstrap/linker64
MODDIR=/vendor/lib/modules
export LD_LIBRARY_PATH=$SYS/lib64/bootstrap:/vendor/lib64:$SYS/lib64
# Neither loader can reach a property service — there is no Android init here —
# so wmt_loader cannot publish the chip id and wmt_launcher spins forever
# waiting to read it.  This shim answers both with a flat file.  See
# tools/propshim/wpdprops.c.
export LD_PRELOAD=$SD/wifi/libwpdprops.so

say() { echo "[mtk] $*"; }
die() { echo "[mtk] !! $*"; exit 1; }
mounted() { grep -q " $1 " /proc/mounts; }

# --- 1. storage ------------------------------------------------------------
mkdir -p $SD
# the SD controller enumerates a few seconds into boot, and this script can be
# fired the instant the console appears, so wait for the node rather than fail
i=0
while [ ! -b /dev/mmcblk1p1 ] && [ $i -lt 30 ]; do sleep 1; i=$((i+1)); done
mounted $SD || mount /dev/mmcblk1p1 $SD || die "no SD card at /dev/mmcblk1p1"

for p in vendor system; do
    mounted /$p && continue
    [ -f $SD/$p.img ] || die "$SD/$p.img missing — run scripts/vendor_up.py first"
    mkdir -p /$p
    L=$(losetup -f) && losetup -r $L $SD/$p.img && mount -t ext4 -o ro $L /$p \
        || die "could not mount $SD/$p.img at /$p"
    say "mounted /$p"
done

[ -x $LINKER ] || die "bootstrap linker missing at $LINKER"

# --- 2. kernel modules, part 1 ---------------------------------------------
# Only wmt_drv here.  The wifi modules must not be loaded until wmt_loader has
# run -- see stage 3a.
insmod_once() {
    grep -q "^$1 " /proc/modules && return 0
    insmod $MODDIR/$1.ko || die "insmod $1 failed"
    say "insmod $1"
}
insmod_once wmt_drv

# --- 3. chip detect: creates /dev/stpwmt and starts the mtk_wmtd thread -----
# Keyed on the shim's store rather than on /dev/stpwmt: the node can already
# exist from a run that happened before the shim was in place, and in that case
# the chip id was never published and the launcher would still hang.
if ! grep -q chipid /tmp/wpd-props 2>/dev/null; then
    # exits non-zero because it cannot reach Android's property service; the
    # ioctl it exists for has already happened by then, so the status is noise.
    [ -f $SD/wifi/libwpdprops.so ] || die "property shim missing at $SD/wifi/libwpdprops.so"
    $LINKER /vendor/bin/wmt_loader >/tmp/wmt_loader.log 2>&1
    # the node lands a beat after the ioctl returns, so give devtmpfs a moment
    i=0
    while [ ! -c /dev/stpwmt ] && [ $i -lt 20 ]; do sleep 1; i=$((i+1)); done
    [ -c /dev/stpwmt ] || die "wmt_loader did not create /dev/stpwmt"
    say "chip detected, /dev/stpwmt up"
fi

# --- 3a. kernel modules, part 2 --------------------------------------------
# Order copied from /vendor/etc/init/init.wlan_drv.rc:
#
#     on property:vendor.connsys.driver.ready=yes
#       insmod wmt_chrdev_wifi.ko
#       insmod wlan_drv_${ro.vendor.wlan.gen}.ko
#
# Android holds these back until wmt_loader has published driver.ready, because
# only then has wmt_drv stored the CONNSYS EMI physical base.  Loading
# wlan_drv_gen4m before that leaves it with an unset base and the firmware
# download dies at
#     wlanDownloadEMISection: Consys emi memory address gConEmiPhyBaseFinal invalid
# followed by a FW_START that never gets a response.
insmod_once wmt_chrdev_wifi
insmod_once wlan_drv_gen4m

# --- 4. patch server -- MUST be up before the first /dev/wmtWifi write ------
# pidof is no help here: the process is exec'd through the linker, so its comm
# is "linker64" and only the argv tail names the launcher.
launcher_up() { ps | grep -v grep | grep -q '[w]mt_launcher'; }

# A launcher that never got the chip id sits in a sleep loop with /dev/stpwmt
# unopened.  It looks alive to ps but will never answer a patch request, so
# treat "running but not serving" as not running.
launcher_serving() {
    for p in $(ps | grep -v grep | grep '[w]mt_launcher' | awk '{print $1}'); do
        ls -l /proc/$p/fd 2>/dev/null | grep -q stpwmt && return 0
    done
    return 1
}

if launcher_up && ! launcher_serving; then
    say "killing a wedged wmt_launcher"
    kill $(ps | grep -v grep | grep '[w]mt_launcher' | awk '{print $1}') 2>/dev/null
    sleep 1
fi

if ! launcher_serving; then
    # Arguments copied verbatim from /vendor/etc/init/init_connectivity.rc:
    #     service wmt_launcher /vendor/bin/wmt_launcher -p /vendor/firmware/ -o 1
    # The trailing slash matters -- the launcher concatenates path + filename.
    setsid $LINKER /vendor/bin/wmt_launcher -p /vendor/firmware/ -o 1 \
        >/tmp/wmt_launcher.log 2>&1 </dev/null &
    sleep 3
    launcher_serving || die "wmt_launcher started but never opened /dev/stpwmt"
    say "wmt_launcher serving patches from /vendor/firmware"
fi

# --- 5. power on ------------------------------------------------------------
# Set MTK_NO_POWERON=1 to stop here.  A failed power-on wedges the driver's
# command buffer until the next reboot, so anything that just needs /vendor
# mounted should not risk it.
if [ "${MTK_NO_POWERON:-0}" = "1" ]; then
    say "stopping before power-on (MTK_NO_POWERON=1)"
    exit 0
fi

# There is no ueventd here, so nothing turns the driver's class uevent into a
# device node -- and a plain `echo 1 > /dev/wmtWifi` then CREATES it as a regular
# file, which swallows every write silently (the write succeeds, `cat` reads back
# what you wrote, and the driver never sees a thing).  Build the real node from
# the major:minor the driver publishes in sysfs; the major is dynamic.
DEVNUM=$(cat /sys/class/wmtWifi/wmtWifi/dev 2>/dev/null)
[ -n "$DEVNUM" ] || die "wmt_chrdev_wifi did not register /sys/class/wmtWifi"
if [ ! -c /dev/wmtWifi ]; then
    rm -f /dev/wmtWifi
    mknod /dev/wmtWifi c "${DEVNUM%%:*}" "${DEVNUM##*:}" \
        || die "could not create /dev/wmtWifi ($DEVNUM)"
    chmod 600 /dev/wmtWifi
    say "created /dev/wmtWifi c $DEVNUM"
fi

if [ ! -d /sys/class/net/wlan0 ]; then
    # Capture the FIRST attempt's trace.  Once an attempt fails the chip latches
    # into "chip reset and chip no response" and every later trace is just noise,
    # so the only informative log is the one from a virgin power-on.
    dmesg -c > /dev/null 2>&1
    if echo 1 > /dev/wmtWifi 2>/tmp/wmton.err; then
        sleep 5
    else
        sleep 3
        dmesg > /tmp/wmton.dmesg 2>/dev/null
        say "---- first power-on trace ----"
        grep -iE '\[wlan\]|WMT-CORE|WMT-IC|fw_dl|RAM_CODE|patch' /tmp/wmton.dmesg \
            | grep -viE 'mtk_stp_btm|ActiveOpQ' | head -60
        say "---- end trace (full copy in /tmp/wmton.dmesg) ----"
        die "WMT turn-on failed"
    fi
fi

[ -d /sys/class/net/wlan0 ] || die "no wlan0 after power-on"
say "wlan0 is up"
ip link set wlan0 up 2>/dev/null || ifconfig wlan0 up
