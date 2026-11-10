//! The shell event loop: X events + core events -> gestures -> app -> render -> present. Redraws
//! only when something changed (input, core status, the minute boundary); writes /run/walpad-ok
//! after the first frame is on screen (the boot ramdisk health check).
use crate::app::{load_catalog, read_clock_24h, read_pinned, App, Cmd, WallTime};
use crate::config::read_devd;
use crate::core_client::{CoreClient, CoreEvent};
use crate::dock::{load_wm_classes, Dock};
use crate::gesture::{Gesture, Recogniser};
use crate::ime::{FocusKbd, ImEvent, ImServer, KbdAction};
use crate::keyboard::SysKeyboard;
use crate::mousepad::MousePad;
use crate::settings_screen::gather;
use crate::window::{WinEvent, Window};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::ShellInput;
use walpad_ui::components::Painter;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};

pub struct Options {
    pub root: Root,
    /// X socket, default <root>/tmp/.X11-unix/X0.
    pub x_socket: Option<PathBuf>,
    /// Core control socket, default <root>/run/walpad/control.sock.
    pub core_socket: Option<PathBuf>,
    pub fonts_dir: Option<PathBuf>,
}

pub struct Shell {
    win: Window,
    /// The system dock (None when its X connection could not be opened or died).
    dock: Option<Dock>,
    /// The system keyboard (None when its X connection could not be opened or died).
    kbd: Option<SysKeyboard>,
    /// The mouse pad (None when its X connection could not be opened or died).
    pad: Option<MousePad>,
    /// The XIM server whose focus events drive the keyboard (None when it could not start or
    /// its X connection died: the terminal then gets the keyboard at start, as before).
    ime: Option<ImServer>,
    /// Text-input focus -> keyboard visibility.
    focus: FocusKbd,
    core: CoreClient,
    canvas: Canvas,
    fonts: Fonts,
    pub app: App,
    rec: Recogniser,
    root: Root,
    clock_24h: bool,
    started: Instant,
    next_minute: Instant,
    dirty: bool,
    need_refresh: bool,
    first_frame: bool,
    pub frames: u64,
}

/// Initial size from the orientation file (the window manager resizes it to the visible area).
pub fn initial_size(root: &Root) -> (i32, i32) {
    let o = std::fs::read_to_string(root.p("/run/walpad/orientation"))
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .unwrap_or(0);
    if matches!(o, 90 | 270) {
        (1280, 800)
    } else {
        (800, 1280)
    }
}

