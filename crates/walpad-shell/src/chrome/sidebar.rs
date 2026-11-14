use crate::app::{Model, Target};
use walpad_ui::components::{Painter, Tab};
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Icon, Rect};

pub const NAV_HOME: usize = 0;
pub const NAV_APPS: usize = 1;
pub const NAV_SETTINGS: usize = 4;

pub(super) const PAD_TOP: i32 = 20;
const GAP: i32 = SP_3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideItem {
    /// The drawn button.
    pub rect: Rect,
    /// The touch area: the full sidebar width around the button.
    pub hit: Rect,
    pub target: Target,
    pub icon: Icon,
    /// The Apps toggle is a circle, the rest rounded squares.
    pub round: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SidebarLayout {
    pub items: Vec<SideItem>,
    /// y of the short divider above Settings.
    pub divider_y: i32,
}

/// Home, the pinned apps (`(tile index, icon)`, Settings shortcut excluded), a divider, Settings,
/// a flexible spacer, and the Apps toggle at the bottom (chevron right closed, left open).
pub fn sidebar_layout(frame: &Frame, pinned: &[(usize, Icon)], panel_open: bool) -> SidebarLayout {
    let sb = frame.sidebar;
    let x = sb.x + (sb.w - BOX_DOCK) / 2;
    let item = |y: i32, target: Target, icon: Icon, round: bool| SideItem {
        rect: Rect::new(x, y, BOX_DOCK, BOX_DOCK),
        hit: Rect::new(sb.x, y - GAP / 2, sb.w, BOX_DOCK + GAP),
        target,
        icon,
        round,
    };
    let toggle_y = sb.bottom() - PAD_TOP - BOX_DOCK;
    let mut items = vec![item(
        sb.y + PAD_TOP,
        Target::Nav(NAV_HOME),
        Icon::Home,
        false,
    )];
    let mut y = sb.y + PAD_TOP + BOX_DOCK + GAP;
    // pinned apps only while Settings and the toggle still fit below them
    let room = |y: i32| y + BOX_DOCK + 2 * GAP + 1 + BOX_DOCK + GAP <= toggle_y;
    for (i, icon) in pinned {
        if !room(y) {
            break;
        }
        items.push(item(y, Target::Tile(*i), *icon, false));
        y += BOX_DOCK + GAP;
    }
    let divider_y = y;
    y += 1 + GAP;
    items.push(item(y, Target::Nav(NAV_SETTINGS), Icon::Settings, false));
    items.push(item(
        toggle_y,
        Target::Nav(NAV_APPS),
        if panel_open {
            Icon::Back
        } else {
            Icon::ChevronRight
        },
        true,
    ));
    SidebarLayout { items, divider_y }
}

/// The pinned apps shown in the sidebar: Home tiles minus the Settings shortcut (Settings has
/// its own button).
pub fn pinned_items(m: &Model) -> Vec<(usize, Icon)> {
    m.tiles
        .iter()
        .enumerate()
        .filter(|(_, t)| t.id != "settings")
        .map(|(i, t)| (i, t.icon))
        .collect()
}

/// The sidebar destination drawn as active.
pub fn active_target(active: Tab, panel_open: bool) -> Option<Target> {
    if panel_open {
        return Some(Target::Nav(NAV_APPS));
    }
    match active {
        Tab::Home | Tab::Apps => Some(Target::Nav(NAV_HOME)),
        Tab::Settings => Some(Target::Nav(NAV_SETTINGS)),
        Tab::Network
        | Tab::Device
        | Tab::Mobile
        | Tab::Bluetooth
        | Tab::Vpn
        | Tab::Media
        | Tab::Phone
        | Tab::Gps
        | Tab::Notifications => None,
    }
}

pub(super) fn draw_item(p: &mut Painter, it: &SideItem, pressed: bool, active: bool) {
    if it.round {
        p.round_button(it.rect, it.icon, pressed, active);
    } else {
        p.icon_box(it.rect, it.icon, pressed, active);
    }
}
