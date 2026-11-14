use super::{pointer_events, LockEvent, LockWindow, GUARD_EVERY_MS};
use crate::lock_screen::Out;
use walpad_ui::components::Painter;
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as _, GrabMode, GrabStatus, KeyButMask, MapState, StackMode,
};
use x11rb::protocol::Event;
use x11rb::CURRENT_TIME;

impl LockWindow {
    pub(super) fn raise(&self) -> Result<(), XError> {
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

    pub(super) fn release(&mut self) -> Result<(), XError> {
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
    pub(super) fn guard(&mut self, now: u64) -> Result<(), XError> {
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

    pub(super) fn draw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
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
