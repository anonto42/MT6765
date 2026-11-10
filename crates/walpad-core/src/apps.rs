//! App allowlist (/etc/walpad/apps.json): apps launch only by ID, with the contract environment.
//! Never takes a path, shell string or executable name from a client.
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::Root;
use walpad_proto::AppState;

/// Environment every launched app gets (copied from walpad-session; see CONTRACTS.md).
pub const CONTRACT_ENV: &[(&str, &str)] = &[
    ("DISPLAY", ":0"),
    ("HOME", "/root"),
    ("XDG_RUNTIME_DIR", "/run/walpad"),
    ("LIBGL_ALWAYS_SOFTWARE", "1"),
    ("MOZ_DISABLE_CONTENT_SANDBOX", "1"),
    ("MOZ_DISABLE_GMP_SANDBOX", "1"),
    ("MOZ_DISABLE_RDD_SANDBOX", "1"),
    ("MOZ_DISABLE_SOCKET_PROCESS_SANDBOX", "1"),
    ("MOZ_WEBRENDER", "0"),
    ("MOZ_ACCELERATED", "0"),
    ("PULSE_SERVER", "unix:/run/pulse/native"),
    ("GTK_A11Y", "none"),
    ("NO_AT_BRIDGE", "1"),
    // the shell's XIM server: text-input focus shows and hides the system keyboard (owner request
    // 2026-11-10; walpad-shell ime.rs)
    ("XMODIFIERS", "@im=walpad"),
    ("GTK_IM_MODULE", "xim"),
    ("QT_IM_MODULE", "xim"),
];

const BIN_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

#[derive(Debug, Clone, Deserialize)]
pub struct AppDef {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    pub exec: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub single_instance: bool,
    #[serde(default)]
    pub wm_class: String,
    /// Extra exit codes that count as a normal exit (e.g. a terminal returns its shell's status).
    #[serde(default)]
    pub exit_ok: Vec<i32>,
    /// What a finger drag does in this app's windows: "scroll" (default: wheel events, long
    /// press = right click) or "mouse" (a drag stays a mouse drag). Anything else = default.
    #[serde(default)]
    pub touch: Option<String>,
}

/// How a child we started ended (M6.3: only `Crashed` raises a notice).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Clean,
    /// We asked it to close (Close app): never a crash, whatever the status.
    Closed,
    Crashed {
        code: Option<i32>,
        signal: Option<i32>,
    },
}

struct Kid {
    child: Child,
    /// Set when Close app was requested: SIGTERM sent, SIGKILL after the grace period.
    closing: Option<Instant>,
    killed: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    Unknown,
    NotInstalled,
    Spawn(String),
    /// A previous copy is still shutting down after Close app.
    Closing,
}

/// Terminate a process politely. Refuses pid 0/1 and our own pid.
pub fn terminate_pid(pid: u32, hard: bool) -> bool {
    use rustix::process::{kill_process, Pid, Signal};
    if pid <= 1 || pid == std::process::id() {
        return false;
    }
    let Some(p) = Pid::from_raw(pid as i32) else {
        return false;
    };
    kill_process(p, if hard { Signal::KILL } else { Signal::TERM }).is_ok()
}

#[derive(Default)]
pub struct Apps {
    defs: Vec<AppDef>,
    children: HashMap<String, Kid>,
    /// Apps with a window on screen that we did not start (from the X scan).
    ext: HashSet<String>,
    grace: Option<Duration>,
}

