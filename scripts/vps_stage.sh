#!/bin/bash
# Report the VPS build stages and separate automated work from owner actions.
set -euo pipefail

ROOT=$(cd "$(dirname "$0")/.." && pwd)
IP=${WPD_IP:-}
[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)

remote=$(ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    alpine=missing
    [ -f /mnt/vps/etc/alpine-release ] && alpine=$(cat /mnt/vps/etc/alpine-release)
    docker=missing
    [ -x /mnt/vps/usr/bin/docker ] && docker=installed
    devices=no
    awk '\''$1 == "devices" && $4 == 1 { found=1 } END { exit !found }'\'' /proc/cgroups && devices=yes || true
    daemon=stopped
    [ -S /mnt/vps/run/docker.sock ] && daemon=running
    native=missing
    [ -x /mnt/vps/opt/walpad/services ] && native=installed
    caddy=stopped
    [ -s /mnt/vps/run/walpad-services/caddy.pid ] && \
        kill -0 $(cat /mnt/vps/run/walpad-services/caddy.pid) 2>/dev/null && caddy=running
    printf "alpine=%s\ndocker=%s\ndevices=%s\ndaemon=%s\nnative=%s\ncaddy=%s\n" \
        "$alpine" "$docker" "$devices" "$daemon" "$native" "$caddy"
')

value() { sed -n "s/^$1=//p" <<<"$remote"; }
alpine=$(value alpine)
docker=$(value docker)
devices=$(value devices)
daemon=$(value daemon)
native=$(value native)
caddy=$(value caddy)

echo "WalPad VPS stage report ($IP)"
echo "  1. Persistent WiFi + SSH       COMPLETE"
echo "  2. Internal persistent Alpine  $([ "$alpine" != missing ] && echo "COMPLETE ($alpine)" || echo BLOCKED)"
echo "  3. Docker + Compose packages   $([ "$docker" = installed ] && echo COMPLETE || echo PENDING)"
if [ "$devices" = yes ]; then
    echo "  4. Kernel container support    COMPLETE"
    echo "  5. Docker daemon test          $([ "$daemon" = running ] && echo COMPLETE || echo PENDING)"
else
    echo "  4. Kernel container support    BLOCKED (CONFIG_CGROUP_DEVICE is missing)"
    echo "  5. Docker daemon test          WAITING FOR STAGE 4"
fi
if [ "$native" = installed ] && [ "$caddy" = running ]; then
    echo "  6. Native manager + Caddy      COMPLETE (LAN HTTP is live)"
else
    echo "  6. Native manager + Caddy      PENDING"
fi
echo "  7. Public DNS/TLS exposure     WAITING FOR DOMAIN/INGRESS CHOICE"
echo
echo "Your work:"
echo "  Now: open http://$IP/ and confirm the test page from another device."
echo "  Next deployment: tell me which application/service you want hosted."
echo "  Stage 7: provide a domain/DNS provider, or choose Tailscale-only private access."
echo "  Security: do not open router ports 22 or 2375; only publish HTTPS after TLS is ready."
