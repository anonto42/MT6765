use super::{MousePad, PadOut};
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto;
use x11rb::protocol::Event;

impl MousePad {
    /// Handle everything the server has sent (non-blocking). Returns true when the pad hid itself
    /// (its Close button, or the shell came to the front = the user went Home).
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<bool, XError> {
        let mut hid = false;
        let mut warp = false;
        let mut map_dirty = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.bw.atoms.active => {
                    let was = self.active;
                    self.active = self.bw.read_active(&self.xc);
                    if self.mapped && was != self.active {
                        if self.active == self.shell && was != 0 {
                            self.set_visible(false, fonts)?;
                            hid = true;
                        } else {
                            // the window manager restacks its windows: keep the pad on top
                            self.bw.raise(&self.xc)?;
                            if let Some(m) = &self.marker {
                                m.place(&self.xc, self.st.cursor)?;
                            }
                        }
                    }
                }
                Event::Expose(e) if e.window == self.bw.win && e.count == 0 && self.mapped => {
                    self.redraw(fonts)?;
                }
                Event::Expose(e)
                    if e.count == 0 && self.marker.as_ref().is_some_and(|m| m.win == e.window) =>
                {
                    if let Some(m) = &self.marker {
                        m.present(&self.xc)?;
                    }
                }
                Event::ButtonPress(e) if e.event == self.bw.win && e.detail == 1 => {
                    if self.mapped {
                        self.st.press(e.root_x as i32, e.root_y as i32, e.time);
                        if self.st.pressed_button().is_some() {
                            self.redraw(fonts)?;
                        }
                        warp = true;
                    }
                }
                Event::MotionNotify(e) if e.event == self.bw.win => {
                    if self.mapped && self.st.motion(e.root_x as i32, e.root_y as i32) {
                        map_dirty = true;
                        warp = true;
                    } else if self.mapped && self.st.pressed_button().is_some() {
                        warp = true;
                    } else if self.mapped && e.state.contains(xproto::KeyButMask::BUTTON1) {
                        // a finger on the pad (still within the slop): keep the pointer away
                        warp = true;
                    }
                }
                Event::ButtonRelease(e) if e.event == self.bw.win && e.detail == 1 => {
                    let was_button = self.st.pressed_button().is_some();
                    let before = self.st.cursor;
                    let out = self.st.release(e.time);
                    // a drag's low-pass backlog, or a tap putting the cursor back
                    map_dirty |= self.st.cursor != before;
                    if !self.mapped {
                        continue;
                    }
                    match out {
                        Some(PadOut::Hide) => {
                            self.set_visible(false, fonts)?;
                            hid = true;
                            warp = false;
                        }
                        Some(PadOut::Click(b)) => {
                            self.click(b)?;
                            warp = false;
                        }
                        None => warp = true,
                    }
                    if self.mapped && was_button {
                        self.redraw(fonts)?;
                    }
                }
                Event::Error(e) => return Err(XError(format!("mouse pad X error: {e:?}"))),
                _ => {}
            }
        }
        if self.mapped {
            if map_dirty {
                self.redraw_map(fonts)?;
            }
            if warp {
                self.warp()?;
            }
        }
        self.xc.conn.flush()?;
        Ok(hid)
    }

    /// For tests and diagnostics: is the window currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.bw.server_mapped(&self.xc)
    }
}
