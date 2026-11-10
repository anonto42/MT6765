//! System dock (decision D3): a 56 px strip with Home, Keyboard and Close app, shown only while
//! another app (Firefox, the terminal, ...) is in front, so those apps can always reach Home by
//! touch. It is a separate small X window (WM_CLASS `walpad-dock`, `_NET_WM_WINDOW_TYPE_DOCK`) with
//! a bottom strut, mapped while a non-shell window is `_NET_ACTIVE_WINDOW` and unmapped when the
//! shell (or nothing) is active, so apps keep their whole area otherwise. It has its own X
//! connection; every wait on it has a deadline (walpad-x).
use crate::app::Cmd;
use std::os::unix::net::UnixStream;
use std::path::Path;
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
/// Title of the shell window (what Home raises).
pub const SHELL_TITLE: &str = crate::window::TITLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockButton {
    Home,
    Keyboard,
    CloseApp,
}

pub const BUTTONS: [(DockButton, &str, Icon); 3] = [
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

/// `_NET_WM_STRUT_PARTIAL`: struts count from the screen edge, so the part of the root below the
/// visible area is included (landscape: 480 + 56), and so is the keyboard under the dock (the
/// combined strut; the keyboard's own strut is smaller, the window manager takes the largest).
pub fn strut_partial(area: (i32, i32), kbd_h: i32, root_h: i32) -> [u32; 12] {
    crate::keyboard::bottom_strut(dock_rect(area, kbd_h), root_h)
}

/// Hit areas of the three buttons (equal thirds of the strip, strip-local coordinates).
pub fn button_rects(w: i32) -> [(Rect, DockButton); 3] {
    let cell = w / 3;
    let mut out = [(Rect::new(0, 0, 0, 0), DockButton::Home); 3];
    for (i, (b, _, _)) in BUTTONS.iter().enumerate() {
        let x = i as i32 * cell;
        let cw = if i == 2 { w - 2 * cell } else { cell };
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
    // line-art like the shell: charcoal strip, a divider on top, outlined pill buttons
    p.cv.rect(Rect::new(0, 0, w, DOCK_H), BG);
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

/// Should the dock be up? `active` is `_NET_ACTIVE_WINDOW` (0 = none). The dock's own window being
/// active (a tap on it) changes nothing.
pub fn wants_visible(active: u32, shell: u32, dock: u32, currently: bool) -> bool {
    if active == dock {
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
        DockButton::Keyboard => Some(Cmd::KeyboardToggle),
        DockButton::CloseApp => app_for_class(apps, active_class).map(Cmd::CloseApp),
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

pub struct Dock {
    xc: XConn,
    win: XWindow,
    /// 1x1 managed window that only carries the strut (the strip itself is override-redirect so
    /// openbox cannot move it; openbox only honours struts of windows it manages)
    strut_win: XWindow,
    shell: XWindow,
    gc: u32,
    depth: u8,
    atoms: Atoms,
    apps: Vec<(String, String)>,
    root_h: i32,
    area: (i32, i32),
    /// Height of the system keyboard below the dock (0 while it is hidden).
    kbd_h: i32,
    mapped: bool,
    active: u32,
    pressed: Option<DockButton>,
    canvas: Canvas,
}

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
        let r = dock_rect(area, 0);
        let win = conn.generate_id()?;
        let aux = CreateWindowAux::new()
            .event_mask(
                EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::EXPOSURE
                    | EventMask::STRUCTURE_NOTIFY,
            )
            .background_pixel(0x001A_1D21)
            // openbox placed the managed strip at the top of the screen over the app, ignoring
            // the requested position: the strip is unmanaged and placed by us
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
        let class = format!("{WM_CLASS_INSTANCE}\0{WM_CLASS_CLASS}\0");
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
            WM_CLASS_INSTANCE.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            atoms.net_name,
            atoms.utf8,
            WM_CLASS_INSTANCE.as_bytes(),
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            atoms.window_type,
            AtomEnum::ATOM,
            &[atoms.type_dock],
        )?;
        // never take the keyboard focus from the app in front: WM_HINTS flags=InputHint, input=false
        conn.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_HINTS,
            AtomEnum::WM_HINTS,
            &[1, 0, 0, 0, 0, 0, 0, 0, 0],
        )?;
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
            &[1, 0, 0, 0, 0, 0, 0, 0, 0],
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
            strut_win,
            shell,
            gc,
            depth,
            atoms,
            apps,
            root_h,
            area,
            kbd_h: 0,
            mapped: false,
            active: 0,
            pressed: None,
            canvas: Canvas::new(r.w as u32, DOCK_H as u32, BG).ok_or("dock canvas")?,
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
    pub fn window(&self) -> XWindow {
        self.win
    }
    pub fn visible(&self) -> bool {
        self.mapped
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

    /// The shell window changed size (rotation): move and resize the strip and its strut.
    pub fn set_shell_size(&mut self, w: i32, h: i32) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        let r = dock_rect(area, self.kbd_h);
        self.canvas = Canvas::new(r.w as u32, DOCK_H as u32, BG).ok_or("dock canvas")?;
        self.place(r)
    }

    /// The system keyboard appeared (its height) or went away (0): stack the strip directly above
    /// it and extend the strut over both.
    pub fn set_keyboard_height(&mut self, kbd_h: i32) -> Result<(), XError> {
        if kbd_h == self.kbd_h {
            return Ok(());
        }
        self.kbd_h = kbd_h;
        self.place(dock_rect(self.area, kbd_h))
    }

    fn place(&mut self, r: Rect) -> Result<(), XError> {
        self.xc.conn.configure_window(
            self.win,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        self.write_strut()?;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn set_mapped(&mut self, on: bool) -> Result<(), XError> {
        if on == self.mapped {
            if on {
                // the window manager restacks the apps it manages; keep the unmanaged strip on top
                self.xc.conn.configure_window(
                    self.win,
                    &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
                )?;
                self.xc.conn.flush()?;
            }
            return Ok(());
        }
        self.mapped = on;
        if on {
            self.xc.conn.map_window(self.strut_win)?;
            self.xc.conn.map_window(self.win)?;
            // openbox places a newly mapped window itself (it ended up at the top of the screen,
            // covering the app): ask for our place again once mapped, and put the strip on top
            // (a remapped window keeps its old place in the stack)
            let r = dock_rect(self.area, self.kbd_h);
            self.xc.conn.configure_window(
                self.win,
                &ConfigureWindowAux::new()
                    .x(r.x)
                    .y(r.y)
                    .width(r.w as u32)
                    .height(r.h as u32)
                    .stack_mode(StackMode::ABOVE),
            )?;
        } else {
            self.pressed = None;
            self.xc.conn.unmap_window(self.win)?;
            self.xc.conn.unmap_window(self.strut_win)?;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            self.pressed,
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

    /// Handle everything the server has sent (non-blocking); returns the commands for the core.
    pub fn poll(&mut self, fonts: &mut Fonts) -> Result<Vec<Cmd>, XError> {
        let mut cmds = Vec::new();
        let (w, _) = self.area;
        while let Some(ev) = self.xc.conn.poll_for_event()? {
            match ev {
                Event::PropertyNotify(e) if e.atom == self.atoms.active => {
                    self.active = self.read_active();
                    let on = wants_visible(self.active, self.shell, self.win, self.mapped);
                    let was = self.mapped;
                    self.set_mapped(on)?;
                    if on && !was {
                        // paint right away: a remapped window is not always sent an Expose
                        self.redraw(fonts)?;
                    }
                }
                Event::Expose(e) if e.window == self.win && e.count == 0 => {
                    self.redraw(fonts)?;
                }
                Event::ButtonPress(e) if e.event == self.win && e.detail == 1 => {
                    self.pressed = button_at(w, e.event_x as i32, e.event_y as i32);
                    self.redraw(fonts)?;
                }
                Event::ButtonRelease(e) if e.event == self.win && e.detail == 1 => {
                    let hit = button_at(w, e.event_x as i32, e.event_y as i32);
                    let down = self.pressed.take();
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
                    }
                    self.redraw(fonts)?;
                }
                Event::Error(e) => return Err(XError(format!("dock X error: {e:?}"))),
                _ => {}
            }
        }
        Ok(cmds)
    }

    /// For tests and diagnostics: is the window currently mapped according to the server?
    pub fn server_mapped(&self) -> bool {
        self.xc
            .conn
            .get_window_attributes(self.win)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != MapState::UNMAPPED)
    }
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
    fn three_buttons_tile_the_strip_with_big_targets() {
        for w in [800, 1280] {
            let rs = button_rects(w);
            assert_eq!(
                rs.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
                [DockButton::Home, DockButton::Keyboard, DockButton::CloseApp]
            );
            assert_eq!(rs[0].0.x, 0);
            assert_eq!(rs[2].0.right(), w, "no gap at the right edge");
            for pair in rs.windows(2) {
                assert_eq!(pair[0].0.right(), pair[1].0.x);
            }
            assert!(rs.iter().all(|(r, _)| r.h >= TOUCH_MIN && r.w >= TOUCH_MIN));
        }
        assert_eq!(button_at(800, 10, 10), Some(DockButton::Home));
        assert_eq!(button_at(800, 400, 30), Some(DockButton::Keyboard));
        assert_eq!(button_at(800, 790, 30), Some(DockButton::CloseApp));
        assert_eq!(button_at(800, 400, 56), None, "below the strip");
    }

    #[test]
    fn visibility_follows_the_active_window() {
        let (shell, dock) = (10, 20);
        assert!(!wants_visible(0, shell, dock, false), "nothing active");
        assert!(
            !wants_visible(shell, shell, dock, true),
            "the shell is in front"
        );
        assert!(wants_visible(77, shell, dock, false), "a foreign window");
        assert!(
            wants_visible(dock, shell, dock, true),
            "a tap on the dock keeps it"
        );
        assert!(!wants_visible(dock, shell, dock, false));
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
