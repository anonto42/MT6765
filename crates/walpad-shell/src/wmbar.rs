//! Window bar (change multitasking-windows): a 48 px strip at the top of the visible area while an
//! app window is in front and either the app runs several windows (the terminal: one tab per
//! window, a "+" button, an x per tab) or the layout mode is Free (the bar is the title bar:
//! drag it to move the window, buttons snap it left / right / top / bottom / full) plus a
//! corner grip on the window that resizes it. Override-redirect windows with WM_HINTS
//! input=false (they never take the keyboard focus from the terminal, so the system keyboard
//! keeps typing into it) and a managed 1x1 `walpad-wmbar-strut` window that carries the top
//! strut, like the dock. Own X connection; every wait has a deadline (walpad-x). The decisions
//! are pure functions (tested); the core clamps every placement again.
use crate::app::Cmd;
use crate::windows_ui::{read_prefs, WindowsPrefs};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::wm::{self, LayoutMode, SnapZone, WRect};
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect, Style};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, PropMode, StackMode, Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const BAR_H: i32 = 48;
pub const BAR_CLASS: &str = "walpad-wmbar";
pub const GRIP_CLASS: &str = "walpad-wmgrip";
/// Hit size of the resize grip.
pub const GRIP: i32 = 56;
pub const TAB_MIN_W: i32 = 76;
pub const TAB_MAX_W: i32 = 168;
pub const PLUS_W: i32 = 52;
pub const CLOSE_W: i32 = 40;
pub const SNAP_W: i32 = 48;
/// Room kept for the drag area between the tabs and the snap buttons.
const DRAG_MIN_W: i32 = 40;
/// While a window is dragged a placement is sent at most this often.
const DRAG_SEND_MS: u64 = 70;
/// How often the bar looks at the window facts while an app is in front.
const REFRESH_MS: u64 = 500;
/// Smallest movement (px) that counts as a drag, not a tap.
const DRAG_SLOP: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub win: u32,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Tab(u32),
    TabClose(u32),
    New,
    Snap(SnapZone),
    /// The free area of the bar: drags the window in free mode.
    Drag,
}

/// An app of apps.json as far as the bar needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub class: String,
    /// `max_instances` (1 = one window only).
    pub max: u32,
}

pub fn load_apps(root: &Root) -> Vec<AppInfo> {
    walpad_proto::appdefs::entries(
        &root.p(walpad_proto::appdefs::APPS_JSON),
        &root.p(walpad_proto::appdefs::APPS_D),
    )
    .iter()
    .filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let class = e["wm_class"]
            .as_str()
            .filter(|c| !c.is_empty())?
            .to_string();
        Some(AppInfo {
            name: e["name"].as_str().unwrap_or(&id).to_string(),
            max: e["max_instances"].as_u64().unwrap_or(1).max(1) as u32,
            id,
            class,
        })
    })
    .collect()
}

/// The first app whose wm_class is one of the window's class names (case-insensitive).
pub fn app_for<'a>(apps: &'a [AppInfo], class: &[String]) -> Option<&'a AppInfo> {
    apps.iter()
        .find(|a| class.iter().any(|c| c.eq_ignore_ascii_case(&a.class)))
}

/// Should the bar be up for the window in front?
pub fn wants_visible(app: Option<&AppInfo>, mode: LayoutMode) -> bool {
    app.is_some_and(|a| a.max > 1 || mode == LayoutMode::Free)
}

/// Tab text: "Terminal 2" when there is room, else the number.
pub fn tab_label(name: &str, n: usize, tab_w: i32) -> String {
    if tab_w >= 120 {
        format!("{name} {n}")
    } else {
        n.to_string()
    }
}

/// What the bar draws and where its hit areas are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarModel {
    pub width: i32,
    /// One tab per window of a multi-window app (empty = a single title instead).
    pub tabs: Vec<Tab>,
    pub active: u32,
    pub title: String,
    pub can_new: bool,
    /// Free mode: snap buttons and a drag area.
    pub free: bool,
}

pub const SNAP_BUTTONS: [SnapZone; 5] = [
    SnapZone::Left,
    SnapZone::Right,
    SnapZone::Top,
    SnapZone::Bottom,
    SnapZone::Full,
];

