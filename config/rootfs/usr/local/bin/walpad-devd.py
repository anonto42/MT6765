#!/usr/bin/env python3
"""walpad-devd: the WalPad device daemon ("driver").  One process, several behaviours:

  * auto-rotation   accelerometer (/dev/gsensor) -> /run/walpad/orientation (0/90/180/270), the
                    window-manager work area (EWMH struts) follows, fbmirror rotates the picture,
                    walpad-touchd maps touches through the same rotation
  * screen timeout  dim, then backlight off after inactivity; any touch/key wakes it
  * cover sensor    HALL_DEV switch: backlight off when the cover is closed, on when opened
  * battery guard   voltage/percent estimate, low-battery flag, safe power-off when critical
  * status export   /run/walpad/status.json (battery, orientation, screen, wifi, uptime)
  * power button    POWER short = screen on/off, POWER long = power menu (Power off / Reboot / ...);
                    while the menu is open POWER short = select.  Volume keys are handled by walpad-keys.
  * commands        write a line to /run/walpad/devd.cmd (see walpad-ctl):
                       screen on|off|toggle      rotate auto|lock|0|90|180|270
                       brightness N|+N|-N        keyboard place      reload
Settings: /etc/walpad/devd.json (all keys optional)."""
import fcntl, glob, json, os, select, struct, subprocess, sys, time

RUN = "/run/walpad"
ORI_FILE, STATUS, FIFO = RUN + "/orientation", RUN + "/status.json", RUN + "/devd.cmd"
BL = "/sys/class/leds/lcd-backlight/brightness"
CFG_PATH = "/etc/walpad/devd.json"
DEFAULTS = {
    "auto_rotate": True, "rotate_flip": False, "tilt_min": 3000, "tilt_ratio": 1.35, "rotate_samples": 3,
    "dim_after_s": 90, "dim_level": 20, "off_after_s": 240, "lid_off": True,
    "low_batt_mv": 3550, "critical_batt_mv": 3350, "default_brightness": 120,
}
EVT = struct.Struct("llHHi")
cfg = dict(DEFAULTS)
state = {"orientation": 0, "locked": False, "screen": "on", "brightness": 120, "saved": 120, "last_act": time.time(),
         "lid_closed": False, "low_batt": False, "pwr_down": 0.0, "pwr_long": False}
xdisp = None

def log(*a): print(time.strftime("%H:%M:%S"), *a, flush=True)
def rd(path, default=""):
    try: return open(path).read().strip()
    except OSError: return default
def wr(path, val):
    try: open(path, "w").write(str(val))
    except OSError: pass

def load_cfg():
    global cfg
    cfg = dict(DEFAULTS)
    try: cfg.update(json.load(open(CFG_PATH)))
    except Exception as e: log("config:", e, "- using defaults")

# ---------------------------------------------------------------- X helpers (struts, keyboard placement)
def x_connect():
    global xdisp
    from Xlib import display
    for _ in range(60):
        try: xdisp = display.Display(":0"); return
        except Exception: time.sleep(0.5)
    log("cannot reach X; rotation work-area disabled")

strut_win = None
def set_struts(ori):
    """Reserve the part of the 1280x1280 X screen that is NOT visible, so maximized windows fill the visible area."""
    global strut_win
    if xdisp is None: return
    from Xlib import X, Xatom
    root = xdisp.screen().root
    if strut_win is None:
        strut_win = root.create_window(0, 0, 1, 1, 0, xdisp.screen().root_depth, X.InputOutput, override_redirect=0)
        strut_win.change_property(xdisp.intern_atom("_NET_WM_WINDOW_TYPE"), Xatom.ATOM, 32, [xdisp.intern_atom("_NET_WM_WINDOW_TYPE_DOCK")])
        strut_win.change_property(xdisp.intern_atom("WM_NAME"), Xatom.STRING, 8, b"walpad-strut")
        strut_win.change_property(xdisp.intern_atom("_NET_WM_NAME"), xdisp.intern_atom("UTF8_STRING"), 8, b"walpad-strut")
        strut_win.map()
    landscape = ori in (90, 270)
    right = 0 if landscape else 480            # portrait: hide the right 480 px
    bottom = 480 if landscape else 0           # landscape: hide the bottom 480 px
    partial = [0, right, 0, bottom, 0, 0, 0, 1279 if right else 0, 0, 0, 0, 1279 if bottom else 0]
    if not right: partial[6] = partial[7] = 0
    if right: partial[6], partial[7] = 0, 1279
    if bottom: partial[10], partial[11] = 0, 1279
    strut_win.change_property(xdisp.intern_atom("_NET_WM_STRUT_PARTIAL"), Xatom.CARDINAL, 32, partial)
    strut_win.change_property(xdisp.intern_atom("_NET_WM_STRUT"), Xatom.CARDINAL, 32, [0, right, 0, bottom])
    xdisp.sync()

