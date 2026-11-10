//! System dock (decision D3): a 56 px strip with Home, Keyboard and Close app for the app in front
//! (Firefox, the terminal, ...), so those apps can always reach Home by touch.
//!
//! Collapsible (owner request 2026-11-10): while a non-shell window is `_NET_ACTIVE_WINDOW` only a
//! small arrow handle (WM_CLASS `walpad-dock-handle`, 56x28, rounded top corners) sits at the
//! right end of the bottom edge, without a strut, so the app keeps its whole height. A tap on the
//! handle expands the strip (WM_CLASS `walpad-dock`) with a bottom strut (a managed 1x1
//! `walpad-dock-strut` window) and turns the arrow down; the strip collapses again after an
//! action, after `AUTO_COLLAPSE_MS` without a tap, or on a second tap on the handle. Both windows
//! are override-redirect with WM_HINTS input=false, so they never take the focus. While the system
//! keyboard or the mouse pad is up, handle and strip sit directly above it. The dock has its own
//! X connection; every wait on it has a deadline (walpad-x).
use crate::app::Cmd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect, Style};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ConfigureWindowAux,
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, MapState, PropMode, StackMode,
    Window as XWindow, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const DOCK_H: i32 = 56;
pub const WM_CLASS_INSTANCE: &str = "walpad-dock";
pub const WM_CLASS_CLASS: &str = "WalPad-Dock";
pub const HANDLE_CLASS: &str = "walpad-dock-handle";
/// The collapsed dock: an arrow handle, touch-wide, half a dock high.
pub const HANDLE_W: i32 = 56;
pub const HANDLE_H: i32 = 28;
/// Gap between the handle and the right edge of the visible area.
pub const HANDLE_MARGIN: i32 = SP_2;
/// Rounded top corners of the handle (the SHAPE region and the drawing).
pub const HANDLE_RADIUS: i32 = 10;
/// The expanded strip folds back after this long without a tap.
pub const AUTO_COLLAPSE_MS: u64 = 5000;
/// Title of the shell window (what Home raises).
pub const SHELL_TITLE: &str = crate::window::TITLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockButton {
    Back,
    Home,
    Keyboard,
    CloseApp,
}

pub const BUTTONS: [(DockButton, &str, Icon); 4] = [
    (DockButton::Back, "Back", Icon::Back),
    (DockButton::Home, "Home", Icon::Home),
    (DockButton::Keyboard, "Keyboard", Icon::Keyboard),
    (DockButton::CloseApp, "Close app", Icon::Close),
];

/// The visible area for a shell window of this size. The shell shrinks by the dock's own strut
/// while the dock is up (a maximised window respects struts), so the orientation is taken from the
/// aspect and the full panel size from CONTRACTS (800x1280 / 1280x800), never from the shrunk size.
pub fn visible_area(shell_w: i32, shell_h: i32) -> (i32, i32) {
    if shell_w > shell_h {
        (1280, 800)
    } else {
        (800, 1280)
    }
}

/// The dock window: full width, flush with the bottom of the visible area, or directly above the
/// system keyboard while it is up (`kbd_h` = its height, 0 when hidden).
pub fn dock_rect(area: (i32, i32), kbd_h: i32) -> Rect {
    Rect::new(0, area.1 - kbd_h - DOCK_H, area.0, DOCK_H)
}

/// The handle: at the right end, directly on top of the strip when expanded, else on the bottom
/// edge of the visible area (above the keyboard / mouse pad while one is up).
pub fn handle_rect(area: (i32, i32), kbd_h: i32, expanded: bool) -> Rect {
    let bottom = if expanded {
        dock_rect(area, kbd_h).y
    } else {
        area.1 - kbd_h
    };
    Rect::new(
        area.0 - HANDLE_W - HANDLE_MARGIN,
        bottom - HANDLE_H,
        HANDLE_W,
        HANDLE_H,
    )
}

