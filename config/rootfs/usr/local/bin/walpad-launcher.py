#!/usr/bin/env python3
"""WalPad home screen (Tk): status bar and big touch tiles.  Works with touch and with the hardware
keys (VOL+ = up, VOL- = down, POWER = select; see walpad-keys.py).  Creates /run/walpad-ok when the
window is up so the boot ramdisk knows the session is healthy."""
import json, os, subprocess, time, tkinter as tk
from tkinter import font as tkfont

ENV = dict(os.environ, DISPLAY=":0")
BL = "/sys/class/leds/lcd-backlight/brightness"

def rd(path, default=""):
    try: return open(path).read().strip()
    except OSError: return default

def battery():
    cap = rd("/sys/class/power_supply/battery/capacity", "-1")
    st = rd("/sys/class/power_supply/battery/status", "?")
    try: c = int(cap)
    except ValueError: c = -1
    est = ""
    if not 0 <= c <= 100:
        try:
            v = int(rd("/sys/class/power_supply/battery/voltage_now", "0"))
            v = v // 1000 if v > 100000 else v
            c = max(0, min(100, (v - 3300) * 100 // 1000)) if v > 2500 else -1; est = "~"
        except ValueError: c = -1
    return ("%s%d%%" % (est, c) if c >= 0 else "?%") + " " + st

def wifi():
    s = rd("/run/walpad/wifi-state", "off").split()
    if not s: return "Wi-Fi: off"
    if s[0] == "COMPLETED" and len(s) > 1: return "Wi-Fi: " + s[1]
    return "Wi-Fi: " + s[0].lower()

class Home(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("WalPad Home")
        self.configure(bg="#0b0e16")
        self.geometry("800x1280+0+0")      # openbox maximizes it into the visible area (portrait or landscape)
        self.f_big = tkfont.Font(family="DejaVu Sans", size=26, weight="bold")
        self.f_mid = tkfont.Font(family="DejaVu Sans", size=16)
        self.bar = tk.Label(self, bg="#161c2c", fg="#cdd5e6", font=self.f_mid, anchor="w", padx=16, pady=12)
        self.bar.pack(fill="x")
        tk.Label(self, text="WALPAD 8G", bg="#0b0e16", fg="#e8ecf4", font=tkfont.Font(family="DejaVu Sans", size=34, weight="bold")).pack(pady=(30, 10))
        grid = tk.Frame(self, bg="#0b0e16"); grid.pack(expand=True, fill="both", padx=24, pady=10)
        self.tiles = [
            ("Browser", "#22345a", lambda: self.run(["firefox-esr", "--new-instance", "about:blank"])),
            ("Terminal", "#28543f", lambda: self.run(["st", "-f", "DejaVu Sans Mono:size=14"])),
            ("Wi-Fi", "#62403a", lambda: self.run(["walpad-wifi-ui.py"])),
            ("Keyboard", "#4c3062", lambda: self.run(["walpad-keys-action", "keyboard"])),
            ("Brightness", "#3a4a70", self.bright),
            ("Rotation", "#35506a", lambda: self.run(["walpad-ctl", "rotate", "toggle"])),
            ("Screen off", "#5a3a3a", lambda: self.run(["walpad-ctl", "screen", "off"])),
            ("Power", "#7a2a2a", lambda: self.run(["walpad-powermenu.py"])),
        ]
        self.btns = []
        for i, (name, col, cmd) in enumerate(self.tiles):
            b = tk.Button(grid, text=name, bg=col, fg="#ffffff", activebackground=col, activeforeground="#ffffff",
                          font=self.f_big, relief="flat", bd=0, highlightthickness=6, highlightbackground="#0b0e16",
                          highlightcolor="#ffdc3c", command=cmd, takefocus=1)
            b.grid(row=i // 2, column=i % 2, sticky="nsew", padx=10, pady=10)
            self.btns.append(b)
        for c in range(2): grid.columnconfigure(c, weight=1, uniform="c")
        for r in range(4): grid.rowconfigure(r, weight=1, uniform="r")
        tk.Label(self, text="VOL+/-  move    POWER  select    hold VOL-  Home    hold VOL+  keyboard    hold POWER  screen",
                 bg="#0b0e16", fg="#7d879b", font=tkfont.Font(family="DejaVu Sans", size=11)).pack(pady=12)
        self.sel = 0
        self.bind("<Up>", lambda e: self.move(-1)); self.bind("<Down>", lambda e: self.move(1))
        self.bind("<Return>", lambda e: self.btns[self.sel].invoke())
        self.move(0)
        self.tick()
        self.after(500, self.healthy)

    def move(self, d):
        self.sel = (self.sel + d) % len(self.btns)
        self.btns[self.sel].focus_set()
        for i, b in enumerate(self.btns): b.configure(highlightbackground="#ffdc3c" if i == self.sel else "#0b0e16")

    def run(self, cmd):
        try: subprocess.Popen(cmd, env=dict(ENV, MOZ_DISABLE_CONTENT_SANDBOX="1", MOZ_WEBRENDER="0", LIBGL_ALWAYS_SOFTWARE="1"),
                              stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except OSError as e: self.bar.configure(text="cannot start %s: %s" % (cmd[0], e))

    def bright(self):
        cur = int(self.status().get("brightness", 120) or 120)
        nxt = 40 if cur >= 220 else cur + 60
        self.run(["walpad-ctl", "brightness", str(nxt)]); self.bar.configure(text="brightness %d" % nxt)

    def status(self):
        try: return json.load(open("/run/walpad/status.json"))
        except Exception: return {}

    def tick(self):
        st = self.status(); b = st.get("battery") or {}
        if b:
            pct = b.get("percent", -1); bat = ("%s%d%%" % ("~" if b.get("estimated") else "", pct) if pct >= 0 else "?%") + " " + str(b.get("status", ""))
        else: bat = battery()
        rot = "rot %d%s" % (st.get("orientation", 0), " (locked)" if st.get("rotation_locked") else "") if st else ""
        self.bar.configure(text="%s    %s    %s    %s" % (time.strftime("%H:%M"), bat, wifi(), rot))
        self.after(2000, self.tick)

    def healthy(self):
        try: open("/run/walpad-ok", "w").write("ok\n")
        except OSError: pass

if __name__ == "__main__":
    Home().mainloop()
