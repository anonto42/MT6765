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
use walpad_ui::{Align, Canvas, Face, Fonts, Icon, Rect, Style};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, EventMask, Window as XWindow};
use x11rb::protocol::xtest::ConnectionExt as _;
use x11rb::protocol::Event;

pub const WM_CLASS_INSTANCE: &str = "walpad-mousepad";
pub const WM_CLASS_CLASS: &str = "WalPad-MousePad";
pub const STRUT_CLASS: &str = "walpad-mousepad-strut";
/// Pointer gain: 10 px of finger travel move the cursor 15 px.
pub const ACCEL: f32 = 1.5;
/// Finger travel (px, either axis) below which a touch is still a tap / long press.
pub const TAP_SLOP_PX: i32 = SLOP_PX;
/// A touch held this long without moving is a right click.
pub const LONG_PRESS_MS: u32 = 500;
/// The core's touch bridge releases the button 150 ms after the last touch report: subtracted
/// from the press duration.
pub const BRIDGE_LIFT_MS: u32 = 150;
/// Warp positions remembered for echo suppression.
const PENDING_MAX: usize = 32;

const PAD: i32 = SP_2;

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
    let mh = (surface.h - 2 * SP_3).min(120);
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
    /// Last finger position used for the relative motion (root coordinates).
    last: (i32, i32),
    t0: u32,
    /// Travel exceeded the slop: this touch moves the cursor and clicks nothing.
    moving: bool,
    /// The touch began on a button of the row.
    button: Option<PadButton>,
}

/// The pad's gesture state (pure: fed root coordinates and X timestamps).
#[derive(Debug, Clone, PartialEq)]
pub struct PadState {
    area: (i32, i32),
    pub cursor: (i32, i32),
    rem: (f32, f32),
    touch: Option<Touch>,
    pending: VecDeque<(i32, i32)>,
}

impl PadState {
    pub fn new(area: (i32, i32)) -> PadState {
        let mut s = PadState {
            area,
            cursor: (0, 0),
            rem: (0.0, 0.0),
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

    /// Finger down at a root position, X time `t` (ms).
    pub fn press(&mut self, x: i32, y: i32, t: u32) {
        self.touch = Some(Touch {
            start: (x, y),
            last: (x, y),
            t0: t,
            moving: false,
            button: self.button_at((x, y)),
        });
        self.rem = (0.0, 0.0);
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
        if t.button.is_some() {
            t.last = (x, y);
            return false;
        }
        if !t.moving {
            if (x - t.start.0).abs().max((y - t.start.1).abs()) <= TAP_SLOP_PX {
                return false;
            }
            // past the slop: the whole travel from the start counts
            t.moving = true;
        }
        let dx = (x - t.last.0) as f32 * ACCEL + self.rem.0;
        let dy = (y - t.last.1) as f32 * ACCEL + self.rem.1;
        t.last = (x, y);
        let (ix, iy) = (dx.trunc(), dy.trunc());
        self.rem = (dx - ix, dy - iy);
        let b = cursor_bounds(self.area);
        let nx = (self.cursor.0 + ix as i32).clamp(b.x, b.right() - 1);
        let ny = (self.cursor.1 + iy as i32).clamp(b.y, b.bottom() - 1);
        if nx != self.cursor.0 + ix as i32 {
            self.rem.0 = 0.0;
        }
        if ny != self.cursor.1 + iy as i32 {
            self.rem.1 = 0.0;
        }
        let moved = (nx, ny) != self.cursor;
        self.cursor = (nx, ny);
        moved
    }

    /// Finger up at X time `t`. The release position is not used: the pointer sits on the cursor
    /// by then, the finger's last position is what counts.
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
        if touch.moving {
            return None;
        }
        let held = t.wrapping_sub(touch.t0).saturating_sub(BRIDGE_LIFT_MS);
        Some(PadOut::Click(if held >= LONG_PRESS_MS {
            BUTTON_RIGHT
        } else {
            BUTTON_LEFT
        }))
    }
}

// ------------------------------------------------------------------------------------ drawing

/// The mini-map: the visible area in small, the pad's band greyed, a dot where the cursor is.
pub fn paint_map(p: &mut Painter, map: Rect, area: (i32, i32), cursor: (i32, i32)) {
    p.cv.rect(map.inset(-2), BG);
    p.cv.rrect(map, 4, PRESSED_FILL);
    let pad = pad_rect(area);
    let band_y = map.y + pad.y * map.h / area.1.max(1);
    p.cv.rect(
        Rect::new(map.x + 1, band_y, map.w - 2, map.bottom() - band_y - 1),
        DIVIDER,
    );
    p.cv.rrect_stroke(map, 4, 1.0, CARD_BORDER);
    let dx = map.x + cursor.0 * map.w / area.0.max(1);
    let dy = map.y + cursor.1 * map.h / area.1.max(1);
    p.cv.circle(dx as f32, dy as f32, 5.0, BG);
    p.cv.circle(dx as f32, dy as f32, 3.5, TEXT);
}

/// Draws the whole pad over the canvas (pad-sized).
pub fn paint_pad(p: &mut Painter, st: &PadState) {
    let area = st.area();
    let bounds = p.cv.bounds();
    let l = pad_layout(area);
    p.cv.rect(bounds, BG);
    p.divider(0, 0, bounds.w);
    p.outline_box(l.surface, RADIUS_CARD, false, false, false);
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
        Style::new(Face::Bold, SIZE_BODY, TEXT),
        Rect::new(hint.x, y, hint.w, lh),
        Align::Left,
    );
    y += lh + 6;
    for s in lines {
        p.text(
            s,
            Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
            Rect::new(hint.x, y, hint.w, lh),
            Align::Left,
        );
        y += lh;
    }
    paint_map(p, l.map, area, st.cursor);
    let pressed = st.pressed_button();
    for (r, b) in &l.buttons {
        p.outline_box(*r, RADIUS - 4, pressed == Some(*b), false, false);
        let label = |s: &str, p: &mut Painter| {
            p.text(
                s,
                Style::new(Face::Bold, SIZE_SECONDARY, TEXT),
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
                p.cv.icon(Icon::Mouse, x, y, ICON, TEXT);
                p.cv.icon(Icon::ChevronDown, x + ICON + 2, y + 4, 16, TEXT);
            }
        }
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
        xc.conn.flush()?;
        Ok(MousePad {
            xc,
            bw,
            shell,
            mapped: false,
            active,
            st: PadState::new(area),
            canvas: Canvas::new(r.w as u32, r.h as u32, BG).ok_or("mouse pad canvas")?,
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
        self.canvas = Canvas::new(r.w as u32, r.h as u32, BG).ok_or("mouse pad canvas")?;
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
                        }
                    }
                }
                Event::Expose(e) if e.window == self.bw.win && e.count == 0 && self.mapped => {
                    self.redraw(fonts)?;
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
                    let out = self.st.release(e.time);
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

    #[test]
    fn relative_motion_with_acceleration() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        let c0 = s.cursor;
        s.press(x, y, 1000);
        // within the slop: nothing moves yet
        assert!(!s.motion(x + 5, y + 5));
        assert_eq!(s.cursor, c0);
        // past it: the whole travel from the start counts, x1.5
        assert!(s.motion(x + 20, y));
        assert_eq!(s.cursor, (c0.0 + 30, c0.1));
        assert!(s.motion(x + 20, y - 40));
        assert_eq!(s.cursor, (c0.0 + 30, c0.1 - 60));
        // fractions carry over: 1 px steps move 1, 2, 1, 2 ...
        let before = s.cursor.0;
        for i in 1..=4 {
            s.motion(x + 20 + i, y - 40);
        }
        assert_eq!(s.cursor.0, before + 6);
        // a drag clicks nothing
        assert_eq!(s.release(1400), None);
        // the next touch starts from where the cursor is, not from the finger
        let c1 = s.cursor;
        s.press(x + 300, y + 50, 2000);
        assert!(s.motion(x + 300 - 20, y + 50));
        assert_eq!(s.cursor, (c1.0 - 30, c1.1));
    }

