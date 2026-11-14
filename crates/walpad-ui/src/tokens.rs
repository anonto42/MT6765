//! Design tokens (docs/rust/UI.md): the line-art theme. One charcoal background, white strokes and
//! text. "White at N %" stroke/text colours are pre-blended over the background at compile time
//! (opaque writes). Surfaces (popovers, cards, rows, buttons, icon boxes, sheets, the dock, the
//! keyboard, the mouse pad) get a translucent dark fill (`Tint`, owner request 2026-11-10) that is
//! alpha-blended over whatever is beneath (the wallpaper art), so a component always reads apart
//! from the background. No gradients, no shadows. Cards, row groups and the page surface carry no
//! outline (owner request 2026-11-12, `ui-polish`): the shade difference alone separates them, the
//! way grouped sections work in iOS/Material settings; outlines stay on controls that need an edge
//! to read as tappable or as a popover/sheet floating over arbitrary content (buttons, fields,
//! icon boxes, popovers, dialogs).

mod color;
#[cfg(test)]
mod tests;
mod theme;

pub use color::*;
pub use theme::*;

/// Spacing scale.
pub const SP: [i32; 7] = [4, 8, 12, 16, 24, 32, 48];
pub const SP_1: i32 = 4;
pub const SP_2: i32 = 8;
pub const SP_3: i32 = 12;
pub const SP_4: i32 = 16;
pub const SP_5: i32 = 24;
pub const SP_6: i32 = 32;
pub const SP_7: i32 = 48;

/// Corner radius: 0 px everywhere for the boxy geometric UI.
pub const RADIUS: i32 = 0;
pub const RADIUS_CARD: i32 = 0;
pub const RADIUS_ICON: i32 = 0;
/// Stroke width of outlines (px).
pub const OUTLINE_W: f32 = 1.5;

/// Minimum touch target and fixed control heights.
pub const TOUCH_MIN: i32 = 56;
pub const ROW_H: i32 = 64;
pub const ROW_WIFI_H: i32 = 72;
pub const BUTTON_H: i32 = 56;
pub const ICON: i32 = 24;

/// Frame layout: sidebar on the left, header across the main area.
pub const SIDEBAR_W: i32 = 96;
pub const HEADER_H: i32 = 64;
pub const CONTENT_PAD: i32 = 24;
pub const DOCK_H: i32 = 56;
/// Icon boxes: sidebar buttons, Home grid, Apps panel.
pub const BOX_DOCK: i32 = 56;
pub const BOX_GRID: i32 = 64;
pub const BOX_PANEL: i32 = 48;

/// Type sizes in px (faces in `text::Face`).
pub const SIZE_CLOCK: f32 = 20.0;
pub const SIZE_TITLE: f32 = 26.0;
pub const SIZE_SECTION: f32 = 13.0;
pub const SIZE_BODY: f32 = 17.0;
pub const SIZE_SECONDARY: f32 = 14.0;
pub const SIZE_CAPTION: f32 = 13.0;
pub const SIZE_LABEL: f32 = 12.0;
pub const SIZE_TINY: f32 = 11.0;
pub const SIZE_MONO: f32 = 15.0;

/// Interaction thresholds (ms / px).
pub const TAP_MAX_MS: u64 = 500;
pub const LONG_PRESS_MS: u64 = 500;
pub const SLOP_PX: i32 = 10;

// Compile-time guard: every row/button is at least a minimum touch target.
const _: () = assert!(
    ROW_H >= TOUCH_MIN && BUTTON_H >= TOUCH_MIN && ROW_WIFI_H >= TOUCH_MIN && BOX_DOCK >= TOUCH_MIN
);
