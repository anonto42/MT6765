//! Media screen (owner request 2026-11-11, change media-apps-capture): browse pictures, videos and
//! music by type (plus the recently opened), tap an item to open it in the matching player.
//! Model, folder scan, layout and drawing are pure (content coordinates, like vpn_screen.rs); the
//! slow work (folder scan, thumbnails with `walpad-thumb`) lives in `media_worker.rs`, the state
//! changes in `App`.

mod model;
mod render;
mod scan;

#[cfg(test)]
mod tests;

pub use model::MediaModel;
pub use render::{fmt_duration, fmt_size, layout, render, stem, visible_range, MediaLayout, View};
pub use scan::{read_recent, remember, scan};

use walpad_proto::Status;
use walpad_ui::{Canvas, Icon};

/// Thumbnail tile (square) and the gap between tiles; list row height for music and recent.
pub const TILE: i32 = 152;
pub const GAP: i32 = 12;
pub(crate) const NAME_H: i32 = 22;
pub(crate) const TILE_RADIUS: i32 = 0;
pub const LIST_ROW_H: i32 = 72;
pub(crate) const TITLE_H: i32 = 40;
pub(crate) const SEG_H: i32 = 52;
/// Bounds of the folder scan: items kept, directory entries looked at, depth below a root.
pub const MAX_ITEMS: usize = 600;
pub const MAX_VISITS: usize = 4000;
pub const MAX_DEPTH: usize = 3;
/// Decoded thumbnails kept in memory (about 100 KB each).
pub const MAX_THUMBS: usize = 160;
/// Recently opened files, newest first (shell-local state, one path per line).
pub const RECENT_FILE: &str = "/var/lib/walpad/media-recent";
pub const MAX_RECENT: usize = 30;

/// Folders searched, in this order (all are inside the roots the core allows to open).
pub const SCAN_DIRS: &[&str] = &[
    "/root/Pictures",
    "/root/DCIM",
    "/root/Videos",
    "/root/Movies",
    "/root/Music",
    "/root/Downloads",
    "/tmp/Downloads",
    "/mnt/sd",
];
/// Every folder directly in this one (USB disks) is searched as well.
pub const USB_PARENT: &str = "/mnt/usb";

const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "webp", "tif", "tiff"];
const VIDEO_EXT: &[&str] = &[
    "mp4", "mkv", "webm", "avi", "mov", "m4v", "mpg", "mpeg", "3gp", "flv", "wmv", "ogv", "ts",
];
const AUDIO_EXT: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "wav", "m4a", "aac", "wma",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Image,
    Video,
    Audio,
}

impl Kind {
    pub fn of(path: &str) -> Option<Kind> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        let e = ext.as_str();
        if IMAGE_EXT.contains(&e) {
            Some(Kind::Image)
        } else if VIDEO_EXT.contains(&e) {
            Some(Kind::Video)
        } else if AUDIO_EXT.contains(&e) {
            Some(Kind::Audio)
        } else {
            None
        }
    }
    pub fn icon(self) -> Icon {
        match self {
            Kind::Image => Icon::Image,
            Kind::Video => Icon::Media,
            Kind::Audio => Icon::Music,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Kind::Image => "Photo",
            Kind::Video => "Video",
            Kind::Audio => "Music",
        }
    }
    /// App ids that can open this kind, best first (all declare `"open"` in the catalogue).
    pub fn players(self) -> &'static [&'static str] {
        match self {
            Kind::Image => &["gpicview", "imv", "ristretto", "gthumb"],
            Kind::Video | Kind::Audio => &["mpv", "vlc"],
        }
    }
    /// What to install when no player is there.
    pub fn missing_hint(self) -> &'static str {
        match self {
            Kind::Image => "No photo viewer installed. Open Settings > Apps.",
            Kind::Video | Kind::Audio => {
                "No media player installed. Install it in Settings > Apps."
            }
        }
    }
}

/// The first of the kind's players that the core lists as installed.
pub fn player_for(kind: Kind, st: &Option<Status>) -> Option<&'static str> {
    let apps = &st.as_ref()?.apps;
    kind.players()
        .iter()
        .copied()
        .find(|id| apps.iter().any(|a| a.app == *id))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// Absolute path as the player and the core see it.
    pub path: String,
    pub name: String,
    pub kind: Kind,
    /// Seconds since the epoch (0 = unknown).
    pub mtime: u64,
    pub size: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Photos,
    Videos,
    Music,
    Recent,
}

impl Section {
    pub const ALL: [Section; 4] = [
        Section::Photos,
        Section::Videos,
        Section::Music,
        Section::Recent,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Section::Photos => "Photos",
            Section::Videos => "Videos",
            Section::Music => "Music",
            Section::Recent => "Recent",
        }
    }
    /// Photos and videos are shown as a grid of thumbnails, music and recent as a list.
    pub fn grid(self) -> bool {
        matches!(self, Section::Photos | Section::Videos)
    }
    pub(crate) fn kind(self) -> Option<Kind> {
        match self {
            Section::Photos => Some(Kind::Image),
            Section::Videos => Some(Kind::Video),
            Section::Music => Some(Kind::Audio),
            Section::Recent => None,
        }
    }
    pub(crate) fn empty_text(self) -> (&'static str, &'static str) {
        match self {
            Section::Photos => ("No photos found", "Pictures, DCIM, SD cards and USB disks"),
            Section::Videos => ("No videos found", "Videos, Movies, SD cards and USB disks"),
            Section::Music => ("No music found", "Music, Downloads, SD cards and USB disks"),
            Section::Recent => (
                "Nothing opened yet",
                "Files you open here show up in this list",
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaTarget {
    Section(usize),
    /// Index into the current section's list.
    Item(usize),
    Refresh,
}

/// What `ffprobe` told about a file (all optional).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Info {
    pub duration: Option<u32>,
    pub title: Option<String>,
    pub artist: Option<String>,
}

pub enum Thumb {
    /// Asked for, not there yet.
    Pending,
    /// The helper could not make one (no tool, unsupported, too large): the placeholder stays.
    Missing,
    Ready(Canvas),
}
