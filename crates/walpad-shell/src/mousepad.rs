//! Mouse pad: a laptop-style touchpad docked at the bottom of the screen that drives the X pointer
//! over any app (browser, terminal). Opened from the Apps panel's TOOLS.
//!
//! Windows: exactly like the system keyboard (keyboard.rs), an override-redirect window that never
//! takes the focus (WM_HINTS input=false) plus a managed 1x1 dock-type window with the bottom
//! strut; same size (30 % of the visible height, full width). The keyboard and the pad replace each
//! other (the shell loop closes one when the other opens).
//!
//! Input: the core's touch bridge moves the real X pointer to the finger and holds button 1 (XTEST)
//! while a finger is down, so a touch on the pad arrives here as ButtonPress / MotionNotify /
//! ButtonRelease (the pad keeps the implicit grab until the finger lifts, wherever the pointer is).
//! The pad keeps its own virtual cursor: finger motion moves it relatively (x1.5, clamped to the
//! visible area above the pad) and after every pad event the real pointer is put back on it with
//! an XTEST motion, so the app under it sees the cursor there, not under the finger. Those warps
//! come back as MotionNotify events of their own; `PadState` remembers the positions it warped to
//! and ignores their echoes. Clicks are XTEST button presses at the virtual cursor, sent after the
//! finger lifted (the grab is over, so they reach the app under the cursor).
//!
//! Every wait on the X connection has a deadline (walpad-x); no request here waits for a reply
//! except the setup in `open`.
use crate::dock::visible_area;
use crate::keyboard::{create_bottom_windows, keyboard_rect, BottomWindows};
use std::collections::VecDeque;
use std::os::unix::net::UnixStream;
use std::path::Path;
use walpad_ui::components::Painter;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Color, Face, Fonts, Icon, Rect, Style};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, ConnectionExt as _, CreateGCAux, EventMask, Window as XWindow,
};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;

pub const WM_CLASS_INSTANCE: &str = "walpad-mousepad";
pub const WM_CLASS_CLASS: &str = "WalPad-MousePad";
pub const STRUT_CLASS: &str = "walpad-mousepad-strut";
/// WM_CLASS of the cursor marker window.
pub const MARKER_CLASS: &str = "walpad-cursor";
/// Pointer gain: 10 px of finger travel move the cursor 15 px.
pub const ACCEL: f32 = 1.5;
/// Finger travel (px, either axis) before the cursor starts to move.
pub const TAP_SLOP_PX: i32 = SLOP_PX;
/// A touch that stayed within this travel (px, either axis) can still be a tap or a long press
/// (the panel's touch is noisy: the finger "moves" a few px on its own).
pub const TAP_TRAVEL_PX: i32 = 20;
/// A tap: released within this time (after the bridge's lift delay).
pub const TAP_MAX_MS: u32 = 300;
/// A touch held this long within the tap travel is a right click.
pub const LONG_PRESS_MS: u32 = 500;
/// One finger sample that jumps further than this (px, either axis) is noise.
pub const JUMP_PX: i32 = 80;
/// Low-pass: the share of the pending travel applied per sample.
pub const SMOOTH: f32 = 0.6;
/// The core's touch bridge releases the button 150 ms after the last touch report: subtracted
/// from the press duration.
pub const BRIDGE_LIFT_MS: u32 = 150;
/// Warp positions remembered for echo suppression.
const PENDING_MAX: usize = 32;

const PAD: i32 = SP_2;
/// Height cap of the mini-map (px).
const MAP_MAX_H: i32 = 200;

/// The pad window over other apps (no compositor, nothing to blend with): the strip is
/// `SURFACE_STRONG` over BG flattened into one opaque colour; the touch surface adds `SURFACE`.
pub fn strip_color() -> Color {
    surface_strong().over(bg())
}
/// The touch surface's (opaque) colour: what the mini-map is restored to.
pub fn surface_bg() -> Color {
    surface().over(strip_color())
}

/// X pointer buttons.
pub const BUTTON_LEFT: u8 = 1;
pub const BUTTON_RIGHT: u8 = 3;
pub const BUTTON_WHEEL_UP: u8 = 4;
pub const BUTTON_WHEEL_DOWN: u8 = 5;

/// The pad window: same place and size as the system keyboard.
pub fn pad_rect(area: (i32, i32)) -> Rect {
    keyboard_rect(area)
}

/// Where the virtual cursor may go: the visible area above the pad (a click below would land on
/// the pad itself).
pub fn cursor_bounds(area: (i32, i32)) -> Rect {
    Rect::new(0, 0, area.0, pad_rect(area).y)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadButton {
    Left,
    Right,
    ScrollUp,
    ScrollDown,
    /// Minimise (hide) the pad.
    Close,
}

pub const PAD_BUTTONS: [PadButton; 5] = [
    PadButton::Left,
    PadButton::Right,
    PadButton::ScrollUp,
    PadButton::ScrollDown,
    PadButton::Close,
];

/// Pad-local geometry: the touch surface, the mini-map in its top-right corner and the button row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PadLayout {
    pub surface: Rect,
    pub map: Rect,
    pub buttons: Vec<(Rect, PadButton)>,
}

