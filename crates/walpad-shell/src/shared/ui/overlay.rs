//! Modal overlays: the Wi-Fi password sheet (with the on-screen keyboard), the app options sheet,
//! the forget-network and power dialogs, the power menu, and the two popovers (quick settings and
//! the Apps panel, drawn by `panels`). Geometry is pure so hit areas are testable.
mod render;
mod sheet;
#[cfg(test)]
mod tests;
mod types;

pub use render::{
    app_sheet_geom, key_at, password_geom, render_app_sheet, render_forget, render_password,
    render_power_confirm, render_power_menu, PasswordGeom, PASSWORD_SHEET_H,
};
pub use sheet::{dns_list, normalize_mac, PasswordSheet};
pub use types::{AppSheet, Overlay, SheetPurpose, POWER_CANCEL, POWER_ITEMS};
