//! Media screen (owner request 2026-11-11, change media-apps-capture): browse pictures, videos and
//! music by type (plus the recently opened), tap an item to open it in the matching player.
//! Model, folder scan, layout and drawing are pure (content coordinates, like vpn_screen.rs); the
//! slow work (folder scan, thumbnails with `walpad-thumb`) lives in `media_worker.rs`, the state
//! changes in `App`.
use crate::app::Target;
use std::collections::{HashMap, HashSet};
use walpad_hw::Root;
use walpad_proto::Status;
use walpad_ui::components::Painter;
use walpad_ui::layout::Frame;
use walpad_ui::tokens::*;
use walpad_ui::{Align, Canvas, Color, Face, Icon, Rect, Style};

/// Thumbnail tile (square) and the gap between tiles; list row height for music and recent.
pub const TILE: i32 = 152;
pub const GAP: i32 = 12;
const NAME_H: i32 = 22;
const TILE_RADIUS: i32 = 0;
pub const LIST_ROW_H: i32 = 72;
const TITLE_H: i32 = 40;
const SEG_H: i32 = 52;
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
/// Folders never entered (Android and Windows housekeeping on cards).
const SKIP_DIRS: &[&str] = &[
    "Android",
    "LOST.DIR",
    "System Volume Information",
    "$RECYCLE.BIN",
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
    fn kind(self) -> Option<Kind> {
        match self {
            Section::Photos => Some(Kind::Image),
            Section::Videos => Some(Kind::Video),
            Section::Music => Some(Kind::Audio),
            Section::Recent => None,
        }
    }
    fn empty_text(self) -> (&'static str, &'static str) {
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

pub struct MediaModel {
    pub section: Section,
    pub items: Vec<Item>,
    pub scanned: bool,
    pub scanning: bool,
    pub recent: Vec<String>,
    pub thumbs: HashMap<String, Thumb>,
    pub infos: HashMap<String, Info>,
    info_asked: HashSet<String>,
    /// Paths the screen wants thumbnails / facts for (drained by `App::tick`).
    pub want_thumbs: Vec<String>,
    pub want_info: Vec<String>,
    order: Vec<String>,
}

impl Default for MediaModel {
    fn default() -> Self {
        MediaModel {
            section: Section::Photos,
            items: Vec::new(),
            scanned: false,
            scanning: false,
            recent: Vec::new(),
            thumbs: HashMap::new(),
            infos: HashMap::new(),
            info_asked: HashSet::new(),
            want_thumbs: Vec::new(),
            want_info: Vec::new(),
            order: Vec::new(),
        }
    }
}

impl MediaModel {
    /// The current section's items (Recent: the opened files that still make sense, newest first).
    pub fn list(&self) -> Vec<Item> {
        match self.section.kind() {
            Some(k) => self.items.iter().filter(|i| i.kind == k).cloned().collect(),
            None => self
                .recent
                .iter()
                .filter_map(|p| {
                    self.items
                        .iter()
                        .find(|i| &i.path == p)
                        .cloned()
                        .or_else(|| {
                            Some(Item {
                                path: p.clone(),
                                name: p.rsplit('/').next()?.to_string(),
                                kind: Kind::of(p)?,
                                mtime: 0,
                                size: 0,
                            })
                        })
                })
                .collect(),
        }
    }

    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        self.scanned = true;
        self.scanning = false;
    }

    pub fn set_recent(&mut self, recent: Vec<String>) {
        self.recent = recent;
    }

    /// A thumbnail arrived (PNG bytes, or None when the helper could not make one). The picture
    /// is scaled to the tile size once, here.
    pub fn on_thumb(&mut self, path: &str, png: Option<&[u8]>) {
        let t = png
            .and_then(Canvas::from_png)
            .and_then(|c| c.scaled(TILE as u32, TILE as u32))
            .map_or(Thumb::Missing, Thumb::Ready);
        if matches!(t, Thumb::Ready(_)) {
            self.order.retain(|p| p != path);
            self.order.push(path.to_string());
            while self.order.len() > MAX_THUMBS {
                let old = self.order.remove(0);
                // forgotten thumbnails are asked for again when they scroll into view
                self.thumbs.remove(&old);
            }
        }
        self.thumbs.insert(path.to_string(), t);
    }

    /// Forget that `path` was asked for (its job was dropped): it is asked for again when visible.
    pub fn forget(&mut self, path: &str) {
        if matches!(self.thumbs.get(path), Some(Thumb::Pending)) {
            self.thumbs.remove(path);
        }
        if !self.infos.contains_key(path) {
            self.info_asked.remove(path);
        }
    }

    pub fn on_info(&mut self, path: &str, info: Info) {
        self.infos.insert(path.to_string(), info);
    }

    /// Note what the visible part of the list needs. `first..last` is the item index range on or
    /// near the screen.
    pub fn note_visible(&mut self, list: &[Item], first: usize, last: usize) {
        for it in list.iter().skip(first).take(last.saturating_sub(first) + 1) {
            let wants_thumb = it.kind != Kind::Audio || self.section.grid();
            if wants_thumb && !self.thumbs.contains_key(&it.path) {
                self.thumbs.insert(it.path.clone(), Thumb::Pending);
                self.want_thumbs.push(it.path.clone());
            }
            let wants_info = it.kind != Kind::Image;
            if wants_info && self.info_asked.insert(it.path.clone()) {
                self.want_info.push(it.path.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------- recent list file
pub fn read_recent(root: &Root) -> Vec<String> {
    std::fs::read_to_string(root.p(RECENT_FILE))
        .map(|s| {
            s.lines()
                .filter(|l| walpad_proto::valid_open_path(l))
                .take(MAX_RECENT)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Put `path` first (no duplicates) and save; returns the new list.
pub fn remember(root: &Root, path: &str) -> Vec<String> {
    let mut v = read_recent(root);
    v.retain(|p| p != path);
    v.insert(0, path.to_string());
    v.truncate(MAX_RECENT);
    let file = root.p(RECENT_FILE);
    if let Some(d) = file.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = file.with_extension("tmp");
    if std::fs::write(&tmp, v.join("\n") + "\n").is_ok() {
        let _ = std::fs::rename(&tmp, &file);
    }
    v
}

// ---------------------------------------------------------------------- folder scan
/// Look for media in the user folders, SD card and USB disks. Bounded: at most `MAX_VISITS`
/// directory entries and `MAX_ITEMS` items (the newest), depth `MAX_DEPTH`, no symlinks followed,
/// hidden entries skipped. Newest first.
pub fn scan(root: &Root) -> Vec<Item> {
    let mut dirs: Vec<String> = SCAN_DIRS.iter().map(|s| s.to_string()).collect();
    if let Ok(rd) = std::fs::read_dir(root.p(USB_PARENT)) {
        let mut usb: Vec<String> = rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.'))
            .map(|n| format!("{USB_PARENT}/{n}"))
            .collect();
        usb.sort();
        dirs.extend(usb);
    }
    let mut out: Vec<Item> = Vec::new();
    let mut seen = HashSet::new();
    let mut visits = 0usize;
    // (virtual path, depth)
    let mut stack: Vec<(String, usize)> = dirs.into_iter().rev().map(|d| (d, 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(root.p(&dir)) else {
            continue;
        };
        let mut subs = Vec::new();
        for e in rd.filter_map(Result::ok) {
            visits += 1;
            if visits > MAX_VISITS {
                break;
            }
            let Ok(name) = e.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            let vpath = format!("{dir}/{name}");
            if ft.is_dir() {
                if depth < MAX_DEPTH && !SKIP_DIRS.contains(&name.as_str()) {
                    subs.push((vpath, depth + 1));
                }
            } else if ft.is_file() {
                let Some(kind) = Kind::of(&name) else {
                    continue;
                };
                if !walpad_proto::valid_open_path(&vpath) || !seen.insert(vpath.clone()) {
                    continue;
                }
                let md = e.metadata().ok();
                out.push(Item {
                    mtime: md
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_secs()),
                    size: md.as_ref().map_or(0, |m| m.len()),
                    path: vpath,
                    name,
                    kind,
                });
            }
        }
        subs.sort();
        stack.extend(subs.into_iter().rev());
        if visits > MAX_VISITS {
            break;
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.name.cmp(&b.name)));
    out.truncate(MAX_ITEMS);
    out
}

// ---------------------------------------------------------------------- text helpers
/// "3:07" / "1:02:09".
pub fn fmt_duration(s: u32) -> String {
    let (h, m, sec) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

pub fn fmt_size(b: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if b >= 1024 * MB {
        format!("{:.1} GB", b as f64 / (1024.0 * MB as f64))
    } else if b >= MB {
        format!("{} MB", b / MB)
    } else {
        format!("{} KB", (b / 1024).max(1))
    }
}

/// File name without its extension, for list titles.
pub fn stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((s, _)) if !s.is_empty() => s,
        _ => name,
    }
}

// ---------------------------------------------------------------------- layout
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaLayout {
    pub title: Rect,
    pub refresh: Rect,
    pub seg: Rect,
    /// One rectangle per list item (grid cell with its name line, or a list row).
    pub cells: Vec<Rect>,
    /// Where the spinner or the empty message goes.
    pub empty: Rect,
    pub total: i32,
}

pub fn layout(frame: &Frame, section: Section, n: usize) -> MediaLayout {
    let c = frame.padded();
    let mut y = SP_4;
    let refresh = Rect::new(c.right() - TOUCH_MIN, y - 4, TOUCH_MIN, TOUCH_MIN);
    let title = Rect::new(c.x, y, refresh.x - SP_3 - c.x, TITLE_H);
    y += TOUCH_MIN + SP_2;
    let seg = Rect::new(c.x, y, c.w, SEG_H);
    y += SEG_H + SP_4;
    let mut cells = Vec::with_capacity(n);
    let empty = Rect::new(c.x, y, c.w, 220);
    if section.grid() {
        let cols = ((c.w + GAP) / (TILE + GAP)).max(1);
        let used = cols * TILE + (cols - 1) * GAP;
        let x0 = c.x + (c.w - used) / 2;
        let cell_h = TILE + NAME_H + GAP;
        for i in 0..n as i32 {
            cells.push(Rect::new(
                x0 + (i % cols) * (TILE + GAP),
                y + (i / cols) * cell_h,
                TILE,
                TILE + NAME_H,
            ));
        }
        y += (n as i32 + cols - 1) / cols * cell_h;
    } else {
        for i in 0..n as i32 {
            cells.push(Rect::new(c.x, y + i * LIST_ROW_H, c.w, LIST_ROW_H));
        }
        y += n as i32 * LIST_ROW_H;
    }
    let total = if n == 0 { empty.bottom() } else { y } + SP_6;
    MediaLayout {
        title,
        refresh,
        seg,
        cells,
        empty,
        total,
    }
}

/// Item index range (first, last) whose cells touch the view `scroll..scroll+view_h` (content
/// coordinates), plus one screen of look-ahead each way.
pub fn visible_range(l: &MediaLayout, scroll: i32, view_h: i32) -> Option<(usize, usize)> {
    let top = scroll - view_h;
    let bottom = scroll + 2 * view_h;
    let mut first = None;
    let mut last = 0;
    for (i, r) in l.cells.iter().enumerate() {
        if r.bottom() >= top && r.y <= bottom {
            first.get_or_insert(i);
            last = i;
        }
    }
    first.map(|f| (f, last))
}

// ---------------------------------------------------------------------- drawing
pub struct View<'a> {
    pub m: &'a MediaModel,
    pub list: &'a [Item],
    pub st: &'a Option<Status>,
    pub pressed: Option<Target>,
    pub spinner_deg: u16,
}

fn blit_at(p: &mut Painter, src: &Canvas, x: i32, y: i32) {
    let (ox, oy) = p.cv.origin();
    p.cv.blit(src, x + ox, y + oy);
}

fn placeholder(p: &mut Painter, r: Rect, kind: Kind) {
    p.cv.rect(r, SURFACE);
    let (cx, cy) = r.center();
    p.cv.icon(kind.icon(), cx - 20, cy - 20, 40, TEXT_DISABLED);
}

fn duration_badge(p: &mut Painter, tile: Rect, secs: u32) {
    let s = fmt_duration(secs);
    let w = 12 + 8 * s.len() as i32;
    let b = Rect::new(tile.right() - w - SP_2, tile.bottom() - 26 - SP_2, w, 26);
    p.cv.rect(b, Tint::new(Color::hex(0x000000), 82));
    p.text(
        &s,
        Style::new(Face::Regular, SIZE_CAPTION, TEXT),
        b,
        Align::Center,
    );
}

pub fn render(p: &mut Painter, frame: &Frame, v: &View) -> Vec<(Rect, Target)> {
    let sec = v.m.section;
    let l = layout(frame, sec, v.list.len());
    let mut hits = Vec::new();
    let pr = |t: Target| v.pressed == Some(t);
    p.screen_title("Media", l.title);
    let rt = Target::Media(MediaTarget::Refresh);
    p.round_button(l.refresh, Icon::Refresh, pr(rt), false);
    hits.push((l.refresh, rt));
    let pressed_seg = Section::ALL
        .iter()
        .position(|s| pr(Target::Media(MediaTarget::Section(*s as usize))));
    let segs = p.segmented(
        l.seg,
        &Section::ALL.map(Section::label),
        sec as usize,
        pressed_seg,
        false,
    );
    for (i, r) in segs.iter().enumerate() {
        hits.push((*r, Target::Media(MediaTarget::Section(i))));
    }

    if v.list.is_empty() {
        if v.m.scanning || !v.m.scanned {
            let (cx, cy) = l.empty.center();
            p.spinner(cx, cy - 20, 40, v.spinner_deg as f32);
            p.text(
                "Looking for media...",
                Style::new(Face::Regular, SIZE_BODY, TEXT_SECONDARY),
                Rect::new(l.empty.x, cy + 16, l.empty.w, 28),
                Align::Center,
            );
        } else {
            let (title, hint) = sec.empty_text();
            let icon = match sec {
                Section::Photos => Icon::Image,
                Section::Videos => Icon::Media,
                Section::Music => Icon::Music,
                Section::Recent => Icon::Folder,
            };
            p.empty_state(l.empty, icon, title, hint);
        }
        return hits;
    }

    for (i, (it, r)) in v.list.iter().zip(&l.cells).enumerate() {
        let t = Target::Media(MediaTarget::Item(i));
        let info = v.m.infos.get(&it.path);
        let installed = player_for(it.kind, v.st).is_some();
        if sec.grid() {
            let tile = Rect::new(r.x, r.y, TILE, TILE);
            match v.m.thumbs.get(&it.path) {
                Some(Thumb::Ready(c)) => blit_at(p, c, tile.x, tile.y),
                _ => placeholder(p, tile, it.kind),
            }
            let _ = TILE_RADIUS;
            if pr(t) {
                p.cv.rect(tile, PRESSED_TINT);
                p.cv.rect(Rect::new(tile.x, tile.bottom() - 3, tile.w, 3), TEXT);
            } else {
                p.cv.rect(Rect::new(tile.x, tile.bottom() - 1, tile.w, 1), DIVIDER);
            }
            if it.kind == Kind::Video {
                let (cx, cy) = tile.center();
                if !matches!(v.m.thumbs.get(&it.path), Some(Thumb::Ready(_))) {
                    p.cv.icon(Icon::Media, cx - 20, cy - 20, 40, TEXT_DISABLED);
                }
                if let Some(d) = info.and_then(|i| i.duration) {
                    duration_badge(p, tile, d);
                }
            }
            p.text(
                stem(&it.name),
                Style::new(
                    Face::Regular,
                    SIZE_CAPTION,
                    if installed {
                        TEXT_SECONDARY
                    } else {
                        TEXT_DISABLED
                    },
                ),
                Rect::new(r.x, tile.bottom() + 2, TILE, NAME_H),
                Align::Left,
            );
        } else {
            if pr(t) {
                p.row_pressed(*r);
            }
            let icon_box = Rect::new(r.x + SP_2, r.y + (r.h - 48) / 2, 48, 48);
            match v.m.thumbs.get(&it.path) {
                Some(Thumb::Ready(c)) => {
                    if let Some(small) = c.scaled(48, 48) {
                        blit_at(p, &small, icon_box.x, icon_box.y);
                    }
                }
                _ => {
                    p.cv.rect(icon_box, SURFACE_STRONG);
                    p.cv.icon(
                        it.kind.icon(),
                        icon_box.x + 12,
                        icon_box.y + 12,
                        24,
                        TEXT_SECONDARY,
                    );
                }
            }
            let tx = icon_box.right() + SP_3;
            let tw = r.right() - tx - SP_3;
            let title = info
                .and_then(|i| i.title.clone())
                .unwrap_or_else(|| stem(&it.name).to_string());
            p.text(
                &title,
                Style::new(
                    Face::Regular,
                    SIZE_BODY,
                    if installed { TEXT } else { TEXT_DISABLED },
                ),
                Rect::new(tx, r.y + 10, tw, 26),
                Align::Left,
            );
            let mut sub = Vec::new();
            if let Some(a) = info.and_then(|i| i.artist.clone()) {
                sub.push(a);
            }
            if let Some(d) = info.and_then(|i| i.duration) {
                sub.push(fmt_duration(d));
            }
            if sub.is_empty() && it.size > 0 {
                sub.push(fmt_size(it.size));
            }
            if sec == Section::Recent {
                sub.push(it.kind.label().to_string());
            }
            p.text(
                &sub.join(" · "),
                Style::new(Face::Regular, SIZE_CAPTION, TEXT_SECONDARY),
                Rect::new(tx, r.y + 38, tw, 22),
                Align::Left,
            );
            p.divider(r.x, r.bottom() - 1, r.w);
        }
        hits.push((*r, t));
    }
    hits
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Image => "Photo",
            Kind::Video => "Video",
            Kind::Audio => "Music",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use walpad_proto::AppState;

    fn tmp(tag: &str) -> Root {
        let d = std::env::temp_dir().join(format!("walpad-media-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Root::new(d)
    }
    fn touch(r: &Root, p: &str, age: u64) {
        let f = r.p(p);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "x").unwrap();
        let t = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(t)
            .unwrap();
    }
    fn item(n: &str, k: Kind) -> Item {
        Item {
            path: format!("/root/Pictures/{n}"),
            name: n.into(),
            kind: k,
            mtime: 1,
            size: 10,
        }
    }

    #[test]
    fn kinds_come_from_the_extension_case_insensitively() {
        assert_eq!(Kind::of("/a/b.JPG"), Some(Kind::Image));
        assert_eq!(Kind::of("/a/b.mkv"), Some(Kind::Video));
        assert_eq!(Kind::of("/a/b.Flac"), Some(Kind::Audio));
        assert_eq!(Kind::of("/a/b.txt"), None);
        assert_eq!(Kind::of("/a/noext"), None);
    }

    #[test]
    fn scan_finds_media_newest_first_and_skips_the_rest() {
        let r = tmp("scan");
        touch(&r, "/root/Pictures/old.jpg", 500);
        touch(&r, "/root/Pictures/new.png", 10);
        touch(&r, "/root/Pictures/sub/deep/x.gif", 100);
        touch(&r, "/mnt/sd/DCIM/100/v.mp4", 50);
        touch(&r, "/mnt/usb/DISK1/Music/s.mp3", 20);
        touch(&r, "/root/Music/t.flac", 30);
        touch(&r, "/root/Pictures/notes.txt", 1);
        touch(&r, "/root/Pictures/.hidden.jpg", 1);
        touch(&r, "/root/Pictures/.thumbs/t.jpg", 1);
        touch(&r, "/mnt/sd/Android/data/a.jpg", 1);
        touch(&r, "/etc/not-scanned.jpg", 1);
        // a symlink to a folder outside is not followed
        std::fs::create_dir_all(r.p("/outside")).unwrap();
        touch(&r, "/outside/secret.jpg", 1);
        std::os::unix::fs::symlink(r.p("/outside"), r.p("/root/Pictures/link")).unwrap();
        let items = scan(&r);
        let paths: Vec<&str> = items.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "/root/Pictures/new.png",
                "/mnt/usb/DISK1/Music/s.mp3",
                "/root/Music/t.flac",
                "/mnt/sd/DCIM/100/v.mp4",
                "/root/Pictures/sub/deep/x.gif",
                "/root/Pictures/old.jpg",
            ]
        );
        assert!(items.iter().all(|i| walpad_proto::valid_open_path(&i.path)));
    }

    #[test]
    fn scan_is_bounded() {
        let r = tmp("bounded");
        for i in 0..(MAX_ITEMS + 80) {
            touch(&r, &format!("/root/Pictures/p{i:04}.jpg"), i as u64);
        }
        touch(&r, "/root/Pictures/a/b/c/d/too-deep.jpg", 0);
        let items = scan(&r);
        assert_eq!(items.len(), MAX_ITEMS);
        assert!(items.iter().all(|i| !i.path.contains("too-deep")));
        assert_eq!(items[0].name, "p0000.jpg", "the newest are kept");
    }

    #[test]
    fn recent_list_is_deduplicated_capped_and_validated() {
        let r = tmp("recent");
        assert!(read_recent(&r).is_empty());
        remember(&r, "/root/a.mp3");
        remember(&r, "/root/b.mp3");
        let v = remember(&r, "/root/a.mp3");
        assert_eq!(v, ["/root/a.mp3", "/root/b.mp3"]);
        for i in 0..40 {
            remember(&r, &format!("/root/f{i}.mp3"));
        }
        assert_eq!(read_recent(&r).len(), MAX_RECENT);
        assert_eq!(read_recent(&r)[0], "/root/f39.mp3");
        // a hand-edited file with a bad line: it is dropped
        std::fs::write(r.p(RECENT_FILE), "/etc/passwd\n-x\n/root/ok.mp3\n").unwrap();
        assert_eq!(read_recent(&r), ["/root/ok.mp3"]);
    }

    #[test]
    fn sections_filter_by_kind_and_recent_keeps_its_order() {
        let mut m = MediaModel::default();
        m.set_items(vec![
            item("a.jpg", Kind::Image),
            item("b.mp4", Kind::Video),
            item("c.mp3", Kind::Audio),
        ]);
        assert_eq!(m.list().len(), 1);
        m.section = Section::Videos;
        assert_eq!(m.list()[0].name, "b.mp4");
        m.section = Section::Recent;
        m.set_recent(vec![
            "/root/Pictures/c.mp3".into(),
            "/root/gone.mp4".into(),
            "/root/Pictures/a.jpg".into(),
            "/root/readme.txt".into(),
        ]);
        let names: Vec<String> = m.list().into_iter().map(|i| i.name).collect();
        assert_eq!(
            names,
            ["c.mp3", "gone.mp4", "a.jpg"],
            "unknown files stay, non-media goes"
        );
    }

    #[test]
    fn players_are_chosen_among_the_installed_apps() {
        let mut st = crate::app::tests::status(100, &[]);
        st.apps = vec![AppState {
            app: "vlc".into(),
            running: false,
        }];
        let st = Some(st);
        assert_eq!(player_for(Kind::Video, &st), Some("vlc"));
        assert_eq!(player_for(Kind::Image, &st), None);
        assert_eq!(player_for(Kind::Audio, &None), None);
        let mut s2 = st.clone().unwrap();
        s2.apps.push(AppState {
            app: "mpv".into(),
            running: false,
        });
        s2.apps.push(AppState {
            app: "gpicview".into(),
            running: false,
        });
        let s2 = Some(s2);
        assert_eq!(player_for(Kind::Video, &s2), Some("mpv"));
        assert_eq!(player_for(Kind::Audio, &s2), Some("mpv"));
        assert_eq!(player_for(Kind::Image, &s2), Some("gpicview"));
    }

    #[test]
    fn layout_grid_and_list_fit_both_orientations() {
        for f in [Frame::new(800, 1280), Frame::new(1280, 800)] {
            let c = f.padded();
            let g = layout(&f, Section::Photos, 23);
            assert_eq!(g.cells.len(), 23);
            for (i, a) in g.cells.iter().enumerate() {
                assert!(a.x >= c.x && a.right() <= c.right(), "{a:?}");
                assert!(a.w >= TOUCH_MIN && a.h >= TOUCH_MIN);
                for b in &g.cells[i + 1..] {
                    assert!(
                        a.right() <= b.x
                            || b.right() <= a.x
                            || a.bottom() <= b.y
                            || b.bottom() <= a.y,
                        "{a:?} overlaps {b:?}"
                    );
                }
            }
            assert!(g.total >= g.cells.last().unwrap().bottom());
            assert!(g.seg.bottom() < g.cells[0].y);
            let cols = g.cells.iter().filter(|r| r.y == g.cells[0].y).count() as i32;
            assert_eq!(cols, (c.w + GAP) / (TILE + GAP));
            let l = layout(&f, Section::Music, 5);
            assert!(l.cells.iter().all(|r| r.h == LIST_ROW_H && r.w == c.w));
            assert_eq!(layout(&f, Section::Music, 0).cells.len(), 0);
            assert!(layout(&f, Section::Music, 0).total > l.seg.bottom());
        }
    }

    #[test]
    fn only_what_is_near_the_screen_is_asked_for_once() {
        let f = Frame::new(800, 1280);
        let list: Vec<Item> = (0..200)
            .map(|i| item(&format!("p{i}.jpg"), Kind::Image))
            .collect();
        let mut m = MediaModel::default();
        m.set_items(list.clone());
        let l = layout(&f, Section::Photos, list.len());
        let (a, b) = visible_range(&l, 0, f.content.h).unwrap();
        assert_eq!(a, 0);
        assert!(b > 12 && b < 100, "a couple of screens, not all 200: {b}");
        m.note_visible(&list, a, b);
        assert_eq!(m.want_thumbs.len(), b - a + 1);
        assert!(m.want_info.is_empty(), "photos have no duration");
        let n = m.want_thumbs.len();
        m.note_visible(&list, a, b);
        assert_eq!(m.want_thumbs.len(), n, "never asked twice");
        // videos and music also want their facts
        let mut v = MediaModel::default();
        let vl = vec![item("v.mp4", Kind::Video), item("s.mp3", Kind::Audio)];
        v.section = Section::Recent;
        v.note_visible(&vl, 0, 1);
        assert_eq!(v.want_info.len(), 2);
        assert_eq!(
            v.want_thumbs.len(),
            1,
            "a list row has no thumbnail for music"
        );
    }

    #[test]
    fn thumbnails_decode_scale_and_are_capped() {
        let mut m = MediaModel::default();
        let png = {
            let c = Canvas::new(192, 192, Color::hex(0x336699)).unwrap();
            c.encode_png().unwrap()
        };
        m.on_thumb("/root/a.jpg", Some(&png));
        match m.thumbs.get("/root/a.jpg") {
            Some(Thumb::Ready(c)) => assert_eq!((c.width(), c.height()), (TILE, TILE)),
            _ => panic!("ready"),
        }
        m.on_thumb("/root/bad.jpg", Some(b"not a png"));
        assert!(matches!(
            m.thumbs.get("/root/bad.jpg"),
            Some(Thumb::Missing)
        ));
        m.on_thumb("/root/none.jpg", None);
        assert!(matches!(
            m.thumbs.get("/root/none.jpg"),
            Some(Thumb::Missing)
        ));
        for i in 0..MAX_THUMBS + 20 {
            m.on_thumb(&format!("/root/t{i}.jpg"), Some(&png));
        }
        let ready = m
            .thumbs
            .values()
            .filter(|t| matches!(t, Thumb::Ready(_)))
            .count();
        assert!(ready <= MAX_THUMBS);
    }

    #[test]
    fn durations_and_sizes_are_formatted() {
        assert_eq!(fmt_duration(187), "3:07");
        assert_eq!(fmt_duration(3729), "1:02:09");
        assert_eq!(fmt_duration(5), "0:05");
        assert_eq!(fmt_size(500), "1 KB");
        assert_eq!(fmt_size(5 * 1024 * 1024), "5 MB");
        assert_eq!(stem("song.final.mp3"), "song.final");
        assert_eq!(stem(".hidden"), ".hidden");
    }
}
