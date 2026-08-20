# Device recon index

Generated 2026-08-19 00:12:42 — 49 of 49 probes collected.

**Collection complete.**

## identity

| probe | status | command |
|---|---|---|
| [`uname`](identity/uname.txt) | collected | `uname -a` |
| [`version`](identity/version.txt) | collected | `cat /proc/version` |
| [`cmdline`](identity/cmdline.txt) | collected | `cat /proc/cmdline` |
| [`dt-model`](identity/dt-model.txt) | collected | `cat /sys/firmware/devicetree/base/model 2>/dev/null; echo; cat /sys/firmware/devicetree/ba` |
| [`hostname`](identity/hostname.txt) | collected | `hostname 2>/dev/null; cat /proc/sys/kernel/hostname` |
| [`uptime`](identity/uptime.txt) | collected | `cat /proc/uptime; uptime 2>/dev/null` |

## power

| probe | status | command |
|---|---|---|
| [`power-supply`](power/power-supply.txt) | collected | `for f in /sys/class/power_supply/*/uevent; do echo "== $f"; cat $f; echo; done` |
| [`thermal`](power/thermal.txt) | collected | `for f in /sys/class/thermal/thermal_zone*/; do echo "$(basename $f) $(cat $f/type 2>/dev/n` |
| [`charger`](power/charger.txt) | collected | `ls /sys/class/power_supply/ 2>&1; cat /proc/mtk_battery_cmd/* 2>/dev/null \| head -40` |

## storage

| probe | status | command |
|---|---|---|
| [`partitions`](storage/partitions.txt) | collected | `cat /proc/partitions` |
| [`by-name`](storage/by-name.txt) | collected | `ls -l /dev/block/by-name/ 2>&1` |
| [`mounts`](storage/mounts.txt) | collected | `cat /proc/mounts` |
| [`df`](storage/df.txt) | collected | `df -h 2>&1` |
| [`filesystems`](storage/filesystems.txt) | collected | `cat /proc/filesystems` |
| [`mmc`](storage/mmc.txt) | collected | `for d in /sys/class/mmc_host/*/*/; do echo "== $d"; for a in name type cid csd serial manf` |
| [`sdcard`](storage/sdcard.txt) | collected | `ls -la /mnt/ 2>&1; echo '--- sd contents:'; ls -la /mnt/sd/ 2>&1` |

## kernel

| probe | status | command |
|---|---|---|
| [`config-gz`](kernel/config-gz.txt) | collected | `(zcat /proc/config.gz 2>/dev/null \|\| cat /proc/config.gz 2>/dev/null) \| grep -v '^#' \|` |
| [`modules-live`](kernel/modules-live.txt) | collected | `cat /proc/modules 2>&1` |
| [`modules-dir`](kernel/modules-dir.txt) | collected | `ls -R /lib/modules 2>&1 \| head -60` |
| [`devices`](kernel/devices.txt) | collected | `cat /proc/devices` |
| [`cpuinfo`](kernel/cpuinfo.txt) | collected | `cat /proc/cpuinfo` |
| [`meminfo`](kernel/meminfo.txt) | collected | `cat /proc/meminfo` |
| [`interrupts`](kernel/interrupts.txt) | collected | `cat /proc/interrupts` |
| [`iomem`](kernel/iomem.txt) | collected | `cat /proc/iomem 2>&1 \| head -60` |
| [`sysclass`](kernel/sysclass.txt) | collected | `ls /sys/class/` |
| [`platform-devs`](kernel/platform-devs.txt) | collected | `ls /sys/bus/platform/devices/ 2>&1` |
| [`taint-selinux`](kernel/taint-selinux.txt) | collected | `cat /proc/sys/kernel/tainted 2>/dev/null; getenforce 2>/dev/null; cat /sys/fs/selinux/enfo` |

## wifi

| probe | status | command |
|---|---|---|
| [`chardevs`](wifi/chardevs.txt) | collected | `ls -l /dev/wmtWifi /dev/stpwmt /dev/conninfra_dev /dev/wmtdetect /dev/wmtBT 2>&1` |
| [`connsys`](wifi/connsys.txt) | collected | `ls /sys/bus/platform/devices/ 2>/dev/null \| grep -iE 'conn\|wmt\|wifi\|wlan\|mt66\|mt76' ` |
| [`cfg80211`](wifi/cfg80211.txt) | collected | `ls -d /sys/module/cfg80211 /sys/module/mac80211 /sys/class/ieee80211/* 2>&1` |
| [`kconfig`](wifi/kconfig.txt) | collected | `(zcat /proc/config.gz 2>/dev/null \|\| cat /proc/config.gz 2>/dev/null) \| grep -iE 'CFG80` |
| [`firmware-path`](wifi/firmware-path.txt) | collected | `cat /sys/module/firmware_class/parameters/path 2>&1; echo '--- /lib/firmware:'; ls -R /lib` |
| [`dmesg-wifi`](wifi/dmesg-wifi.txt) | collected | `dmesg 2>/dev/null \| grep -iE 'wlan\|wifi\|mt66\|mt76\|conn(sys\|infra)\|wmt\|cfg80211' \|` |

## net

| probe | status | command |
|---|---|---|
| [`interfaces`](net/interfaces.txt) | collected | `ls /sys/class/net/; echo '---'; ip addr 2>/dev/null \|\| ifconfig -a 2>&1` |
| [`routes`](net/routes.txt) | collected | `ip route 2>/dev/null \|\| route -n 2>&1` |
| [`gadget`](net/gadget.txt) | collected | `ls -R /sys/kernel/config/usb_gadget/ 2>&1 \| head -60` |
| [`gadget-udc`](net/gadget-udc.txt) | collected | `ls /sys/class/udc/ 2>&1; for f in /sys/class/udc/*/; do echo "== $f"; cat $f/state $f/func` |
| [`resolv-hosts`](net/resolv-hosts.txt) | collected | `cat /etc/resolv.conf /etc/hosts 2>&1` |
| [`listening`](net/listening.txt) | collected | `netstat -tulpn 2>/dev/null \|\| cat /proc/net/tcp 2>&1 \| head -20` |

## userspace

| probe | status | command |
|---|---|---|
| [`init`](userspace/init.txt) | collected | `ls -la /; echo '--- /init:'; head -100 /init 2>&1` |
| [`busybox`](userspace/busybox.txt) | collected | `busybox 2>&1 \| tail -25` |
| [`bin`](userspace/bin.txt) | collected | `ls /bin /sbin /usr/bin /usr/sbin 2>&1` |
| [`tools-check`](userspace/tools-check.txt) | collected | `for t in iw iwconfig wpa_supplicant wpa_passphrase udhcpc dhcpcd ip ifconfig insmod modpro` |
| [`processes`](userspace/processes.txt) | collected | `ps 2>&1 \| head -40` |
| [`etc`](userspace/etc.txt) | collected | `ls -la /etc/ 2>&1` |
| [`libs`](userspace/libs.txt) | collected | `ls /lib /usr/lib 2>&1 \| head -40` |

## hw

| probe | status | command |
|---|---|---|
| [`input`](hw/input.txt) | collected | `ls /sys/class/input/ 2>&1; echo '---'; cat /proc/bus/input/devices 2>&1 \| head -40` |
| [`display`](hw/display.txt) | collected | `ls /sys/class/graphics/ /sys/class/drm/ /dev/fb* 2>&1` |
| [`misc-dev`](hw/misc-dev.txt) | collected | `ls /dev/ 2>&1` |

