#!/bin/bash
# WalPad 8G server setup - run ON THE TABLET (via serial console)
# Usage: sh /mnt/sd/setup_server.sh

set -e

SERVER_DIR="/mnt/sd/server"
DROPBEAR_DIR="$SERVER_DIR/dropbear"
NGINX_DIR="$SERVER_DIR/nginx"
SAMBA_DIR="$SERVER_DIR/samba"

echo "=== WalPad 8G Server Setup ==="
echo "Target: $SERVER_DIR"

mkdir -p "$DROPBEAR_DIR" "$NGINX_DIR" "$SAMBA_DIR"

# 1. Dropbear SSH over serial (ttyGS1)
echo "[1/4] Generating dropbear host keys..."
dropbearkey -t rsa -f "$DROPBEAR_DIR/dropbear_rsa_host_key" 2>/dev/null || true
dropbearkey -t ed25519 -f "$DROPBEAR_DIR/dropbear_ed25519_host_key" 2>/dev/null || true

echo "[2/4] Creating dropbear start script..."
cat > "$DROPBEAR_DIR/start.sh" <<'EOF'
#!/bin/sh
# Start dropbear SSH on ttyGS1 (serial port 2)
# Connect from host: sudo screen /dev/ttyACM1 115200
# Then SSH: ssh -p 2222 root@localhost (or use the serial directly)

# dropbear doesn't daemonize well on serial; run in foreground
exec dropbear -F -E -r /mnt/sd/server/dropbear/dropbear_rsa_host_key \
    -r /mnt/sd/server/dropbear/dropbear_ed25519_host_key \
    -p 2222 -a -K 60
EOF
chmod +x "$DROPBEAR_DIR/start.sh"

# 2. Basic nginx config (if nginx binary exists)
echo "[3/4] Creating nginx config..."
cat > "$NGINX_DIR/nginx.conf" <<'EOF'
worker_processes 1;
daemon off;
error_log /mnt/sd/server/nginx/error.log warn;
pid /mnt/sd/server/nginx/nginx.pid;

events {
    worker_connections 64;
}

http {
    include /etc/nginx/mime.types 2>/dev/null || true;
    default_type application/octet-stream;
    access_log /mnt/sd/server/nginx/access.log;
    sendfile on;

    server {
        listen 8080;
        server_name walpad.local;
        root /mnt/sd/server-data;
        autoindex on;
        location / {
            try_files $uri $uri/ =404;
        }
    }
}
EOF

# 3. Samba config (if smbd exists)
echo "[4/4] Creating samba config..."
cat > "$SAMBA_DIR/smb.conf" <<'EOF'
[global]
workgroup = WORKGROUP
server string = WalPad Server
security = user
map to guest = Bad User
guest account = root
log file = /mnt/sd/server/samba/log.%m
max log size = 100
load printers = no
printing = bsd

[server-data]
path = /mnt/sd/server-data
read only = no
guest ok = yes
force user = root
create mask = 0664
directory mask = 0775
EOF

# 4. Persistence: add to /mnt/sd/init-extra.sh (sourced by custom init if present)
echo "Creating persistence hook..."
cat > "$SERVER_DIR/init-extra.sh" <<'EOF'
#!/bin/sh
# This script can be called from the tablet's /init to start services
# Edit /init on the ramdisk to: . /mnt/sd/server/init-extra.sh

export PATH="/sbin:/bin:/usr/sbin:/usr/bin:$PATH"

# Start dropbear on ttyGS1 (serial port 2) in background
if [ -x /mnt/sd/server/dropbear/start.sh ]; then
    /mnt/sd/server/dropbear/start.sh &
    echo "dropbear started on ttyGS1 (port 2222)"
fi

# Start nginx if binary exists
if command -v nginx >/dev/null 2>&1 && [ -f /mnt/sd/server/nginx/nginx.conf ]; then
    nginx -c /mnt/sd/server/nginx/nginx.conf -p /mnt/sd/server/nginx/
    echo "nginx started on port 8080"
fi

# Start samba if binaries exist
if command -v smbd >/dev/null 2>&1 && [ -f /mnt/sd/server/samba/smb.conf ]; then
    smbd -s /mnt/sd/server/samba/smb.conf -D
    nmbd -s /mnt/sd/server/samba/smb.conf -D
    echo "samba started"
fi
EOF
chmod +x "$SERVER_DIR/init-extra.sh"

echo ""
echo "=== Setup complete ==="
echo "Files created in $SERVER_DIR:"
ls -la "$SERVER_DIR"/
echo ""
echo "To use:"
echo "  1. Copy init-extra.sh into ramdisk /init (see below)"
echo "  2. Run services manually for now:"
echo "     sh /mnt/sd/server/dropbear/start.sh   # SSH on ttyGS1"
echo "  3. For persistence, edit the ramdisk /init to source /mnt/sd/server/init-extra.sh"
echo ""
echo "To make persistent across reboots (requires repacking boot.img):"
echo "  1. Unpack boot.img ramdisk"
echo "  2. Edit /init: add '. /mnt/sd/server/init-extra.sh' before the loop"
echo "  3. Repack and flash via BROM window (make instant_catch)"