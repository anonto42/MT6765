//! App process spawning, reaping, and termination.

use super::{terminate_pid, AppDef, Apps, Exit, LaunchError, CONTRACT_ENV};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command, Stdio};
use std::time::Instant;
use walpad_hw::Root;

pub(super) struct Kid {
    pub(super) child: Child,
    /// Set when Close app was requested: SIGTERM sent, SIGKILL after the grace period.
    pub(super) closing: Option<Instant>,
    pub(super) killed: bool,
}

/// Key of instance `n` (1-based) of an app in the child table: the app id for the first copy,
/// `<id>#<n>` for the others (app ids never contain '#').
pub(super) fn instance_key(id: &str, n: u32) -> String {
    if n <= 1 {
        id.to_string()
    } else {
        format!("{id}#{n}")
    }
}

/// App id and 1-based instance number of a child-table key.
pub(super) fn split_key(key: &str) -> (&str, u32) {
    match key.split_once('#') {
        Some((id, n)) => (id, n.parse().unwrap_or(1)),
        None => (key, 1),
    }
}

impl Apps {
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
                            d.id == split_key(id).0
                                && st.code().is_some_and(|c| d.exit_ok.contains(&c))
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
        for (key, _) in &gone {
            self.children.remove(key);
            // an app that still has another copy keeps its running mark
            let base = split_key(key).0;
            if !self.children.keys().any(|k| split_key(k).0 == base) {
                self.ext.remove(base);
            }
        }
        gone.into_iter()
            .map(|(k, e)| (split_key(&k).0.to_string(), e))
            .collect()
    }

    /// Start an app; returns its pid.
    pub fn spawn(&mut self, id: &str, root: &Root) -> Result<u32, LaunchError> {
        self.spawn_open(id, root, None)
    }

    /// The argv of an app, with the file to open (already resolved) when there is one.
    pub fn argv(d: &AppDef, path: Option<&str>) -> Vec<String> {
        let mut v = d.exec.clone();
        if let Some(p) = path {
            v.extend(d.open_args.iter().map(|a| {
                if a == "{path}" {
                    p.to_string()
                } else {
                    a.clone()
                }
            }));
        }
        v
    }

    /// Start an app, optionally opening one file (`path` is the output of `resolve_open_path`).
    pub fn spawn_open(
        &mut self,
        id: &str,
        root: &Root,
        path: Option<&str>,
    ) -> Result<u32, LaunchError> {
        self.spawn_key(id, 1, root, path)
    }

    /// Start one more copy of an app that allows several (`max_instances`): the lowest free
    /// instance number. Returns (pid, instance number).
    pub fn spawn_new(&mut self, id: &str, root: &Root) -> Result<(u32, u32), LaunchError> {
        let d = self.def(id).ok_or(LaunchError::Unknown)?;
        let max = d.max_instances.max(1);
        let n = (1..=max)
            .find(|n| !self.children.contains_key(&instance_key(id, *n)))
            .ok_or(LaunchError::Limit)?;
        self.spawn_key(id, n, root, None).map(|pid| (pid, n))
    }

    fn spawn_key(
        &mut self,
        id: &str,
        n: u32,
        root: &Root,
        path: Option<&str>,
    ) -> Result<u32, LaunchError> {
        let key = instance_key(id, n);
        let d = self.def(id).cloned().ok_or(LaunchError::Unknown)?;
        if path.is_some() && !d.open {
            return Err(LaunchError::Spawn("this app does not open files".into()));
        }
        let argv = Self::argv(&d, path);
        if !self.installed(&d, root) {
            return Err(LaunchError::NotInstalled);
        }
        if let Some(k) = self.children.get_mut(&key) {
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
        let (theme_k, theme_v) = Self::theme_env(root);
        let child = Command::new(&argv[0])
            .args(&argv[1..])
            .envs(CONTRACT_ENV.iter().copied())
            .envs(self.proxy_env())
            .env(theme_k, theme_v)
            .envs(d.env.iter())
            .stdin(Stdio::null())
            .stdout(errlog())
            .stderr(errlog())
            .spawn()
            .map_err(|e| LaunchError::Spawn(e.to_string()))?;
        let pid = child.id();
        self.children.insert(
            key,
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
        let mut any = false;
        for (key, k) in self.children.iter_mut() {
            // every copy of the app; copies that are already closing are left alone
            if split_key(key).0 == id && k.closing.is_none() {
                k.closing = Some(now);
                terminate_pid(k.child.id(), false);
                any = true;
            }
        }
        any
    }
}
