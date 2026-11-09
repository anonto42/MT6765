//! Application state: what the shell shows and how taps map to local navigation or core commands.
use crate::core_client::CoreEvent;
use crate::gesture::Gesture;
use crate::home;
use serde_json::{json, Value};
use walpad_hw::Root;
use walpad_proto::Status;
use walpad_ui::components::{Painter, Tab, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Face, Icon, Rect, Style};

pub type Ms = u64;

#[derive(Debug, Clone, PartialEq)]
pub struct WallTime {
    pub hour: u8,
    pub minute: u8,
    /// "AM"/"PM" in 12-hour mode.
    pub suffix: Option<&'static str>,
    /// e.g. "Monday, 09 November".
    pub date: String,
}

impl WallTime {
    pub fn new(hour: u8, minute: u8, date: &str, clock_24h: bool) -> WallTime {
        if clock_24h {
            WallTime {
                hour,
                minute,
                suffix: None,
                date: date.into(),
            }
        } else {
            let h12 = if hour.is_multiple_of(12) {
                12
            } else {
                hour % 12
            };
            WallTime {
                hour: h12,
                minute,
                suffix: Some(if hour >= 12 { "PM" } else { "AM" }),
                date: date.into(),
            }
        }
    }

    /// Local time now and the milliseconds until the next minute boundary.
    pub fn now(clock_24h: bool) -> (WallTime, u64) {
        let z = jiff::Zoned::now();
        let ms_in_minute = z.second() as u64 * 1000 + (z.subsec_nanosecond() as u64) / 1_000_000;
        let date = z.strftime("%A, %d %B").to_string();
        (
            WallTime::new(z.hour() as u8, z.minute() as u8, &date, clock_24h),
            60_000u64.saturating_sub(ms_in_minute).max(1),
        )
    }
}

/// An app known from /etc/walpad/apps.json.
#[derive(Debug, Clone, PartialEq)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub icon: Icon,
}

