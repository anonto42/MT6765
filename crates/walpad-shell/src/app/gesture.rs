//! Hit testing, touch gestures, sliders, media actions, and app activation for `App`.

use crate::gesture::Gesture;
use crate::media::{self, MediaTarget};
use crate::overlay::{AppSheet, Overlay};
use crate::volume_ui;
use walpad_ui::components::{Tab, Tone};
use walpad_ui::tokens::SP_4;
use walpad_ui::Rect;

use super::cmd::{Cmd, Outcome};
use super::model::{
    native_brightness, percent, Ms, ANIM_STEP_MS, OVERLAY_ANIM_MS, SLIDER_SEND_EVERY_MS,
    TAB_ANIM_MS,
};
use super::target::Target;
use super::App;

impl App {
    pub(super) fn hit(&self, x: i32, y: i32) -> Option<Target> {
        if self.model.overlay.is_some() {
            return self
                .overlay_hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, y))
                .map(|(_, t)| *t);
        }
        if self.scrolls() && self.view.contains(x, y) {
            let cy = y - self.view.y + self.scroll;
            if let Some((_, t)) = self
                .content_hits
                .iter()
                .rev()
                .find(|(r, _)| r.contains(x, cy))
            {
                return Some(*t);
            }
        }
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, t)| *t)
    }

    /// The brightness slider's rectangle (Quick Settings: overlay hits; Device: content hits). x is
    /// never scrolled.
    pub(super) fn slider_rect(&self) -> Option<Rect> {
        self.overlay_hits
            .iter()
            .chain(self.hits.iter())
            .chain(self.content_hits.iter())
            .find(|(_, t)| *t == self.model.slider_target)
            .map(|(r, _)| *r)
    }

    pub(super) fn slider_value_at(&self, x: i32) -> f32 {
        let track = self
            .slider_rect()
            .map(|r| r.inset(SP_4))
            .unwrap_or(Rect::new(0, 0, 1, 1));
        ((x - track.x) as f32 / track.w.max(1) as f32).clamp(0.0, 1.0)
    }

    pub fn on_gesture(&mut self, g: &Gesture, now: Ms) -> Outcome {
        self.now = now;
        let prev_active = self.active;
        let prev_overlay = self.model.overlay.is_some();
        let mut out = Outcome::default();
        match *g {
            Gesture::Pressed { x, y } => {
                self.scroll_vel = 0.0;
                let t = self.hit(x, y);
                self.model.pressed = t;
                if let (Some(Target::Key(i)), Some(Overlay::Password(s))) =
                    (t, &mut self.model.overlay)
                {
                    s.pressed_key = Some(i);
                }
                let slider_ok = match t {
                    Some(Target::Slider) => self.model.connected,
                    Some(Target::VolumeSlider) => volume_ui::usable(&self.model),
                    _ => false,
                };
                if slider_ok {
                    self.model.slider_target = t.unwrap_or(Target::Slider);
                    self.model.slider_active = true;
                    self.model.slider_value = self.slider_value_at(x);
                }
                out.dirty = t.is_some();
            }
            Gesture::Cancel => {
                out.dirty = self.model.pressed.take().is_some();
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    out.dirty |= s.pressed_key.take().is_some();
                }
            }
            Gesture::Tap { x, y } => {
                let t = self.hit(x, y);
                self.model.pressed = None;
                if let Some(Overlay::Password(s)) = &mut self.model.overlay {
                    s.pressed_key = None;
                }
                let was_slider = std::mem::take(&mut self.model.slider_active);
                out.dirty = true;
                match t {
                    Some(t) => self.on_tap(t, x, was_slider, &mut out),
                    None => out.dirty = was_slider || out.dirty,
                }
            }
            Gesture::LongPress { x, y } => {
                self.model.slider_active = false;
                out.dirty = self.model.pressed.take().is_some();
                let panel = matches!(self.model.overlay, Some(Overlay::AppsPanel));
                if self.model.overlay.is_none() || panel {
                    let id = match self.hit(x, y) {
                        Some(Target::Tile(i)) => self.model.tiles.get(i).map(|t| t.id.clone()),
                        Some(Target::AppTile(i) | Target::PanelActive(i)) => {
                            self.model.apps.get(i).map(|t| t.id.clone())
                        }
                        _ => None,
                    };
                    if let Some(id) = id {
                        if let Some(t) = self.model.apps.iter().find(|t| t.id == id) {
                            let sheet = AppSheet {
                                id: id.clone(),
                                name: t.name.clone(),
                                running: t.running,
                                pinned: self.model.pinned.contains(&id),
                            };
                            self.model.overlay = Some(Overlay::App(sheet));
                            out.dirty = true;
                        }
                    }
                }
            }
            Gesture::DragStart { .. } => {
                self.scroll_vel = 0.0;
                self.scroll_drag_ms = now;
            }
            Gesture::Drag { dx, dy } => {
                if self.model.slider_active {
                    let w = self.slider_rect().map_or(1, |r| r.inset(SP_4).w.max(1));
                    self.model.slider_value =
                        (self.model.slider_value + dx as f32 / w as f32).clamp(0.0, 1.0);
                    out.dirty = true;
                    if now.saturating_sub(self.last_slider_send) >= SLIDER_SEND_EVERY_MS {
                        self.last_slider_send = now;
                        out.cmd(self.slider_cmd());
                    }
                } else if self.scrolls() && self.model.overlay.is_none() {
                    let dt = now.saturating_sub(self.scroll_drag_ms).clamp(8, 64) as f32;
                    let inst = (dy as f32 / dt) * ANIM_STEP_MS as f32;
                    self.scroll_vel = self.scroll_vel * 0.45 + inst * 0.55;
                    self.scroll_drag_ms = now;
                    let max = (self.content_h - self.view.h).max(0);
                    let s = (self.scroll - dy).clamp(0, max);
                    if s != self.scroll {
                        self.scroll = s;
                        out.dirty = true;
                    }
                }
            }
            Gesture::DragEnd => {
                if std::mem::take(&mut self.model.slider_active) {
                    out.cmd(self.slider_cmd());
                    out.dirty = true;
                } else if self.scrolls()
                    && self.model.overlay.is_none()
                    && self.scroll_vel.abs() >= 1.5
                    && now.saturating_sub(self.scroll_drag_ms) <= 80
                {
                    self.scroll_fling_at = now.saturating_add(ANIM_STEP_MS);
                } else {
                    self.scroll_vel = 0.0;
                }
            }
        }
        if self.animate_transitions {
            if self.active != prev_active {
                self.tab_anim_start = now;
                self.tab_anim_until = now.saturating_add(TAB_ANIM_MS);
                self.anim_next = now.saturating_add(ANIM_STEP_MS);
            }
            if !prev_overlay && self.model.overlay.is_some() {
                self.overlay_anim_start = now;
                self.overlay_anim_until = now.saturating_add(OVERLAY_ANIM_MS);
                self.anim_next = now.saturating_add(ANIM_STEP_MS);
            }
        }
        out
    }

    /// The command for the slider being dragged (its value is in `slider_value`).
    pub(super) fn slider_cmd(&self) -> Cmd {
        match self.model.slider_target {
            Target::VolumeSlider => Cmd::SetVolume(percent(self.model.slider_value)),
            _ => Cmd::SetBrightness(native_brightness(self.model.slider_value)),
        }
    }

    pub(super) fn on_media_tap(&mut self, t: MediaTarget, out: &mut Outcome) {
        match t {
            MediaTarget::Section(i) => {
                if let Some(s) = media::Section::ALL.get(i) {
                    self.model.media.section = *s;
                    self.scroll = 0;
                    self.content_hits.clear();
                }
            }
            MediaTarget::Refresh => {
                self.model.media.scanning = true;
                out.cmd(Cmd::MediaScan);
            }
            MediaTarget::Item(i) => {
                let list = self.model.media.list();
                let Some(it) = list.get(i) else { return };
                match media::player_for(it.kind, &self.model.status) {
                    Some(app) if self.model.connected => {
                        out.cmd(Cmd::LaunchFile {
                            app: app.to_string(),
                            path: it.path.clone(),
                        });
                        out.cmd(Cmd::MediaRemember(it.path.clone()));
                    }
                    Some(_) => self.toast("Not connected to the system service", Tone::Warning),
                    None => self.toast(it.kind.missing_hint(), Tone::Warning),
                }
            }
        }
    }

    /// An answer from the Media background thread; true when the screen needs a redraw.
    pub fn on_media(&mut self, ev: crate::media_worker::Event) -> bool {
        use crate::media_worker::Event;
        match ev {
            Event::Scanned(items) => self.model.media.set_items(items),
            Event::Recent(r) => self.model.media.set_recent(r),
            Event::Thumb(p, png) => self.model.media.on_thumb(&p, png.as_deref()),
            Event::Info(p, i) => self.model.media.on_info(&p, i),
            Event::Dropped(p) => self.model.media.forget(&p),
        }
        self.active == Tab::Media
    }

    pub(super) fn activate(&mut self, id: &str, out: &mut Outcome) {
        if id == "settings" {
            self.go(Tab::Settings, out);
        } else if id == "media" {
            self.go(Tab::Media, out);
        } else if id == "phone" {
            self.go(Tab::Phone, out);
        } else if self.model.connected {
            out.cmd(Cmd::Launch(id.to_string()));
        }
    }
}
