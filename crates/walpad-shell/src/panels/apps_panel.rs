use crate::app::{Model, Target, Tile};
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Color, Face, Icon, Rect, Style};

pub const PANEL_W: i32 = 320;
const PANEL_PAD: i32 = 20;
const PANEL_COLS: i32 = 4;
const CELL_W: i32 = (PANEL_W - 2 * PANEL_PAD) / PANEL_COLS;
const CELL_H: i32 = BOX_PANEL + 6 + 18;
const CELL_GAP: i32 = SP_3;
const CAPTION_H: i32 = 16;
/// Close badge on a running app: drawn 20 px round, tapped through a 40x40 rectangle.
pub const BADGE_R: i32 = 10;
pub const BADGE_HIT: i32 = 40;

/// What a tool in the Apps panel's TOOLS grid does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolAction {
    /// Open the mouse pad (a touchpad over the apps: mousepad.rs).
    MousePad,
    /// Show the clipboard history (seltools.rs).
    Clipboard,
    /// Open the app store at the catalogue (Settings > Apps).
    AppStore,
    /// Open the Media screen (pictures, videos, music).
    Media,
    /// Open the Phone screen (owner request 2026-11-11); the tile carries the unread badge.
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub icon: Icon,
    pub label: &'static str,
    pub action: ToolAction,
}

/// The TOOLS section, in grid order. Add a tool here (and its action in `App::on_tap`).
pub const TOOLS: [Tool; 5] = [
    Tool {
        icon: Icon::Mouse,
        label: "Mouse pad",
        action: ToolAction::MousePad,
    },
    Tool {
        icon: Icon::Clipboard,
        label: "Clipboard",
        action: ToolAction::Clipboard,
    },
    Tool {
        icon: Icon::Apps,
        label: "App store",
        action: ToolAction::AppStore,
    },
    Tool {
        icon: Icon::Media,
        label: "Media",
        action: ToolAction::Media,
    },
    Tool {
        icon: Icon::Phone,
        label: "Phone",
        action: ToolAction::Phone,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanelGeom {
    pub panel: Rect,
    pub active_label: Rect,
    pub active: Vec<Rect>,
    /// "No apps running" line when nothing runs.
    pub active_empty: Option<Rect>,
    pub divider: i32,
    pub tools_label: Rect,
    pub tools: Vec<Rect>,
}

fn grid_h(n: usize) -> i32 {
    let rows = (n as i32 + PANEL_COLS - 1) / PANEL_COLS;
    rows * CELL_H + (rows - 1).max(0) * CELL_GAP
}

/// The icon box of a panel cell (48 px, centred at the top of the cell).
pub fn cell_box(cell: Rect) -> Rect {
    Rect::new(
        cell.x + (cell.w - BOX_PANEL) / 2,
        cell.y,
        BOX_PANEL,
        BOX_PANEL,
    )
}

/// The close badge of a running app's cell: (drawn circle bounds, enlarged hit rectangle). The
/// badge sits on the icon box's top-right corner; the hit rectangle stays clear of the box centre,
/// so a tap on the icon itself still focuses the app.
pub fn close_badge(cell: Rect) -> (Rect, Rect) {
    let b = cell_box(cell);
    let (cx, cy) = (b.right() - 2, b.y + 2);
    (
        Rect::new(cx - BADGE_R, cy - BADGE_R, 2 * BADGE_R, 2 * BADGE_R),
        Rect::new(cx - BADGE_HIT / 2, cy - BADGE_HIT / 2, BADGE_HIT, BADGE_HIT),
    )
}

/// Bottom-left flush against the sidebar and bottom edge.
pub fn panel_geom(frame: &Frame, running: usize, tools: usize) -> PanelGeom {
    let active_h = if running == 0 { 24 } else { grid_h(running) };
    let h = PANEL_PAD
        + CAPTION_H
        + SP_3
        + active_h
        + SP_4
        + 1
        + SP_4
        + CAPTION_H
        + SP_3
        + grid_h(tools)
        + PANEL_PAD;
    let x = SIDEBAR_W;
    let top = (frame.h - h).max(HEADER_H);
    let ix = x + PANEL_PAD;
    let cells = |y0: i32, n: usize| -> Vec<Rect> {
        (0..n as i32)
            .map(|i| {
                Rect::new(
                    ix + (i % PANEL_COLS) * CELL_W,
                    y0 + (i / PANEL_COLS) * (CELL_H + CELL_GAP),
                    CELL_W,
                    CELL_H,
                )
            })
            .collect()
    };
    let mut y = top + PANEL_PAD;
    let active_label = Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, CAPTION_H);
    y += CAPTION_H + SP_3;
    let active = cells(y, running);
    let active_empty = (running == 0).then(|| Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, 24));
    y += active_h + SP_4;
    let divider = y;
    y += 1 + SP_4;
    let tools_label = Rect::new(ix, y, PANEL_W - 2 * PANEL_PAD, CAPTION_H);
    y += CAPTION_H + SP_3;
    PanelGeom {
        panel: Rect::new(x, top, PANEL_W, h),
        active_label,
        active,
        active_empty,
        divider,
        tools_label,
        tools: cells(y, tools),
    }
}

