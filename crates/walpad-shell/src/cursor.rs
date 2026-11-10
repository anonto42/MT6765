//! No X cursor on the touch screen (owner request 2026-11-10: "when I open the touch pad another
//! mouse pointer appears").
//!
//! Xvfb draws the pointer into its framebuffer, so fbmirror copies it to the panel: with the
//! touch bridge's XTEST pointer that is a cursor under the finger, and while the mouse pad is open
//! a second one next to the pad's own marker (mousepad.rs, `walpad-cursor`). The shell hides the
//! X cursor for good with XFixes `HideCursor` on the root window: unlike a blank root cursor it
//! also wins over the cursors apps set themselves (Firefox's arrow, text beams, GTK busy
//! cursors). The hide lasts as long as the connection that asked for it, so the connection is
//! held for the life of the shell; walpad-session additionally blanks the root cursor
//! (`xsetroot -cursor`) for the time before the shell is up.
//!
//! Decision: during normal touch use no pointer is drawn at all (a finger is its own pointer);
//! while the mouse pad is open the pad's `walpad-cursor` marker is the only cursor on screen.
use std::path::Path;
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::ConnectionExt as _;

/// Holds the connection that keeps the cursor hidden.
pub struct CursorHider {
    _xc: XConn,
}

/// Hide the X cursor on the root window until the returned value is dropped.
pub fn hide(socket: &Path) -> Result<CursorHider, XError> {
    let xc = walpad_x::connect(socket)?;
    // HideCursor needs XFixes 4.0
    let v = xc.conn.xfixes_query_version(4, 0)?.reply()?;
    if v.major_version < 4 {
        return Err(XError(format!(
            "XFixes {}.{} is too old to hide the cursor",
            v.major_version, v.minor_version
        )));
    }
    xc.conn.xfixes_hide_cursor(xc.root)?;
    xc.conn.flush()?;
    Ok(CursorHider { _xc: xc })
}
