#!/bin/sh
# Keep the tablet reachable when the access point or DHCP lease disappears.
SD=/mnt/sd
LOG=$SD/wifi/watchdog.log
exec >>"$LOG" 2>&1
echo "=== WiFi watchdog started $(date 2>/dev/null) ==="

while :; do
    ipaddr=$(ip -4 addr show wlan0 2>/dev/null | sed -n 's/.*inet \([0-9.]*\).*/\1/p' | head -1)
    connected=0
    if [ -n "$ipaddr" ] && command -v wpa_cli >/dev/null 2>&1; then
        wpa_cli -p /tmp/wpa -i wlan0 status 2>/dev/null | grep -q '^wpa_state=COMPLETED$' && connected=1
    elif [ -n "$ipaddr" ]; then
        connected=1
    fi
    if [ "$connected" -eq 0 ] && [ -x "$SD/mtk_stack_up.sh" ] && [ -x "$SD/wifi_assoc.sh" ]; then
        echo "$(date 2>/dev/null) reconnecting WiFi"
        WIFI_RECONNECT=1 sh "$SD/mtk_stack_up.sh" || true
        WIFI_RECONNECT=1 sh "$SD/wifi_assoc.sh" || true
    fi
    sleep 30
done
