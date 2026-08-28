#!/bin/sh
# Minimal persistent service supervisor for the WalPad Alpine chroot.
set -u

CONF_DIR=/etc/walpad/services.d
RUN_DIR=/run/walpad-services
LOG_DIR=/var/log/walpad-services
mkdir -p "$CONF_DIR" "$RUN_DIR" "$LOG_DIR"

names() {
    for conf in "$CONF_DIR"/*.conf; do
        [ -f "$conf" ] && basename "$conf" .conf
    done
}

load() {
    NAME=$1
    COMMAND=
    WORKDIR=/
    # Service files are root-owned deployment configuration.
    . "$CONF_DIR/$NAME.conf"
    [ -n "$COMMAND" ] || { echo "$NAME: COMMAND is missing" >&2; return 2; }
}

running() {
    [ -s "$RUN_DIR/$1.pid" ] || return 1
    pid=$(cat "$RUN_DIR/$1.pid")
    kill -0 "$pid" 2>/dev/null
}

start_one() {
    load "$1" || return
    if running "$NAME"; then echo "$NAME: already running"; return; fi
    rm -f "$RUN_DIR/$NAME.pid"
    (
        cd "$WORKDIR" || exit 1
        exec setsid sh -c "exec $COMMAND" >>"$LOG_DIR/$NAME.log" 2>&1
    ) </dev/null &
    pid=$!
    echo "$pid" >"$RUN_DIR/$NAME.pid"
    sleep 1
    if running "$NAME"; then echo "$NAME: started (pid $pid)"
    else echo "$NAME: failed; see $LOG_DIR/$NAME.log" >&2; return 1
    fi
}

stop_one() {
    load "$1" || return
    if ! running "$NAME"; then rm -f "$RUN_DIR/$NAME.pid"; echo "$NAME: stopped"; return; fi
    pid=$(cat "$RUN_DIR/$NAME.pid")
    kill -TERM "-$pid" 2>/dev/null || kill -TERM "$pid" 2>/dev/null || true
    i=0
    while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 10 ]; do sleep 1; i=$((i + 1)); done
    kill -KILL "-$pid" 2>/dev/null || true
    rm -f "$RUN_DIR/$NAME.pid"
    echo "$NAME: stopped"
}

action=${1:-status}
selected=${2:-}
[ -n "$selected" ] || selected=$(names)

case "$action" in
    start|up) for name in $selected; do start_one "$name" || exit 1; done ;;
    stop|down) for name in $selected; do stop_one "$name"; done ;;
    restart) for name in $selected; do stop_one "$name"; start_one "$name" || exit 1; done ;;
    status)
        for name in $selected; do
            if running "$name"; then echo "$name: running (pid $(cat "$RUN_DIR/$name.pid"))"
            else echo "$name: stopped"; fi
        done
        ;;
    logs)
        [ -n "${2:-}" ] || { echo "usage: services logs SERVICE" >&2; exit 2; }
        tail -n "${LINES:-100}" "$LOG_DIR/$2.log"
        ;;
    *) echo "usage: services {up|down|restart|status|logs} [SERVICE]" >&2; exit 2 ;;
esac
