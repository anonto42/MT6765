//! Application state: what the shell shows and how taps, core events and replies change it.
//! Pure of I/O: the shell loop feeds it gestures and core events and sends the commands it returns.

mod cmd;
mod cmd_request;
mod config;
mod core_events;
mod core_notices;
mod core_replies;
mod gesture;
mod lock_flow;
mod lock_ops;
mod model;
mod phone_flow;
mod phone_tap;
mod power_menu;
mod render;
mod render_scrolled;
mod setup;
mod sheets;
mod tap;
mod tap_connectivity;
mod tap_device;
mod tap_network;
mod target;
mod tick;

use crate::chrome::{self, Chrome};
use std::collections::HashMap;
use walpad_ui::components::Tab;
use walpad_ui::layout::Frame;
use walpad_ui::Rect;

pub use cmd::{Cmd, Outcome, Purpose};
pub use config::{
    load_catalog, read_clock_24h, read_pinned, read_theme, read_wallpaper, read_wallpaper_path,
    AppEntry, WallTime, DEFAULT_PINNED,
};
pub use lock_flow::{lock_rows, LockAct, LockFlow};
pub use model::{Model, Ms, Toast};
pub use target::{Target, Tile};

pub struct App {
    pub frame: Frame,
    /// Cached background layer (charcoal, watermark, sidebar at rest).
    pub chrome: Chrome,
    pub active: Tab,
    pub model: Model,
    catalog: Vec<AppEntry>,
    /// Hit areas in screen coordinates (sidebar, header pill, Home grid).
    hits: Vec<(Rect, Target)>,
    /// Hit areas of scrollable screens, in content coordinates.
    content_hits: Vec<(Rect, Target)>,
    overlay_hits: Vec<(Rect, Target)>,
    pub scroll: i32,
    scroll_vel: f32,
    scroll_drag_ms: Ms,
    scroll_fling_at: Ms,
    content_h: i32,
    view: Rect,
    last_slider_send: Ms,
    last_spin: Ms,
    animate_transitions: bool,
    tab_anim_start: Ms,
    tab_anim_until: Ms,
    overlay_anim_start: Ms,
    overlay_anim_until: Ms,
    anim_next: Ms,
    pending: HashMap<u64, Purpose>,
    now: Ms,
    /// Commands produced by time (tick), drained by the event loop with `take_cmds`.
    queued: Vec<Cmd>,
}

#[cfg(test)]
pub(crate) mod tests;
