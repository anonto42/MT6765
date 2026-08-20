#!/usr/bin/env python3
"""
device_recon.py — collect everything knowable about the tablet, into data/.

Runs a large catalogue of probes over the serial console and writes one file
per probe under data/recon/<group>/<name>.txt, plus an INDEX.md and a
KEY-FACTS.md summary.

The boot window is short and the battery is weak, so this is built to be
interrupted: every probe is written to disk the moment it completes, and a
re-run skips whatever was already collected. Run it repeatedly across several
windows until INDEX.md reports nothing pending.

Usage:
    scripts/device_recon.py                # collect what's still missing
    scripts/device_recon.py --watch        # keep retrying across windows
    scripts/device_recon.py --fresh        # ignore previous results, redo all
    scripts/device_recon.py --only power storage
    scripts/device_recon.py --list         # show the catalogue, collect nothing
"""
import os
import sys
import time
import argparse

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from serial_probe import Console, WindowClosed, wait_for_window, device_present, repo_root  # noqa: E402

OUT = os.path.join(repo_root(), "data", "recon")

# (group, name, timeout, command)
# Ordered so that if the window dies early, the most decision-relevant facts
# are already on disk.
PROBES = [
    # --- identity: answers "what kernel, what board" -----------------------
    ("identity", "uname",         6,  "uname -a"),
    ("identity", "version",       6,  "cat /proc/version"),
    ("identity", "cmdline",       6,  "cat /proc/cmdline"),
    ("identity", "dt-model",      6,  "cat /sys/firmware/devicetree/base/model 2>/dev/null; echo; "
                                      "cat /sys/firmware/devicetree/base/compatible 2>/dev/null | tr '\\0' '\\n'"),
    ("identity", "hostname",      6,  "hostname 2>/dev/null; cat /proc/sys/kernel/hostname"),
    ("identity", "uptime",        6,  "cat /proc/uptime; uptime 2>/dev/null"),

    # --- power: the dead battery is the project's root blocker -------------
    ("power",    "power-supply",  12, "for f in /sys/class/power_supply/*/uevent; do "
                                      "echo \"== $f\"; cat $f; echo; done"),
    ("power",    "thermal",       10, "for f in /sys/class/thermal/thermal_zone*/; do "
                                      "echo \"$(basename $f) $(cat $f/type 2>/dev/null) $(cat $f/temp 2>/dev/null)\"; done"),
    ("power",    "charger",       10, "ls /sys/class/power_supply/ 2>&1; "
                                      "cat /proc/mtk_battery_cmd/* 2>/dev/null | head -40"),

    # --- storage / partitions ---------------------------------------------
    ("storage",  "partitions",    8,  "cat /proc/partitions"),
    ("storage",  "by-name",       10, "ls -l /dev/block/by-name/ 2>&1"),
    ("storage",  "mounts",        8,  "cat /proc/mounts"),
    ("storage",  "df",            8,  "df -h 2>&1"),
    ("storage",  "filesystems",   6,  "cat /proc/filesystems"),
    ("storage",  "mmc",           10, "for d in /sys/class/mmc_host/*/*/; do echo \"== $d\"; "
                                      "for a in name type cid csd serial manfid oemid fwrev hwrev; do "
                                      "printf '  %-8s %s\\n' $a \"$(cat $d/$a 2>/dev/null)\"; done; done"),
    ("storage",  "sdcard",        10, "ls -la /mnt/ 2>&1; echo '--- sd contents:'; ls -la /mnt/sd/ 2>&1"),

    # --- kernel capability: decides the whole WiFi plan --------------------
    ("kernel",   "config-gz",     25, "(zcat /proc/config.gz 2>/dev/null || cat /proc/config.gz 2>/dev/null) "
                                      "| grep -v '^#' | grep ."),
    ("kernel",   "modules-live",  8,  "cat /proc/modules 2>&1"),
    ("kernel",   "modules-dir",   12, "ls -R /lib/modules 2>&1 | head -60"),
    ("kernel",   "devices",       8,  "cat /proc/devices"),
    ("kernel",   "cpuinfo",       8,  "cat /proc/cpuinfo"),
    ("kernel",   "meminfo",       8,  "cat /proc/meminfo"),
    ("kernel",   "interrupts",    8,  "cat /proc/interrupts"),
    ("kernel",   "iomem",         8,  "cat /proc/iomem 2>&1 | head -60"),
    ("kernel",   "sysclass",      8,  "ls /sys/class/"),
    ("kernel",   "platform-devs", 10, "ls /sys/bus/platform/devices/ 2>&1"),
    ("kernel",   "taint-selinux", 8,  "cat /proc/sys/kernel/tainted 2>/dev/null; "
                                      "getenforce 2>/dev/null; cat /sys/fs/selinux/enforce 2>/dev/null"),

    # --- wifi / bluetooth --------------------------------------------------
    ("wifi",     "chardevs",      8,  "ls -l /dev/wmtWifi /dev/stpwmt /dev/conninfra_dev /dev/wmtdetect "
                                      "/dev/wmtBT 2>&1"),
    ("wifi",     "connsys",       8,  "ls /sys/bus/platform/devices/ 2>/dev/null "
                                      "| grep -iE 'conn|wmt|wifi|wlan|mt66|mt76' || echo '(none)'"),
    ("wifi",     "cfg80211",      8,  "ls -d /sys/module/cfg80211 /sys/module/mac80211 /sys/class/ieee80211/* 2>&1"),
    ("wifi",     "kconfig",       12, "(zcat /proc/config.gz 2>/dev/null || cat /proc/config.gz 2>/dev/null) "
                                      "| grep -iE 'CFG80211|MAC80211|WEXT|MTK_COMBO|WLAN|MT7668|CONNINFRA|WIFI'"),
    ("wifi",     "firmware-path", 8,  "cat /sys/module/firmware_class/parameters/path 2>&1; "
                                      "echo '--- /lib/firmware:'; ls -R /lib/firmware 2>&1 | head -40"),
    ("wifi",     "dmesg-wifi",    10, "dmesg 2>/dev/null | grep -iE 'wlan|wifi|mt66|mt76|conn(sys|infra)|wmt|cfg80211' "
                                      "| tail -40"),

    # --- networking / usb gadget ------------------------------------------
    ("net",      "interfaces",    8,  "ls /sys/class/net/; echo '---'; ip addr 2>/dev/null || ifconfig -a 2>&1"),
    ("net",      "routes",        8,  "ip route 2>/dev/null || route -n 2>&1"),
    ("net",      "gadget",        12, "ls -R /sys/kernel/config/usb_gadget/ 2>&1 | head -60"),
    ("net",      "gadget-udc",    8,  "ls /sys/class/udc/ 2>&1; "
                                      "for f in /sys/class/udc/*/; do echo \"== $f\"; "
                                      "cat $f/state $f/function 2>/dev/null; done"),
    ("net",      "resolv-hosts",  8,  "cat /etc/resolv.conf /etc/hosts 2>&1"),
    ("net",      "listening",     8,  "netstat -tulpn 2>/dev/null || cat /proc/net/tcp 2>&1 | head -20"),

    # --- userspace: what can actually be run on this thing -----------------
    ("userspace","init",          12, "ls -la /; echo '--- /init:'; head -100 /init 2>&1"),
    ("userspace","busybox",       10, "busybox 2>&1 | tail -25"),
    ("userspace","bin",           10, "ls /bin /sbin /usr/bin /usr/sbin 2>&1"),
    ("userspace","tools-check",   10, "for t in iw iwconfig wpa_supplicant wpa_passphrase udhcpc dhcpcd ip "
                                      "ifconfig insmod modprobe dropbear dropbearkey ssh scp nginx smbd "
                                      "tar gzip base64 wget curl openssl python; do "
                                      "command -v $t >/dev/null 2>&1 && echo \"OK   $t\" || echo \"MISS $t\"; done"),
    ("userspace","processes",     8,  "ps 2>&1 | head -40"),
    ("userspace","etc",           10, "ls -la /etc/ 2>&1"),
    ("userspace","libs",          10, "ls /lib /usr/lib 2>&1 | head -40"),

    # --- input / display (is a local console possible?) --------------------
    ("hw",       "input",         10, "ls /sys/class/input/ 2>&1; echo '---'; cat /proc/bus/input/devices 2>&1 | head -40"),
    ("hw",       "display",       8,  "ls /sys/class/graphics/ /sys/class/drm/ /dev/fb* 2>&1"),
    ("hw",       "misc-dev",      8,  "ls /dev/ 2>&1"),
]