/// Width of one tab (all equal) for `n` tabs in the room left of the snap buttons.
pub fn tab_width(m: &BarModel) -> i32 {
    let n = m.tabs.len().max(1) as i32;
    let snap = if m.free {
        SNAP_BUTTONS.len() as i32 * SNAP_W + SP_2
    } else {
        0
    };
    let plus = if m.can_new { PLUS_W } else { 0 };
    let avail = m.width - SP_2 - snap - plus - DRAG_MIN_W;
    (avail / n).clamp(TAB_MIN_W, TAB_MAX_W)
}

/// Hit areas in bar coordinates; the Drag area comes last (anything not hit before it).
pub fn layout(m: &BarModel) -> Vec<(Rect, Hit)> {
    let mut v = Vec::new();
    let mut x = SP_2;
    let tw = tab_width(m);
    for t in &m.tabs {
        v.push((
            Rect::new(x + tw - CLOSE_W, 0, CLOSE_W, BAR_H),
            Hit::TabClose(t.win),
        ));
        v.push((Rect::new(x, 0, tw - CLOSE_W, BAR_H), Hit::Tab(t.win)));
        x += tw;
    }
    if m.can_new {
        v.push((Rect::new(x, 0, PLUS_W, BAR_H), Hit::New));
        x += PLUS_W;
    }
    let mut end = m.width;
    if m.free {
        for z in SNAP_BUTTONS.iter().rev() {
            end -= SNAP_W;
            v.push((Rect::new(end, 0, SNAP_W, BAR_H), Hit::Snap(*z)));
        }
    }
    v.push((Rect::new(x, 0, (end - x).max(0), BAR_H), Hit::Drag));
    v
}

pub fn hit(m: &BarModel, x: i32, y: i32) -> Option<Hit> {
    layout(m)
        .into_iter()
        .find(|(r, _)| r.contains(x, y))
        .map(|(_, h)| h)
}

/// `_NET_WM_STRUT_PARTIAL` of the bar: a top strut over the visible width.
pub fn strut_partial(width: i32) -> [u32; 12] {
    [
        0,
        0,
        BAR_H as u32,
        0,
        0,
        0,
        0,
        0,
        0,
        (width - 1).max(0) as u32,
        0,
        0,
    ]
}

/// The corner grip: inside the bottom-right corner of the window, kept on screen.
pub fn grip_rect(win: WRect, area: (i32, i32)) -> Rect {
    let x = (win.right() - GRIP).clamp(0, (area.0 - GRIP).max(0));
    let y = (win.bottom() - GRIP).clamp(0, (area.1 - GRIP).max(0));
    Rect::new(x, y, GRIP, GRIP)
}

/// A small picture of a snap zone: an outlined window with the target part filled.
fn snap_glyph(p: &mut Painter, zone: SnapZone, at: Rect, colour: Color) {
    let s = 22;
    let g = Rect::new(at.x + (at.w - s) / 2, at.y + (at.h - s) / 2, s, s);
    p.cv.rrect_stroke(g, 3, 1.5, colour);
    let half = (s - 4) / 2;
    let fill = match zone {
        SnapZone::Left => Rect::new(g.x + 2, g.y + 2, half, s - 4),
        SnapZone::Right => Rect::new(g.x + 2 + half, g.y + 2, half, s - 4),
        SnapZone::Top => Rect::new(g.x + 2, g.y + 2, s - 4, half),
        SnapZone::Bottom => Rect::new(g.x + 2, g.y + 2 + half, s - 4, half),
        SnapZone::Full => Rect::new(g.x + 2, g.y + 2, s - 4, s - 4),
    };
    p.cv.rect(fill, colour);
}

