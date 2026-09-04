#!/bin/sh
# Idempotent on-tablet repair for the persistent VPS service stack.
# This runs from vps_boot.sh after /mnt/vps is mounted.  The canonical bundle
# on the SD card lets a reboot repair missing/partial files without SSH.
set -u
SD=/mnt/sd/vps/walpad-services
ROOT=/mnt/vps

[ -d "$SD" ] || exit 0
mkdir -p "$ROOT/opt/walpad" "$ROOT/etc/walpad/services.d" \
    "$ROOT/opt/walpad/compose"
[ -f "$SD/services_ctl.sh" ] && cp "$SD/services_ctl.sh" "$ROOT/opt/walpad/services"
for f in power_button.py display_dashboard.py terminal_ui.py; do
    [ -f "$SD/$f" ] && cp "$SD/$f" "$ROOT/opt/walpad/$f"
done
[ -d "$SD/services" ] && cp "$SD/services/"*.conf "$ROOT/etc/walpad/services.d/" 2>/dev/null || true
[ -f "$SD/compose/walpad-compose.yml" ] && cp "$SD/compose/walpad-compose.yml" "$ROOT/opt/walpad/compose/"
cat > "$ROOT/opt/walpad/start.sh" <<'EOF'
#!/bin/sh
# Start all persistent services; services_ctl excludes the opt-in terminal.
exec /opt/walpad/services up
EOF
chmod 755 "$ROOT/opt/walpad/services" "$ROOT/opt/walpad/start.sh" \
    "$ROOT/opt/walpad/"*.py 2>/dev/null || true
touch "$ROOT/var/.walpad-services-ready"
exit 0
