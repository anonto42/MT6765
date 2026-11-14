use super::*;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, WindowClass,
};

#[test]
fn back_is_alt_left_in_browser_and_files_and_escape_elsewhere() {
    let c = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    assert_eq!(back_spec(&c("Navigator", "firefox-esr")), "alt+Left");
    assert_eq!(back_spec(&c("pcmanfm", "Pcmanfm")), "alt+Left");
    assert_eq!(back_spec(&c("st", "St")), "Escape");
    assert_eq!(back_spec(&[]), "Escape");
    assert!(parse_key_spec(back_spec(&[])).is_some());
    assert!(parse_key_spec("alt+Left").is_some());
}

#[test]
fn wm_class_matching() {
    let c = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
    assert!(class_matches(&c("st", "St"), "st"));
    assert!(
        !class_matches(&c("Navigator", "firefox-esr"), "st"),
        "short needles are exact"
    );
    assert!(class_matches(&c("Navigator", "Firefox-esr"), "firefox-esr"));
    assert!(class_matches(&c("Navigator", "Firefox-esr"), "firefox"));
    assert!(!class_matches(&c("a", "b"), ""));
}

#[test]
fn keysym_and_spec_parsing() {
    assert_eq!(keysym("Return"), Some(0xff0d));
    assert_eq!(keysym("Prior"), Some(0xff55));
    assert_eq!(keysym("F5"), Some(0xffc2));
    assert_eq!(keysym("a"), Some(0x61));
    assert_eq!(keysym("rm -rf"), None);
    assert_eq!(parse_key_spec("ctrl+Tab"), Some((vec![0xffe3], 0xff09)));
    assert_eq!(
        parse_key_spec("ctrl+alt+Left"),
        Some((vec![0xffe3, 0xffe9], 0xff51))
    );
    assert_eq!(parse_key_spec("Down"), Some((vec![], 0xff54)));
    assert_eq!(parse_key_spec("a+b"), None, "non-modifier before the key");
    assert_eq!(parse_key_spec(""), None);
}

struct Xvfb(Child, std::path::PathBuf);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn xvfb(n: u32) -> Option<Xvfb> {
    // a killed Xvfb leaves its socket behind: do not mistake it for the new server
    let _ = std::fs::remove_file(format!("/tmp/.X11-unix/X{n}"));
    let child = Command::new("Xvfb")
        .args([
            &format!(":{n}"),
            "-screen",
            "0",
            "1280x1280x24",
            "-nolisten",
            "tcp",
            "-ac",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let sock = std::path::PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    let t = Instant::now();
    while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some(Xvfb(child, sock))
}

#[test]
fn struts_pointer_keys_dock_against_real_xvfb() {
    let Some(x) = xvfb(60 + std::process::id() % 30) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let mut b = XBridge::connect(&x.1).unwrap();

    // struts
    b.set_struts(0).unwrap();
    b.set_struts(90).unwrap();
    let sp = b.atom("_NET_WM_STRUT_PARTIAL").unwrap();
    let w = b.strut_win.unwrap();
    let got = b
        .conn
        .get_property(false, w, sp, AtomEnum::CARDINAL, 0, 12)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(
        got.value32().unwrap().collect::<Vec<_>>(),
        crate::logic::rotation::strut_partial(90).to_vec()
    );

    // pointer
    b.pointer_move(123, 456).unwrap();
    let q = b.conn.query_pointer(b.root).unwrap().reply().unwrap();
    assert_eq!((q.root_x, q.root_y), (123, 456));

    // keyboard dock: a fake matchbox-keyboard window gets moved and resized
    let kw = b.conn.generate_id().unwrap();
    b.conn
        .create_window(
            0,
            kw,
            b.root,
            0,
            0,
            100,
            100,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap();
    b.conn
        .change_property8(
            PropMode::REPLACE,
            kw,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"matchbox-keyboard\0Matchbox-keyboard\0",
        )
        .unwrap();
    b.conn.map_window(kw).unwrap();
    assert_eq!(b.dock_keyboard(0).unwrap(), 1);
    let g = b.conn.get_geometry(kw).unwrap().reply().unwrap();
    assert_eq!((g.x, g.y, g.width, g.height), (0, 896, 800, 384));
    b.dock_keyboard(90).unwrap();
    let g = b.conn.get_geometry(kw).unwrap().reply().unwrap();
    assert_eq!((g.x, g.y, g.width, g.height), (0, 560, 1280, 240));

    // key injection reaches a window that listens for key presses (pointer is over it)
    let lw = b.conn.generate_id().unwrap();
    b.conn
        .create_window(
            0,
            lw,
            b.root,
            0,
            0,
            1280,
            1280,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS),
        )
        .unwrap();
    b.conn.map_window(lw).unwrap();
    b.conn
        .set_input_focus(xproto::InputFocus::POINTER_ROOT, lw, 0u32)
        .unwrap();
    b.conn.flush().unwrap();
    b.send_key("Return").unwrap();
    let want = b.keycode(0xff0d).unwrap().0;
    let t = Instant::now();
    let mut seen = false;
    while t.elapsed() < Duration::from_secs(3) && !seen {
        if let Some(x11rb::protocol::Event::KeyPress(e)) = b.conn.poll_for_event().unwrap() {
            seen = e.detail == want;
        } else {
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    assert!(seen, "Return key press was not delivered");
    assert!(b.send_key("bogus key").is_err());
}

#[test]
fn connect_to_missing_server_fails_fast() {
    let t = Instant::now();
    assert!(XBridge::connect(Path::new("/tmp/.X11-unix/X999")).is_err());
    assert!(t.elapsed() < Duration::from_secs(2));
}
