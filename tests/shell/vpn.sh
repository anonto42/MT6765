#!/bin/sh
# Host tests for walpad-vpn (owner request 2026-11-10, change vpn-and-dns): fake root, stateful
# fake ip/iptables/wg/wireguard-go/openvpn.  Run by tests/shell/run.sh.  No real keys: the fixtures
# carry a dummy marker string so the tests can prove it never reaches argv, a log or the status.
cd "$(dirname "$0")/../.." || exit 1
S=$PWD/config/rootfs/usr/local/sbin
VS=$S/walpad-vpn
T=$(mktemp -d); trap 'kill $(cat "$T"/root/run/walpad/vpn/tun.pid "$T"/root/run/walpad/vpn/watch.pid 2>/dev/null) 2>/dev/null; rm -rf "$T"' EXIT
FAIL=0
ok()  { echo "PASS $1"; }
bad() { echo "FAIL $1: got '$2' want '$3'"; FAIL=$((FAIL+1)); }
eq()  { [ "$2" = "$3" ] && ok "$1" || bad "$1" "$2" "$3"; }
kv()  { printf '%s\n' "$1" | sed -n "s/^$2=//p" | head -n 1; }

sh -n "$VS" && ok "syntax walpad-vpn" || bad "syntax walpad-vpn" error ok
for p in wireguard-go wireguard-tools-wg openvpn iptables-legacy iproute2; do
  case " $(WALPAD_PRINT_PKGS=1 bash scripts/build_rootfs.sh) " in *" $p "*) ok "image has $p";; *) bad "image has $p" missing present;; esac
done

