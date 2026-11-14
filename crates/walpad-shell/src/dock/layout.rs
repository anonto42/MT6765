use walpad_ui::tokens::SP_2;
use walpad_ui::{Icon, Rect};

pub const DOCK_H: i32 = 56;
/// Width of one icon-only button cell in the expanded dock strip.
pub const DOCK_CELL_W: i32 = DOCK_H;
/// Total width of the compact icon-only dock strip.
pub const DOCK_W: i32 = BUTTONS.len() as i32 * DOCK_CELL_W;
pub const WM_CLASS_INSTANCE: &str = "walpad-dock";
pub const WM_CLASS_CLASS: &str = "WalPad-Dock";
pub const HANDLE_CLASS: &str = "walpad-dock-handle";
/// The collapsed dock: an arrow handle, touch-wide, half a dock high.
pub const HANDLE_W: i32 = 56;
pub const HANDLE_H: i32 = 28;
/// Gap between the handle and the right edge of the visible area.
pub const HANDLE_MARGIN: i32 = SP_2;
/// Corner radius of the handle (0 px: sharp boxy handle).
pub const HANDLE_RADIUS: i32 = 0;
/// Movement (px) before a press on the handle becomes a drag instead of a tap.
pub(super) const HANDLE_DRAG_SLOP: i32 = 8;
/// The expanded strip folds back after this long without a tap.
pub const AUTO_COLLAPSE_MS: u64 = 5000;
/// Title of the shell window (what Home raises).
pub const SHELL_TITLE: &str = crate::window::TITLE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockButton {
    Back,
    Home,
    Keyboard,
    CloseApp,
}

pub const BUTTONS: [(DockButton, &str, Icon); 4] = [
    (DockButton::Back, "Back", Icon::Back),
    (DockButton::Home, "Home", Icon::Home),
    (DockButton::Keyboard, "Keyboard", Icon::Keyboard),
    (DockButton::CloseApp, "Close app", Icon::Close),
];

/// The visible area for a shell window of this size. The shell shrinks by the dock's own strut
/// while the dock is up (a maximised window respects struts), so the orientation is taken from the
/// aspect and the full panel size from CONTRACTS (800x1280 / 1280x800), never from the shrunk size.
pub fn visible_area(shell_w: i32, shell_h: i32) -> (i32, i32) {
    if shell_w > shell_h {
        (1280, 800)
    } else {
        (800, 1280)
    }
}

/// Clamp a handle top-left position so the handle stays completely inside the full visible area.
pub fn clamp_handle_pos(pos: (i32, i32), area: (i32, i32), _kbd_h: i32) -> (i32, i32) {
    let max_x = (area.0 - HANDLE_W).max(0);
    let max_y = (area.1 - HANDLE_H).max(0);
    (pos.0.clamp(0, max_x), pos.1.clamp(0, max_y))
}

/// Default handle position: right side, at the bottom edge of the visible area (above the
/// keyboard / mouse pad while one is up).
pub fn default_handle_pos(area: (i32, i32), kbd_h: i32) -> (i32, i32) {
    let max_y = (area.1 - kbd_h - HANDLE_H).max(0);
    ((area.0 - HANDLE_W - HANDLE_MARGIN).max(0), max_y)
}

/// Compact icon-only dock strip positioned next to `handle` (above it when there is room, else
/// below it), clamped inside `area`.
pub fn dock_rect_for_handle(area: (i32, i32), handle: Rect) -> Rect {
    let x = (handle.right() - DOCK_W).clamp(0, (area.0 - DOCK_W).max(0));
    let y = if handle.y >= DOCK_H {
        handle.y - DOCK_H
    } else {
        handle.bottom().min((area.1 - DOCK_H).max(0))
    };
    Rect::new(x, y, DOCK_W, DOCK_H)
}

/// The dock window at the default right-side handle position.
pub fn dock_rect(area: (i32, i32), kbd_h: i32) -> Rect {
    let (hx, hy) = default_handle_pos(area, kbd_h);
    dock_rect_for_handle(area, Rect::new(hx, hy, HANDLE_W, HANDLE_H))
}

/// The handle at the default right-side position.
pub fn handle_rect(area: (i32, i32), kbd_h: i32, _expanded: bool) -> Rect {
    let (x, y) = default_handle_pos(area, kbd_h);
    Rect::new(x, y, HANDLE_W, HANDLE_H)
}

/// The handle's outline as rectangles (SHAPE bounding region, handle-local): sharp rectangular box.
pub fn handle_shape() -> Vec<Rect> {
    vec![Rect::new(0, 0, HANDLE_W, HANDLE_H)]
}

/// `_NET_WM_STRUT_PARTIAL`: struts count from the screen edge, so the part of the root below the
/// visible area is included (landscape: 480 + 56), and so is the keyboard under the dock (the
/// combined strut; the keyboard's own strut is smaller, the window manager takes the largest).
pub fn strut_partial(area: (i32, i32), kbd_h: i32, root_h: i32) -> [u32; 12] {
    crate::keyboard::bottom_strut(dock_rect(area, kbd_h), root_h)
}

/// Hit areas of the buttons (equal cells of the strip, strip-local coordinates).
pub fn button_rects(w: i32) -> [(Rect, DockButton); BUTTONS.len()] {
    let n = BUTTONS.len() as i32;
    let cell = w / n;
    let mut out = [(Rect::new(0, 0, 0, 0), DockButton::Home); BUTTONS.len()];
    for (i, (b, _, _)) in BUTTONS.iter().enumerate() {
        let x = i as i32 * cell;
        let cw = if i as i32 == n - 1 {
            w - (n - 1) * cell
        } else {
            cell
        };
        out[i] = (Rect::new(x, 0, cw, DOCK_H), *b);
    }
    out
}

pub fn button_at(w: i32, x: i32, y: i32) -> Option<DockButton> {
    button_rects(w)
        .iter()
        .find(|(r, _)| r.contains(x, y))
        .map(|(_, b)| *b)
}