/// Indices into `apps` of the running ones.
pub fn running(apps: &[Tile]) -> Vec<usize> {
    apps.iter()
        .enumerate()
        .filter(|(_, t)| t.running)
        .map(|(i, _)| i)
        .collect()
}

/// A panel cell: the 48 px icon box (translucent `SURFACE_ICON` over the panel fill) and its name.
fn panel_icon(p: &mut Painter, cell: Rect, icon: Icon, name: &str, pressed: bool) {
    p.app_icon(cell, BOX_PANEL, icon, name, pressed, false);
}

fn panel_caption(p: &mut Painter, s: &str, r: Rect) {
    p.caption_upper(s, r);
}

/// Square close badge: overlaps the icon box's corner with a dark fill, no outline.
fn paint_close_badge(p: &mut Painter, r: Rect, pressed: bool) {
    p.cv.rect(r, Tint::new(Color::hex(0x000000), 80));
    if pressed {
        p.cv.rect(r, pressed_tint());
    }
    p.cv.icon(Icon::Close, r.x + 4, r.y + 4, r.w - 8, text());
}

/// The Apps panel's outline rectangle for the current model.
pub fn apps_panel_rect(frame: &Frame, m: &Model) -> Rect {
    panel_geom(frame, running(&m.apps).len(), TOOLS.len()).panel
}

pub fn render_apps_panel(p: &mut Painter, frame: &Frame, m: &Model) -> Vec<(Rect, Target)> {
    let run = running(&m.apps);
    let g = panel_geom(frame, run.len(), TOOLS.len());
    let mut hits = vec![
        (frame.screen(), Target::Dismiss),
        (g.panel, Target::PanelBody),
    ];
    // translucent (owner request 2026-11-10): SURFACE_STRONG over the wallpaper, no scrim
    p.popover(g.panel);
    panel_caption(p, "Active apps", g.active_label);
    if let Some(r) = g.active_empty {
        p.text(
            "No apps running",
            Style::new(Face::Regular, SIZE_CAPTION, text_disabled()),
            r,
            Align::Left,
        );
    }
    for (cell, &i) in g.active.iter().zip(&run) {
        let t = &m.apps[i];
        let target = Target::PanelActive(i);
        panel_icon(p, *cell, t.icon, &t.name, m.pressed == Some(target));
        hits.push((*cell, target));
    }
    // the badges after every icon: their hit rectangles overlap the neighbouring cell and must win
    // the hit test (it searches from the end)
    for (cell, &i) in g.active.iter().zip(&run) {
        let (badge, hit) = close_badge(*cell);
        paint_close_badge(p, badge, m.pressed == Some(Target::PanelClose(i)));
        hits.push((hit, Target::PanelClose(i)));
    }
    p.divider(g.panel.x, g.divider, PANEL_W);
    panel_caption(p, "Tools", g.tools_label);
    for (i, (cell, t)) in g.tools.iter().zip(TOOLS.iter()).enumerate() {
        let target = Target::PanelTool(i);
        panel_icon(p, *cell, t.icon, t.label, m.pressed == Some(target));
        if t.action == ToolAction::Phone {
            crate::phone_ui::paint_badge(p, cell_box(*cell), crate::phone_ui::unread(&m.status));
        }
        hits.push((*cell, target));
    }
    hits
}
