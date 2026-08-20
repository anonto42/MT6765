#!/usr/bin/env bash
# link_host.sh — configure the HOST side of the WalPad-Server USB network.
#
# The tablet's server ramdisk brings up:
#   usb0    = 192.168.42.1/24   (CDC-ECM,  host iface enp10s0f3u1i4)
#   rndis0  = 192.168.43.1/24   (RNDIS,    host iface enp10s0f3u1i6)
# The host needs matching addresses, then it can reach the tablet directly.
#
# Usage:  sudo bash scripts/link_host.sh [up|down|status]
set -eu

ECM_HOST_IF="${ECM_HOST_IF:-enp10s0f3u1i4}"
RNDIS_HOST_IF="${RNDIS_HOST_IF:-enp10s0f3u1i6}"
DEV_ECM_IP="${DEV_ECM_IP:-192.168.42.1}"
DEV_RNDIS_IP="${DEV_RNDIS_IP:-192.168.43.1}"

cmd="${1:-up}"
case "$cmd" in
  up)
    ip addr add 192.168.42.100/24 dev "$ECM_HOST_IF" 2>/dev/null || true
    ip addr add 192.168.43.100/24 dev "$RNDIS_HOST_IF" 2>/dev/null || true
    ip link set "$ECM_HOST_IF" up
    ip link set "$RNDIS_HOST_IF" up
    sleep 1
    echo "== ping tablet (ECM) =="
    ping -c 3 -W 2 "$DEV_ECM_IP" || echo "no ECM reply"
    echo "== ARP on $ECM_HOST_IF =="
    ip neigh show dev "$ECM_HOST_IF"
    echo "== ping tablet (RNDIS) =="
    ping -c 3 -W 2 "$DEV_RNDIS_IP" || echo "no RNDIS reply"
    echo "== ARP on $RNDIS_HOST_IF =="
    ip neigh show dev "$RNDIS_HOST_IF"
    ;;
  down)
    ip addr flush dev "$ECM_HOST_IF" 2>/dev/null || true
    ip addr flush dev "$RNDIS_HOST_IF" 2>/dev/null || true
    ip link set "$ECM_HOST_IF" down 2>/dev/null || true
    ip link set "$RNDIS_HOST_IF" down 2>/dev/null || true
    echo "links down"
    ;;
  status)
    ip addr show "$ECM_HOST_IF" 2>/dev/null | grep inet || echo "$ECM_HOST_IF: no ip"
    ip addr show "$RNDIS_HOST_IF" 2>/dev/null | grep inet || echo "$RNDIS_HOST_IF: no ip"
    ;;
  *) echo "usage: $0 [up|down|status]"; exit 1 ;;
esac