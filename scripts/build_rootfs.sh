#!/usr/bin/env bash
# Build a minimal Alpine aarch64 root filesystem image for the tablet's userdata partition.
#   scripts/build_rootfs.sh [size-in-MiB]   ->  build/rootfs/rootfs.ext4
#   WALPAD_NO_PYTHON=1 scripts/build_rootfs.sh   ->  build/rootfs-nopython/rootfs.ext4 (card M7.1):
#     no python3/tk, no .py runtime, Rust-only runtime.conf, padui rescue instead of the Tk launcher
#     (config/rootfs-nopython/).  Default (unset or 0) is the unchanged Python-default image.
# No arm64 emulation needed: apk installs foreign-arch packages into a directory with --no-scripts
# (busybox applet links are created on the device by /sbin/walpad-init).  Heavy packages (X11,
# Firefox, ...) are installed later on the device itself, once Wi-Fi works.
# Everything runs inside an x86_64 Alpine container (files are root-owned there); the final image
# is chowned back to the calling user.
set -euo pipefail
cd "$(dirname "$0")/.."
SIZE_MIB=${1:-auto}
NO_PY=${WALPAD_NO_PYTHON:-0}
case $NO_PY in 0|1) ;; *) echo "WALPAD_NO_PYTHON must be 0 or 1" >&2; exit 2;; esac
# the no-Python image never defaults to the directory that holds the flashed image
OUT=${WALPAD_ROOTFS_OUT:-build/rootfs}; [ "$NO_PY" = 1 ] && OUT=${WALPAD_ROOTFS_OUT:-build/rootfs-nopython}
BASE_PKGS="alpine-baselayout alpine-keys apk-tools busybox busybox-extras musl-utils libc-utils dropbear dropbear-ssh \
      wpa_supplicant iw wireless-regdb e2fsprogs e2fsprogs-extra util-linux-misc ca-certificates-bundle tzdata \
      iproute2 procps curl nano htop kbd"
GUI_PKGS="xvfb libxtst py3-xlib xset xsetroot xrandr xinput setxkbmap xdotool xrdb xclip xkbcomp \
      xkeyboard-config fontconfig font-misc-misc font-cursor-misc \
      openbox matchbox-keyboard st xterm mesa-dri-gallium font-dejavu font-noto-emoji \
      python3 python3-tkinter tk alsa-utils alsa-lib firefox-esr"
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
mkdir -p "$OUT"
AUTH=""; [ -f "$HOME/.ssh/id_ed25519.pub" ] && AUTH="$(cat "$HOME/.ssh/id_ed25519.pub")"
docker run --rm --platform linux/amd64 -v "$PWD/$OUT":/out -v "$PWD/config/rootfs":/cfg:ro -v "$PWD/tools":/tools:ro -v "$PWD/config/rootfs-nopython":/cfg-np:ro -v "$PWD/build/rust/aarch64":/rust:ro \
  -e NO_PY="$NO_PY" -e SIZE_MIB="$SIZE_MIB" -e PKGS="$PKGS" -e WALPAD_GUI="${WALPAD_GUI:-1}" -e AUTH="$AUTH" -e UID_="$(id -u)" -e GID_="$(id -g)" \
  alpine:latest sh -euc '
  REL=$(cut -d. -f1,2 /etc/alpine-release); R=https://dl-cdn.alpinelinux.org/alpine/v$REL
  rm -rf /out/root /out/rootfs.ext4; mkdir -p /out/root
  for try in 1 2 3 4 5 6; do
    apk add --root /out/root --arch aarch64 --initdb --no-scripts --keys-dir /usr/share/apk/keys/aarch64 \
        -X $R/main -X $R/community $PKGS && break
    echo "apk failed (attempt $try), retrying in 15 s"; sleep 15
    [ $try = 6 ] && exit 1
  done
  printf "%s\n%s\n" "$R/main" "$R/community" > /out/root/etc/apk/repositories
  echo "alpine v$REL aarch64" > /out/release.txt
  mkdir -p /out/root/sbin /out/root/root/.ssh /out/root/run /out/root/proc /out/root/sys /out/root/dev /out/root/tmp
  echo walpad > /out/root/etc/hostname
  printf "nameserver 1.1.1.1\nnameserver 8.8.8.8\n" > /out/root/etc/resolv.conf
  cp -a /cfg/. /out/root/
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
  fi
  # no-Python image: delete the .py runtime, add the Rust-only session/init/runtime.conf, assert no Python is left
  [ "$NO_PY" = 1 ] && WALPAD_CFG_DIR=/cfg sh /cfg-np/apply.sh /out/root
  chmod 755 /out/root/sbin/walpad-init /out/root/usr/local/sbin/* /out/root/usr/local/bin/*
  if [ "$NO_PY" = 1 ]; then
    ! ls /out/root/usr/bin/python* /out/root/usr/bin/wish* > /dev/null 2>&1 || { echo "no-python build: python/tk binary present"; exit 1; }
    ! apk info --root /out/root 2>/dev/null | grep -E "^(python3|py3-|tk|tcl)" || { echo "no-python build: python/tk package installed"; exit 1; }
  fi
  apk info --root /out/root -v 2>/dev/null | sort > /out/packages.txt || true
  (cd /out/root && find . | sort) > /out/files.txt
  [ -n "$AUTH" ] && { echo "$AUTH" > /out/root/root/.ssh/authorized_keys; chmod 700 /out/root/root; chmod 600 /out/root/root/.ssh/authorized_keys; }
  for try in 1 2 3 4 5; do apk add --no-cache e2fsprogs >/dev/null && break; sleep 10; done
  # 4.19 does not know the newest ext4 features (orphan_file, ...): keep to the classic set
  if [ "$SIZE_MIB" = "auto" ]; then USED=$(du -sm /out/root | cut -f1); SIZE_MIB=$(( USED * 13 / 10 + 400 )); [ $SIZE_MIB -lt 1536 ] && SIZE_MIB=1536; fi
  mke2fs -q -t ext4 -b 4096 -L walpad-root -O ^orphan_file,^metadata_csum_seed -d /out/root /out/rootfs.ext4 ${SIZE_MIB}M
  echo "image size: ${SIZE_MIB} MiB"
  echo "files: $(du -sh /out/root | cut -f1)"
  chown -R $UID_:$GID_ /out'
echo "built $OUT/rootfs.ext4 ($(cat "$OUT/release.txt"))"
