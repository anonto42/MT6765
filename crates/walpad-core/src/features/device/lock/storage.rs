//! Persistence, rate-limiting state, and PBKDF2/hex helpers for the screen lock.
use super::{Lock, LockError};
use hmac::Hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use walpad_proto::LockKind;

pub const LOCK_FILE: &str = "/var/lib/walpad/lock.json";
pub const LOCKED_FILE: &str = "/run/walpad/locked";
pub(super) const BOOT_FILE: &str = "/run/walpad/lock-boot";
pub(super) const RATE_FILE: &str = "/run/walpad/lock-rate";

/// PBKDF2 rounds for new secrets (stored in the file, so it can be raised later). About 0.2 s on
/// the tablet's Cortex-A53 in plain Rust; a 4-digit PIN is guessable offline whatever this is, the
/// rate limiting guards the live device.
pub const ITERATIONS: u32 = 100_000;
const MAX_STORED_ITERATIONS: u32 = 10_000_000;
/// Wrong secrets per burst, then the first refusal lasts `BASE_DELAY_MS` and doubles each burst.
pub const BURST: u32 = 5;
pub const BASE_DELAY_MS: u64 = 30_000;
pub const MAX_DELAY_MS: u64 = 3_600_000;
pub(super) const POLL_FILE_EVERY_MS: u64 = 1000;

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct Stored {
    pub v: u32,
    pub kind: LockKind,
    pub algo: String,
    pub iter: u32,
    pub salt: String,
    pub hash: String,
    #[serde(default)]
    pub auto_lock_s: u32,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Rate {
    pub fails: u32,
    pub until_ms: u64,
}

impl Rate {
    pub fn wait_ms(&self, now: u64) -> u64 {
        self.until_ms.saturating_sub(now).min(MAX_DELAY_MS)
    }

    pub fn fail(&mut self, now: u64) {
        self.fails = self.fails.saturating_add(1);
        if self.fails.is_multiple_of(BURST) {
            let burst = self.fails / BURST - 1;
            let delay = (BASE_DELAY_MS << burst.min(7)).min(MAX_DELAY_MS);
            self.until_ms = now + delay;
        }
    }

    pub fn attempts_left(&self) -> u32 {
        BURST - self.fails % BURST
    }
}

pub(super) fn secs_up(ms: u64) -> u32 {
    ms.div_ceil(1000).min(u64::from(u32::MAX)) as u32
}

/// The salted-hash primitive: PBKDF2-HMAC-SHA256.
pub(super) fn derive(secret: &str, salt: &[u8], iter: u32) -> [u8; 32] {
    let mut out = [0u8; 32];
    // only fails for an invalid output length
    let _ = pbkdf2::pbkdf2::<Hmac<Sha256>>(secret.as_bytes(), salt, iter, &mut out);
    out
}

pub(super) fn enc(b: &[u8]) -> String {
    b.iter()
        .flat_map(|x| [x >> 4, x & 15])
        .map(|n| (b'a' + n) as char)
        .collect()
}

pub(super) fn dec(s: &str) -> Option<Vec<u8>> {
    let n: Vec<u8> = s
        .bytes()
        .map(|c| (b'a'..=b'p').contains(&c).then(|| c - b'a'))
        .collect::<Option<_>>()?;
    n.len()
        .is_multiple_of(2)
        .then(|| n.chunks(2).map(|p| p[0] << 4 | p[1]).collect())
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

pub(super) fn random_salt() -> std::io::Result<[u8; 16]> {
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
    pub(super) fn load(&self) -> Option<Stored> {
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

    pub(super) fn load_rate(&self) -> Rate {
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

    pub(super) fn save_rate(&self) {
        if self.rate == Rate::default() {
            let _ = std::fs::remove_file(&self.rate_file);
        } else {
            let _ = write_private(
                &self.rate_file,
                format!("{} {}\n", self.rate.fails, self.rate.until_ms).as_bytes(),
            );
        }
    }

    pub(super) fn sync_marker(&self) {
        if self.locked {
            let _ = write_private(&self.locked_file, b"1\n");
        } else {
            let _ = std::fs::remove_file(&self.locked_file);
        }
    }

    pub(super) fn set_locked(&mut self, on: bool) {
        let on = on && self.stored.is_some();
        if on != self.locked {
            self.locked = on;
            self.sync_marker();
        }
    }

    /// Does `secret` match? (Always runs the hash, whatever the kind.)
    pub(super) fn verify(&self, secret: &str) -> bool {
        let Some(s) = &self.stored else { return false };
        let (Some(salt), Some(want)) = (dec(&s.salt), dec(&s.hash)) else {
            return false;
        };
        ct_eq(&derive(secret, &salt, s.iter), &want)
    }

    /// Check a secret against the limiter and the hash. Wrong secrets are counted.
    pub(super) fn attempt(&mut self, secret: &str, now: u64) -> Result<(), LockError> {
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

    pub(super) fn save(&self, st: &Stored) -> Result<(), LockError> {
        let bytes = serde_json::to_vec_pretty(st).map_err(|e| LockError::Io(e.to_string()))?;
        write_private(&self.file, &bytes).map_err(|e| LockError::Io(format!("lock.json: {e}")))
    }
}
