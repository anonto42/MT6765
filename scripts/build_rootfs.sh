#!/usr/bin/env bash
# Build a minimal Alpine aarch64 root filesystem image for the tablet's userdata partition.
#   scripts/build_rootfs.sh [size-in-MiB]   ->  build/rootfs/rootfs.ext4
#   WALPAD_NO_PYTHON=1 scripts/build_rootfs.sh   ->  build/rootfs-nopython/rootfs.ext4 (card M7.1):
#     no python3/tk, no .py runtime, Rust-only runtime.conf, padui rescue instead of the Tk launcher
#     (config/rootfs-nopython/).  Default (unset or 0) is the unchanged Python-default image.
# No arm64 emulation needed: apk installs foreign-arch packages into a directory with --no-scripts
# (busybox applet links are created on the device by /sbin/walpad-init).  Heavy packages (X11,
# Firefox, ...) are installed later on the device itself, once Wi-Fi works.
# Reproducibility (card M9.3): the Alpine release is pinned in ONE place (ALPINE_REL below; it names both
# the container image tag and the apk repository path).  Next to the image the build writes
#   packages.txt      `apk info -v | sort` of the installed packages (name-version)
#   files.txt         every path in the tree
#   config-hashes.txt sha256 + mode + path of every file the repo owns (config/rootfs, the no-Python
#                     overlay, tools/ binaries, Rust binaries), as installed in the image
#   release.txt       release, container image digest, apk-tools version
#   image.sha256      sha256 of rootfs.ext4 (informational: see docs/rust/reports/M9.1-M9.3.md)
# Two clean builds with the same inputs give identical packages.txt and config-hashes.txt.  The
# package versions themselves still move when Alpine publishes a point update inside the release.
# tools/propshim (libwpdprops.so) is NOT an input of this image: it only serves the legacy SD-card
# Wi-Fi flow, so it is not built here (see the report).
# Everything runs inside an x86_64 Alpine container (files are root-owned there); the final image
# is chowned back to the calling user.
set -euo pipefail
cd "$(dirname "$0")/.."
ALPINE_REL=3.24   # the only place that names the Alpine release (image tag + repository path)
SIZE_MIB=${1:-auto}
NO_PY=${WALPAD_NO_PYTHON:-0}
case $NO_PY in 0|1) ;; *) echo "WALPAD_NO_PYTHON must be 0 or 1" >&2; exit 2;; esac
# the no-Python image never defaults to the directory that holds the flashed image
OUT=${WALPAD_ROOTFS_OUT:-build/rootfs}; [ "$NO_PY" = 1 ] && OUT=${WALPAD_ROOTFS_OUT:-build/rootfs-nopython}
BASE_PKGS="alpine-baselayout alpine-keys apk-tools busybox busybox-extras musl-utils libc-utils dropbear dropbear-ssh \
      wpa_supplicant iw wireless-regdb e2fsprogs e2fsprogs-extra util-linux-misc ca-certificates-bundle tzdata \
      iproute2 procps curl nano htop kbd fuse-exfat ntfs-3g alsa-utils hostapd iptables \
      bluez bluez-deprecated dbus pulseaudio pulseaudio-utils"
      bluez bluez-deprecated dbus \
      iptables-legacy wireguard-go wireguard-tools-wg openvpn"
GUI_PKGS="xvfb libxtst py3-xlib xset xsetroot xrandr xinput setxkbmap xdotool xrdb xclip xkbcomp \
      xkeyboard-config fontconfig font-misc-misc font-cursor-misc \
      openbox matchbox-keyboard st xterm mesa-dri-gallium font-dejavu font-noto-emoji \
      python3 python3-tkinter tk alsa-utils alsa-lib firefox-esr pcmanfm adwaita-icon-theme"