/// The handle's outline as rectangles (SHAPE bounding region, handle-local): rounded top
/// corners, square bottom (it sits on the screen edge or on the strip).
pub fn handle_shape() -> Vec<Rect> {
    let r = HANDLE_RADIUS;
    let mut out = Vec::new();
    for y in 0..r {
        let dy = (r - y) as f32 - 0.5;
        let inset = (r as f32 - ((r * r) as f32 - dy * dy).max(0.0).sqrt()).round() as i32;
        out.push(Rect::new(inset, y, HANDLE_W - 2 * inset, 1));
    }
    out.push(Rect::new(0, r, HANDLE_W, HANDLE_H - r));
    out
}

/// Draw the handle with its top-left corner at `at` (in its own window: (0, 0)). Only the shaped
/// area is painted: the opaque strip colour, a line-art outline (open at the bottom) and an arrow
/// pointing where a tap moves the strip: up = expand, down = collapse.
pub fn render_handle(p: &mut Painter, at: (i32, i32), expanded: bool, pressed: bool) {
    let r = Rect::new(at.0, at.1, HANDLE_W, HANDLE_H);
    // taller than the window so the bottom corners are square (the window clips them)
    let tall = Rect::new(r.x, r.y, r.w, r.h + HANDLE_RADIUS + 2);
    let strip = SURFACE_STRONG.over(BG);
    let fill = if pressed {
        PRESSED_TINT.over(strip)
    } else {
        strip
    };
    // paint into a handle-sized canvas (it clips the bottom corners square), then copy only the
    // shaped rows, so whatever lies around the rounded corners stays as it is
    let Some(mut c) = Canvas::new(HANDLE_W as u32, HANDLE_H as u32, BG) else {
        return;
    };
    let local = Rect::new(0, 0, tall.w, tall.h);
    c.rrect(local, HANDLE_RADIUS, fill);
    c.rrect_stroke(local, HANDLE_RADIUS, OUTLINE_W, CARD_BORDER);
    let (cx, cy) = (HANDLE_W as f32 / 2.0, HANDLE_H as f32 / 2.0 + 1.0);
    let (dx, dy) = (8.0, 4.5);
    let (tip, base) = if expanded {
        (cy + dy, cy - dy)
    } else {
        (cy - dy, cy + dy)
    };
    c.line((cx - dx, base), (cx, tip), 2.0, TEXT);
    c.line((cx, tip), (cx + dx, base), 2.0, TEXT);
    for s in handle_shape() {
        p.cv.copy_rect_from_at(&c, (r.x, r.y), Rect::new(r.x + s.x, r.y + s.y, s.w, s.h));
    }
}

/// `_NET_WM_STRUT_PARTIAL`: struts count from the screen edge, so the part of the root below the
/// visible area is included (landscape: 480 + 56), and so is the keyboard under the dock (the
/// combined strut; the keyboard's own strut is smaller, the window manager takes the largest).
pub fn strut_partial(area: (i32, i32), kbd_h: i32, root_h: i32) -> [u32; 12] {
    crate::keyboard::bottom_strut(dock_rect(area, kbd_h), root_h)
}

/// Hit areas of the buttons (equal cells of the strip, strip-local coordinates).
pub fn button_rects(w: i32) -> [(Rect, DockButton); BUTTONS.len()] {
    let n = BUTTONS.len() as i32;
    let cell = w / n;
    let mut out = [(Rect::new(0, 0, 0, 0), DockButton::Home); BUTTONS.len()];
    for (i, (b, _, _)) in BUTTONS.iter().enumerate() {
        let x = i as i32 * cell;
        let cw = if i as i32 == n - 1 {
            w - (n - 1) * cell
        } else {
            cell
        };
        out[i] = (Rect::new(x, 0, cw, DOCK_H), *b);
    }
    out
}

pub fn button_at(w: i32, x: i32, y: i32) -> Option<DockButton> {
    button_rects(w)
        .iter()
        .find(|(r, _)| r.contains(x, y))
        .map(|(_, b)| *b)
}

