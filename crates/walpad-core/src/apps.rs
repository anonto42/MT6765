//! App allowlist (/etc/walpad/apps.json): apps launch only by ID, with the contract environment.
//! Never takes a path, shell string or executable name from a client.
use serde::Deserialize;
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
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
    ("GTK_A11Y", "none"),
    ("NO_AT_BRIDGE", "1"),
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
}

#[derive(Debug, PartialEq, Eq)]
pub enum LaunchError {
    Unknown,
    NotInstalled,
    Spawn(String),
}

#[derive(Default)]
pub struct Apps {
    defs: Vec<AppDef>,
    children: HashMap<String, Child>,
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
        }
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

    /// Reap exited children (non-blocking waitpid; kernel 4.19 has no pidfd_open).
    pub fn reap(&mut self) -> Vec<String> {
        let mut gone = Vec::new();
        self.children.retain(|id, c| match c.try_wait() {
            Ok(Some(_)) | Err(_) => {
                gone.push(id.clone());
                false
            }
            Ok(None) => true,
        });
        gone
    }

    pub fn running(&self, id: &str) -> bool {
        self.children.contains_key(id)
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

    pub fn spawn(&mut self, id: &str, root: &Root) -> Result<(), LaunchError> {
        let d = self.def(id).cloned().ok_or(LaunchError::Unknown)?;
        if !self.installed(&d, root) {
            return Err(LaunchError::NotInstalled);
        }
        let child = Command::new(&d.exec[0])
            .args(&d.exec[1..])
            .envs(CONTRACT_ENV.iter().copied())
            .envs(d.env.iter())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LaunchError::Spawn(e.to_string()))?;
        self.children.insert(id.to_string(), child);
        Ok(())
    }

    /// Kill a running child (used by "Close app").
    pub fn close(&mut self, id: &str) -> bool {
        match self.children.remove(id) {
            Some(mut c) => {
                let _ = c.kill();
                let _ = c.wait();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root_with(apps: &str, bins: &[&str]) -> Root {
        let d =
            std::env::temp_dir().join(format!("walpad-apps-{}-{}", std::process::id(), apps.len()));
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
    fn loads_filters_and_reports_installed_only() {
        let r = root_with(JSON, &["sleep"]);
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
        assert!(a.close("sleeper"));
        assert!(!a.running("sleeper"));
    }

    #[test]
    fn reaps_exited_children() {
        let r = root_with(r#"[{"id":"quick","exec":["true"]}]"#, &["true"]);
        let mut a = Apps::load(&r);
        a.spawn("quick", &r).unwrap();
        let t = std::time::Instant::now();
        let mut gone = vec![];
        while gone.is_empty() && t.elapsed() < std::time::Duration::from_secs(3) {
            gone = a.reap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(gone, vec!["quick".to_string()]);
        assert!(!a.running("quick"));
    }

    #[test]
    fn bad_json_means_no_apps() {
        let r = root_with("not json", &[]);
        assert!(Apps::load(&r).status(&r).is_empty());
    }
}
