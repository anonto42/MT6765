#!/bin/sh
# Direct recovery hotspot. Keeps SSH reachable when router Wi-Fi is absent.
set -u
AP=ap0
ADDR=192.168.50.1
CONF=/etc/walpad/recovery-hostapd.conf
mkdir -p /run/walpad-recovery
ip link set "$AP" up 2>/dev/null || true
ip addr add "$ADDR/24" dev "$AP" 2>/dev/null || true
dnsmasq --no-daemon --interface="$AP" --bind-interfaces \
  --dhcp-range=192.168.50.20,192.168.50.100,255.255.255.0,12h \
  --dhcp-option=3,"$ADDR" --dhcp-option=6,"$ADDR" \
  --pid-file=/run/walpad-recovery/dnsmasq.pid \
  --log-facility=/var/log/walpad-recovery-dhcp.log &
# Run in the foreground so the WalPad service supervisor can track/restart it.
exec hostapd "$CONF"
