//! `App` construction, configuration setters, tile list rebuilding, and navigation.

use crate::config::DevdSettings;
use crate::overlay::Overlay;
use crate::settings_screen::SysInfo;
use crate::store::StoreView;
use std::collections::HashMap;
use walpad_ui::components::{Tab, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::Icon;

use super::chrome::Chrome;
use super::cmd::{Cmd, Outcome};
use super::config::{AppEntry, WallTime};
use super::model::{Model, Toast, TOAST_MS};
use super::target::Tile;
use super::App;

impl App {
    pub fn new(w: i32, h: i32, catalog: Vec<AppEntry>, wall: WallTime) -> App {
        let frame = Frame::new(w, h);
        let mut a = App {
            frame,
            chrome: Chrome::default(),
            active: Tab::Home,
            model: Model::blank(wall),
            catalog,
            hits: Vec::new(),
            content_hits: Vec::new(),
            overlay_hits: Vec::new(),
            scroll: 0,
            scroll_vel: 0.0,
            scroll_drag_ms: 0,
            scroll_fling_at: 0,
            content_h: 0,
            view: frame.content,
            last_slider_send: 0,
            last_spin: 0,
            animate_transitions: false,
            tab_anim_start: 0,
            tab_anim_until: 0,
            overlay_anim_start: 0,
            overlay_anim_until: 0,
            anim_next: 0,
            pending: HashMap::new(),
            now: 0,
            queued: Vec::new(),
        };
        a.rebuild_tiles();
        a
    }

    /// The app definitions changed (an app was installed or removed): rebuild the tiles.
    pub fn set_catalog(&mut self, catalog: Vec<AppEntry>) -> bool {
        let changed = self.catalog != catalog;
        self.catalog = catalog;
        self.rebuild_tiles();
        changed
    }

    pub fn with_pinned(mut self, pinned: Vec<String>) -> App {
        self.model.pinned = pinned;
        self.rebuild_tiles();
        self
    }

    /// Enable smooth ease-out slide transitions on tab switches and overlays (tablet runtime).
    pub fn with_transitions(mut self) -> App {
        self.animate_transitions = true;
        self
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.frame = Frame::new(w, h);
        self.hits.clear();
        self.content_hits.clear();
        self.overlay_hits.clear();
        self.scroll = 0;
    }

    pub fn set_wall(&mut self, wall: WallTime) -> bool {
        let changed = self.model.wall != wall;
        self.model.wall = wall;
        changed
    }

    /// Installed apps (what the core reports) in catalog order, then the Settings shortcut; the Home
    /// tiles are the pinned ones among them (at most four).
    pub(super) fn rebuild_tiles(&mut self) {
        let mut apps = Vec::new();
        if let Some(st) = &self.model.status {
            for e in self.catalog.iter().filter(|e| e.id != "wifi") {
                if let Some(a) = st.apps.iter().find(|a| a.app == e.id) {
                    apps.push(Tile {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        icon: e.icon,
                        running: a.running,
                    });
                }
            }
        }
        // Phone is a shell-internal destination, not an exec app: pinnable like one
        apps.push(Tile {
            id: "phone".into(),
            name: "Phone".into(),
            icon: Icon::Phone,
            running: false,
        });
        apps.push(Tile {
            id: "media".into(),
            name: "Media".into(),
            icon: Icon::Media,
            running: false,
        });
        apps.push(Tile {
            id: "settings".into(),
            name: "Settings".into(),
            icon: Icon::Settings,
            running: false,
        });
        let tiles = self
            .model
            .pinned
            .iter()
            .filter_map(|id| apps.iter().find(|t| &t.id == id).cloned())
            .take(4)
            .collect();
        self.model.apps = apps;
        self.model.tiles = tiles;
    }

    /// New configuration read from the files (devd.json, shell.json). Returns true to redraw.
    pub fn set_config(&mut self, devd: DevdSettings, clock_24h: bool) -> bool {
        let changed = self.model.devd != devd || self.model.clock_24h != clock_24h;
        self.model.devd = devd;
        self.model.clock_24h = clock_24h;
        changed
    }

    pub fn set_wallpaper(&mut self, kind: walpad_ui::art::WallpaperKind, path: String) -> bool {
        let changed = self.model.wallpaper != kind || self.model.wallpaper_path != path;
        self.model.wallpaper = kind;
        self.model.wallpaper_path = path;
        changed
    }

    /// The saved window layout changed (the core saved it, or a tap changed it).
    pub fn set_windows(&mut self, w: crate::windows_ui::WindowsPrefs) -> bool {
        let changed = self.model.windows != w;
        self.model.windows = w;
        changed
    }

    pub fn set_sysinfo(&mut self, sys: SysInfo) -> bool {
        let changed = self.model.sys != sys;
        self.model.sys = sys;
        changed
    }

    /// Remember which request was sent with which id (to interpret its reply).
    pub fn note_sent(&mut self, id: u64, cmd: &Cmd) {
        self.pending.insert(id, cmd.purpose());
    }

    /// A warning toast from outside the model (the Recents strip).
    pub fn notice(&mut self, text: impl Into<String>) {
        self.toast(text, Tone::Warning);
    }

    pub(super) fn toast(&mut self, text: impl Into<String>, tone: Tone) {
        self.model.toast = Some(Toast {
            text: text.into(),
            tone,
            until: self.now + TOAST_MS,
        });
    }

    pub(super) fn go(&mut self, tab: Tab, out: &mut Outcome) {
        if self.model.overlay.as_ref().is_some_and(Overlay::is_popover) {
            self.model.overlay = None;
        }
        // the Apps destination is the Home grid (the panel is a popover over any screen)
        let tab = if tab == Tab::Apps { Tab::Home } else { tab };
        if tab != self.active {
            self.active = tab;
            self.scroll = 0;
            self.content_hits.clear();
            self.model.settings_page = None;
            self.model.imei_shown = false;
            self.model.store.view = StoreView::Installed;
            self.model.store.armed = None;
        }
        if matches!(tab, Tab::Device | Tab::Settings) {
            out.refresh = true;
        }
        if tab == Tab::Network && self.model.connected {
            out.cmd(Cmd::WifiListSaved);
        }
        if tab == Tab::Vpn && self.model.connected {
            out.cmd(Cmd::VpnFiles);
        }
        if tab == Tab::Media {
            self.model.media.scanning = true;
            out.cmd(Cmd::MediaScan);
        }
        if tab == Tab::Phone {
            self.open_phone(out);
        }
        if tab == Tab::Notifications && self.model.connected {
            out.cmd(Cmd::Notifications);
            // owner request 2026-11-12b: opening the screen clears the header badge, not just
            // reading each row one at a time - optimistic (the core answer confirms it) so the
            // badge updates at once instead of waiting for the next status poll
            if self
                .model
                .status
                .as_ref()
                .is_some_and(|s| s.notifications.unread_count > 0)
            {
                out.cmd(Cmd::NotificationsMarkAllRead);
                if let Some(s) = &mut self.model.status {
                    s.notifications.unread_count = 0;
                }
                for n in &mut self.model.notifications {
                    n.read = true;
                }
            }
        }
        out.dirty = true;
    }

    pub fn take_cmds(&mut self) -> Vec<Cmd> {
        std::mem::take(&mut self.queued)
    }
}