pub fn pad_layout(area: (i32, i32)) -> PadLayout {
    let r = pad_rect(area);
    let by = r.h - PAD - BUTTON_H;
    let n = PAD_BUTTONS.len() as i32;
    let bw = (r.w - 2 * PAD - (n - 1) * PAD) / n;
    let buttons = PAD_BUTTONS
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let x = PAD + i as i32 * (bw + PAD);
            // the last button takes the rounding leftover
            let w = if i as i32 == n - 1 { r.w - PAD - x } else { bw };
            (Rect::new(x, by, w, BUTTON_H), *b)
        })
        .collect();
    let surface = Rect::new(PAD, PAD, r.w - 2 * PAD, by - 2 * PAD);
    // the mini-map is the visible cursor of last resort (the X cursor may not show on the panel)
    let mh = (surface.h - 2 * SP_3).min(MAP_MAX_H);
    let mw = mh * area.0 / area.1.max(1);
    let map = Rect::new(surface.right() - SP_3 - mw, surface.y + SP_3, mw, mh);
    PadLayout {
        surface,
        map,
        buttons,
    }
}

/// What a finished touch asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadOut {
    /// Press and release this X button at the virtual cursor.
    Click(u8),
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Touch {
    start: (i32, i32),
    /// Finger position the relative motion is measured from (root coordinates).
    last: (i32, i32),
    /// Last accepted finger sample (jump filter reference).
    prev: (i32, i32),
    /// A sample that jumped more than `JUMP_PX`: dropped unless the next one confirms it.
    spike: Option<(i32, i32)>,
    t0: u32,
    /// Largest distance (either axis) of an accepted sample from `start`.
    travel: i32,
    /// Travel exceeded the slop: this touch moves the cursor.
    moving: bool,
    /// The virtual cursor when the finger went down (a tap clicks there).
    cursor0: (i32, i32),
    /// The touch began on a button of the row.
    button: Option<PadButton>,
}

fn cheb(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs().max((a.1 - b.1).abs())
}

/// The pad's gesture state (pure: fed root coordinates and X timestamps).
///
/// Robust against the panel's noisy touch: a single sample that jumps more than `JUMP_PX` from the
/// previous one is dropped (two in a row that agree re-anchor without moving the cursor); deltas
/// go through a low-pass that keeps the total travel (`acc` is spent `SMOOTH` per sample and the
/// rest on release); nothing moves inside the 10 px slop, then the slop travel is included; a tap
/// tolerates 20 px of jitter and clicks where the cursor was when the finger went down.
#[derive(Debug, Clone, PartialEq)]
pub struct PadState {
    area: (i32, i32),
    pub cursor: (i32, i32),
    rem: (f32, f32),
    /// Accelerated travel not yet applied (the low-pass backlog).
    acc: (f32, f32),
    touch: Option<Touch>,
    pending: VecDeque<(i32, i32)>,
}

impl PadState {
    pub fn new(area: (i32, i32)) -> PadState {
        let mut s = PadState {
            area,
            cursor: (0, 0),
            rem: (0.0, 0.0),
            acc: (0.0, 0.0),
            touch: None,
            pending: VecDeque::new(),
        };
        s.recenter();
        s
    }

    pub fn area(&self) -> (i32, i32) {
        self.area
    }

    /// Cursor back to the middle of the area above the pad.
    pub fn recenter(&mut self) {
        let b = cursor_bounds(self.area);
        self.cursor = b.center();
        self.rem = (0.0, 0.0);
        self.acc = (0.0, 0.0);
    }

    /// Rotation: new area, the cursor recentred, any touch forgotten.
    pub fn set_area(&mut self, area: (i32, i32)) {
        self.area = area;
        self.touch = None;
        self.pending.clear();
        self.recenter();
    }

    /// Forget a touch in progress (the pad was hidden).
    pub fn reset_touch(&mut self) {
        self.touch = None;
        self.pending.clear();
        self.acc = (0.0, 0.0);
    }

    pub fn pressed_button(&self) -> Option<PadButton> {
        self.touch.and_then(|t| t.button)
    }

    fn button_at(&self, root: (i32, i32)) -> Option<PadButton> {
        let r = pad_rect(self.area);
        let (lx, ly) = (root.0 - r.x, root.1 - r.y);
        pad_layout(self.area)
            .buttons
            .iter()
            .find(|(b, _)| b.contains(lx, ly))
            .map(|(_, b)| *b)
    }

    /// The real pointer was put on the cursor: its MotionNotify is ours, not the finger's.
    pub fn note_warp(&mut self) {
        if self.pending.back() != Some(&self.cursor) {
            self.pending.push_back(self.cursor);
            if self.pending.len() > PENDING_MAX {
                self.pending.pop_front();
            }
        }
    }

