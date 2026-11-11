//! The lock screen's X window (OpenSpec `lock-screen`). X11 has no secure lock without a
//! compositor, so this is the usual honest version: an override-redirect window the size of the
//! whole root (the window manager never sees it), kept above every other window, with an active
//! keyboard and pointer grab so no touch, click or key reaches an app underneath. It runs on its
//! own X connection like the keyboard and the mouse pad; every wait has a deadline (walpad-x).
//!
//! Defences against the grab being lost or the stacking changing: new top-level windows trigger a
//! re-raise (SubstructureNotify on the root), and every 400 ms a missing grab is retaken (another
//! client can hold one for a moment, e.g. a menu) and the window raised again.
//!
//! The content (clock, pad, countdown) is `lock_screen.rs`; the secret goes out through
//! `LockEvent::Submit` and the shell sends it to the core, which alone verifies it.
use crate::app::WallTime;
use crate::dock::visible_area;
use crate::lock_screen::{LockScreen, Out};
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_proto::LockKind;
use walpad_ui::components::Painter;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, GrabMode, GrabStatus, KeyButMask, MapState, PropMode, StackMode,
    Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

pub const WM_CLASS_INSTANCE: &str = "walpad-lock";
pub const WM_CLASS_CLASS: &str = "WalPad-Lock";
/// How often a lost grab / stacking is repaired while locked.
const GUARD_EVERY_MS: u64 = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockEvent {
    /// Verify this secret with the core (`unlock`).
    Submit(String),
}

pub struct LockWindow {
    xc: XConn,
    win: XWindow,
    gc: u32,
    depth: u8,
    min_keycode: u8,
    per: usize,
    syms: Vec<u32>,
    root_size: (i32, i32),
    shown: bool,
    grabbed: bool,
    /// The call screen is over the lock screen (owner request 2026-11-11): no re-raising and no
    /// grabbing until the call is over.
    paused: bool,
    screen: LockScreen,
    canvas: Canvas,
    next_guard: u64,
    dirty: bool,
}

fn pointer_events() -> EventMask {
    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::BUTTON1_MOTION
}

