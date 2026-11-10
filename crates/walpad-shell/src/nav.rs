//! System navigation (`app-switcher-navigation`): the bottom-edge swipe and the Recents strip.
//!
//! Over a foreign app an input-only, override-redirect window (`walpad-edge`) covers only the 24 px
//! edge zone (edge.rs); a touch that starts there is a swipe: short = Home, held = Recents. A
//! touch anywhere else never reaches this code. Recents is one override-redirect window over the
//! visible area (`walpad-recents`) drawn with recents.rs; thumbnails come from `XGetImage` of the
//! window in front (no compositor), taken when a swipe starts and every `CAPTURE_EVERY` while an
//! app is in front, and kept after it loses the focus. A capture that fails (window not viewable,
//! odd depth) leaves the card with the app icon. Own X connection; every wait has a deadline.
use crate::app::Cmd;
use crate::dock::{app_for_class, visible_area};
use crate::edge::{zone_rect, EdgeEvent, EdgeSwipe};
use crate::gesture::{Gesture, Input, Recogniser};
use crate::recents::{Action, Card, Recents};
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_ui::components::Painter;
use walpad_ui::tokens::BG;
use walpad_ui::{Canvas, Fonts, Icon, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux,
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, ImageFormat, MapState, PropMode,
    StackMode, Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const EDGE_CLASS: &str = "walpad-edge";
pub const RECENTS_CLASS: &str = "walpad-recents";
/// Refresh the thumbnail of the app in front this often.
pub const CAPTURE_EVERY: Duration = Duration::from_secs(10);
/// First capture after an app came to the front (it needs a moment to draw).
pub const CAPTURE_FIRST: Duration = Duration::from_millis(2000);
/// Largest stored thumbnail (px per side); cards never show more.
pub const THUMB_MAX: usize = 640;

/// What the loop has to do after `poll`.
#[derive(Debug, Default)]
pub struct NavOut {
    /// For the shell (`Shell::send`): `CloseApp`, keyboard/pad hide on Home.
    pub cmds: Vec<Cmd>,
    /// Toast texts (a close that got no answer).
    pub notices: Vec<String>,
}

struct Atoms {
    active: u32,
    stacking: u32,
    client_list: u32,
}

/// `(app id, name, icon)` of every installed app (the cards' labels).
pub type Catalog = Vec<(String, String, Icon)>;

pub struct Nav {
    xc: XConn,
    edge: XWindow,
    rwin: XWindow,
    gc: u32,
    depth: u8,
    atoms: Atoms,
    shell: XWindow,
    classes: Vec<(String, String)>,
    catalog: Catalog,
    area: (i32, i32),
    /// The keyboard, the mouse pad or the expanded dock is up: no edge zone then.
    blocked: bool,
    active: u32,
    edge_mapped: bool,
    swipe: EdgeSwipe,
    rec: Recogniser,
    recents: Option<Recents>,
    canvas: Canvas,
    /// Last capture per app id.
    thumbs: HashMap<String, Canvas>,
    /// The strip must be painted again.
    dirty: bool,
    next_capture: Option<Instant>,
    epoch: Instant,
}

fn intern(c: &impl Connection, n: &str) -> Result<u32, XError> {
    Ok(c.intern_atom(false, n.as_bytes())?.reply()?.atom)
}

impl Nav {
    pub fn open(
        socket: &Path,
        shell: XWindow,
        classes: Vec<(String, String)>,
        catalog: Catalog,
        shell_size: (i32, i32),
    ) -> Result<Nav, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let atoms = Atoms {
            active: intern(conn, "_NET_ACTIVE_WINDOW")?,
            stacking: intern(conn, "_NET_CLIENT_LIST_STACKING")?,
            client_list: intern(conn, "_NET_CLIENT_LIST")?,
        };
        let depth = conn
            .setup()
            .roots
            .first()
            .map(|s| s.root_depth)
            .unwrap_or(24);
        let area = visible_area(shell_size.0, shell_size.1);
        let z = zone_rect(area);
        let touch = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::BUTTON1_MOTION;
        let edge = conn.generate_id()?;
        conn.create_window(
            0,
            edge,
            xc.root,
            z.x as i16,
            z.y as i16,
            z.w as u16,
            z.h as u16,
            0,
            WindowClass::INPUT_ONLY,
            0,
            &CreateWindowAux::new()
                .override_redirect(1)
                .event_mask(touch),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            edge,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            format!("{EDGE_CLASS}\0{EDGE_CLASS}\0").as_bytes(),
        )?;
        let rwin = conn.generate_id()?;
        conn.create_window(
            0,
            rwin,
            xc.root,
            0,
            0,
            area.0 as u16,
            area.1 as u16,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0x002B_3035)
                .event_mask(touch | EventMask::EXPOSURE),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            rwin,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            format!("{RECENTS_CLASS}\0{RECENTS_CLASS}\0").as_bytes(),
        )?;
        let gc = conn.generate_id()?;
        conn.create_gc(gc, rwin, &CreateGCAux::new())?;
        conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        conn.flush()?;
        let mut n = Nav {
            xc,
            edge,
            rwin,
            gc,
            depth,
            atoms,
            shell,
            classes,
            catalog,
            area,
            blocked: false,
            active: 0,
            edge_mapped: false,
            swipe: EdgeSwipe::new(area),
            rec: Recogniser::new(),
            recents: None,
            canvas: Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("recents canvas")?,
            thumbs: HashMap::new(),
            dirty: false,
            next_capture: None,
            epoch: Instant::now(),
        };
        n.active = n.read_active();
        n.update_edge()?;
        Ok(n)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn recents_open(&self) -> bool {
        self.recents.is_some()
    }
    /// The edge zone window is mapped (an app is in front and nothing blocks the zone).
    pub fn edge_active(&self) -> bool {
        self.edge_mapped
    }
    pub fn edge_window(&self) -> XWindow {
        self.edge
    }
    pub fn recents_window(&self) -> XWindow {
        self.rwin
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Time until the next hold / timeout / capture needs `poll`.
    pub fn next_deadline(&self) -> Option<Duration> {
        let now = self.now_ms();
        let ms = [
            self.swipe.next_deadline(),
            self.rec.next_deadline(),
            self.recents.as_ref().and_then(|r| r.next_deadline()),
        ]
        .into_iter()
        .flatten()
        .min()
        .map(|t| Duration::from_millis(t.saturating_sub(now)));
        let cap = self
            .next_capture
            .map(|t| t.saturating_duration_since(Instant::now()));
        [ms, cap].into_iter().flatten().min()
    }

    fn read_active(&self) -> u32 {
        self.prop32(self.xc.root, self.atoms.active, AtomEnum::WINDOW, 1)
            .first()
            .copied()
            .unwrap_or(0)
    }

    fn prop32(&self, w: XWindow, atom: u32, ty: AtomEnum, len: u32) -> Vec<u32> {
        self.xc
            .conn
            .get_property(false, w, atom, ty, 0, len)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn class_of(&self, w: XWindow) -> Vec<String> {
        self.xc
            .conn
            .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| {
                r.value
                    .split(|b| *b == 0)
                    .filter(|s| !s.is_empty())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The app id of a window (None for the shell's and the system's own windows).
    fn app_of(&self, w: XWindow) -> Option<String> {
        if w == self.shell {
            return None;
        }
        app_for_class(&self.classes, &self.class_of(w))
    }

    /// Running apps with a window, most recently used first (top of the stack first), one window
    /// per app.
    fn windows(&self) -> Vec<(String, XWindow)> {
        let mut list = self.prop32(self.xc.root, self.atoms.stacking, AtomEnum::WINDOW, 4096);
        if list.is_empty() {
            list = self.prop32(self.xc.root, self.atoms.client_list, AtomEnum::WINDOW, 4096);
            list.reverse();
        }
        let mut out: Vec<(String, XWindow)> = Vec::new();
        for w in list.into_iter().rev() {
            if let Some(app) = self.app_of(w) {
                if !out.iter().any(|(a, _)| *a == app) {
                    out.push((app, w));
                }
            }
        }
        out
    }

    /// The shell window changed size (rotation).
    pub fn set_shell_size(&mut self, w: i32, h: i32) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.swipe.set_area(area);
        self.canvas = Canvas::new(area.0 as u32, area.1 as u32, BG).ok_or("recents canvas")?;
        let z = zone_rect(area);
        self.configure(self.edge, z)?;
        self.configure(self.rwin, Rect::new(0, 0, area.0, area.1))?;
        if let Some(r) = &mut self.recents {
            r.set_area(area);
        }
        self.dirty = self.recents.is_some();
        self.xc.conn.flush()?;
        Ok(())
    }

    fn configure(&self, w: XWindow, r: Rect) -> Result<(), XError> {
        self.xc.conn.configure_window(
            w,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        Ok(())
    }

    /// The system keyboard / mouse pad / expanded dock is up (true) or gone: the edge zone only
    /// exists while the bottom edge of the screen is free.
    pub fn set_blocked(&mut self, blocked: bool) -> Result<(), XError> {
        if blocked != self.blocked {
            self.blocked = blocked;
            self.update_edge()?;
        }
        Ok(())
    }

    /// Map / unmap / raise the edge window for the current state.
    fn update_edge(&mut self) -> Result<(), XError> {
        let want = !self.blocked && self.active != 0 && self.active != self.shell;
        if want {
            if !self.edge_mapped {
                self.xc.conn.map_window(self.edge)?;
            }
            // the window manager restacks the apps it manages: stay above them
            self.xc.conn.configure_window(
                self.edge,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )?;
        } else if self.edge_mapped {
            self.xc.conn.unmap_window(self.edge)?;
            self.swipe.set_area(self.area);
        }
        self.edge_mapped = want;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// `XGetImage` of a viewable window, boxed down to at most `THUMB_MAX` per side.
    fn capture(&self, w: XWindow) -> Option<Canvas> {
        let c = &self.xc.conn;
        let at = c.get_window_attributes(w).ok()?.reply().ok()?;
        if at.map_state != MapState::VIEWABLE {
            return None;
        }
        let g = c.get_geometry(w).ok()?.reply().ok()?;
        let (sw, sh) = (g.width as usize, g.height as usize);
        if sw == 0 || sh == 0 {
            return None;
        }
        let img = c
            .get_image(ImageFormat::Z_PIXMAP, w, 0, 0, g.width, g.height, !0)
            .ok()?
            .reply()
            .ok()?;
        if img.data.len() != sw * sh * 4 {
            return None;
        }
        let k = (THUMB_MAX as f32 / sw as f32)
            .min(THUMB_MAX as f32 / sh as f32)
            .min(1.0);
        let tw = ((sw as f32 * k) as u32).max(1);
        let th = ((sh as f32 * k) as u32).max(1);
        Canvas::from_bgrx_scaled(sw, sh, &img.data, tw, th)
    }

    /// Refresh the cached thumbnail of the app in front.
    fn capture_active(&mut self) {
        let w = self.active;
        if w == 0 || self.blocked {
            return;
        }
        let Some(app) = self.app_of(w) else { return };
        if let Some(t) = self.capture(w) {
            self.thumbs.insert(app, t);
        }
    }

    /// `_NET_ACTIVE_WINDOW` request to the window manager (source: pager), plus a restack.
    fn activate(&self, w: XWindow) -> Result<(), XError> {
        let c = &self.xc.conn;
        let ev = ClientMessageEvent::new(32, w, self.atoms.active, [2, 0, 0, 0, 0]);
        c.send_event(
            false,
            self.xc.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        c.configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        c.flush()?;
        Ok(())
    }

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
