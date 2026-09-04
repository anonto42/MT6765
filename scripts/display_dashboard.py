#!/usr/bin/env python3
"""Render WalPad VPS status directly to the stock MediaTek framebuffer."""

import argparse
import fcntl
import gzip
import mmap
import os
import socket
import struct
import subprocess
import time

WIDTH = 800
HEIGHT = 1280
STRIDE = 3200
FB_SIZE = STRIDE * HEIGHT * 3
BG = (18, 24, 38, 255)
FG = (235, 241, 255, 255)
MUTED = (157, 170, 196, 255)
GREEN = (74, 222, 128, 255)
ORANGE = (251, 146, 60, 255)
ROTATION = 0
GSENSOR_READ = 0x80048503
LAST_CPU = None
LAST_NET = None
LAST_SAMPLE = None


def read(path, default="?"):
    try:
        with open(path, encoding="utf-8") as handle:
            value = handle.read().strip()
            return value or default
    except OSError:
        return default


def command(argv, default="?"):
    try:
        return subprocess.check_output(argv, text=True, timeout=3).strip() or default
    except (OSError, subprocess.SubprocessError):
        return default


def load_psf(path):
    opener = gzip.open if path.endswith(".gz") else open
    with opener(path, "rb") as handle:
        data = handle.read()
    magic, version, header_size, flags, count, glyph_size, height, width = struct.unpack_from(
        "<8I", data, 0
    )
    if magic != 0x864AB572:
        raise ValueError("dashboard requires a PSF2 font")
    return data[header_size : header_size + count * glyph_size], count, glyph_size, width, height


def pixel(frame, x, y, color):
    logical_width, logical_height = ((WIDTH, HEIGHT) if ROTATION in (0, 180) else (HEIGHT, WIDTH))
    if 0 <= x < logical_width and 0 <= y < logical_height:
        if ROTATION == 0:
            px, py = x, y
        elif ROTATION == 90:
            px, py = WIDTH - 1 - y, x
        elif ROTATION == 180:
            px, py = WIDTH - 1 - x, HEIGHT - 1 - y
        else:
            px, py = y, HEIGHT - 1 - x
        offset = py * STRIDE + px * 4
        # MediaTek fb0 uses BGRA/XRGB ordering; neutral colors work either way.
        r, g, b, a = color
        frame[offset : offset + 4] = bytes((b, g, r, a))


def rect(frame, x, y, width, height, color):
    logical_width, logical_height = ((WIDTH, HEIGHT) if ROTATION in (0, 180) else (HEIGHT, WIDTH))
    if ROTATION:
        for py in range(max(0, y), min(logical_height, y + height)):
            for px in range(max(0, x), min(logical_width, x + width)):
                pixel(frame, px, py, color)
        return
    r, g, b, a = color
    row = bytes((b, g, r, a)) * width
    for py in range(max(0, y), min(HEIGHT, y + height)):
        start = py * STRIDE + max(0, x) * 4
        frame[start : start + len(row)] = row


