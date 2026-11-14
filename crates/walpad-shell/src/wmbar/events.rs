use super::model::{hit, Hit, BAR_H, DRAG_SEND_MS, DRAG_SLOP};
use super::window::{Drag, DragKind, WmBar};
use crate::app::Cmd;
use std::time::{Duration, Instant};
use walpad_proto::wm::{self, SnapZone, WRect};
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::Event;

impl WmBar {
    fn start_drag(&mut self, kind: DragKind, root_xy: (i32, i32)) {
        let w = self.model.active;
        if let Some(start) = self.geometry(w) {
            self.drag = Some(Drag {
                kind,
                window: w,
                start,
                at: root_xy,
                moved: false,
                last_sent: Instant::now() - Duration::from_millis(DRAG_SEND_MS),
            });
        }
    }

    fn drag_cmd(&self, d: &Drag, root_xy: (i32, i32)) -> Cmd {
        let (dx, dy) = (root_xy.0 - d.at.0, root_xy.1 - d.at.1);
        // the bar is not part of the area a window may use
        let area = WRect::new(0, BAR_H, self.area.0, self.area.1 - BAR_H);
        let r = match d.kind {
            DragKind::Move => wm::move_rect(d.start, dx, dy, area),
            DragKind::Resize => wm::resize_rect(d.start, dx, dy, area),
        };
        Cmd::WindowRect {
            window: d.window,
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
        }
    }

    /// Handle everything the server has sent (non-blocking) and the refresh timer.
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<Cmd>, XError> {
        let mut cmds = Vec::new();
        let mut dirty = false;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e)
                    if e.atom == self.atoms.active || e.atom == self.atoms.client_list =>
                {
                    dirty = true;
                }
                Event::Expose(e) if e.window == self.bar && e.count == 0 => self.redraw(fonts)?,
                Event::Expose(e) if e.window == self.grip && e.count == 0 => {
                    self.redraw_grip(fonts)?
                }
                Event::ButtonPress(e) if e.detail == 1 && e.event == self.bar => {
                    self.pressed = hit(&self.model, e.event_x as i32, e.event_y as i32);
                    if self.pressed == Some(Hit::Drag) && self.model.free {
                        self.start_drag(DragKind::Move, (e.root_x as i32, e.root_y as i32));
                    }
                    self.redraw(fonts)?;
                }
                Event::ButtonPress(e) if e.detail == 1 && e.event == self.grip => {
                    self.grip_pressed = true;
                    self.start_drag(DragKind::Resize, (e.root_x as i32, e.root_y as i32));
                    self.redraw_grip(fonts)?;
                }
                Event::MotionNotify(e) if e.event == self.bar || e.event == self.grip => {
                    let xy = (e.root_x as i32, e.root_y as i32);
                    if let Some(mut d) = self.drag.take() {
                        if (xy.0 - d.at.0).abs() + (xy.1 - d.at.1).abs() >= DRAG_SLOP {
                            d.moved = true;
                        }
                        if d.moved && d.last_sent.elapsed() >= Duration::from_millis(DRAG_SEND_MS) {
                            cmds.push(self.drag_cmd(&d, xy));
                            d.last_sent = Instant::now();
                        }
                        self.drag = Some(d);
                    }
                }
                Event::ButtonRelease(e) if e.detail == 1 && e.event == self.bar => {
                    let down = self.pressed.take();
                    let up = hit(&self.model, e.event_x as i32, e.event_y as i32);
                    let drag = self.drag.take();
                    if let Some(d) = drag.filter(|d| d.moved) {
                        cmds.push(self.drag_cmd(&d, (e.root_x as i32, e.root_y as i32)));
                        dirty = true;
                    } else if down.is_some() && down == up {
                        cmds.extend(self.action(down));
                        dirty = true;
                    }
                    self.redraw(fonts)?;
                }
                Event::ButtonRelease(e) if e.detail == 1 && e.event == self.grip => {
                    self.grip_pressed = false;
                    if let Some(d) = self.drag.take().filter(|d| d.moved) {
                        cmds.push(self.drag_cmd(&d, (e.root_x as i32, e.root_y as i32)));
                    }
                    dirty = true;
                    self.redraw_grip(fonts)?;
                }
                Event::Error(e) => return Err(XError(format!("window bar X error: {e:?}"))),
                _ => {}
            }
        }
        if dirty || (self.app_up && Instant::now() >= self.next_refresh) {
            self.refresh(fonts)?;
        }
        Ok(cmds)
    }

    /// What a tap on a hit area asks the core for.
    fn action(&self, h: Option<Hit>) -> Option<Cmd> {
        match h? {
            Hit::Tab(w) => Some(Cmd::FocusWindow(w)),
            Hit::TabClose(w) => Some(Cmd::CloseWindow(w)),
            Hit::New => Some(Cmd::NewInstance(self.app_id.clone())),
            Hit::Snap(z) => Some(Cmd::Snap {
                window: self.model.active,
                zone: match z {
                    SnapZone::Left => "left",
                    SnapZone::Right => "right",
                    SnapZone::Top => "top",
                    SnapZone::Bottom => "bottom",
                    SnapZone::Full => "full",
                },
            }),
            Hit::Drag => None,
        }
    }
}
