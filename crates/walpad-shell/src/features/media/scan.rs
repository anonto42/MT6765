//! Media folder scanner and persistent recent-files list.

use super::{
    Item, Kind, MAX_DEPTH, MAX_ITEMS, MAX_RECENT, MAX_VISITS, RECENT_FILE, SCAN_DIRS, USB_PARENT,
};
use std::collections::HashSet;
use walpad_hw::Root;

/// Folders never entered (Android and Windows housekeeping on cards).
const SKIP_DIRS: &[&str] = &[
    "Android",
    "LOST.DIR",
    "System Volume Information",
    "$RECYCLE.BIN",
];

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
