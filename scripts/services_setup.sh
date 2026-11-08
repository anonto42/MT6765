#!/bin/bash
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
IP=${WPD_IP:-}
[ -n "$IP" ] || IP=$("$ROOT/scripts/wifi_find.sh" --ip-only)

tar -C "$ROOT" -cf - scripts/services_ctl.sh scripts/power_button.py \
    scripts/display_dashboard.py scripts/terminal_ui.py config/services config/compose \
    scripts/vps_boot.sh scripts/vps_self_setup.sh \
| ssh -F /dev/null -o BatchMode=yes "root@$IP" '
    set -e
    tmp=/tmp/walpad-services-setup
    mkdir -p "$tmp"
    tar -C "$tmp" -xf -
    chroot /mnt/vps /sbin/apk add caddy curl redis
    mkdir -p /mnt/vps/opt/walpad /mnt/vps/opt/walpad/compose \
        /mnt/vps/etc/walpad/services.d /mnt/vps/etc/caddy \
        /mnt/vps/srv/walpad-demo /mnt/vps/var/lib/caddy /mnt/vps/var/log/caddy
    mkdir -p /mnt/vps/var/lib/redis
    chroot /mnt/vps /bin/sh -c 'id redis >/dev/null 2>&1 && chown -R redis:redis /var/lib/redis || true'
    cp "$tmp/scripts/services_ctl.sh" /mnt/vps/opt/walpad/services
    cp "$tmp/scripts/power_button.py" /mnt/vps/opt/walpad/power_button.py
    cp "$tmp/scripts/display_dashboard.py" /mnt/vps/opt/walpad/display_dashboard.py
    cp "$tmp/scripts/terminal_ui.py" /mnt/vps/opt/walpad/terminal_ui.py
    cp "$tmp/config/compose/walpad-compose.yml" /mnt/vps/opt/walpad/compose/
    cp "$tmp/config/services/"*.conf /mnt/vps/etc/walpad/services.d/
    cp "$tmp/config/services/Caddyfile" /mnt/vps/etc/caddy/Caddyfile
    cp "$tmp/config/services/index.html" /mnt/vps/srv/walpad-demo/index.html
    chmod 755 /mnt/vps/opt/walpad/services /mnt/vps/opt/walpad/power_button.py \
        /mnt/vps/opt/walpad/display_dashboard.py
    chmod 755 /mnt/vps/opt/walpad/terminal_ui.py
    # Keep a complete canonical copy on SD so vps_boot can self-repair the
    # mounted VPS without requiring WiFi/SSH after a reboot.
    rm -rf /mnt/sd/vps/walpad-services
    mkdir -p /mnt/sd/vps/walpad-services/services /mnt/sd/vps/walpad-services/compose
    cp "$tmp/scripts/services_ctl.sh" /mnt/sd/vps/walpad-services/services_ctl.sh
    cp "$tmp/scripts/power_button.py" /mnt/sd/vps/walpad-services/power_button.py
    cp "$tmp/scripts/display_dashboard.py" /mnt/sd/vps/walpad-services/display_dashboard.py
    cp "$tmp/scripts/terminal_ui.py" /mnt/sd/vps/walpad-services/terminal_ui.py
    cp "$tmp/config/services/"*.conf /mnt/sd/vps/walpad-services/services/
    cp "$tmp/config/compose/"* /mnt/sd/vps/walpad-services/compose/ 2>/dev/null || true
    chmod 755 /mnt/sd/vps/walpad-services/services_ctl.sh /mnt/sd/vps/walpad-services/*.py
    cp "$tmp/scripts/vps_self_setup.sh" /mnt/sd/vps/vps_self_setup.sh
    chmod 755 /mnt/sd/vps/vps_self_setup.sh
    # Keep the boot hook on the persistent SD payload in sync.  Otherwise a
    # reboot can execute an older volatile setup and appear to erase services.
    cp "$tmp/scripts/vps_boot.sh" /mnt/sd/vps/boot.sh
    chmod 755 /mnt/sd/vps/boot.sh
cat > /mnt/vps/opt/walpad/start.sh <<"EOF"
#!/bin/sh
# Start all persistent services; services_ctl excludes the opt-in terminal.
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