def text(frame, font, value, x, y, color=FG, scale=1):
    glyphs, count, glyph_size, glyph_width, glyph_height = font
    bytes_per_row = (glyph_width + 7) // 8
    cursor = x
    for char in value:
        index = ord(char)
        if index >= count:
            index = ord("?")
        glyph = glyphs[index * glyph_size : (index + 1) * glyph_size]
        for gy in range(glyph_height):
            for gx in range(glyph_width):
                if glyph[gy * bytes_per_row + gx // 8] & (0x80 >> (gx % 8)):
                    rect(frame, cursor + gx * scale, y + gy * scale, scale, scale, color)
        cursor += (glyph_width + 2) * scale


def panel(frame, x, y, width, height, color=MUTED):
    rect(frame, x, y, width, 2, color)
    rect(frame, x, y + height - 2, width, 2, color)
    rect(frame, x, y, 2, height, color)
    rect(frame, x + width - 2, y, 2, height, color)


def indicator(frame, x, y, color):
    rect(frame, x, y, 13, 13, color)


def cpu_percent():
    global LAST_CPU
    try:
        fields = [int(value) for value in read("/proc/stat").splitlines()[0].split()[1:]]
        total, idle = sum(fields), fields[3] + fields[4]
        current = (total, idle)
        if LAST_CPU is None:
            result = "..."
        else:
            delta_total = total - LAST_CPU[0]
            result = f"{100 * (delta_total - (idle - LAST_CPU[1])) / max(1, delta_total):.0f}%"
        LAST_CPU = current
        return result
    except (ValueError, IndexError):
        return "?"


def memory_usage():
    values = {}
    for line in read("/proc/meminfo", "").splitlines():
        parts = line.split()
        if len(parts) >= 2:
            values[parts[0].rstrip(":")] = int(parts[1])
    total = values.get("MemTotal", 0) / 1048576
    used = (values.get("MemTotal", 0) - values.get("MemAvailable", 0)) / 1048576
    return f"{used:.1f}/{total:.1f} GB"


def network_rate():
    global LAST_NET, LAST_SAMPLE
    try:
        line = next(line for line in read("/proc/net/dev", "").splitlines() if "wlan0:" in line)
        fields = line.split(":", 1)[1].split()
        current, now = (int(fields[0]), int(fields[8])), time.monotonic()
        if LAST_NET is None:
            result = "RX ...  TX ..."
        else:
            elapsed = max(0.1, now - LAST_SAMPLE)
            rx = (current[0] - LAST_NET[0]) * 8 / elapsed / 1_000_000
            tx = (current[1] - LAST_NET[1]) * 8 / elapsed / 1_000_000
            result = f"RX {rx:.2f} TX {tx:.2f} Mbps"
        LAST_NET, LAST_SAMPLE = current, now
        return result
    except (StopIteration, ValueError, IndexError):
        return "offline"


def endpoint(port):
    started = time.monotonic()
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=0.8) as sock:
            sock.sendall(b"GET / HTTP/1.0\r\nHost: localhost\r\n\r\n")
            ok = b"HTTP/" in sock.recv(64)
        return ok, f"{(time.monotonic() - started) * 1000:.0f}ms"
    except OSError:
        return False, "DOWN"


def hardware_status():
    accel = os.path.exists("/dev/gsensor")
    audio_out = os.path.exists("/dev/snd/pcmC0D0p")
    audio_in = os.path.exists("/dev/snd/pcmC0D1c")
    camera = bool(command(["sh", "-c", "ls /dev/video* /dev/media* 2>/dev/null | head -1"], ""))
    return "ACCEL " + ("AUDIO_OUT " if audio_out else "NO_SPK ") + ("AUDIO_IN " if audio_in else "NO_MIC ") + ("CAMERA " if camera else "NO_CAMERA")


def uptime_short():
    try:
        seconds = int(float(read("/proc/uptime").split()[0]))
        days, seconds = divmod(seconds, 86400)
        hours, seconds = divmod(seconds, 3600)
        minutes = seconds // 60
        if days:
            return f"{days}d {hours}h {minutes}m"
        if hours:
            return f"{hours}h {minutes}m"
        return f"{minutes}m"
    except (ValueError, IndexError):
        return "?"


def status_lines():
    ip = command(["sh", "-c", "ip -4 -o addr show wlan0 | awk '{print $4}' | cut -d/ -f1"])
    wifi = command(["sh", "-c", "iw dev wlan0 link 2>/dev/null | sed -n 's/^\tSSID: //p'"], "connected")
    capacity = read("/sys/class/power_supply/battery/capacity")
    charging = read("/sys/class/power_supply/battery/status")
    temperature = read("/sys/class/power_supply/battery/temp")
    try:
        temperature = f"{int(temperature) / 10:.1f} C"
    except ValueError:
        pass
    # This program runs chrooted inside the persistent VPS filesystem, where
    # that filesystem is mounted as / rather than /mnt/vps.
    storage = command(["sh", "-c", "df -h / 2>/dev/null | awk 'NR==2 {print $4 \" free / \" $2}'"])
    uptime = command(["sh", "-c", "awk '{printf \"%.1f hours\", $1/3600}' /proc/uptime"])
    services = command(["sh", "-c", "/opt/walpad/services status 2>/dev/null | tr '\n' ' '"])
    return ip, wifi, capacity, charging, temperature, storage, uptime, services


def orientation(sensor):
    """Return framebuffer rotation from the MediaTek accelerometer."""
    try:
        buf = bytearray(128)
        fcntl.ioctl(sensor, GSENSOR_READ, buf, True)
        values = bytes(buf).split(b"\0", 1)[0].decode().split()
        axes = []
        for value in values[:3]:
            number = int(value, 16)
            axes.append(number - (1 << 32) if number & (1 << 31) else number)
        x, y, _ = axes
        # A margin prevents jitter while the device is nearly diagonal.
        if abs(x) > abs(y) * 1.25:
            return 90 if x > 0 else 270
        if abs(y) > abs(x) * 1.25:
            return 0 if y > 0 else 180
    except (OSError, ValueError):
        pass
    return ROTATION