    #[test]
    fn cursor_is_clamped_to_the_visible_area_above_the_pad() {
        for area in [P, L] {
            let mut s = PadState::new(area);
            let (x, y) = on_surface(area);
            s.press(x, y, 0);
            s.motion(x - 2000, y - 2000);
            assert_eq!(s.cursor, (0, 0));
            s.motion(x + 3000, y + 3000);
            let b = cursor_bounds(area);
            assert_eq!(s.cursor, (b.right() - 1, b.bottom() - 1));
            assert_eq!(s.cursor, (area.0 - 1, pad_rect(area).y - 1));
            // and back out of the corner right away (no leftover fraction)
            s.motion(x + 2999, y + 3000);
            assert_eq!(s.cursor.0, area.0 - 2, "-1.5 px: one now, the half carried");
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
                       // the first warp's echo arrives late (after the finger sample): no jump
        assert!(!s.motion(c0.0, c0.1));
        assert_eq!(s.cursor, (c0.0 + 30, c0.1));
        // the newest echo too
        assert!(!s.motion(c0.0 + 30, c0.1));
        // the finger again: delta from its own last position
        assert!(s.motion(x + 30, y));
        assert_eq!(s.cursor, (c0.0 + 45, c0.1));
    }

    #[test]
    fn quick_tap_is_a_left_click_long_press_a_right_click() {
        let mut s = PadState::new(P);
        let (x, y) = on_surface(P);
        // the bridge releases 150 ms after the finger lifted
        s.press(x, y, 10_000);
        assert_eq!(s.release(10_000 + 120 + 150), Some(PadOut::Click(1)));
        s.press(x, y, 20_000);
        s.motion(x + 3, y - 2); // jitter within the slop still counts as a tap
        assert_eq!(s.release(20_000 + 499 + 150), Some(PadOut::Click(1)));
        s.press(x, y, 30_000);
        assert_eq!(s.release(30_000 + 500 + 150), Some(PadOut::Click(3)));
        // held, then moved: a drag, no click
        s.press(x, y, 40_000);
        s.motion(x + 40, y);
        assert_eq!(s.release(41_000), None);
        // X time wraps around
        s.press(x, y, u32::MAX - 50);
        assert_eq!(s.release(100), Some(PadOut::Click(1)));
        // a release without a press does nothing
        assert_eq!(s.release(5), None);
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
