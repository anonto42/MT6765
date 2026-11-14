//! The screen lock (owner request 2026-11-10, OpenSpec `lock-screen`): a PIN, password or 3x3
//! pattern kept as a salted PBKDF2-HMAC-SHA256 hash, verified only here, with rate limiting.
//!
//! * `/var/lib/walpad/lock.json` (mode 0600): kind, iterations, salt, hash, auto-lock delay. The
//!   salt and hash are written as "letter hex" (nibble 0-15 = `a`-`p`), so the file never holds a
//!   run of digits that could be mistaken for (part of) a PIN.
//! * `/run/walpad/locked` exists while the tablet is locked: the shell restarting, an app
//!   crashing or the core restarting changes nothing (the core is the authority, tmpfs clears at
//!   boot).
//! * `/run/walpad/lock-boot` marks "this boot already started the core once": the first start
//!   after a boot comes up locked when a lock is set.
//! * `/run/walpad/lock-rate` keeps the wrong-attempt counter across a core restart.
//!
//! Rescue: `rm /var/lib/walpad/lock.json` (what `walpad-ctl lock_clear` does) is noticed within a
//! second; the tablet unlocks and stays unlocked. Nothing here ever reads, logs or echoes a
//! secret: only `verify` sees it, and only to feed the hash.
//!
//! All times are milliseconds from a caller-supplied clock, so tests use a fake one.
mod storage;
#[cfg(test)]
mod tests;