# No-Python image: python3 (+tkinter/tk/xlib) only served the .py runtime.  xdotool was used only by
# walpad-keys.py/walpad-devd.py (the Rust core injects keys itself over XTEST).  Kept on purpose:
# matchbox-keyboard and alsa-utils (amixer), which walpad-core spawns; xset/xsetroot (session);
# st, firefox-esr (apps.json); xkbcomp/xrdb/xclip/xrandr/xinput/setxkbmap (X tooling, no gain dropping).
if [ "$NO_PY" = 1 ]; then
  KEEP=""; for p in $GUI_PKGS; do
    case $p in python3|python3-tkinter|tk|py3-xlib|xdotool) ;; *) KEEP="$KEEP $p" ;; esac
  done; GUI_PKGS=$KEEP
fi
PKGS="$BASE_PKGS"; [ "${WALPAD_GUI:-1}" = "1" ] && PKGS="$BASE_PKGS $GUI_PKGS"
[ -z "${WALPAD_PRINT_PKGS:-}" ] || { echo "$PKGS"; exit 0; }   # dry run for tests/shell/run.sh
mkdir -p "$OUT" build/bt data/camera   # build/bt/hci_vhci.ko comes from scripts/build_bt_module.sh (optional)
# host-built static tools that go into the image (gitignored binaries): build them if missing
for t in dmlinear padui fbmirror; do
  [ -x "tools/$t/$t" ] || { echo "building tools/$t"; bash "tools/$t/build.sh" >&2; }
