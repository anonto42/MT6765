//! Output volume (owner request 2026-11-10). The core owns the value (0-100 % and a mute flag,
//! persisted in /var/lib/walpad/volume as `<percent> <muted 0|1>`); everything that touches the
//! sound system is done by the fixed script /usr/local/sbin/walpad-volume, run as a child with a
//! timeout and polled from the daemon loop (never waited on):
//!   `apply <percent> <0|1>`  set volume and mute (sound server software volume, else the codec's
//!                            PGA gain), `get` report `volume= muted= available= backend=`.
//! Rapid changes (a slider drag) coalesce: at most one child runs, the newest wanted state is
//! applied when it ends. A `get` every 10 s notices a sound server that came up (or went away) later
//! than the core and re-applies the stored state then.
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::AudioStatus;

pub const SCRIPT: &str = "/usr/local/sbin/walpad-volume";
pub const FILE: &str = "/var/lib/walpad/volume";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
const PROBE_EVERY: Duration = Duration::from_secs(10);
/// A test tone lasts 1 s; the script bounds its players at 6 s.
const TONE_TIMEOUT: Duration = Duration::from_secs(12);
const SAVE_DELAY: Duration = Duration::from_millis(800);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Job {
    Get,
    Apply,
}

struct Run {
    job: Job,
    child: Child,
    deadline: Instant,
}

pub struct Volume {
    script: PathBuf,
    file: PathBuf,
    out: PathBuf,
    base: Option<PathBuf>,
    st: AudioStatus,
    run: Option<Run>,
    /// An `apply` of the current state is wanted.
    pending: bool,
    next_probe: Instant,
    /// Save the state at this time (coalesces slider drags into one write).
    save_at: Option<Instant>,
    backend: Option<String>,
    timeout: Duration,
    probe_every: Duration,
    /// The running test tone (`walpad-volume test`) and when to give up on it.
    tone: Option<(Child, Instant)>,
}

/// One `key=value` line of the script's output.
fn field<'a>(out: &'a str, key: &str) -> Option<&'a str> {
    out.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v.trim())
}

/// `<percent> <muted>` of the state file.
pub fn parse_state(text: &str) -> Option<(u8, bool)> {
    let mut it = text.split_whitespace();
    let v: u32 = it.next()?.parse().ok()?;
    let m = it.next().map(|m| m == "1").unwrap_or(false);
    (v <= 100).then_some((v as u8, m))
}

impl Volume {
    pub fn new(root: &Root, now: Instant) -> Self {
        let mut st = AudioStatus::default();
        let file = root.p(FILE);
        if let Some((v, m)) = std::fs::read_to_string(&file)
            .ok()
            .and_then(|t| parse_state(&t))
        {
            st.volume = v;
            st.muted = m;
        }
        Volume {
            script: root.p(SCRIPT),
            file,
            out: root.p("/run/walpad/volume-out"),
            base: (root.base() != std::path::Path::new("/")).then(|| root.base().to_path_buf()),
            st,
            run: None,
            pending: false,
            next_probe: now,
            save_at: None,
            backend: None,
            timeout: Duration::from_secs(8),
            probe_every: PROBE_EVERY,
            tone: None,
        }
    }

    /// Test hook: how long a script run may take.
    pub fn set_timeout(&mut self, d: Duration) {
        self.timeout = d;
    }

    /// Test hook: how often `get` runs.
    pub fn set_probe_every(&mut self, d: Duration) {
        self.probe_every = d;
    }

    pub fn status(&self) -> AudioStatus {
        self.st
    }

