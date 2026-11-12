//! Notification centre (owner request 2026-11-12, change `device-notifications`): a bounded ring
//! buffer of the events the owner wants to know about - charger plug/unplug, USB/OTG device
//! attach/detach, SD card / USB disk mount/unmount, Bluetooth connect/disconnect, mobile data
//! up/down and the hotspot client count - generated from status transitions the daemon already
//! tracks (`crate::logic::notifications::diff`; see `docs/rust/CONTRACTS.md`).
//!
//! Persisted to `/var/lib/walpad/notifications.json` (atomic rename, like `stats.rs`'s
//! statusbar.json) so the history survives a shell/core restart. `/var/lib/walpad` sits on the
//! userdata partition, so in practice it survives a reboot too, the same as e.g. the saved volume
//! level or the Wi-Fi switch: nothing here deliberately wipes it at boot.
use crate::logic::notifications::{diff, Snapshot};
use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{Notification, NotificationCategory, NotificationsStatus};

pub const STORE_FILE: &str = "/var/lib/walpad/notifications.json";
/// Last N notifications are kept; older ones fall off the front.
const CAPACITY: usize = 100;
const EVERY: Duration = Duration::from_secs(2);
const MAX_FILE: u64 = 1 << 20;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct Store {
    next_id: u64,
    /// Oldest first.
    items: VecDeque<Notification>,
}

pub struct Notifications {
    file: PathBuf,
    store: Store,
    /// None until the first observation (a daemon restart never floods the history with "just
    /// connected" for everything already plugged in).
    prev: Option<Snapshot>,
    next: Instant,
    every: Duration,
}

fn read_file(path: &std::path::Path) -> Option<String> {
    use std::io::Read;
    let mut s = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_FILE)
        .read_to_string(&mut s)
        .ok()?;
    Some(s)
}

impl Notifications {
    pub fn new(root: &Root) -> Notifications {
        let file = root.p(STORE_FILE);
        let store = read_file(&file)
            .and_then(|t| serde_json::from_str::<Store>(&t).ok())
            .unwrap_or_default();
        Notifications {
            file,
            store,
            prev: None,
            next: Instant::now(),
            every: EVERY,
        }
    }

    /// Test hook: a shorter sampling interval.
    pub fn set_every(&mut self, d: Duration) {
        self.every = d;
    }

    /// True when the next 2 s sampling window has arrived (avoids building the sysfs snapshot on
    /// every event-loop turn).
    pub fn is_due(&self, now: Instant) -> bool {
        now >= self.next
    }

