//! X11 presenter: premultiplied RGBA canvas -> BGRX ZPixmap via PutImage in row chunks below the
//! server's maximum request length. No MIT-SHM (the kernel has no SYSVIPC).
use walpad_ui::{Canvas, Rect};
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, Drawable, Gcontext, ImageFormat};

/// PutImage chunks stay well below 256 KiB so one frame never stalls the server for long.
const CHUNK_BYTES: usize = 256 * 1024;

/// RGBA (alpha ignored) rows `y0..y1`, columns `x0..x1` -> BGRX bytes.
pub fn rgba_to_bgrx(
    rgba: &[u8],
    stride_px: usize,
    x0: usize,
    x1: usize,
    y0: usize,
    y1: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        let row = &rgba[(y * stride_px + x0) * 4..(y * stride_px + x1) * 4];
        for px in row.as_chunks::<4>().0 {
            out.extend_from_slice(&[px[2], px[1], px[0], 0]);
        }
    }
    out
}

/// Rows per request for a damage width in px, given a byte budget.
pub fn rows_per_chunk(width_px: usize, max_request_bytes: usize) -> usize {
    let budget = max_request_bytes.min(CHUNK_BYTES).saturating_sub(64);
    (budget / (width_px.max(1) * 4)).max(1)
}

/// Copy `damage` (clipped to the canvas) to the window.
pub fn present<C: Connection>(
    conn: &C,
    win: Drawable,
    gc: Gcontext,
    depth: u8,
    cv: &Canvas,
    damage: Rect,
) -> Result<(), XError> {
    let bounds = cv.bounds();
    let x0 = damage.x.max(0).min(bounds.w) as usize;
    let y0 = damage.y.max(0).min(bounds.h) as usize;
    let x1 = damage.right().max(0).min(bounds.w) as usize;
    let y1 = damage.bottom().max(0).min(bounds.h) as usize;
    if x1 <= x0 || y1 <= y0 {
        return Ok(());
    }
    let step = rows_per_chunk(x1 - x0, conn.maximum_request_bytes());
    let mut y = y0;
    while y < y1 {
        let end = (y + step).min(y1);
        let data = rgba_to_bgrx(cv.rgba(), bounds.w as usize, x0, x1, y, end);
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            win,
            gc,
            (x1 - x0) as u16,
            (end - y) as u16,
            x0 as i16,
            y as i16,
            0,
            depth,
            &data,
        )?;
        y = end;
    }
    conn.flush()?;
    // Wait for Xvfb to finish rasterizing all PutImage chunks into Xvfb_screen0 before
    // waking fbmirror, so fast scrolls never present a torn half-frame.
    let _ = conn.get_input_focus().map(|c| c.reply());
    wake_fbmirror();
    Ok(())
}

/// Poke fbmirror's wake FIFO (non-blocking) so UI animations/scrolls present immediately
/// instead of waiting for fbmirror's 100 ms idle sleep.
fn wake_fbmirror() {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(0x800) // O_NONBLOCK
        .open("/run/walpad/fbwake")
    {
        let _ = f.write(&[1]);
    }
}

#[cfg(test)]
mod tests;