done
# camacdk (bionic helper for walpad-camera) is optional: the image still builds without it
[ -x data/camera/camacdk ] || bash tools/camacdk/build.sh >&2 || echo "camacdk not built: walpad-camera will report the helper as missing" >&2
ALPINE_DIGEST=$(docker image inspect --format '{{index .RepoDigests 0}}' "alpine:$ALPINE_REL" 2>/dev/null || true)
[ -n "$ALPINE_DIGEST" ] || { docker pull -q "alpine:$ALPINE_REL" >/dev/null; ALPINE_DIGEST=$(docker image inspect --format '{{index .RepoDigests 0}}' "alpine:$ALPINE_REL" 2>/dev/null || echo unknown); }
AUTH=""; [ -f "$HOME/.ssh/id_ed25519.pub" ] && AUTH="$(cat "$HOME/.ssh/id_ed25519.pub")"
docker run --rm --platform linux/amd64 -v "$PWD/$OUT":/out -v "$PWD/config/rootfs":/cfg:ro -v "$PWD/tools":/tools:ro -v "$PWD/config/rootfs-nopython":/cfg-np:ro -v "$PWD/build/rust/aarch64":/rust:ro -v "$PWD/build/bt":/bt:ro -v "$PWD/data/camera":/camera:ro \
  -e ALPINE_REL="$ALPINE_REL" -e ALPINE_DIGEST="$ALPINE_DIGEST" -e SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1700000000}" -e NO_PY="$NO_PY" -e SIZE_MIB="$SIZE_MIB" -e PKGS="$PKGS" -e WALPAD_GUI="${WALPAD_GUI:-1}" -e AUTH="$AUTH" -e UID_="$(id -u)" -e GID_="$(id -g)" \
  "alpine:$ALPINE_REL" sh -euc '
  REL=$(cut -d. -f1,2 /etc/alpine-release)
  [ "$REL" = "$ALPINE_REL" ] || { echo "container is Alpine $REL, expected pinned $ALPINE_REL"; exit 1; }
  R=https://dl-cdn.alpinelinux.org/alpine/v$REL
  rm -rf /out/root /out/rootfs.ext4; mkdir -p /out/root
  for try in 1 2 3 4 5 6; do
    apk add --root /out/root --arch aarch64 --initdb --no-scripts --keys-dir /usr/share/apk/keys/aarch64 \
        -X $R/main -X $R/community $PKGS && break
    echo "apk failed (attempt $try), retrying in 15 s"; sleep 15
    [ $try = 6 ] && exit 1
  done
  printf "%s\n%s\n" "$R/main" "$R/community" > /out/root/etc/apk/repositories
  { echo "alpine v$REL aarch64"; echo "image $ALPINE_DIGEST"; echo "apk-tools $(apk --version | cut -d" " -f2 | tr -d ,)"; } > /out/release.txt
  mkdir -p /out/root/sbin /out/root/root/.ssh /out/root/run /out/root/proc /out/root/sys /out/root/dev /out/root/tmp
  echo walpad > /out/root/etc/hostname
  printf "nameserver 1.1.1.1\nnameserver 8.8.8.8\n" > /out/root/etc/resolv.conf
  cp -a /cfg/. /out/root/
  find /out/root -name __pycache__ -type d -prune -exec rm -rf {} + 2>/dev/null || true   # never ship stray bytecode
  install -m 755 /tools/dmlinear/dmlinear /out/root/usr/local/bin/dmlinear
  install -m 755 /tools/padui/padui /out/root/usr/local/bin/padui
  install -m 755 /tools/fbmirror/fbmirror /out/root/usr/local/bin/fbmirror
  # Rust binaries (additions only; runtime.conf keeps Python as the default).  Skipped if not built.
  if [ -f /rust/walpad-core ]; then
    REL=/out/root/usr/local/lib/walpad/image
    mkdir -p $REL && install -m 755 /rust/walpad-core /rust/walpad-shell /rust/walpad-ctl $REL/
    ln -sfn image /out/root/usr/local/lib/walpad/current
    ln -sfn ../lib/walpad/current/walpad-core /out/root/usr/local/bin/walpad-core
    ln -sfn ../lib/walpad/current/walpad-shell /out/root/usr/local/bin/walpad-shell
    ln -sfn ../lib/walpad/current/walpad-ctl /out/root/usr/local/bin/walpad-ctl-rs
    if [ -f /rust/walpad-btbridge ]; then
      install -m 755 /rust/walpad-btbridge $REL/
      ln -sfn ../lib/walpad/current/walpad-btbridge /out/root/usr/local/bin/walpad-btbridge
    fi
  fi
  # camera: the ACDK helper, run through the Android linker by walpad-camera
  [ -f /camera/camacdk ] && install -m 755 /camera/camacdk /out/root/usr/local/bin/camacdk
  # Bluetooth: our hci_vhci.ko for the running 4.19.127 kernel (scripts/build_bt_module.sh), and the
  # D-Bus system user (apk runs with --no-scripts, so the dbus package did not create it)
  if [ -f /bt/hci_vhci.ko ]; then mkdir -p /out/root/usr/local/lib/modules && install -m 644 /bt/hci_vhci.ko /out/root/usr/local/lib/modules/; fi
  grep -q "^messagebus:" /out/root/etc/group || echo "messagebus:x:101:messagebus" >> /out/root/etc/group
  grep -q "^messagebus:" /out/root/etc/passwd || echo "messagebus:x:100:101:messagebus:/run/dbus:/sbin/nologin" >> /out/root/etc/passwd
  grep -q "^messagebus:" /out/root/etc/shadow 2>/dev/null || echo "messagebus:!::0:::::" >> /out/root/etc/shadow
  # Sound server (walpad-sound): pulseaudio --system drops to the pulse user, which apk (--no-scripts) did
  # not create; it needs the audio group for /dev/snd.  NOT pulseaudio-alsa / alsa-plugins-pulse: they
  # would redirect ALSA `default` into pulse while the pulse sink opens ALSA (a loop).
  grep -q "^pulse:" /out/root/etc/group || echo "pulse:x:102:" >> /out/root/etc/group
  grep -q "^pulse-access:" /out/root/etc/group || echo "pulse-access:x:103:" >> /out/root/etc/group
  grep -q "^pulse:" /out/root/etc/passwd || echo "pulse:x:102:102:PulseAudio:/var/lib/pulse:/sbin/nologin" >> /out/root/etc/passwd
  grep -q "^pulse:" /out/root/etc/shadow 2>/dev/null || echo "pulse:!::0:::::" >> /out/root/etc/shadow
  awk -F: -v OFS=: "\$1==\"audio\" && \$4 !~ /(^|,)pulse(,|\$)/ { \$4 = (\$4 == \"\" ? \"pulse\" : \$4 \",pulse\") } { print }" /out/root/etc/group > /out/group.new && cat /out/group.new > /out/root/etc/group && rm -f /out/group.new
  # no-Python image: delete the .py runtime, add the Rust-only session/init/runtime.conf, assert no Python is left
  [ "$NO_PY" = 1 ] && WALPAD_CFG_DIR=/cfg sh /cfg-np/apply.sh /out/root
  chmod 755 /out/root/sbin/walpad-init /out/root/usr/local/sbin/* /out/root/usr/local/bin/*
  if [ "$NO_PY" = 1 ]; then
    ! ls /out/root/usr/bin/python* /out/root/usr/bin/wish* > /dev/null 2>&1 || { echo "no-python build: python/tk binary present"; exit 1; }
    ! apk info --root /out/root 2>/dev/null | grep -E "^(python3|py3-|tk|tcl)" || { echo "no-python build: python/tk package installed"; exit 1; }
  fi
  apk info --root /out/root -v 2>/dev/null | LC_ALL=C sort > /out/packages.txt || true
  [ -s /out/packages.txt ] || { echo "empty package manifest"; exit 1; }
  (cd /out/root && find . | LC_ALL=C sort) > /out/files.txt
  # sha256 + mode + path of everything the repo owns, as it ended up in the image (before the per-host ssh key)
  { (cd /cfg && find . -type f); [ "$NO_PY" = 1 ] && (cd /cfg-np/files && find . -type f)
    echo ./usr/local/bin/dmlinear; echo ./usr/local/bin/padui; echo ./usr/local/bin/fbmirror
    [ -f /rust/walpad-core ] && for b in walpad-core walpad-shell walpad-ctl walpad-btbridge; do echo ./usr/local/lib/walpad/image/$b; done
    echo ./usr/local/lib/modules/hci_vhci.ko; echo ./usr/local/bin/camacdk
    true; } | LC_ALL=C sort -u | while read -r f; do
      [ -f "/out/root/$f" ] || continue
      echo "$(sha256sum "/out/root/$f" | cut -d" " -f1) $(stat -c %a "/out/root/$f") $f"
    done > /out/config-hashes.txt
  [ -s /out/config-hashes.txt ] || { echo "empty config hash listing"; exit 1; }
  [ -n "$AUTH" ] && { echo "$AUTH" > /out/root/root/.ssh/authorized_keys; chmod 700 /out/root/root; chmod 600 /out/root/root/.ssh/authorized_keys; }
  for try in 1 2 3 4 5; do apk add --no-cache e2fsprogs >/dev/null && break; sleep 10; done
  # 4.19 does not know the newest ext4 features (orphan_file, ...): keep to the classic set
  if [ "$SIZE_MIB" = "auto" ]; then USED=$(du -sm /out/root | cut -f1); SIZE_MIB=$(( USED * 13 / 10 + 400 )); [ $SIZE_MIB -lt 1536 ] && SIZE_MIB=1536; fi
  find /out/root -exec touch -h -d @$SOURCE_DATE_EPOCH {} +   # same mtimes in every build
  # fixed UUID/hash seed/time: no random or wall-clock input of our own in the image
  E2FSPROGS_FAKE_TIME=$SOURCE_DATE_EPOCH mke2fs -q -t ext4 -U 0f1e2d3c-4b5a-4968-8776-655443322110 -E hash_seed=0f1e2d3c-4b5a-4968-8776-655443322110  -b 4096 -L walpad-root -O ^orphan_file,^metadata_csum_seed -d /out/root /out/rootfs.ext4 ${SIZE_MIB}M
  echo "image size: ${SIZE_MIB} MiB"
  echo "files: $(du -sh /out/root | cut -f1)"
  chown -R $UID_:$GID_ /out'
(cd "$OUT" && sha256sum rootfs.ext4 > image.sha256)
echo "built $OUT/rootfs.ext4 ($(head -n 1 "$OUT/release.txt"))"
