use super::{Nav, NavOut, CAPTURE_EVERY, CAPTURE_FIRST};
use crate::app::Cmd;
use crate::edge::EdgeEvent;
use crate::gesture::{Gesture, Input, Recogniser};
use crate::recents::{Action, Card, Recents};
use std::time::Instant;
use walpad_ui::components::Painter;
use walpad_ui::{Fonts, Icon};
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    ConfigureWindowAux, ConnectionExt as _, StackMode, Window as XWindow,
};
use x11rb::protocol::Event;

impl Nav {
    fn open_recents(&mut self) -> Result<(), XError> {
        let wins = self.windows();
        self.thumbs.retain(|a, _| wins.iter().any(|(w, _)| w == a));
        let cards = wins
            .iter()
            .map(|(app, _)| {
                let (name, icon) = self
                    .catalog
                    .iter()
                    .find(|(id, _, _)| id == app)
                    .map(|(_, n, i)| (n.clone(), *i))
                    .unwrap_or((app.clone(), Icon::Apps));
                Card {
                    app: app.clone(),
                    name,
                    icon,
                    thumb: self.thumbs.get(app).and_then(|t| t.crop(t.bounds())),
                }
            })
            .collect();
        self.recents = Some(Recents::new(self.area, cards));
        self.rec = Recogniser::new();
        self.xc.conn.map_window(self.rwin)?;
        self.xc.conn.configure_window(
            self.rwin,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        // painted by `poll` right away (a remapped window is not always sent an Expose)
        self.dirty = true;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn close_recents(&mut self) -> Result<(), XError> {
        if self.recents.take().is_some() {
            self.xc.conn.unmap_window(self.rwin)?;
            self.rec = Recogniser::new();
            self.dirty = false;
            self.xc.conn.flush()?;
        }
        Ok(())
    }

    fn draw_recents(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        self.dirty = false;
        let Some(r) = &self.recents else {
            return Ok(());
        };
        r.render(&mut Painter {
            cv: &mut self.canvas,
            fonts,
        });
        crate::present::present(
            &self.xc.conn,
            self.rwin,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    /// Answer to a `close_app` the strip asked for (`ok`: the core accepted it).
    pub fn close_result(&mut self, app: &str, ok: bool) {
        if let Some(r) = &mut self.recents {
            r.close_result(app, ok);
            self.dirty = true;
        }
        if ok {
            self.thumbs.remove(app);
        }
    }

    /// A close request for `app` is waiting for the core's answer.
    pub fn is_closing(&self, app: &str) -> bool {
        self.recents.as_ref().is_some_and(|r| r.is_closing(app))
    }

    /// What the edge swipe decided.
    fn on_edge(&mut self, evs: Vec<EdgeEvent>, out: &mut NavOut) -> Result<(), XError> {
        for ev in evs {
            match ev {
                EdgeEvent::Started => self.capture_active(),
                EdgeEvent::Home => {
                    self.activate(self.shell)?;
                    // going Home puts the system keyboard and the mouse pad away
                    out.cmds.push(Cmd::KeyboardHide);
                    out.cmds.push(Cmd::MousePadHide);
                }
                EdgeEvent::Recents => self.open_recents()?,
                EdgeEvent::Cancelled => {}
            }
        }
        Ok(())
    }

    fn on_strip(&mut self, gs: Vec<Gesture>, out: &mut NavOut) -> Result<(), XError> {
        let now = self.now_ms();
        for g in gs {
            let Some(r) = &mut self.recents else { break };
            let actions = r.on_gesture(&g, now);
            self.dirty = true;
            for a in actions {
                match a {
                    Action::Focus(app) => {
                        let w = self.windows().into_iter().find(|(id, _)| *id == app);
                        self.close_recents()?;
                        if let Some((_, w)) = w {
                            self.activate(w)?;
                        }
                        return Ok(());
                    }
                    Action::Close(app) => out.cmds.push(Cmd::CloseApp(app)),
                    Action::Dismiss => {
                        self.close_recents()?;
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle everything the server has sent (non-blocking) and the timers.
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<NavOut, XError> {
        let mut out = NavOut::default();
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            let now = self.now_ms();
            match ev {
                Event::PropertyNotify(e) if e.atom == self.atoms.active => {
                    let was = self.active;
                    self.active = self.read_active();
                    if self.active != was {
                        // another window came to the front (Home key, an app of its own): the
                        // strip is stale
                        self.close_recents()?;
                        self.next_capture = Some(Instant::now() + CAPTURE_FIRST);
                    }
                    self.update_edge()?;
                }
                Event::PropertyNotify(e) if e.atom == self.atoms.stacking => {
                    // the window manager restacked the apps: stay above them
                    if self.edge_mapped {
                        self.update_edge()?;
                    }
                }
                Event::Expose(e) if e.window == self.rwin && e.count == 0 => self.dirty = true,
                Event::ButtonPress(e) if e.detail == 1 => {
                    let i = Input::Down {
                        x: e.root_x as i32,
                        y: e.root_y as i32,
                    };
                    self.feed(e.event, i, now, &mut out)?;
                }
                Event::MotionNotify(e) => {
                    let i = Input::Move {
                        x: e.root_x as i32,
                        y: e.root_y as i32,
                    };
                    self.feed(e.event, i, now, &mut out)?;
                }
                Event::ButtonRelease(e) if e.detail == 1 => {
                    let i = Input::Up {
                        x: e.root_x as i32,
                        y: e.root_y as i32,
                    };
                    self.feed(e.event, i, now, &mut out)?;
                }
                Event::Error(e) => return Err(XError(format!("nav X error: {e:?}"))),
                _ => {}
            }
        }
        let now = self.now_ms();
        let evs = self.swipe.tick(now);
        self.on_edge(evs, &mut out)?;
        let gs = self.rec.tick(now);
        self.on_strip(gs, &mut out)?;
        if let Some(r) = &mut self.recents {
            let late = r.tick(now);
            if !late.is_empty() {
                self.dirty = true;
                out.notices.push("Could not close the app".into());
            }
        }
        if self.next_capture.is_some_and(|t| Instant::now() >= t) {
            self.next_capture = None;
            if self.edge_mapped && self.recents.is_none() {
                self.capture_active();
                self.next_capture = Some(Instant::now() + CAPTURE_EVERY);
            }
        }
        if self.dirty {
            self.draw_recents(fonts)?;
        }
        Ok(out)
    }

    fn feed(
        &mut self,
        window: XWindow,
        i: Input,
        now: u64,
        out: &mut NavOut,
    ) -> Result<(), XError> {
        if window == self.edge {
            let evs = self.swipe.feed(i, now);
            self.on_edge(evs, out)
        } else if window == self.rwin {
            let gs = self.rec.feed(i, now);
            self.on_strip(gs, out)
        } else {
            Ok(())
        }
    }
}
