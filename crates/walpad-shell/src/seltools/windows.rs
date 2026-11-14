use super::model::{active_class, bar_rect, buttons, is_terminal, list_rect};
use crate::dock::visible_area;
use crate::keyboard::{load_keymap, KeyMap};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Instant;
use walpad_ui::components::Painter;
use walpad_ui::edit_bar::{ClipHit, TEXT_BUTTONS};
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts, Rect};
use walpad_x::{XConn, XError};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConfigureWindowAux, ConnectionExt as _, CreateGCAux,
    CreateWindowAux, EventMask, PropMode, StackMode, Window as XWindow, WindowClass,
};
use x11rb::wrapper::ConnectionExt as _;

pub(super) struct Win {
    pub id: XWindow,
    pub gc: u32,
    pub canvas: Canvas,
    pub mapped: bool,
}

pub struct EditBar {
    pub(super) xc: XConn,
    pub(super) shell: XWindow,
    pub(super) keymap: KeyMap,
    pub(super) area: (i32, i32),
    pub(super) bottom_h: i32,
    pub(super) active_atom: u32,
    pub(super) active: u32,
    pub(super) depth: u8,
    pub(super) bar: Win,
    pub(super) list: Win,
    /// A terminal is in front.
    pub(super) terminal: bool,
    /// What the toolbar should show (the shell's rule), and what is mapped.
    wanted: bool,
    pub(super) bar_pressed: Option<usize>,
    pub(super) list_pressed: Option<ClipHit>,
    pub(super) entries: Vec<String>,
    pub(super) list_hits: Vec<(Rect, ClipHit)>,
    pub(super) list_until: Option<Instant>,
}

fn make_window(xc: &XConn, class: &str, wm_class: &str, r: Rect) -> Result<Win, XError> {
    let conn = &xc.conn;
    let id = conn.generate_id()?;
    conn.create_window(
        0,
        id,
        xc.root,
        r.x as i16,
        r.y as i16,
        r.w as u16,
        r.h as u16,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new()
            .event_mask(EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::EXPOSURE)
            .background_pixel(0x002B_3035)
            .override_redirect(1),
    )?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, id, &CreateGCAux::new())?;
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        format!("{class}\0{wm_class}\0").as_bytes(),
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        class.as_bytes(),
    )?;
    // WM_HINTS flags=InputHint, input=false
    conn.change_property32(
        PropMode::REPLACE,
        id,
        AtomEnum::WM_HINTS,
        AtomEnum::WM_HINTS,
        &[1u32, 0, 0, 0, 0, 0, 0, 0, 0],
    )?;
    Ok(Win {
        id,
        gc,
        canvas: Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("edit window canvas")?,
        mapped: false,
    })
}

impl EditBar {
    pub fn open(socket: &Path, shell: XWindow, shell_size: (i32, i32)) -> Result<Self, XError> {
        let xc = walpad_x::connect(socket)?;
        let keymap = load_keymap(&xc)?;
        let area = visible_area(shell_size.0, shell_size.1);
        let active_atom = xc
            .conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
            .reply()?
            .atom;
        let depth = xc
            .conn
            .setup()
            .roots
            .first()
            .map(|s| s.root_depth)
            .unwrap_or(24);
        let bar = make_window(
            &xc,
            super::BAR_CLASS,
            "WalPad-SelTools",
            bar_rect(area, 0, TEXT_BUTTONS.len()),
        )?;
        let list = make_window(
            &xc,
            super::LIST_CLASS,
            "WalPad-Clipboard",
            list_rect(area, 0, 0),
        )?;
        xc.conn.change_window_attributes(
            xc.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )?;
        xc.conn.flush()?;
        let mut e = EditBar {
            xc,
            shell,
            keymap,
            area,
            bottom_h: 0,
            active_atom,
            active: 0,
            depth,
            bar,
            list,
            terminal: false,
            wanted: false,
            bar_pressed: None,
            list_pressed: None,
            entries: Vec::new(),
            list_hits: Vec::new(),
            list_until: None,
        };
        e.read_active();
        Ok(e)
    }

    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }
    pub fn bar_window(&self) -> XWindow {
        self.bar.id
    }
    pub fn list_window(&self) -> XWindow {
        self.list.id
    }
    pub fn bar_visible(&self) -> bool {
        self.bar.mapped
    }
    pub fn list_visible(&self) -> bool {
        self.list.mapped
    }
    /// The shell's own screen is the active window (Home): no toolbar over it.
    pub fn home(&self) -> bool {
        self.active == self.shell && self.active != 0
    }
    pub fn terminal_mode(&self) -> bool {
        self.terminal
    }
    pub fn next_deadline(&self) -> Option<Instant> {
        self.list_until
    }
    pub fn bar_geometry(&self) -> Rect {
        bar_rect(self.area, self.bottom_h, buttons(self.terminal).len())
    }

    pub(super) fn read_active(&mut self) {
        let class = active_class(&self.xc, self.active_atom);
        self.terminal = is_terminal(&class);
        self.active = self
            .xc
            .conn
            .get_property(
                false,
                self.xc.root,
                self.active_atom,
                AtomEnum::WINDOW,
                0,
                1,
            )
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().and_then(|mut v| v.next()))
            .unwrap_or(0);
    }

    /// The toolbar and the list follow `wanted` and the list state; call after any change.
    pub fn set_wanted(&mut self, on: bool, fonts: &mut Fonts) -> Result<(), XError> {
        if on == self.wanted && on == self.bar.mapped {
            return Ok(());
        }
        self.wanted = on;
        self.apply_bar(fonts)
    }

    pub(super) fn apply_bar(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let on = self.wanted;
        let r = self.bar_geometry();
        if on {
            self.bar.canvas = Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("toolbar canvas")?;
            self.xc.conn.configure_window(
                self.bar.id,
                &ConfigureWindowAux::new()
                    .x(r.x)
                    .y(r.y)
                    .width(r.w as u32)
                    .height(r.h as u32),
            )?;
            if !self.bar.mapped {
                self.xc.conn.map_window(self.bar.id)?;
                self.bar.mapped = true;
            }
            self.raise(self.bar.id)?;
            self.redraw_bar(fonts)?;
        } else if self.bar.mapped {
            self.xc.conn.unmap_window(self.bar.id)?;
            self.bar.mapped = false;
            self.bar_pressed = None;
        }
        self.xc.conn.flush()?;
        Ok(())
    }

    pub(super) fn raise(&self, w: XWindow) -> Result<(), XError> {
        self.xc
            .conn
            .configure_window(w, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))?;
        Ok(())
    }

    pub(super) fn redraw_bar(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        Painter {
            cv: &mut self.bar.canvas,
            fonts,
        }
        .edit_bar(buttons(self.terminal), self.bar_pressed);
        crate::present::present(
            &self.xc.conn,
            self.bar.id,
            self.bar.gc,
            self.depth,
            &self.bar.canvas,
            self.bar.canvas.bounds(),
        )
    }

    /// The keyboard / mouse pad changed height: the toolbar and the list move with it.
    pub fn set_bottom(&mut self, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        if h == self.bottom_h {
            return Ok(());
        }
        self.bottom_h = h;
        self.relayout(fonts)
    }

    /// Rotation.
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.relayout(fonts)
    }

    fn relayout(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        if self.bar.mapped {
            self.apply_bar(fonts)?;
        }
        if self.list.mapped {
            self.apply_list(fonts)?;
        }
        Ok(())
    }
}
