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

mod parse;
mod poll;
#[cfg(test)]
mod tests;

pub use parse::parse_state;
use parse::{Job, Run};

pub struct Volume {
    script: PathBuf,
    file: PathBuf,
    pub(super) out: PathBuf,
    base: Option<PathBuf>,
    pub(super) st: AudioStatus,
    run: Option<Run>,
    /// An `apply` of the current state is wanted.
    pub(super) pending: bool,
    pub(super) next_probe: Instant,
    /// Save the state at this time (coalesces slider drags into one write).
    pub(super) save_at: Option<Instant>,
    pub(super) backend: Option<String>,
    timeout: Duration,
    pub(super) probe_every: Duration,
    /// The running test tone (`walpad-volume test`) and when to give up on it.
    pub(super) tone: Option<(Child, Instant)>,
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

    pub(super) fn save(&mut self) {
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
}