/// Draw the strip (`cv` is `w` x 56). `pressed` highlights one button.
pub fn render(p: &mut Painter, pressed: Option<DockButton>) {
    let w = p.cv.width();
    // its own window over other apps (no compositor): SURFACE_STRONG over BG flattened into one
    // opaque strip, a divider on top, translucent outlined pill buttons
    p.cv.rect(Rect::new(0, 0, w, DOCK_H), SURFACE_STRONG.over(BG));
    p.divider(0, 0, w);
    for (i, (r, b)) in button_rects(w).iter().enumerate() {
        let (_, label, icon) = BUTTONS[i];
        let pill = Rect::new(r.x + SP_3, 7, r.w - 2 * SP_3, DOCK_H - 14);
        p.outline_box(pill, pill.h / 2, pressed == Some(*b), false, false);
        let tw = p.fonts.measure(label, Face::Regular, SIZE_SECONDARY).ceil() as i32;
        let group = ICON + SP_2 + tw;
        let x0 = r.x + (r.w - group) / 2;
        let colour = if *b == DockButton::CloseApp {
            DANGER
        } else {
            TEXT
        };
        p.cv.icon(icon, x0, (DOCK_H - ICON) / 2, ICON, colour);
        p.text(
            label,
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT),
            Rect::new(x0 + ICON + SP_2, 0, tw + 4, DOCK_H),
            Align::Left,
        );
    }
}

