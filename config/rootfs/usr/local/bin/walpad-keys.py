#!/usr/bin/env python3
"""walpad-keys: gesture engine for the two VOLUME buttons (POWER belongs to walpad-devd).

Per button: single / double / triple / quad tap, long press (optionally repeating while held);
both buttons together = chord.  Every gesture runs an action from /etc/walpad/keys.json:

  key:NAME        send an X key (xdotool), e.g. key:Up  key:Prior  key:ctrl+Tab  key:alt+Left
  ctl:ARGS        command for walpad-devd: ctl:brightness +40  ctl:rotate toggle  ctl:screen off
  launch:APP-ID   start an app from the /etc/walpad/apps.json allowlist, e.g. launch:browser
  run:COMMAND     start a program, e.g. run:st  (Python runtime only; the Rust input subsystem rejects it)
  home            bring the Home launcher to the front
  keyboard        show/hide the on-screen keyboard
  screenshot      save the screen to /root/screenshots/shot-<time>.xwd
  volume:+5 / -5  ALSA master volume in percent (needs working audio)
  powermenu       open the power menu
  none            do nothing
A press that only woke the screen (walpad-devd logs /run/walpad/wake-ts) is swallowed."""
import glob, json, os, select, shlex, shutil, struct, subprocess, time

EVT = struct.Struct("llHHi")
KEYS = {114: "vol_down", 115: "vol_up"}
RUN = "/run/walpad"
ENV = dict(os.environ, DISPLAY=":0")
DEFAULT = {"tap_window_ms": 350, "long_ms": 600, "chord_ms": 160, "repeat_ms": 140,
           "vol_up": {"single": "key:Up", "double": "key:Prior", "triple": "ctl:brightness +40", "quad": "none", "long": "keyboard"},
           "vol_down": {"single": "key:Down", "double": "key:Next", "triple": "ctl:brightness -40", "quad": "none", "long": "home"},
           "both": {"press": "ctl:rotate toggle"}}
cfg = dict(DEFAULT)
APPS_FILE = "/etc/walpad/apps.json"
# environment of every launched app (docs/rust/CONTRACTS.md)
APP_ENV = {"DISPLAY": ":0", "HOME": "/root", "XDG_RUNTIME_DIR": "/run/walpad", "LIBGL_ALWAYS_SOFTWARE": "1",
           "MOZ_DISABLE_CONTENT_SANDBOX": "1", "MOZ_DISABLE_GMP_SANDBOX": "1", "MOZ_DISABLE_RDD_SANDBOX": "1",
           "MOZ_DISABLE_SOCKET_PROCESS_SANDBOX": "1", "MOZ_WEBRENDER": "0", "MOZ_ACCELERATED": "0",
           "GTK_A11Y": "none", "NO_AT_BRIDGE": "1"}

def log(*a): print(time.strftime("%H:%M:%S"), *a, flush=True)
def load():
    global cfg
    cfg = json.loads(json.dumps(DEFAULT))
    try:
        user = json.load(open("/etc/walpad/keys.json"))
        for k, v in user.items():
            if isinstance(v, dict) and isinstance(cfg.get(k), dict): cfg[k].update(v)
            else: cfg[k] = v
    except Exception as e: log("keys.json:", e, "- using defaults")

def sh(*cmd, wait=False):
    try:
        p = subprocess.Popen(list(cmd), env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if wait: p.wait(timeout=8)
    except Exception as e: log("run failed:", cmd, e)

def spawn(cmd, env):
    subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)

def launch(app_id):
    """Start exactly the `exec` list of the allowlisted app; the id is the only thing keys.json supplies."""
    try:
        apps = json.load(open(APPS_FILE))
        app = next(a for a in apps if isinstance(a, dict) and a.get("id") == app_id)
        cmd = app["exec"]
        if not (isinstance(cmd, list) and cmd and all(isinstance(c, str) for c in cmd)): raise ValueError("bad exec")
    except StopIteration: log("launch: unknown app id", repr(app_id), "- ignored"); return
    except Exception as e: log("launch: cannot read", APPS_FILE, e, "- ignored"); return
    env = dict(os.environ, **APP_ENV)
    env.update({str(k): str(v) for k, v in (app.get("env") or {}).items()})
    try: spawn(cmd, env)
    except Exception as e: log("launch failed:", app_id, e)

def keyboard_toggle():
    if subprocess.call(["pgrep", "-f", "matchbox-keyboard"], stdout=subprocess.DEVNULL) == 0:
        subprocess.call(["pkill", "-f", "matchbox-keyboard"])
    else:
        sh("matchbox-keyboard"); time.sleep(1.5); ctl("keyboard")