def place_keyboard():
    """Dock matchbox-keyboard to the bottom of the visible area (it sizes itself for the full 1280x1280 screen)."""
    ori = state["orientation"]; landscape = ori in (90, 270)
    vw, vh = (1280, 800) if landscape else (800, 1280)
    kh = 340 if landscape else 420
    env = dict(os.environ, DISPLAY=":0")
    try:
        ids = subprocess.run(["xdotool", "search", "--class", "matchbox-keyboard"], env=env, capture_output=True, text=True, timeout=5).stdout.split()
        for wid in ids:
            subprocess.run(["xdotool", "windowsize", wid, str(vw), str(kh)], env=env, timeout=5)
            subprocess.run(["xdotool", "windowmove", wid, "0", str(vh - kh)], env=env, timeout=5)
    except Exception as e: log("keyboard placement:", e)

# ---------------------------------------------------------------- screen / brightness
def set_brightness(v):
    v = max(0, min(255, int(v))); state["brightness"] = v; wr(BL, v)
def screen(mode):
    if mode == "toggle": mode = "off" if state["screen"] != "off" else "on"
    if mode == "on":
        if state["screen"] == "off" or state["screen"] == "dim": set_brightness(state["saved"])
        state["screen"] = "on"
    elif mode == "dim" and state["screen"] == "on":
        state["saved"] = state["brightness"] or cfg["default_brightness"]; set_brightness(cfg["dim_level"]); state["screen"] = "dim"
    elif mode == "off" and state["screen"] != "off":
        if state["screen"] == "on": state["saved"] = state["brightness"] or cfg["default_brightness"]
        set_brightness(0); state["screen"] = "off"
    state["last_act"] = time.time()

# ---------------------------------------------------------------- rotation
gs = None
def read_gsensor():
    global gs
    try:
        if gs is None: gs = open("/dev/gsensor", "rb", buffering=0)
        b = bytearray(128); fcntl.ioctl(gs, 0x80048503, b, True)
        v = []
        for t in bytes(b).split(b"\0", 1)[0].decode().split()[:3]:
            n = int(t, 16); v.append(n - (1 << 32) if n & (1 << 31) else n)
        return tuple(v) if len(v) == 3 else None
    except (OSError, ValueError): return None

def decide(x, y):
    if max(abs(x), abs(y)) < cfg["tilt_min"]: return None       # lying flat: keep the current orientation
    r = cfg["tilt_ratio"]
    if abs(x) > abs(y) * r: o = 90 if x > 0 else 270
    elif abs(y) > abs(x) * r: o = 0 if y > 0 else 180
    else: return None
    if cfg["rotate_flip"]: o = {0: 180, 180: 0, 90: 270, 270: 90}[o]
    return o

def set_orientation(o, why=""):
    if o == state["orientation"] and os.path.exists(ORI_FILE): return
    state["orientation"] = o; wr(ORI_FILE, o); log("orientation ->", o, why)
    try: set_struts(o)
    except Exception as e: log("struts:", e)
    place_keyboard()

