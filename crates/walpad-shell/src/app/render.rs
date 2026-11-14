//! Top-level frame rendering and overlay dispatch for `App`.

use crate::bluetooth;
use crate::chrome::{self, ChromeKey};
use crate::home;
use crate::overlay::{self, Overlay};
use crate::panels;
use crate::phone_ui::{self, PhoneTarget};
use crate::volume_ui;
use walpad_ui::components::Painter;
use walpad_ui::tokens::{SP_1, SP_4};
use walpad_ui::Rect;

use super::model::ease_out_offset;
use super::target::Target;
use super::App;

impl App {
    /// One frame: cached background (memcpy), pressed sidebar button, the screen, header, banner,
    /// overlays, toast. Scrolled screens draw straight into the frame through the canvas origin.
    pub fn render(&mut self, p: &mut Painter) {
        let frame = self.frame;
        let panel_open = matches!(self.model.overlay, Some(Overlay::AppsPanel));
        let sidebar =
            chrome::sidebar_layout(&frame, &chrome::pinned_items(&self.model), panel_open);
        let key = ChromeKey {
            w: frame.w,
            h: frame.h,
            sidebar: sidebar.clone(),
            active: chrome::active_target(self.active, panel_open),
            page: self.scrolls(),
            theme: walpad_ui::tokens::theme(),
            wallpaper: self.model.wallpaper,
            wallpaper_path: self.model.wallpaper_path.clone(),
        };
        self.chrome.paint(p, &frame, key, self.model.live_phase_ms);
        self.chrome.render_pressed(p, &sidebar, self.model.pressed);
        self.hits.clear();
        self.content_hits.clear();
        for it in &sidebar.items {
            self.hits.push((it.hit, it.target));
        }

        let banner = self.banner();
        let banner_h = self.banner_h();
        self.view = Rect::new(
            frame.content.x,
            frame.content.y + banner_h,
            frame.content.w,
            frame.content.h - banner_h,
        );
        let tab_dy = ease_out_offset(self.now, self.tab_anim_start, self.tab_anim_until, 28);
        if self.scrolls() {
            self.render_scrolled(p, tab_dy);
            // scrolled rows may have run up under the header and banner: restore that strip
            self.chrome.restore(
                p.cv,
                Rect::new(frame.header.x, 0, frame.header.w, frame.header.h + banner_h),
            );
        } else {
            p.cv.set_origin(0, tab_dy);
            let home = home::render(p, &frame, &self.model, banner_h);
            p.cv.set_origin(0, 0);
            self.hits.extend(home);
        }
        let qs_open = matches!(self.model.overlay, Some(Overlay::QuickSettings));
        let (pill, bell) = chrome::render_header(p, &frame, &self.model, qs_open);
        self.hits.push((pill, Target::QuickSettings));
        self.hits.push((bell, Target::NotificationsCard));
        if let Some((text, tone)) = banner {
            p.banner(
                Rect::new(
                    frame.content.x + SP_4,
                    frame.content.y + SP_1,
                    frame.content.w - 2 * SP_4,
                    48,
                ),
                text,
                tone,
            );
        }

        let ov_off = ease_out_offset(
            self.now,
            self.overlay_anim_start,
            self.overlay_anim_until,
            28,
        );
        if ov_off != 0 {
            match &self.model.overlay {
                Some(Overlay::QuickSettings) => p.cv.set_origin(0, -ov_off),
                Some(Overlay::AppsPanel) => p.cv.set_origin(-ov_off, 0),
                Some(_) => p.cv.set_origin(0, ov_off),
                None => {}
            }
        }
        let deg = self.model.spinner_deg;
        self.overlay_hits = match &self.model.overlay {
            Some(Overlay::Password(s)) => overlay::render_password(p, &frame, s, deg),
            Some(Overlay::App(s)) => overlay::render_app_sheet(p, &frame, s),
            Some(Overlay::Forget { ssid, .. }) => overlay::render_forget(p, &frame, ssid),
            Some(Overlay::BtPair { name, state, .. }) => {
                bluetooth::render_pair(p, &frame, name, state, deg)
            }
            Some(Overlay::BtForget { name, .. }) => bluetooth::render_forget(p, &frame, name),
            Some(Overlay::PowerConfirm {
                poweroff,
                armed_until,
            }) => overlay::render_power_confirm(p, &frame, *poweroff, armed_until.is_some()),
            Some(Overlay::PowerMenu { focus, armed, .. }) => {
                overlay::render_power_menu(p, &frame, *focus, armed.map(|(i, _)| i))
            }
            Some(Overlay::QuickSettings) => panels::render_quick_settings(p, &frame, &self.model),
            Some(Overlay::AppsPanel) => panels::render_apps_panel(p, &frame, &self.model),
            Some(Overlay::Phone(s)) => {
                let mut hits = vec![(frame.screen(), Target::Phone(PhoneTarget::SheetClose))];
                hits.extend(phone_ui::render_sheet(p, &frame, s, &self.model.phone));
                hits
            }
            None => Vec::new(),
        };
        p.cv.set_origin(0, 0);
        if let Some(t) = &self.model.toast {
            p.toast(frame.content, &t.text, t.tone);
        }
        if let Some(o) = &self.model.osd {
            volume_ui::render_osd(p, frame.content, o);
        }
    }
}