    /// Finger down at a root position, X time `t` (ms). Every press re-anchors: the motion is
    /// measured from here, never from where the pointer was warped to after the last batch.
    pub fn press(&mut self, x: i32, y: i32, t: u32) {
        self.touch = Some(Touch {
            start: (x, y),
            last: (x, y),
            prev: (x, y),
            spike: None,
            t0: t,
            travel: 0,
            moving: false,
            cursor0: self.cursor,
            button: self.button_at((x, y)),
        });
        self.rem = (0.0, 0.0);
        self.acc = (0.0, 0.0);
    }

    /// Move the cursor by whole pixels of `d` (+ the carried fraction), clamped to the bounds.
    fn apply(&mut self, d: (f32, f32)) -> bool {
        let dx = d.0 + self.rem.0;
        let dy = d.1 + self.rem.1;
        let (ix, iy) = (dx.trunc(), dy.trunc());
        self.rem = (dx - ix, dy - iy);
        let b = cursor_bounds(self.area);
        let nx = (self.cursor.0 + ix as i32).clamp(b.x, b.right() - 1);
        let ny = (self.cursor.1 + iy as i32).clamp(b.y, b.bottom() - 1);
        if nx != self.cursor.0 + ix as i32 {
            self.rem.0 = 0.0;
            self.acc.0 = 0.0;
        }
        if ny != self.cursor.1 + iy as i32 {
            self.rem.1 = 0.0;
            self.acc.1 = 0.0;
        }
        let moved = (nx, ny) != self.cursor;
        self.cursor = (nx, ny);
        moved
    }

    /// Pointer motion at a root position. Returns true when the virtual cursor moved.
    pub fn motion(&mut self, x: i32, y: i32) -> bool {
        // the echo of one of our warps: drop it and every older one
        if let Some(i) = self.pending.iter().position(|p| *p == (x, y)) {
            self.pending.drain(..=i);
            return false;
        }
        let Some(t) = self.touch.as_mut() else {
            return false;
        };
        let p = (x, y);
        if t.button.is_some() {
            t.last = p;
            return false;
        }
        // jump filter: one wild sample is noise; two that agree mean the finger is really there
        if cheb(p, t.prev) > JUMP_PX {
            match t.spike {
                Some(s) if cheb(p, s) <= JUMP_PX => {
                    t.spike = None;
                    t.prev = p;
                    if t.moving {
                        t.last = p;
                    } else {
                        // still a tap candidate: start over from here
                        t.start = p;
                        t.last = p;
                    }
                }
                _ => t.spike = Some(p),
            }
            return false;
        }
        t.spike = None;
        t.prev = p;
        t.travel = t.travel.max(cheb(p, t.start));
        if !t.moving {
            if cheb(p, t.start) <= TAP_SLOP_PX {
                return false;
            }
            // past the slop: the whole travel from the start counts (t.last is still the start)
            t.moving = true;
        }
        let d = ((x - t.last.0) as f32 * ACCEL, (y - t.last.1) as f32 * ACCEL);
        t.last = p;
        self.acc.0 += d.0;
        self.acc.1 += d.1;
        let out = (self.acc.0 * SMOOTH, self.acc.1 * SMOOTH);
        self.acc.0 -= out.0;
        self.acc.1 -= out.1;
        self.apply(out)
    }

    /// Finger up at X time `t`. The release position is not used: the pointer sits on the cursor
    /// by then, the finger's last position is what counts. A drag spends its low-pass backlog.
    pub fn release(&mut self, t: u32) -> Option<PadOut> {
        let touch = self.touch.take()?;
        if let Some(b) = touch.button {
            // released off the button it started on: nothing
            if self.button_at(touch.last) != Some(b) {
                return None;
            }
            return Some(match b {
                PadButton::Left => PadOut::Click(BUTTON_LEFT),
                PadButton::Right => PadOut::Click(BUTTON_RIGHT),
                PadButton::ScrollUp => PadOut::Click(BUTTON_WHEEL_UP),
                PadButton::ScrollDown => PadOut::Click(BUTTON_WHEEL_DOWN),
                PadButton::Close => PadOut::Hide,
            });
        }
        let held = t.wrapping_sub(touch.t0).saturating_sub(BRIDGE_LIFT_MS);
        if touch.travel < TAP_TRAVEL_PX {
            let click = if held < TAP_MAX_MS {
                Some(BUTTON_LEFT)
            } else if held >= LONG_PRESS_MS {
                Some(BUTTON_RIGHT)
            } else {
                None
            };
            if let Some(b) = click {
                // jitter may have nudged the cursor: click where it was when the finger went down
                self.cursor = touch.cursor0;
                self.acc = (0.0, 0.0);
                self.rem = (0.0, 0.0);
                return Some(PadOut::Click(b));
            }
        }
        let rest = std::mem::take(&mut self.acc);
        if touch.moving {
            self.apply(rest);
        }
        None
    }
}

// ------------------------------------------------------------------------------------ drawing

