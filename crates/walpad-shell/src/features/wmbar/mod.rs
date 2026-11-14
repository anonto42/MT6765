//! Window bar (change multitasking-windows): a 48 px strip at the top of the visible area while an
//! app window is in front and either the app runs several windows (the terminal: one tab per
//! window, a "+" button, an x per tab) or the layout mode is Free (the bar is the title bar:
//! drag it to move the window, buttons snap it left / right / top / bottom / full) plus a
//! corner grip on the window that resizes it. Override-redirect windows with WM_HINTS
//! input=false (they never take the keyboard focus from the terminal, so the system keyboard
//! keeps typing into it) and a managed 1x1 `walpad-wmbar-strut` window that carries the top
//! strut, like the dock. Own X connection; every wait has a deadline (walpad-x). The decisions
//! are pure functions (tested); the core clamps every placement again.

mod events;
mod model;
mod refresh;
mod render;
#[cfg(test)]
mod tests;
mod window;
mod window_ops;

pub use model::{
    app_for, grip_rect, hit, layout, load_apps, strut_partial, tab_label, tab_width, wants_visible,
    AppInfo, BarModel, Hit, Tab, BAR_CLASS, BAR_H, CLOSE_W, GRIP, GRIP_CLASS, PLUS_W, SNAP_BUTTONS,
    SNAP_W, TAB_MAX_W, TAB_MIN_W,
};
pub use render::{render, render_grip};
pub use window::WmBar;
