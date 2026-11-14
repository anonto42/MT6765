//! System dock (decision D3): a 56 px strip with Home, Keyboard and Close app for the app in front
//! (Firefox, the terminal, ...), so those apps can always reach Home by touch.
//!
//! Collapsible (owner request 2026-11-10): while a non-shell window is `_NET_ACTIVE_WINDOW` only a
//! small arrow handle (WM_CLASS `walpad-dock-handle`, 56x28, rounded top corners) sits at the
//! right end of the bottom edge, without a strut, so the app keeps its whole height. A tap on the
//! handle expands the strip (WM_CLASS `walpad-dock`) with a bottom strut (a managed 1x1
//! `walpad-dock-strut` window) and turns the arrow down; the strip collapses again after an
//! action, after `AUTO_COLLAPSE_MS` without a tap, or on a second tap on the handle. Both windows
//! are override-redirect with WM_HINTS input=false, so they never take the focus. While the system
//! keyboard or the mouse pad is up, handle and strip sit directly above it. The dock has its own
//! X connection; every wait on it has a deadline (walpad-x).

mod events;
mod layout;
mod render;
mod state;
#[cfg(test)]
mod tests;
mod window;
mod window_ops;

pub use layout::{
    button_at, button_rects, clamp_handle_pos, default_handle_pos, dock_rect, dock_rect_for_handle,
    handle_rect, handle_shape, strut_partial, visible_area, DockButton, AUTO_COLLAPSE_MS, BUTTONS,
    DOCK_CELL_W, DOCK_H, DOCK_W, HANDLE_CLASS, HANDLE_H, HANDLE_MARGIN, HANDLE_RADIUS, HANDLE_W,
    SHELL_TITLE, WM_CLASS_CLASS, WM_CLASS_INSTANCE,
};
pub use render::{render, render_handle};
pub use state::{app_for_class, command_for, load_wm_classes, wants_visible, Fold};
pub use window::Dock;
