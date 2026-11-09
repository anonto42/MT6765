//! The shell against a real Xvfb: properties, first frame, /run/walpad-ok, click -> tab switch,
//! resize on rotation.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_shell::shell::{Options, Shell};
use walpad_ui::tokens::*;
use walpad_x::XConn;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    self, AtomEnum, ConfigureWindowAux, ConnectionExt as _, ImageFormat, Window,
};
use x11rb::protocol::xtest::ConnectionExt as _;

/// A display number no other test in this binary (or process) uses: the tests run in parallel.
fn display_num() -> u32 {
    use std::sync::atomic::AtomicU32;
    static NEXT: AtomicU32 = AtomicU32::new(0);
    200 + (std::process::id() % 100) * 8 + NEXT.fetch_add(1, Ordering::SeqCst) % 8
}

struct Xvfb(Child, PathBuf);
impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn xvfb(n: u32) -> Option<Xvfb> {
    let c = Command::new("Xvfb")
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
    let sock = PathBuf::from(format!("/tmp/.X11-unix/X{n}"));
    let t = Instant::now();
    while !sock.exists() && t.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    sock.exists().then_some(Xvfb(c, sock))
}
fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(8) {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    panic!("timed out waiting for {what}");
}

struct Running {
    stop: Arc<AtomicBool>,
    h: Option<std::thread::JoinHandle<Result<(), String>>>,
}
impl Running {
    fn start(root: &Path, sock: &Path) -> Running {
        let (root, sock) = (root.to_path_buf(), sock.to_path_buf());
        let stop = Arc::new(AtomicBool::new(false));
        let s2 = stop.clone();
        let h = std::thread::spawn(move || {
            let mut sh = Shell::new(Options {
                root: Root::new(root),
                x_socket: Some(sock),
                core_socket: None,
                fonts_dir: Some(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"),
                ),
            })?;
            sh.run(&s2)
        });
        Running { stop, h: Some(h) }
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.h.take() {
            let _ = h.join();
        }
    }
}

struct Probe(XConn);
impl Probe {
    fn find_shell(&self) -> Option<Window> {
        let c = &self.0.conn;
        for w in c.query_tree(self.0.root).ok()?.reply().ok()?.children {
            let p = c
                .get_property(false, w, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 64)
                .ok()?
                .reply()
                .ok()?;
            if p.value.starts_with(b"walpad-shell\0") {
                return Some(w);
            }
        }
        None
    }
    fn image(&self, w: Window, width: u16, height: u16) -> Vec<u8> {
        self.0
            .conn
            .get_image(ImageFormat::Z_PIXMAP, w, 0, 0, width, height, !0)
            .unwrap()
            .reply()
            .unwrap()
            .data
    }
    fn click(&self, x: i16, y: i16) {
        let (c, r) = (&self.0.conn, self.0.root);
        c.xtest_fake_input(xproto::MOTION_NOTIFY_EVENT, 0, 0, r, x, y, 0)
            .unwrap();
        c.xtest_fake_input(xproto::BUTTON_PRESS_EVENT, 1, 0, r, 0, 0, 0)
            .unwrap();
        c.xtest_fake_input(xproto::BUTTON_RELEASE_EVENT, 1, 0, r, 0, 0, 0)
            .unwrap();
        c.flush().unwrap();
    }
}
fn px(img: &[u8], w: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let i = (y * w + x) * 4;
    (img[i + 2], img[i + 1], img[i]) // BGRX -> RGB
}
fn count(img: &[u8], w: usize, x0: usize, x1: usize, y0: usize, y1: usize, c: Color) -> usize {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .filter(|&(x, y)| px(img, w, x, y) == (c.r, c.g, c.b))
        .count()
}

