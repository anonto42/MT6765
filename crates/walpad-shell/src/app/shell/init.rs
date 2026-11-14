use super::Shell;
use crate::app::{load_catalog, read_clock_24h, read_pinned, read_theme, App, WallTime};
use crate::call_window::CallWindow;
use crate::clipboard::Keeper;
use crate::core_client::CoreClient;
use crate::dock::{load_wm_classes, Dock};
use crate::gesture::Recogniser;
use crate::ime::{FocusKbd, ImServer};
use crate::keyboard::SysKeyboard;
use crate::lock_window::LockWindow;
use crate::mousepad::MousePad;
use crate::nav::Nav;
use crate::seltools::{BarState, EditBar};
use crate::window::Window;
use crate::wmbar::WmBar;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::LockKind;
use walpad_ui::components::Painter;
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts};

pub struct Options {
    pub root: Root,
    /// X socket, default <root>/tmp/.X11-unix/X0.
    pub x_socket: Option<PathBuf>,
    /// Core control socket, default <root>/run/walpad/control.sock.
    pub core_socket: Option<PathBuf>,
    pub fonts_dir: Option<PathBuf>,
}

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
        let wmbar = match WmBar::open(
            &sock,
            crate::wmbar::load_apps(&opts.root),
            opts.root.clone(),
            (w, h),
        ) {
            Ok(b) => Some(b),
            Err(e) => {
                eprintln!("walpad-shell: window bar disabled: {}", e.0);
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
            Canvas::new(w as u32, h as u32, bg()).ok_or("cannot allocate the frame buffer")?;
        let clock_24h = read_clock_24h(&opts.root);
        walpad_ui::tokens::set_theme(read_theme(&opts.root));
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
        let call = match CallWindow::open(&sock, (w, h)) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("walpad-shell: call screen disabled: {}", e.0);
                None
            }
        };
        let mut shell = Shell {
            win,
            dock,
            wmbar,
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
            call,
            call_status: Default::default(),
            call_reopen_at: 0,
            sock,
            focus: FocusKbd::new(),
            edit,
            keeper,
            bar: BarState::default(),
            _cursor: cursor,
            core,
            media: crate::media_worker::Worker::start(opts.root.clone()),
            canvas,
            fonts,
            app: App::new(w, h, catalog, wall)
                .with_pinned(read_pinned(&opts.root))
                .with_transitions(),
            rec: Recogniser::new(),
            root: opts.root,
            clock_24h,
            started: Instant::now(),
            next_minute: Instant::now() + Duration::from_millis(to_minute),
            dirty: true,
            screen_off: false,
            need_refresh: true,
            first_frame: true,
            frames: 0,
        };
        if shell.lock_wanted {
            shell.sync_lock();
        }
        Ok(shell)
    }

    pub(super) fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    pub(super) fn redraw(&mut self) -> Result<(), String> {
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
}
