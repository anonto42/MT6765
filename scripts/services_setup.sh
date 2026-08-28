#!/bin/bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
IP=${WPD_IP:-}
[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)

tar -C "$ROOT" -cf - scripts/services_ctl.sh config/services \
| ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    set -e
    tmp=/tmp/walpad-services-setup
    mkdir -p "$tmp"
    tar -C "$tmp" -xf -
    chroot /mnt/vps /sbin/apk add caddy curl
    mkdir -p /mnt/vps/opt/walpad /mnt/vps/etc/walpad/services.d /mnt/vps/etc/caddy \
        /mnt/vps/srv/walpad-demo /mnt/vps/var/lib/caddy /mnt/vps/var/log/caddy
    cp "$tmp/scripts/services_ctl.sh" /mnt/vps/opt/walpad/services
    cp "$tmp/config/services/"*.conf /mnt/vps/etc/walpad/services.d/
    cp "$tmp/config/services/Caddyfile" /mnt/vps/etc/caddy/Caddyfile
    cp "$tmp/config/services/index.html" /mnt/vps/srv/walpad-demo/index.html
    chmod 755 /mnt/vps/opt/walpad/services
    cat > /mnt/vps/opt/walpad/start.sh <<"EOF"
#!/bin/sh
exec /opt/walpad/services up
EOF
    chmod 755 /mnt/vps/opt/walpad/start.sh
    chroot /mnt/vps /usr/sbin/caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
    chroot /mnt/vps /opt/walpad/services down
    chroot /mnt/vps /opt/walpad/services up
    chroot /mnt/vps /opt/walpad/services status
    chroot /mnt/vps /usr/bin/curl --max-time 5 -fsS http://127.0.0.1/ | grep -q "WalPad VPS is online"
'
curl --max-time 5 -fsS "http://$IP/" | grep -q "WalPad VPS is online"
echo "Native service stack passed local and LAN HTTP tests: http://$IP/"
