//! The two popovers: Quick Settings (anchored under the header pill, right) and the Apps panel
//! (anchored bottom-left next to the sidebar). Geometry is pure; rendering returns hit areas. Both
//! close on a tap outside: their hit list starts with a full-screen `Dismiss` and the panel body
//! swallows taps between rows (`PanelBody`).
mod apps_panel;
mod qs_model;
mod qs_render;
#[cfg(test)]
mod tests;

pub use apps_panel::{
    apps_panel_rect, cell_box, close_badge, panel_geom, render_apps_panel, running, PanelGeom,
    Tool, ToolAction, BADGE_HIT, BADGE_R, PANEL_W, TOOLS,
};
pub use qs_model::{
    battery_label, battery_row, battery_time_text, hotspot_row, qs_geom, wifi_mark, wifi_row,
    QsGeom, QS_W,
};
pub use qs_render::render_quick_settings;
