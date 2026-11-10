//! App store screen (change app-catalogue-manager), shown as the Settings > Apps page and opened
//! straight to the catalogue from the Apps panel's Tools tile. Model, layout and drawing are pure
//! (content coordinates, like settings_screen.rs); the state changes live in `App`.
use crate::app::Target;
use serde_json::Value;
use walpad_proto::DEFAULT_ROLES;
use walpad_ui::components::{row_runs, Accessory, ButtonKind, ButtonState, Painter, Tone};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

/// Free space that must remain after an install (the core enforces it; shown here too).
pub const DEFAULT_RESERVE_KB: u64 = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct StoreEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub summary: String,
    pub size_kb: u64,
    pub installed: bool,
    pub system: bool,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Busy {
    pub op: String,
    pub id: String,
    pub percent: u8,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoreView {
    /// Installed apps and default apps.
    #[default]
    Installed,
    /// The catalogue by category.
    Browse,
    /// Choose the app for one role (index into `DEFAULT_ROLES`).
    Role(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreTarget {
    Browse,
    Install(usize),
    Remove(usize),
    Role(usize),
    PickDefault(usize),
    DismissError,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoreModel {
    pub view: StoreView,
    pub entries: Vec<StoreEntry>,
    pub loaded: bool,
    pub free_kb: Option<u64>,
    pub reserve_kb: u64,
    pub busy: Option<Busy>,
    /// Effective default app id per role (index = `DEFAULT_ROLES`).
    pub defaults: Vec<Option<String>>,
    pub error: Option<String>,
    /// An uninstall waiting for its confirming second tap (entry index).
    pub armed: Option<usize>,
}

pub fn role_label(role: &str) -> &'static str {
    match role {
        "browser" => "Web browser",
        "files" => "Files",
        "terminal" => "Terminal",
        "video" => "Video",
        "music" => "Music",
        "image" => "Images",
        "pdf" => "PDF documents",
        _ => "Other",
    }
}

pub fn fmt_size(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if kb < 1024 {
        format!("{kb} KB")
    } else if mb < 10.0 {
        format!("{mb:.1} MB")
    } else if mb < 1024.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
}

impl StoreModel {
    pub fn apply_catalogue(&mut self, v: &Value) {
        self.entries = v["entries"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|e| {
                        Some(StoreEntry {
                            id: e["id"].as_str()?.to_string(),
                            name: e["name"].as_str()?.to_string(),
                            category: e["category"].as_str().unwrap_or("Other").to_string(),
                            summary: e["summary"].as_str().unwrap_or("").to_string(),
                            size_kb: e["size_kb"].as_u64().unwrap_or(0),
                            installed: e["installed"].as_bool().unwrap_or(false),
                            system: e["system"].as_bool().unwrap_or(false),
                            roles: e["roles"]
                                .as_array()
                                .map(|r| {
                                    r.iter()
                                        .filter_map(|x| x.as_str().map(String::from))
                                        .collect()
                                })
                                .unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.free_kb = v["free_kb"].as_u64();
        self.reserve_kb = v["reserve_kb"].as_u64().unwrap_or(DEFAULT_RESERVE_KB);
        self.defaults = DEFAULT_ROLES
            .iter()
            .map(|r| v["defaults"][*r].as_str().map(String::from))
            .collect();
        self.busy = v["busy"].as_object().map(|b| Busy {
            op: v["busy"]["op"].as_str().unwrap_or("install").to_string(),
            id: v["busy"]["id"].as_str().unwrap_or("").to_string(),
            percent: b
                .get("percent")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(100) as u8,
            text: v["busy"]["text"].as_str().unwrap_or("").to_string(),
        });
        self.loaded = true;
        self.armed = None;
    }

    pub fn name_of(&self, id: &str) -> String {
        self.entries
            .iter()
            .find(|e| e.id == id)
            .map_or_else(|| id.to_string(), |e| e.name.clone())
    }

    /// Why an install would be refused for lack of space (the same rule the core applies).
    pub fn space_problem(&self, e: &StoreEntry) -> Option<String> {
        let free = self.free_kb?;
        let reserve = if self.reserve_kb == 0 {
            DEFAULT_RESERVE_KB
        } else {
            self.reserve_kb
        };
        (free.saturating_sub(e.size_kb) < reserve).then(|| {
            format!(
                "Not enough free space: {} needed, {} free, {} must stay free",
                fmt_size(e.size_kb),
                fmt_size(free),
                fmt_size(reserve)
            )
        })
    }

    fn role_candidates(&self, role: &str) -> Vec<usize> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.installed && e.roles.iter().any(|r| r == role))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn title(&self) -> &'static str {
        match self.view {
            StoreView::Installed => "Apps",
            StoreView::Browse => "App store",
            StoreView::Role(i) => DEFAULT_ROLES.get(i).map_or("Apps", |r| role_label(r)),
        }
    }
}

enum Item {
    Note(String, Color),
    Error(String),
    Progress {
        title: String,
        percent: u8,
        text: String,
    },
    Button(&'static str, StoreTarget, ButtonKind),
    Section(String),
    Row {
        icon: Option<Icon>,
        title: String,
        sub: Option<String>,
        acc: Accessory,
        target: Option<StoreTarget>,
    },
}

pub struct Layout {
    back: Rect,
    title: Rect,
    items: Vec<(Rect, Item)>,
    pub total: i32,
}

const ERROR_H: i32 = 92;
const PROGRESS_H: i32 = 92;
const LABEL_H: i32 = 28;

fn items(m: &StoreModel) -> Vec<(i32, Item)> {
    let mut v: Vec<(i32, Item)> = Vec::new();
    let free = match m.free_kb {
        Some(f) => format!(
            "Free space: {} ({} is always kept free)",
            fmt_size(f),
            fmt_size(if m.reserve_kb == 0 {
                DEFAULT_RESERVE_KB
            } else {
                m.reserve_kb
            })
        ),
        None => "Free space: unknown".to_string(),
    };
    v.push((24, Item::Note(free, TEXT_SECONDARY)));
    if let Some(e) = &m.error {
        v.push((ERROR_H, Item::Error(e.clone())));
    }
    if let Some(b) = &m.busy {
        let verb = if b.op == "remove" {
            "Uninstalling"
        } else {
            "Installing"
        };
        v.push((
            PROGRESS_H,
            Item::Progress {
                title: format!("{verb} {}", m.name_of(&b.id)),
                percent: b.percent,
                text: b.text.clone(),
            },
        ));
    }
    if !m.loaded {
        v.push((
            ROW_H,
            Item::Note("Loading the catalogue...".into(), TEXT_SECONDARY),
        ));
        return v;
    }
    let busy_id = m.busy.as_ref().map(|b| b.id.as_str());
    match m.view {
        StoreView::Installed => {
            v.push((
                BUTTON_H,
                Item::Button("Get more apps", StoreTarget::Browse, ButtonKind::Primary),
            ));
            v.push((LABEL_H, Item::Section("Installed apps".into())));
            for (i, e) in m.entries.iter().enumerate().filter(|(_, e)| e.installed) {
                let sub = format!("{} · {}", fmt_size(e.size_kb), e.category);
                let (acc, target) = if e.system {
                    (Accessory::Value("Built in".into()), None)
                } else if m.armed == Some(i) {
                    (
                        Accessory::Action("Tap to confirm".into()),
                        Some(StoreTarget::Remove(i)),
                    )
                } else if busy_id == Some(e.id.as_str()) {
                    (Accessory::Value("Working...".into()), None)
                } else {
                    (
                        Accessory::Action("Uninstall".into()),
                        Some(StoreTarget::Remove(i)),
                    )
                };
                v.push((
                    ROW_H,
                    Item::Row {
                        icon: Some(Icon::Apps),
                        title: e.name.clone(),
                        sub: Some(sub),
                        acc,
                        target,
                    },
                ));
            }
            v.push((LABEL_H, Item::Section("Default apps".into())));
            for (i, role) in DEFAULT_ROLES.iter().enumerate() {
                let cur = m
                    .defaults
                    .get(i)
                    .and_then(|d| d.as_deref())
                    .map_or_else(|| "None".to_string(), |id| m.name_of(id));
                v.push((
                    ROW_H,
                    Item::Row {
                        icon: None,
                        title: role_label(role).into(),
                        sub: None,
                        acc: Accessory::Value(cur),
                        target: Some(StoreTarget::Role(i)),
                    },
                ));
            }
        }
        StoreView::Browse => {
            let mut cats: Vec<&str> = Vec::new();
            for e in m.entries.iter().filter(|e| !e.system) {
                if !cats.contains(&e.category.as_str()) {
                    cats.push(&e.category);
                }
            }
            for c in cats {
                v.push((LABEL_H, Item::Section(c.to_string())));
                for (i, e) in m
                    .entries
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| !e.system && e.category == c)
                {
                    let sub = if e.summary.is_empty() {
                        fmt_size(e.size_kb)
                    } else {
                        format!("{} · {}", e.summary, fmt_size(e.size_kb))
                    };
                    let (acc, target) = if busy_id == Some(e.id.as_str()) {
                        (Accessory::Value("Working...".into()), None)
                    } else if e.installed {
                        (Accessory::Value("Installed".into()), None)
                    } else {
                        (
                            Accessory::Action("Install".into()),
                            Some(StoreTarget::Install(i)),
                        )
                    };
                    v.push((
                        ROW_H,
                        Item::Row {
                            icon: Some(Icon::Apps),
                            title: e.name.clone(),
                            sub: Some(sub),
                            acc,
                            target,
                        },
                    ));
                }
            }
        }
        StoreView::Role(ri) => {
            let role = DEFAULT_ROLES.get(ri).copied().unwrap_or("");
            let cur = m.defaults.get(ri).and_then(|d| d.as_deref());
            let cands = m.role_candidates(role);
            for i in &cands {
                let e = &m.entries[*i];
                let is = cur == Some(e.id.as_str());
                v.push((
                    ROW_H,
                    Item::Row {
                        icon: Some(if is { Icon::Check } else { Icon::Apps }),
                        title: e.name.clone(),
                        sub: Some(e.summary.clone()).filter(|s| !s.is_empty()),
                        acc: if is {
                            Accessory::Value("Default".into())
                        } else {
                            Accessory::Action("Use".into())
                        },
                        target: (!is).then_some(StoreTarget::PickDefault(*i)),
                    },
                ));
            }
            let hint = if cands.len() < 2 {
                "Install more apps for this from the App store"
            } else {
                ""
            };
            if !hint.is_empty() {
                v.push((24, Item::Note(hint.into(), TEXT_SECONDARY)));
            }
            v.push((
                BUTTON_H,
                Item::Button("Get more apps", StoreTarget::Browse, ButtonKind::Secondary),
            ));
        }
    }
    v
}

pub fn layout(frame: &Frame, m: &StoreModel) -> Layout {
    let c = frame.padded();
    let mut y = SP_4;
    let back = Rect::new(c.x, y, TOUCH_MIN, BUTTON_H);
    let title = Rect::new(c.x + TOUCH_MIN + SP_2, y, c.w - TOUCH_MIN - SP_2, BUTTON_H);
    y += BUTTON_H + SP_3;
    let mut out = Vec::new();
    for (h, it) in items(m) {
        let gap = match it {
            Item::Section(_) => SP_2,
            Item::Button(..) | Item::Error(_) | Item::Progress { .. } => SP_1,
            _ => 0,
        };
        y += gap;
        out.push((Rect::new(c.x, y, c.w, h), it));
        y += h + if matches!(
            out.last(),
            Some((_, Item::Button(..) | Item::Error(_) | Item::Progress { .. }))
        ) {
            SP_2
        } else {
            0
        };
    }
    Layout {
        back,
        title,
        items: out,
        total: y + SP_6,
    }
}

fn wrap(p: &mut Painter, s: &str, w: i32, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        let t = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if cur.is_empty() || p.fonts.measure(&t, Face::Regular, SIZE_SECONDARY) as i32 <= w {
            cur = t;
        } else {
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines.truncate(max);
    lines
}

pub fn render(
    p: &mut Painter,
    frame: &Frame,
    m: &StoreModel,
    online: bool,
    pressed: Option<Target>,
) -> Vec<(Rect, Target)> {
    let l = layout(frame, m);
    let mut hits = Vec::new();
    p.outline_box(
        l.back.inset(4),
        RADIUS,
        pressed == Some(Target::Back),
        false,
        false,
    );
    p.cv.icon(
        Icon::Back,
        l.back.x + (l.back.w - ICON) / 2,
        l.back.y + (l.back.h - ICON) / 2,
        ICON,
        TEXT,
    );
    p.screen_title(m.title(), l.title);
    hits.push((l.back, Target::Back));
    let busy = m.busy.is_some();
    for g in row_runs(
        l.items
            .iter()
            .map(|(r, it)| (*r, matches!(it, Item::Row { .. }))),
    ) {
        p.row_group(g);
    }
    for (r, it) in &l.items {
        match it {
            Item::Note(s, col) => p.text(
                s,
                Style::new(Face::Regular, SIZE_SECONDARY, *col),
                Rect::new(r.x + SP_1, r.y, r.w, r.h),
                Align::Left,
            ),
            Item::Section(s) => p.section_label(s, Rect::new(r.x + SP_1, r.y, r.w, r.h)),
            Item::Error(msg) => {
                let t = StoreTarget::DismissError;
                let pr = pressed == Some(Target::Store(t));
                p.card(*r, pr);
                p.cv.icon(Icon::Warning, r.x + SP_3, r.y + SP_3, ICON, DANGER);
                p.text(
                    "Could not finish (tap to dismiss)",
                    Style::new(Face::Bold, SIZE_SECONDARY, DANGER),
                    Rect::new(
                        r.x + SP_3 + ICON + SP_2,
                        r.y + SP_2,
                        r.w - 2 * SP_4 - ICON,
                        28,
                    ),
                    Align::Left,
                );
                let w = r.w - 2 * SP_3;
                for (i, line) in wrap(p, msg, w, 2).iter().enumerate() {
                    p.text(
                        line,
                        Style::new(Face::Regular, SIZE_SECONDARY, TEXT),
                        Rect::new(r.x + SP_3, r.y + 34 + i as i32 * 22, w, 22),
                        Align::Left,
                    );
                }
                hits.push((*r, Target::Store(t)));
            }
            Item::Progress {
                title,
                percent,
                text,
            } => {
                p.card(*r, false);
                p.text(
                    title,
                    Style::new(Face::Regular, SIZE_BODY, TEXT),
                    Rect::new(r.x + SP_4, r.y + SP_2, r.w - 2 * SP_4 - 56, 28),
                    Align::Left,
                );
                p.text(
                    &format!("{percent}%"),
                    Style::new(Face::Bold, SIZE_BODY, TEXT),
                    Rect::new(r.right() - SP_4 - 56, r.y + SP_2, 56, 28),
                    Align::Right,
                );
                let bar = Rect::new(r.x + SP_4, r.y + 42, r.w - 2 * SP_4, 8);
                p.cv.rrect(bar, 4, DIVIDER);
                let fw = (bar.w * *percent as i32 / 100).max(if *percent > 0 { 8 } else { 0 });
                if fw > 0 {
                    p.cv.rrect(Rect::new(bar.x, bar.y, fw, bar.h), 4, ACCENT);
                }
                p.text(
                    text,
                    Style::new(Face::Regular, SIZE_SECONDARY, TEXT_SECONDARY),
                    Rect::new(r.x + SP_4, r.y + 58, r.w - 2 * SP_4, 24),
                    Align::Left,
                );
            }
            Item::Button(label, t, kind) => {
                let st = if !online {
                    ButtonState::Disabled
                } else if pressed == Some(Target::Store(*t)) {
                    ButtonState::Pressed
                } else {
                    ButtonState::Default
                };
                p.button(*r, label, *kind, st, None);
                hits.push((*r, Target::Store(*t)));
            }
            Item::Row {
                icon,
                title,
                sub,
                acc,
                target,
            } => {
                let t = target.map(Target::Store);
                let disabled = !online
                    || (busy
                        && target.is_some_and(|t| {
                            matches!(t, StoreTarget::Install(_) | StoreTarget::Remove(_))
                        }));
                p.list_row(
                    *r,
                    *icon,
                    title,
                    sub.as_deref(),
                    acc,
                    t.is_some() && pressed == t,
                    disabled,
                );
                if let Some(t) = t {
                    hits.push((*r, t));
                }
            }
        }
    }
    let _ = Tone::Info;
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    pub fn sample() -> StoreModel {
        let mut m = StoreModel::default();
        m.apply_catalogue(&json!({
            "entries": [
                {"id":"browser","name":"Browser","category":"Built in","size_kb":224000,"installed":true,"system":true,"roles":["browser"]},
                {"id":"mousepad","name":"Mousepad","category":"Documents","summary":"Simple text editor","size_kb":700,"installed":true,"system":false,"roles":[]},
                {"id":"evince","name":"Evince","category":"Documents","summary":"PDF viewer","size_kb":15000,"installed":true,"system":false,"roles":["pdf"]},
                {"id":"zathura","name":"Zathura","category":"Documents","summary":"Light PDF viewer","size_kb":1500,"installed":false,"system":false,"roles":["pdf"]},
                {"id":"mpv","name":"mpv","category":"Media","summary":"Video player","size_kb":37600,"installed":false,"system":false,"roles":["video"]}
            ],
            "free_kb": 5242880, "reserve_kb": 2097152, "busy": null,
            "defaults": {"browser":"browser","pdf":"evince","video":null}
        }));
        m
    }

    #[test]
    fn sizes_are_readable() {
        assert_eq!(fmt_size(700), "700 KB");
        assert_eq!(fmt_size(1536), "1.5 MB");
        assert_eq!(fmt_size(37600), "37 MB");
        assert_eq!(fmt_size(3 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn catalogue_reply_is_parsed() {
        let m = sample();
        assert!(m.loaded && m.entries.len() == 5 && m.busy.is_none());
        assert_eq!(m.free_kb, Some(5242880));
        assert_eq!(m.defaults[6].as_deref(), Some("evince"), "pdf");
        assert_eq!(m.defaults[3], None, "video has none");
        assert_eq!(m.name_of("mpv"), "mpv");
    }

    #[test]
    fn space_rule_matches_the_core() {
        let mut m = sample();
        let big = StoreEntry {
            size_kb: 3 * 1024 * 1024,
            ..m.entries[4].clone()
        };
        m.free_kb = Some(4 * 1024 * 1024);
        let msg = m.space_problem(&big).unwrap();
        assert!(
            msg.contains("Not enough free space")
                && msg.contains("3.0 GB")
                && msg.contains("4.0 GB"),
            "{msg}"
        );
        m.free_kb = Some(6 * 1024 * 1024);
        assert!(m.space_problem(&big).is_none());
        assert!(m.space_problem(&m.entries[4].clone()).is_none());
    }

    #[test]
    fn layouts_do_not_overlap_and_targets_are_touchable() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            for view in [
                StoreView::Installed,
                StoreView::Browse,
                StoreView::Role(6),
                StoreView::Role(3),
            ] {
                let mut m = sample();
                m.view = view;
                m.error = Some("Not enough free space".into());
                m.busy = Some(Busy {
                    op: "install".into(),
                    id: "mpv".into(),
                    percent: 40,
                    text: "Installing".into(),
                });
                let l = layout(&f, &m);
                for (i, (a, ia)) in l.items.iter().enumerate() {
                    if matches!(
                        ia,
                        Item::Row {
                            target: Some(_),
                            ..
                        } | Item::Button(..)
                    ) {
                        assert!(a.h >= TOUCH_MIN, "{a:?}");
                    }
                    for (b, _) in &l.items[i + 1..] {
                        assert!(a.bottom() <= b.y, "{a:?} overlaps {b:?}");
                    }
                }
                assert!(l.total > l.items.last().map_or(0, |(r, _)| r.bottom()));
            }
        }
    }
}
