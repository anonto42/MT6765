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
use hmac::Hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use walpad_hw::Root;
use walpad_proto::{valid_lock_secret, LockKind, LockStatus};

pub const LOCK_FILE: &str = "/var/lib/walpad/lock.json";
pub const LOCKED_FILE: &str = "/run/walpad/locked";
const BOOT_FILE: &str = "/run/walpad/lock-boot";
const RATE_FILE: &str = "/run/walpad/lock-rate";

/// PBKDF2 rounds for new secrets (stored in the file, so it can be raised later). About 0.2 s on
/// the tablet's Cortex-A53 in plain Rust; a 4-digit PIN is guessable offline whatever this is, the
/// rate limiting guards the live device.
pub const ITERATIONS: u32 = 100_000;
const MAX_STORED_ITERATIONS: u32 = 10_000_000;
/// Wrong secrets per burst, then the first refusal lasts `BASE_DELAY_MS` and doubles each burst.
pub const BURST: u32 = 5;
pub const BASE_DELAY_MS: u64 = 30_000;
pub const MAX_DELAY_MS: u64 = 3_600_000;
const POLL_FILE_EVERY_MS: u64 = 1000;

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

#[derive(Serialize, Deserialize, Clone)]
struct Stored {
    v: u32,
    kind: LockKind,
    algo: String,
    iter: u32,
    salt: String,
    hash: String,
    #[serde(default)]
    auto_lock_s: u32,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Rate {
    fails: u32,
    until_ms: u64,
}

impl Rate {
    fn wait_ms(&self, now: u64) -> u64 {
        self.until_ms.saturating_sub(now).min(MAX_DELAY_MS)
    }
    fn fail(&mut self, now: u64) {
        self.fails = self.fails.saturating_add(1);
        if self.fails.is_multiple_of(BURST) {
            let burst = self.fails / BURST - 1;
            let delay = (BASE_DELAY_MS << burst.min(7)).min(MAX_DELAY_MS);
            self.until_ms = now + delay;
        }
    }
    fn attempts_left(&self) -> u32 {
        BURST - self.fails % BURST
    }
}

fn secs_up(ms: u64) -> u32 {
    ms.div_ceil(1000).min(u64::from(u32::MAX)) as u32
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

/// The salted-hash primitive: PBKDF2-HMAC-SHA256.
fn derive(secret: &str, salt: &[u8], iter: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    // only fails for an invalid output length
    let _ = pbkdf2::pbkdf2::<Hmac<Sha256>>(secret.as_bytes(), salt, iter, &mut out);
    out
}

fn enc(b: &[u8]) -> String {
    b.iter()
        .flat_map(|x| [x >> 4, x & 15])
        .map(|n| (b'a' + n) as char)
        .collect()
}

fn dec(s: &str) -> Option<Vec<u8>> {
    let n: Vec<u8> = s
        .bytes()
        .map(|c| (b'a'..=b'p').contains(&c).then(|| c - b'a'))
        .collect::<Option<_>>()?;
    n.len().is_multiple_of(2).then(|| n.chunks(2).map(|p| p[0] << 4 | p[1]).collect())
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

fn random_salt() -> std::io::Result<[u8; 16]> {
    use std::io::Read;
    let mut b = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
    Ok(b)
}

fn write_private(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
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

    fn load(&self) -> Option<Stored> {
        let s = std::fs::read_to_string(&self.file).ok()?;
        let st: Stored = serde_json::from_str(&s).ok()?;
        (st.v == 1
            && st.algo == "pbkdf2-sha256"
            && st.iter > 0
            && st.iter <= MAX_STORED_ITERATIONS
            && dec(&st.salt).is_some_and(|b| !b.is_empty())
            && dec(&st.hash).is_some_and(|b| b.len() == 32))
        .then_some(st)
    }

    fn load_rate(&self) -> Rate {
        std::fs::read_to_string(&self.rate_file)
            .ok()
            .and_then(|s| {
                let mut it = s.split_whitespace().map(|x| x.parse::<u64>().ok());
                Some(Rate {
                    fails: it.next()??.min(u64::from(u32::MAX)) as u32,
                    until_ms: it.next()??,
                })
            })
            .unwrap_or_default()
    }

    fn save_rate(&self) {
        if self.rate == Rate::default() {
            let _ = std::fs::remove_file(&self.rate_file);
        } else {
            let _ = write_private(
                &self.rate_file,
                format!("{} {}\n", self.rate.fails, self.rate.until_ms).as_bytes(),
            );
        }
    }

    fn sync_marker(&self) {
        if self.locked {
            let _ = write_private(&self.locked_file, b"1\n");
        } else {
            let _ = std::fs::remove_file(&self.locked_file);
        }
    }

    fn set_locked(&mut self, on: bool) {
        let on = on && self.stored.is_some();
        if on != self.locked {
            self.locked = on;
            self.sync_marker();
        }
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

    /// Does `secret` match? (Always runs the hash, whatever the kind.)
    fn verify(&self, secret: &str) -> bool {
        let Some(s) = &self.stored else { return false };
        let (Some(salt), Some(want)) = (dec(&s.salt), dec(&s.hash)) else {
            return false;
        };
        ct_eq(&derive(secret, &salt, s.iter), &want)
    }

    /// Check a secret against the limiter and the hash. Wrong secrets are counted.
    fn attempt(&mut self, secret: &str, now: u64) -> Result<(), LockError> {
        if self.stored.is_none() {
            return Err(LockError::NotSet);
        }
        let wait = self.rate.wait_ms(now);
        if wait > 0 {
            return Err(LockError::Wait {
                retry_in_s: secs_up(wait),
            });
        }
        if self.verify(secret) {
            self.rate = Rate::default();
            self.save_rate();
            Ok(())
        } else {
            self.rate.fail(now);
            self.save_rate();
            let wait = self.rate.wait_ms(now);
            Err(LockError::WrongSecret {
                retry_in_s: (wait > 0).then(|| secs_up(wait)),
            })
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

    fn save(&self, st: &Stored) -> Result<(), LockError> {
        let bytes = serde_json::to_vec_pretty(st).map_err(|e| LockError::Io(e.to_string()))?;
        write_private(&self.file, &bytes).map_err(|e| LockError::Io(format!("lock.json: {e}")))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn root(name: &str) -> (std::path::PathBuf, Root) {
        let d = std::env::temp_dir().join(format!("walpad-lock-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let r = Root::new(&d);
        (d, r)
    }

    fn fresh(name: &str) -> (std::path::PathBuf, Lock) {
        let (d, r) = root(name);
        let mut l = Lock::new(&r, 0);
        l.set_iterations(8);
        (d, l)
    }

    #[test]
    fn letter_hex_round_trips_and_has_no_digits() {
        let b: Vec<u8> = (0..=255).collect();
        let e = enc(&b);
        assert!(e.bytes().all(|c| (b'a'..=b'p').contains(&c)));
        assert_eq!(dec(&e).unwrap(), b);
        assert!(dec("abc").is_none() && dec("ab1c").is_none());
    }

    #[test]
    fn setting_a_pin_stores_a_private_salted_hash_without_the_pin() {
        let (d, mut l) = fresh("set");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        let f = d.join("var/lib/walpad/lock.json");
        let mode = std::fs::metadata(&f).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let text = std::fs::read_to_string(&f).unwrap();
        assert!(!text.contains("4321"), "{text}");
        assert!(text.contains("pbkdf2-sha256"));
        // salted: two locks with the same PIN differ
        let first: Stored = serde_json::from_str(&text).unwrap();
        l.set(LockKind::Pin, "4321", Some("4321"), 0).unwrap();
        let second: Stored = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_ne!(first.salt, second.salt);
        assert_ne!(first.hash, second.hash);
        assert!(l.verify("4321") && !l.verify("4322"));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_new_secret_must_fit_its_kind_and_changing_needs_the_current_one() {
        let (d, mut l) = fresh("kind");
        assert!(matches!(
            l.set(LockKind::Pin, "12", None, 0),
            Err(LockError::Invalid(_))
        ));
        assert!(matches!(
            l.set(LockKind::Pattern, "0120", None, 0),
            Err(LockError::Invalid(_))
        ));
        l.set(LockKind::Password, "correct horse", None, 0).unwrap();
        assert_eq!(
            l.set(LockKind::Pin, "1234", None, 0),
            Err(LockError::WrongSecret { retry_in_s: None })
        );
        assert!(matches!(
            l.set(LockKind::Pin, "1234", Some("nope nope"), 0),
            Err(LockError::WrongSecret { .. })
        ));
        l.set(LockKind::Pin, "1234", Some("correct horse"), 0)
            .unwrap();
        assert_eq!(l.status(0).kind, Some(LockKind::Pin));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn five_wrong_secrets_refuse_for_30_s_even_the_right_one_then_double() {
        let (d, mut l) = fresh("rate");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        l.lock_now();
        let mut t = 1000;
        for i in 0..4 {
            assert_eq!(
                l.unlock("0000", t),
                Unlock::Wrong {
                    attempts_left: 4 - i
                }
            );
            t += 100;
        }
        // the 5th wrong one starts the refusal
        assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 30 });
        assert_eq!(l.status(t).retry_in_s, Some(30));
        // the right PIN during the delay is refused and does not unlock
        assert_eq!(l.unlock("4321", t + 5000), Unlock::Wait { retry_in_s: 25 });
        assert!(l.locked());
        // refused attempts are not counted: after 30 s five more are allowed...
        t += 30_000;
        for _ in 0..4 {
            assert!(matches!(l.unlock("0000", t), Unlock::Wrong { .. }));
        }
        // ...and the next burst waits 60 s
        assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 60 });
        t += 60_000;
        for _ in 0..4 {
            l.unlock("0000", t);
        }
        assert_eq!(l.unlock("0000", t), Unlock::Wait { retry_in_s: 120 });
        // a right PIN after the delay unlocks and resets the count
        t += 120_000;
        assert_eq!(l.unlock("4321", t), Unlock::Unlocked);
        assert!(!l.locked());
        l.lock_now();
        assert_eq!(l.unlock("0000", t), Unlock::Wrong { attempts_left: 4 });
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_delay_is_capped_and_survives_a_restart() {
        let (d, mut l) = fresh("cap");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        l.lock_now();
        let mut t = 0;
        for _ in 0..80 {
            t += 3_600_000 + 1;
            l.unlock("0000", t);
        }
        assert!(l.status(t).retry_in_s.unwrap_or(0) <= 3600);
        // a restarted core (same boot) still refuses
        let r = Root::new(&d);
        let mut l2 = Lock::new(&r, t);
        l2.set_iterations(8);
        assert!(l2.locked() && l2.status(t).retry_in_s.is_some());
        assert!(matches!(l2.unlock("4321", t + 1), Unlock::Wait { .. }));
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn changing_or_removing_counts_wrong_current_secrets_too() {
        let (d, mut l) = fresh("remove");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        for _ in 0..5 {
            assert!(l.remove("0000", 10).is_err());
        }
        assert!(matches!(
            l.remove("4321", 11),
            Err(LockError::Wait { retry_in_s: 30 })
        ));
        l.remove("4321", 31_000).unwrap();
        assert!(!l.configured());
        assert!(!d.join("var/lib/walpad/lock.json").exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn first_start_after_boot_is_locked_a_restart_keeps_the_state() {
        let (d, mut l) = fresh("boot");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        assert!(!l.locked(), "setting a lock does not lock the tablet");
        let r = Root::new(&d);
        // core restarted in the same boot while unlocked: stays unlocked
        let l2 = Lock::new(&r, 0);
        assert!(!l2.locked());
        // a reboot clears /run: the first start is locked
        std::fs::remove_dir_all(d.join("run")).unwrap();
        let mut l3 = Lock::new(&r, 0);
        l3.set_iterations(8);
        assert!(l3.locked());
        assert!(d.join("run/walpad/locked").exists());
        // a restart while locked stays locked; unlocking removes the marker
        let mut l4 = Lock::new(&r, 0);
        l4.set_iterations(8);
        assert!(l4.locked());
        assert_eq!(l4.unlock("4321", 1), Unlock::Unlocked);
        assert!(!d.join("run/walpad/locked").exists());
        assert!(!Lock::new(&r, 0).locked());
        // no lock set: never locked at boot
        std::fs::remove_dir_all(d.join("run")).unwrap();
        std::fs::remove_file(d.join("var/lib/walpad/lock.json")).unwrap();
        assert!(!Lock::new(&r, 0).locked());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn auto_lock_counts_from_screen_off_and_a_short_wake_does_not_lock() {
        let (d, mut l) = fresh("auto");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        l.set_auto_lock(30).unwrap();
        assert_eq!(l.status(0).auto_lock_s, 30);
        l.screen(true, 1000);
        assert!(!l.tick(20_000) && !l.locked());
        l.screen(false, 25_000);
        assert!(!l.tick(100_000) && !l.locked(), "woke before the delay");
        l.screen(true, 100_000);
        assert!(!l.tick(129_999));
        assert!(l.tick(130_000) && l.locked());
        // delay 0 = at once
        l.unlock("4321", 131_000);
        l.set_auto_lock(0).unwrap();
        l.screen(false, 131_000);
        l.screen(true, 132_000);
        assert!(l.tick(132_000) && l.locked());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn rescue_by_removing_the_file_unlocks_within_a_second() {
        let (d, mut l) = fresh("rescue");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        l.lock_now();
        std::fs::remove_file(d.join("var/lib/walpad/lock.json")).unwrap();
        assert!(!l.tick(500), "not looked at yet");
        assert!(l.tick(1500));
        assert!(!l.configured() && !l.locked());
        assert!(!d.join("run/walpad/locked").exists());
        // lock_now without a lock does nothing
        assert!(!l.lock_now());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn a_damaged_file_counts_as_no_lock_never_as_locked_forever() {
        let (d, mut l) = fresh("bad");
        l.set(LockKind::Pin, "4321", None, 0).unwrap();
        std::fs::write(d.join("var/lib/walpad/lock.json"), "{\"v\":1,").unwrap();
        let r = Root::new(&d);
        assert!(!Lock::new(&r, 0).configured());
        let _ = std::fs::remove_dir_all(d);
    }
}