/// `(app id, wm_class)` of every app in apps.json that names a window class.
pub fn load_wm_classes(root: &Root) -> Vec<(String, String)> {
    let Some(v) = std::fs::read_to_string(root.p("/etc/walpad/apps.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return Vec::new();
    };
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e["id"].as_str()?.to_string();
                    let class = e["wm_class"].as_str().filter(|c| !c.is_empty())?;
                    Some((id, class.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The apps.json id for a window's WM_CLASS (instance, class), compared case-insensitively.
pub fn app_for_class(apps: &[(String, String)], wm_class: &[String]) -> Option<String> {
    apps.iter()
        .find(|(_, c)| wm_class.iter().any(|w| w.eq_ignore_ascii_case(c)))
        .map(|(id, _)| id.clone())
}

/// Should the dock (its handle) be up? `active` is `_NET_ACTIVE_WINDOW` (0 = none). One of the
/// dock's own windows being active (a tap on it) changes nothing.
pub fn wants_visible(active: u32, shell: u32, dock: &[u32], currently: bool) -> bool {
    if dock.contains(&active) {
        currently
    } else {
        active != 0 && active != shell
    }
}

/// What a tap does. Home is handled in X by the dock itself, so it yields no command.
pub fn command_for(
    b: DockButton,
    apps: &[(String, String)],
    active_class: &[String],
) -> Option<Cmd> {
    match b {
        DockButton::Home => None,
        DockButton::Back => Some(Cmd::Back),
        DockButton::Keyboard => Some(Cmd::KeyboardToggle),
        DockButton::CloseApp => app_for_class(apps, active_class).map(Cmd::CloseApp),
    }
}

/// Expanded / collapsed state of the strip with its auto-collapse timer (milliseconds on any
/// monotonic clock).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub expanded: bool,
    collapse_at: Option<u64>,
}

impl Fold {
    /// A tap on the handle: expand (with the timer) or collapse.
    pub fn toggle(&mut self, now: u64) {
        if self.expanded {
            self.collapse();
        } else {
            self.expanded = true;
            self.collapse_at = Some(now + AUTO_COLLAPSE_MS);
        }
    }
    /// A touch on the expanded strip restarts the timer.
    pub fn touched(&mut self, now: u64) {
        if self.expanded {
            self.collapse_at = Some(now + AUTO_COLLAPSE_MS);
        }
    }
    pub fn collapse(&mut self) {
        self.expanded = false;
        self.collapse_at = None;
    }
    /// True when the timer just folded the strip.
    pub fn tick(&mut self, now: u64) -> bool {
        if self.collapse_at.is_some_and(|t| now >= t) {
            self.collapse();
            return true;
        }
        false
    }
    pub fn deadline(&self) -> Option<u64> {
        self.collapse_at
    }
}

struct Atoms {
    active: u32,
    window_type: u32,
    type_dock: u32,
    strut_partial: u32,
    strut: u32,
    net_name: u32,
    utf8: u32,
}

/// What is pressed (highlighted until the release).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    Handle,
    Button(DockButton),
}

pub struct Dock {
    xc: XConn,
    /// the strip (override-redirect: openbox moved a managed strip to the top of the screen)
    win: XWindow,
    /// the arrow handle (override-redirect, SHAPE-d with rounded top corners when available)
    handle: XWindow,
    /// 1x1 managed window that only carries the strut while the strip is expanded (openbox only
    /// honours struts of windows it manages)
    strut_win: XWindow,
    shell: XWindow,
    gc: u32,
    handle_gc: u32,
    depth: u8,
    atoms: Atoms,
    apps: Vec<(String, String)>,
    root_h: i32,
    area: (i32, i32),
    /// Height of the system keyboard / mouse pad below the dock (0 while hidden).
    kbd_h: i32,
    /// the handle is up (a foreign app is active)
    mapped: bool,
    fold: Fold,
    active: u32,
    pressed: Option<Pressed>,
    canvas: Canvas,
    handle_canvas: Canvas,
    epoch: Instant,
}

/// WM_HINTS flags=InputHint, input=false: never take the keyboard focus from the app in front.
const NO_INPUT: [u32; 9] = [1, 0, 0, 0, 0, 0, 0, 0, 0];

impl Dock {
    pub fn open(
        socket: &Path,
        shell: XWindow,
        apps: Vec<(String, String)>,
        shell_size: (i32, i32),
    ) -> Result<Dock, XError> {
        let xc = walpad_x::connect(socket)?;
        let conn = &xc.conn;
        let intern = |n: &str| -> Result<u32, XError> {
            Ok(conn.intern_atom(false, n.as_bytes())?.reply()?.atom)
        };
        let atoms = Atoms {
            active: intern("_NET_ACTIVE_WINDOW")?,
            window_type: intern("_NET_WM_WINDOW_TYPE")?,
            type_dock: intern("_NET_WM_WINDOW_TYPE_DOCK")?,
            strut_partial: intern("_NET_WM_STRUT_PARTIAL")?,
            strut: intern("_NET_WM_STRUT")?,
            net_name: intern("_NET_WM_NAME")?,
            utf8: intern("UTF8_STRING")?,
        };
        let setup = conn.setup();
        let depth = setup.roots.first().map(|s| s.root_depth).unwrap_or(24);
        let root_h = conn.get_geometry(xc.root)?.reply()?.height as i32;
        let area = visible_area(shell_size.0, shell_size.1);
        let events = EventMask::BUTTON_PRESS
            | EventMask::BUTTON_RELEASE
            | EventMask::EXPOSURE
            | EventMask::STRUCTURE_NOTIFY;
        let make = |r: Rect, instance: &str, class: &str| -> Result<(XWindow, u32), XError> {
            let win = conn.generate_id()?;
            let aux = CreateWindowAux::new()
                .event_mask(events)
                .background_pixel(0x001A_1D21)
                .override_redirect(1);
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
                &aux,
            )?;
            let gc = conn.generate_id()?;
            conn.create_gc(gc, win, &CreateGCAux::new())?;
            let wm_class = format!("{instance}\0{class}\0");
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                wm_class.as_bytes(),
            )?;
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                instance.as_bytes(),
            )?;
            conn.change_property8(
                PropMode::REPLACE,
                win,
                atoms.net_name,
                atoms.utf8,
                instance.as_bytes(),
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
        let r = dock_rect(area, 0);
        let (win, gc) = make(r, WM_CLASS_INSTANCE, WM_CLASS_CLASS)?;
        let (handle, handle_gc) = make(handle_rect(area, 0, false), HANDLE_CLASS, HANDLE_CLASS)?;
        shape_handle(&xc, handle);
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
            b"walpad-dock-strut",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            strut_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"walpad-dock-strut\0walpad-dock-strut\0",
        )?;
        let mut d = Dock {
            xc,
            win,
            handle,
            strut_win,
            shell,
            gc,
            handle_gc,
            depth,
            atoms,
            apps,
            root_h,
            area,
            kbd_h: 0,
            mapped: false,
            fold: Fold::default(),
            active: 0,
            pressed: None,
            canvas: Canvas::new(r.w as u32, DOCK_H as u32, BG).ok_or("dock canvas")?,
            handle_canvas: Canvas::new(HANDLE_W as u32, HANDLE_H as u32, BG)
                .ok_or("dock canvas")?,
            epoch: Instant::now(),
        };
        d.write_strut()?;
        let root = d.xc.root;
        d.xc.conn.change_window_attributes(
            root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        d.active = d.read_active();
        d.xc.conn.flush()?;
        Ok(d)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    /// The strip window.
    pub fn window(&self) -> XWindow {
        self.win
    }
    /// The handle window.
    pub fn handle_window(&self) -> XWindow {
        self.handle
    }
    /// The handle is up (a foreign app is in front).
    pub fn visible(&self) -> bool {
        self.mapped
    }
    /// The strip is expanded.
    pub fn expanded(&self) -> bool {
        self.mapped && self.fold.expanded
    }

    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Time until the strip folds back by itself (None while collapsed).
    pub fn next_deadline(&self) -> Option<Duration> {
        self.fold
            .deadline()
            .map(|t| Duration::from_millis(t.saturating_sub(self.now_ms())))
    }

    fn write_strut(&self) -> Result<(), XError> {
        let s = strut_partial(self.area, self.kbd_h, self.root_h);
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

    fn read_active(&self) -> u32 {
        self.xc
            .conn
            .get_property(
                false,
                self.xc.root,
                self.atoms.active,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0)
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

    /// The shell window changed size (rotation): move and resize the strip, the handle and the
    /// strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        let r = dock_rect(area, self.kbd_h);
        self.canvas = Canvas::new(r.w as u32, DOCK_H as u32, BG).ok_or("dock canvas")?;
        self.place()
    }

    /// The system keyboard / mouse pad appeared (its height) or went away (0): stack handle and
    /// strip directly above it and extend the strut over both.
    pub fn set_keyboard_height(&mut self, kbd_h: i32) -> Result<(), XError> {
        if kbd_h == self.kbd_h {
            return Ok(());
        }
        self.kbd_h = kbd_h;
        self.place()
    }

    fn configure(&self, w: XWindow, r: Rect, raise: bool) -> Result<(), XError> {
        let mut aux = ConfigureWindowAux::new()
            .x(r.x)
            .y(r.y)
            .width(r.w as u32)
            .height(r.h as u32);
        if raise {
            aux = aux.stack_mode(StackMode::ABOVE);
        }
        self.xc.conn.configure_window(w, &aux)?;
        Ok(())
    }

    /// Put both windows where the current state wants them (and the strut).
    fn place(&mut self) -> Result<(), XError> {
        let expanded = self.fold.expanded;
        self.configure(self.win, dock_rect(self.area, self.kbd_h), false)?;
        self.configure(
            self.handle,
            handle_rect(self.area, self.kbd_h, expanded),
            false,
        )?;
        self.write_strut()?;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show or hide everything (the handle follows the active window; the strip starts folded).
    fn set_mapped(&mut self, on: bool) -> Result<(), XError> {
        if on == self.mapped {
            if on {
                // the window manager restacks the apps it manages; keep the unmanaged windows on
                // top
                self.raise()?;
                self.xc.conn.flush()?;
            }
            return Ok(());
        }
        self.mapped = on;
        self.pressed = None;
        if on {
            self.fold.collapse();
            self.xc.conn.map_window(self.handle)?;
            // openbox places a newly mapped window itself: ask for our place again once mapped,
            // and put it on top (a remapped window keeps its old place in the stack)
            self.configure(self.handle, handle_rect(self.area, self.kbd_h, false), true)?;
        } else {
            self.fold.collapse();
            self.xc.conn.unmap_window(self.handle)?;
            self.xc.conn.unmap_window(self.win)?;
            self.xc.conn.unmap_window(self.strut_win)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        let above = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        if self.fold.expanded {
            self.xc.conn.configure_window(self.win, &above)?;
        }
        self.xc.conn.configure_window(self.handle, &above)?;
        Ok(())
    }

    /// Expand or fold the strip (the handle moves with it, the strut comes and goes with it).
    fn apply_fold(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let expanded = self.fold.expanded;
        if expanded {
            self.write_strut()?;
            self.xc.conn.map_window(self.strut_win)?;
            self.xc.conn.map_window(self.win)?;
            self.configure(self.win, dock_rect(self.area, self.kbd_h), true)?;
        } else {
            self.xc.conn.unmap_window(self.win)?;
            self.xc.conn.unmap_window(self.strut_win)?;
        }
        self.configure(
            self.handle,
            handle_rect(self.area, self.kbd_h, expanded),
            true,
        )?;
        if expanded {
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
        }
        self.redraw_handle(fonts)?;
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

    /// Raise and focus the shell: the standard `_NET_ACTIVE_WINDOW` request to the window manager,
    /// plus a plain restack (works without one).
    fn raise_shell(&self) -> Result<(), XError> {
        let c = &self.xc.conn;
        let ev = ClientMessageEvent::new(32, self.shell, self.atoms.active, [2, 0, 0, 0, 0]);
        c.send_event(
            false,
            self.xc.root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        c.configure_window(
            self.shell,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        c.flush()?;
        Ok(())
    }

    /// Handle everything the server has sent (non-blocking) and the auto-collapse timer; returns
    /// the commands for the core.
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<Cmd>, XError> {
        let mut cmds = Vec::new();
        let (w, _) = self.area;
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
                    self.pressed = Some(Pressed::Handle);
                    self.redraw_handle(fonts)?;
                }
                Event::ButtonRelease(e) if e.event == self.handle && e.detail == 1 => {
                    let inside = Rect::new(0, 0, HANDLE_W, HANDLE_H)
                        .contains(e.event_x as i32, e.event_y as i32);
                    let down = self.pressed.take();
                    if inside && down == Some(Pressed::Handle) {
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

    /// For tests and diagnostics: is the strip currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.xc
            .conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
}

/// Round the handle's top corners with the SHAPE extension (a plain rectangle without it).
fn shape_handle(xc: &XConn, win: XWindow) {
    use x11rb::connection::RequestConnection as _;
    use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
    use x11rb::protocol::xproto::{ClipOrdering, Rectangle};
    let conn = &xc.conn;
    if !matches!(
        conn.extension_information(x11rb::protocol::shape::X11_EXTENSION_NAME),
        Ok(Some(_))
    ) {
        return;
    }
    let rects: Vec<Rectangle> = handle_shape()
        .iter()
        .map(|r| Rectangle {
            x: r.x as i16,
            y: r.y as i16,
            width: r.w as u16,
            height: r.h as u16,
        })
        .collect();
    let _ = conn.shape_rectangles(
        SO::SET,
        SK::BOUNDING,
        ClipOrdering::UNSORTED,
        win,
        0,
        0,
        &rects,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_sits_at_the_bottom_of_the_visible_area_in_both_orientations() {
        assert_eq!(dock_rect((800, 1280), 0), Rect::new(0, 1224, 800, 56));
        assert_eq!(dock_rect((1280, 800), 0), Rect::new(0, 744, 1280, 56));
        // directly above the system keyboard while it is up
        assert_eq!(dock_rect((800, 1280), 384), Rect::new(0, 840, 800, 56));
        assert_eq!(dock_rect((1280, 800), 240), Rect::new(0, 504, 1280, 56));
    }

    #[test]
    fn area_comes_from_the_aspect_so_the_strut_shrink_does_not_move_the_dock() {
        assert_eq!(visible_area(800, 1280), (800, 1280));
        assert_eq!(
            visible_area(800, 1224),
            (800, 1280),
            "shell shrunk by the strut"
        );
        assert_eq!(visible_area(1280, 800), (1280, 800));
        assert_eq!(visible_area(1280, 744), (1280, 800));
    }

    #[test]
    fn strut_counts_from_the_screen_edge() {
        // Xvfb is 1280x1280: portrait leaves 0 below the panel, landscape 480
        let p = strut_partial((800, 1280), 0, 1280);
        assert_eq!((p[3], p[10], p[11]), (56, 0, 799));
        let l = strut_partial((1280, 800), 0, 1280);
        assert_eq!((l[3], l[10], l[11]), (536, 0, 1279));
        // combined with the keyboard: dock + keyboard (+ 480 below the panel in landscape)
        assert_eq!(strut_partial((800, 1280), 384, 1280)[3], 440);
        assert_eq!(strut_partial((1280, 800), 240, 1280)[3], 776);
        assert!(p[..3].iter().chain(&p[4..10]).all(|v| *v == 0));
    }

    #[test]
    fn four_buttons_tile_the_strip_with_big_targets() {
        for w in [800, 1280] {
            let rs = button_rects(w);
            assert_eq!(
                rs.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
                [
                    DockButton::Back,
                    DockButton::Home,
                    DockButton::Keyboard,
                    DockButton::CloseApp
                ]
            );
            assert_eq!(rs[0].0.x, 0);
            assert_eq!(rs[3].0.right(), w, "no gap at the right edge");
            for pair in rs.windows(2) {
                assert_eq!(pair[0].0.right(), pair[1].0.x);
            }
            assert!(rs.iter().all(|(r, _)| r.h >= TOUCH_MIN && r.w >= TOUCH_MIN));
        }
        assert_eq!(button_at(800, 10, 10), Some(DockButton::Back));
        assert_eq!(button_at(800, 300, 30), Some(DockButton::Home));
        assert_eq!(button_at(800, 500, 30), Some(DockButton::Keyboard));
        assert_eq!(button_at(800, 790, 30), Some(DockButton::CloseApp));
        assert_eq!(button_at(800, 400, 56), None, "below the strip");
    }

    #[test]
    fn visibility_follows_the_active_window() {
        let (shell, dock) = (10, [20, 21]);
        assert!(!wants_visible(0, shell, &dock, false), "nothing active");
        assert!(
            !wants_visible(shell, shell, &dock, true),
            "the shell is in front"
        );
        assert!(wants_visible(77, shell, &dock, false), "a foreign window");
        assert!(
            wants_visible(20, shell, &dock, true),
            "a tap on the dock keeps it"
        );
        assert!(
            wants_visible(21, shell, &dock, true),
            "... or on its handle"
        );
        assert!(!wants_visible(20, shell, &dock, false));
    }

    #[test]
    fn handle_sits_at_the_right_end_of_the_bottom_edge_or_on_the_strip() {
        // collapsed: on the bottom edge, nothing reserved
        assert_eq!(
            handle_rect((800, 1280), 0, false),
            Rect::new(800 - 56 - 8, 1252, 56, 28)
        );
        assert_eq!(
            handle_rect((1280, 800), 0, false),
            Rect::new(1280 - 56 - 8, 772, 56, 28)
        );
        // expanded: directly on top of the strip
        assert_eq!(handle_rect((800, 1280), 0, true).bottom(), 1224);
        assert_eq!(handle_rect((1280, 800), 0, true).bottom(), 744);
        // with the keyboard (or the mouse pad) up: above it, folded or not
        assert_eq!(handle_rect((800, 1280), 384, false).bottom(), 896);
        assert_eq!(handle_rect((800, 1280), 384, true).bottom(), 840);
        assert_eq!(handle_rect((1280, 800), 240, false).bottom(), 560);
        assert_eq!(handle_rect((1280, 800), 240, true).bottom(), 504);
        for area in [(800, 1280), (1280, 800)] {
            let h = handle_rect(area, 0, false);
            assert!(h.x >= 0 && h.right() <= area.0);
            assert!(h.w >= TOUCH_MIN, "touch-wide");
        }
    }

    #[test]
    fn handle_shape_rounds_only_the_top_corners() {
        let s = handle_shape();
        let inside = |x, y| s.iter().any(|r| r.contains(x, y));
        assert!(!inside(0, 0) && !inside(HANDLE_W - 1, 0), "top corners cut");
        assert!(inside(HANDLE_W / 2, 0), "top edge");
        assert!(inside(0, HANDLE_H - 1) && inside(HANDLE_W - 1, HANDLE_H - 1));
        assert!(s
            .iter()
            .all(|r| r.x >= 0 && r.right() <= HANDLE_W && r.bottom() <= HANDLE_H));
        // rows widen monotonically down to the full width
        let widths: Vec<i32> = s.iter().map(|r| r.w).collect();
        assert!(widths.windows(2).all(|p| p[0] <= p[1]));
        assert_eq!(*widths.last().unwrap(), HANDLE_W);
    }

    #[test]
    fn fold_expands_on_a_tap_and_collapses_by_itself() {
        let mut f = Fold::default();
        assert!(!f.expanded, "collapsed by default");
        assert_eq!(f.deadline(), None);
        f.toggle(1000);
        assert!(f.expanded);
        assert_eq!(f.deadline(), Some(1000 + AUTO_COLLAPSE_MS));
        // a touch on the strip restarts the timer
        f.touched(3000);
        assert!(!f.tick(1000 + AUTO_COLLAPSE_MS));
        assert!(f.expanded);
        assert!(f.tick(3000 + AUTO_COLLAPSE_MS), "folds after 5 s idle");
        assert!(!f.expanded);
        assert!(!f.tick(99_999), "only once");
        // a second tap on the handle folds it at once
        f.toggle(0);
        f.toggle(10);
        assert!(!f.expanded);
        assert_eq!(f.deadline(), None);
        // touches while folded do nothing
        f.touched(20);
        assert_eq!(f.deadline(), None);
    }

    #[test]
    fn window_classes_map_to_app_ids() {
        let apps = vec![
            ("browser".to_string(), "firefox-esr".to_string()),
            ("terminal".to_string(), "st".to_string()),
        ];
        let c = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
        assert_eq!(
            app_for_class(&apps, &c("Navigator", "firefox-esr")),
            Some("browser".into())
        );
        assert_eq!(
            app_for_class(&apps, &c("st", "St")),
            Some("terminal".into())
        );
        assert_eq!(app_for_class(&apps, &c("xterm", "XTerm")), None);
        assert_eq!(app_for_class(&apps, &[]), None);
    }

    #[test]
    fn buttons_produce_the_right_commands() {
        let apps = vec![("terminal".to_string(), "st".to_string())];
        let st = vec!["st".to_string(), "St".to_string()];
        let other = vec!["xterm".to_string()];
        assert_eq!(command_for(DockButton::Home, &apps, &st), None);
        assert_eq!(
            command_for(DockButton::Back, &apps, &other),
            Some(Cmd::Back)
        );
        assert_eq!(
            command_for(DockButton::Keyboard, &apps, &other),
            Some(Cmd::KeyboardToggle)
        );
        assert_eq!(
            command_for(DockButton::CloseApp, &apps, &st),
            Some(Cmd::CloseApp("terminal".into()))
        );
        assert_eq!(
            command_for(DockButton::CloseApp, &apps, &other),
            None,
            "unknown window class: nothing happens"
        );
    }

    #[test]
    fn wm_classes_are_read_from_apps_json() {
        let d = std::env::temp_dir().join(format!("walpad-dock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
        std::fs::write(
            d.join("etc/walpad/apps.json"),
            r#"[{"id":"browser","wm_class":"firefox-esr"},{"id":"x"},{"id":"t","wm_class":""}]"#,
        )
        .unwrap();
        assert_eq!(
            load_wm_classes(&Root::new(&d)),
            vec![("browser".to_string(), "firefox-esr".to_string())]
        );
        assert!(load_wm_classes(&Root::new(d.join("missing"))).is_empty());
    }
}