/// The mini-map: the visible area in small, the pad's band greyed, a crosshair and a square marker
/// where the cursor is.
pub fn paint_map(p: &mut Painter, map: Rect, area: (i32, i32), cursor: (i32, i32)) {
    p.cv.rect(map.inset(-2), surface_bg());
    p.cv.rect(map, pressed_tint());
    let pad = pad_rect(area);
    let band_y = map.y + pad.y * map.h / area.1.max(1);
    p.cv.rect(
        Rect::new(map.x, band_y, map.w, map.bottom() - band_y),
        divider(),
    );
    let dx = map.x + cursor.0 * map.w / area.0.max(1);
    let dy = map.y + cursor.1 * map.h / area.1.max(1);
    // crosshair through the cursor (inside the map), then the square marker
    p.cv.rect(Rect::new(map.x, dy, map.w, 1), divider());
    p.cv.rect(Rect::new(dx, map.y, 1, map.h), divider());
    p.cv.rect(Rect::new(dx - 6, dy - 6, 13, 13), Color::hex(0x000000));
    p.cv.rect(Rect::new(dx - 4, dy - 4, 9, 9), text());
}

/// Draws the whole pad over the canvas (pad-sized).
pub fn paint_pad(p: &mut Painter, st: &PadState) {
    let area = st.area();
    let bounds = p.cv.bounds();
    let l = pad_layout(area);
    p.cv.rect(bounds, strip_color());
    p.divider(0, 0, bounds.w);
    p.outline_box(l.surface, 0, false, false, false);
    let compact = l.surface.h < 200;
    let hint = Rect::new(
        l.surface.x + SP_5,
        l.surface.y,
        l.map.x - l.surface.x - 2 * SP_5,
        l.surface.h,
    );
    let (title, lines) = (
        "Mouse pad",
        [
            "Drag to move the pointer",
            "Tap to click \u{b7} hold for a right click",
        ],
    );
    let lh = if compact { 20 } else { 24 };
    let block = lh + 6 + 2 * lh;
    let mut y = hint.y + (hint.h - block) / 2;
    p.text(
        title,
        Style::new(Face::Bold, SIZE_BODY, text()),
        Rect::new(hint.x, y, hint.w, lh),
        Align::Left,
    );
    y += lh + 6;
    for s in lines {
        p.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, text_secondary()),
            Rect::new(hint.x, y, hint.w, lh),
            Align::Left,
        );
        y += lh;
    }
    paint_map(p, l.map, area, st.cursor);
    let pressed = st.pressed_button();
    for (r, b) in &l.buttons {
        p.outline_box(*r, 0, pressed == Some(*b), false, false);
        let label = |s: &str, p: &mut Painter| {
            p.text(
                s,
                Style::new(Face::Bold, SIZE_SECONDARY, text()),
                *r,
                Align::Center,
            )
        };
        match b {
            PadButton::Left => label("Left click", p),
            PadButton::Right => label("Right click", p),
            PadButton::ScrollUp => label("\u{2191} Scroll", p),
            PadButton::ScrollDown => label("\u{2193} Scroll", p),
            PadButton::Close => {
                // mouse with a chevron down: "put the pad away" (the keyboard's minimise key)
                let w = ICON + 2 + 16;
                let (x, y) = (r.x + (r.w - w) / 2, r.y + (r.h - ICON) / 2);
                p.cv.icon(Icon::Mouse, x, y, ICON, text());
                p.cv.icon(Icon::ChevronDown, x + ICON + 2, y + 4, 16, text());
            }
        }
    }
}

// ------------------------------------------------------------------------------------ cursor marker

/// Our own pointer arrow (the Xvfb cursor may not reach the panel): `#` black edge, `.` white
/// body, space = not part of the window (SHAPE bounding region). The tip (0, 0) is the hotspot.
const ARROW: [&str; 19] = [
    "#",
    "##",
    "#.#",
    "#..#",
    "#...#",
    "#....#",
    "#.....#",
    "#......#",
    "#.......#",
    "#........#",
    "#.........#",
    "#......#####",
    "#...#..#",
    "#..##..#",
    "#.#  #..#",
    "##   #..#",
    "#     #..#",
    "      #..#",
    "       ##",
];
/// Arrow pixels are drawn 2x2 (the panel is ~190 dpi).
pub const MARKER_SCALE: i32 = 2;

/// Marker window size (px).
pub fn marker_size() -> (i32, i32) {
    let w = ARROW.iter().map(|r| r.len()).max().unwrap_or(1) as i32;
    (w * MARKER_SCALE, ARROW.len() as i32 * MARKER_SCALE)
}

/// The arrow's visible region as rectangles (one per run of drawn cells in a row), scaled.
pub fn marker_shape() -> Vec<Rect> {
    let mut out = Vec::new();
    for (y, row) in ARROW.iter().enumerate() {
        let b = row.as_bytes();
        let mut x = 0;
        while x < b.len() {
            if b[x] == b' ' {
                x += 1;
                continue;
            }
            let x0 = x;
            while x < b.len() && b[x] != b' ' {
                x += 1;
            }
            out.push(Rect::new(
                x0 as i32 * MARKER_SCALE,
                y as i32 * MARKER_SCALE,
                (x - x0) as i32 * MARKER_SCALE,
                MARKER_SCALE,
            ));
        }
    }
    out
}

