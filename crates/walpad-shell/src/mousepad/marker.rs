use super::layout::MARKER_CLASS;
use walpad_ui::tokens::text;
use walpad_ui::{Canvas, Color, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection as _;
use x11rb::protocol::xproto::{
    self, ConnectionExt as _, CreateGCAux, EventMask, Window as XWindow,
};
use x11rb::wrapper::ConnectionExt as _;

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
pub(super) struct Marker {
    pub(super) win: XWindow,
    gc: xproto::Gcontext,
    depth: u8,
    canvas: Canvas,
}

impl Marker {
    /// None when the server has no SHAPE extension (a rectangular marker would hide the app and
    /// catch the clicks) or a request fails: the pad then works without it.
    pub(super) fn create(xc: &XConn, depth: u8) -> Option<Marker> {
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

    pub(super) fn present(&self, xc: &XConn) -> Result<(), XError> {
        crate::present::present(
            &xc.conn,
            self.win,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    pub(super) fn set_mapped(&self, xc: &XConn, on: bool) -> Result<(), XError> {
        if on {
            xc.conn.map_window(self.win)?;
        } else {
            xc.conn.unmap_window(self.win)?;
        }
        Ok(())
    }

    /// Tip on `cursor`, above every other window.
    pub(super) fn place(&self, xc: &XConn, cursor: (i32, i32)) -> Result<(), XError> {
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
