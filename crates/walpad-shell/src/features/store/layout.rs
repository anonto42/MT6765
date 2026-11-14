use super::model::{fmt_size, role_label, StoreModel, StoreTarget, StoreView, DEFAULT_RESERVE_KB};
use walpad_proto::DEFAULT_ROLES;
use walpad_ui::components::{Accessory, ButtonKind};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Color, Icon, Rect};

pub(super) enum Item {
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
    pub(super) back: Rect,
    pub(super) title: Rect,
    pub(super) items: Vec<(Rect, Item)>,
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
    v.push((24, Item::Note(free, text_secondary())));
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
            Item::Note("Loading the catalogue...".into(), text_secondary()),
        ));
        return v;
    }
    let busy_label = |id: &str| {
        m.busy.as_ref().filter(|b| b.id == id).map(|b| {
            if b.percent > 0 {
                format!("{}%", b.percent)
            } else {
                "Working...".into()
            }
        })
    };
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
                } else if let Some(lbl) = busy_label(&e.id) {
                    (Accessory::Value(lbl), None)
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
                    let (acc, target) = if let Some(lbl) = busy_label(&e.id) {
                        (Accessory::Value(lbl), None)
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
                v.push((24, Item::Note(hint.into(), text_secondary())));
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