#[test]
fn shell_runs_on_a_real_x_server() {
    let n = display_num();
    let Some(x) = xvfb(n) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let root = std::env::temp_dir().join(format!("walpad-shell-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("run/walpad")).unwrap();
    let _sh = Running::start(&root, &x.1);
    let probe = Probe(walpad_x::connect(&x.1).unwrap());

    // 1. the health file appears only after the first frame is on screen
    wait_for("/run/walpad-ok", || root.join("run/walpad-ok").exists());

    // 2. window properties
    let win = probe
        .find_shell()
        .expect("shell window with WM_CLASS walpad-shell");
    let name = probe
        .0
        .conn
        .get_property(false, win, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 64)
        .unwrap()
        .reply()
        .unwrap();
    assert_eq!(name.value, b"WalPad Home");
    let g = probe.0.conn.get_geometry(win).unwrap().reply().unwrap();
    assert_eq!((g.width, g.height), (800, 1280), "portrait by default");

    // 3. the first frame: status bar bg, nav bar surface, Home highlighted in accent
    let (w, h) = (800usize, 1280usize);
    let img = probe.image(win, 800, 1280);
    assert_eq!(
        px(&img, w, 400, 600),
        (BG.r, BG.g, BG.b),
        "content background"
    );
    assert_eq!(
        px(&img, w, 5, 1250),
        (SURFACE.r, SURFACE.g, SURFACE.b),
        "bottom navigation surface"
    );
    let home_accent = count(&img, w, 0, 160, 1192, h, ACCENT);
    assert!(
        home_accent > 20,
        "Home tab is drawn in the accent colour ({home_accent})"
    );
    assert_eq!(
        count(&img, w, 320, 480, 1192, h, ACCENT),
        0,
        "Network is not active yet"
    );

    // 4. a real click on the Network tab switches the active tab
    probe.click(400, 1240);
    wait_for("Network tab active", || {
        let img = probe.image(win, 800, 1280);
        count(&img, w, 320, 480, 1192, h, ACCENT) > 20
            && count(&img, w, 0, 160, 1192, h, ACCENT) == 0
    });

    // 5. rotation: the window manager resizes the window; the shell re-lays-out for 1280x800
    probe
        .0
        .conn
        .configure_window(win, &ConfigureWindowAux::new().width(1280).height(800))
        .unwrap();
    probe.0.conn.flush().unwrap();
    wait_for("landscape layout", || {
        let g = probe.0.conn.get_geometry(win).unwrap().reply().unwrap();
        if (g.width, g.height) != (1280, 800) {
            return false;
        }
        let img = probe.image(win, 1280, 800);
        px(&img, 1280, 1270, 760) == (SURFACE.r, SURFACE.g, SURFACE.b)
            && px(&img, 1280, 600, 400) == (BG.r, BG.g, BG.b)
            && count(&img, 1280, 512, 768, 712, 800, ACCENT) > 20
    });
    // the active tab (Network) is still Network after the resize, and a tap lands on the new layout
    probe.click(1280 / 5 * 4 + 40, 760);
    wait_for("Settings active in landscape", || {
        let img = probe.image(win, 1280, 800);
        count(&img, 1280, 1024, 1280, 712, 800, ACCENT) > 20
    });
}

// ---------------------------------------------------------------------------------------------
// shell + real core daemon + real X server

fn core_status(root: &Path) -> Option<serde_json::Value> {
    use std::io::{BufRead, BufReader, Write};
    let s = std::os::unix::net::UnixStream::connect(root.join("run/walpad/control.sock")).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut w = s.try_clone().ok()?;
    let mut r = BufReader::new(s);
    let mut call = |id: u64, cmd: &str| -> Option<serde_json::Value> {
        writeln!(
            w,
            "{}",
            serde_json::json!({"v":1,"id":id,"cmd":cmd,"args":{"client":"t","proto":1}})
        )
        .ok()?;
        let mut l = String::new();
        r.read_line(&mut l).ok()?;
        serde_json::from_str(&l).ok()
    };
    call(1, "hello")?;
    call(2, "get_status").map(|v| v["data"].clone())
}

#[test]
fn shell_drives_the_core_and_shows_when_it_is_gone() {
    use walpad_core::{daemon::Daemon, fake};
    use walpad_shell::home;
    use walpad_ui::layout::Frame;

    let n = display_num();
    let Some(x) = xvfb(n) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let root = std::env::temp_dir().join(format!("walpad-shell-core-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("usr/bin")).unwrap();
    std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
    fake::populate(&root).unwrap();
    std::fs::write(root.join("usr/bin/sleep"), "").unwrap(); // makes "browser" count as installed
    std::fs::write(root.join("etc/walpad/apps.json"), r#"[{"id":"browser","name":"Browser","icon":"globe","exec":["sleep","30"],"single_instance":true,"wm_class":"x"}]"#).unwrap();

    let mut core = Daemon::new(Root::new(&root)).unwrap();
    let core_stop = Arc::new(AtomicBool::new(false));
    let cs = core_stop.clone();
    let core_thread = std::thread::spawn(move || core.run(&cs));
    let _sh = Running::start(&root, &x.1);
    let probe = Probe(walpad_x::connect(&x.1).unwrap());
    wait_for("shell window", || probe.find_shell().is_some());
    let win = probe.find_shell().unwrap();

    // connected: the frame is drawn (nav bar present) AND the offline banner has disappeared.
    // (The window background equals BG, so "no banner" alone is true before the first frame.)
    let banner_px = |img: &[u8]| px(img, 800, 400, 64);
    wait_for("drawn and connected (no banner)", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, 5, 1250) == (SURFACE.r, SURFACE.g, SURFACE.b)
            && banner_px(&img) == (BG.r, BG.g, BG.b)
    });

    let frame = Frame::new(800, 1280);
    let l = home::layout(&frame, 0, 2); // Browser + Settings tiles (terminal is not installed)
    let c = l.buttons[0].center();
    probe.click(c.0 as i16, c.1 as i16);
    wait_for("rotation locked in the core", || {
        core_status(&root).is_some_and(|s| s["rotation_locked"] == true)
    });

    let right = (l.slider.inset(SP_4).right() - 1) as i16;
    probe.click(right, l.slider.center().1 as i16);
    wait_for("brightness 255 in the core", || {
        core_status(&root).is_some_and(|s| s["brightness"] == 255)
    });
    // the status event flows back: the shell now shows 100%
    wait_for("shell shows the new brightness", || {
        // knob at the right end of the track: accent pixels near the right edge of the slider
        let img = probe.image(win, 800, 1280);
        let (sx, sy) = (
            l.slider.inset(SP_4).right() as usize,
            l.slider.center().1 as usize,
        );
        (sx - 20..sx + 2).any(|xx| px(&img, 800, xx, sy) == (ACCENT.r, ACCENT.g, ACCENT.b))
    });

    // the core goes away: the "System service not responding" banner appears
    core_stop.store(true, Ordering::Relaxed);
    core_thread.join().unwrap();
    wait_for("offline banner", || {
        banner_px(&probe.image(win, 800, 1280))
            == (SURFACE_RAISED.r, SURFACE_RAISED.g, SURFACE_RAISED.b)
    });
}

// ---------------------------------------------------------------------------------------------
// the Wi-Fi flow, clicked through for real: shell + core + fake supplicant + X server

#[test]
fn wifi_scan_and_secured_connect_clicked_through_the_real_stack() {
    use walpad_core::{daemon::Daemon, fake, wpa};
    use walpad_shell::network::{layout, NetEntry, NetModel};
    use walpad_shell::overlay::password_geom;
    use walpad_ui::components::{keyboard_layout, Key, KeyboardLayer};
    use walpad_ui::layout::Frame;

    let n = display_num();
    let Some(x) = xvfb(n) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let root = std::env::temp_dir().join(format!("walpad-shell-wifi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("etc/wpa_supplicant")).unwrap();
    std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
    fake::populate(&root).unwrap();
    let sup = wpa::fake::spawn(
        &root,
        wpa::fake::default_aps(),
        Some(root.join("etc/wpa_supplicant/wpa_supplicant.conf")),
    )
    .unwrap();
    let mut core = Daemon::new(Root::new(&root)).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let core_thread = std::thread::spawn(move || core.run(&s2));
    let _sh = Running::start(&root, &x.1);
    let probe = Probe(walpad_x::connect(&x.1).unwrap());
    wait_for("shell window", || probe.find_shell().is_some());
    let win = probe.find_shell().unwrap();
    wait_for("drawn and connected", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, 5, 1250) == (SURFACE.r, SURFACE.g, SURFACE.b)
            && px(&img, 800, 400, 64) == (BG.r, BG.g, BG.b)
    });
    let click = |sx: i32, sy: i32| {
        probe.click(sx as i16, sy as i16);
        std::thread::sleep(Duration::from_millis(120));
    };
    let frame = Frame::new(800, 1280);
    let view_y = frame.content.y; // no banner while connected

    // Network tab (third nav item)
    click(frame.nav.w / 5 * 2 + 20, frame.nav.y + 40);
    // the status has an address but no SSID yet, so there is no "connected" card
    let st = None;
    let empty = NetModel::default();
    let l = layout(&frame, &st, &empty);
    // wait until the tab is showing: the Scan button is drawn (a Scan label region differs from bg)
    let scan = l.scan_btn.center();
    wait_for("network tab", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, 700, (view_y + l.scan_btn.y + 2) as usize) != (BG.r, BG.g, BG.b)
    });
    click(scan.0, view_y + scan.1);

    // scan results arrive: the first row (Home, strongest) shows accent signal bars
    let rows = 3;
    let model = NetModel {
        results: (0..rows)
            .map(|i| NetEntry {
                ssid_hex: String::new(),
                ssid: format!("{i}"),
                bars: 3,
                secured: true,
            })
            .collect(),
        ..NetModel::default()
    };
    let l2 = layout(&frame, &st, &model);
    let row0 = l2.avail_rows[0];
    wait_for("scan results", || {
        let img = probe.image(win, 800, 1280);
        let y = (view_y + row0.y + row0.h / 2) as usize;
        (730..765).any(|xx| px(&img, 800, xx, y) == (ACCENT.r, ACCENT.g, ACCENT.b))
    });
    assert!(
        sup.log.lock().unwrap().iter().any(|c| c == "SCAN"),
        "the shell asked the core, the core asked the supplicant"
    );

    // tap "Home" (secured): the password sheet opens, type the passphrase on the on-screen keyboard
    click(row0.center().0, view_y + row0.center().1);
    let g = password_geom(&frame);
    wait_for("password sheet", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, g.sheet.x as usize + 5, g.sheet.y as usize + 100)
            == (SURFACE_RAISED.r, SURFACE_RAISED.g, SURFACE_RAISED.b)
    });
    for ch in "correct horse".chars() {
        let want = if ch == ' ' { Key::Space } else { Key::Char(ch) };
        let k = keyboard_layout(g.keyboard, KeyboardLayer::Lower)
            .into_iter()
            .find(|k| k.key == want)
            .unwrap();
        click(k.rect.center().0, k.rect.center().1);
    }
    click(g.connect.center().0, g.connect.center().1);

    // the supplicant joined "Home" with the derived key and the core reports it
    wait_for("connected to Home in the core", || {
        core_status(&root).is_some_and(|s| s["wifi"]["ssid"] == "Home")
    });
    let log = sup.log.lock().unwrap().join("\n");
    assert!(
        !log.contains("correct horse"),
        "the passphrase never reaches the supplicant, only the derived key"
    );
    assert!(log.contains("SET_NETWORK 1 psk "), "{log}");
    // the sheet closes by itself on success
    wait_for("sheet closed", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, g.sheet.x as usize + 5, g.sheet.y as usize + 100)
            != (SURFACE_RAISED.r, SURFACE_RAISED.g, SURFACE_RAISED.b)
    });

    stop.store(true, Ordering::Relaxed);
    core_thread.join().unwrap();
}

