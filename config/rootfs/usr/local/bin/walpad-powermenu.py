#!/usr/bin/env python3
"""Power menu (Tk): Power off / Reboot / Screen off / Rotation lock / Cancel.
Touch works; with the buttons: VOL single tap = up/down, POWER short = select (walpad-devd sends Return
while /run/walpad/powermenu-open exists)."""
import os, subprocess, tkinter as tk
from tkinter import font as tkfont

FLAG = "/run/walpad/powermenu-open"
def ctl(c):
    try: open("/run/walpad/devd.cmd", "w").write(c + "\n")
    except OSError: pass
def halt(cmd):
    os.sync(); subprocess.Popen(cmd)

class Menu(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("WalPad Power Menu"); self.configure(bg="#0b0e16")
        try: open(FLAG, "w").write("1")
        except OSError: pass
        f = tkfont.Font(family="DejaVu Sans", size=26, weight="bold")
        tk.Label(self, text="Power", bg="#0b0e16", fg="#e8ecf4", font=tkfont.Font(family="DejaVu Sans", size=32, weight="bold")).pack(pady=24)
        items = [("Power off", "#7a2a2a", lambda: halt(["poweroff", "-f"])), ("Reboot", "#6a4a20", lambda: halt(["reboot", "-f"])),
                 ("Screen off", "#3a4a70", lambda: (ctl("screen off"), self.close())), ("Rotation lock", "#4c3062", lambda: (ctl("rotate toggle"), self.close())),
                 ("Cancel", "#28543f", self.close)]
        self.btns = []
        for text, col, cmd in items:
            b = tk.Button(self, text=text, font=f, bg=col, fg="white", activebackground=col, relief="flat", bd=0, highlightthickness=6,
                          highlightbackground="#0b0e16", highlightcolor="#ffdc3c", command=cmd)
            b.pack(fill="x", padx=30, pady=10, ipady=18); self.btns.append(b)
        self.sel = 4
        self.bind("<Up>", lambda e: self.move(-1)); self.bind("<Down>", lambda e: self.move(1))
        self.bind("<Return>", lambda e: self.btns[self.sel].invoke()); self.bind("<Escape>", lambda e: self.close())
        self.move(0); self.after(20000, self.close)
        self.protocol("WM_DELETE_WINDOW", self.close)
    def move(self, d):
        self.sel = (self.sel + d) % len(self.btns); self.btns[self.sel].focus_set()
        for i, b in enumerate(self.btns): b.configure(highlightbackground="#ffdc3c" if i == self.sel else "#0b0e16")
    def close(self):
        try: os.unlink(FLAG)
        except OSError: pass
        self.destroy()

if __name__ == "__main__":
    Menu().mainloop()
    try: os.unlink(FLAG)
    except OSError: pass