impl Apps {
    pub fn load(root: &Root) -> Self {
        let defs = std::fs::read_to_string(root.p("/etc/walpad/apps.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<AppDef>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|d| !d.id.is_empty() && !d.exec.is_empty())
            .collect();
        Apps {
            defs,
            children: HashMap::new(),
            ext: HashSet::new(),
            grace: None,
        }
    }

    /// Touch behaviour for a window with this WM_CLASS: the matching app's `touch` field, else
    /// the default (scroll).
    pub fn touch_mode(&self, class: &[String]) -> crate::logic::gesture::Mode {
        self.defs
            .iter()
            .filter(|d| !d.wm_class.is_empty() && crate::xbridge::class_matches(class, &d.wm_class))
            .find_map(|d| {
                d.touch
                    .as_deref()
                    .and_then(crate::logic::gesture::Mode::parse)
            })
            .unwrap_or_default()
    }

    pub fn def(&self, id: &str) -> Option<&AppDef> {
        self.defs.iter().find(|d| d.id == id)
    }

    /// M6.2: an app is shown only if its executable exists.
    pub fn installed(&self, d: &AppDef, root: &Root) -> bool {
        let exe = &d.exec[0];
        if exe.starts_with('/') {
            return root.p(exe).is_file();
        }
        BIN_DIRS
            .iter()
            .any(|dir| root.p(&format!("{dir}/{exe}")).is_file())
    }

    pub fn defs(&self) -> &[AppDef] {
        &self.defs
    }

    /// How long a closing child gets before SIGKILL.
    pub fn set_close_grace(&mut self, d: Duration) {
        self.grace = Some(d);
    }
    fn grace(&self) -> Duration {
        self.grace.unwrap_or(Duration::from_secs(3))
    }

    /// Reap exited children (non-blocking waitpid; kernel 4.19 has no pidfd_open) and escalate
    /// closing children that ignored SIGTERM.
    pub fn reap(&mut self, now: Instant) -> Vec<(String, Exit)> {
        let grace = self.grace();
        let mut gone = Vec::new();
        for (id, k) in self.children.iter_mut() {
            match k.child.try_wait() {
                Ok(Some(st)) => {
                    let exit = if k.closing.is_some() {
                        Exit::Closed
                    } else if st.success()
                        || self.defs.iter().any(|d| {
                            &d.id == id && st.code().is_some_and(|c| d.exit_ok.contains(&c))
                        })
                    {
                        Exit::Clean
                    } else {
                        Exit::Crashed {
                            code: st.code(),
                            signal: st.signal(),
                        }
                    };
                    gone.push((id.clone(), exit));
                }
                Err(_) => gone.push((id.clone(), Exit::Closed)),
                Ok(None) => {
                    if let Some(t) = k.closing {
                        if !k.killed && now.duration_since(t) >= grace {
                            let _ = k.child.kill();
                            k.killed = true;
                        }
                    }
                }
            }
        }
        for (id, _) in &gone {
            self.children.remove(id);
            self.ext.remove(id);
        }
        gone
    }

    /// Do we own a live (possibly closing) child for this app?
    pub fn owns(&self, id: &str) -> bool {
        self.children.contains_key(id)
    }

    /// Running = a child we started (not already closing) or a window found on screen.
    pub fn running(&self, id: &str) -> bool {
        match self.children.get(id) {
            Some(k) => k.closing.is_none(),
            None => self.ext.contains(id),
        }
    }

    /// Apps with a wm_class we can look for on screen.
    pub fn watches_windows(&self) -> bool {
        self.defs.iter().any(|d| !d.wm_class.is_empty())
    }

    /// Replace the set of apps seen on screen. Returns (id, now_running) for apps that are not
    /// our children (children report through `reap`).
    pub fn set_ext(&mut self, seen: HashSet<String>) -> Vec<(String, bool)> {
        let mut changes = Vec::new();
        for id in seen.difference(&self.ext) {
            if !self.children.contains_key(id) {
                changes.push((id.clone(), true));
            }
        }
        for id in self.ext.difference(&seen) {
            if !self.children.contains_key(id) {
                changes.push((id.clone(), false));
            }
        }
        self.ext = seen;
        changes
    }

    pub fn status(&self, root: &Root) -> Vec<AppState> {
        self.defs
            .iter()
            .filter(|d| self.installed(d, root))
            .map(|d| AppState {
                app: d.id.clone(),
                running: self.running(&d.id),
            })
            .collect()
    }

    /// Start an app; returns its pid.
    pub fn spawn(&mut self, id: &str, root: &Root) -> Result<u32, LaunchError> {
        let d = self.def(id).cloned().ok_or(LaunchError::Unknown)?;
        if !self.installed(&d, root) {
            return Err(LaunchError::NotInstalled);
        }
        if let Some(k) = self.children.get_mut(id) {
            // an old copy that is still alive must not be forgotten (it would never be reaped)
            if matches!(k.child.try_wait(), Ok(None)) {
                return Err(LaunchError::Closing);
            }
        }
        // The app's own error output goes to /var/log/walpad-app-<id>.log (ids are lower-case slugs),
        // so a crash on the tablet can be diagnosed; it falls back to /dev/null.
        let errlog = || {
            let p = root.p(&format!("/var/log/walpad-app-{id}.log"));
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .map(Stdio::from)
                .unwrap_or_else(|_| Stdio::null())
        };
        let child = Command::new(&d.exec[0])
            .args(&d.exec[1..])
            .envs(CONTRACT_ENV.iter().copied())
            .envs(d.env.iter())
            .stdin(Stdio::null())
            .stdout(errlog())
            .stderr(errlog())
            .spawn()
            .map_err(|e| LaunchError::Spawn(e.to_string()))?;
        let pid = child.id();
        self.children.insert(
            id.to_string(),
            Kid {
                child,
                closing: None,
                killed: false,
            },
        );
        Ok(pid)
    }

    /// Close an app we started: SIGTERM now, SIGKILL after the grace period; its exit is
    /// reported as `Exit::Closed`, never as a crash.
    pub fn request_close(&mut self, id: &str, now: Instant) -> bool {
        match self.children.get_mut(id) {
            Some(k) if k.closing.is_none() => {
                k.closing = Some(now);
                terminate_pid(k.child.id(), false);
                true
            }
            // already closing: nothing more to do (and nothing new was closed)
            Some(_) => false,
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with(apps: &str, bins: &[&str]) -> Root {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("walpad-apps-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
        std::fs::create_dir_all(d.join("usr/bin")).unwrap();
        std::fs::write(d.join("etc/walpad/apps.json"), apps).unwrap();
        for b in bins {
            std::fs::write(d.join("usr/bin").join(b), "").unwrap();
        }
        Root::new(d)
    }

    const JSON: &str = r#"[
        {"id":"sleeper","name":"S","exec":["sleep","30"],"single_instance":true,"wm_class":"sleeper"},
        {"id":"ghost","name":"G","exec":["definitely-not-installed"]},
        {"id":"","exec":["x"]},
        {"id":"noexec","exec":[]}
    ]"#;

    #[test]
    fn touch_mode_per_app_with_scroll_as_the_default() {
        use crate::logic::gesture::Mode;
        let r = root_with(
            r#"[
              {"id":"term","exec":["st"],"wm_class":"st","touch":"mouse"},
              {"id":"web","exec":["firefox-esr"],"wm_class":"firefox-esr","touch":"scroll"},
              {"id":"odd","exec":["x"],"wm_class":"oddapp","touch":"bogus"},
              {"id":"none","exec":["y"],"wm_class":"plain"}
            ]"#,
            &[],
        );
        let a = Apps::load(&r);
        let c = |i: &str, k: &str| vec![i.to_string(), k.to_string()];
        assert_eq!(a.touch_mode(&c("st", "St")), Mode::Mouse);
        assert_eq!(a.touch_mode(&c("Navigator", "Firefox-esr")), Mode::Scroll);
        assert_eq!(a.touch_mode(&c("oddapp", "Oddapp")), Mode::Scroll);
        assert_eq!(a.touch_mode(&c("plain", "Plain")), Mode::Scroll);
        assert_eq!(a.touch_mode(&c("stranger", "Stranger")), Mode::Scroll);
        assert_eq!(a.touch_mode(&[]), Mode::Scroll);
    }

    #[test]
    fn loads_filters_and_reports_installed_only() {
        // The child ignores SIGTERM, so it is still alive while "closing" (a plain `sleep` can die
        // in milliseconds and make the Closing assertion below race); SIGKILL ends it after the grace period.
        let json = JSON.replace(
            r#"["sleep","30"]"#,
            r#"["sh","-c","trap '' TERM; exec sleep 30"]"#,
        );
        let r = root_with(&json, &["sh"]);
        let mut a = Apps::load(&r);
        assert!(a.def("noexec").is_none() && a.def("").is_none());
        let st = a.status(&r);
        assert_eq!(
            st,
            vec![AppState {
                app: "sleeper".into(),
                running: false
            }],
            "ghost is not installed, so it is hidden"
        );
        assert_eq!(a.spawn("ghost", &r), Err(LaunchError::NotInstalled));
        assert_eq!(a.spawn("rm -rf /", &r), Err(LaunchError::Unknown));
        a.spawn("sleeper", &r).unwrap();
        assert!(a.running("sleeper"));
        assert!(a.status(&r)[0].running);
        assert!(a.request_close("sleeper", Instant::now()));
        assert!(!a.running("sleeper"), "closing counts as not running");
        assert_eq!(
            a.spawn("sleeper", &r),
            Err(LaunchError::Closing),
            "an old copy must not be forgotten"
        );
        let gone = reap_all(&mut a, 1);
        assert_eq!(gone, vec![("sleeper".to_string(), Exit::Closed)]);
        assert!(!a.owns("sleeper"));
    }

    fn reap_all(a: &mut Apps, want: usize) -> Vec<(String, Exit)> {
        let t = Instant::now();
        let mut gone = vec![];
        while gone.len() < want && t.elapsed() < Duration::from_secs(5) {
            gone.extend(a.reap(Instant::now()));
            std::thread::sleep(Duration::from_millis(20));
        }
        gone
    }

    #[test]
    fn app_that_disappears_from_disk_is_unlisted_again() {
        let r = root_with(JSON, &["sleep"]);
        let a = Apps::load(&r);
        assert_eq!(a.status(&r).len(), 1);
        std::fs::remove_file(r.p("/usr/bin/sleep")).unwrap();
        assert!(a.status(&r).is_empty(), "missing executable: not listed");
    }

    #[test]
    fn exit_kinds_clean_crash_signal_exit_ok_and_closed() {
        let json = r#"[
            {"id":"ok","exec":["true"]},
            {"id":"bad","exec":["false"]},
            {"id":"term","exec":["sh","-c","kill -9 $$"]},
            {"id":"shellish","exec":["false"],"exit_ok":[1]}
        ]"#;
        let r = root_with(json, &["true", "false", "sh"]);
        let mut a = Apps::load(&r);
        for id in ["ok", "bad", "term", "shellish"] {
            a.spawn(id, &r).unwrap();
        }
        let mut gone = reap_all(&mut a, 4);
        gone.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(
            gone,
            vec![
                (
                    "bad".to_string(),
                    Exit::Crashed {
                        code: Some(1),
                        signal: None
                    }
                ),
                ("ok".to_string(), Exit::Clean),
                ("shellish".to_string(), Exit::Clean),
                (
                    "term".to_string(),
                    Exit::Crashed {
                        code: None,
                        signal: Some(9)
                    }
                ),
            ]
        );
    }

    #[test]
    fn closing_child_that_ignores_sigterm_is_killed_after_the_grace_period() {
        let json =
            r#"[{"id":"stubborn","exec":["sh","-c","trap '' TERM; while :; do sleep 1; done"]}]"#;
        let r = root_with(json, &["sh"]);
        let mut a = Apps::load(&r);
        a.set_close_grace(Duration::from_millis(300));
        a.spawn("stubborn", &r).unwrap();
        std::thread::sleep(Duration::from_millis(200)); // let sh install its trap
        assert!(a.request_close("stubborn", Instant::now()));
        let gone = reap_all(&mut a, 1);
        assert_eq!(gone, vec![("stubborn".to_string(), Exit::Closed)]);
    }

    #[test]
    fn ext_windows_count_as_running_and_report_changes() {
        let r = root_with(JSON, &["sleep"]);
        let mut a = Apps::load(&r);
        let set = |ids: &[&str]| ids.iter().map(|s| s.to_string()).collect::<HashSet<_>>();
        assert_eq!(
            a.set_ext(set(&["sleeper"])),
            vec![("sleeper".to_string(), true)]
        );
        assert!(a.running("sleeper") && a.status(&r)[0].running);
        assert!(a.set_ext(set(&["sleeper"])).is_empty());
        assert_eq!(a.set_ext(set(&[])), vec![("sleeper".to_string(), false)]);
        assert!(!a.running("sleeper"));
    }

    #[test]
    fn reaps_exited_children() {
        let r = root_with(r#"[{"id":"quick","exec":["true"]}]"#, &["true"]);
        let mut a = Apps::load(&r);
        a.spawn("quick", &r).unwrap();
        let gone = reap_all(&mut a, 1);
        assert_eq!(gone, vec![("quick".to_string(), Exit::Clean)]);
        assert!(!a.running("quick"));
    }

    #[test]
    fn bad_json_means_no_apps() {
        let r = root_with("not json", &[]);
        assert!(Apps::load(&r).status(&r).is_empty());
    }
}
