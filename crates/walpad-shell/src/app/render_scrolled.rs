//! Scrolled screen rendering and settings row assembly for `App`.

use crate::bluetooth;
use crate::device_screen;
use crate::gps_screen;
use crate::media;
use crate::mobile;
use crate::network;
use crate::notifications_ui;
use crate::phone_ui;
use crate::settings_screen::{self, Page};
use crate::store;
use crate::volume_ui;
use crate::vpn_screen;
use walpad_ui::components::{Painter, Tab};
use walpad_ui::tokens::{accent, divider};
use walpad_ui::Rect;

use super::App;

impl App {
    /// Rows of the open Settings subpage (empty for the category list).
    pub(super) fn settings_rows(&self) -> Vec<settings_screen::Row> {
        match self.model.settings_page {
            None => Vec::new(),
            Some(Page::Sound) => volume_ui::sound_rows(&self.model),
            Some(pg) => {
                let mut rows = settings_screen::page_rows(
                    pg,
                    &settings_screen::Ctx {
                        sys: &self.model.sys,
                        status: &self.model.status,
                        devd: &self.model.devd,
                        clock_24h: self.model.clock_24h,
                    },
                );
                if pg == Page::Input {
                    rows.extend(crate::windows_ui::rows(&self.model.windows));
                }
                rows
            }
        }
    }

    /// Scrolled screens: the whole layout is walked in content coordinates with the canvas origin
    /// at (0, view.y - scroll); primitives skip what falls outside the frame. No off-screen copy.
    pub(super) fn render_scrolled(&mut self, p: &mut Painter, tab_dy: i32) {
        let frame = self.frame;
        let media_list = if self.active == Tab::Media {
            let list = self.model.media.list();
            let lay = media::layout(&frame, self.model.media.section, list.len());
            if let Some((a, b)) = media::visible_range(&lay, self.scroll, self.view.h) {
                self.model.media.note_visible(&list, a, b);
            }
            list
        } else {
            Vec::new()
        };
        let total = match self.active {
            Tab::Media => media::layout(&frame, self.model.media.section, media_list.len()).total,
            Tab::Vpn => {
                vpn_screen::layout(&frame, &self.model.status, self.model.vpn_files.len()).total
            }
            Tab::Network => network::layout(&frame, &self.model.status, &self.model.net).total,
            Tab::Device => device_screen::layout(&frame, &self.model.status, &self.model.devd).1,
            Tab::Settings if self.model.settings_page == Some(Page::Apps) => {
                store::layout(&frame, &self.model.store).total
            }
            Tab::Settings => {
                settings_screen::layout(&frame, self.model.settings_page, &self.settings_rows())
                    .total
            }
            Tab::Mobile => mobile::layout(&frame, &self.model.status).total,
            Tab::Bluetooth => bluetooth::layout(&frame, &self.model.status).total,
            Tab::Phone => phone_ui::layout_total(&frame, &self.model.phone, &self.model.status),
            Tab::Gps => gps_screen::layout(&frame, &self.model.status).total,
            Tab::Notifications => {
                notifications_ui::layout(&frame, self.model.notifications.len()).total
            }
            _ => 0,
        }
        .max(self.view.h);
        self.content_h = total;
        self.scroll = self.scroll.clamp(0, (total - self.view.h).max(0));
        p.cv.set_origin(0, self.view.y - self.scroll + tab_dy);
        let m = &self.model;
        self.content_hits = match self.active {
            Tab::Network => network::render(
                p,
                &frame,
                &m.status,
                &m.net,
                m.connected,
                m.pressed,
                m.spinner_deg,
            ),
            Tab::Device => device_screen::render(
                p,
                &frame,
                &m.status,
                &m.devd,
                m.connected,
                m.brightness_value(),
                m.slider_active,
                m.wallpaper,
                &m.wallpaper_path,
                m.pressed,
            ),
            Tab::Settings if m.settings_page == Some(Page::Apps) => {
                store::render(p, &frame, &m.store, m.connected, m.pressed)
            }
            Tab::Settings => {
                let rows = self.settings_rows();
                settings_screen::render(p, &frame, m.settings_page, &rows, m.connected, m.pressed)
            }
            Tab::Mobile => mobile::render(
                p,
                &frame,
                &mobile::View {
                    st: &m.status,
                    info: &m.mobile_info,
                    imei_shown: m.imei_shown,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Bluetooth => bluetooth::render(
                p,
                &frame,
                &bluetooth::View {
                    st: &m.status,
                    online: m.connected,
                    pressed: m.pressed,
                    spinner_deg: m.spinner_deg,
                    busy: m.bt_busy.as_deref(),
                },
            ),
            Tab::Vpn => vpn_screen::render(
                p,
                &frame,
                &vpn_screen::View {
                    st: &m.status,
                    files: &m.vpn_files,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Media => media::render(
                p,
                &frame,
                &media::View {
                    m: &m.media,
                    list: &media_list,
                    st: &m.status,
                    pressed: m.pressed,
                    spinner_deg: m.spinner_deg,
                },
            ),
            Tab::Phone => phone_ui::render(
                p,
                &frame,
                &phone_ui::View {
                    st: &m.status,
                    m: &m.phone,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Gps => gps_screen::render(
                p,
                &frame,
                &gps_screen::View {
                    st: &m.status,
                    online: m.connected,
                    pressed: m.pressed,
                },
            ),
            Tab::Notifications => notifications_ui::render(
                p,
                &frame,
                &notifications_ui::View {
                    items: &m.notifications,
                    pressed: m.pressed,
                },
            ),
            Tab::Home | Tab::Apps => Vec::new(),
        };
        p.cv.set_origin(0, 0);
        if self.content_h > self.view.h && self.view.h > 0 {
            let track = Rect::new(self.view.right() - 3, self.view.y, 2, self.view.h);
            p.cv.rect(track, divider());
            let thumb_h = ((self.view.h as i64 * self.view.h as i64) / self.content_h as i64)
                .clamp(24, self.view.h as i64) as i32;
            let max_scroll = (self.content_h - self.view.h).max(1);
            let thumb_y = self.view.y
                + ((self.scroll as i64 * (self.view.h - thumb_h) as i64) / max_scroll as i64)
                    as i32;
            p.cv.rect(Rect::new(track.x, thumb_y, 2, thumb_h), accent());
        }
    }
}
