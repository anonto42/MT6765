//! App store (change `app-catalogue-manager`): the shipped catalogue, installs and uninstalls
//! through the fixed helper /usr/local/sbin/walpad-pkg, registering installed apps in `apps.d`, and
//! the default apps per role.
//!
//! Safety model: clients only ever name a catalogue *id*. The package names come from
//! /etc/walpad/catalogue.json (shipped read-only data), the helper is run by its fixed path with an
//! empty environment and its own process group, one job at a time, bounded in time, and a free
//! space check (the 2 GiB reserve) runs before anything is started. System entries are never
//! uninstalled. The apps.d entry is written only after the helper reported success.

mod catalogue;
mod defaults;
mod jobs;
#[cfg(test)]
mod tests;

pub use catalogue::{
    check_space, installed_packages, is_installed, valid_package, Catalogue, Entry, CATALOGUE,
    DEFAULTS, MIMEAPPS, RESERVE_KB, ROLE_MIMES, SCRIPT,
};
pub use defaults::{effective, Defaults};

use catalogue::{INSTALL_TIMEOUT, OUT, REMOVE_TIMEOUT};
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::process::Child;
use std::time::{Duration, Instant};
use walpad_hw::{sysinfo, Root};
use walpad_proto::{appdefs::APPS_D, DEFAULT_ROLES};

#[derive(Debug, PartialEq, Eq)]
pub enum PkgError {
    Busy,
    /// Not a catalogue id, role or app.
    Unknown(String),
    /// Known but not allowed (system app, already installed, not installed, no space ...).
    Refused(String),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Install,
    Remove,
}

impl Op {
    pub fn name(self) -> &'static str {
        match self {
            Op::Install => "install",
            Op::Remove => "remove",
        }
    }
}

struct Job {
    op: Op,
    id: String,
    child: Child,
    deadline: Instant,
    off: u64,
    partial: String,
    percent: u8,
    text: String,
    ok: bool,
    error: Option<String>,
}

pub struct Pkg {
    root: Root,
    script: PathBuf,
    out: PathBuf,
    job: Option<Job>,
    notices: Vec<Value>,
    reserve_kb: Option<u64>,
    timeouts: [Duration; 2],
}

/// Short printable text from the helper (never trusted to be tidy).
fn clean(s: &str) -> String {
    let t: String = s.chars().filter(|c| !c.is_control()).take(160).collect();
    t.trim().to_string()
}

impl Pkg {
    pub fn new(root: &Root) -> Pkg {
        Pkg {
            root: root.clone(),
            script: root.p(SCRIPT),
            out: root.p(OUT),
            job: None,
            notices: Vec::new(),
            reserve_kb: None,
            timeouts: [INSTALL_TIMEOUT, REMOVE_TIMEOUT],
        }
    }

    /// Test hooks: a different free-space reserve and job timeouts (install, remove).
    pub fn set_reserve_kb(&mut self, kb: u64) {
        self.reserve_kb = Some(kb);
    }

    pub fn set_timeouts(&mut self, install: Duration, remove: Duration) {
        self.timeouts = [install, remove];
    }

    pub fn busy_with(&self, id: &str) -> bool {
        self.job.as_ref().is_some_and(|j| j.id == id)
    }

    pub fn take_notices(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.notices)
    }

    fn reserve(&self, cat: &Catalogue) -> u64 {
        self.reserve_kb.unwrap_or(cat.reserve_kb)
    }

    /// The `pkg_catalogue` answer. Also re-registers an app whose package is installed but whose
    /// apps.d entry is missing (a core restart between apk and the registration).
    pub fn catalogue_json(&self) -> Value {
        let cat = Catalogue::load(&self.root);
        let pk = installed_packages(&self.root);
        for e in &cat.entries {
            if !e.system && is_installed(e, &pk) && !self.registered(e) {
                let _ = self.register(e);
            }
        }
        let entries: Vec<Value> = cat
            .entries
            .iter()
            .map(|e| {
                json!({
                    "id": e.id, "name": e.name, "category": e.category, "summary": e.summary,
                    "size_kb": e.size_kb, "installed": is_installed(e, &pk), "system": e.system,
                    "roles": e.roles, "icon": e.icon,
                })
            })
            .collect();
        let defaults: Map<String, Value> = DEFAULT_ROLES
            .iter()
            .map(|r| {
                (
                    r.to_string(),
                    effective(r, &cat, &pk, &Defaults::load(&self.root))
                        .map_or(Value::Null, Value::from),
                )
            })
            .collect();
        let free_kb = sysinfo::disk_usage(&self.root).map(|(_, f)| f / 1024);
        let busy = self
            .job
            .as_ref()
            .map(|j| json!({"op": j.op.name(), "id": j.id, "percent": j.percent, "text": j.text}));
        json!({
            "entries": entries,
            "free_kb": free_kb,
            "reserve_kb": self.reserve(&cat),
            "busy": busy,
            "defaults": defaults,
        })
    }

    fn registered(&self, e: &Entry) -> bool {
        e.exec.is_empty() || self.root.p(&format!("{APPS_D}/{}.json", e.id)).is_file()
    }
}