impl LockWindow {
    pub fn open(socket: &Path, shell_size: (i32, i32), wall: WallTime) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let setup = conn.setup();
        let depth = setup.roots.first().map(|s| s.root_depth).unwrap_or(24);
        let (min, max) = (setup.min_keycode, setup.max_keycode);
        let km = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
        let g = conn.get_geometry(xc.root)?.reply()?;
        let root_size = (g.width as i32, g.height as i32);
        let win = conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .event_mask(pointer_events() | EventMask::EXPOSURE | EventMask::KEY_PRESS)
            .background_pixel(0x0039_3E46)
            .override_redirect(1);
        conn.create_window(
            0,
            win,
            xc.root,
            0,
            0,
            root_size.0 as u16,
            root_size.1 as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &aux,
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;
        let class = format!("{WM_CLASS_INSTANCE}\0{WM_CLASS_CLASS}\0");
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            class.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            WM_CLASS_INSTANCE.as_bytes(),
        )?;
        // new top-level windows (menus, a restart of the shell's own strips) -> we raise again
        conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        )?;
        conn.flush()?;
        let area = visible_area(shell_size.0, shell_size.1);
        Ok(LockWindow {
            xc,
            win,
            gc,
            depth,
            min_keycode: min,
            per: km.keysyms_per_keycode as usize,
            syms: km.keysyms,
            root_size,
            shown: false,
            grabbed: false,
            paused: false,
            screen: LockScreen::new(LockKind::Pin, area, wall),
            canvas: Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("lock canvas")?,
            next_guard: 0,
            dirty: false,
        })
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn window(&self) -> XWindow {
        self.win
    }
    pub fn visible(&self) -> bool {
        self.shown
    }
    pub fn grabbed(&self) -> bool {
        self.grabbed
    }
    pub fn screen(&self) -> &LockScreen {
        &self.screen
    }
    /// The window's size (the whole root).
    pub fn size(&self) -> (i32, i32) {
        self.root_size
    }

    /// Show the lock screen over everything and take the grabs. `retry_in_s` = attempts are
    /// refused for that long already.
    pub fn show(
        &mut self,
        kind: LockKind,
        retry_in_s: Option<u32>,
        now: u64,
        fonts: &mut Fonts,
    ) -> Result<(), XError> {
        self.screen.set_kind(kind);
        self.screen.now = now;
        self.screen.set_retry(retry_in_s);
        if !self.shown {
            self.shown = true;
            self.screen.wipe();
            self.screen.error = None;
            self.screen.busy = false;
            self.xc.conn.map_window(self.win)?;
            self.raise()?;
            self.next_guard = 0;
        }
        self.guard(now)?;
        self.draw(fonts)
    }

    /// Back to the app underneath: release the grabs and unmap.
    pub fn hide(&mut self) -> Result<(), XError> {
        if !self.shown {
            return Ok(());
        }
        self.shown = false;
        self.screen.wipe();
        self.screen.busy = false;
        self.release()?;
        self.xc.conn.unmap_window(self.win)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        if self.paused {
            return Ok(());
        }
        self.xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

    /// Take the keyboard and pointer grabs (idempotent).
    fn grab(&mut self) -> Result<bool, XError> {
        let c = &self.xc.conn;
        let kb = c
            .grab_keyboard(
                false,
                self.win,
                CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?
            .status;
        let ptr = c
            .grab_pointer(
                false,
                self.win,
                pointer_events(),
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                self.win,
                0u32,
                CURRENT_TIME,
            )?
            .reply()?
            .status;
        self.grabbed = kb == GrabStatus::SUCCESS && ptr == GrabStatus::SUCCESS;
        if !self.grabbed {
            // all or nothing: a half grab would leave a hole
            if kb == GrabStatus::SUCCESS {
                c.ungrab_keyboard(CURRENT_TIME)?;
            }
            if ptr == GrabStatus::SUCCESS {
                c.ungrab_pointer(CURRENT_TIME)?;
            }
        }
        c.flush()?;
        Ok(self.grabbed)
    }

    fn release(&mut self) -> Result<(), XError> {
        self.xc.conn.ungrab_keyboard(CURRENT_TIME)?;
        self.xc.conn.ungrab_pointer(CURRENT_TIME)?;
        self.grabbed = false;
        Ok(())
    }

    /// Let the call screen sit over the lock screen (true), or take the screen back (false).
    pub fn set_paused(&mut self, paused: bool) -> Result<(), XError> {
        if paused == self.paused {
            return Ok(());
        }
        self.paused = paused;
        if self.shown {
            if paused {
                self.release()?;
            } else {
                self.next_guard = 0; // raise and grab again at once
            }
            self.xc.conn.flush()?;
        }
        Ok(())
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Repair the stacking and the grabs.
    fn guard(&mut self, now: u64) -> Result<(), XError> {
        if !self.shown || self.paused || now < self.next_guard {
            return Ok(());
        }
        self.next_guard = now + GUARD_EVERY_MS;
        self.raise()?;
        if !self.grabbed {
            self.grab()?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    pub fn set_wall(&mut self, wall: WallTime, fonts: &mut Fonts) -> Result<(), XError> {
        if self.screen.wall != wall {
            self.screen.wall = wall;
            if self.shown {
                self.draw(fonts)?;
            }
        }
        Ok(())
    }

    /// The shell window was resized (rotation): same root, other visible area.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.screen.area {
            return Ok(());
        }
        self.screen.set_area(area);
        self.canvas = Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("lock canvas")?;
        if self.shown {
            self.draw(fonts)?;
        }
        Ok(())
    }

    /// The core answered `unlock` with `unlocked: false`.
    pub fn rejected(
        &mut self,
        attempts_left: Option<u32>,
        retry_in_s: Option<u32>,
        fonts: &mut Fonts,
    ) -> Result<(), XError> {
        self.screen.rejected(attempts_left, retry_in_s);
        self.draw(fonts)
    }

    pub fn failed(&mut self, msg: &str, fonts: &mut Fonts) -> Result<(), XError> {
        self.screen.failed(msg);
        self.draw(fonts)
    }

    /// The core says attempts are refused for `secs` more seconds (status event).
    pub fn sync_retry(&mut self, secs: Option<u32>, fonts: &mut Fonts) -> Result<(), XError> {
        let was = self.screen.waiting();
        if secs.is_some() != was {
            self.screen.set_retry(secs);
            if self.shown {
                self.draw(fonts)?;
            }
        }
        Ok(())
    }

    /// Milliseconds until the countdown needs a redraw or the guard runs (None when hidden).
    pub fn next_deadline(&self, now: u64) -> Option<u64> {
        if !self.shown {
            return None;
        }
        let guard = self.next_guard.saturating_sub(now).max(1);
        let cd = self
            .screen
            .next_deadline()
            .map(|t| t.saturating_sub(now).max(1));
        Some(cd.map_or(guard, |c| c.min(guard)))
    }

    fn draw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        if !self.shown {
            return Ok(());
        }
        self.screen.render(&mut Painter {
            cv: &mut self.canvas,
            fonts,
        });
        crate::present::present(
            &self.xc.conn,
            self.win,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )?;
        self.dirty = false;
        Ok(())
    }

    fn keysym(&self, keycode: u8, shift: bool) -> Option<u32> {
        let i = (keycode.checked_sub(self.min_keycode)? as usize) * self.per.max(1);
        let col = usize::from(shift && self.per > 1);
        self.syms.get(i + col).copied().filter(|s| *s != 0)
    }

    fn out(&mut self, o: Out, evs: &mut Vec<LockEvent>) {
        match o {
            Out::None => {}
            Out::Redraw => self.dirty = true,
            Out::Submit(s) => {
                self.dirty = true;
                evs.push(LockEvent::Submit(s));
            }
        }
    }

    /// X events and timers. `now` is the shell clock (ms).
    pub fn poll(&mut self, now: u64, fonts: &mut Fonts) -> Result<Vec<LockEvent>, XError> {
        let mut evs = Vec::new();
        let mut restack = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::ButtonPress(e) if e.event == self.win && e.detail == 1 && self.shown => {
                    let o = self.screen.down(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut evs);
                }
                Event::MotionNotify(e) if e.event == self.win && self.shown => {
                    let o = self.screen.moved(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut evs);
                }
                Event::ButtonRelease(e) if e.event == self.win && e.detail == 1 && self.shown => {
                    let o = self.screen.up(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut evs);
                }
                Event::KeyPress(e) if self.shown => {
                    let shift = e.state.contains(KeyButMask::SHIFT);
                    if let Some(sym) = self.keysym(e.detail, shift) {
                        let o = self.screen.keysym(sym);
                        self.out(o, &mut evs);
                    }
                }
                Event::Expose(e) if e.window == self.win && e.count == 0 => self.dirty = true,
                // somebody mapped or restacked a top-level window: we go back on top
                Event::MapNotify(e) if e.window != self.win => restack = true,
                Event::ConfigureNotify(e) if e.window != self.win => restack = true,
                Event::CirculateNotify(e) if e.window != self.win => restack = true,
                Event::Error(e) => return Err(XError(format!("lock X error: {e:?}"))),
                _ => {}
            }
        }
        if self.shown {
            if restack && !self.paused {
                self.raise()?;
                self.xc.conn.flush()?;
            }
            if self.screen.tick(now) {
                self.dirty = true;
            }
            self.guard(now)?;
            if self.dirty {
                self.draw(fonts)?;
            }
        }
        Ok(evs)
    }

    /// For tests and diagnostics: does the server say the window is mapped?
    pub fn server_mapped(&self) -> bool {
        self.xc
            .conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
}
