use super::model::{strut_partial, BAR_H};
use super::render::{render, render_grip};
use super::window::WmBar;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use walpad_proto::wm::WRect;
use walpad_ui::components::Painter;
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts};
use walpad_x::XError;
use x11rb::protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt as _, PropMode};
use x11rb::wrapper::ConnectionExt as _;

impl WmBar {
    pub fn wait_fd(&self) -> &UnixStream {
        &self.xc.wait_fd
    }

    pub fn visible(&self) -> bool {
        self.shown
    }

    pub fn next_deadline(&self) -> Option<Duration> {
        self.app_up
            .then(|| self.next_refresh.saturating_duration_since(Instant::now()))
    }

    /// The shell window changed size (rotation).
    pub fn set_shell_size(&mut self, w: i32, h: i32, fonts: &mut Fonts) -> Result<(), XError> {
        let area = crate::dock::visible_area(w, h);
        if area == self.area {
            return Ok(());
        }
        self.area = area;
        self.model.width = area.0;
        self.canvas = Canvas::new(area.0 as u32, BAR_H as u32, bg()).ok_or("bar canvas")?;
        self.xc.conn.configure_window(
            self.bar,
            &ConfigureWindowAux::new()
                .width(area.0 as u32)
                .height(BAR_H as u32),
        )?;
        self.write_strut()?;
        self.refresh(fonts)
    }

    pub(super) fn write_strut(&self) -> Result<(), XError> {
        let s = strut_partial(self.area.0);
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

    pub(super) fn read_window(&self, w: u32, atom: u32, ty: AtomEnum, len: u32) -> Vec<u32> {
        self.xc
            .conn
            .get_property(false, w, atom, ty, 0, len)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|v| v.collect()))
            .unwrap_or_default()
    }

    pub(super) fn class_of(&self, w: u32) -> Vec<String> {
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

    /// A window's rectangle in root coordinates.
    pub(super) fn geometry(&self, w: u32) -> Option<WRect> {
        let g = self.xc.conn.get_geometry(w).ok()?.reply().ok()?;
        let t = self
            .xc
            .conn
            .translate_coordinates(w, self.xc.root, 0, 0)
            .ok()?
            .reply()
            .ok()?;
        Some(WRect::new(
            t.dst_x as i32,
            t.dst_y as i32,
            g.width as i32,
            g.height as i32,
        ))
    }

    pub(super) fn redraw(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render(
            &mut Painter {
                cv: &mut self.canvas,
                fonts,
            },
            &self.model,
            self.pressed,
        );
        crate::present::present(
            &self.xc.conn,
            self.bar,
            self.gc,
            self.depth,
            &self.canvas,
            self.canvas.bounds(),
        )
    }

    pub(super) fn redraw_grip(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        render_grip(
            &mut Painter {
                cv: &mut self.grip_canvas,
                fonts,
            },
            self.grip_pressed,
        );
        crate::present::present(
            &self.xc.conn,
            self.grip,
            self.grip_gc,
            self.depth,
            &self.grip_canvas,
            self.grip_canvas.bounds(),
        )
    }
}