use std::path::PathBuf;
use storage::{
    derive, enc, random_salt, secs_up, Rate, Stored, BOOT_FILE, POLL_FILE_EVERY_MS, RATE_FILE,
};
pub use storage::{BASE_DELAY_MS, BURST, ITERATIONS, LOCKED_FILE, LOCK_FILE, MAX_DELAY_MS};
use walpad_hw::Root;
use walpad_proto::{valid_lock_secret, LockKind, LockStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LockError {
    /// A lock is set and the current secret is missing or wrong.
    WrongSecret {
        retry_in_s: Option<u32>,
    },
    /// Too many wrong secrets: wait.
    Wait {
        retry_in_s: u32,
    },
    /// The new secret does not fit its kind.
    Invalid(String),
    /// No lock is set (remove / unlock-only operations).
    NotSet,
    Io(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unlock {
    Unlocked,
    /// Wrong secret; `attempts_left` before the next refusal period.
    Wrong {
        attempts_left: u32,
    },
    /// Refused without looking at the secret.
    Wait {
        retry_in_s: u32,
    },
}

pub struct Lock {
    file: PathBuf,
    locked_file: PathBuf,
    rate_file: PathBuf,
    stored: Option<Stored>,
    locked: bool,
    rate: Rate,
    iterations: u32,
    /// The screen went off at this time (None while it is on).
    off_since: Option<u64>,
    next_poll: u64,
}

impl Lock {
    /// Load the lock and decide the state at core start: the first start after a boot comes up
    /// locked when a lock is set; later starts keep what `/run/walpad/locked` says.
    pub fn new(root: &Root, now: u64) -> Lock {
        let mut l = Lock {
            file: root.p(LOCK_FILE),
            locked_file: root.p(LOCKED_FILE),
            rate_file: root.p(RATE_FILE),
            stored: None,
            locked: false,
            rate: Rate::default(),
            iterations: ITERATIONS,
            off_since: None,
            next_poll: now + POLL_FILE_EVERY_MS,
        };
        l.stored = l.load();
        let boot = root.p(BOOT_FILE);
        let first_start = !boot.exists();
        if first_start {
            if let Some(d) = boot.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let _ = std::fs::write(&boot, "1\n");
        }
        l.locked = l.stored.is_some() && (first_start || l.locked_file.exists());
        l.sync_marker();
        l.rate = l.load_rate();
        l
    }

    /// Test hook: fewer PBKDF2 rounds for new secrets (the file records its own count).
    pub fn set_iterations(&mut self, n: u32) {
        self.iterations = n.max(1);
    }

    pub fn configured(&self) -> bool {
        self.stored.is_some()
    }

    pub fn locked(&self) -> bool {
        self.locked
    }

    pub fn auto_lock_s(&self) -> u32 {
        self.stored.as_ref().map_or(0, |s| s.auto_lock_s)
    }

    pub fn status(&self, now: u64) -> LockStatus {
        let wait = self.rate.wait_ms(now);
        LockStatus {
            configured: self.configured(),
            kind: self.stored.as_ref().map(|s| s.kind),
            locked: self.locked,
            retry_in_s: (self.locked && wait > 0).then(|| secs_up(wait)),
            auto_lock_s: self.auto_lock_s(),
        }
    }

    /// `unlock`.
    pub fn unlock(&mut self, secret: &str, now: u64) -> Unlock {
        if !self.locked {
            return Unlock::Unlocked;
        }
        match self.attempt(secret, now) {
            Ok(()) => {
                self.set_locked(false);
                Unlock::Unlocked
            }
            Err(LockError::Wait { retry_in_s }) => Unlock::Wait { retry_in_s },
            Err(LockError::WrongSecret { retry_in_s }) => match retry_in_s {
                Some(s) => Unlock::Wait { retry_in_s: s },
                None => Unlock::Wrong {
                    attempts_left: self.rate.attempts_left(),
                },
            },
            // the lock vanished under us
            Err(_) => Unlock::Unlocked,
        }
    }

    /// `lock_set`: first lock needs no current secret; changing it does.
    pub fn set(
        &mut self,
        kind: LockKind,
        secret: &str,
        current: Option<&str>,
        now: u64,
    ) -> Result<(), LockError> {
        if !valid_lock_secret(kind, secret) {
            return Err(LockError::Invalid(
                walpad_proto::lock_secret_rule(kind).into(),
            ));
        }
        if self.stored.is_some() {
            let Some(cur) = current else {
                return Err(LockError::WrongSecret { retry_in_s: None });
            };
            self.attempt(cur, now)?;
        }
        let salt = random_salt().map_err(|e| LockError::Io(format!("no random salt: {e}")))?;
        let st = Stored {
            v: 1,
            kind,
            algo: "pbkdf2-sha256".into(),
            iter: self.iterations,
            salt: enc(&salt),
            hash: enc(&derive(secret, &salt, self.iterations)),
            auto_lock_s: self.auto_lock_s(),
        };
        self.save(&st)?;
        self.stored = Some(st);
        Ok(())
    }

    /// `lock_remove`: needs the current secret.
    pub fn remove(&mut self, current: &str, now: u64) -> Result<(), LockError> {
        self.attempt(current, now)?;
        self.clear();
        Ok(())
    }

    /// Drop the lock without a secret (rescue: the file is gone or `walpad-ctl lock_clear` ran).
    pub fn clear(&mut self) {
        let _ = std::fs::remove_file(&self.file);
        self.stored = None;
        self.rate = Rate::default();
        self.save_rate();
        self.set_locked(false);
    }

    /// `lock_config`.
    pub fn set_auto_lock(&mut self, secs: u32) -> Result<(), LockError> {
        let Some(mut st) = self.stored.clone() else {
            return Err(LockError::NotSet);
        };
        st.auto_lock_s = secs;
        self.save(&st)?;
        self.stored = Some(st);
        Ok(())
    }

    /// `lock_now`, and the short power press.
    pub fn lock_now(&mut self) -> bool {
        self.set_locked(true);
        self.locked
    }

    /// The screen went off (`off`) or came back. Auto-lock counts from the moment it went off.
    pub fn screen(&mut self, off: bool, now: u64) {
        match (off, self.off_since) {
            (true, None) => self.off_since = Some(now),
            (false, Some(_)) => self.off_since = None,
            _ => {}
        }
    }

    /// Timers: auto-lock, and the once-a-second look at lock.json (rescue by `rm`, or a changed
    /// file). Returns true when something visible changed.
    pub fn tick(&mut self, now: u64) -> bool {
        let before = (self.locked, self.stored.is_some());
        if let Some(t) = self.off_since {
            if self.stored.is_some()
                && !self.locked
                && now.saturating_sub(t) >= u64::from(self.auto_lock_s()) * 1000
            {
                self.set_locked(true);
            }
        }
        if now >= self.next_poll {
            self.next_poll = now + POLL_FILE_EVERY_MS;
            let present = self.file.exists();
            if present != self.stored.is_some() {
                self.stored = self.load();
                if self.stored.is_none() {
                    self.rate = Rate::default();
                    self.save_rate();
                    self.set_locked(false);
                }
            }
        }
        before != (self.locked, self.stored.is_some())
    }
}
