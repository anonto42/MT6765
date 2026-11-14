use super::model::{active_class, is_terminal, list_rect, paste_stroke};
use super::windows::EditBar;
use crate::keyboard::{inject, XK_CONTROL_L};
use std::time::{Duration, Instant};
use walpad_ui::components::Painter;
use walpad_ui::edit_bar::{CLIP_HEAD_H, CLIP_ROW_H};
use walpad_ui::tokens::bg;
use walpad_ui::{Canvas, Fonts, Rect};
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConfigureWindowAux, ConnectionExt as _, Window as XWindow};

impl EditBar {
    // ---- clipboard list
    /// Show the list with these entries (newest first).
    pub fn show_list(&mut self, entries: &[String], fonts: &mut Fonts) -> Result<(), XError> {
        self.entries = entries.to_vec();
        self.list_until = Some(Instant::now() + Duration::from_millis(super::LIST_MS));
        self.list_pressed = None;
        self.apply_list(fonts)
    }

    /// The history changed while the list is up.
    pub fn update_list(&mut self, entries: &[String], fonts: &mut Fonts) -> Result<(), XError> {
        if self.list.mapped {
            self.entries = entries.to_vec();
            self.apply_list(fonts)?;
        }
        Ok(())
    }

    pub fn hide_list(&mut self) -> Result<(), XError> {
        self.list_until = None;
        if self.list.mapped {
            self.xc.conn.unmap_window(self.list.id)?;
            self.list.mapped = false;
            self.list_pressed = None;
            self.xc.conn.flush()?;
        }
        Ok(())
    }

    pub(super) fn apply_list(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        let r = list_rect(self.area, self.bottom_h, self.entries.len());
        let shown = self.shown_entries(r);
        self.list.canvas = Canvas::new(r.w as u32, r.h as u32, bg()).ok_or("list canvas")?;
        self.xc.conn.configure_window(
            self.list.id,
            &ConfigureWindowAux::new()
                .x(r.x)
                .y(r.y)
                .width(r.w as u32)
                .height(r.h as u32),
        )?;
        if !self.list.mapped {
            self.xc.conn.map_window(self.list.id)?;
            self.list.mapped = true;
        }
        self.raise(self.list.id)?;
        self.redraw_list(fonts, shown)?;
        self.xc.conn.flush()?;
        Ok(())
    }

    /// How many entries the window of height `r.h` shows.
    pub(super) fn shown_entries(&self, r: Rect) -> usize {
        if self.entries.is_empty() {
            0
        } else {
            (((r.h - CLIP_HEAD_H) / CLIP_ROW_H) as usize).min(self.entries.len())
        }
    }

    pub(super) fn redraw_list(&mut self, fonts: &mut Fonts, shown: usize) -> Result<(), XError> {
        let entries = &self.entries[..shown];
        self.list_hits = Painter {
            cv: &mut self.list.canvas,
            fonts,
        }
        .clipboard_list(entries, self.list_pressed);
        crate::present::present(
            &self.xc.conn,
            self.list.id,
            self.list.gc,
            self.depth,
            &self.list.canvas,
            self.list.canvas.bounds(),
        )
    }

    /// Type the paste shortcut of the app in front (after the shell made the entry the clipboard).
    pub fn send_paste(&self) -> Result<(), XError> {
        let class = active_class(&self.xc, self.active_atom);
        inject(&self.xc, &self.keymap, paste_stroke(is_terminal(&class)))
    }

    /// For tests and diagnostics: is the window mapped according to the server?
    pub fn server_mapped(&self, w: XWindow) -> bool {
        self.xc
            .conn
            .get_window_attributes(w)
            .ok()
            .and_then(|c| c.reply().ok())
            .is_some_and(|a| a.map_state != xproto::MapState::UNMAPPED)
    }

    /// The Control_L keysym is part of every edit shortcut: false when the server has no keycode
    /// for it (the toolbar then cannot type).
    pub fn can_type(&self) -> bool {
        self.keymap.lookup(XK_CONTROL_L).is_some()
    }
}
