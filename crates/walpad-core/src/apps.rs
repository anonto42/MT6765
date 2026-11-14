//! App allowlist (/etc/walpad/apps.json): apps launch only by ID, with the contract environment.
//! Never takes a path, shell string or executable name from a client.

mod process;
mod registry;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_open;

pub use registry::Apps;

use serde::Deserialize;
use std::collections::HashMap;
use walpad_hw::Root;

/// Environment every launched app gets (copied from walpad-session; see CONTRACTS.md).
pub const CONTRACT_ENV: &[(&str, &str)] = &[
    ("DISPLAY", ":0"),
    ("HOME", "/root"),
    ("XDG_RUNTIME_DIR", "/run/walpad"),
    ("LIBGL_ALWAYS_SOFTWARE", "1"),
    ("MESA_LOADER_DRIVER_OVERRIDE", "llvmpipe"),
    ("EGL_PLATFORM", "x11"),
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
    /// The app can open one file (`launch` with `args.path`): the core appends `open_args` to
    /// `exec`, replacing the element `{path}` by the validated, symlink-resolved path.
    #[serde(default)]
    pub open: bool,
    #[serde(default = "default_open_args")]
    pub open_args: Vec<String>,
    /// How many copies may run at once (the terminal: several windows, change
    /// multitasking-windows). 0 and 1 mean one.
    #[serde(default)]
    pub max_instances: u32,
}

fn default_open_args() -> Vec<String> {
    vec!["--".into(), "{path}".into()]
}

/// Validate and resolve the file a `launch` may open: syntax (`walpad_proto::valid_open_path`),
/// then the real path after symlinks must still be a regular file under an allowed root. Returns
/// the resolved path as the app will see it (absolute, no symlinks, never starting with `-`).
pub fn resolve_open_path(root: &Root, raw: &str) -> Result<String, String> {
    if !walpad_proto::valid_open_path(raw) {
        return Err("path is not an allowed file path".into());
    }
    let host = std::fs::canonicalize(root.p(raw)).map_err(|_| "no such file".to_string())?;
    let base = std::fs::canonicalize(root.base()).map_err(|e| e.to_string())?;
    let rel = host
        .strip_prefix(&base)
        .map_err(|_| "path leaves the file system root".to_string())?;
    let virt = format!("/{}", rel.to_string_lossy());
    if !walpad_proto::valid_open_path(&virt) {
        return Err("path resolves outside the allowed folders".into());
    }
    if !host.is_file() {
        return Err("not a regular file".into());
    }
    Ok(virt)
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

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    Unknown,
    NotInstalled,
    Spawn(String),
    /// A previous copy is still shutting down after Close app.
    Closing,
    /// The app already runs as many copies as `max_instances` allows (or is single-instance).
    Limit,
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
