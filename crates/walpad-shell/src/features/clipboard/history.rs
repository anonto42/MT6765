use std::collections::VecDeque;
use std::time::Duration;

pub const HISTORY_MAX: usize = 10;
/// Largest entry (bytes): bigger copies are ignored.
pub const ENTRY_MAX: usize = 64 * 1024;
/// A source app that does not answer a conversion within this time is given up on.
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(1);

/// The text history, newest first (pure: unit-tested).
#[derive(Debug, Default, Clone)]
pub struct History {
    items: VecDeque<String>,
}

impl History {
    /// Add an entry on top. Empty or blank text, NUL bytes and text over `ENTRY_MAX` are refused
    /// (false); a text already in the list moves to the top; the oldest entry falls off.
    pub fn push(&mut self, text: String) -> bool {
        if text.trim().is_empty() || text.len() > ENTRY_MAX || text.contains('\0') {
            return false;
        }
        self.items.retain(|t| *t != text);
        self.items.push_front(text);
        self.items.truncate(HISTORY_MAX);
        true
    }

    /// Move entry `i` to the top and return it.
    pub fn promote(&mut self, i: usize) -> Option<&str> {
        let t = self.items.remove(i)?;
        self.items.push_front(t);
        self.items.front().map(String::as_str)
    }

    pub fn get(&self, i: usize) -> Option<&str> {
        self.items.get(i).map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn clear(&mut self) {
        self.items.clear();
    }
    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.items.iter()
    }
}

/// What happened (the shell reacts: redraw the list, show the selection toolbar).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipEvent {
    /// The history changed.
    HistoryChanged,
    /// PRIMARY got a new owner (`true`: text may be selected) or lost it.
    PrimaryChanged(bool),
}