GROUP_ORDER = []
for g, _, _, _ in PROBES:
    if g not in GROUP_ORDER:
        GROUP_ORDER.append(g)


def path_for(group, name):
    return os.path.join(OUT, group, name + ".txt")


def already_have(group, name):
    p = path_for(group, name)
    return os.path.isfile(p) and os.path.getsize(p) > 0


def save(group, name, cmd, out, complete):
    p = path_for(group, name)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, "w") as f:
        f.write("# %s/%s   %s\n" % (group, name, time.strftime("%Y-%m-%d %H:%M:%S")))
        f.write("# $ %s\n" % cmd)
        if not complete:
            f.write("# NOTE: probe timed out - output may be truncated\n")
        f.write("\n" + (out if out.strip() else "(no output)") + "\n")


def collect(pending, per_probe_extra=0.0):
    """Run pending probes in one window. Returns number completed."""
    con = Console()
    try:
        if not con.open(wait=True):
            return 0
    except WindowClosed:
        print("\n!! window closed during handshake; re-run to continue")
        return 0
    done = 0
    try:
        for group, name, timeout, cmd in pending:
            try:
                out, ok = con.run(cmd, timeout + per_probe_extra)
            except WindowClosed as e:
                print("\n!! window closed: %s" % e)
                print("   %d probes collected this window; re-run to continue" % done)
                break
            save(group, name, cmd, out, ok)
            done += 1
            flag = "" if ok else "  [timeout, partial]"
            first = (out.strip().split("\n") or [""])[0][:60]
            print("  [%2d/%2d] %-10s %-14s %s%s" % (done, len(pending), group, name, first, flag))
    finally:
        con.close()
    return done


