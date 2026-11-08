#!/usr/bin/env python3
"""Wi-Fi settings (Tk): scan, pick a network, type the password on a built-in on-screen keyboard,
connect and save.  Talks to wpa_supplicant through wpa_cli (config is saved by update_config=1)."""
import os, subprocess, time, tkinter as tk
from tkinter import font as tkfont

IFACE = "wlan0"
CTRL = "/run/wpa_supplicant"

def wcli(*args, timeout=15):
    try:
        r = subprocess.run(["wpa_cli", "-p", CTRL, "-i", IFACE] + list(args), capture_output=True, text=True, timeout=timeout)
        return r.stdout.strip()
    except Exception as e:
        return "ERR " + str(e)

def status():
    d = {}
    for line in wcli("status").splitlines():
        if "=" in line:
            k, v = line.split("=", 1); d[k] = v
    return d

def scan_results():
    wcli("scan"); time.sleep(4)
    nets = {}
    for line in wcli("scan_results").splitlines()[1:]:
        p = line.split("\t")
        if len(p) < 5 or not p[4]: continue
        ssid = p[4].encode("latin-1", "ignore").decode("utf-8", "replace")
        sig = int(p[2]) if p[2].lstrip("-").isdigit() else -100
        sec = "WPA" if "WPA" in p[3] or "RSN" in p[3] else ("WEP" if "WEP" in p[3] else "open")
        if ssid not in nets or sig > nets[ssid][0]: nets[ssid] = (sig, sec)
    return sorted(((s, v[0], v[1]) for s, v in nets.items()), key=lambda t: -t[1])

class Keyboard(tk.Frame):
    ROWS = ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"]
    SYM = ["!@#$%^&*()", "-_=+[]{}/", ".,:;'\"?\\|~", "<>`"]
    def __init__(self, master, target):
        super().__init__(master, bg="#0b0e16"); self.target = target; self.shift = False; self.sym = False
        self.f = tkfont.Font(family="DejaVu Sans", size=20, weight="bold"); self.build()
    def build(self):
        for w in self.winfo_children(): w.destroy()
        rows = self.SYM if self.sym else self.ROWS
        for r in rows:
            fr = tk.Frame(self, bg="#0b0e16"); fr.pack(fill="x", pady=3)
            for ch in r:
                c = ch.upper() if (self.shift and not self.sym) else ch
                tk.Button(fr, text=c, font=self.f, bg="#22345a", fg="white", relief="flat",
                          command=lambda c=c: self.target.insert("insert", c)).pack(side="left", expand=True, fill="both", padx=2, ipady=10)
        fr = tk.Frame(self, bg="#0b0e16"); fr.pack(fill="x", pady=3)
        tk.Button(fr, text="shift", font=self.f, bg="#3a4a70", fg="white", relief="flat", command=self.toggle_shift).pack(side="left", padx=2, ipady=10, ipadx=6)
        tk.Button(fr, text="123" if not self.sym else "abc", font=self.f, bg="#3a4a70", fg="white", relief="flat", command=self.toggle_sym).pack(side="left", padx=2, ipady=10, ipadx=6)
        tk.Button(fr, text="space", font=self.f, bg="#22345a", fg="white", relief="flat", command=lambda: self.target.insert("insert", " ")).pack(side="left", expand=True, fill="x", padx=2, ipady=10)
        tk.Button(fr, text="<-", font=self.f, bg="#6a3030", fg="white", relief="flat", command=lambda: self.target.delete(max(0, self.target.index("insert") - 1), "insert")).pack(side="left", padx=2, ipady=10, ipadx=10)
    def toggle_shift(self): self.shift = not self.shift; self.build()
    def toggle_sym(self): self.sym = not self.sym; self.build()

