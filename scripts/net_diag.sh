#!/usr/bin/env bash
# net_diag.sh — controlled host-side USB-net test with counters.
# Shows whether the host can actually transmit on the ECM/RNDIS links.
set -u
ECM="${1:-enp10s0f3u1i4}"
RNDIS="${2:-enp10s0f3u1i6}"

cnt() {
  echo "  $1 rx_pkt=$(cat /sys/class/net/$1/statistics/rx_packets 2>/dev/null) tx_pkt=$(cat /sys/class/net/$1/statistics/tx_packets 2>/dev/null) rx_b=$(cat /sys/class/net/$1/statistics/rx_bytes 2>/dev/null) tx_b=$(cat /sys/class/net/$1/statistics/tx_bytes 2>/dev/null)"
}

echo "== before =="
cnt $ECM; cnt $RNDIS

echo "== reconfigure $ECM (ECM) =="
ip link set $ECM down 2>/dev/null
ip link set $ECM up
ip addr flush dev $ECM 2>/dev/null
ip addr add 192.168.42.100/24 dev $ECM
sleep 1
ping -c 2 -W 1 192.168.42.1 2>&1 | tail -2

echo "== reconfigure $RNDIS (RNDIS) =="
ip link set $RNDIS down 2>/dev/null
ip link set $RNDIS up
ip addr flush dev $RNDIS 2>/dev/null
ip addr add 192.168.43.100/24 dev $RNDIS
sleep 1
ping -c 2 -W 1 192.168.43.1 2>&1 | tail -2

echo "== after =="
cnt $ECM; cnt $RNDIS
echo "== neighbors =="
ip neigh show dev $ECM; ip neigh show dev $RNDIS