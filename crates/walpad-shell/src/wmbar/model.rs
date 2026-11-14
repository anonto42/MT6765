use walpad_hw::Root;
use walpad_proto::wm::{LayoutMode, SnapZone, WRect};
use walpad_ui::tokens::SP_2;
use walpad_ui::Rect;

pub const BAR_H: i32 = 48;
pub const BAR_CLASS: &str = "walpad-wmbar";
pub const GRIP_CLASS: &str = "walpad-wmgrip";
/// Hit size of the resize grip.
pub const GRIP: i32 = 56;
pub const TAB_MIN_W: i32 = 76;
pub const TAB_MAX_W: i32 = 168;
pub const PLUS_W: i32 = 52;
pub const CLOSE_W: i32 = 40;
pub const SNAP_W: i32 = 48;
/// Room kept for the drag area between the tabs and the snap buttons.
const DRAG_MIN_W: i32 = 40;
/// While a window is dragged a placement is sent at most this often.
pub(super) const DRAG_SEND_MS: u64 = 70;
/// How often the bar looks at the window facts while an app is in front.
pub(super) const REFRESH_MS: u64 = 500;
/// Smallest movement (px) that counts as a drag, not a tap.
pub(super) const DRAG_SLOP: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub win: u32,
    pub label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Tab(u32),
    TabClose(u32),
    New,
    Snap(SnapZone),
    /// The free area of the bar: drags the window in free mode.
    Drag,
}

/// An app of apps.json as far as the bar needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppInfo {
    pub id: String,
    pub name: String,
    pub class: String,
    /// `max_instances` (1 = one window only).
    pub max: u32,
}

pub fn load_apps(root: &Root) -> Vec<AppInfo> {
    walpad_proto::appdefs::entries(
        &root.p(walpad_proto::appdefs::APPS_JSON),
        &root.p(walpad_proto::appdefs::APPS_D),
    )
    .iter()
    .filter_map(|e| {
        let id = e["id"].as_str()?.to_string();
        let class = e["wm_class"]
            .as_str()
            .filter(|c| !c.is_empty())?
            .to_string();
        Some(AppInfo {
            name: e["name"].as_str().unwrap_or(&id).to_string(),
            max: e["max_instances"].as_u64().unwrap_or(1).max(1) as u32,
            id,
            class,
        })
    })
    .collect()
}

/// The first app whose wm_class is one of the window's class names (case-insensitive).
pub fn app_for<'a>(apps: &'a [AppInfo], class: &[String]) -> Option<&'a AppInfo> {
    apps.iter()
        .find(|a| class.iter().any(|c| c.eq_ignore_ascii_case(&a.class)))
}

/// Should the bar be up for the window in front?
pub fn wants_visible(app: Option<&AppInfo>, mode: LayoutMode) -> bool {
    app.is_some_and(|a| a.max > 1 || mode == LayoutMode::Free)
}

/// Tab text: "Terminal 2" when there is room, else the number.
pub fn tab_label(name: &str, n: usize, tab_w: i32) -> String {
    if tab_w >= 120 {
        format!("{name} {n}")
    } else {
        n.to_string()
    }
}

/// What the bar draws and where its hit areas are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarModel {
    pub width: i32,
    /// One tab per window of a multi-window app (empty = a single title instead).
    pub tabs: Vec<Tab>,
    pub active: u32,
    pub title: String,
    pub can_new: bool,
    /// Free mode: snap buttons and a drag area.
    pub free: bool,
}

pub const SNAP_BUTTONS: [SnapZone; 5] = [
    SnapZone::Left,
    SnapZone::Right,
    SnapZone::Top,
    SnapZone::Bottom,
    SnapZone::Full,
];

/// Width of one tab (all equal) for `n` tabs in the room left of the snap buttons.
pub fn tab_width(m: &BarModel) -> i32 {
    let n = m.tabs.len().max(1) as i32;
    let snap = if m.free {
        SNAP_BUTTONS.len() as i32 * SNAP_W + SP_2
    } else {
        0
    };
    let plus = if m.can_new { PLUS_W } else { 0 };
    let avail = m.width - SP_2 - snap - plus - DRAG_MIN_W;
    (avail / n).clamp(TAB_MIN_W, TAB_MAX_W)
}

/// Hit areas in bar coordinates; the Drag area comes last (anything not hit before it).
pub fn layout(m: &BarModel) -> Vec<(Rect, Hit)> {
    let mut v = Vec::new();
    let mut x = SP_2;
    let tw = tab_width(m);
    for t in &m.tabs {
        v.push((
            Rect::new(x + tw - CLOSE_W, 0, CLOSE_W, BAR_H),
            Hit::TabClose(t.win),
        ));
        v.push((Rect::new(x, 0, tw - CLOSE_W, BAR_H), Hit::Tab(t.win)));
        x += tw;
    }
    if m.can_new {
        v.push((Rect::new(x, 0, PLUS_W, BAR_H), Hit::New));
        x += PLUS_W;
    }
    let mut end = m.width;
    if m.free {
        for z in SNAP_BUTTONS.iter().rev() {
            end -= SNAP_W;
            v.push((Rect::new(end, 0, SNAP_W, BAR_H), Hit::Snap(*z)));
        }
    }
    v.push((Rect::new(x, 0, (end - x).max(0), BAR_H), Hit::Drag));
    v
}

pub fn hit(m: &BarModel, x: i32, y: i32) -> Option<Hit> {
    layout(m)
        .into_iter()
        .find(|(r, _)| r.contains(x, y))
        .map(|(_, h)| h)
}

/// `_NET_WM_STRUT_PARTIAL` of the bar: a top strut over the visible width.
pub fn strut_partial(width: i32) -> [u32; 12] {
    [
        0,
        0,
        BAR_H as u32,
        0,
        0,
        0,
        0,
        0,
        0,
        (width - 1).max(0) as u32,
        0,
        0,
    ]
}

/// The corner grip: inside the bottom-right corner of the window, kept on screen.
pub fn grip_rect(win: WRect, area: (i32, i32)) -> Rect {
    let x = (win.right() - GRIP).clamp(0, (area.0 - GRIP).max(0));
    let y = (win.bottom() - GRIP).clamp(0, (area.1 - GRIP).max(0));
    Rect::new(x, y, GRIP, GRIP)
}
