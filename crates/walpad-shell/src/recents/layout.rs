use super::Ms;
use walpad_ui::tokens::*;
use walpad_ui::{Canvas, Icon};

pub const GAP: i32 = SP_5;
/// Card padding around the thumbnail.
pub const PAD: i32 = SP_2;
/// Label strip under the thumbnail (icon, name, close button).
pub const LABEL_H: i32 = 56;
/// Hit size of the close button in the label strip.
pub const CLOSE_BTN: i32 = 48;
/// A card dragged up this far (px) and released is closed.
pub const CLOSE_DRAG_PX: i32 = 96;
/// A close request that gets no answer in this long puts the card back.
pub const CLOSE_TIMEOUT_MS: Ms = 3000;
/// Cards that fit side by side before the strip scrolls.
pub const FIT: i32 = 3;

pub struct Card {
    pub app: String,
    pub name: String,
    pub icon: Icon,
    /// Last window capture (None: the icon is drawn instead).
    pub thumb: Option<Canvas>,
}

/// What the strip asks the shell to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Focus this app and close the strip.
    Focus(String),
    /// Send `close_app` for this app (the card is hidden until the answer comes).
    Close(String),
    /// Close the strip.
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    pub card_w: i32,
    pub thumb_w: i32,
    pub thumb_h: i32,
    pub card_h: i32,
    pub top: i32,
    /// x of the first card at scroll 0 (centres the row when it fits).
    pub left: i32,
    pub content_w: i32,
    pub max_scroll: i32,
}

/// Card sizes for `n` cards on a visible area: up to three side by side (one or two get larger
/// cards, never wider than half the screen), the thumbnail keeps the screen's aspect.
pub fn geometry(area: (i32, i32), n: usize) -> Geometry {
    let n = n.max(1) as i32;
    let slots = n.min(FIT);
    let card_w = ((area.0 - (slots + 1) * GAP) / slots).min(area.0 / 2);
    let thumb_w = card_w - 2 * PAD;
    let thumb_h = thumb_w * area.1 / area.0;
    let card_h = PAD + thumb_h + LABEL_H;
    let content_w = n * card_w + (n + 1) * GAP;
    let max_scroll = (content_w - area.0).max(0);
    Geometry {
        card_w,
        thumb_w,
        thumb_h,
        card_h,
        top: ((area.1 - card_h) / 2 - SP_5).max(SP_5),
        left: ((area.0 - content_w) / 2).max(0) + GAP,
        content_w,
        max_scroll,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Drag {
    None,
    /// Undecided: the first movement picks the axis.
    Start(Option<usize>),
    Vertical(usize),
    Horizontal,
}