pub fn render(p: &mut Painter, m: &BarModel, pressed: Option<Hit>) {
    let w = p.cv.width();
    p.cv.rect(Rect::new(0, 0, w, BAR_H), SURFACE_STRONG.over(BG));
    p.divider(0, BAR_H - 1, w);
    let tw = tab_width(m);
    let mut x = SP_2;
    for (i, t) in m.tabs.iter().enumerate() {
        let r = Rect::new(x, 6, tw - SP_1, BAR_H - 12);
        let down = matches!(pressed, Some(Hit::Tab(wn) | Hit::TabClose(wn)) if wn == t.win);
        p.outline_box(r, r.h / 2, down, t.win == m.active, false);
        let label = if tw >= 120 {
            t.label.clone()
        } else {
            (i + 1).to_string()
        };
        let colour = if t.win == m.active {
            TEXT
        } else {
            TEXT_SECONDARY
        };
        p.text(
            &label,
            Style::new(Face::Regular, SIZE_SECONDARY, colour),
            Rect::new(r.x + SP_3, 0, r.w - CLOSE_W - SP_2, BAR_H),
            Align::Left,
        );
        let cx = x + tw - CLOSE_W + (CLOSE_W - 16) / 2 - 2;
        p.cv.icon(Icon::Close, cx, (BAR_H - 16) / 2, 16, TEXT_SECONDARY);
        x += tw;
    }
    if m.tabs.is_empty() && !m.title.is_empty() {
        p.text(
            &m.title,
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT_LABEL),
            Rect::new(SP_3, 0, w / 2, BAR_H),
            Align::Left,
        );
    }
    if m.can_new {
        let r = Rect::new(x + 6, 6, PLUS_W - 12, BAR_H - 12);
        p.outline_box(r, r.h / 2, pressed == Some(Hit::New), false, false);
        let (cx, cy) = r.center();
        let (cx, cy) = (cx as f32, cy as f32);
        p.cv.line((cx - 8.0, cy), (cx + 8.0, cy), 2.0, TEXT);
        p.cv.line((cx, cy - 8.0), (cx, cy + 8.0), 2.0, TEXT);
    }
    if m.free {
        for (r, h) in layout(m) {
            if let Hit::Snap(z) = h {
                let colour = if pressed == Some(h) { TEXT } else { TEXT_LABEL };
                snap_glyph(p, z, r, colour);
            }
        }
    }
}

/// The resize grip: two diagonal strokes in the corner.
pub fn render_grip(p: &mut Painter, pressed: bool) {
    let s = p.cv.width();
    p.cv.rrect(
        Rect::new(0, 0, s, s),
        10,
        if pressed {
            PRESSED_TINT.over(SURFACE_STRONG.over(BG))
        } else {
            SURFACE_STRONG.over(BG)
        },
    );
    p.cv.rrect_stroke(Rect::new(0, 0, s, s), 10, OUTLINE_W, CARD_BORDER);
    let m = s as f32;
    for k in [0.0f32, 9.0] {
        p.cv.line(
            (m - 14.0 - k, m - 10.0),
            (m - 10.0, m - 14.0 - k),
            2.0,
            TEXT,
        );
    }
}

// ------------------------------------------------------------------------------------------ X

struct Atoms {
    active: u32,
    client_list: u32,
    window_type: u32,
    type_dock: u32,
    strut_partial: u32,
    strut: u32,
    net_name: u32,
    utf8: u32,
}

/// WM_HINTS flags=InputHint, input=false: never take the keyboard focus from the app in front.
const NO_INPUT: [u32; 9] = [1, 0, 0, 0, 0, 0, 0, 0, 0];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragKind {
    Move,
    Resize,
}

struct Drag {
    kind: DragKind,
    window: u32,
    start: WRect,
    at: (i32, i32),
    moved: bool,
    last_sent: Instant,
}

pub struct WmBar {
    xc: XConn,
    bar: XWindow,
    grip: XWindow,
    strut_win: XWindow,
    gc: u32,
    grip_gc: u32,
    depth: u8,
    atoms: Atoms,
    apps: Vec<AppInfo>,
    root: Root,
    /// visible area (width, height)
    area: (i32, i32),
    shown: bool,
    grip_shown: bool,
    model: BarModel,
    prefs: WindowsPrefs,
    pressed: Option<Hit>,
    grip_pressed: bool,
    drag: Option<Drag>,
    canvas: Canvas,
    grip_canvas: Canvas,
    next_refresh: Instant,
    /// the active window is an app window (the periodic refresh runs)
    app_up: bool,
    app_id: String,
}