def ctl(args):
    try:
        fd = os.open(RUN + "/devd.cmd", os.O_WRONLY | os.O_NONBLOCK); os.write(fd, (args + "\n").encode()); os.close(fd)
    except OSError as e: log("devd not reachable:", e)

def do(action):
    if not action or action == "none": return
    log("action:", action)
    kind, _, arg = action.partition(":")
    if kind == "key": sh("xdotool", "key", arg)
    elif kind == "ctl": ctl(arg)
    elif kind == "launch": launch(arg.strip())
    elif kind == "run": sh(*shlex.split(arg))
    elif action == "home": sh("xdotool", "search", "--name", "WalPad Home", "windowactivate")
    elif action == "keyboard": keyboard_toggle()
    elif action == "powermenu": sh("walpad-powermenu.py")
    elif action == "screenshot":
        os.makedirs("/root/screenshots", exist_ok=True)
        try: shutil.copy(RUN + "/xfb/Xvfb_screen0", "/root/screenshots/shot-%s.xwd" % time.strftime("%Y%m%d-%H%M%S"))
        except OSError as e: log("screenshot failed:", e)
    elif kind == "volume": sh("amixer", "-q", "sset", "Master", arg.lstrip("+") + "%+" if arg.startswith("+") else arg.lstrip("-") + "%-")

def wake_after(t):
    try: return float(open(RUN + "/wake-ts").read()) >= t - 0.05
    except (OSError, ValueError): return False

class Key:
    def __init__(self, name): self.name = name; self.down = False; self.t = 0.0; self.long = False; self.taps = 0; self.deadline = 0.0; self.mute = False; self.last_rep = 0.0

def find():
    out = []
    for d in glob.glob("/sys/class/input/input*"):
        try: n = open(d + "/name").read().strip()
        except OSError: continue
        if n in ("mtk-kpd", "mtk-pmic-keys"):
            for e in glob.glob(d + "/event*"):
                path = "/dev/input/" + os.path.basename(e)
                if not os.path.exists(path):
                    maj, mino = map(int, open(e + "/dev").read().strip().split(":")); os.mknod(path, 0o660 | 0o020000, os.makedev(maj, mino))
                out.append(path)
    return out

def main():
    load()
    paths = []
    for _ in range(60):
        paths = find()
        if paths: break
        time.sleep(1)
    log("devices", paths, "config", {k: cfg[k] for k in ("tap_window_ms", "long_ms")})
    fds = [os.open(p, os.O_RDONLY | os.O_NONBLOCK) for p in paths]
    keys = {115: Key("vol_up"), 114: Key("vol_down")}
    chord_until_release = False
    while True:
        r, _, _ = select.select(fds, [], [], 0.04)
        now = time.time()
        for fd in r:
            try: data = os.read(fd, 24 * 32)
            except BlockingIOError: continue
            for off in range(0, len(data) - 23, 24):
                _, _, typ, code, val = EVT.unpack_from(data, off)
                if typ != 1 or code not in keys: continue
                k = keys[code]; other = keys[115 if code == 114 else 114]
                if val == 1:
                    k.down = True; k.t = now; k.long = False; k.mute = False; k.last_rep = now
                    if other.down and now - other.t <= cfg["chord_ms"] / 1000.0 and not other.long:
                        k.mute = other.mute = True; k.taps = other.taps = 0; chord_until_release = True
                        do(cfg.get("both", {}).get("press", "none"))
                elif val == 0 and k.down:
                    k.down = False
                    if not (k.mute or k.long) and not wake_after(k.t):
                        k.taps += 1; k.deadline = now + cfg["tap_window_ms"] / 1000.0
                    k.mute = False
        for k in keys.values():
            c = cfg.get(k.name, {})
            if k.down and not k.long and not k.mute and now - k.t >= cfg["long_ms"] / 1000.0:
                k.long = True; k.taps = 0
                if not wake_after(k.t): do(c.get("long", "none"))
            if k.down and k.long and c.get("hold_repeat") and now - k.last_rep >= cfg["repeat_ms"] / 1000.0:
                k.last_rep = now; do(c.get("long", "none"))
            if k.taps and not k.down and now >= k.deadline:
                name = {1: "single", 2: "double", 3: "triple"}.get(k.taps, "quad"); k.taps = 0
                do(c.get(name, "none"))

if __name__ == "__main__":
    main()