V=$T/v; R=$T/root; B=$V/bin
mkdir -p "$B" "$R/etc/walpad" "$R/var/lib/walpad/vpn" "$R/run/walpad" "$R/etc" "$R/sys/class/net/wg0/statistics"
chmod 700 "$R/var/lib/walpad/vpn"
export V
printf '1234\n' > "$R/sys/class/net/wg0/statistics/rx_bytes"; printf '99\n' > "$R/sys/class/net/wg0/statistics/tx_bytes"
KEYM=FAKEKEY-not-a-real-key-ZZ9
printf '[Interface]\nPrivateKey = %s\nAddress = 10.8.0.2/32\nDNS = 10.8.0.1\nMTU = 1380\n\n[Peer]\nPublicKey = FAKEPUB\nEndpoint = vpn.example.net:51820\nAllowedIPs = 0.0.0.0/0\n' "$KEYM" > "$R/var/lib/walpad/vpn/home.conf"
printf '[Interface]\nPrivateKey = %s\nAddress = 10.8.0.2/32\nPostUp = touch /tmp/pwned\n[Peer]\nPublicKey = FAKEPUB\nEndpoint = 203.0.113.9:51820\nAllowedIPs = 0.0.0.0/0\n' "$KEYM" > "$R/var/lib/walpad/vpn/evil.conf"
printf 'client\nremote 198.51.100.7 1194\nup /tmp/pwned.sh\n' > "$R/var/lib/walpad/vpn/evilo.ovpn"
printf 'client\nremote 198.51.100.7 1194\ndev tun\n<key>\n%s\n</key>\n' "$KEYM" > "$R/var/lib/walpad/vpn/work.ovpn"
chmod 600 "$R"/var/lib/walpad/vpn/*
printf 'nameserver 192.168.0.1\nsearch lan\n' > "$R/etc/resolv.conf"

cat > "$B/ip" <<'EOF'
#!/bin/sh
echo "ip $*" >> "$V/calls"
[ "$1" = -6 ] && exit 0
case "$1 $2" in
  "route show") if [ "$3" = default ]; then grep '^default' "$V/routes"; else cat "$V/routes"; fi ;;
  "route add") shift 2; echo "$*" >> "$V/routes" ;;
  "route del") shift 2; grep -vxF -- "$*" "$V/routes" > "$V/routes.n"; mv "$V/routes.n" "$V/routes" ;;
  "link show") [ -e "$V/link-$3" ] || exit 1 ;;
  "link del") rm -f "$V/link-$3"; grep -v " dev $3\$" "$V/routes" > "$V/routes.n"; mv "$V/routes.n" "$V/routes" ;;
esac
exit 0
EOF
cat > "$B/iptables" <<'EOF'
#!/bin/sh
echo "iptables $*" >> "$V/calls"
f=$V/ipt; touch "$f"
case $1 in
  -N) grep -qxF "N $2" "$f" && exit 1; echo "N $2" >> "$f" ;;
  -A) c=$2; shift 2; echo "A $c $*" >> "$f" ;;
  -I) c=$2; shift 3; echo "A $c $*" >> "$f" ;;
  -D) c=$2; shift 2; grep -qxF "A $c $*" "$f" || exit 1; grep -vxF "A $c $*" "$f" > "$f.n"; mv "$f.n" "$f" ;;
  -F) grep -v "^A $2 " "$f" > "$f.n"; mv "$f.n" "$f" ;;
  -X) grep -vxF "N $2" "$f" > "$f.n"; mv "$f.n" "$f" ;;
  -S) cat "$f" ;;
esac
exit 0
EOF
cat > "$B/wg" <<'EOF'
#!/bin/sh
echo "wg $*" >> "$V/calls"
case $1 in
  setconf) cp "$3" "$V/wg-seen.conf"; [ -f "$V/wg-reject" ] && exit 1 ;;
  show) [ "$3" = latest-handshakes ] && printf 'FAKEPUB\t%s\n' "$(cat "$V/hs" 2>/dev/null || echo 0)" ;;
esac
exit 0
EOF
cat > "$B/wireguard-go" <<'EOF'
#!/bin/sh
echo "wireguard-go $*" >> "$V/calls"
touch "$V/link-$1"
exec sleep 300
EOF
cat > "$B/openvpn" <<'EOF'
#!/bin/sh
echo "openvpn $*" >> "$V/calls"
while [ $# -gt 0 ]; do case $1 in --writepid) pf=$2; shift;; --log-append) lf=$2; shift;; esac; shift; done
sleep 300 > /dev/null 2>&1 &
echo $! > "$pf"
touch "$V/link-walpad-tun"
echo "Initialization Sequence Completed" >> "$lf"
EOF
cat > "$B/getent" <<'EOF'
#!/bin/sh
[ "$1" = ahostsv4 ] && echo "203.0.113.9 STREAM $2"
exit 0
EOF
cp "$B/iptables" "$B/iptables-legacy"; printf '#!/bin/sh\nexit 0\n' > "$B/ip6tables-legacy"
chmod +x "$B"/*

PRE_ROUTES='default via 192.168.0.1 dev wlan0 metric 10
192.168.0.0/24 dev wlan0 proto kernel scope link src 192.168.0.100'
reset() {
  printf '%s\n' "$PRE_ROUTES" > "$V/routes"; : > "$V/ipt"; : > "$V/calls"
  echo "N OUTPUT" > "$V/ipt"
  rm -f "$V"/link-* "$V/wg-reject" "$R/var/log/walpad-vpn.log"
  echo $(( $(date +%s) - 5 )) > "$V/hs"
  printf 'nameserver 192.168.0.1\nsearch lan\n' > "$R/etc/resolv.conf"
}
conf() { printf '%s\n' "$@" > "$R/etc/walpad/vpn.conf"; }
vpn() { WALPAD_VPN_ROOT=$R PATH=$B:$PATH WALPAD_VPN_HS_WAIT=${HSW:-3} WALPAD_VPN_REVERT_S=${REV:-60} WALPAD_VPN_WATCH_EVERY=${WE:-1} sh "$VS" "$1" 2>&1; }
leaks() { # prove the key marker is nowhere an observer could see it
  n=$(cat "$V/calls" "$R/var/log/walpad-vpn.log" "$R/var/log/walpad-vpn-openvpn.log" 2>/dev/null | grep -c "$KEYM")
  ps -eo args | grep -c "$KEYM" | { read -r p; echo $((n + p - 1)); }
}
procs() { echo $(( $(ps -eo args | grep -c '[s]leep 300') - ${PROCS0:-0} )); }
PROCS0=$(ps -eo args | grep -c '[s]leep 300')

# --- no profile
reset; conf "kill_switch=0"
OUT=$(vpn up); eq "up without a profile fails" "$?/$(kv "$OUT" state)/$(kv "$OUT" reason)" "1/failed/no VPN profile selected"
OUT=$(vpn status); eq "status after a failed start" "$(kv "$OUT" state)" failed
vpn down >/dev/null
OUT=$(vpn bogus); eq "unknown verb" "$?" 2

# --- WireGuard, no kill switch
reset; conf "profile=home" "kill_switch=0"
OUT=$(vpn up); eq "wireguard up succeeds" "$?/$(kv "$OUT" state)" "0/on"
eq "  profile and kind" "$(kv "$OUT" profile)/$(kv "$OUT" kind)/$(kv "$OUT" iface)" "home/wireguard/wg0"
case $(kv "$OUT" handshake_age) in [0-9]|[0-9][0-9]) ok "  status shows the handshake age";; *) bad "  status shows the handshake age" "$(kv "$OUT" handshake_age)" "a few seconds";; esac
eq "  rx/tx from the interface" "$(kv "$OUT" rx)/$(kv "$OUT" tx)" "1234/99"
grep -qx '0.0.0.0/1 dev wg0' "$V/routes" && grep -qx '128.0.0.0/1 dev wg0' "$V/routes" && ok "  default route goes through the tunnel (two /1 routes)" || bad "  default route through the tunnel" "$(cat "$V/routes")" "0.0.0.0/1 dev wg0"
grep -qx '203.0.113.9/32 via 192.168.0.1 dev wlan0' "$V/routes" && ok "  endpoint stays on the real network" || bad "  endpoint host route" "$(cat "$V/routes")" "203.0.113.9/32 via ..."
grep -q 'ip addr add 10.8.0.2/32 dev wg0' "$V/calls" && grep -q 'ip link set wg0 mtu 1380 up' "$V/calls" && ok "  address and MTU from the config" || bad "  address and MTU" "$(grep 'ip ' "$V/calls")" ok
grep -q '^PrivateKey' "$V/wg-seen.conf" && ! grep -qi '^Address\|^DNS\|^MTU' "$V/wg-seen.conf" && ok "  wg setconf gets a filtered copy with the key" || bad "  filtered wg config" "$(cat "$V/wg-seen.conf")" "key without Address/DNS"
[ -e "$R/run/walpad/vpn/wg.conf" ] && bad "  temporary config deleted" present removed || ok "  temporary config deleted"
eq "  DNS from the tunnel config" "$(sed -n 's/^nameserver //p' "$R/etc/resolv.conf" | tr '\n' ' ')" "10.8.0.1 "
grep -qx 'search lan' "$R/etc/resolv.conf" && ok "  search domain kept" || bad "  search domain kept" "$(cat "$R/etc/resolv.conf")" "search lan"
eq "  run dir mode" "$(stat -c %a "$R/run/walpad/vpn")" 700
eq "  key never in argv, calls, logs or ps" "$(leaks)" 0
OUT=$(vpn status); eq "  status call" "$?/$(kv "$OUT" state)" "0/on"
case $OUT in *"$KEYM"*) bad "  status has no secrets" leaked clean;; *) ok "  status has no secrets";; esac
OUT=$(vpn down); eq "down succeeds" "$?/$(kv "$OUT" state)" "0/off"
eq "  routes equal the saved pre-tunnel state" "$(cat "$V/routes")" "$PRE_ROUTES"
eq "  iptables unchanged" "$(cat "$V/ipt")" "N OUTPUT"
eq "  resolv.conf restored" "$(cat "$R/etc/resolv.conf")" "$(printf 'nameserver 192.168.0.1\nsearch lan')"
eq "  tunnel process gone" "$(procs)" 0
[ -e "$V/link-wg0" ] && bad "  interface deleted" present removed || ok "  interface deleted"
OUT=$(vpn status); eq "  status off" "$(kv "$OUT" state)" off

# --- kill switch + confirm
reset; conf "profile=home" "kill_switch=1"
OUT=$(vpn up); eq "kill switch up succeeds" "$?/$(kv "$OUT" state)/$(kv "$OUT" kill_switch)" "0/on/1"
case $(kv "$OUT" revert_in) in [0-9]*) ok "  waiting for confirmation: revert_in shown";; *) bad "  revert_in shown" "$(kv "$OUT" revert_in)" number;; esac
K=$(cat "$V/ipt")
case $K in *"A WALPAD_VPN_KS -o lo -j ACCEPT"*) ok "  loopback allowed";; *) bad "  loopback allowed" "$K" ok;; esac
case $K in *"A WALPAD_VPN_KS -o wg0 -j ACCEPT"*) ok "  tunnel allowed";; *) bad "  tunnel allowed" "$K" ok;; esac
case $K in *"A WALPAD_VPN_KS -p udp --sport 68 --dport 67 -j ACCEPT"*) ok "  DHCP allowed";; *) bad "  DHCP allowed" "$K" ok;; esac
case $K in *"A WALPAD_VPN_KS -d 203.0.113.9 -j ACCEPT"*) ok "  VPN endpoint allowed";; *) bad "  VPN endpoint allowed" "$K" ok;; esac
case $K in *"A WALPAD_VPN_KS -p tcp --sport 22 -d 192.168.0.0/16 -j ACCEPT"*) ok "  LAN SSH replies allowed";; *) bad "  LAN SSH replies" "$K" ok;; esac
eq "  everything else is rejected (last rule)" "$(grep '^A WALPAD_VPN_KS' "$V/ipt" | tail -n 1)" "A WALPAD_VPN_KS -j REJECT"
eq "  chain is first in OUTPUT" "$(grep -c '^A OUTPUT -j WALPAD_VPN_KS' "$V/ipt")" 1
OUT=$(vpn confirm); eq "confirm" "$?/$(kv "$OUT" confirmed)" "0/1"
OUT=$(vpn status); eq "  confirmed: no revert countdown" "$(kv "$OUT" revert_in)/$(kv "$OUT" kill_switch)" "/1"
# the tunnel process is killed: the kill switch keeps blocking, the watcher brings the tunnel back
kill "$(cat "$R/run/walpad/vpn/tun.pid")"; sleep 0.3
OUT=$(vpn status); eq "tunnel killed: reported as reconnecting" "$(kv "$OUT" state)" starting
eq "  kill switch still in place" "$(grep -c 'WALPAD_VPN_KS -j REJECT' "$V/ipt")" 1
i=0; while [ $i -lt 8 ]; do OUT=$(vpn status); [ "$(kv "$OUT" state)" = on ] && break; i=$((i+1)); sleep 1; done
eq "  the watcher restarts the tunnel" "$(kv "$OUT" state)" on
OUT=$(vpn down); eq "down with kill switch" "$?" 0
eq "  iptables -S equals the pre-tunnel state" "$(cat "$V/ipt")" "N OUTPUT"
eq "  routes equal the pre-tunnel state" "$(cat "$V/routes")" "$PRE_ROUTES"
eq "  no leftovers" "$(procs)" 0
eq "  no key leaks" "$(leaks)" 0

# --- safe revert: nobody confirms
reset; conf "profile=home" "kill_switch=1"
REV=2 vpn up >/dev/null
i=0; while [ $i -lt 10 ]; do OUT=$(vpn status); [ "$(kv "$OUT" state)" = failed ] && break; i=$((i+1)); sleep 1; done
eq "unconfirmed kill switch is reverted" "$(kv "$OUT" state)" failed
case $(kv "$OUT" reason) in reverted*) ok "  reason says reverted";; *) bad "  reason" "$(kv "$OUT" reason)" reverted;; esac
eq "  iptables back to the pre-tunnel state" "$(cat "$V/ipt")" "N OUTPUT"
eq "  routes back" "$(cat "$V/routes")" "$PRE_ROUTES"
eq "  tunnel stopped" "$(procs)" 0
vpn down >/dev/null

# --- failures roll back
reset; conf "profile=home" "kill_switch=1"
rm -f "$V/hs"
OUT=$(HSW=2 vpn up); eq "no handshake fails" "$?/$(kv "$OUT" state)" "1/failed"
case $(kv "$OUT" reason) in "no handshake"*) ok "  says why";; *) bad "  reason" "$(kv "$OUT" reason)" "no handshake";; esac
eq "  routes restored" "$(cat "$V/routes")" "$PRE_ROUTES"
eq "  iptables restored" "$(cat "$V/ipt")" "N OUTPUT"
eq "  resolv.conf restored" "$(cat "$R/etc/resolv.conf")" "$(printf 'nameserver 192.168.0.1\nsearch lan')"
eq "  nothing left running" "$(procs)" 0
reset; : > "$V/wg-reject"; conf "profile=home"
OUT=$(vpn up); eq "rejected config fails" "$?/$(kv "$OUT" reason)" "1/the WireGuard config was rejected"
eq "  routes restored" "$(cat "$V/routes")" "$PRE_ROUTES"
reset; printf '%s\n' 'default via 192.168.0.1 dev wlan0' > "$V/routes"; : > "$V/routes"; conf "profile=home"
OUT=$(vpn up); eq "no default route fails" "$(kv "$OUT" reason)" "no network: there is no default route"
reset; conf "profile=evil"
OUT=$(vpn up); eq "PostUp scripts are refused" "$?/$(kv "$OUT" reason)" "1/the profile runs scripts (PreUp/PostUp): not allowed"
case $(cat "$V/calls") in *wireguard-go*) bad "  tunnel never started" started no;; *) ok "  tunnel never started";; esac
reset; conf "profile=evilo"
OUT=$(vpn up); eq "OpenVPN up script is refused" "$?/$(kv "$OUT" reason)" "1/the profile runs scripts: not allowed"
reset; conf "profile=../etc/passwd"
OUT=$(vpn up); eq "path traversal in the profile name" "$(kv "$OUT" reason)" "profile not found"
reset; conf "profile=nope"
OUT=$(vpn up); eq "unknown profile" "$(kv "$OUT" reason)" "profile not found"

# --- OpenVPN
reset; conf "profile=work" "kill_switch=1"
OUT=$(vpn up); eq "openvpn up succeeds" "$?/$(kv "$OUT" state)/$(kv "$OUT" kind)/$(kv "$OUT" iface)" "0/on/openvpn/walpad-tun"
grep -q 'A WALPAD_VPN_KS -o walpad-tun -j ACCEPT' "$V/ipt" && grep -q 'A WALPAD_VPN_KS -d 198.51.100.7 -j ACCEPT' "$V/ipt" && ok "  kill switch allows the tunnel and the remote" || bad "  openvpn kill switch" "$(cat "$V/ipt")" ok
grep -q -- '--config .*work.ovpn' "$V/calls" && grep -q -- '--script-security 1' "$V/calls" && ok "  config by path, scripts disabled" || bad "  openvpn argv" "$(grep openvpn "$V/calls")" ok
eq "  key never in argv or logs" "$(leaks)" 0
OUT=$(vpn down); eq "openvpn down" "$?" 0
eq "  iptables and routes restored" "$(cat "$V/ipt")|$(cat "$V/routes")" "N OUTPUT|$PRE_ROUTES"
eq "  no leftovers" "$(procs)" 0

# --- DNS and proxy without a tunnel
reset; conf "dns=1.1.1.1,9.9.9.9" "proxy=http://10.0.0.2:3128"
mkdir -p "$R/usr/lib/firefox-esr/distribution"
OUT=$(vpn apply); eq "apply succeeds" "$?" 0
eq "DNS servers written" "$(sed -n 's/^nameserver //p' "$R/etc/resolv.conf" | tr '\n' ' ')" "1.1.1.1 9.9.9.9 "
grep -qx 'export http_proxy=http://10.0.0.2:3128' "$R/etc/profile.d/walpad-proxy.sh" && grep -qx 'export HTTPS_PROXY=http://10.0.0.2:3128' "$R/etc/profile.d/walpad-proxy.sh" && ok "proxy exported to login shells" || bad "proxy exported" "$(cat "$R/etc/profile.d/walpad-proxy.sh" 2>/dev/null)" ok
grep -q '"HTTPProxy":"10.0.0.2:3128"' "$R/usr/lib/firefox-esr/distribution/policies.json" && ok "Firefox policy written" || bad "Firefox policy" "$(cat "$R/usr/lib/firefox-esr/distribution/policies.json" 2>/dev/null)" ok
conf "dns=" "proxy="
vpn apply >/dev/null
eq "clearing restores resolv.conf" "$(cat "$R/etc/resolv.conf")" "$(printf 'nameserver 192.168.0.1\nsearch lan')"
[ -e "$R/etc/profile.d/walpad-proxy.sh" ] || [ -e "$R/usr/lib/firefox-esr/distribution/policies.json" ] && bad "proxy files removed" present removed || ok "proxy files removed"
echo '{"policies":{"Homepage":{}}}' > "$R/usr/lib/firefox-esr/distribution/policies.json"
conf "proxy=http://10.0.0.2:3128"; vpn apply >/dev/null
grep -q Homepage "$R/usr/lib/firefox-esr/distribution/policies.json" && ok "foreign Firefox policies are never overwritten" || bad "foreign policies kept" overwritten kept
conf "proxy=http://u:p@h:80;x"; vpn apply >/dev/null
[ -e "$R/etc/profile.d/walpad-proxy.sh" ] && bad "a malformed proxy is ignored" written ignored || ok "a malformed proxy is ignored"
conf "dns=1.1.1.1; rm -rf /"; vpn apply >/dev/null
eq "a malformed DNS entry is ignored" "$(cat "$R/etc/resolv.conf")" "$(printf 'nameserver 192.168.0.1\nsearch lan')"
OUT=$(vpn confirm); eq "confirm with nothing running fails" "$?" 1

exit $FAIL
