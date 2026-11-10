#!/bin/sh
# Deploy Rust binaries to the tablet without reflashing (card M0.2).
#   deploy.sh [--dry-run] [--host H] [--restart core|shell|none] [FILE...]
#   deploy.sh --rollback [--host H] [--restart ...]
# Default FILEs: build/rust/aarch64/walpad-*.  Hashes are verified on the tablet.
# Test mode: --local runs the "remote" side on this machine; with --base/--bin
# pointing at a scratch dir nothing real is touched.
set -eu
cd "$(dirname "$0")/.."

DRY=0 ROLLBACK=0 LOCAL=0 HOST="" RESTART=none
BASE=/usr/local/lib/walpad BIN=/usr/local/bin
while [ $# -gt 0 ]; do
  case $1 in
    --dry-run) DRY=1 ;;
    --rollback) ROLLBACK=1 ;;
    --local) LOCAL=1 ;;
    --host) HOST=$2; shift ;;
    --base) BASE=$2; shift ;;
    --bin) BIN=$2; shift ;;
    --restart) RESTART=$2; shift ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    --) shift; break ;;
    -*) echo "unknown option $1" >&2; exit 2 ;;
    *) break ;;
  esac
  shift
done
case $RESTART in core|shell|none) ;; *) echo "--restart core|shell|none" >&2; exit 2 ;; esac

# WALPAD_SSH_EXTRA adds ssh options, e.g. "-o UserKnownHostsFile=build/ssh/known_hosts" (each flash makes a new host key).
SSH_OPTS="-o ConnectTimeout=8 -o ServerAliveInterval=5 -o ServerAliveCountMax=3 -o BatchMode=yes ${WALPAD_SSH_EXTRA:-}"
if [ $LOCAL -eq 0 ]; then
  [ -n "$HOST" ] || HOST=$(bash scripts/wifi_find.sh --ip-only) || { echo "no tablet found; use --host" >&2; exit 1; }
fi
rsh() { if [ $LOCAL -eq 1 ]; then sh -c "$1"; else ssh $SSH_OPTS "root@$HOST" "$1"; fi; }
rput() { # rput local remote
  if [ $LOCAL -eq 1 ]; then cp "$1" "$2"; else ssh $SSH_OPTS "root@$HOST" "cat > '$2'" < "$1"; fi; }

do_restart() {
  case $RESTART in
    core)  rsh "pkill -x walpad-core || true" ;;   # init loop restarts it
    shell) rsh "$BIN/walpad-ctl restart_shell" ;;
    none)  ;;
  esac
}

if [ $ROLLBACK -eq 1 ]; then
  echo "rollback: swap current <-> previous in $BASE"
  [ $DRY -eq 1 ] && { echo "(dry run)"; exit 0; }
  rsh "set -e; cd '$BASE'; [ -L previous ] || { echo 'no previous release' >&2; exit 1; };
       c=\$(readlink current); p=\$(readlink previous);
       ln -sfn \"\$p\" current; ln -sfn \"\$c\" previous; echo \"current -> \$p\""
  do_restart; exit 0
fi

[ $# -gt 0 ] || set -- build/rust/aarch64/walpad-*
for f in "$@"; do [ -f "$f" ] || { echo "missing file: $f" >&2; exit 1; }; done
SHA=$(git rev-parse --short HEAD)
git diff --quiet HEAD -- 2>/dev/null || SHA="$SHA-dirty"
REL="$BASE/$SHA"
echo "release $SHA -> $REL ($( [ $LOCAL -eq 1 ] && echo local || echo "$HOST" ))"
SUMS=$(for f in "$@"; do (cd "$(dirname "$f")" && sha256sum "$(basename "$f")"); done)
echo "$SUMS"
[ $DRY -eq 1 ] && { echo "(dry run: nothing copied)"; exit 0; }

rsh "mkdir -p '$REL' '$BIN'"
for f in "$@"; do rput "$f" "$REL/$(basename "$f").new"; done
# verify hashes on the tablet before anything becomes live
# (a here-document, not a pipe: `exit` inside a piped loop would only leave the subshell)
while read -r h n; do
  got=$(rsh "sha256sum '$REL/$n.new'" | cut -d' ' -f1)
  [ "$got" = "$h" ] || { echo "HASH MISMATCH for $n: nothing made live" >&2; exit 1; }
done <<EOF
$(printf '%s\n' "$SUMS" | sed 's/  / /')
EOF
rsh "set -e; cd '$REL'; for n in *.new; do chmod 755 \"\$n\"; mv \"\$n\" \"\${n%.new}\"; done
     # a partial deploy (one FILE) keeps the other binaries of the live release
     if [ -d '$BASE/current' ] && [ \"\$(readlink '$BASE/current')\" != '$REL' ]; then for b in '$BASE'/current/walpad-*; do
       [ -f \"\$b\" ] && [ ! -e \"'$REL'/\$(basename \"\$b\")\" ] && cp -p \"\$b\" '$REL'/; done; fi
     cd '$BASE'
     if [ -L current ] && [ \"\$(readlink current)\" != '$REL' ]; then ln -sfn \"\$(readlink current)\" previous; fi
     ln -sfn '$REL' current
     for n in \$(ls '$REL'); do ln -sfn '$BASE/current/'\"\$n\" '$BIN/'\"\$n\"; done
     echo deployed $SHA"
do_restart
