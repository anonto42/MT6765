//! In-memory state for the Media screen (items, thumbnails, metadata requests).

use super::{Info, Item, Kind, Section, Thumb, MAX_THUMBS, TILE};
use std::collections::{HashMap, HashSet};
use walpad_ui::Canvas;

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
