use super::{pointer_events, visible_area, CallWindow, GUARD_EVERY_MS};
use crate::call_screen::{CallAct, Out};
use walpad_ui::components::Painter;
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts};
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as _, GrabMode, GrabStatus, StackMode,
};
use x11rb::CURRENT_TIME;

impl CallWindow {
    pub(super) fn raise(&self) -> Result<(), XError> {
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

    pub(super) fn release(&mut self) -> Result<(), XError> {
        self.xc.conn.ungrab_keyboard(CURRENT_TIME)?;
        self.xc.conn.ungrab_pointer(CURRENT_TIME)?;
        self.grabbed = false;
        Ok(())
    }

    pub(super) fn guard(&mut self, now: u64) -> Result<(), XError> {
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
        self.canvas = Canvas::new(area.0 as u32, area.1 as u32, bg()).ok_or("call canvas")?;
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

    pub(super) fn out(&mut self, o: Out, acts: &mut Vec<CallAct>) {
        match o {
            Out::None => {}
            Out::Redraw => self.dirty = true,
            Out::Act(a) => {
                self.dirty = true;
                acts.push(a);
            }
        }
    }
}