class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("WalPad Wi-Fi"); self.configure(bg="#0b0e16"); self.geometry("800x1280+0+0")
        try: self.attributes("-fullscreen", True)
        except tk.TclError: pass
        self.f = tkfont.Font(family="DejaVu Sans", size=18); self.fb = tkfont.Font(family="DejaVu Sans", size=22, weight="bold")
        self.info = tk.Label(self, bg="#161c2c", fg="#cdd5e6", font=self.f, anchor="w", padx=14, pady=12); self.info.pack(fill="x")
        bar = tk.Frame(self, bg="#0b0e16"); bar.pack(fill="x", pady=6)
        for text, cmd, col in (("< Back", self.destroy, "#3a4a70"), ("Scan", self.scan, "#28543f"), ("Saved", self.saved, "#4c3062")):
            tk.Button(bar, text=text, font=self.fb, bg=col, fg="white", relief="flat", command=cmd).pack(side="left", expand=True, fill="x", padx=6, ipady=12)
        self.lb = tk.Listbox(self, font=self.f, bg="#161c2c", fg="#e8ecf4", selectbackground="#2a6ad0", activestyle="none", height=10, bd=0, highlightthickness=0)
        self.lb.pack(fill="both", expand=True, padx=10, pady=6); self.lb.bind("<<ListboxSelect>>", self.pick)
        self.form = tk.Frame(self, bg="#0b0e16")
        self.msg = tk.Label(self, bg="#0b0e16", fg="#ffdc3c", font=self.f); self.msg.pack(fill="x", pady=4)
        self.nets = []; self.mode = "scan"
        self.refresh(); self.after(500, self.scan)

    def refresh(self):
        st = status(); self.info.configure(text="%s   %s   %s" % (st.get("wpa_state", "no supplicant"), st.get("ssid", ""), st.get("ip_address", "")))
        self.after(3000, self.refresh)

    def scan(self):
        self.mode = "scan"; self.msg.configure(text="scanning..."); self.update_idletasks()
        self.nets = scan_results(); self.lb.delete(0, "end")
        for ssid, sig, sec in self.nets:
            self.lb.insert("end", "%-28s %4d dBm  %s" % (ssid[:28], sig, sec))
        self.msg.configure(text="%d networks - tap one" % len(self.nets))

    def saved(self):
        self.mode = "saved"; self.lb.delete(0, "end"); self.known = []
        for line in wcli("list_networks").splitlines()[1:]:
            p = line.split("\t")
            if len(p) >= 2: self.known.append((p[0], p[1])); self.lb.insert("end", "%s   (tap to forget)" % p[1])
        self.msg.configure(text="%d saved networks" % len(self.known))

    def pick(self, _):
        sel = self.lb.curselection()
        if not sel: return
        if self.mode == "saved":
            nid = self.known[sel[0]][0]; wcli("remove_network", nid); wcli("save_config"); self.saved(); return
        ssid, sig, sec = self.nets[sel[0]]
        self.ask(ssid, sec)

    def ask(self, ssid, sec):
        for w in self.form.winfo_children(): w.destroy()
        self.form.pack(fill="x", padx=10, pady=6, before=self.msg)
        if sec == "open":
            self.connect(ssid, ""); self.form.pack_forget(); return
        tk.Label(self.form, text="Password for " + ssid, bg="#0b0e16", fg="#e8ecf4", font=self.f).pack(anchor="w")
        self.pw = tk.Entry(self.form, font=self.fb, bg="#161c2c", fg="white", insertbackground="white", relief="flat")
        self.pw.pack(fill="x", ipady=8, pady=4)
        Keyboard(self.form, self.pw).pack(fill="x")
        row = tk.Frame(self.form, bg="#0b0e16"); row.pack(fill="x", pady=6)
        tk.Button(row, text="Cancel", font=self.fb, bg="#6a3030", fg="white", relief="flat", command=self.form.pack_forget).pack(side="left", expand=True, fill="x", padx=6, ipady=10)
        tk.Button(row, text="Connect", font=self.fb, bg="#28543f", fg="white", relief="flat",
                  command=lambda: (self.connect(ssid, self.pw.get()), self.form.pack_forget())).pack(side="left", expand=True, fill="x", padx=6, ipady=10)

    def connect(self, ssid, pw):
        self.msg.configure(text="connecting to %s ..." % ssid); self.update_idletasks()
        nid = wcli("add_network").splitlines()[-1].strip()
        if not nid.isdigit(): self.msg.configure(text="wpa_supplicant not running (Wi-Fi stack down?)"); return
        wcli("set_network", nid, "ssid", '"%s"' % ssid.replace('"', '\\"'))
        if pw: wcli("set_network", nid, "psk", '"%s"' % pw.replace('"', '\\"'))
        else: wcli("set_network", nid, "key_mgmt", "NONE")
        wcli("enable_network", nid); wcli("select_network", nid); wcli("save_config")
        for _ in range(25):
            time.sleep(1); self.update()
            if status().get("wpa_state") == "COMPLETED": self.msg.configure(text="connected to " + ssid); return
        self.msg.configure(text="could not connect (wrong password?)")

if __name__ == "__main__":
    App().mainloop()