# ---------------------------------------------------------------- battery
def battery():
    cap = rd("/sys/class/power_supply/battery/capacity", "-1"); st = rd("/sys/class/power_supply/battery/status", "?")
    try: c = int(cap)
    except ValueError: c = -1
    try:
        mv = int(rd("/sys/class/power_supply/battery/voltage_now", "0")); mv = mv // 1000 if mv > 100000 else mv
    except ValueError: mv = 0
    est = False
    if not 0 <= c <= 100 and mv > 2500: c = max(0, min(100, (mv - 3300) * 100 // 1000)); est = True
    return {"percent": c, "estimated": est, "status": st, "mv": mv, "temp_dC": rd("/sys/class/power_supply/battery/temp", "")}

def battery_guard():
    b = battery(); discharging = b["status"].lower().startswith("dischar")
    if discharging and 0 < b["mv"] < cfg["critical_batt_mv"]:
        log("CRITICAL battery", b["mv"], "mV: safe power-off"); os.sync(); subprocess.call(["poweroff", "-f"])
    low = discharging and 0 < b["mv"] < cfg["low_batt_mv"]
    if low != state["low_batt"]: state["low_batt"] = low; log("low battery flag ->", low)
    return b

# ---------------------------------------------------------------- status + commands
def write_status():
    s = {"time": int(time.time()), "uptime_s": float(rd("/proc/uptime", "0 0").split()[0]), "battery": battery(),
         "orientation": state["orientation"], "rotation_locked": state["locked"], "screen": state["screen"],
         "brightness": state["brightness"], "lid_closed": state["lid_closed"], "low_battery": state["low_batt"],
         "wifi": rd(RUN + "/wifi-state", "off")}
    tmp = STATUS + ".tmp"
    try: open(tmp, "w").write(json.dumps(s)); os.replace(tmp, STATUS)
    except OSError: pass

def command(line):
    p = line.strip().split()
    if not p: return
    log("cmd:", line.strip())
    try:
        if p[0] == "screen" and len(p) > 1: screen(p[1])
        elif p[0] == "rotate" and len(p) > 1:
            if p[1] == "toggle": state["locked"] = not state["locked"]
            elif p[1] == "auto": state["locked"] = False
            elif p[1] == "lock": state["locked"] = True
            elif p[1] in ("0", "90", "180", "270"): state["locked"] = True; set_orientation(int(p[1]), "manual")
        elif p[0] == "brightness" and len(p) > 1:
            a = p[1]; cur = state["brightness"]
            v = cur + int(a) if a[0] in "+-" else int(a)
            set_brightness(v); state["saved"] = state["brightness"]; state["screen"] = "on" if v > 0 else "off"
        elif p[0] == "keyboard": place_keyboard()
        elif p[0] == "reload": load_cfg()
    except Exception as e: log("command failed:", e)

# ---------------------------------------------------------------- input activity + cover sensor
def find_inputs():
    names = {"mtk-tpd": "touch", "gsl3680": "touch", "mtk-kpd": "key", "mtk-pmic-keys": "key", "HALL_DEV": "hall"}
    out = []
    for d in glob.glob("/sys/class/input/input*"):
        n = rd(d + "/name")
        if n in names:
            for e in glob.glob(d + "/event*"):
                path = "/dev/input/" + os.path.basename(e)
                if not os.path.exists(path):
                    maj, mino = map(int, rd(e + "/dev").split(":")); os.mknod(path, 0o660 | 0o020000, os.makedev(maj, mino))
                out.append((path, names[n]))
    return out

def main():
    os.makedirs(RUN, exist_ok=True); load_cfg()
    if not os.path.exists(FIFO): os.mkfifo(FIFO)
    ffd = os.open(FIFO, os.O_RDWR | os.O_NONBLOCK); os.chmod(FIFO, 0o666)
    x_connect()
    state["saved"] = state["brightness"] = int(rd(BL, str(cfg["default_brightness"])) or cfg["default_brightness"]) or cfg["default_brightness"]
    set_orientation(0, "initial"); wr(ORI_FILE, 0)
    ins = [(os.open(p, os.O_RDONLY | os.O_NONBLOCK), kind) for p, kind in find_inputs()]
    log("inputs:", [(k) for _, k in ins])
    fdkind = {fd: kind for fd, kind in ins}
    cand, cand_n = None, 0
    t_batt = t_stat = 0.0; buf = b""
    while True:
        r, _, _ = select.select([ffd] + [fd for fd, _ in ins], [], [], 0.1 if state["pwr_down"] else 0.4)
        now = time.time()
        for fd in r:
            if fd == ffd:
                try: buf += os.read(ffd, 512)
                except BlockingIOError: pass
                while b"\n" in buf:
                    line, buf = buf.split(b"\n", 1); command(line.decode(errors="replace"))
                continue
            try: data = os.read(fd, 24 * 32)
            except BlockingIOError: continue
            kind = fdkind[fd]
            for off in range(0, len(data) - 23, 24):
                _, _, typ, code, val = EVT.unpack_from(data, off)
                if kind == "hall" and typ == 5 and cfg["lid_off"]:           # EV_SW
                    closed = bool(val); state["lid_closed"] = closed; log("cover", "closed" if closed else "open")
                    screen("off" if closed else "on")
                elif kind == "key" and typ == 1 and code == 116:             # POWER button
                    if val == 1: state["pwr_down"] = now; state["pwr_long"] = False
                    elif val == 0 and state["pwr_down"]:
                        if not state["pwr_long"]:
                            if os.path.exists(RUN + "/powermenu-open"): subprocess.Popen(["xdotool", "key", "Return"], env=dict(os.environ, DISPLAY=":0"))
                            else: screen("toggle")
                        state["pwr_down"] = 0.0
                elif kind in ("touch", "key") and typ in (1, 3):
                    state["last_act"] = now
                    if state["screen"] != "on" and not state["lid_closed"] and (kind == "touch" or typ == 1):
                        screen("on")
                        if kind == "key": wr(RUN + "/wake-ts", "%.3f" % now)       # tells walpad-keys to swallow this press
        # auto-rotation
        if cfg["auto_rotate"] and not state["locked"] and state["screen"] != "off":
            g = read_gsensor()
            if g:
                o = decide(g[0], g[1])
                if o is not None and o != state["orientation"]:
                    cand_n = cand_n + 1 if o == cand else 1; cand = o
                    if cand_n >= cfg["rotate_samples"]: set_orientation(o, "accel x=%d y=%d" % (g[0], g[1])); cand_n = 0
                else: cand, cand_n = None, 0
        # POWER long press -> power menu
        if state["pwr_down"] and not state["pwr_long"] and now - state["pwr_down"] >= 1.0:
            state["pwr_long"] = True
            if state["screen"] != "on": screen("on")
            if not os.path.exists(RUN + "/powermenu-open"):
                subprocess.Popen(["walpad-powermenu.py"], env=dict(os.environ, DISPLAY=":0"), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        # inactivity
        idle = now - state["last_act"]
        if state["screen"] == "on" and idle > cfg["dim_after_s"] > 0: screen("dim")
        if state["screen"] in ("on", "dim") and idle > cfg["off_after_s"] > 0: screen("off")
        if now - t_batt > 10: battery_guard(); t_batt = now
        if now - t_stat > 2: write_status(); t_stat = now

if __name__ == "__main__":
    try: main()
    except KeyboardInterrupt: pass
    except Exception as e:
        log("fatal:", repr(e)); sys.exit(1)