_display_initialized = False
def commit_display():
    """Tell mtkfb to present the newly written shared framebuffer pages."""
    if os.path.exists("/run/walpad-screen-off"):
        return
    global _display_initialized
    try:
        with open("/sys/class/graphics/fb0/pan", "w", encoding="ascii") as handle:
            handle.write("0,1280")
        with open("/sys/class/graphics/fb0/pan", "w", encoding="ascii") as handle:
            handle.write("0,0")
        # Only wake the panel once. Repeated blank/unblank resets mtk-tpd and
        # prevents the touchscreen from emitting input events.
        if not _display_initialized:
            with open("/sys/class/graphics/fb0/blank", "w", encoding="ascii") as handle:
                handle.write("1")
            time.sleep(0.04)
            with open("/sys/class/graphics/fb0/blank", "w", encoding="ascii") as handle:
                handle.write("0")
            _display_initialized = True
    except OSError:
        pass


def render(frame, font):
    # Clear the physical page directly; subsequent drawing uses logical
    # coordinates transformed according to ROTATION.
    frame[: STRIDE * HEIGHT] = bytes((BG[2], BG[1], BG[0], BG[3])) * (WIDTH * HEIGHT)
    ip, wifi, capacity, charging, temperature, storage, uptime, services = status_lines()
    hardware = hardware_status()
    cpu, ram, net = cpu_percent(), memory_usage(), network_rate()
    disk = command(["sh", "-c", "df -P / | awk 'NR==2 {print $5}'"])
    caddy_ok, caddy_ms = endpoint(80)
    demo_ok, demo_ms = endpoint(8081)
    api_ok, api_ms = endpoint(8080)
    service_rows = []
    for item in services.split("  "):
        if item.strip():
            service_rows.append((item.split(":", 1)[0], "running" in item))
    # Show the services that determine public availability first; glob order
    # otherwise hides the ecommerce processes behind dashboard/demo entries.
    service_map = dict(service_rows)
    service_rows = [(name, service_map.get(name, False)) for name in
                    ("caddy", "cloudflared", "ecommerce-backend", "ecommerce-frontend")]
    if ROTATION in (90, 270):
        panel(frame, 20, 18, 1240, 764)
        text(frame, font, "VPS: aloevol-prod", 42, 38, FG)
        indicator(frame, 335, 43, GREEN if caddy_ok and demo_ok else ORANGE)
        text(frame, font, "HEALTHY" if caddy_ok and api_ok else "DEGRADED", 360, 38, GREEN if caddy_ok and api_ok else ORANGE)
        text(frame, font, "Uptime: " + uptime_short(), 1000, 38, MUTED)
        metrics = [("CPU", cpu), ("RAM", ram), ("DISK", disk), ("NETWORK", net)]
        for index, (label, value) in enumerate(metrics):
            x = 42 + index * 305
            text(frame, font, label, x, 105, ORANGE)
            text(frame, font, value, x, 140, FG)
            if index:
                rect(frame, x - 20, 92, 2, 100, MUTED)
        rect(frame, 20, 205, 1240, 2, MUTED)
        text(frame, font, "SERVICES", 42, 230, ORANGE)
        text(frame, font, "APPLICATION HEALTH", 655, 230, ORANGE)
        for index, (name, ok) in enumerate(service_rows):
            y = 270 + index * 38
            indicator(frame, 45, y + 4, GREEN if ok else ORANGE)
            text(frame, font, name, 70, y, FG)
        for index, (name, ok, latency) in enumerate((("Caddy", caddy_ok, caddy_ms), ("Go API", api_ok, api_ms))):
            y = 270 + index * 38
            indicator(frame, 658, y + 4, GREEN if ok else ORANGE)
            text(frame, font, f"{name:<20} {latency}", 683, y, FG)
        rect(frame, 630, 207, 2, 220, MUTED)
        rect(frame, 20, 427, 1240, 2, MUTED)
        text(frame, font, "CONTAINERS", 42, 450, ORANGE)
        indicator(frame, 45, 490, ORANGE)
        text(frame, font, "Podman unavailable: kernel cgroups", 70, 486, FG)
        text(frame, font, "DEPLOYMENTS", 655, 450, ORANGE)
        text(frame, font, "infra      local configuration", 658, 486, FG)
        rect(frame, 630, 429, 2, 160, MUTED)
        rect(frame, 20, 589, 1240, 2, MUTED)
        text(frame, font, "SYSTEM / ACCESS", 42, 612, ORANGE)
        text(frame, font, f"WiFi: {wifi}  IP: {ip}  SSH: root@{ip}", 42, 650, FG)
        text(frame, font, f"Battery: {capacity}% {charging}  Temp: {temperature}  Storage: {storage}", 42, 687, FG)
        text(frame, font, "Hardware: " + hardware, 42, 724, FG)
        text(frame, font, "ALERTS", 655, 612, ORANGE)
        indicator(frame, 658, 654, ORANGE)
        text(frame, font, "Container kernel support missing", 683, 650, FG)
        text(frame, font, time.strftime("Updated %Y-%m-%d %H:%M:%S"), 900, 745, MUTED)
    else:
        panel(frame, 20, 18, 760, 1244)
        text(frame, font, "VPS: aloevol-prod", 42, 40, FG)
        indicator(frame, 45, 82, GREEN if caddy_ok and demo_ok else ORANGE)
        text(frame, font, ("HEALTHY" if caddy_ok and api_ok else "DEGRADED") + "  Uptime " + uptime_short(), 70, 77, GREEN)
        metrics = [("CPU", cpu), ("RAM", ram), ("DISK", disk), ("NETWORK", net)]
        for index, (label, value) in enumerate(metrics):
            x = 42 + (index % 2) * 375
            y = 145 + (index // 2) * 90
            text(frame, font, label, x, y, ORANGE)
            text(frame, font, value, x, y + 35, FG)
        rect(frame, 20, 335, 760, 2, MUTED)
        text(frame, font, "SERVICES", 42, 365, ORANGE)
        for index, (name, ok) in enumerate(service_rows):
            y = 405 + index * 42
            indicator(frame, 45, y + 4, GREEN if ok else ORANGE)
            text(frame, font, name, 70, y, FG)
        text(frame, font, "APPLICATION HEALTH", 42, 600, ORANGE)
        for index, (name, ok, latency) in enumerate((("Caddy", caddy_ok, caddy_ms), ("Go API", api_ok, api_ms))):
            y = 640 + index * 42
            indicator(frame, 45, y + 4, GREEN if ok else ORANGE)
            text(frame, font, f"{name:<20} {latency}", 70, y, FG)
        text(frame, font, "CONTAINERS / ALERTS", 42, 760, ORANGE)
        indicator(frame, 45, 805, ORANGE)
        text(frame, font, "Podman unavailable: kernel cgroups", 70, 801, FG)
        text(frame, font, "SYSTEM / ACCESS", 42, 900, ORANGE)
        text(frame, font, f"WiFi: {wifi}", 42, 945, FG)
        text(frame, font, f"IP: {ip}", 42, 985, FG)
        text(frame, font, f"Battery: {capacity}% {charging}", 42, 1025, FG)
        text(frame, font, f"Temp: {temperature}", 42, 1065, FG)
        text(frame, font, "Hardware: " + hardware, 42, 1100, FG)
        text(frame, font, "SSH root@" + ip, 42, 1140, GREEN)
        text(frame, font, time.strftime("Updated %Y-%m-%d %H:%M:%S"), 42, 1185, MUTED)
    # mtkfb exposes three vertically stacked 800x1280 pages and may retain the
    # bootloader logo on page 1 or 2. Keep every page identical so the dashboard
    # remains visible regardless of the driver's current y-offset.
    page_size = STRIDE * HEIGHT
    page = frame[:page_size]
    frame[page_size : page_size * 2] = page
    frame[page_size * 2 : page_size * 3] = page


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--font", required=True)
    parser.add_argument("--interval", type=int, default=10)
    parser.add_argument("--once", action="store_true")
    args = parser.parse_args()
    font = load_psf(args.font)
    global ROTATION
    sensor = None
    try:
        sensor = open("/dev/gsensor", "rb", buffering=0)
    except OSError:
        pass
    with open("/dev/fb0", "r+b", buffering=0) as fb:
        with mmap.mmap(fb.fileno(), FB_SIZE, mmap.MAP_SHARED, mmap.PROT_READ | mmap.PROT_WRITE) as frame:
            while True:
                if sensor:
                    ROTATION = orientation(sensor)
                render(frame, font)
                commit_display()
                if args.once:
                    break
                time.sleep(max(2, args.interval))


if __name__ == "__main__":
    main()