KEY_FACTS = [
    ("kernel version",      "identity", "uname"),
    ("board / SoC",         "identity", "dt-model"),
    ("boot cmdline",        "identity", "cmdline"),
    ("battery",             "power",    "power-supply"),
    ("wifi char devices",   "wifi",     "chardevs"),
    ("cfg80211 present",    "wifi",     "cfg80211"),
    ("loadable modules",    "kernel",   "modules-dir"),
    ("partitions by name",  "storage",  "by-name"),
    ("available tools",     "userspace","tools-check"),
]


def write_index():
    os.makedirs(OUT, exist_ok=True)
    have = [(g, n) for g, n, _, _ in PROBES if already_have(g, n)]
    missing = [(g, n) for g, n, _, _ in PROBES if not already_have(g, n)]

    with open(os.path.join(OUT, "INDEX.md"), "w") as f:
        f.write("# Device recon index\n\n")
        f.write("Generated %s — %d of %d probes collected.\n\n"
                % (time.strftime("%Y-%m-%d %H:%M:%S"), len(have), len(PROBES)))
        if missing:
            f.write("**Still pending (%d):** re-run `make recon` during another boot window.\n\n"
                    % len(missing))
            for g, n in missing:
                f.write("- `%s/%s`\n" % (g, n))
            f.write("\n")
        else:
            f.write("**Collection complete.**\n\n")
        for g in GROUP_ORDER:
            rows = [(gg, nn, cc) for gg, nn, _, cc in PROBES if gg == g]
            if not rows:
                continue
            f.write("## %s\n\n| probe | status | command |\n|---|---|---|\n" % g)
            for gg, nn, cc in rows:
                st = "collected" if already_have(gg, nn) else "pending"
                f.write("| [`%s`](%s/%s.txt) | %s | `%s` |\n"
                        % (nn, gg, nn, st, cc.replace("|", "\\|")[:90]))
            f.write("\n")

    with open(os.path.join(OUT, "KEY-FACTS.md"), "w") as f:
        f.write("# Key facts\n\nPulled from the recon dump — %s\n\n"
                % time.strftime("%Y-%m-%d %H:%M:%S"))
        for label, g, n in KEY_FACTS:
            f.write("## %s\n\n" % label)
            p = path_for(g, n)
            if not os.path.isfile(p):
                f.write("_not collected yet (`%s/%s`)_\n\n" % (g, n))
                continue
            body = open(p).read().split("\n")
            body = [ln for ln in body if not ln.startswith("#")]
            text = "\n".join(body).strip()
            f.write("```\n%s\n```\n\n" % (text[:1500] if text else "(no output)"))
    return len(have), len(missing)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--watch", action="store_true", help="keep retrying across boot windows")
    ap.add_argument("--fresh", action="store_true", help="re-collect everything")
    ap.add_argument("--only", nargs="*", help="limit to these groups")
    ap.add_argument("--list", action="store_true", help="print the catalogue and exit")
    ap.add_argument("--slow", type=float, default=0.0, help="extra seconds per probe")
    args = ap.parse_args()

    if args.list:
        for g in GROUP_ORDER:
            print("== %s" % g)
            for gg, nn, t, cc in PROBES:
                if gg == g:
                    print("   %-14s (%ss)  %s" % (nn, t, cc[:70]))
        return 0

    sel = [p for p in PROBES if not args.only or p[0] in args.only]
    if args.fresh:
        pending = sel
    else:
        pending = [p for p in sel if not already_have(p[0], p[1])]

    if not pending:
        have, miss = write_index()
        print("nothing pending - %d probes already collected" % have)
        print("index: %s" % os.path.join(OUT, "INDEX.md"))
        return 0

    print("%d probes to collect -> %s" % (len(pending), OUT))
    while True:
        collect(pending, args.slow)
        have, miss = write_index()
        pending = [p for p in sel if not already_have(p[0], p[1])]
        print("\ncollected %d/%d; %d pending" % (have, len(PROBES), len(pending)))
        print("index: %s/INDEX.md   summary: %s/KEY-FACTS.md" % (OUT, OUT))
        if not pending:
            print("recon complete")
            return 0
        if not args.watch:
            print("re-run to collect the rest (or use --watch)")
            return 0
        print("waiting for the next window...")
        while device_present():
            time.sleep(1)
        if not wait_for_window(300):
            return 1


if __name__ == "__main__":
    sys.exit(main())