impl Shell {
    pub fn new(opts: Options) -> Result<Shell, String> {
        let fonts = match &opts.fonts_dir {
            Some(d) => Fonts::load(d),
            None => Fonts::load_default(),
        }
        .map_err(|e| format!("fonts: {e:?}"))?;
        let (w, h) = initial_size(&opts.root);
        let sock = opts
            .x_socket
            .clone()
            .unwrap_or_else(|| opts.root.p("/tmp/.X11-unix/X0"));
        let win = Window::open(&sock, w, h).map_err(|e| format!("X: {}", e.0))?;
        let dock = match Dock::open(&sock, win.win, load_wm_classes(&opts.root), (w, h)) {
            Ok(d) => Some(d),
            Err(e) => {
                eprintln!("walpad-shell: system dock disabled: {}", e.0);
                None
            }
        };
        let kbd = match SysKeyboard::open(&sock, win.win, (w, h)) {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("walpad-shell: system keyboard disabled: {}", e.0);
                None
            }
        };
        let pad = match MousePad::open(&sock, win.win, (w, h)) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("walpad-shell: mouse pad disabled: {}", e.0);
                None
            }
        };
        let ime = match ImServer::open(&sock) {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("walpad-shell: input method (XIM) disabled: {}", e.0);
                None
            }
        };
        let canvas =
            Canvas::new(w as u32, h as u32, BG).ok_or("cannot allocate the frame buffer")?;
        let clock_24h = read_clock_24h(&opts.root);
        let (wall, to_minute) = WallTime::now(clock_24h);
        let core = CoreClient::new(
            opts.core_socket
                .clone()
                .unwrap_or_else(|| opts.root.p(walpad_proto::SOCKET_PATH)),
        );
        Ok(Shell {
            win,
            dock,
            kbd,
            pad,
            ime,
            focus: FocusKbd::new(),
            core,
            canvas,
            fonts,
            app: App::new(w, h, load_catalog(&opts.root), wall)
                .with_pinned(read_pinned(&opts.root)),
            rec: Recogniser::new(),
            root: opts.root,
            clock_24h,
            started: Instant::now(),
            next_minute: Instant::now() + Duration::from_millis(to_minute),
            dirty: true,
            need_refresh: true,
            first_frame: true,
            frames: 0,
        })
    }

    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn send(&mut self, cmds: Vec<Cmd>) {
        for c in cmds {
            // the system keyboard is ours: apply locally, then report the state to the core
            let kbd = match c {
                Cmd::KeyboardToggle => Some(!self.keyboard_visible()),
                Cmd::KeyboardShow => Some(true),
                Cmd::KeyboardHide => Some(false),
                _ => None,
            };
            if let Some(on) = kbd {
                self.set_keyboard(on, true);
                continue;
            }
            // so is the mouse pad (no core command)
            match c {
                Cmd::MousePadShow => {
                    self.set_pad(true);
                    continue;
                }
                Cmd::MousePadHide => {
                    self.set_pad(false);
                    continue;
                }
                _ => {}
            }
            let (name, args) = c.request();
            if let Some(id) = self.core.send(name, args) {
                self.app.note_sent(id, &c);
            }
        }
    }

    pub fn keyboard_visible(&self) -> bool {
        self.kbd.as_ref().is_some_and(|k| k.visible())
    }

    /// Show/hide the system keyboard by hand (dock button, core command, Home, the mouse pad),
    /// move the dock above it, and (`report`) tell the core, so its `keyboard toggle` (hardware
    /// keys, walpad-ctl) knows the real state. Requests that came from the core are not reported
    /// back.
    fn set_keyboard(&mut self, on: bool, report: bool) {
        self.focus.manual(on);
        self.show_keyboard(on, report);
    }

    /// What the focus state machine decided (it already knows).
    fn apply_focus(&mut self, a: Option<KbdAction>) {
        match a {
            Some(KbdAction::Show) => self.show_keyboard(true, true),
            Some(KbdAction::Hide) => self.show_keyboard(false, true),
            None => {}
        }
    }

    fn show_keyboard(&mut self, on: bool, report: bool) {
        // the keyboard and the mouse pad share the bottom of the screen: one replaces the other
        if on {
            self.set_pad(false);
        }
        let Some(mut k) = self.kbd.take() else {
            return;
        };
        match k.set_visible(on, &mut self.fonts) {
            Ok(()) => self.kbd = Some(k),
            Err(e) => eprintln!("walpad-shell: system keyboard disabled: {}", e.0),
        }
        self.bottom_changed();
        if report {
            self.report_keyboard();
        }
    }

    pub fn mouse_pad_visible(&self) -> bool {
        self.pad.as_ref().is_some_and(|p| p.visible())
    }

    /// Show/hide the mouse pad; showing it puts the keyboard away (and tells the core).
    fn set_pad(&mut self, on: bool) {
        if on && self.keyboard_visible() {
            self.set_keyboard(false, true);
        }
        let Some(mut p) = self.pad.take() else {
            return;
        };
        match p.set_visible(on, &mut self.fonts) {
            Ok(()) => self.pad = Some(p),
            Err(e) => eprintln!("walpad-shell: mouse pad disabled: {}", e.0),
        }
        self.bottom_changed();
    }

    /// The keyboard or the mouse pad was shown, hidden or rotated: the dock sits directly above
    /// whichever is up.
    fn bottom_changed(&mut self) {
        let kbd_h = match (&self.kbd, &self.pad) {
            (Some(k), _) if k.visible() => k.height(),
            (_, Some(p)) if p.visible() => p.height(),
            _ => 0,
        };
        if let Some(d) = &mut self.dock {
            if let Err(e) = d.set_keyboard_height(kbd_h) {
                eprintln!("walpad-shell: system dock: {}", e.0);
            }
        }
    }

    fn report_keyboard(&mut self) {
        let c = if self.keyboard_visible() {
            Cmd::KeyboardShow
        } else {
            Cmd::KeyboardHide
        };
        let (name, args) = c.request();
        self.core.send(name, args);
    }

    fn apply_gestures(&mut self, gs: Vec<Gesture>) {
        let now = self.now_ms();
        for g in gs {
            let out = self.app.on_gesture(&g, now);
            self.dirty |= out.dirty;
            self.need_refresh |= out.refresh;
            self.send(out.cmds);
        }
    }

    /// Re-read the configuration files and the system facts the Device/Settings screens show.
    fn reload(&mut self) {
        self.need_refresh = false;
        let devd = read_devd(&self.root);
        let clock_24h = read_clock_24h(&self.root);
        self.dirty |= self.app.set_config(devd, clock_24h);
        if clock_24h != self.clock_24h {
            self.clock_24h = clock_24h;
            let (wall, to_minute) = WallTime::now(clock_24h);
            self.next_minute = Instant::now() + Duration::from_millis(to_minute);
            self.dirty |= self.app.set_wall(wall);
        }
        self.dirty |= self.app.set_sysinfo(gather(&self.root));
    }

    fn resize(&mut self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if let Some(c) = Canvas::new(w as u32, h as u32, BG) {
            self.canvas = c;
            self.app.resize(w, h);
            self.dirty = true;
            if let Some(mut k) = self.kbd.take() {
                match k.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.kbd = Some(k),
                    Err(e) => eprintln!("walpad-shell: system keyboard disabled: {}", e.0),
                }
            }
            if let Some(mut p) = self.pad.take() {
                match p.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.pad = Some(p),
                    Err(e) => eprintln!("walpad-shell: mouse pad disabled: {}", e.0),
                }
            }
            self.bottom_changed();
            if let Some(d) = &mut self.dock {
                if let Err(e) = d.set_shell_size(w, h) {
                    eprintln!("walpad-shell: system dock: {}", e.0);
                }
            }
        }
    }

    fn redraw(&mut self) -> Result<(), String> {
        self.app.render(&mut Painter {
            cv: &mut self.canvas,
            fonts: &mut self.fonts,
        });
        self.win
            .present(&self.canvas, self.canvas.bounds())
            .map_err(|e| format!("present: {}", e.0))?;
        self.frames += 1;
        if self.first_frame {
            self.first_frame = false;
            // the boot ramdisk kills the session and falls back to padui if this is missing after 150 s
            let _ = std::fs::write(self.root.p("/run/walpad-ok"), "ok\n");
        }
        self.dirty = false;
        Ok(())
    }

    /// One loop iteration. Returns Err when the X connection is gone (the supervisor restarts us).
    pub fn step(&mut self, max_wait: Duration) -> Result<(), String> {
        let events = self.win.poll_events().map_err(|e| e.0)?;
        for ev in events {
            match ev {
                WinEvent::Input(i) => {
                    let now = self.now_ms();
                    let gs = self.rec.feed(i, now);
                    self.apply_gestures(gs);
                }
                WinEvent::Resize(w, h) => self.resize(w, h),
                WinEvent::Expose => self.dirty = true,
                WinEvent::Key(_) => {}
            }
        }
        let now = self.now_ms();
        let gs = self.rec.tick(now);
        self.apply_gestures(gs);
        if let Some(mut d) = self.dock.take() {
            match d.poll(&mut self.fonts) {
                Ok(cmds) => {
                    self.dock = Some(d);
                    self.send(cmds);
                }
                Err(e) => eprintln!("walpad-shell: system dock disabled: {}", e.0),
            }
        }
        if let Some(mut k) = self.kbd.take() {
            match k.poll(&mut self.fonts) {
                Ok(hid) => {
                    self.kbd = Some(k);
                    if hid {
                        // minimise key (down until the next focus-in), or the user went Home
                        self.focus.manual(false);
                        self.bottom_changed();
                        self.report_keyboard();
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: system keyboard disabled: {}", e.0);
                    self.bottom_changed();
                }
            }
        }
        if let Some(mut p) = self.pad.take() {
            match p.poll(&mut self.fonts) {
                Ok(hid) => {
                    self.pad = Some(p);
                    if hid {
                        // Close button, or the user went Home
                        self.bottom_changed();
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: mouse pad disabled: {}", e.0);
                    self.bottom_changed();
                }
            }
        }
        if let Some(mut s) = self.ime.take() {
            match s.poll() {
                Ok(evs) => {
                    self.ime = Some(s);
                    let now = self.now_ms();
                    for ev in evs {
                        let a = match ev {
                            ImEvent::Connected => {
                                self.focus.client_connected();
                                None
                            }
                            ImEvent::FocusIn(ic) => {
                                self.focus.focus_in(ic, now);
                                None
                            }
                            ImEvent::FocusOut(ic) | ImEvent::Destroyed(ic) => {
                                self.focus.focus_out(ic, now);
                                None
                            }
                            ImEvent::ClientGone(w) => {
                                self.focus.client_gone(w, now);
                                None
                            }
                        };
                        self.apply_focus(a);
                    }
                }
                Err(e) => eprintln!("walpad-shell: input method (XIM) disabled: {}", e.0),
            }
        }
        let now = self.now_ms();
        let a = self.focus.tick(now);
        self.apply_focus(a);

        let now = self.now_ms();
        for ev in self.core.tick(Instant::now()) {
            match &ev {
                CoreEvent::Connected => {
                    self.clock_24h = read_clock_24h(&self.root);
                    // a (re)started core learns the keyboard state from us
                    self.report_keyboard();
                }
                CoreEvent::Input(ShellInput::KeyboardShow) => self.set_keyboard(true, false),
                CoreEvent::Input(ShellInput::KeyboardHide) => self.set_keyboard(false, false),
                // a terminal is useless without keys: XIM focus normally brings the keyboard up
                // (st reports its focus); without an XIM connection it comes up as a fallback
                CoreEvent::Apps(v) if v["kind"] == "started" && v["app"] == "terminal" => {
                    let a = self.focus.app_started(now, self.ime.is_some());
                    self.apply_focus(a);
                }
                _ => {}
            }
            let out = self.app.on_core(&ev, now);
            self.dirty |= out.dirty;
            self.need_refresh |= out.refresh;
            self.send(out.cmds);
        }
        self.dirty |= self.app.tick(now);
        let queued = self.app.take_cmds();
        self.send(queued);
        if Instant::now() >= self.next_minute {
            let (wall, to_minute) = WallTime::now(self.clock_24h);
            self.next_minute = Instant::now() + Duration::from_millis(to_minute);
            self.dirty |= self.app.set_wall(wall);
            // uptime and memory change: refresh the facts once a minute while Settings/Device is open
            self.need_refresh |= matches!(
                self.app.active,
                walpad_ui::components::Tab::Settings | walpad_ui::components::Tab::Device
            );
        }
        if self.need_refresh {
            self.reload();
        }
        if self.dirty {
            self.redraw()?;
        }

        // sleep until the next thing that needs us: input, core traffic, a long-press, the minute
        let mut wait = max_wait.as_millis() as u64;
        if let Some(d) = self.rec.next_deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        if let Some(d) = self.focus.next_deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        if let Some(d) = self.dock.as_ref().and_then(|d| d.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        wait = wait.min(
            self.next_minute
                .saturating_duration_since(Instant::now())
                .as_millis() as u64,
        );
        if !self.core.connected() {
            wait = wait.min(500); // reconnect attempts
        }
        let wait = wait.max(1);
        let mut fds = vec![&self.win.xc.wait_fd];
        fds.extend(self.core.fd());
        fds.extend(self.dock.as_ref().map(|d| d.wait_fd()));
        fds.extend(self.kbd.as_ref().map(|k| k.wait_fd()));
        fds.extend(self.pad.as_ref().map(|p| p.wait_fd()));
        fds.extend(self.ime.as_ref().map(|s| s.wait_fd()));
        walpad_x::wait_any(&fds, Some(wait));
        Ok(())
    }

    pub fn run(&mut self, stop: &AtomicBool) -> Result<(), String> {
        while !stop.load(Ordering::Relaxed) {
            self.step(Duration::from_secs(5))?;
        }
        Ok(())
    }

    pub fn size(&self) -> (i32, i32) {
        self.win.size
    }
}
