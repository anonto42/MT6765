use super::layout::*;
use super::render::{render, render_handle};
use super::state::*;
use super::window::{Dock, HandleDrag, Pressed};
use crate::app::Cmd;
use walpad_ui::components::Painter;
use walpad_ui::{Fonts, Rect};
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::Event;

impl Dock {
    fn apply_fold(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let expanded = self.fold.expanded;
        let hr = self.current_handle_rect();
        if expanded {
            self.xc.conn.unmap_window(self.handle)?;
            self.xc.conn.map_window(self.win)?;
            self.configure(self.win, dock_rect_for_handle(self.area, hr), true)?;
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
        } else {
            self.xc.conn.unmap_window(self.win)?;
            if self.mapped {
                self.xc.conn.map_window(self.handle)?;
                self.configure(self.handle, hr, true)?;
                self.redraw_handle(fonts)?;
            }
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn collapse(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        if self.fold.expanded {
            self.fold.collapse();
            self.apply_fold(fonts)?;
        }
        Ok(())
    }

    fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let pressed = match self.pressed {
            Some(Pressed::Button(b)) => Some(b),
            _ => None,
        };
        render(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            pressed,
        );
        crate::present::present(
            &self.xc.conn,
            self.win,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    fn redraw_handle(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render_handle(
            &mut Painter {
                cv: &mut self.handle_canvas,
                fonts,
            },
            (0, 0),
            self.fold.expanded,
            self.pressed == Some(Pressed::Handle),
        );
        crate::present::present(
            &self.xc.conn,
            self.handle,
            self.handle_gc,
            self.depth,
            &self.handle_canvas,
            self.handle_canvas.bounds(),
        )
    }

    /// Handle everything the server has sent (non-blocking) and the auto-collapse timer; returns
    /// the commands for the core.
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<Cmd>, XError> {
        let mut cmds = Vec::new();
        let w = DOCK_W;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.atoms.active => {
                    self.active = self.read_active();
                    let mine = [self.win, self.handle, self.strut_win];
                    let on = wants_visible(self.active, self.shell, &mine, self.mapped);
                    let was = self.mapped;
                    self.set_mapped(on)?;
                    if on && !was {
                        // paint right away: a remapped window is not always sent an Expose
                        self.redraw_handle(fonts)?;
                    }
                }
                Event::Expose(e) if e.window == self.win && e.count == 0 => {
                    self.redraw(fonts)?;
                }
                Event::Expose(e) if e.window == self.handle && e.count == 0 => {
                    self.redraw_handle(fonts)?;
                }
                Event::ButtonPress(e) if e.event == self.handle && e.detail == 1 => {
                    let hr = self.current_handle_rect();
                    self.drag = Some(HandleDrag {
                        start_root: (e.root_x as i32, e.root_y as i32),
                        origin: (hr.x, hr.y),
                        moved: false,
                    });
                    self.pressed = Some(Pressed::Handle);
                    self.redraw_handle(fonts)?;
                }
                Event::MotionNotify(e) if e.event == self.handle => {
                    if let Some(d) = &mut self.drag {
                        let dx = e.root_x as i32 - d.start_root.0;
                        let dy = e.root_y as i32 - d.start_root.1;
                        if !d.moved && (dx.abs() > HANDLE_DRAG_SLOP || dy.abs() > HANDLE_DRAG_SLOP)
                        {
                            d.moved = true;
                        }
                        if d.moved {
                            let pos = clamp_handle_pos(
                                (d.origin.0 + dx, d.origin.1 + dy),
                                self.area,
                                self.kbd_h,
                            );
                            self.handle_pos = Some(pos);
                            let hr = Rect::new(pos.0, pos.1, HANDLE_W, HANDLE_H);
                            self.configure(self.handle, hr, true)?;
                            if self.fold.expanded {
                                let now = self.now_ms();
                                self.fold.touched(now);
                                self.configure(
                                    self.win,
                                    dock_rect_for_handle(self.area, hr),
                                    true,
                                )?;
                            }
                            self.xc.conn.flush()?;
                        }
                    }
                }
                Event::ButtonRelease(e) if e.event == self.handle && e.detail == 1 => {
                    let moved = self.drag.take().is_some_and(|d| d.moved);
                    let inside = Rect::new(0, 0, HANDLE_W, HANDLE_H)
                        .contains(e.event_x as i32, e.event_y as i32);
                    let down = self.pressed.take();
                    if !moved && inside && down == Some(Pressed::Handle) {
                        let now = self.now_ms();
                        self.fold.toggle(now);
                        self.apply_fold(fonts)?;
                    } else {
                        self.redraw_handle(fonts)?;
                    }
                }
                Event::ButtonPress(e) if e.event == self.win && e.detail == 1 => {
                    self.pressed =
                        button_at(w, e.event_x as i32, e.event_y as i32).map(Pressed::Button);
                    let now = self.now_ms();
                    self.fold.touched(now);
                    self.redraw(fonts)?;
                }
                Event::ButtonRelease(e) if e.event == self.win && e.detail == 1 => {
                    let hit = button_at(w, e.event_x as i32, e.event_y as i32);
                    let down = match self.pressed.take() {
                        Some(Pressed::Button(b)) => Some(b),
                        _ => None,
                    };
                    let now = self.now_ms();
                    self.fold.touched(now);
                    self.redraw(fonts)?;
                    if let (Some(b), true) = (hit, hit == down) {
                        if b == DockButton::Home {
                            self.raise_shell()?;
                            // going Home puts the system keyboard and the mouse pad away
                            cmds.push(Cmd::KeyboardHide);
                            cmds.push(Cmd::MousePadHide);
                        } else if b == DockButton::CloseApp {
                            // by window id, not by guessing the app from WM_CLASS (owner
                            // report, 2026-11-12: the dock's close button did nothing while in
                            // a terminal - terminal and Kali share one class, "st", so
                            // `app_for_class` could pick the wrong app, or an app that was not
                            // even running, and silently close nothing)
                            if self.active != 0 {
                                cmds.push(Cmd::CloseWindow(self.active));
                            }
                        } else {
                            let class = self.class_of(self.active);
                            cmds.extend(command_for(b, &self.apps, &class));
                        }
                        // an action folds the strip back
                        self.collapse(fonts)?;
                    }
                }
                Event::Error(e) => return Err(XError(format!("dock X error: {e:?}"))),
                _ => {}
            }
        }
        let now = self.now_ms();
        if self.fold.tick(now) {
            self.pressed = None;
            self.apply_fold(fonts)?;
        }
        Ok(cmds)
    }
}
