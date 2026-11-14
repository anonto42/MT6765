use super::{tap, Stroke, SysKeyboard, Tap};
use walpad_ui::components::SysKbdState;
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::Event;

impl SysKeyboard {
    /// Handle everything the server has sent (non-blocking). Returns true when the keyboard hid
    /// itself (the minimise key, or the shell came to the front = the user went Home).
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<bool, XError> {
        let mut hid = false;
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
                            // the window manager restacks its windows: keep the keyboard on top
                            self.bw.raise(&self.xc)?;
                            self.xc.conn.flush()?;
                        }
                    }
                }
                Event::Expose(e) if e.window == self.bw.win && e.count == 0 && self.mapped => {
                    self.redraw(fonts)?;
                }
                Event::ButtonPress(e) if e.event == self.bw.win && e.detail == 1 => {
                    let prev = self.pressed;
                    let hit = self.key_at(e.event_x as i32, e.event_y as i32);
                    self.pressed = hit;
                    if prev != hit {
                        for idx in [prev, hit].into_iter().flatten() {
                            self.redraw_key(idx, fonts)?;
                        }
                    }
                }
                Event::ButtonRelease(e) if e.event == self.bw.win && e.detail == 1 => {
                    let hit = self.key_at(e.event_x as i32, e.event_y as i32);
                    let down = self.pressed.take();
                    let old_state = self.state;
                    let key = match (hit, down) {
                        (Some(h), Some(d)) if h == d => self.keys.get(h).map(|k| k.key),
                        _ => None,
                    };
                    match key.map(|k| tap(&mut self.state, k)) {
                        Some(Tap::Hide) => {
                            self.set_visible(false, fonts)?;
                            hid = true;
                        }
                        Some(Tap::Send(s)) => {
                            self.send_and_redraw(s, old_state, down, fonts)?;
                        }
                        Some(Tap::Paste) => {
                            let class =
                                crate::seltools::active_class(&self.xc, self.bw.atoms.active);
                            let s =
                                crate::seltools::paste_stroke(crate::seltools::is_terminal(&class));
                            self.send_and_redraw(s, old_state, down, fonts)?;
                        }
                        Some(Tap::State) => self.redraw(fonts)?,
                        None => {
                            if let Some(idx) = down {
                                self.redraw_key(idx, fonts)?;
                            }
                        }
                    }
                }
                Event::Error(e) => return Err(XError(format!("keyboard X error: {e:?}"))),
                _ => {}
            }
        }
        Ok(hid)
    }

    fn send_and_redraw(
        &mut self,
        s: Stroke,
        old_state: SysKbdState,
        down: Option<usize>,
        fonts: &mut Fonts,
    ) -> Result<(), XError> {
        if let Err(e) = self.send(s) {
            eprintln!("walpad-shell: keyboard: {}", e.0);
        }
        if self.state != old_state {
            self.redraw(fonts)?;
        } else if let Some(idx) = down {
            self.redraw_key(idx, fonts)?;
        }
        Ok(())
    }

    /// For tests and diagnostics: is the window currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.bw.server_mapped(&self.xc)
    }
}