/// Paint the arrow into a marker-sized canvas.
pub fn paint_marker(cv: &mut Canvas) {
    cv.fill(Color::hex(0x000000));
    for (y, row) in ARROW.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            if c == b'.' {
                cv.rect(
                    Rect::new(
                        x as i32 * MARKER_SCALE,
                        y as i32 * MARKER_SCALE,
                        MARKER_SCALE,
                        MARKER_SCALE,
                    ),
                    text(),
                );
            }
        }
    }
}

/// A small override-redirect window shaped like an arrow that follows the virtual cursor. Its
/// input region is empty, so clicks at the cursor go through it to the app below.
struct Marker {
    win: XWindow,
    gc: xproto::Gcontext,
    depth: u8,
    canvas: Canvas,
}

impl Marker {
    /// None when the server has no SHAPE extension (a rectangular marker would hide the app and
    /// catch the clicks) or a request fails: the pad then works without it.
    fn create(xc: &XConn, depth: u8) -> Option<Marker> {
        use x11rb::connection::RequestConnection as _;
        use x11rb::protocol::shape::{ConnectionExt as _, SK, SO};
        use x11rb::protocol::xproto::{ClipOrdering, CreateWindowAux};
        let conn = &xc.conn;
        conn.extension_information(x11rb::protocol::shape::X11_EXTENSION_NAME)
            .ok()??;
        let (w, h) = marker_size();
        let win = conn.generate_id().ok()?;
        conn.create_window(
            0,
            win,
            xc.root,
            0,
            0,
            w as u16,
            h as u16,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .override_redirect(1)
                .background_pixel(0)
                .event_mask(EventMask::EXPOSURE),
        )
        .ok()?
        .check()
        .ok()?;
        // named, so tools and tests can find it
        conn.change_property8(
            xproto::PropMode::REPLACE,
            win,
            xproto::AtomEnum::WM_CLASS,
            xproto::AtomEnum::STRING,
            format!("{MARKER_CLASS}\0{MARKER_CLASS}\0").as_bytes(),
        )
        .ok()?;
        let rects: Vec<xproto::Rectangle> = marker_shape()
            .iter()
            .map(|r| xproto::Rectangle {
                x: r.x as i16,
                y: r.y as i16,
                width: r.w as u16,
                height: r.h as u16,
            })
            .collect();
        let shaped = conn
            .shape_rectangles(
                SO::SET,
                SK::BOUNDING,
                ClipOrdering::UNSORTED,
                win,
                0,
                0,
                &rects,
            )
            .ok()
            .and_then(|c| c.check().ok())
            .and_then(|_| {
                conn.shape_rectangles(SO::SET, SK::INPUT, ClipOrdering::UNSORTED, win, 0, 0, &[])
                    .ok()
            })
            .and_then(|c| c.check().ok());
        let gc = conn.generate_id().ok();
        let gc_ok = gc.and_then(|g| {
            conn.create_gc(g, win, &CreateGCAux::new())
                .ok()
                .and_then(|c| c.check().ok())
        });
        let canvas = Canvas::new(w as u32, h as u32, Color::hex(0x000000));
        match (shaped, gc, gc_ok, canvas) {
            (Some(()), Some(gc), Some(()), Some(mut canvas)) => {
                paint_marker(&mut canvas);
                Some(Marker {
                    win,
                    gc,
                    depth,
                    canvas,
                })
            }
            _ => {
                let _ = conn.destroy_window(win);
                None
            }
        }
    }

