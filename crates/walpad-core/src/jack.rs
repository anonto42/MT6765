//! Headphone jack route / mic source (owner request 2026-11-12, change `headphone-mic-jack`).
//! The core owns the persisted override (`/var/lib/walpad/jack-force`: `none|headphones|headset`)
//! and applies it (plus any future live detection, see `walpad_hw::jack` and
//! `crate::logic::jack`) by running the fixed script `/usr/local/sbin/walpad-audio jack <route>
//! <mic>`, as a child with a timeout, polled from the daemon loop (never waited on) — the same
//! pattern as `volume.rs` and `hotspot.rs`.
use crate::logic::jack::{self as logic, MicSource, Route};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::{JackKind, JackStatus};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-audio";
pub const FILE: &str = "/var/lib/walpad/jack-force";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const TIMEOUT: Duration = Duration::from_secs(8);
/// Live accdet detection is re-read this often (always None on this kernel build; cheap no-op).
const DETECT_EVERY: Duration = Duration::from_secs(2);

fn parse_kind(s: &str) -> Option<JackKind> {
    match s.trim() {
        "headphones" => Some(JackKind::Headphones),
        "headset" => Some(JackKind::HeadsetWithMic),
        _ => None,
    }
}

fn kind_name(k: JackKind) -> &'static str {
    match k {
        JackKind::Headphones => "headphones",
        JackKind::HeadsetWithMic => "headset",
    }
}

struct Run {
    child: Child,
    deadline: Instant,
}

pub struct Jack {
    root: Root,
    script: PathBuf,
    file: PathBuf,
    base: Option<PathBuf>,
    st: JackStatus,
    run: Option<Run>,
    pending: bool,
    next_detect: Instant,
    timeout: Duration,
}

impl Jack {
    pub fn new(root: &Root, now: Instant) -> Self {
        let file = root.p(FILE);
        let forced = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| parse_kind(&t));
        let mut j = Jack {
            root: root.clone(),
            script: root.p(SCRIPT),
            file,
            base: (root.base() != std::path::Path::new("/")).then(|| root.base().to_path_buf()),
            st: JackStatus {
                detected: None,
                forced,
                route: None,
            },
            run: None,
            pending: false,
            next_detect: now,
            timeout: TIMEOUT,
        };
        j.recompute();
        // apply once at startup so a core restart re-asserts the saved route
        j.pending = true;
        j
    }

    /// Test hook: how long the script may run.
    pub fn set_timeout(&mut self, d: Duration) {
        self.timeout = d;
    }

    pub fn status(&self) -> JackStatus {
        self.st
    }

    fn recompute(&mut self) {
        self.st.route = logic::effective(self.st.detected, self.st.forced);
    }

    /// `jack_force {route}`: owner Settings override. `None` clears it.
    pub fn set_force(&mut self, route: Option<JackKind>) -> JackStatus {
        if route != self.st.forced {
            self.st.forced = route;
            self.recompute();
            self.save();
            self.pending = true;
        }
        self.st
    }

    fn save(&self) {
        let text = match self.st.forced {
            Some(k) => kind_name(k).to_string(),
            None => "none".to_string(),
        };
        if let Some(d) = self.file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(".tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.file);
        }
    }

    fn spawn(&mut self, now: Instant) {
        let route = match logic::route_for(self.st.route) {
            Route::Headphone => "headphone",
            Route::Speaker => "speaker",
        };
        let mic = match logic::mic_source_for(self.st.route) {
            MicSource::HeadsetMic => "headset",
            MicSource::BuiltIn => "builtin",
        };
        let mut cmd = Command::new(&self.script);
        cmd.args(["jack", route, mic])
            .env_clear()
            .env("PATH", PATH)
            .env("HOME", "/root")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(b) = &self.base {
            cmd.env("WALPAD_AUDIO_ROOT", b);
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        if let Ok(child) = cmd.spawn() {
            self.run = Some(Run {
                child,
                deadline: now + self.timeout,
            });
        }
    }

    fn kill(mut r: Run) {
        if let Some(pid) = rustix::process::Pid::from_raw(r.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    }

    /// Poll the child, re-read (future) live detection, start the next job. True on a change.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st;
        if now >= self.next_detect {
            self.next_detect = now + DETECT_EVERY;
            // A plain switch can only say "something is here"; it can never tell a mic-equipped
            // headset apart from plain headphones (see walpad_hw::jack), so a bare `true` can
            // only become `Headphones`, never `HeadsetWithMic`.
            let detected = walpad_hw::jack::read_jack(&self.root)
                .and_then(|present| present.then_some(JackKind::Headphones));
            if detected != self.st.detected {
                self.st.detected = detected;
                self.recompute();
                self.pending = true;
            }
        }
        if let Some(r) = self.run.as_mut() {
            match r.child.try_wait() {
                Ok(Some(_)) => {
                    self.run = None;
                }
                Ok(None) if now >= r.deadline => {
                    if let Some(r) = self.run.take() {
                        Self::kill(r);
                    }
                }
                Ok(None) => {}
                Err(_) => self.run = None,
            }
        }
        if self.run.is_none() && self.pending {
            self.pending = false;
            self.spawn(now);
        }
        self.st != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_name_round_trip() {
        for k in [JackKind::Headphones, JackKind::HeadsetWithMic] {
            assert_eq!(parse_kind(kind_name(k)), Some(k));
        }
        assert_eq!(parse_kind("none"), None);
        assert_eq!(parse_kind(""), None);
        assert_eq!(parse_kind("garbage"), None);
    }

    #[test]
    fn new_reads_the_persisted_override_and_applies_once() {
        let dir = std::env::temp_dir().join(format!("walpad-jack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("var/lib/walpad")).unwrap();
        std::fs::write(dir.join("var/lib/walpad/jack-force"), "headset").unwrap();
        let root = Root::new(dir.clone());
        let now = Instant::now();
        let j = Jack::new(&root, now);
        assert_eq!(j.status().forced, Some(JackKind::HeadsetWithMic));
        assert_eq!(j.status().route, Some(JackKind::HeadsetWithMic));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_force_persists_and_clears() {
        let dir = std::env::temp_dir().join(format!("walpad-jack-test2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = Root::new(dir.clone());
        let mut j = Jack::new(&root, Instant::now());
        let st = j.set_force(Some(JackKind::Headphones));
        assert_eq!(st.route, Some(JackKind::Headphones));
        assert_eq!(
            std::fs::read_to_string(dir.join("var/lib/walpad/jack-force")).unwrap(),
            "headphones"
        );
        let st = j.set_force(None);
        assert_eq!(st.route, None);
        assert_eq!(
            std::fs::read_to_string(dir.join("var/lib/walpad/jack-force")).unwrap(),
            "none"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
