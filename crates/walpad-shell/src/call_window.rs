//! The call screen's X window (owner request 2026-11-11, change `phone-dialer`): like the lock
//! screen an override-redirect window the size of the whole root, kept above every other window
//! (apps, the dock, the keyboard) with an active keyboard and pointer grab, so an incoming call
//! appears over any app and answering or declining never reaches the app underneath. While it is
//! up the lock window is paused (`LockWindow::set_paused`): the call screen shows over the lock
//! screen and offers nothing but the call controls; when it goes the lock screen is back.
//! Own X connection, every wait has a deadline (walpad-x). The content is `call_screen.rs`.
use crate::call_screen::{CallAct, CallScreen, Out};
use crate::dock::visible_area;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_proto::PhoneStatus;
use walpad_ui::components::Painter;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, GrabMode, GrabStatus, MapState, PropMode, StackMode,
    Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

pub const WM_CLASS_INSTANCE: &str = "walpad-call";
pub const WM_CLASS_CLASS: &str = "WalPad-Call";
/// How often a lost grab / stacking is repaired while the screen is up.
const GUARD_EVERY_MS: u64 = 400;

pub struct CallWindow {
    xc: XConn,
    win: XWindow,
    gc: u32,
    depth: u8,
    root_size: (i32, i32),
    shown: bool,
    grabbed: bool,
    screen: CallScreen,
    canvas: Canvas,
    next_guard: u64,
    dirty: bool,
}

fn pointer_events() -> EventMask {
    EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::BUTTON1_MOTION
}

impl CallWindow {
    pub fn open(socket: &Path, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let setup = conn.setup();
        let depth = setup.roots.first().map(|s| s.root_depth).unwrap_or(24);
        let g = conn.get_geometry(xc.root)?.reply()?;
        let root_size = (g.width as i32, g.height as i32);
        let win = conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .event_mask(pointer_events() | EventMask::EXPOSURE)
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
        conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        )?;
        conn.flush()?;
        let area = visible_area(shell_size.0, shell_size.1);
        Ok(CallWindow {
            xc,
            win,
            gc,
            depth,
            root_size,
            shown: false,
            grabbed: false,
            screen: CallScreen::new(area),
            canvas: Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("call canvas")?,
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
    pub fn screen(&self) -> &CallScreen {
        &self.screen
    }
    pub fn size(&self) -> (i32, i32) {
        self.root_size
    }

    /// The core's phone status: show, update or hide the screen.
    pub fn update(&mut self, st: &PhoneStatus, now: u64, fonts: &mut Fonts) -> Result<(), XError> {
        self.screen.set_status(st, now);
        if self.screen.wanted() {
            if !self.shown {
                self.shown = true;
                self.xc.conn.map_window(self.win)?;
                self.raise()?;
                self.next_guard = 0;
            }
            self.guard(now)?;
            self.draw(fonts)
        } else {
            self.hide()
        }
    }

    /// Back to what is underneath: release the grabs and unmap.
    pub fn hide(&mut self) -> Result<(), XError> {
        if !self.shown {
            return Ok(());
        }
        self.shown = false;
        self.release()?;
        self.xc.conn.unmap_window(self.win)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        self.xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

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

    fn guard(&mut self, now: u64) -> Result<(), XError> {
        if !self.shown || now < self.next_guard {
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

    /// The shell window was resized (rotation).
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.screen.area {
            return Ok(());
        }
        self.screen.set_area(area);
        self.canvas = Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("call canvas")?;
        if self.shown {
            self.draw(fonts)?;
        }
        Ok(())
    }

    /// Milliseconds until the timer or the guard needs us (None when hidden).
    pub fn next_deadline(&self, now: u64) -> Option<u64> {
        if !self.shown {
            return None;
        }
        let guard = self.next_guard.saturating_sub(now).max(1);
        let timer = self.screen.next_deadline(now);
        Some(timer.map_or(guard, |t| t.min(guard)))
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

    fn out(&mut self, o: Out, acts: &mut Vec<CallAct>) {
        match o {
            Out::None => {}
            Out::Redraw => self.dirty = true,
            Out::Act(a) => {
                self.dirty = true;
                acts.push(a);
            }
        }
    }

    /// X events and timers. `now` is the shell clock (ms). Returns what the owner asked for.
    pub fn poll(&mut self, now: u64, fonts: &mut Fonts) -> Result<Vec<CallAct>, XError> {
        let mut acts = Vec::new();
        let mut restack = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::ButtonPress(e) if e.event == self.win && e.detail == 1 && self.shown => {
                    let o = self.screen.down(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut acts);
                }
                Event::MotionNotify(e) if e.event == self.win && self.shown => {
                    let o = self.screen.moved(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut acts);
                }
                Event::ButtonRelease(e) if e.event == self.win && e.detail == 1 && self.shown => {
                    let o = self.screen.up(e.event_x as i32, e.event_y as i32);
                    self.out(o, &mut acts);
                }
                Event::Expose(e) if e.window == self.win && e.count == 0 => self.dirty = true,
                Event::MapNotify(e) if e.window != self.win => restack = true,
                Event::ConfigureNotify(e) if e.window != self.win => restack = true,
                Event::CirculateNotify(e) if e.window != self.win => restack = true,
                Event::Error(e) => return Err(XError(format!("call X error: {e:?}"))),
                _ => {}
            }
        }
        if self.shown {
            if restack {
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
        Ok(acts)
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