impl WmBar {
    pub fn open(
        socket: &Path,
        apps: Vec<AppInfo>,
        root: Root,
        shell_size: (i32, i32),
    ) -> Result<WmBar, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            active: intern("_NET_ACTIVE_WINDOW")?,
            client_list: intern("_NET_CLIENT_LIST")?,
            window_type: intern("_NET_WM_WINDOW_TYPE")?,
            type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            strut: intern("_NET_WM_STRUT")?,
            net_name: intern("_NET_WM_NAME")?,
            utf8: intern("UTF8_STRING")?,
        };
        let depth = conn
            .setup()
            .roots
            .first()
            .map(|s| s.root_depth)
            .unwrap_or(24);
        let area = crate::dock::visible_area(shell_size.0, shell_size.1);
        let events = EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::BUTTON_MOTION
            | EventMask::EXPOSURE;
        let make = |r: Rect, name: &str| -> Result<(XWindow, u32), XError> {
            let win = conn.generate_id()?;
            conn.create_window(
                0,
                win,
                xc.root,
                r.x as i16,
                r.y as i16,
                r.w as u16,
                r.h as u16,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new()
                    .event_mask(events)
                    .background_pixel(0x001A_1D21)
                    .override_redirect(1),
            )?;
            let gc = conn.generate_id()?;
            conn.create_gc(gc, win, &CreateGCAux::new())?;
            let class = format!("{name}\0{name}\0");
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                class.as_bytes(),
            )?;
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                name.as_bytes(),
            )?;
            conn.change_property8(
                PropMode::REPLACE,
                win,
                atoms.net_name,
                atoms.utf8,
                name.as_bytes(),
            )?;
            conn.change_property32(
                PropMode::REPLACE,
                win,
                atoms.window_type,
                AtomEnum::ATOM,
                &[atoms.type_dock],
            )?;
            conn.change_property32(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_HINTS,
                AtomEnum::WM_HINTS,
                &NO_INPUT,
            )?;
            Ok((win, gc))
        };
        let (bar, gc) = make(Rect::new(0, 0, area.0, BAR_H), BAR_CLASS)?;
        let (grip, grip_gc) = make(Rect::new(0, 0, GRIP, GRIP), GRIP_CLASS)?;
        let strut_win = conn.generate_id()?;
        conn.create_window(
            0,
            strut_win,
            xc.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            strut_win,
            atoms.window_type,
            AtomEnum::ATOM,
            &[atoms.type_dock],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &NO_INPUT,
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            atoms.net_name,
            atoms.utf8,
            b"walpad-wmbar-strut",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"walpad-wmbar-strut\0walpad-wmbar-strut\0",
        )?;
        let rootw = xc.root;
        conn.change_window_attributes(
            rootw,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        conn.flush()?;
        let b = WmBar {
            xc,
            bar,
            grip,
            strut_win,
            gc,
            grip_gc,
            depth,
            atoms,
            apps,
            prefs: read_prefs(&root),
            root,
            area,
            shown: false,
            grip_shown: false,
            model: BarModel {
                width: area.0,
                tabs: Vec::new(),
                active: 0,
                title: String::new(),
                can_new: false,
                free: false,
            },
            pressed: None,
            grip_pressed: false,
            drag: None,
            canvas: Canvas::new(area.0 as u32, BAR_H as u32, BG).ok_or("bar canvas")?,
            grip_canvas: Canvas::new(GRIP as u32, GRIP as u32, BG).ok_or("grip canvas")?,
            next_refresh: Instant::now(),
            app_up: false,
            app_id: String::new(),
        };
        b.write_strut()?;
        Ok(b)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }

    pub fn visible(&self) -> bool {
        self.shown
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.app_up
            .then(|| self.next_refresh.saturating_duration_since(Instant::now()))
    }

    /// The shell window changed size (rotation).
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = crate::dock::visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.model.width = area.0;
        self.canvas = Canvas::new(area.0 as u32, BAR_H as u32, BG).ok_or("bar canvas")?;
        self.xc.conn.configure_window(
            self.bar,
            &ConfigureWindowAux::new()
                .width(area.0 as u32)
                .height(BAR_H as u32),
        )?;
        self.write_strut()?;
        self.refresh(fonts)
    }

    fn write_strut(&self) -> Result<(), XError> {
        let s = strut_partial(self.area.0);
        let c = &self.xc.conn;
        c.change_property32(
            PropMode::REPLACE,
            self.strut_win,
            self.atoms.strut_partial,
            AtomEnum::CARDINAL,
            &s,
        )?;
        c.change_property32(
            PropMode::REPLACE,
            self.strut_win,
            self.atoms.strut,
            AtomEnum::CARDINAL,
            &s[..4],
        )?;
        Ok(())
    }

    fn read_window(&self, w: u32, atom: u32, ty: AtomEnum, len: u32) -> Vec<u32> {
        self.xc
            .conn
            .get_property(false, w, atom, ty, 0, len)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    fn class_of(&self, w: u32) -> Vec<String> {
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

    /// A window's rectangle in root coordinates.
    fn geometry(&self, w: u32) -> Option<WRect> {
        let g = self.xc.conn.get_geometry(w).ok()?.reply().ok()?;
        let t = self
            .xc
            .conn
            .translate_coordinates(w, self.xc.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some(WRect::new(
            t.dst_x as i32,
            t.dst_y as i32,
            g.width as i32,
            g.height as i32,
        ))
    }

    /// Look at the window in front and put the bar, the tabs and the grip where they belong.
    pub fn refresh(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        self.next_refresh = Instant::now() + Duration::from_millis(REFRESH_MS);
        if self.drag.is_some() {
            return Ok(()); // the window is being moved: leave the grip alone
        }
        self.prefs = read_prefs(&self.root);
        let active = self
            .read_window(self.xc.root, self.atoms.active, AtomEnum::WINDOW, 1)
            .first()
            .copied()
            .unwrap_or(0);
        let mine = [self.bar, self.grip, self.strut_win];
        let class = if active == 0 || mine.contains(&active) {
            Vec::new()
        } else {
            self.class_of(active)
        };
        // the shell's own windows (WM_CLASS walpad*) are no app windows
        let is_shell = class
            .iter()
            .any(|c| c.to_ascii_lowercase().starts_with("walpad"));
        let app = if is_shell {
            None
        } else {
            app_for(&self.apps, &class).cloned()
        };
        // a tap on the bar or the grip changes nothing about what is in front
        if mine.contains(&active) && self.shown {
            return Ok(());
        }
        self.app_up = !class.is_empty() && !is_shell;
        let on = wants_visible(app.as_ref(), self.prefs.mode);
        let mut model = BarModel {
            width: self.area.0,
            tabs: Vec::new(),
            active,
            title: String::new(),
            can_new: false,
            free: self.prefs.mode == LayoutMode::Free,
        };
        if let Some(a) = &app {
            self.app_id = a.id.clone();
            model.title = a.name.clone();
            if a.max > 1 {
                let wins =
                    self.read_window(self.xc.root, self.atoms.client_list, AtomEnum::WINDOW, 4096);
                let mut same: Vec<u32> = wins
                    .into_iter()
                    .filter(|w| {
                        self.class_of(*w)
                            .iter()
                            .any(|c| c.eq_ignore_ascii_case(&a.class))
                    })
                    .collect();
                same.sort_unstable();
                let tw = tab_width(&BarModel {
                    tabs: same
                        .iter()
                        .map(|w| Tab {
                            win: *w,
                            label: String::new(),
                        })
                        .collect(),
                    can_new: true,
                    ..model.clone()
                });
                model.tabs = same
                    .iter()
                    .enumerate()
                    .map(|(i, w)| Tab {
                        win: *w,
                        label: tab_label(&a.name, i + 1, tw),
                    })
                    .collect();
                model.can_new = model.tabs.len() < a.max as usize;
            }
        }
        if !on {
            self.set_shown(false)?;
            self.model = model;
            return Ok(());
        }
        let changed = model != self.model || !self.shown;
        self.model = model;
        self.set_shown(true)?;
        if changed {
            self.redraw(fonts)?;
        }
        self.place_grip(fonts, active)?;
        self.raise()?;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn set_shown(&mut self, on: bool) -> Result<(), XError> {
        if on == self.shown {
            return Ok(());
        }
        self.shown = on;
        let c = &self.xc.conn;
        if on {
            self.write_strut()?;
            c.map_window(self.strut_win)?;
            c.map_window(self.bar)?;
            c.configure_window(
                self.bar,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(self.area.0 as u32)
                    .height(BAR_H as u32)
                    .stack_mode(StackMode::ABOVE),
            )?;
        } else {
            c.unmap_window(self.bar)?;
            c.unmap_window(self.strut_win)?;
            c.unmap_window(self.grip)?;
            self.grip_shown = false;
            self.pressed = None;
        }
        c.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        let above = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        self.xc.conn.configure_window(self.bar, &above)?;
        if self.grip_shown {
            self.xc.conn.configure_window(self.grip, &above)?;
        }
        Ok(())
    }

    fn place_grip(&mut self, fonts: &mut Fonts, active: u32) -> Result<(), XError> {
        let want = self.model.free && self.shown && active != 0;
        let rect = if want { self.geometry(active) } else { None };
        match rect {
            Some(r) => {
                let g = grip_rect(r, self.area);
                self.xc.conn.configure_window(
                    self.grip,
                    &ConfigureWindowAux::new()
                        .x(g.x)
                        .y(g.y)
                        .width(GRIP as u32)
                        .height(GRIP as u32)
                        .stack_mode(StackMode::ABOVE),
                )?;
                if !self.grip_shown {
                    self.grip_shown = true;
                    self.xc.conn.map_window(self.grip)?;
                    self.redraw_grip(fonts)?;
                }
            }
            None => {
                if self.grip_shown {
                    self.grip_shown = false;
                    self.xc.conn.unmap_window(self.grip)?;
                }
            }
        }
        Ok(())
    }

    fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            &self.model,
            self.pressed,
        );
        crate::present::present(
            &self.xc.conn,
            self.bar,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    fn redraw_grip(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render_grip(
            &mut Painter {
                cv: &mut self.grip_canvas,
                fonts,
            },
            self.grip_pressed,
        );
        crate::present::present(
            &self.xc.conn,
            self.grip,
            self.grip_gc,
            self.depth,
            &self.grip_canvas,
            self.grip_canvas.bounds(),
        )
    }

    /// The drag target: the window in front (`model.active`).
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

#[cfg(test)]
mod tests {
    use super::*;

    fn apps() -> Vec<AppInfo> {
        vec![
            AppInfo {
                id: "browser".into(),
                name: "Browser".into(),
                class: "firefox-esr".into(),
                max: 1,
            },
            AppInfo {
                id: "terminal".into(),
                name: "Terminal".into(),
                class: "st".into(),
                max: 6,
            },
        ]
    }

    fn model(n: u32, free: bool, width: i32) -> BarModel {
        BarModel {
            width,
            tabs: (1..=n)
                .map(|i| Tab {
                    win: 100 + i,
                    label: format!("Terminal {i}"),
                })
                .collect(),
            active: 101,
            title: "Terminal".into(),
            can_new: n < 6,
            free,
        }
    }

    #[test]
    fn the_bar_shows_for_multi_window_apps_and_in_free_mode_only() {
        let a = apps();
        let term = app_for(&a, &["st".into(), "St".into()]);
        let ff = app_for(&a, &["Navigator".into(), "firefox-esr".into()]);
        assert!(wants_visible(term, LayoutMode::Single));
        assert!(wants_visible(term, LayoutMode::Split));
        assert!(!wants_visible(ff, LayoutMode::Single));
        assert!(!wants_visible(ff, LayoutMode::Split));
        assert!(wants_visible(ff, LayoutMode::Free));
        assert!(!wants_visible(None, LayoutMode::Free));
    }

    #[test]
    fn tabs_plus_and_snap_buttons_fit_in_both_widths_and_never_overlap() {
        for width in [800, 1280] {
            for n in 1..=6 {
                for free in [false, true] {
                    let m = model(n, free, width);
                    let l = layout(&m);
                    for (i, (a, _)) in l.iter().enumerate() {
                        assert!(a.x >= 0 && a.right() <= width, "{a:?} in {width}");
                        assert!(a.h == BAR_H);
                        for (b, _) in l.iter().skip(i + 1) {
                            let overlap = a.x < b.right() && b.x < a.right() && a.w > 0 && b.w > 0;
                            assert!(
                                !overlap,
                                "{a:?} overlaps {b:?} (n={n} free={free} w={width})"
                            );
                        }
                    }
                    // every tab is big enough to hit
                    assert!(tab_width(&m) >= TAB_MIN_W);
                    if free {
                        let snaps = l.iter().filter(|(_, h)| matches!(h, Hit::Snap(_))).count();
                        assert_eq!(snaps, 5);
                    }
                }
            }
        }
    }

    #[test]
    fn hits_name_the_tab_its_close_the_plus_the_snaps_and_the_drag_area() {
        let m = model(2, true, 800);
        let tw = tab_width(&m);
        assert_eq!(hit(&m, SP_2 + 10, 20), Some(Hit::Tab(101)));
        assert_eq!(hit(&m, SP_2 + tw - 10, 20), Some(Hit::TabClose(101)));
        assert_eq!(hit(&m, SP_2 + tw + 10, 20), Some(Hit::Tab(102)));
        assert_eq!(hit(&m, SP_2 + 2 * tw + 10, 20), Some(Hit::New));
        assert_eq!(hit(&m, 800 - 10, 20), Some(Hit::Snap(SnapZone::Full)));
        assert_eq!(
            hit(&m, 800 - 5 * SNAP_W + 10, 20),
            Some(Hit::Snap(SnapZone::Left))
        );
        assert_eq!(hit(&m, SP_2 + 2 * tw + PLUS_W + 5, 20), Some(Hit::Drag));
        assert_eq!(hit(&m, 10, BAR_H + 5), None);
        // no snap buttons outside free mode
        let m = model(2, false, 800);
        assert_eq!(hit(&m, 800 - 10, 20), Some(Hit::Drag));
    }

    #[test]
    fn plus_goes_away_at_the_window_limit_and_labels_shrink_with_the_tab() {
        let m = model(6, false, 800);
        assert!(!m.can_new);
        assert!(!layout(&m).iter().any(|(_, h)| *h == Hit::New));
        assert_eq!(tab_label("Terminal", 2, 168), "Terminal 2");
        assert_eq!(tab_label("Terminal", 2, 80), "2");
    }

    #[test]
    fn strut_is_a_top_strut_over_the_visible_width() {
        let s = strut_partial(800);
        assert_eq!(&s[..4], &[0, 0, BAR_H as u32, 0]);
        assert_eq!((s[8], s[9]), (0, 799));
    }

    #[test]
    fn grip_sits_in_the_window_corner_and_stays_on_screen() {
        let g = grip_rect(WRect::new(100, 100, 400, 300), (800, 1280));
        assert_eq!((g.x, g.y), (500 - GRIP, 400 - GRIP));
        let g = grip_rect(WRect::new(0, 0, 800, 1280), (800, 1280));
        assert_eq!((g.right(), g.bottom()), (800, 1280));
        let g = grip_rect(WRect::new(0, 0, 10, 10), (800, 1280));
        assert_eq!((g.x, g.y), (0, 0));
    }

    #[test]
    fn apps_json_gives_names_classes_and_limits() {
        let dir = std::env::temp_dir().join(format!("walpad-wmbar-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("etc/walpad")).unwrap();
        std::fs::write(
            dir.join("etc/walpad/apps.json"),
            r#"[{"id":"terminal","name":"Terminal","exec":["st"],"wm_class":"st","max_instances":6},{"id":"files","name":"Files","exec":["pcmanfm"],"wm_class":"pcmanfm"},{"id":"x","name":"X","exec":["x"]}]"#,
        )
        .unwrap();
        let a = load_apps(&Root::new(&dir));
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].max, a[1].max), (6, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
