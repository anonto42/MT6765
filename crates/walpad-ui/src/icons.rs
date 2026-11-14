//! Vector icons on a 24 px grid with 2 px round strokes, drawn in code (no image assets).
use crate::canvas::Canvas;
use crate::tokens::Color;
use tiny_skia::{Path, Transform};

mod glyphs_a;
mod glyphs_b;
mod pen;
#[cfg(test)]
mod tests;

use pen::Pen;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Home,
    Apps,
    Wifi,
    Device,
    Settings,
    Globe,
    Terminal,
    Folder,
    Keyboard,
    Sun,
    Rotate,
    Power,
    Battery,
    Lock,
    ChevronRight,
    ChevronDown,
    Bluetooth,
    Check,
    Close,
    Refresh,
    Screen,
    Back,
    Warning,
    Backspace,
    Shift,
    Eye,
    EyeOff,
    Spinner,
    /// The mouse pad tool (Apps panel > Tools).
    Mouse,
    /// SIM card (mobile network, owner request 2026-11-10).
    Sim,
    /// Speaker with sound waves / crossed out (volume, owner request 2026-11-10).
    Volume,
    VolumeOff,
    /// Clipboard (Apps panel > Tools).
    Clipboard,
    /// Media screen (change media-apps-capture): film frame with a play triangle, a picture, a note.
    Media,
    Image,
    Music,
    /// Phone handset, speech bubble, person, microphone (muted), dial pad (owner request
    /// 2026-11-11, `phone-dialer`).
    Phone,
    Message,
    Person,
    Mic,
    MicOff,
    Keypad,
    /// Map pin (GPS, owner request 2026-11-11).
    Location,
    /// Lightning bolt: shown next to the battery percent while charging (owner request 2026-11-11).
    Bolt,
    /// Notification centre bell (owner request 2026-11-12, change `device-notifications`).
    Bell,
    /// Code editor (owner request 2026-11-12, "an ide ... geany or lite xl"): angle brackets
    /// with a slash, `</>`.
    Code,
    /// Selection toolbar (owner report 2026-11-13, "copy paste functions are not
    /// profashonalize like normal android devices"): icons instead of the word "Cut".
    Cut,
    /// Two overlapping sheets, same request as `Cut`.
    Copy,
    /// Four corner brackets (a crop/viewfinder shape), same request as `Cut`: "Select all".
    SelectAll,
    /// A left-curling arrow, same request as `Cut`: "Undo".
    Undo,
}

impl Icon {
    pub const ALL: [Icon; 50] = [
        Icon::Home,
        Icon::Apps,
        Icon::Wifi,
        Icon::Device,
        Icon::Settings,
        Icon::Globe,
        Icon::Terminal,
        Icon::Folder,
        Icon::Keyboard,
        Icon::Sun,
        Icon::Rotate,
        Icon::Power,
        Icon::Battery,
        Icon::Lock,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::Bluetooth,
        Icon::Check,
        Icon::Close,
        Icon::Refresh,
        Icon::Screen,
        Icon::Back,
        Icon::Warning,
        Icon::Backspace,
        Icon::Shift,
        Icon::Eye,
        Icon::EyeOff,
        Icon::Spinner,
        Icon::Mouse,
        Icon::Sim,
        Icon::Volume,
        Icon::VolumeOff,
        Icon::Clipboard,
        Icon::Media,
        Icon::Image,
        Icon::Music,
        Icon::Phone,
        Icon::Message,
        Icon::Person,
        Icon::Mic,
        Icon::MicOff,
        Icon::Keypad,
        Icon::Location,
        Icon::Bolt,
        Icon::Bell,
        Icon::Code,
        Icon::Cut,
        Icon::Copy,
        Icon::SelectAll,
        Icon::Undo,
    ];

    /// Icon id used in apps.json (`"icon": "globe"`).
    pub fn from_name(n: &str) -> Option<Icon> {
        Some(match n {
            "home" => Icon::Home,
            "apps" => Icon::Apps,
            "wifi" => Icon::Wifi,
            "device" => Icon::Device,
            "settings" => Icon::Settings,
            "globe" => Icon::Globe,
            "terminal" => Icon::Terminal,
            "folder" | "files" => Icon::Folder,
            "keyboard" => Icon::Keyboard,
            "bluetooth" => Icon::Bluetooth,
            "mouse" => Icon::Mouse,
            "sim" => Icon::Sim,
            "volume" => Icon::Volume,
            "clipboard" => Icon::Clipboard,
            "location" | "gps" => Icon::Location,
            "bolt" | "charging" => Icon::Bolt,
            "bell" | "notifications" => Icon::Bell,
            "media" | "video" => Icon::Media,
            "image" | "photos" => Icon::Image,
            "music" => Icon::Music,
            "phone" => Icon::Phone,
            "message" | "messages" => Icon::Message,
            "code" | "editor" => Icon::Code,
            _ => return None,
        })
    }
}

fn build(icon: Icon) -> (Option<Path>, Option<Path>) {
    let mut p = Pen::new();
    if !glyphs_a::build_a(icon, &mut p) {
        glyphs_b::build_b(icon, &mut p);
    }
    (p.stroke.finish(), p.dots.finish())
}

/// Draw `icon` with its top-left at (x, y), `size` px square (24 = native).
pub fn draw_icon(cv: &mut Canvas, icon: Icon, x: i32, y: i32, size: i32, color: Color) {
    draw_icon_rotated(cv, icon, x, y, size, color, 0.0);
}

/// Same, rotated by `angle_deg` around the icon centre (spinners).
pub fn draw_icon_rotated(
    cv: &mut Canvas,
    icon: Icon,
    x: i32,
    y: i32,
    size: i32,
    color: Color,
    angle_deg: f32,
) {
    let s = size as f32 / 24.0;
    let t = Transform::from_rotate_at(angle_deg, 12.0, 12.0)
        .post_scale(s, s)
        .post_translate(x as f32, y as f32);
    let (stroke, dots) = build(icon);
    if let Some(path) = stroke {
        // 2 px at the native 24 px size; grows slowly so large icons do not turn into blobs
        cv.stroke(&path, (1.0 + s) / s, color, t);
    }
    if let Some(path) = dots {
        cv.fill_path(&path, color, t);
    }
}

impl Canvas {
    pub fn icon(&mut self, icon: Icon, x: i32, y: i32, size: i32, color: Color) {
        draw_icon(self, icon, x, y, size, color);
    }
    pub fn icon_rotated(
        &mut self,
        icon: Icon,
        x: i32,
        y: i32,
        size: i32,
        color: Color,
        angle_deg: f32,
    ) {
        draw_icon_rotated(self, icon, x, y, size, color, angle_deg);
    }
}
