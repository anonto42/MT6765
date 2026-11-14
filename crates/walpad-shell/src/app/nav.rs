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
use crate::dock::visible_area;
use crate::edge::{zone_rect, EdgeSwipe};
use crate::gesture::Recogniser;
use crate::recents::Recents;
use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Icon, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, PropMode, Window as XWindow, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

mod strip;
mod x11;

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

pub(super) struct Atoms {
    active: u32,
    stacking: u32,
    client_list: u32,
}

/// `(app id, name, icon)` of every installed app (the cards' labels).
pub type Catalog = Vec<(String, String, Icon)>;

pub struct Nav {
    pub(super) xc: XConn,
    pub(super) edge: XWindow,
    rwin: XWindow,
    gc: u32,
    depth: u8,
    pub(super) atoms: Atoms,
    shell: XWindow,
    classes: Vec<(String, String)>,
    pub(super) catalog: Catalog,
    area: (i32, i32),
    /// The keyboard, the mouse pad or the expanded dock is up: no edge zone then.
    pub(super) blocked: bool,
    active: u32,
    edge_mapped: bool,
    swipe: EdgeSwipe,
    rec: Recogniser,
    pub(super) recents: Option<Recents>,
    canvas: Canvas,
    /// Last capture per app id.
    pub(super) thumbs: HashMap<String, Canvas>,
    /// The strip must be painted again.
    dirty: bool,
    pub(super) next_capture: Option<Instant>,
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
            canvas: Canvas::new(area.0 as u32, area.1 as u32, bg()).ok_or("recents canvas")?,
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

    pub fn set_shell_size(&mut self, w: i32, h: i32) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.swipe.set_area(area);
        self.canvas = Canvas::new(area.0 as u32, area.1 as u32, bg()).ok_or("recents canvas")?;
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
}
