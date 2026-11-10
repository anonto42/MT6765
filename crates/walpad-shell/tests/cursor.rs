//! The X cursor is never drawn (no second pointer next to the mouse pad's own marker): Xvfb's
//! framebuffer file, which fbmirror copies to the panel, is compared with and without the shell's
//! cursor hider, for the default cursor and for a cursor an app sets on its own window.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_x::XConn;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, CreateWindowAux, EventMask, WindowClass};
use x11rb::protocol::xtest::ConnectionExt as _;

struct Xvfb(Child, PathBuf, PathBuf);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.2);
    }
}

fn xvfb(n: u32) -> Option<Xvfb> {
    let fb = std::env::temp_dir().join(format!("walpad-cursor-fb-{n}"));
    let _ = std::fs::remove_dir_all(&fb);
    std::fs::create_dir_all(&fb).ok()?;
    let sock = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    let _ = std::fs::remove_file(&sock);
    let c = Command::new("Xvfb")
        .args([
            &format!(":{n}"),
            "-screen",
            "0",
            "400x300x24",
            "-fbdir",
            fb.to_str()?,
            "-nolisten",
            "tcp",
            "-ac",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let t = Instant::now();
    while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some(Xvfb(c, sock, fb))
}

/// The pixels of the XWD framebuffer file (what fbmirror reads).
fn pixels(fb: &Path) -> Vec<u8> {
    let d = std::fs::read(fb.join("Xvfb_screen0")).unwrap();
    let hs = u32::from_be_bytes(d[0..4].try_into().unwrap()) as usize;
    let ncolors = u32::from_be_bytes(d[76..80].try_into().unwrap()) as usize;
    d[hs + ncolors * 12..].to_vec()
}

fn pointer_to(xc: &XConn, x: i16, y: i16) {
    xc.conn
        .xtest_fake_input(
            x11rb::protocol::xproto::MOTION_NOTIFY_EVENT,
            0,
            0,
            xc.root,
            x,
            y,
            0,
        )
        .unwrap();
    xc.conn.flush().unwrap();
    // a round trip: the motion has been processed when the reply is back
    xc.conn.get_input_focus().unwrap().reply().unwrap();
    std::thread::sleep(Duration::from_millis(150));
}

#[test]
fn the_x_cursor_is_not_drawn_not_even_an_apps_own_cursor() {
    let n = 700 + std::process::id() % 200;
    let Some(x) = xvfb(n) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let xc = walpad_x::connect(&x.1).unwrap();
    // a plain background, so any cursor pixel shows
    xc.conn
        .change_window_attributes(
            xc.root,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new()
                .background_pixel(0x0020_2020),
        )
        .unwrap();
    xc.conn.clear_area(false, xc.root, 0, 0, 0, 0).unwrap();
    // what openbox does at start: the arrow as the root window's cursor
    let font = xc.conn.generate_id().unwrap();
    xc.conn.open_font(font, b"cursor").unwrap();
    let arrow = xc.conn.generate_id().unwrap();
    xc.conn
        .create_glyph_cursor(arrow, font, font, 68, 69, 0, 0, 0, 0xffff, 0xffff, 0xffff)
        .unwrap();
    xc.conn
        .change_window_attributes(
            xc.root,
            &x11rb::protocol::xproto::ChangeWindowAttributesAux::new().cursor(arrow),
        )
        .unwrap();
    xc.conn.flush().unwrap();
    pointer_to(&xc, 10, 10);
    let blank = pixels(&x.2);

    // premise: without the hider the default cursor is in the framebuffer
    pointer_to(&xc, 200, 150);
    let with_cursor = pixels(&x.2);
    assert_ne!(
        blank, with_cursor,
        "the test cannot see the cursor: no premise"
    );

    // an app window with its own cursor (the "fleur" glyph), covering the lower half
    let cur = xc.conn.generate_id().unwrap();
    xc.conn
        .create_glyph_cursor(cur, font, font, 52, 53, 0, 0, 0, 0xffff, 0xffff, 0xffff)
        .unwrap();
    let win = xc.conn.generate_id().unwrap();
    xc.conn
        .create_window(
            0,
            win,
            xc.root,
            0,
            150,
            400,
            150,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new()
                .background_pixel(0x0020_2020)
                .cursor(cur)
                .event_mask(EventMask::EXPOSURE),
        )
        .unwrap();
    xc.conn.map_window(win).unwrap();
    xc.conn.flush().unwrap();
    pointer_to(&xc, 10, 10);
    let blank_with_win = pixels(&x.2);
    pointer_to(&xc, 200, 220);
    assert!(
        blank_with_win != pixels(&x.2),
        "premise: the app's cursor is drawn"
    );

    // the shell hides it
    let _hider = walpad_shell::cursor::hide(&x.1).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    pointer_to(&xc, 10, 10);
    let blank_hidden = pixels(&x.2);
    for (px, py) in [(200, 70), (200, 220), (50, 280), (300, 30)] {
        pointer_to(&xc, px, py);
        assert!(
            pixels(&x.2) == blank_hidden,
            "a cursor is drawn at ({px}, {py})"
        );
    }
}
