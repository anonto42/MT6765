//! The shell event loop: X events + core events -> gestures -> app -> render -> present. Redraws
//! only when something changed (input, core status, the minute boundary); writes /run/walpad-ok
//! after the first frame is on screen (the boot ramdisk health check).
use crate::app::{load_catalog, read_clock_24h, read_pinned, App, Cmd, WallTime};
use crate::clipboard::{ClipEvent, Keeper};
use crate::config::read_devd;
use crate::core_client::{CoreClient, CoreEvent};
use crate::cursor::CursorHider;
use crate::dock::{load_wm_classes, Dock};
use crate::gesture::{Gesture, Recogniser};
use crate::ime::{FocusKbd, ImEvent, ImServer, KbdAction};
use crate::keyboard::SysKeyboard;
use crate::lock_window::{LockEvent, LockWindow};
use crate::mousepad::MousePad;
use crate::nav::Nav;
use crate::seltools::{BarState, EditBar, EditOut};
use crate::settings_screen::gather;
use crate::window::{WinEvent, Window};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{LockKind, ShellInput};
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
    /// Bottom-edge swipe (Home / Recents) and the Recents strip (None when its X connection could
    /// not be opened or died).
    nav: Option<Nav>,
    /// `close_app` requests the Recents strip is waiting for: (request id, app id).
    nav_close: Vec<(u64, String)>,
    /// The XIM server whose focus events drive the keyboard (None when it could not start or
    /// its X connection died: the terminal then gets the keyboard at start, as before).
    ime: Option<ImServer>,
    /// The lock screen (None when its X connection could not be opened or died; retried while
    /// the core says the tablet is locked).
    lock: Option<LockWindow>,
    /// The core says (or, before it answered, `/run/walpad/locked` says) the tablet is locked.
    lock_wanted: bool,
    lock_kind: LockKind,
    lock_retry_in_s: Option<u32>,
    /// Id of the `unlock` request in flight.
    lock_pending: Option<u64>,
    lock_reopen_at: u64,
    sock: PathBuf,
    /// Text-input focus -> keyboard visibility.
    focus: FocusKbd,
    /// The selection toolbar and the clipboard list (None when their X connection died).
    edit: Option<EditBar>,
    /// The clipboard keeper (None when XFixes or its connection is not available).
    keeper: Option<Keeper>,
    /// When the selection toolbar shows.
    bar: BarState,
    /// Keeps the X cursor hidden (cursor.rs); None when XFixes is not available.
    _cursor: Option<CursorHider>,
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
        let catalog = load_catalog(&opts.root);
        let nav = match Nav::open(
            &sock,
            win.win,
            load_wm_classes(&opts.root),
            catalog
                .iter()
                .map(|a| (a.id.clone(), a.name.clone(), a.icon))
                .collect(),
            (w, h),
        ) {
            Ok(n) => Some(n),
            Err(e) => {
                eprintln!("walpad-shell: edge swipe / Recents disabled: {}", e.0);
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
        let cursor = match crate::cursor::hide(&sock) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("walpad-shell: cannot hide the X cursor: {}", e.0);
                None
            }
        };
        let edit = match EditBar::open(&sock, win.win, (w, h)) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("walpad-shell: selection toolbar disabled: {}", e.0);
                None
            }
        };
        let keeper = match Keeper::open(&sock) {
            Ok(k) => Some(k),
            Err(e) => {
                eprintln!("walpad-shell: clipboard keeper disabled: {}", e.0);
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
        // locked at boot / after a shell restart: the core's marker file is there before its first
        // status event, so the lock screen is up before the first frame of Home
        let lock_wanted = opts.root.p("/run/walpad/locked").exists();
        let lock = match LockWindow::open(&sock, (w, h), wall.clone()) {
            Ok(l) => Some(l),
            Err(e) => {
                eprintln!("walpad-shell: lock screen disabled: {}", e.0);
                None
            }
        };
        let mut shell = Shell {
            win,
            dock,
            kbd,
            pad,
            nav,
            nav_close: Vec::new(),
            ime,
            lock,
            lock_wanted,
            lock_kind: LockKind::Pin,
            lock_retry_in_s: None,
            lock_pending: None,
            lock_reopen_at: 0,
            sock,
            focus: FocusKbd::new(),
            edit,
            keeper,
            bar: BarState::default(),
            _cursor: cursor,
            core,
            canvas,
            fonts,
            app: App::new(w, h, catalog, wall).with_pinned(read_pinned(&opts.root)),
            rec: Recogniser::new(),
            root: opts.root,
            clock_24h,
            started: Instant::now(),
            next_minute: Instant::now() + Duration::from_millis(to_minute),
            dirty: true,
            need_refresh: true,
            first_frame: true,
            frames: 0,
        };
        if shell.lock_wanted {
            shell.sync_lock();
        }
        Ok(shell)
    }

    // ------------------------------------------------------------------ lock screen
    /// Bring the lock window in line with `lock_wanted`.
    fn sync_lock(&mut self) {
        let now = self.now_ms();
        if self.lock.is_none() && self.lock_wanted && now >= self.lock_reopen_at {
            self.lock_reopen_at = now + 1000;
            let wall = self.app.model.wall.clone();
            match LockWindow::open(&self.sock, self.win.size, wall) {
                Ok(l) => self.lock = Some(l),
                Err(e) => eprintln!("walpad-shell: lock screen unavailable: {}", e.0),
            }
        }
        let Some(mut w) = self.lock.take() else {
            return;
        };
        let (kind, retry) = (self.lock_kind, self.lock_retry_in_s);
        let r = if !self.lock_wanted {
            self.lock_pending = None;
            w.hide()
        } else if !w.visible() {
            // the strips of the shell go away with the app they belong to
            self.set_keyboard(false, true);
            self.set_pad(false);
            w.show(kind, retry, now, &mut self.fonts)
        } else {
            w.sync_retry(retry, &mut self.fonts)
        };
        match r {
            Ok(()) => self.lock = Some(w),
            Err(e) => {
                eprintln!("walpad-shell: lock screen error: {}", e.0);
                self.lock_reopen_at = now + 1000;
            }
        }
    }

    fn on_lock_status(&mut self, l: &walpad_proto::LockStatus) {
        self.lock_wanted = l.locked;
        self.lock_kind = l.kind.unwrap_or(LockKind::Pin);
        self.lock_retry_in_s = l.retry_in_s;
        self.sync_lock();
    }

    fn on_lock_event(&mut self, e: LockEvent) {
        let LockEvent::Submit(secret) = e;
        // the secret goes onto the control socket and nowhere else (never a log, never argv)
        match self
            .core
            .send("unlock", serde_json::json!({ "secret": secret }))
        {
            Some(id) => self.lock_pending = Some(id),
            None => {
                if let Some(l) = self.lock.as_mut() {
                    let _ = l.failed("The system service is not responding", &mut self.fonts);
                }
            }
        }
    }

    /// The core's answer to `unlock`.
    fn on_unlock_reply(&mut self, ok: bool, data: &serde_json::Value, msg: Option<&str>) {
        let Some(mut l) = self.lock.take() else {
            return;
        };
        let r = if ok && data["unlocked"] == true {
            // the status event follows; go back to the app at once
            self.lock_wanted = false;
            self.lock_retry_in_s = None;
            l.hide()
        } else if ok {
            l.rejected(
                data["attempts_left"].as_u64().map(|n| n as u32),
                data["retry_in_s"].as_u64().map(|n| n as u32),
                &mut self.fonts,
            )
        } else {
            l.failed(msg.unwrap_or("Could not check the secret"), &mut self.fonts)
        };
        match r {
            Ok(()) => self.lock = Some(l),
            Err(e) => eprintln!("walpad-shell: lock screen error: {}", e.0),
        }
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
                Cmd::ClipboardShow => {
                    self.show_clipboard_list();
                    continue;
                }
                _ => {}
            }
            let (name, args) = c.request();
            match self.core.send(name, args) {
                Some(id) => {
                    self.app.note_sent(id, &c);
                    if let Cmd::CloseApp(app) = &c {
                        if self.nav.as_ref().is_some_and(|n| n.is_closing(app)) {
                            self.nav_close.push((id, app.clone()));
                        }
                    }
                }
                None => {
                    // not connected: the card comes back at once
                    if let (Cmd::CloseApp(app), Some(n)) = (&c, &mut self.nav) {
                        n.close_result(app, false);
                        self.app.notice("Could not close the app");
                    }
                }
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
        if let Some(mut e) = self.edit.take() {
            match e.set_bottom(kbd_h, &mut self.fonts) {
                Ok(()) => self.edit = Some(e),
                Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
            }
        }
    }

    fn show_clipboard_list(&mut self) {
        let entries: Vec<String> = self
            .keeper
            .as_ref()
            .map(|k| k.history().iter().cloned().collect())
            .unwrap_or_default();
        if let Some(mut e) = self.edit.take() {
            match e.show_list(&entries, &mut self.fonts) {
                Ok(()) => self.edit = Some(e),
                Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
            }
        }
    }

    /// Keeper and toolbar traffic; then the toolbar's visibility follows the rule.
    fn step_edit(&mut self) {
        let now = self.now_ms();
        let mut history_changed = false;
        if let Some(mut k) = self.keeper.take() {
            match k.poll() {
                Ok(evs) => {
                    for ev in evs {
                        match ev {
                            ClipEvent::PrimaryChanged(true) => self.bar.selection(now),
                            ClipEvent::PrimaryChanged(false) => self.bar.clear_selection(),
                            ClipEvent::HistoryChanged => history_changed = true,
                        }
                    }
                    self.keeper = Some(k);
                }
                Err(e) => eprintln!("walpad-shell: clipboard keeper disabled: {}", e.0),
            }
        }
        let Some(mut e) = self.edit.take() else {
            return;
        };
        let mut ok = true;
        match e.poll(&mut self.fonts) {
            Ok(outs) => {
                for o in outs {
                    match o {
                        EditOut::CopyPrimary => {
                            if let Some(k) = &mut self.keeper {
                                if let Err(err) = k.copy_primary() {
                                    eprintln!("walpad-shell: clipboard: {}", err.0);
                                }
                            }
                        }
                        EditOut::Paste(i) => {
                            let pasted = match &mut self.keeper {
                                Some(k) => k.paste(i).unwrap_or(false),
                                None => false,
                            };
                            if pasted {
                                if let Err(err) = e.send_paste() {
                                    eprintln!("walpad-shell: paste: {}", err.0);
                                }
                            }
                        }
                        EditOut::ClearHistory => {
                            if let Some(k) = &mut self.keeper {
                                if let Err(err) = k.clear() {
                                    eprintln!("walpad-shell: clipboard: {}", err.0);
                                }
                            }
                        }
                        EditOut::Acted => self.bar.clear_selection(),
                    }
                }
            }
            Err(err) => {
                eprintln!("walpad-shell: selection toolbar disabled: {}", err.0);
                ok = false;
            }
        }
        if ok {
            if history_changed {
                let entries: Vec<String> = self
                    .keeper
                    .as_ref()
                    .map(|k| k.history().iter().cloned().collect())
                    .unwrap_or_default();
                ok = e.update_list(&entries, &mut self.fonts).is_ok();
            }
            self.bar.text_focus = self.focus.has_focus();
            self.bar.home = e.home();
            self.bar.dock_out = self.dock.as_ref().is_some_and(|d| d.expanded());
            let want = self.bar.visible(now);
            ok = ok && e.set_wanted(want, &mut self.fonts).is_ok();
        }
        if ok {
            self.edit = Some(e);
        } else {
            eprintln!("walpad-shell: selection toolbar disabled");
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
        // apps installed or removed in the app store
        self.dirty |= self.app.set_catalog(load_catalog(&self.root));
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
            if let Some(mut e) = self.edit.take() {
                match e.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.edit = Some(e),
                    Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
                }
            }
            self.bottom_changed();
            if let Some(l) = self.lock.as_mut() {
                if let Err(e) = l.set_shell_size(w, h, &mut self.fonts) {
                    eprintln!("walpad-shell: lock screen: {}", e.0);
                }
            }
            if let Some(d) = &mut self.dock {
                if let Err(e) = d.set_shell_size(w, h) {
                    eprintln!("walpad-shell: system dock: {}", e.0);
                }
            }
            if let Some(n) = &mut self.nav {
                if let Err(e) = n.set_shell_size(w, h) {
                    eprintln!("walpad-shell: edge swipe / Recents: {}", e.0);
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
        if self.lock_wanted && self.lock.is_none() {
            self.sync_lock();
        }
        if let Some(mut l) = self.lock.take() {
            match l.poll(now, &mut self.fonts) {
                Ok(evs) => {
                    self.lock = Some(l);
                    for e in evs {
                        self.on_lock_event(e);
                    }
                }
                Err(e) => {
                    eprintln!("walpad-shell: lock screen error: {}", e.0);
                    self.lock_reopen_at = now + 1000;
                }
            }
        }
        if let Some(mut d) = self.dock.take() {
            match d.poll(&mut self.fonts) {
                Ok(cmds) => {
                    self.dock = Some(d);
                    self.send(cmds);
                }
                Err(e) => eprintln!("walpad-shell: system dock disabled: {}", e.0),
            }
        }
        // the edge zone only exists while the bottom edge is free of keyboard, pad and dock strip
        let blocked = self.keyboard_visible()
            || self.mouse_pad_visible()
            || self.dock.as_ref().is_some_and(|d| d.expanded());
        if let Some(mut n) = self.nav.take() {
            let res = n
                .set_blocked(blocked)
                .and_then(|()| n.poll(&mut self.fonts));
            match res {
                Ok(out) => {
                    self.nav = Some(n);
                    for t in out.notices {
                        self.app.notice(t);
                        self.dirty = true;
                    }
                    self.send(out.cmds);
                }
                Err(e) => eprintln!("walpad-shell: edge swipe / Recents disabled: {}", e.0),
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
        self.step_edit();

        let now = self.now_ms();
        for ev in self.core.tick(Instant::now()) {
            match &ev {
                CoreEvent::Connected => {
                    self.clock_24h = read_clock_24h(&self.root);
                    // a (re)started core learns the keyboard state from us
                    self.report_keyboard();
                }
                CoreEvent::Status(s) => self.on_lock_status(&s.lock),
                CoreEvent::Reply {
                    id,
                    ok,
                    data,
                    error,
                } if self.lock_pending == Some(*id) => {
                    self.lock_pending = None;
                    self.on_unlock_reply(*ok, data, error.as_ref().map(|e| e.msg.as_str()));
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
            if let CoreEvent::Reply { id, ok, data, .. } = &ev {
                if let Some(i) = self.nav_close.iter().position(|(r, _)| r == id) {
                    let (_, app) = self.nav_close.remove(i);
                    if let Some(n) = &mut self.nav {
                        n.close_result(&app, *ok && data["closed"] != false);
                    }
                }
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
            if let Some(l) = self.lock.as_mut() {
                if let Err(e) = l.set_wall(wall.clone(), &mut self.fonts) {
                    eprintln!("walpad-shell: lock screen: {}", e.0);
                }
            }
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
        if let Some(d) = self.bar.deadline() {
            wait = wait.min(d.saturating_sub(self.now_ms()));
        }
        for d in [
            self.keeper.as_ref().and_then(|k| k.next_deadline()),
            self.edit.as_ref().and_then(|e| e.next_deadline()),
        ]
        .into_iter()
        .flatten()
        {
            wait = wait.min(d.saturating_duration_since(Instant::now()).as_millis() as u64);
        }
        if let Some(d) = self.dock.as_ref().and_then(|d| d.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        if let Some(d) = self.nav.as_ref().and_then(|n| n.next_deadline()) {
            wait = wait.min(d.as_millis() as u64);
        }
        wait = wait.min(
            self.next_minute
                .saturating_duration_since(Instant::now())
                .as_millis() as u64,
        );
        if let Some(d) = self.lock.as_ref().and_then(|l| l.next_deadline(now)) {
            wait = wait.min(d);
        }
        if self.lock_wanted && self.lock.is_none() {
            wait = wait.min(500); // the lock window is retried
        }
        if !self.core.connected() {
            wait = wait.min(500); // reconnect attempts
        }
        let wait = wait.max(1);
        let mut fds = vec![&self.win.xc.wait_fd];
        fds.extend(self.core.fd());
        fds.extend(self.dock.as_ref().map(|d| d.wait_fd()));
        fds.extend(self.kbd.as_ref().map(|k| k.wait_fd()));
        fds.extend(self.pad.as_ref().map(|p| p.wait_fd()));
        fds.extend(self.nav.as_ref().map(|n| n.wait_fd()));
        fds.extend(self.ime.as_ref().map(|s| s.wait_fd()));
        fds.extend(self.keeper.as_ref().map(|k| k.wait_fd()));
        fds.extend(self.edit.as_ref().map(|e| e.wait_fd()));
        fds.extend(self.lock.as_ref().map(|l| l.wait_fd()));
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