    fn present(&self, xc: &XConn) -> Result<(), XError> {
        crate::present::present(
            &xc.conn,
            self.win,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    fn set_mapped(&self, xc: &XConn, on: bool) -> Result<(), XError> {
        if on {
            xc.conn.map_window(self.win)?;
        } else {
            xc.conn.unmap_window(self.win)?;
        }
        Ok(())
    }

    /// Tip on `cursor`, above every other window.
    fn place(&self, xc: &XConn, cursor: (i32, i32)) -> Result<(), XError> {
        xc.conn.configure_window(
            self.win,
            &xproto::ConfigureWindowAux::new()
                .x(cursor.0)
                .y(cursor.1)
                .stack_mode(xproto::StackMode::ABOVE),
        )?;
        Ok(())
    }
}

// ------------------------------------------------------------------------------------ X window

pub struct MousePad {
    xc: XConn,
    bw: BottomWindows,
    shell: XWindow,
    mapped: bool,
    active: u32,
    st: PadState,
    canvas: Canvas,
    marker: Option<Marker>,
}

impl MousePad {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let area = visible_area(shell_size.0, shell_size.1);
        let r = pad_rect(area);
        let bw = create_bottom_windows(
            &xc,
            WM_CLASS_INSTANCE,
            WM_CLASS_CLASS,
            STRUT_CLASS,
            r,
            EventMask::BUTTON_PRESS
                | EventMask::BUTTON_RELEASE
                | EventMask::POINTER_MOTION
                | EventMask::EXPOSURE,
        )?;
        bw.write_strut(&xc, area)?;
        let active = bw.read_active(&xc);
        let marker = Marker::create(&xc, bw.depth);
        xc.conn.flush()?;
        Ok(MousePad {
            marker,
            xc,
            bw,
            shell,
            mapped: false,
            active,
            st: PadState::new(area),
            canvas: Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("mouse pad canvas")?,
        })
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn window(&self) -> XWindow {
        self.bw.win
    }
    pub fn visible(&self) -> bool {
        self.mapped
    }
    /// Height of the pad for the current orientation (also while hidden).
    pub fn height(&self) -> i32 {
        pad_rect(self.st.area()).h
    }
    /// The virtual cursor (root coordinates).
    pub fn cursor(&self) -> (i32, i32) {
        self.st.cursor
    }

    /// The shell window changed size (rotation): move and resize the pad and its strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.st.area() {
            return Ok(());
        }
        self.st.set_area(area);
        let r = pad_rect(area);
        self.canvas = Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("mouse pad canvas")?;
        self.bw.place(&self.xc, r)?;
        self.bw.write_strut(&self.xc, area)?;
        if self.mapped {
            self.redraw(fonts)?;
            self.warp()?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    /// Show (cursor in the middle of the area above the pad, the pointer put there) or hide.
    pub fn set_visible(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.mapped {
            return Ok(());
        }
        self.mapped = on;
        self.st.reset_touch();
        self.bw.set_mapped(&self.xc, on, pad_rect(self.st.area()))?;
        if let Some(m) = &self.marker {
            m.set_mapped(&self.xc, on)?;
        }
        if on {
            self.st.recenter();
            // paint right away: a remapped window is not always sent an Expose
            self.redraw(fonts)?;
            self.warp()?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        paint_pad(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            &self.st,
        );
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    /// Only the mini-map changed (cursor motion): repaint and send that corner.
    fn redraw_map(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let map = pad_layout(self.st.area()).map;
        paint_map(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            map,
            self.st.area(),
            self.st.cursor,
        );
        crate::present::present(
            &self.xc.conn,
            self.bw.win,
            self.bw.gc,
            self.bw.depth,
            &self.canvas,
            map.inset(-2),
        )
    }

    /// Put the real pointer on the virtual cursor (XTEST absolute motion).
    fn warp(&mut self) -> Result<(), XError> {
        let (x, y) = self.st.cursor;
        self.xc.conn.xtest_fake_input(
            xproto::MOTION_NOTIFY_EVENT,
            0,
            0,
            self.xc.root,
            x as i16,
            y as i16,
            0,
        )?;
        self.st.note_warp();
        if let Some(m) = &self.marker {
            m.place(&self.xc, (x, y))?;
        }
        Ok(())
    }

    /// Press and release `button` at the virtual cursor.
    fn click(&mut self, button: u8) -> Result<(), XError> {
        self.warp()?;
        let (c, root) = (&self.xc.conn, self.xc.root);
        c.xtest_fake_input(xproto::BUTTON_PRESS_EVENT, button, 0, root, 0, 0, 0)?;
        c.xtest_fake_input(xproto::BUTTON_RELEASE_EVENT, button, 0, root, 0, 0, 0)?;
        Ok(())
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    const P: (i32, i32) = (800, 1280);
    const L: (i32, i32) = (1280, 800);

    /// A point on the touch surface (root coordinates).
    fn on_surface(area: (i32, i32)) -> (i32, i32) {
        let r = pad_rect(area);
        let s = pad_layout(area).surface;
        (r.x + s.x + 100, r.y + s.y + 60)
    }

    fn on_button(area: (i32, i32), b: PadButton) -> (i32, i32) {
        let r = pad_rect(area);
        let (c, _) = pad_layout(area)
            .buttons
            .into_iter()
            .find(|(_, x)| *x == b)
            .unwrap();
        (r.x + c.center().0, r.y + c.center().1)
    }

    #[test]
    fn same_place_as_the_keyboard_with_a_row_of_big_buttons() {
        for area in [P, L] {
            let r = pad_rect(area);
            assert_eq!(r, keyboard_rect(area));
            assert_eq!(cursor_bounds(area), Rect::new(0, 0, area.0, r.y));
            let l = pad_layout(area);
            assert_eq!(l.buttons.len(), 5);
            for (b, _) in &l.buttons {
                assert!(b.h >= TOUCH_MIN && b.w >= 100, "{b:?}");
                assert!(b.bottom() <= r.h && b.right() <= r.w);
                assert!(b.y > l.surface.bottom());
            }
            assert_eq!(l.buttons.last().unwrap().0.right(), r.w - PAD);
            assert!(l.surface.h >= 150 && l.surface.w > 700);
            // the mini-map keeps the visible area's aspect and sits inside the surface
            assert!(l.surface.contains(l.map.x, l.map.y) && l.map.right() < l.surface.right());
            assert!(((l.map.w * area.1) - (l.map.h * area.0)).abs() <= area.1);
        }
    }

    #[test]
    fn starts_in_the_middle_of_the_area_above_the_pad() {
        assert_eq!(PadState::new(P).cursor, (400, 448));
        assert_eq!(PadState::new(L).cursor, (640, 280));
    }

    /// Drag the finger from `from` by (dx, dy) in steps of at most 40 px (a real finger's
    /// sample spacing), returning the end point.
    fn drag(s: &mut PadState, from: (i32, i32), dx: i32, dy: i32) -> (i32, i32) {
        let n = (dx.abs().max(dy.abs()) + 39) / 40;
        let mut p = from;
        for i in 1..=n.max(1) {
            p = (from.0 + dx * i / n.max(1), from.1 + dy * i / n.max(1));
            s.motion(p.0, p.1);
        }
        p
    }

    #[test]
    fn slop_then_the_whole_travel_counts_smoothed() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        let c0 = s.cursor;
        s.press(x, y, 1000);
        // within the slop: nothing moves yet
        assert!(!s.motion(x + 5, y + 5));
        assert!(!s.motion(x + 10, y));
        assert_eq!(s.cursor, c0);
        // past it: moves, low-passed (part of the 30 px now) ...
        assert!(s.motion(x + 20, y));
        let mid = s.cursor.0 - c0.0;
        assert!(mid > 0 && mid < 30, "smoothed step {mid}");
        // ... and the rest follows on the next samples / on release: the whole travel, x1.5
        assert!(s.motion(x + 20, y - 40));
        assert_eq!(s.release(1400), None, "a drag clicks nothing");
        assert_eq!(s.cursor, (c0.0 + 30, c0.1 - 60));
        // fractions carry: four 1 px steps move 6 px in total
        let before = s.cursor.0;
        s.press(x, y, 2000);
        drag(&mut s, (x, y), 12, 0);
        for i in 1..=4 {
            s.motion(x + 12 + i, y);
        }
        s.release(2500);
        assert_eq!(s.cursor.0, before + 24);
    }

    #[test]
    fn single_sample_jumps_are_ignored() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        let c0 = s.cursor;
        s.press(x, y, 0);
        s.motion(x + 20, y);
        // one wild sample (noise, or an echo we did not recognise): no movement
        let before = s.cursor;
        assert!(!s.motion(x + 300, y - 200));
        assert_eq!(s.cursor, before);
        // the finger goes on where it was
        s.motion(x + 30, y);
        s.release(100);
        assert_eq!(s.cursor, (c0.0 + 45, c0.1), "only the real 30 px count");
        // noise inside the slop does not start a drag nor spoil the tap
        s.press(x, y, 1000);
        assert!(!s.motion(x - 150, y + 90));
        assert!(!s.motion(x + 3, y + 2));
        assert_eq!(s.release(1000 + 100 + 150), Some(PadOut::Click(1)));
        assert_eq!(s.cursor, (c0.0 + 45, c0.1));
    }

    #[test]
    fn two_agreeing_jumps_re_anchor_without_moving() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        let c0 = s.cursor;
        s.press(x, y, 0);
        s.motion(x + 20, y);
        // the finger really is over there now (lifted and put down between reports): re-anchor
        assert!(!s.motion(x + 220, y));
        assert!(!s.motion(x + 222, y));
        s.motion(x + 232, y);
        s.release(500);
        // 20 px before the jump, 10 after the confirming sample; the jump itself never counts
        assert_eq!(s.cursor, (c0.0 + 45, c0.1));
    }

    #[test]
    fn every_press_re_anchors_on_the_finger() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        s.press(x, y, 0);
        s.note_warp();
        drag(&mut s, (x, y), 100, 0);
        s.release(400);
        s.note_warp(); // pointer back on the cursor after the batch
        let c1 = s.cursor;
        // the next touch lands 300 px away from the last finger sample: no delta from there
        s.press(x + 400, y + 50, 2000);
        assert!(!s.motion(x + 405, y + 50));
        drag(&mut s, (x + 405, y + 50), -25, 0);
        s.release(2300);
        assert_eq!(s.cursor, (c1.0 - 30, c1.1), "only this touch's 20 px x1.5");
        // the late echo of the old warp (at the cursor) is recognised, not a finger sample
        s.press(x, y, 3000);
        assert!(!s.motion(c1.0, c1.1));
    }

    #[test]
    fn cursor_is_clamped_to_the_visible_area_above_the_pad() {
        for area in [P, L] {
            let mut s = PadState::new(area);
            let (x, y) = on_surface(area);
            s.press(x, y, 0);
            let p = drag(&mut s, (x, y), -1600, -1600);
            assert_eq!(s.cursor, (0, 0));
            let p = drag(&mut s, p, 3000, 3000);
            let b = cursor_bounds(area);
            assert_eq!(s.cursor, (b.right() - 1, b.bottom() - 1));
            assert_eq!(s.cursor, (area.0 - 1, pad_rect(area).y - 1));
            // and back out of the corner right away (no backlog or fraction pushing outwards)
            s.motion(p.0 - 2, p.1);
            s.release(10_000);
            assert!(s.cursor.0 < area.0 - 1, "{:?}", s.cursor);
        }
    }

    #[test]
    fn echoes_of_our_warps_are_ignored() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        s.press(x, y, 0);
        s.note_warp();
        let c0 = s.cursor;
        assert!(s.motion(x + 20, y)); // finger
        s.note_warp(); // pointer back on the moved cursor
        let c1 = s.cursor;
        // the first warp's echo arrives late (after the finger sample): no jump
        assert!(!s.motion(c0.0, c0.1));
        // the newest echo too
        assert!(!s.motion(c1.0, c1.1));
        // the finger again: delta from its own last position
        s.motion(x + 30, y);
        s.release(500);
        assert_eq!(s.cursor, (c0.0 + 45, c0.1));
    }

