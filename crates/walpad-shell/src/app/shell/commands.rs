use super::Shell;
use crate::app::{
    load_catalog, read_clock_24h, read_theme, read_wallpaper, read_wallpaper_path, Cmd, WallTime,
};
use crate::config::read_devd;
use crate::gesture::Gesture;
use crate::media_worker::Job;
use crate::settings_screen::gather;
use std::time::{Duration, Instant};
use walpad_ui::tokens::bg;
use walpad_ui::Canvas;

impl Shell {
    pub(super) fn send(&mut self, cmds: Vec<Cmd>) {
        for c in cmds {
            // the system keyboard is ours: apply locally, then report the state to the core
            let kbd = match c {
                Cmd::KeyboardToggle => Some(!self.keyboard_visible()),
                Cmd::KeyboardShow => Some(true),
                Cmd::KeyboardHide => Some(false),
                _ => None,
            };
            if let Some(on) = kbd {
                self.set_keyboard(on, true);
                continue;
            }
            // so is the mouse pad (no core command)
            match c {
                Cmd::MousePadShow => {
                    self.set_pad(true);
                    continue;
                }
                Cmd::MousePadHide => {
                    self.set_pad(false);
                    continue;
                }
                Cmd::ClipboardShow => {
                    self.show_clipboard_list();
                    continue;
                }
                // the Media screen's background work (no core command)
                Cmd::MediaScan => {
                    self.media.send(Job::Scan);
                    continue;
                }
                Cmd::MediaThumbs(v) => {
                    self.media.send(Job::Thumbs(v));
                    continue;
                }
                Cmd::MediaInfo(v) => {
                    self.media.send(Job::Info(v));
                    continue;
                }
                Cmd::MediaRemember(p) => {
                    self.media.send(Job::Remember(p));
                    continue;
                }
                Cmd::CopyText(t) => {
                    if let Some(k) = &mut self.keeper {
                        if let Err(err) = k.set_text(&t) {
                            eprintln!("walpad-shell: clipboard: {}", err.0);
                        }
                    }
                    continue;
                }
                _ => {}
            }
            let (name, args) = c.request();
            match self.core.send(name, args) {
                Some(id) => {
                    self.app.note_sent(id, &c);
                    if let Cmd::CloseApp(app) = &c {
                        if self.nav.as_ref().is_some_and(|n| n.is_closing(app)) {
                            self.nav_close.push((id, app.clone()));
                        }
                    }
                }
                None => {
                    // not connected: the card comes back at once
                    if let (Cmd::CloseApp(app), Some(n)) = (&c, &mut self.nav) {
                        n.close_result(app, false);
                        self.app.notice("Could not close the app");
                    }
                }
            }
        }
    }

    pub(super) fn apply_gestures(&mut self, gs: Vec<Gesture>) {
        let now = self.now_ms();
        for g in gs {
            let out = self.app.on_gesture(&g, now);
            self.dirty |= out.dirty;
            self.need_refresh |= out.refresh;
            self.send(out.cmds);
        }
    }

    /// Re-read the configuration files and the system facts the Device/Settings screens show.
    pub(super) fn reload(&mut self) {
        self.need_refresh = false;
        let devd = read_devd(&self.root);
        let clock_24h = read_clock_24h(&self.root);
        let theme = read_theme(&self.root);
        if theme != walpad_ui::tokens::theme() {
            walpad_ui::tokens::set_theme(theme);
            self.dirty = true;
        }
        self.dirty |= self.app.set_config(devd, clock_24h);
        self.dirty |= self
            .app
            .set_wallpaper(read_wallpaper(&self.root), read_wallpaper_path(&self.root));
        self.clock_24h = clock_24h;
        let (wall, to_minute) = WallTime::now(clock_24h);
        self.next_minute = Instant::now() + Duration::from_millis(to_minute);
        self.dirty |= self.app.set_wall(wall);
        self.dirty |= self
            .app
            .set_windows(crate::windows_ui::read_prefs(&self.root));
        self.dirty |= self.app.set_sysinfo(gather(&self.root));
        // apps installed or removed in the app store
        self.dirty |= self.app.set_catalog(load_catalog(&self.root));
    }

    pub(super) fn resize(&mut self, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        if let Some(c) = Canvas::new(w as u32, h as u32, bg()) {
            self.canvas = c;
            self.app.resize(w, h);
            self.dirty = true;
            if let Some(mut k) = self.kbd.take() {
                match k.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.kbd = Some(k),
                    Err(e) => eprintln!("walpad-shell: system keyboard disabled: {}", e.0),
                }
            }
            if let Some(mut p) = self.pad.take() {
                match p.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.pad = Some(p),
                    Err(e) => eprintln!("walpad-shell: mouse pad disabled: {}", e.0),
                }
            }
            if let Some(mut e) = self.edit.take() {
                match e.set_shell_size(w, h, &mut self.fonts) {
                    Ok(()) => self.edit = Some(e),
                    Err(err) => eprintln!("walpad-shell: selection toolbar disabled: {}", err.0),
                }
            }
            self.bottom_changed();
            if let Some(l) = self.lock.as_mut() {
                if let Err(e) = l.set_shell_size(w, h, &mut self.fonts) {
                    eprintln!("walpad-shell: lock screen: {}", e.0);
                }
            }
            if let Some(c) = self.call.as_mut() {
                if let Err(e) = c.set_shell_size(w, h, &mut self.fonts) {
                    eprintln!("walpad-shell: call screen: {}", e.0);
                }
            }
            if let Some(d) = &mut self.dock {
                if let Err(e) = d.set_shell_size(w, h) {
                    eprintln!("walpad-shell: system dock: {}", e.0);
                }
            }
            if let Some(b) = &mut self.wmbar {
                if let Err(e) = b.set_shell_size(w, h, &mut self.fonts) {
                    eprintln!("walpad-shell: window bar: {}", e.0);
                }
            }
            if let Some(n) = &mut self.nav {
                if let Err(e) = n.set_shell_size(w, h) {
                    eprintln!("walpad-shell: edge swipe / Recents: {}", e.0);
                }
            }
        }
    }
}