// ---------------------------------------------------------------------------------------------
// Device tab: tap a setting -> core writes devd.json -> the shell re-reads it and redraws

#[test]
fn device_setting_round_trips_through_the_core_and_back_to_the_screen() {
    use walpad_core::{daemon::Daemon, fake};
    use walpad_shell::config::DevdSettings;
    use walpad_shell::device_screen::{self, Item};
    use walpad_ui::layout::Frame;

    let n = display_num();
    let Some(x) = xvfb(n) else {
        eprintln!("Xvfb not available: skipping");
        return;
    };
    let root = std::env::temp_dir().join(format!("walpad-shell-device-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
    fake::populate(&root).unwrap();
    let mut core = Daemon::new(Root::new(&root)).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let s2 = stop.clone();
    let core_thread = std::thread::spawn(move || core.run(&s2));
    let _sh = Running::start(&root, &x.1);
    let probe = Probe(walpad_x::connect(&x.1).unwrap());
    wait_for("shell window", || probe.find_shell().is_some());
    let win = probe.find_shell().unwrap();
    wait_for("drawn and connected", || {
        let img = probe.image(win, 800, 1280);
        px(&img, 800, 5, 1250) == (SURFACE.r, SURFACE.g, SURFACE.b)
            && px(&img, 800, 400, 64) == (BG.r, BG.g, BG.b)
    });
    let frame = Frame::new(800, 1280);
    let (placed, _) = device_screen::layout(&frame, &None, &DevdSettings::default());
    let dim = placed.iter().find(|p| p.item == Item::DimSeg).unwrap().rect;
    let seg_w = (dim.w - 8) / 5;
    let seg = |i: i32| Rect4 {
        x: dim.x + 4 + i * seg_w,
        y: frame.content.y + dim.y + 4,
        w: seg_w,
        h: 48,
    };
    struct Rect4 {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }
    let accent_in = |img: &[u8], r: &Rect4| {
        (r.y..r.y + r.h)
            .flat_map(|yy| (r.x..r.x + r.w).map(move |xx| (xx, yy)))
            .filter(|&(xx, yy)| {
                px(img, 800, xx as usize, yy as usize) == (ACCENT.r, ACCENT.g, ACCENT.b)
            })
            .count()
    };

    // open the Device tab (fourth nav item); the default "90 s" is selected
    probe.click((frame.nav.w / 5 * 3 + 20) as i16, (frame.nav.y + 40) as i16);
    wait_for("Device tab with 90 s selected", || {
        let img = probe.image(win, 800, 1280);
        accent_in(&img, &seg(3)) > 100
    });
    // tap "30 s": the core writes devd.json, the shell reads it back and moves the highlight
    let (cx, cy) = (seg(1).x + seg(1).w / 2, seg(1).y + 20);
    probe.click(cx as i16, cy as i16);
    wait_for("devd.json written by the core", || {
        std::fs::read_to_string(root.join("etc/walpad/devd.json"))
            .map(|s| s.contains("\"dim_after_s\": 30"))
            .unwrap_or(false)
    });
    wait_for("highlight moved to 30 s", || {
        let img = probe.image(win, 800, 1280);
        accent_in(&img, &seg(1)) > 100 && accent_in(&img, &seg(3)) == 0
    });

    stop.store(true, Ordering::Relaxed);
    core_thread.join().unwrap();
}
