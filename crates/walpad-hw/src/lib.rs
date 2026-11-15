//! walpad-hw: hardware access (sysfs, evdev, gsensor, backlight) behind one path prefix
//! (`WALPAD_ROOT`, default `/`) so everything is testable against a temporary tree.
//! Every read returns Option/Result: a missing sensor is "unavailable", never guessed.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod adapters;
pub mod domain;

pub use adapters::{input, sysfs, sysinfo};
pub use domain::{battery, evdev, gsensor, jack, usb};

use std::path::{Path, PathBuf};

/// Path prefix for every file the core touches.
#[derive(Debug, Clone)]
pub struct Root(PathBuf);

impl Root {
    pub fn new(p: impl Into<PathBuf>) -> Self {
        Root(p.into())
    }
    /// From the WALPAD_ROOT environment variable, defaulting to `/`.
    pub fn from_env() -> Self {
        Root(
            std::env::var_os("WALPAD_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| "/".into()),
        )
    }
    /// Resolve an absolute-looking path (`/sys/...`) under the prefix.
    pub fn p(&self, abs: &str) -> PathBuf {
        self.0.join(abs.trim_start_matches('/'))
    }
    pub fn base(&self) -> &Path {
        &self.0
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    pub fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "walpad-hw-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        let _ = std::fs::create_dir_all(&d);
        d
    }
    pub fn put(root: &super::Root, path: &str, content: &str) {
        let p = root.p(path);
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(p, content);
    }
}
