use super::model::{
    app_for, grip_rect, tab_label, tab_width, wants_visible, BarModel, Tab, BAR_H, GRIP, REFRESH_MS,
};
use super::window::WmBar;
use crate::windows_ui::read_prefs;
use std::time::{Duration, Instant};
use walpad_proto::wm::LayoutMode;
use walpad_ui::Fonts;
use walpad_x::XError;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConfigureWindowAux, ConnectionExt as _, StackMode};

impl WmBar {
    /// Look at the window in front and put the bar, the tabs and the grip where they belong.
    pub fn refresh(&mut self, fonts: &mut Fonts) -> Result<(), XError> {
        self.next_refresh = Instant::now() + Duration::from_millis(REFRESH_MS);
        if self.drag.is_some() {
            return Ok(()); // the window is being moved: leave the grip alone
        }
        self.prefs = read_prefs(&self.root);
        let active = self
            .read_window(self.xc.root, self.atoms.active, AtomEnum::WINDOW, 1)
            .first()
            .copied()
            .unwrap_or(0);
        let mine = [self.bar, self.grip, self.strut_win];
        let class = if active == 0 || mine.contains(&active) {
            Vec::new()
        } else {
            self.class_of(active)
        };
        // the shell's own windows (WM_CLASS walpad*) are no app windows
        let is_shell = class
            .iter()
            .any(|c| c.to_ascii_lowercase().starts_with("walpad"));
        let app = if is_shell {
            None
        } else {
            app_for(&self.apps, &class).cloned()
        };
        // a tap on the bar or the grip changes nothing about what is in front
        if mine.contains(&active) && self.shown {
            return Ok(());
        }
        self.app_up = !class.is_empty() && !is_shell;
        let on = wants_visible(app.as_ref(), self.prefs.mode);
        let mut model = BarModel {
            width: self.area.0,
            tabs: Vec::new(),
            active,
            title: String::new(),
            can_new: false,
            free: self.prefs.mode == LayoutMode::Free,
        };
        if let Some(a) = &app {
            self.app_id = a.id.clone();
            model.title = a.name.clone();
            if a.max > 1 {
                let wins =
                    self.read_window(self.xc.root, self.atoms.client_list, AtomEnum::WINDOW, 4096);
                let mut same: Vec<u32> = wins
                    .into_iter()
                    .filter(|w| {
                        self.class_of(*w)
                            .iter()
                            .any(|c| c.eq_ignore_ascii_case(&a.class))
                    })
                    .collect();
                same.sort_unstable();
                let tw = tab_width(&BarModel {
                    tabs: same
                        .iter()
                        .map(|w| Tab {
                            win: *w,
                            label: String::new(),
                        })
                        .collect(),
                    can_new: true,
                    ..model.clone()
                });
                model.tabs = same
                    .iter()
                    .enumerate()
                    .map(|(i, w)| Tab {
                        win: *w,
                        label: tab_label(&a.name, i + 1, tw),
                    })
                    .collect();
                model.can_new = model.tabs.len() < a.max as usize;
            }
        }
        if !on {
            self.set_shown(false)?;
            self.model = model;
            return Ok(());
        }
        let changed = model != self.model || !self.shown;
        self.model = model;
        self.set_shown(true)?;
        if changed {
            self.redraw(fonts)?;
        }
        self.place_grip(fonts, active)?;
        self.raise()?;
        self.xc.conn.flush()?;
        Ok(())
    }

    fn set_shown(&mut self, on: bool) -> Result<(), XError> {
        if on == self.shown {
            return Ok(());
        }
        self.shown = on;
        let c = &self.xc.conn;
        if on {
            self.write_strut()?;
            c.map_window(self.strut_win)?;
            c.map_window(self.bar)?;
            c.configure_window(
                self.bar,
                &ConfigureWindowAux::new()
                    .x(0)
                    .y(0)
                    .width(self.area.0 as u32)
                    .height(BAR_H as u32)
                    .stack_mode(StackMode::ABOVE),
            )?;
        } else {
            c.unmap_window(self.bar)?;
            c.unmap_window(self.strut_win)?;
            c.unmap_window(self.grip)?;
            self.grip_shown = false;
            self.pressed = None;
        }
        c.flush()?;
        Ok(())
    }

    fn raise(&self) -> Result<(), XError> {
        let above = ConfigureWindowAux::new().stack_mode(StackMode::ABOVE);
        self.xc.conn.configure_window(self.bar, &above)?;
        if self.grip_shown {
            self.xc.conn.configure_window(self.grip, &above)?;
        }
        Ok(())
    }

    fn place_grip(&mut self, fonts: &mut Fonts, active: u32) -> Result<(), XError> {
        let want = self.model.free && self.shown && active != 0;
        let rect = if want { self.geometry(active) } else { None };
        match rect {
            Some(r) => {
                let g = grip_rect(r, self.area);
                self.xc.conn.configure_window(
                    self.grip,
                    &ConfigureWindowAux::new()
                        .x(g.x)
                        .y(g.y)
                        .width(GRIP as u32)
                        .height(GRIP as u32)
                        .stack_mode(StackMode::ABOVE),
                )?;
                if !self.grip_shown {
                    self.grip_shown = true;
                    self.xc.conn.map_window(self.grip)?;
                    self.redraw_grip(fonts)?;
                }
            }
            None => {
                if self.grip_shown {
                    self.grip_shown = false;
                    self.xc.conn.unmap_window(self.grip)?;
                }
            }
        }
        Ok(())
    }
}
