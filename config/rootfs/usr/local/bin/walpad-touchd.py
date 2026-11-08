#!/usr/bin/env python3
"""walpad-touchd: read the Silead GSL touch controller (multitouch "protocol A": ABS_MT_POSITION_X/Y +
BTN_TOUCH, no slots) and drive the X pointer through the XTEST extension (the X server here is Xvfb,
the kernel has no VT so a real Xorg cannot run).  Applies the calibration in /etc/walpad/touch.json.
A finger is considered lifted after 150 ms without events, so a flapping BTN_TOUCH cannot cause taps."""
import glob, json, os, select, struct, sys, time

EVT = struct.Struct("llHHi")
EV_SYN, EV_KEY, EV_ABS = 0, 1, 3
BTN_TOUCH = 0x14A
W, H = 800, 1280

def log(*a): print(time.strftime("%H:%M:%S"), *a, flush=True)

def find_touch():
    for _ in range(60):
        for d in glob.glob("/sys/class/input/input*"):
            try: n = open(d + "/name").read().strip()
            except OSError: continue
            if n in ("mtk-tpd", "gsl3680", "touchscreen"):
                for e in glob.glob(d + "/event*"):
                    path = "/dev/input/" + os.path.basename(e)
                    if not os.path.exists(path):
                        maj, mino = map(int, open(e + "/dev").read().strip().split(":"))
                        os.mknod(path, 0o660 | 0o020000, os.makedev(maj, mino))
                    return path
        time.sleep(0.5)
    return None

def load_cal():
    c = {"swap": False, "flipx": False, "flipy": False, "xmin": 0, "xmax": W, "ymin": 0, "ymax": H}
    try: c.update(json.load(open("/etc/walpad/touch.json")))
    except Exception as e: log("no calibration file:", e)
    return c

_ori = [0, 0.0]
def orientation():
    """Current orientation from walpad-devd (re-read at most every 0.3 s)."""
    now = time.time()
    if now - _ori[1] > 0.3:
        _ori[1] = now
        try: v = int(open("/run/walpad/orientation").read().strip()); _ori[0] = v if v in (0, 90, 180, 270) else 0
        except (OSError, ValueError): _ori[0] = 0
    return _ori[0]

def to_logical(ox, oy):
    """Panel (800x1280, native) -> position on the 1280x1280 X screen, matching fbmirror's rotation."""
    o = orientation()
    if o == 180: return W - 1 - ox, H - 1 - oy
    if o == 90: return oy, W - 1 - ox
    if o == 270: return H - 1 - oy, ox
    return ox, oy

def open_x():
    from Xlib import display, X
    from Xlib.ext import xtest
    for _ in range(60):
        try: return display.Display(":0"), X, xtest
        except Exception: time.sleep(0.5)
    raise RuntimeError("cannot open X display :0")

def main():
    d, X, xtest = open_x()
    log("connected to X")
    src = find_touch()
    if not src: log("no touch controller found"); time.sleep(3600); return
    cal = load_cal(); log("reading", src, "calibration", cal)
    rd = os.open(src, os.O_RDONLY | os.O_NONBLOCK)
    def mapv(v, lo, hi, size, flip):
        r = (v - lo) * (size - 1) / max(1, (hi - lo)); r = min(max(r, 0), size - 1)
        return int(size - 1 - r) if flip else int(r)
    x = y = None; down = False; last = 0.0; moved = False
    while True:
        r, _, _ = select.select([rd], [], [], 0.05 if down else None)
        now = time.time()
        if r:
            try: data = os.read(rd, 24 * 64)
            except BlockingIOError: data = b""
            for off in range(0, len(data) - 23, 24):
                _, _, typ, code, val = EVT.unpack_from(data, off)
                if typ == EV_ABS and code in (53, 0): x = val; moved = True; last = now
                elif typ == EV_ABS and code in (54, 1): y = val; moved = True; last = now
                elif typ == EV_KEY and code == BTN_TOUCH and val == 1: last = now; moved = True
                elif typ == EV_SYN and code == 0 and moved and x is not None and y is not None:
                    moved = False
                    px, py = (y, x) if cal["swap"] else (x, y)
                    ox = mapv(px, cal["xmin"], cal["xmax"], W, cal["flipx"])
                    oy = mapv(py, cal["ymin"], cal["ymax"], H, cal["flipy"])
                    lx, ly = to_logical(ox, oy)
                    xtest.fake_input(d, X.MotionNotify, x=lx, y=ly)
                    if not down: xtest.fake_input(d, X.ButtonPress, 1); down = True
                    d.sync()
        if down and now - last > 0.15:
            xtest.fake_input(d, X.ButtonRelease, 1); d.sync(); down = False; x = y = None

if __name__ == "__main__":
    try: main()
    except Exception as e:
        log("fatal:", repr(e)); sys.exit(1)