    #[test]
    fn taps_tolerate_jitter_and_click_where_the_touch_began() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        let c0 = s.cursor;
        // the bridge releases 150 ms after the finger lifted
        s.press(x, y, 10_000);
        assert_eq!(s.release(10_000 + 120 + 150), Some(PadOut::Click(1)));
        // 15 px of jitter (past the 10 px slop, the cursor twitched): still a tap, and the click
        // lands where the cursor was when the finger went down
        s.press(x, y, 20_000);
        s.motion(x + 8, y - 6);
        s.motion(x + 15, y - 4);
        assert_ne!(s.cursor, c0);
        assert_eq!(s.release(20_000 + 250 + 150), Some(PadOut::Click(1)));
        assert_eq!(s.cursor, c0);
        // too slow for a tap, too short for a long press: nothing
        s.press(x, y, 30_000);
        assert_eq!(s.release(30_000 + 400 + 150), None);
        // a long press within the tolerance: right click at the start position
        s.press(x, y, 40_000);
        s.motion(x + 12, y + 5);
        assert_eq!(s.release(40_000 + 600 + 150), Some(PadOut::Click(3)));
        assert_eq!(s.cursor, c0);
        // 20 px or more is a drag, however quick
        s.press(x, y, 50_000);
        s.motion(x + 10, y);
        s.motion(x + 22, y);
        assert_eq!(s.release(50_000 + 100), None);
        assert_ne!(s.cursor, c0);
        // X time wraps around
        s.press(x, y, u32::MAX - 50);
        assert_eq!(s.release(100), Some(PadOut::Click(1)));
        // a release without a press does nothing
        assert_eq!(s.release(5), None);
    }

    #[test]
    fn marker_shape_is_the_arrow_with_the_tip_at_the_origin() {
        let shape = marker_shape();
        let (w, h) = marker_size();
        assert_eq!((w, h), (24, 38));
        assert!(shape.iter().any(|r| r.contains(0, 0)), "hotspot is drawn");
        assert!(shape.iter().all(|r| r.right() <= w && r.bottom() <= h));
        // the region is not the whole box (a rectangle would hide what is under it)
        let area: i32 = shape.iter().map(|r| r.w * r.h).sum();
        assert!(area < w * h * 2 / 3, "{area}");
        assert!(
            !shape.iter().any(|r| r.contains(w - 1, 0)),
            "top-right is see-through"
        );
        let mut cv = Canvas::new(w as u32, h as u32, BG).unwrap();
        paint_marker(&mut cv);
        assert_eq!(cv.pixel(0, 0), Some(Color::hex(0)), "black edge at the tip");
        assert_eq!(cv.pixel(3, 6), Some(TEXT), "white body");
    }

    #[test]
    fn button_row() {
        let mut s = PadState::new(L);
        let c0 = s.cursor;
        for (b, out) in [
            (PadButton::Left, PadOut::Click(1)),
            (PadButton::Right, PadOut::Click(3)),
            (PadButton::ScrollUp, PadOut::Click(4)),
            (PadButton::ScrollDown, PadOut::Click(5)),
            (PadButton::Close, PadOut::Hide),
        ] {
            let (x, y) = on_button(L, b);
            s.press(x, y, 0);
            assert_eq!(s.pressed_button(), Some(b));
            // motion on a button never moves the cursor; a long hold is still that button
            assert!(!s.motion(x + 3, y + 2));
            assert_eq!(s.release(2000), Some(out), "{b:?}");
            assert_eq!(s.pressed_button(), None);
        }
        assert_eq!(s.cursor, c0);
        // sliding off the button cancels it
        let (x, y) = on_button(L, PadButton::Left);
        s.press(x, y, 0);
        s.motion(x, y - 100);
        assert_eq!(s.release(100), None);
    }

    #[test]
    fn rotation_recentres_and_forgets_the_touch() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        s.press(x, y, 0);
        s.motion(x + 100, y);
        s.set_area(L);
        assert_eq!(s.cursor, (640, 280));
        assert_eq!(s.release(10), None);
    }
}
