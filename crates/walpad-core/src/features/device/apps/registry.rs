//! App allowlist registry, installed checks, status, and external window tracking.

use super::process::{split_key, Kid};
use super::AppDef;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use walpad_hw::Root;
use walpad_proto::AppState;

const BIN_DIRS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

#[derive(Default)]
pub struct Apps {
    pub(super) defs: Vec<AppDef>,
    pub(super) children: HashMap<String, Kid>,
    /// Apps with a window on screen that we did not start (from the X scan).
    pub(super) ext: HashSet<String>,
    pub(super) grace: Option<Duration>,
    /// System HTTP proxy for launched apps (vpn-and-dns; never carries credentials).
    pub(super) proxy: Option<String>,
    /// A file to open as soon as the old copy of the app (asked to close) is gone.
    pub(super) pending_open: HashMap<String, String>,
}

impl Apps {
    /// The shipped /etc/walpad/apps.json plus the apps the app store registered in apps.d.
    fn read_defs(root: &Root) -> Vec<AppDef> {
        walpad_proto::appdefs::entries(
            &root.p(walpad_proto::appdefs::APPS_JSON),
            &root.p(walpad_proto::appdefs::APPS_D),
        )
        .into_iter()
        .filter_map(|v| serde_json::from_value::<AppDef>(v).ok())
        .filter(|d| !d.id.is_empty() && !d.exec.is_empty())
        .collect()
    }

    pub fn load(root: &Root) -> Self {
        Apps {
            defs: Self::read_defs(root),
            children: HashMap::new(),
            ext: HashSet::new(),
            grace: None,
            proxy: None,
            pending_open: HashMap::new(),
        }
    }

    /// Remember a file to open when `id` has exited (it was asked to close).
    pub fn set_pending_open(&mut self, id: &str, path: String) {
        self.pending_open.insert(id.to_string(), path);
    }

    pub fn take_pending_open(&mut self, id: &str) -> Option<String> {
        self.pending_open.remove(id)
    }

    /// Set (or clear) the system proxy exported to apps started from now on.
    pub fn set_proxy(&mut self, proxy: Option<&str>) {
        self.proxy = proxy.map(str::to_string);
    }

    /// GTK theme env var matching the device theme in `/etc/walpad/shell.json`.
    pub(super) fn theme_env(root: &Root) -> (&'static str, &'static str) {
        let light = std::fs::read_to_string(root.p("/etc/walpad/shell.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| {
                v.get("theme")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "light")
            })
            .unwrap_or(false);
        (
            "GTK_THEME",
            if light {
                "Adwaita:light"
            } else {
                "Adwaita:dark"
            },
        )
    }

    /// The proxy variables an app gets (lower and upper case, as tools differ).
    pub(super) fn proxy_env(&self) -> Vec<(&'static str, String)> {
        match &self.proxy {
            None => Vec::new(),
            Some(p) => ["http_proxy", "https_proxy", "HTTP_PROXY", "HTTPS_PROXY"]
                .into_iter()
                .map(|k| (k, p.clone()))
                .chain(["no_proxy", "NO_PROXY"].map(|k| (k, "localhost,127.0.0.1,::1".to_string())))
                .collect(),
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

    /// Read the definitions again (an app was installed or uninstalled). Running children stay.
    pub fn reload(&mut self, root: &Root) {
        self.defs = Self::read_defs(root);
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

    pub(super) fn grace(&self) -> Duration {
        self.grace.unwrap_or(Duration::from_secs(3))
    }

    /// Keys of the children of an app (every instance).
    pub(super) fn keys_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a String> + 'a {
        self.children.keys().filter(move |k| split_key(k).0 == id)
    }

    /// Do we own a live (possibly closing) child for this app?
    pub fn owns(&self, id: &str) -> bool {
        self.keys_of(id).next().is_some()
    }

    /// Running = a child we started (not already closing) or a window found on screen.
    pub fn running(&self, id: &str) -> bool {
        if !self.owns(id) {
            return self.ext.contains(id);
        }
        self.keys_of(id)
            .any(|k| self.children.get(k).is_some_and(|c| c.closing.is_none()))
    }

    /// How many copies of an app may run at once (at least 1).
    pub fn max_instances(&self, id: &str) -> u32 {
        self.def(id).map(|d| d.max_instances.max(1)).unwrap_or(1)
    }

    /// Copies we started that are not closing.
    pub fn instance_count(&self, id: &str) -> usize {
        self.keys_of(id)
            .filter(|k| self.children.get(*k).is_some_and(|c| c.closing.is_none()))
            .count()
    }

    /// App id and instance number of the child with this process id.
    pub fn instance_of_pid(&self, pid: u32) -> Option<(String, u32)> {
        self.children
            .iter()
            .find(|(_, k)| k.child.id() == pid)
            .map(|(key, _)| {
                let (id, n) = split_key(key);
                (id.to_string(), n)
            })
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
}