    /// `volume_test`: play the 1 s tone through the default sink. False while one is still playing.
    pub fn test_tone(&mut self, now: Instant) -> Result<bool, String> {
        if self.tone.is_some() {
            return Ok(false);
        }
        let mut cmd = Command::new(&self.script);
        cmd.arg("test")
            .env_clear()
            .env("PATH", PATH)
            .env("HOME", "/root")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(b) = &self.base {
            cmd.env("WALPAD_VOLUME_ROOT", b);
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "walpad-volume is not installed".to_string(),
            _ => format!("cannot run walpad-volume: {e}"),
        })?;
        self.tone = Some((child, now + TONE_TIMEOUT));
        Ok(true)
    }

    fn touch(&mut self, now: Instant) {
        self.pending = true;
        self.save_at = Some(now + SAVE_DELAY);
    }

    pub fn set_volume(&mut self, percent: i32, now: Instant) -> AudioStatus {
        let v = percent.clamp(0, 100) as u8;
        if v != self.st.volume {
            self.st.volume = v;
            self.touch(now);
        }
        self.st
    }

    /// Relative change; raising the volume of a muted output unmutes it.
    pub fn delta(&mut self, delta: i32, now: Instant) -> AudioStatus {
        let v = (i32::from(self.st.volume) + delta).clamp(0, 100) as u8;
        let unmute = delta > 0 && self.st.muted;
        if v != self.st.volume || unmute {
            self.st.volume = v;
            if unmute {
                self.st.muted = false;
            }
            self.touch(now);
        }
        self.st
    }

    pub fn set_mute(&mut self, on: bool, now: Instant) -> AudioStatus {
        if on != self.st.muted {
            self.st.muted = on;
            self.touch(now);
        }
        self.st
    }

    fn spawn(&mut self, job: Job, now: Instant) {
        let _ = std::fs::create_dir_all(self.out.parent().unwrap_or(std::path::Path::new("/")));
        let Ok(out) = std::fs::File::create(&self.out) else {
            return;
        };
        let mut cmd = Command::new(&self.script);
        match job {
            Job::Get => cmd.arg("get"),
            Job::Apply => cmd.args([
                "apply",
                &self.st.volume.to_string(),
                if self.st.muted { "1" } else { "0" },
            ]),
        };
        cmd.env_clear()
            .env("PATH", PATH)
            .env("HOME", "/root")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(Stdio::null());
        if let Some(b) = &self.base {
            cmd.env("WALPAD_VOLUME_ROOT", b);
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        match cmd.spawn() {
            Ok(child) => {
                self.run = Some(Run {
                    job,
                    child,
                    deadline: now + self.timeout,
                });
            }
            Err(_) => {
                // not installed (or not executable): nothing can change the volume
                self.st.available = false;
            }
        }
    }

    fn kill(mut r: Run) {
        if let Some(pid) = rustix::process::Pid::from_raw(r.child.id() as i32) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = r.child.kill();
        let _ = r.child.wait();
    }

    fn save(&mut self) {
        let text = format!("{} {}\n", self.st.volume, u8::from(self.st.muted));
        if let Some(d) = self.file.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let mut tmp = self.file.as_os_str().to_owned();
        tmp.push(".tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &self.file);
        }
    }

    /// Poll the child, start the next job, write the state file. True when the status changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        let before = self.st;
        if let Some((child, deadline)) = self.tone.as_mut() {
            match child.try_wait() {
                Ok(None) if now < *deadline => {}
                Ok(None) => {
                    if let Some((c, _)) = self.tone.take() {
                        Self::kill(Run {
                            job: Job::Get,
                            child: c,
                            deadline: now,
                        });
                    }
                }
                _ => self.tone = None,
            }
        }
        if let Some(t) = self.save_at {
            if now >= t {
                self.save_at = None;
                self.save();
            }
        }
        if let Some(r) = self.run.as_mut() {
            match r.child.try_wait() {
                Ok(Some(status)) => {
                    let Some(r) = self.run.take() else {
                        return false;
                    };
                    let text = std::fs::read_to_string(&self.out).unwrap_or_default();
                    self.finish(r.job, status.success(), &text);
                }
                Ok(None) if now >= r.deadline => {
                    if let Some(r) = self.run.take() {
                        Self::kill(r);
                    }
                    self.st.available = false;
                }
                Ok(None) => {}
                Err(_) => {
                    self.run = None;
                    self.st.available = false;
                }
            }
        }
        if self.run.is_none() {
            if self.pending {
                self.pending = false;
                self.spawn(Job::Apply, now);
            } else if now >= self.next_probe {
                self.next_probe = now + self.probe_every;
                self.spawn(Job::Get, now);
            }
        }
        self.st != before
    }

    fn finish(&mut self, job: Job, ok: bool, out: &str) {
        let avail = ok && field(out, "available") == Some("1");
        self.st.available = avail;
        if job == Job::Get {
            let backend = field(out, "backend").map(str::to_string);
            // a sound server that started (or stopped) since the last probe: apply the stored state
            if avail && backend != self.backend {
                self.pending = true;
            }
            self.backend = backend;
        } else if avail {
            self.backend = field(out, "backend")
                .map(str::to_string)
                .or(self.backend.take());
        }
    }
}

impl Drop for Volume {
    fn drop(&mut self) {
        if let Some(r) = self.run.take() {
            Self::kill(r);
        }
        if let Some((mut c, _)) = self.tone.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if self.save_at.is_some() {
            self.save();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_file_parsing() {
        assert_eq!(parse_state("70 0\n"), Some((70, false)));
        assert_eq!(parse_state("0 1"), Some((0, true)));
        assert_eq!(parse_state("55"), Some((55, false)));
        assert_eq!(parse_state("101 0"), None);
        assert_eq!(parse_state("x"), None);
        assert_eq!(parse_state(""), None);
    }

    #[test]
    fn output_fields() {
        let o = "volume=40\nmuted=1\navailable=1\nbackend=pulse\n";
        assert_eq!(field(o, "volume"), Some("40"));
        assert_eq!(field(o, "backend"), Some("pulse"));
        assert_eq!(field(o, "nope"), None);
    }
}