    pub fn status(&self) -> NotificationsStatus {
        NotificationsStatus {
            unread_count: self.store.items.iter().filter(|n| !n.read).count() as u32,
        }
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Notification> {
        self.store.items.iter().rev().cloned().collect()
    }

    /// `true` when the id existed (whether or not it changed anything).
    pub fn mark_read(&mut self, id: u64) -> bool {
        let Some(n) = self.store.items.iter_mut().find(|n| n.id == id) else {
            return false;
        };
        if !n.read {
            n.read = true;
            self.save();
        }
        true
    }

    /// Returns how many were newly marked.
    pub fn mark_all_read(&mut self) -> u32 {
        let mut n = 0u32;
        for it in self.store.items.iter_mut() {
            if !it.read {
                it.read = true;
                n += 1;
            }
        }
        if n > 0 {
            self.save();
        }
        n
    }

    /// `true` when the id existed and was removed.
    pub fn clear(&mut self, id: u64) -> bool {
        let before = self.store.items.len();
        self.store.items.retain(|n| n.id != id);
        let changed = self.store.items.len() != before;
        if changed {
            self.save();
        }
        changed
    }

    pub fn clear_all(&mut self) {
        if !self.store.items.is_empty() {
            self.store.items.clear();
            self.save();
        }
    }

    fn push(&mut self, category: NotificationCategory, title: String, body: String, ts_ms: u64) {
        let id = self.store.next_id;
        self.store.next_id += 1;
        self.store.items.push_back(Notification {
            id,
            ts_ms,
            category,
            title,
            body,
            read: false,
        });
        while self.store.items.len() > CAPACITY {
            self.store.items.pop_front();
        }
    }

    fn save(&self) {
        if let Some(d) = self.file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let Ok(js) = serde_json::to_vec(&self.store) else {
            return;
        };
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let Ok(mut f) = std::fs::File::create(&tmp) else {
            return;
        };
        if f.write_all(&js).is_ok() && f.sync_all().is_ok() {
            let _ = std::fs::rename(&tmp, &self.file);
        }
    }

    /// Observe a fresh snapshot (due roughly every 2 s); generates and persists any notifications
    /// that just fired. `ts_ms` is the wall-clock time for their timestamp. Returns the
    /// notifications that fired (empty most of the time).
    pub fn tick(&mut self, now: Instant, ts_ms: u64, snap: Snapshot) -> Vec<Notification> {
        if now < self.next {
            return Vec::new();
        }
        self.next = now + self.every;
        let Some(prev) = self.prev.replace(snap.clone()) else {
            return Vec::new();
        };
        let fired = diff(&prev, &snap);
        if fired.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for n in fired {
            self.push(n.category, n.title, n.body, ts_ms);
            if let Some(last) = self.store.items.back() {
                out.push(last.clone());
            }
        }
        self.save();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> Root {
        let d = std::env::temp_dir().join(format!(
            "walpad-notifications-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&d);
        Root::new(d)
    }

    fn charging(on: bool) -> Snapshot {
        Snapshot {
            charging: on,
            ..Snapshot::default()
        }
    }

    #[test]
    fn first_tick_is_a_baseline_no_notification() {
        let mut n = Notifications::new(&root());
        let t0 = Instant::now();
        assert!(n.tick(t0, 1000, charging(true)).is_empty());
        assert_eq!(n.status().unread_count, 0);
    }

    #[test]
    fn a_transition_fires_once_is_unread_and_bumps_the_badge() {
        let mut n = Notifications::new(&root());
        let t0 = Instant::now();
        n.tick(t0, 1000, charging(false));
        let fired = n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].title, "Charger connected");
        assert!(!fired[0].read);
        assert_eq!(n.status().unread_count, 1);
        assert_eq!(n.list()[0].title, "Charger connected");
        // not due yet: no new reading, no duplicate
        assert!(n
            .tick(t0 + Duration::from_secs(3), 3000, charging(true))
            .is_empty());
    }

    #[test]
    fn mark_read_clear_and_clear_all() {
        let mut n = Notifications::new(&root());
        let t0 = Instant::now();
        n.tick(t0, 1000, charging(false));
        n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
        n.tick(t0 + Duration::from_secs(4), 4000, charging(false));
        assert_eq!(n.status().unread_count, 2);
        let ids: Vec<u64> = n.list().iter().map(|x| x.id).collect();
        assert!(!n.mark_read(999));
        assert!(n.mark_read(ids[0]));
        assert_eq!(n.status().unread_count, 1);
        assert_eq!(n.mark_all_read(), 1);
        assert_eq!(n.status().unread_count, 0);
        assert!(!n.clear(999));
        assert!(n.clear(ids[0]));
        assert_eq!(n.list().len(), 1);
        n.clear_all();
        assert!(n.list().is_empty());
    }

    #[test]
    fn newest_first_and_ring_buffer_caps_at_100() {
        let mut n = Notifications::new(&root());
        let t0 = Instant::now();
        let mut on = false;
        n.tick(t0, 0, charging(on));
        for i in 1..205u64 {
            on = !on;
            n.tick(t0 + Duration::from_secs(2 * i), i * 1000, charging(on));
        }
        let list = n.list();
        assert_eq!(list.len(), 100);
        // newest first: the most recent id is at index 0
        assert!(list[0].id > list[1].id);
        assert!(list.windows(2).all(|w| w[0].id > w[1].id));
    }

    #[test]
    fn persists_across_a_restart_and_tolerates_a_corrupt_file() {
        let r = root();
        {
            let mut n = Notifications::new(&r);
            let t0 = Instant::now();
            n.tick(t0, 1000, charging(false));
            n.tick(t0 + Duration::from_secs(2), 2000, charging(true));
        }
        let again = Notifications::new(&r);
        assert_eq!(again.list().len(), 1);
        assert_eq!(again.status().unread_count, 1);

        // a corrupt file is tolerated: start empty rather than panic or fail to start
        std::fs::write(r.p(STORE_FILE), b"{ not json").unwrap();
        let after_corruption = Notifications::new(&r);
        assert!(after_corruption.list().is_empty());
        assert_eq!(after_corruption.status().unread_count, 0);
    }
}
