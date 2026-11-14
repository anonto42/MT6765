use super::model::{
    active_class, button_stroke, buttons, is_terminal, list_clip_layout, list_rect,
};
use super::{EditBar, EditOut};
use crate::keyboard::inject;
use std::time::Instant;
use walpad_ui::edit_bar::{bar_layout, ClipHit, EditBtn};
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::Event;

impl EditBar {
    /// Handle everything the server has sent (non-blocking).
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<EditOut>, XError> {
        let mut out = Vec::new();
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.active_atom => {
                    let was = (self.active, self.terminal);
                    self.read_active();
                    if was != (self.active, self.terminal) {
                        if self.bar.mapped {
                            if was.1 != self.terminal {
                                self.apply_bar(fonts)?;
                            } else {
                                // the window manager restacked: keep the toolbar on top
                                self.raise(self.bar.id)?;
                            }
                        }
                        if self.list.mapped {
                            self.raise(self.list.id)?;
                        }
                        if self.active == self.shell && was.0 != 0 {
                            // Home: the list goes away (the toolbar follows the shell's rule)
                            self.hide_list()?;
                        }
                        self.xc.conn.flush()?;
                    }
                }
                Event::Expose(e) if e.count == 0 => {
                    if e.window == self.bar.id && self.bar.mapped {
                        self.redraw_bar(fonts)?;
                    } else if e.window == self.list.id && self.list.mapped {
                        let r = list_rect(self.area, self.bottom_h, self.entries.len());
                        let shown = self.shown_entries(r);
                        self.redraw_list(fonts, shown)?;
                    }
                }
                Event::ButtonPress(e) if e.detail == 1 => {
                    if e.event == self.bar.id {
                        self.bar_pressed = self.bar_at(e.event_x as i32, e.event_y as i32);
                        self.redraw_bar(fonts)?;
                    } else if e.event == self.list.id {
                        self.list_pressed = self.list_at(e.event_x as i32, e.event_y as i32);
                        let r = list_rect(self.area, self.bottom_h, self.entries.len());
                        let shown = self.shown_entries(r);
                        self.redraw_list(fonts, shown)?;
                    }
                }
                Event::ButtonRelease(e) if e.detail == 1 => {
                    if e.event == self.bar.id {
                        let hit = self.bar_at(e.event_x as i32, e.event_y as i32);
                        let down = self.bar_pressed.take();
                        if let (Some(h), Some(d)) = (hit, down) {
                            if h == d {
                                self.run_button(buttons(self.terminal)[h], &mut out);
                            }
                        }
                        if self.bar.mapped {
                            self.redraw_bar(fonts)?;
                        }
                    } else if e.event == self.list.id {
                        let hit = self.list_at(e.event_x as i32, e.event_y as i32);
                        let down = self.list_pressed.take();
                        if let (Some(h), Some(d)) = (hit, down) {
                            if h == d {
                                match h {
                                    ClipHit::Entry(i) => {
                                        out.push(EditOut::Paste(i));
                                        self.hide_list()?;
                                    }
                                    ClipHit::Clear => out.push(EditOut::ClearHistory),
                                    ClipHit::Close => self.hide_list()?,
                                }
                            }
                        }
                        if self.list.mapped {
                            let r = list_rect(self.area, self.bottom_h, self.entries.len());
                            let shown = self.shown_entries(r);
                            self.redraw_list(fonts, shown)?;
                        }
                    }
                }
                Event::Error(e) => return Err(XError(format!("edit bar X error: {e:?}"))),
                _ => {}
            }
        }
        if self.list_until.is_some_and(|t| Instant::now() >= t) {
            self.hide_list()?;
        }
        Ok(out)
    }

    fn bar_at(&self, x: i32, y: i32) -> Option<usize> {
        bar_layout(buttons(self.terminal))
            .iter()
            .position(|(r, _)| r.contains(x, y))
    }

    fn list_at(&self, x: i32, y: i32) -> Option<ClipHit> {
        let r = list_rect(self.area, self.bottom_h, self.entries.len());
        let shown = self.shown_entries(r);
        list_clip_layout(r.w, shown)
            .into_iter()
            .find(|(b, _)| b.contains(x, y))
            .map(|(_, h)| h)
    }

    fn run_button(&mut self, b: EditBtn, out: &mut Vec<EditOut>) {
        // the app in front may have changed since the toolbar was drawn
        let terminal = is_terminal(&active_class(&self.xc, self.active_atom));
        if terminal && b == EditBtn::Copy {
            out.push(EditOut::CopyPrimary);
        } else if let Some(s) = button_stroke(b, terminal) {
            if let Err(e) = inject(&self.xc, &self.keymap, s) {
                eprintln!("walpad-shell: edit toolbar: {}", e.0);
            }
        }
        out.push(EditOut::Acted);
    }
}
