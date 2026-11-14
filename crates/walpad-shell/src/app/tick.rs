//! Timer ticks, animations, fling physics, and layout/banner state for `App`.

use crate::overlay::Overlay;
use walpad_ui::components::{Tab, Tone};

use super::cmd::Cmd;
use super::model::{Ms, ANIM_STEP_MS, SPIN_EVERY_MS};
use super::App;

impl App {
    /// Advance toast expiry and the spinner. Returns true when a redraw is needed.
    pub fn tick(&mut self, now: Ms) -> bool {
        self.now = now;
        let mut dirty = false;
        if self.model.toast.as_ref().is_some_and(|t| now >= t.until) {
            self.model.toast = None;
            dirty = true;
        }
        if self.model.osd.is_some_and(|o| now >= o.until) {
            self.model.osd = None;
            dirty = true;
        }
        if self.armed_until().is_some_and(|t| now >= t) {
            match &mut self.model.overlay {
                Some(Overlay::PowerConfirm { armed_until, .. }) => *armed_until = None,
                Some(Overlay::PowerMenu { armed, .. }) => *armed = None,
                _ => {}
            }
            dirty = true;
        }
        if self.model.screen_off_armed_until.is_some_and(|t| now >= t) {
            self.model.screen_off_armed_until = None;
            dirty = true;
        }
        if matches!(&self.model.overlay, Some(Overlay::PowerMenu { until, .. }) if now >= *until) {
            self.close_power_menu();
            dirty = true;
        }
        if self.model.wallpaper.is_live() && !self.scrolls() && now >= self.model.next_live_tick {
            self.model.live_phase_ms = now;
            self.model.next_live_tick = now.saturating_add(100);
            dirty = true;
        }
        if !self.model.media.want_thumbs.is_empty() {
            let v = std::mem::take(&mut self.model.media.want_thumbs);
            self.queued.push(Cmd::MediaThumbs(v));
        }
        if !self.model.media.want_info.is_empty() {
            let v = std::mem::take(&mut self.model.media.want_info);
            self.queued.push(Cmd::MediaInfo(v));
        }
        if self.model.spinning() && now.saturating_sub(self.last_spin) >= SPIN_EVERY_MS {
            self.last_spin = now;
            self.model.spinner_deg = (self.model.spinner_deg + 30) % 360;
            dirty = true;
        }
        if self.scroll_vel.abs() >= 0.5 && now >= self.scroll_fling_at {
            if self.scrolls() && self.model.overlay.is_none() {
                let max = (self.content_h - self.view.h).max(0);
                let step = self.scroll_vel.round() as i32;
                let s = (self.scroll - step).clamp(0, max);
                if s != self.scroll {
                    self.scroll = s;
                    dirty = true;
                }
                if s == 0 || s == max {
                    self.scroll_vel = 0.0;
                } else {
                    self.scroll_vel *= 0.88;
                    self.scroll_fling_at = now.saturating_add(ANIM_STEP_MS);
                }
                if self.scroll_vel.abs() < 0.5 {
                    self.scroll_vel = 0.0;
                }
            } else {
                self.scroll_vel = 0.0;
            }
        }
        let anim_active = self.tab_anim_until > 0 || self.overlay_anim_until > 0;
        if anim_active && now >= self.anim_next {
            dirty = true;
            if now >= self.tab_anim_until {
                self.tab_anim_until = 0;
            }
            if now >= self.overlay_anim_until {
                self.overlay_anim_until = 0;
            }
            if self.tab_anim_until > 0 || self.overlay_anim_until > 0 {
                self.anim_next = now.saturating_add(ANIM_STEP_MS);
            } else {
                self.anim_next = 0;
            }
        }
        dirty
    }

    /// Absolute time at which `tick` next has something to do.
    pub fn next_deadline(&self) -> Option<Ms> {
        let spin = self
            .model
            .spinning()
            .then_some(self.last_spin + SPIN_EVERY_MS);
        let toast = self.model.toast.as_ref().map(|t| t.until);
        let menu = match &self.model.overlay {
            Some(Overlay::PowerMenu { until, .. }) => Some(*until),
            _ => None,
        };
        let osd = self.model.osd.map(|o| o.until);
        let live = (self.model.wallpaper.is_live() && !self.scrolls())
            .then_some(self.model.next_live_tick);
        let fling = (self.scroll_vel.abs() >= 0.5).then_some(self.scroll_fling_at);
        let anim =
            (self.tab_anim_until > 0 || self.overlay_anim_until > 0).then_some(self.anim_next);
        [
            spin,
            toast,
            osd,
            self.armed_until(),
            self.model.screen_off_armed_until,
            menu,
            live,
            fling,
            anim,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn armed_until(&self) -> Option<Ms> {
        self.model.armed_until()
    }

    /// Persistent banner under the status bar. Low battery is a toast now (owner request
    /// 2026-11-12), not a block here - only the core being unreachable still blocks the screen,
    /// since nothing else in the shell can be trusted while that is true.
    pub fn banner(&self) -> Option<(&'static str, Tone)> {
        (!self.model.connected).then_some(("System service not responding", Tone::Danger))
    }

    pub(super) fn banner_h(&self) -> i32 {
        if self.banner().is_some() {
            56
        } else {
            0
        }
    }

    pub(super) fn scrolls(&self) -> bool {
        matches!(
            self.active,
            Tab::Network
                | Tab::Device
                | Tab::Settings
                | Tab::Mobile
                | Tab::Bluetooth
                | Tab::Vpn
                | Tab::Media
                | Tab::Phone
                | Tab::Gps
                | Tab::Notifications
        )
    }
}