pub fn load_catalog(root: &Root) -> Vec<AppEntry> {
    let Some(list) = std::fs::read_to_string(root.p("/etc/walpad/apps.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
    else {
        return Vec::new();
    };
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let id = e["id"].as_str()?.to_string();
                    let name = e["name"].as_str().unwrap_or(&id).to_string();
                    let icon =
                        Icon::from_name(e["icon"].as_str().unwrap_or("")).unwrap_or(Icon::Apps);
                    Some(AppEntry { id, name, icon })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn read_clock_24h(root: &Root) -> bool {
    std::fs::read_to_string(root.p("/etc/walpad/shell.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v["clock_24h"].as_bool())
        .unwrap_or(true)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    pub id: String,
    pub name: String,
    pub icon: Icon,
    pub running: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Nav(usize),
    WifiCard,
    BatteryCard,
    Tile(usize),
    Slider,
    Rotation,
    Keyboard,
    ScreenOff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cmd {
    SetBrightness(u8),
    RotateToggle,
    KeyboardToggle,
    ScreenOff,
    Launch(String),
}

impl Cmd {
    /// (command, args) for the core's IPC.
    pub fn request(&self) -> (&'static str, Value) {
        match self {
            Cmd::SetBrightness(v) => ("set_brightness", json!({"value": v})),
            Cmd::RotateToggle => ("rotate", json!({"mode": "toggle"})),
            Cmd::KeyboardToggle => ("keyboard", json!({"mode": "toggle"})),
            Cmd::ScreenOff => ("screen", json!({"mode": "off"})),
            Cmd::Launch(id) => ("launch", json!({"app": id})),
        }
    }
}

/// Everything the screens draw from.
pub struct Model {
    pub status: Option<Status>,
    pub connected: bool,
    pub tiles: Vec<Tile>,
    pub wall: WallTime,
    pub pressed: Option<Target>,
    pub slider_active: bool,
    pub slider_value: f32,
}

impl Model {
    /// Slider position 0..=1: the dragged value while dragging, else the core's brightness.
    pub fn brightness_value(&self) -> f32 {
        if self.slider_active {
            self.slider_value
        } else {
            self.status
                .as_ref()
                .map_or(0.0, |s| s.brightness as f32 / 255.0)
        }
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Outcome {
    pub dirty: bool,
    pub cmds: Vec<Cmd>,
}

const SLIDER_SEND_EVERY_MS: Ms = 80;
const PINNED: [&str; 3] = ["browser", "terminal", "files"];

pub struct App {
    pub frame: Frame,
    pub active: Tab,
    pub model: Model,
    catalog: Vec<AppEntry>,
    hits: Vec<(Rect, Target)>,
    last_slider_send: Ms,
}

fn native_brightness(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round().clamp(1.0, 255.0) as u8
}

impl App {
    pub fn new(w: i32, h: i32, catalog: Vec<AppEntry>, wall: WallTime) -> App {
        let mut a = App {
            frame: Frame::new(w, h),
            active: Tab::Home,
            model: Model {
                status: None,
                connected: false,
                tiles: Vec::new(),
                wall,
                pressed: None,
                slider_active: false,
                slider_value: 0.0,
            },
            catalog,
            hits: Vec::new(),
            last_slider_send: 0,
        };
        a.rebuild_tiles();
        a
    }

    pub fn resize(&mut self, w: i32, h: i32) {
        self.frame = Frame::new(w, h);
        self.hits.clear();
    }

    pub fn set_wall(&mut self, wall: WallTime) -> bool {
        let changed = self.model.wall != wall;
        self.model.wall = wall;
        changed
    }

    /// First four of browser, terminal, files that are installed, plus the Settings shortcut.
    fn rebuild_tiles(&mut self) {
        let mut tiles = Vec::new();
        if let Some(st) = &self.model.status {
            for id in PINNED {
                if let (Some(e), Some(a)) = (
                    self.catalog.iter().find(|e| e.id == id),
                    st.apps.iter().find(|a| a.app == id),
                ) {
                    tiles.push(Tile {
                        id: e.id.clone(),
                        name: e.name.clone(),
                        icon: e.icon,
                        running: a.running,
                    });
                }
            }
        }
        tiles.push(Tile {
            id: "settings".into(),
            name: "Settings".into(),
            icon: Icon::Settings,
            running: false,
        });
        tiles.truncate(4);
        self.model.tiles = tiles;
    }

    /// Apply a core event. Returns true when the screen must be redrawn.
    pub fn on_core(&mut self, ev: &CoreEvent) -> bool {
        match ev {
            CoreEvent::Connected => {
                self.model.connected = true;
                true
            }
            CoreEvent::Disconnected => {
                self.model.connected = false;
                self.model.status = None;
                self.rebuild_tiles();
                true
            }
            CoreEvent::Status(s) => {
                self.model.status = Some((**s).clone());
                self.model.connected = true;
                self.rebuild_tiles();
                true
            }
            CoreEvent::Wifi(_) | CoreEvent::Error(_) => false,
        }
    }

    /// Persistent banner under the status bar (core down has priority over low battery).
    pub fn banner(&self) -> Option<(&'static str, Tone)> {
        if !self.model.connected {
            return Some(("System service not responding", Tone::Danger));
        }
        let st = self.model.status.as_ref()?;
        let charging = st
            .battery
            .state
            .as_deref()
            .is_some_and(|s| s.to_ascii_lowercase().starts_with("charg"));
        (st.battery.low && !charging)
            .then_some(("Battery low - connect the charger", Tone::Warning))
    }

    fn hit(&self, x: i32, y: i32) -> Option<Target> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, t)| *t)
    }

    fn slider_value_at(&self, x: i32) -> f32 {
        let track = self
            .hits
            .iter()
            .find(|(_, t)| *t == Target::Slider)
            .map(|(r, _)| r.inset(SP_4))
            .unwrap_or(Rect::new(0, 0, 1, 1));
        ((x - track.x) as f32 / track.w.max(1) as f32).clamp(0.0, 1.0)
    }

    pub fn on_gesture(&mut self, g: &Gesture, now: Ms) -> Outcome {
        let mut out = Outcome::default();
        let interactive = self.model.connected;
        match *g {
            Gesture::Pressed { x, y } => {
                let t = self.hit(x, y);
                self.model.pressed = t;
                if t == Some(Target::Slider) && interactive {
                    self.model.slider_active = true;
                    self.model.slider_value = self.slider_value_at(x);
                }
                out.dirty = t.is_some();
            }
            Gesture::Cancel => {
                out.dirty = self.model.pressed.take().is_some();
            }
            Gesture::Tap { x, y } => {
                self.model.pressed = None;
                out.dirty = true;
                let was_slider = std::mem::take(&mut self.model.slider_active);
                match self.hit(x, y) {
                    Some(Target::Nav(i)) => self.active = Tab::ALL[i],
                    Some(Target::WifiCard) => self.active = Tab::Network,
                    Some(Target::BatteryCard) => self.active = Tab::Device,
                    Some(Target::Tile(i)) => match self.model.tiles.get(i).map(|t| t.id.clone()) {
                        Some(id) if id == "settings" => self.active = Tab::Settings,
                        Some(id) if interactive => out.cmds.push(Cmd::Launch(id)),
                        _ => {}
                    },
                    Some(Target::Slider) if interactive => {
                        let v = if was_slider {
                            self.model.slider_value
                        } else {
                            self.slider_value_at(x)
                        };
                        out.cmds.push(Cmd::SetBrightness(native_brightness(v)));
                    }
                    Some(Target::Rotation) if interactive => out.cmds.push(Cmd::RotateToggle),
                    Some(Target::Keyboard) if interactive => out.cmds.push(Cmd::KeyboardToggle),
                    Some(Target::ScreenOff) if interactive => out.cmds.push(Cmd::ScreenOff),
                    _ => out.dirty = was_slider,
                }
            }
            Gesture::LongPress { .. } => {
                self.model.slider_active = false;
                out.dirty = self.model.pressed.take().is_some();
            }
            Gesture::DragStart { .. } => {}
            Gesture::Drag { dx, .. } => {
                if self.model.slider_active {
                    let w = self
                        .hits
                        .iter()
                        .find(|(_, t)| *t == Target::Slider)
                        .map_or(1, |(r, _)| r.inset(SP_4).w.max(1));
                    self.model.slider_value =
                        (self.model.slider_value + dx as f32 / w as f32).clamp(0.0, 1.0);
                    out.dirty = true;
                    if now.saturating_sub(self.last_slider_send) >= SLIDER_SEND_EVERY_MS {
                        self.last_slider_send = now;
                        out.cmds.push(Cmd::SetBrightness(native_brightness(
                            self.model.slider_value,
                        )));
                    }
                }
            }
            Gesture::DragEnd => {
                if std::mem::take(&mut self.model.slider_active) {
                    out.cmds.push(Cmd::SetBrightness(native_brightness(
                        self.model.slider_value,
                    )));
                    out.dirty = true;
                }
            }
        }
        out
    }

    pub fn render(&mut self, p: &mut Painter) {
        p.cv.fill(BG);
        let (ssid, pct, est, low) = match &self.model.status {
            Some(s) => (
                home::wifi_name(s),
                s.battery.percent,
                s.battery.source == walpad_proto::BatterySource::VoltageEstimate,
                s.battery.low,
            ),
            None => (None, None, false, false),
        };
        p.status_bar(self.frame.status, ssid.as_deref(), pct, est, low);
        let banner = self.banner();
        let banner_h = if banner.is_some() { 56 } else { 0 };
        self.hits.clear();
        if self.active == Tab::Home {
            self.hits = home::render(p, &self.frame, &self.model, banner_h);
        } else {
            let c = self.frame.padded();
            let y = c.y + banner_h;
            p.screen_title(self.active.label(), Rect::new(c.x, y + SP_4, c.w, 40));
            p.text(
                "Coming in the next cards",
                Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
                Rect::new(c.x, y + 64, c.w, 24),
                Align::Left,
            );
        }
        if let Some((text, tone)) = banner {
            p.banner(
                Rect::new(0, self.frame.status.bottom(), self.frame.w, 48),
                text,
                tone,
            );
        }
        for (i, r) in p
            .bottom_nav(self.frame.nav, self.active)
            .into_iter()
            .enumerate()
        {
            self.hits.push((r, Target::Nav(i)));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::Path;
    use walpad_proto::{
        AppState, Battery, BatterySource, Hardware, ScreenState, WifiRadio, WifiStatus,
    };
    use walpad_ui::{Canvas, Fonts};

    pub fn status(brightness: u32, apps: &[(&str, bool)]) -> Status {
        Status {
            battery: Battery {
                percent: Some(62),
                source: BatterySource::VoltageEstimate,
                state: Some("Discharging".into()),
                mv: Some(3920),
                temp_c: Some(34.0),
                low: false,
            },
            screen: ScreenState::On,
            brightness,
            orientation: 0,
            rotation_locked: false,
            lid_closed: false,
            wifi: WifiStatus {
                radio: WifiRadio::Up,
                state: Some("COMPLETED".into()),
                ssid: Some("Home".into()),
                ip: Some("192.168.0.100".into()),
                signal_dbm: Some(-58),
            },
            apps: apps
                .iter()
                .map(|(a, r)| AppState {
                    app: a.to_string(),
                    running: *r,
                })
                .collect(),
            hardware: Hardware {
                touch: "ok".into(),
                gsensor: "ok".into(),
                audio: "unverified".into(),
                bluetooth: "unavailable".into(),
                sd_card: "unavailable".into(),
            },
        }
    }

    pub fn catalog() -> Vec<AppEntry> {
        vec![
            AppEntry {
                id: "browser".into(),
                name: "Browser".into(),
                icon: Icon::Globe,
            },
            AppEntry {
                id: "terminal".into(),
                name: "Terminal".into(),
                icon: Icon::Terminal,
            },
            AppEntry {
                id: "files".into(),
                name: "Files".into(),
                icon: Icon::Folder,
            },
        ]
    }

    pub fn wall() -> WallTime {
        WallTime::new(14, 22, "Monday, 09 November", true)
    }

    fn render(app: &mut App) -> Canvas {
        let mut f =
            Fonts::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../walpad-ui/assets/fonts"))
                .unwrap();
        let mut c = Canvas::new(app.frame.w as u32, app.frame.h as u32, BG).unwrap();
        app.render(&mut Painter {
            cv: &mut c,
            fonts: &mut f,
        });
        c
    }

    fn online() -> App {
        let mut app = App::new(800, 1280, catalog(), wall());
        app.on_core(&CoreEvent::Status(Box::new(status(
            120,
            &[("browser", false), ("terminal", true)],
        ))));
        render(&mut app);
        app
    }
    fn center(app: &App, t: Target) -> (i32, i32) {
        app.hits
            .iter()
            .find(|(_, x)| *x == t)
            .map(|(r, _)| r.center())
            .unwrap()
    }
    fn tap(app: &mut App, t: Target) -> Outcome {
        let (x, y) = center(app, t);
        app.on_gesture(&Gesture::Pressed { x, y }, 0);
        app.on_gesture(&Gesture::Tap { x, y }, 10)
    }

    #[test]
    fn tiles_are_the_installed_pinned_apps_plus_settings() {
        let app = online();
        let ids: Vec<&str> = app.model.tiles.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(
            ids,
            ["browser", "terminal", "settings"],
            "files is not installed so it is not shown"
        );
        assert!(app.model.tiles[1].running && !app.model.tiles[0].running);
        let off = App::new(800, 1280, catalog(), wall());
        assert_eq!(
            off.model.tiles.len(),
            1,
            "offline: only the Settings shortcut"
        );
    }

    #[test]
    fn taps_launch_apps_or_navigate() {
        let mut app = online();
        assert_eq!(
            tap(&mut app, Target::Tile(0)).cmds,
            vec![Cmd::Launch("browser".into())]
        );
        assert_eq!(
            tap(&mut app, Target::Rotation).cmds,
            vec![Cmd::RotateToggle]
        );
        assert_eq!(
            tap(&mut app, Target::Keyboard).cmds,
            vec![Cmd::KeyboardToggle]
        );
        assert_eq!(tap(&mut app, Target::ScreenOff).cmds, vec![Cmd::ScreenOff]);
        assert!(tap(&mut app, Target::Tile(2)).cmds.is_empty());
        assert_eq!(
            app.active,
            Tab::Settings,
            "the Settings tile switches tab locally"
        );
        app.active = Tab::Home;
        render(&mut app);
        tap(&mut app, Target::WifiCard);
        assert_eq!(app.active, Tab::Network);
        app.active = Tab::Home;
        render(&mut app);
        tap(&mut app, Target::BatteryCard);
        assert_eq!(app.active, Tab::Device);
        tap(&mut app, Target::Nav(1));
        assert_eq!(app.active, Tab::Apps);
    }

    #[test]
    fn offline_shows_the_banner_and_ignores_commands() {
        let mut app = App::new(800, 1280, catalog(), wall());
        render(&mut app);
        assert_eq!(
            app.banner(),
            Some(("System service not responding", Tone::Danger))
        );
        assert!(tap(&mut app, Target::Rotation).cmds.is_empty());
        assert!(tap(&mut app, Target::Slider).cmds.is_empty());
        assert_eq!(
            tap(&mut app, Target::Nav(2)).cmds,
            vec![],
            "navigation still works offline"
        );
        assert_eq!(app.active, Tab::Network);
        // the core comes up: banner gone
        app.on_core(&CoreEvent::Connected);
        app.on_core(&CoreEvent::Status(Box::new(status(120, &[]))));
        assert_eq!(app.banner(), None);
        // and goes away again
        assert!(app.on_core(&CoreEvent::Disconnected));
        assert_eq!(app.model.status, None);
        assert!(app.banner().is_some());
    }

    #[test]
    fn low_battery_banner_until_charging() {
        let mut app = online();
        let mut s = status(120, &[]);
        s.battery.low = true;
        app.on_core(&CoreEvent::Status(Box::new(s.clone())));
        assert_eq!(
            app.banner(),
            Some(("Battery low - connect the charger", Tone::Warning))
        );
        s.battery.state = Some("Charging".into());
        app.on_core(&CoreEvent::Status(Box::new(s)));
        assert_eq!(app.banner(), None);
    }

    #[test]
    fn slider_tap_sets_and_drag_is_rate_limited_with_a_final_value() {
        let mut app = online();
        let (cx, cy) = center(&app, Target::Slider);
        let track = app
            .hits
            .iter()
            .find(|(_, t)| *t == Target::Slider)
            .map(|(r, _)| r.inset(SP_4))
            .unwrap();
        // tap at the far right = 255, tap at the far left never goes below 1 (0 would switch the screen off)
        app.on_gesture(
            &Gesture::Pressed {
                x: track.right(),
                y: cy,
            },
            0,
        );
        assert_eq!(
            app.on_gesture(
                &Gesture::Tap {
                    x: track.right(),
                    y: cy
                },
                5
            )
            .cmds,
            vec![Cmd::SetBrightness(255)]
        );
        app.on_gesture(&Gesture::Pressed { x: track.x, y: cy }, 10);
        assert_eq!(
            app.on_gesture(&Gesture::Tap { x: track.x, y: cy }, 15).cmds,
            vec![Cmd::SetBrightness(1)]
        );

        // drag from the middle: first move sends, moves inside 80 ms do not, DragEnd sends the final value
        app.on_gesture(&Gesture::Pressed { x: cx, y: cy }, 1000);
        app.on_gesture(&Gesture::Cancel, 1010);
        app.on_gesture(&Gesture::DragStart { x: cx, y: cy }, 1010);
        let a = app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1100);
        assert_eq!(a.cmds.len(), 1);
        let b = app.on_gesture(&Gesture::Drag { dx: 20, dy: 0 }, 1130);
        assert!(b.cmds.is_empty() && b.dirty, "rate limited but redrawn");
        let end = app.on_gesture(&Gesture::DragEnd, 1200);
        assert_eq!(end.cmds.len(), 1);
        assert!(!app.model.slider_active);
        assert!(matches!(end.cmds[0], Cmd::SetBrightness(v) if v > 128));
    }

    #[test]
    fn clock_formats() {
        let w = WallTime::new(14, 5, "Monday, 09 November", false);
        assert_eq!((w.hour, w.minute, w.suffix), (2, 5, Some("PM")));
        assert_eq!(WallTime::new(0, 0, "x", false).hour, 12);
        assert_eq!(WallTime::new(12, 30, "x", false).suffix, Some("PM"));
        assert_eq!(WallTime::new(23, 59, "x", true).suffix, None);
        let (_, ms) = WallTime::now(true);
        assert!((1..=60_000).contains(&ms));
    }

    #[test]
    fn requests_have_the_core_protocol_shape() {
        assert_eq!(
            Cmd::SetBrightness(77).request(),
            ("set_brightness", json!({"value": 77}))
        );
        assert_eq!(
            Cmd::Launch("browser".into()).request(),
            ("launch", json!({"app": "browser"}))
        );
        assert_eq!(Cmd::ScreenOff.request(), ("screen", json!({"mode": "off"})));
    }

    #[test]
    fn catalog_and_settings_files() {
        let root = std::env::temp_dir().join(format!("walpad-app-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("etc/walpad")).unwrap();
        std::fs::write(root.join("etc/walpad/apps.json"), r#"[{"id":"browser","name":"Browser","icon":"globe","exec":["x"]},{"id":"weird","exec":["y"]},{"nope":1}]"#).unwrap();
        let cat = load_catalog(&Root::new(&root));
        assert_eq!(cat.len(), 2);
        assert_eq!(
            (cat[0].icon, cat[1].icon, cat[1].name.as_str()),
            (Icon::Globe, Icon::Apps, "weird")
        );
        assert!(read_clock_24h(&Root::new(&root)), "default 24 h");
        std::fs::write(
            root.join("etc/walpad/shell.json"),
            r#"{"clock_24h": false}"#,
        )
        .unwrap();
        assert!(!read_clock_24h(&Root::new(&root)));
    }
}
