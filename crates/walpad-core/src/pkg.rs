//! App store (change `app-catalogue-manager`): the shipped catalogue, installs and uninstalls
//! through the fixed helper /usr/local/sbin/walpad-pkg, registering installed apps in `apps.d`, and
//! the default apps per role.
//!
//! Safety model: clients only ever name a catalogue *id*. The package names come from
//! /etc/walpad/catalogue.json (shipped read-only data), the helper is run by its fixed path with an
//! empty environment and its own process group, one job at a time, bounded in time, and a free
//! space check (the 2 GiB reserve) runs before anything is started. System entries are never
//! uninstalled. The apps.d entry is written only after the helper reported success.
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use walpad_hw::{sysinfo, Root};
use walpad_proto::{appdefs::APPS_D, valid_app_id, DEFAULT_ROLES};

pub const SCRIPT: &str = "/usr/local/sbin/walpad-pkg";
pub const CATALOGUE: &str = "/etc/walpad/catalogue.json";
pub const DEFAULTS: &str = "/etc/walpad/defaults.json";
pub const MIMEAPPS: &str = "/root/.config/mimeapps.list";
const APK_DB: &str = "/lib/apk/db/installed";
const OUT: &str = "/run/walpad/pkg-out";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
/// Free space that must remain after an install (2 GiB).
pub const RESERVE_KB: u64 = 2 * 1024 * 1024;
const INSTALL_TIMEOUT: Duration = Duration::from_secs(15 * 60 + 30);
const REMOVE_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// File types each role opens (what mimeapps.list gets).
pub const ROLE_MIMES: &[(&str, &[&str])] = &[
    (
        "browser",
        &[
            "x-scheme-handler/http",
            "x-scheme-handler/https",
            "text/html",
        ],
    ),
    ("files", &["inode/directory"]),
    ("terminal", &[]),
    (
        "video",
        &[
            "video/mp4",
            "video/x-matroska",
            "video/webm",
            "video/x-msvideo",
            "video/mpeg",
            "video/quicktime",
        ],
    ),
    (
        "music",
        &[
            "audio/mpeg",
            "audio/flac",
            "audio/ogg",
            "audio/x-wav",
            "audio/mp4",
            "audio/x-vorbis+ogg",
        ],
    ),
    (
        "image",
        &[
            "image/png",
            "image/jpeg",
            "image/gif",
            "image/webp",
            "image/bmp",
        ],
    ),
    ("pdf", &["application/pdf"]),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub packages: Vec<String>,
    pub exec: Vec<String>,
    pub desktop: String,
    pub icon: String,
    pub wm_class: String,
    pub size_kb: u64,
    pub summary: String,
    pub roles: Vec<String>,
    /// Shipped in the image: shown as built in, never uninstalled.
    pub system: bool,
}

/// Alpine package names: what the helper accepts too.
pub fn valid_package(p: &str) -> bool {
    let b = p.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b'+' | b'-')
        })
        && !p.ends_with(".apk")
}

fn str_list(v: &Value) -> Option<Vec<String>> {
    v.as_array()?
        .iter()
        .map(|x| x.as_str().map(str::to_string))
        .collect()
}

impl Entry {
    fn parse(v: &Value) -> Option<Entry> {
        let id = v["id"].as_str()?.to_string();
        if !valid_app_id(&id) || id.starts_with(['-', '_']) {
            return None;
        }
        let packages = str_list(&v["packages"])?;
        if packages.is_empty() || packages.len() > 8 || !packages.iter().all(|p| valid_package(p)) {
            return None;
        }
        let exec = str_list(&v["exec"]).unwrap_or_default();
        if exec.iter().any(|a| a.contains('\0')) {
            return None;
        }
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        Some(Entry {
            name: v["name"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or(&id)
                .to_string(),
            category: v["category"]
                .as_str()
                .filter(|n| !n.is_empty())
                .unwrap_or("Other")
                .to_string(),
            packages,
            exec,
            desktop: s("desktop"),
            icon: s("icon"),
            wm_class: s("wm_class"),
            size_kb: v["size_kb"].as_u64().unwrap_or(0),
            summary: s("summary"),
            roles: str_list(&v["roles"])
                .unwrap_or_default()
                .into_iter()
                .filter(|r| DEFAULT_ROLES.contains(&r.as_str()))
                .collect(),
            system: v["system"].as_bool().unwrap_or(false),
            id,
        })
    }

    /// The apps.d entry of this app (None for package-only entries such as fonts).
    pub fn app_json(&self) -> Option<Value> {
        if self.exec.is_empty() {
            return None;
        }
        Some(json!({
            "id": self.id,
            "name": self.name,
            "icon": if self.icon.is_empty() { "apps" } else { self.icon.as_str() },
            "exec": self.exec,
            "env": {},
            "single_instance": !self.wm_class.is_empty(),
            "wm_class": self.wm_class,
        }))
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalogue {
    pub entries: Vec<Entry>,
    pub reserve_kb: u64,
}

impl Catalogue {
    /// Entries that do not validate are dropped (a broken line never reaches apk).
    pub fn load(root: &Root) -> Catalogue {
        let v: Value = std::fs::read_to_string(root.p(CATALOGUE))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let mut entries: Vec<Entry> = Vec::new();
        for e in v["entries"].as_array().into_iter().flatten() {
            if let Some(e) = Entry::parse(e) {
                if !entries.iter().any(|x| x.id == e.id) {
                    entries.push(e);
                }
            }
        }
        Catalogue {
            entries,
            reserve_kb: v["reserve_kb"]
                .as_u64()
                .unwrap_or(RESERVE_KB)
                .max(RESERVE_KB),
        }
    }
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Names of the installed packages (`P:` lines of the apk database).
pub fn installed_packages(root: &Root) -> HashSet<String> {
    std::fs::read_to_string(root.p(APK_DB))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("P:"))
        .map(|p| p.trim().to_string())
        .collect()
}

/// Is every package of the entry installed? (System entries are part of the image.)
pub fn is_installed(e: &Entry, pk: &HashSet<String>) -> bool {
    e.system || e.packages.iter().all(|p| pk.contains(p))
}

/// The free-space rule: an install may not leave less than `reserve` free.
pub fn check_space(free_kb: u64, need_kb: u64, reserve_kb: u64) -> Result<(), String> {
    if free_kb.saturating_sub(need_kb) >= reserve_kb {
        Ok(())
    } else {
        Err(format!(
            "Not enough free space: {} needed, {} free, {} must stay free",
            fmt_kb(need_kb),
            fmt_kb(free_kb),
            fmt_kb(reserve_kb)
        ))
    }
}

fn fmt_kb(kb: u64) -> String {
    let mb = kb as f64 / 1024.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

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

    // ------------------------------------------------------------------ queries
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

    // ------------------------------------------------------------------ jobs
    /// Start an install or uninstall of a catalogue entry. Returns at once; the job runs as a child
    /// that `tick` polls.
    pub fn start(&mut self, op: Op, id: &str, now: Instant) -> Result<(), PkgError> {
        if self.job.is_some() {
            return Err(PkgError::Busy);
        }
        let cat = Catalogue::load(&self.root);
        let e = cat
            .get(id)
            .cloned()
            .ok_or_else(|| PkgError::Unknown(format!("{id:?} is not in the catalogue")))?;
        let pk = installed_packages(&self.root);
        let installed = is_installed(&e, &pk);
        let mut args: Vec<String> = Vec::new();
        match op {
            Op::Install => {
                if e.system {
                    return Err(PkgError::Refused(format!("{} is built in", e.name)));
                }
                if installed {
                    return Err(PkgError::Refused(format!(
                        "{} is already installed",
                        e.name
                    )));
                }
                let free = sysinfo::disk_usage(&self.root)
                    .map(|(_, f)| f / 1024)
                    .ok_or_else(|| PkgError::Refused("Cannot read the free space".into()))?;
                check_space(free, e.size_kb, self.reserve(&cat)).map_err(PkgError::Refused)?;
                args.extend(["install".into(), "--need-kb".into(), e.size_kb.to_string()]);
            }
            Op::Remove => {
                if e.system {
                    return Err(PkgError::Refused(format!(
                        "{} is a system app and cannot be uninstalled",
                        e.name
                    )));
                }
                if !e.packages.iter().any(|p| pk.contains(p)) {
                    return Err(PkgError::Refused(format!("{} is not installed", e.name)));
                }
                args.push("remove".into());
            }
        }
        args.push(e.id.clone());
        args.extend(e.packages.iter().cloned());

        if let Some(d) = self.out.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let out = std::fs::File::create(&self.out)
            .map_err(|er| PkgError::Failed(format!("output file: {er}")))?;
        let mut cmd = Command::new(&self.script);
        cmd.args(&args)
            .env_clear()
            .env("PATH", PATH)
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(Stdio::null());
        if self.root.base() != Path::new("/") {
            cmd.env("WALPAD_PKG_ROOT", self.root.base());
            cmd.env("WALPAD_PKG_APK", self.root.p("/sbin/apk"));
            cmd.env("WALPAD_PKG_RESERVE_KB", self.reserve(&cat).to_string());
        }
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let child = cmd.spawn().map_err(|er| match er.kind() {
            std::io::ErrorKind::NotFound => PkgError::Failed("walpad-pkg is not installed".into()),
            _ => PkgError::Failed(format!("cannot run walpad-pkg: {er}")),
        })?;
        let timeout = self.timeouts[op as usize];
        self.job = Some(Job {
            op,
            id: e.id.clone(),
            child,
            deadline: now + timeout,
            off: 0,
            partial: String::new(),
            percent: 0,
            text: match op {
                Op::Install => "Starting the installation".into(),
                Op::Remove => "Starting the removal".into(),
            },
            ok: false,
            error: None,
        });
        self.progress_notice();
        Ok(())
    }

    fn progress_notice(&mut self) {
        if let Some(j) = &self.job {
            self.notices.push(json!({
                "kind": "pkg_progress", "op": j.op.name(), "id": j.id,
                "percent": j.percent, "text": j.text,
            }));
        }
    }

    /// Read what the helper printed since the last poll; every progress change becomes a notice.
    fn read_output(j: &mut Job, path: &Path, notices: &mut Vec<Value>) {
        let Ok(mut f) = std::fs::File::open(path) else {
            return;
        };
        if f.seek(SeekFrom::Start(j.off)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.take(64 * 1024).read_to_end(&mut buf).is_err() || buf.is_empty() {
            return;
        }
        j.off += buf.len() as u64;
        j.partial.push_str(&String::from_utf8_lossy(&buf));
        while let Some(i) = j.partial.find('\n') {
            let line: String = j.partial.drain(..=i).collect();
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("PROGRESS ") {
                let (p, text) = rest.split_once(' ').unwrap_or((rest, ""));
                if let Ok(p) = p.parse::<u8>() {
                    let p = p.min(100).max(j.percent);
                    let text = clean(text);
                    if p != j.percent || (!text.is_empty() && text != j.text) {
                        j.percent = p;
                        if !text.is_empty() {
                            j.text = text;
                        }
                        notices.push(json!({
                            "kind": "pkg_progress", "op": j.op.name(), "id": j.id,
                            "percent": j.percent, "text": j.text,
                        }));
                    }
                }
            } else if line == "OK" {
                j.ok = true;
            } else if let Some(rest) = line.strip_prefix("ERROR ") {
                let msg = rest.split_once(' ').map_or(rest, |(_, m)| m);
                j.error = Some(clean(msg));
            }
        }
    }

    /// Poll the running job. Returns true when a job finished (the apps list or the installed
    /// state changed: the caller reloads its app definitions).
    pub fn tick(&mut self, now: Instant) -> bool {
        let Some(mut j) = self.job.take() else {
            return false;
        };
        Self::read_output(&mut j, &self.out, &mut self.notices);
        let status = match j.child.try_wait() {
            Ok(Some(st)) => Some(st.success()),
            Ok(None) if now >= j.deadline => {
                if let Some(pid) = rustix::process::Pid::from_raw(j.child.id() as i32) {
                    let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
                }
                let _ = j.child.kill();
                let _ = j.child.wait();
                j.error = Some(format!(
                    "Timed out after {} minutes",
                    self.timeouts[j.op as usize].as_secs() / 60
                ));
                Some(false)
            }
            Ok(None) => None,
            Err(e) => {
                j.error = Some(format!("cannot wait for walpad-pkg: {e}"));
                Some(false)
            }
        };
        let Some(exited_ok) = status else {
            self.job = Some(j);
            return false;
        };
        // the last lines may still be unread
        Self::read_output(&mut j, &self.out, &mut self.notices);
        let ok = exited_ok && j.ok;
        let error = if ok {
            None
        } else {
            Some(
                j.error
                    .clone()
                    .unwrap_or_else(|| "The package manager failed without a message".to_string()),
            )
        };
        if ok {
            self.finish_ok(j.op, &j.id);
        }
        self.notices.push(json!({
            "kind": "pkg_done", "op": j.op.name(), "id": j.id,
            "ok": ok, "error": error,
        }));
        true
    }

    /// apk succeeded: now (and only now) register or unregister the app and refresh the defaults.
    fn finish_ok(&mut self, op: Op, id: &str) {
        let cat = Catalogue::load(&self.root);
        let Some(e) = cat.get(id) else { return };
        match op {
            Op::Install => {
                let _ = self.register(e);
            }
            Op::Remove => {
                let _ = std::fs::remove_file(self.root.p(&format!("{APPS_D}/{id}.json")));
            }
        }
        let _ = self.apply_defaults();
    }

    fn register(&self, e: &Entry) -> std::io::Result<()> {
        let Some(j) = e.app_json() else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(&j).map_err(std::io::Error::other)?;
        write_atomic(&self.root.p(&format!("{APPS_D}/{}.json", e.id)), &bytes)
    }

    // ------------------------------------------------------------------ default apps
    /// `default_app {role, app}`: the app must be installed and have the role.
    pub fn set_default(&mut self, role: &str, app: &str) -> Result<(), PkgError> {
        if !DEFAULT_ROLES.contains(&role) {
            return Err(PkgError::Unknown(format!(
                "{role:?} is not a default app role"
            )));
        }
        let cat = Catalogue::load(&self.root);
        let e = cat
            .get(app)
            .ok_or_else(|| PkgError::Unknown(format!("{app:?} is not in the catalogue")))?;
        if !e.roles.iter().any(|r| r == role) {
            return Err(PkgError::Refused(format!(
                "{} cannot open {role} files",
                e.name
            )));
        }
        if !is_installed(e, &installed_packages(&self.root)) {
            return Err(PkgError::Refused(format!("{} is not installed", e.name)));
        }
        let mut d = Defaults::load(&self.root);
        d.roles.insert(role.to_string(), app.to_string());
        d.save(&self.root)
            .map_err(|er| PkgError::Failed(format!("cannot save the default apps: {er}")))?;
        self.apply_defaults()
            .map_err(|er| PkgError::Failed(format!("cannot write mimeapps.list: {er}")))
    }

    /// Regenerate mimeapps.list from the effective default of every role.
    pub fn apply_defaults(&self) -> std::io::Result<()> {
        let cat = Catalogue::load(&self.root);
        let pk = installed_packages(&self.root);
        let d = Defaults::load(&self.root);
        let mut text = String::from("[Default Applications]\n");
        for (role, mimes) in ROLE_MIMES {
            let Some(app) = effective(role, &cat, &pk, &d) else {
                continue;
            };
            let Some(desktop) = cat.get(&app).map(|e| e.desktop.clone()) else {
                continue;
            };
            if desktop.is_empty() {
                continue;
            }
            for m in *mimes {
                text.push_str(&format!("{m}={desktop};\n"));
            }
        }
        write_atomic(&self.root.p(MIMEAPPS), text.as_bytes())
    }
}

/// /etc/walpad/defaults.json: the app chosen per role.
#[derive(Debug, Default)]
pub struct Defaults {
    pub roles: BTreeMap<String, String>,
}

impl Defaults {
    pub fn load(root: &Root) -> Defaults {
        let v: Value = std::fs::read_to_string(root.p(DEFAULTS))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        let roles = v["roles"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(r, a)| {
                DEFAULT_ROLES.contains(&r.as_str()) && a.as_str().is_some_and(valid_app_id)
            })
            .filter_map(|(r, a)| Some((r.clone(), a.as_str()?.to_string())))
            .collect();
        Defaults { roles }
    }
    pub fn save(&self, root: &Root) -> std::io::Result<()> {
        let v = json!({"version": 1, "roles": self.roles});
        let bytes = serde_json::to_vec_pretty(&v).map_err(std::io::Error::other)?;
        write_atomic(&root.p(DEFAULTS), &bytes)
    }
}

/// The app that handles `role`: the chosen one while it is installed, else the first installed
/// catalogue entry with that role.
pub fn effective(
    role: &str,
    cat: &Catalogue,
    pk: &HashSet<String>,
    d: &Defaults,
) -> Option<String> {
    let has = |e: &Entry| e.roles.iter().any(|r| r == role) && is_installed(e, pk);
    if let Some(e) = d.roles.get(role).and_then(|a| cat.get(a)) {
        if has(e) {
            return Some(e.id.clone());
        }
    }
    cat.entries.iter().find(|e| has(e)).map(|e| e.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat_json() -> &'static str {
        r#"{"version":1,"reserve_kb":2097152,"entries":[
        {"id":"browser","name":"Browser","category":"Built in","packages":["firefox-esr"],"size_kb":224000,"exec":["firefox-esr"],"wm_class":"firefox-esr","desktop":"firefox-esr.desktop","roles":["browser"],"system":true},
        {"id":"mousepad","name":"Mousepad","category":"Documents","packages":["mousepad"],"size_kb":700,"exec":["mousepad"],"wm_class":"mousepad","desktop":"org.xfce.mousepad.desktop","icon":"apps"},
        {"id":"evince","name":"Evince","category":"Documents","packages":["evince"],"size_kb":15000,"exec":["evince"],"desktop":"org.gnome.Evince.desktop","roles":["pdf"]},
        {"id":"zathura","name":"Zathura","category":"Documents","packages":["zathura","zathura-pdf-mupdf"],"size_kb":1500,"exec":["zathura"],"desktop":"org.pwmt.zathura.desktop","roles":["pdf"]},
        {"id":"font-x","name":"Font","category":"Fonts","packages":["font-x"],"size_kb":100,"exec":[]},
        {"id":"evil","name":"Evil","category":"X","packages":["--allow-untrusted"],"size_kb":1,"exec":["x"]},
        {"id":"evil2","name":"Evil","category":"X","packages":["/tmp/x.apk"],"size_kb":1,"exec":["x"]},
        {"id":"Bad Id","packages":["x"]},
        {"id":"mousepad","name":"dup","packages":["other"]}
        ]}"#
    }

    fn root(tag: &str) -> Root {
        let d = std::env::temp_dir().join(format!("walpad-pkg-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("etc/walpad")).unwrap();
        std::fs::create_dir_all(d.join("lib/apk/db")).unwrap();
        std::fs::write(d.join("etc/walpad/catalogue.json"), cat_json()).unwrap();
        Root::new(d)
    }
    fn db(r: &Root, pk: &[&str]) {
        let t: String = pk.iter().map(|p| format!("C:x\nP:{p}\nV:1\n\n")).collect();
        std::fs::write(r.p(APK_DB), t).unwrap();
    }

    #[test]
    fn catalogue_drops_entries_that_could_reach_apk_wrongly() {
        let r = root("load");
        let c = Catalogue::load(&r);
        let ids: Vec<_> = c.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["browser", "mousepad", "evince", "zathura", "font-x"]);
        assert_eq!(
            c.get("mousepad").unwrap().size_kb,
            700,
            "first duplicate wins"
        );
        assert_eq!(c.reserve_kb, RESERVE_KB);
        for bad in [
            "",
            "-x",
            "--allow-untrusted",
            "a b",
            "/tmp/x.apk",
            "x.apk",
            "A",
            "a;b",
            "a/b",
        ] {
            assert!(!valid_package(bad), "{bad:?}");
        }
        assert!(valid_package("libreoffice-writer") && valid_package("gtk+3.0"));
        // the shipped catalogue is entirely valid
        let shipped = Root::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rootfs"));
        let n = std::fs::read_to_string(shipped.p(CATALOGUE))
            .unwrap()
            .matches("\"id\":")
            .count();
        assert_eq!(Catalogue::load(&shipped).entries.len(), n);
        assert!(n >= 25);
    }

    #[test]
    fn installed_state_comes_from_the_apk_database() {
        let r = root("db");
        let c = Catalogue::load(&r);
        assert!(installed_packages(&r).is_empty());
        db(&r, &["mousepad", "zathura", "busybox"]);
        let pk = installed_packages(&r);
        assert!(is_installed(c.get("mousepad").unwrap(), &pk));
        assert!(
            !is_installed(c.get("zathura").unwrap(), &pk),
            "needs both packages"
        );
        assert!(is_installed(c.get("browser").unwrap(), &pk), "built in");
        assert!(!is_installed(c.get("evince").unwrap(), &pk));
    }

    #[test]
    fn free_space_rule() {
        const GIB: u64 = 1024 * 1024;
        // the scenario: needs 3 GiB with 4 GiB free leaves 1 GiB: refused
        let e = check_space(4 * GIB, 3 * GIB, 2 * GIB).unwrap_err();
        assert!(
            e.contains("Not enough free space") && e.contains("3.0 GB") && e.contains("4.0 GB"),
            "{e}"
        );
        assert!(
            check_space(5 * GIB, 3 * GIB, 2 * GIB).is_ok(),
            "exactly the reserve is fine"
        );
        assert!(
            check_space(3 * GIB, 4 * GIB, 2 * GIB).is_err(),
            "no underflow"
        );
        assert!(check_space(10 * GIB, 0, 2 * GIB).is_ok());
    }

    #[test]
    fn app_entry_is_generated_from_the_catalogue() {
        let c = Catalogue::load(&root("json"));
        let j = c.get("mousepad").unwrap().app_json().unwrap();
        assert_eq!(
            j,
            json!({"id":"mousepad","name":"Mousepad","icon":"apps","exec":["mousepad"],"env":{},"single_instance":true,"wm_class":"mousepad"})
        );
        assert!(
            c.get("font-x").unwrap().app_json().is_none(),
            "packages only: no icon"
        );
    }

    #[test]
    fn effective_default_falls_back_to_an_installed_candidate() {
        let r = root("eff");
        let c = Catalogue::load(&r);
        let mut d = Defaults::default();
        let none = HashSet::new();
        assert_eq!(effective("pdf", &c, &none, &d), None);
        assert_eq!(
            effective("browser", &c, &none, &d).as_deref(),
            Some("browser")
        );
        let pk: HashSet<String> = ["evince".to_string()].into();
        assert_eq!(effective("pdf", &c, &pk, &d).as_deref(), Some("evince"));
        d.roles.insert("pdf".into(), "zathura".into());
        assert_eq!(
            effective("pdf", &c, &pk, &d).as_deref(),
            Some("evince"),
            "the chosen app is not installed: fall back"
        );
        let both: HashSet<String> = ["evince", "zathura", "zathura-pdf-mupdf"]
            .map(String::from)
            .into();
        assert_eq!(effective("pdf", &c, &both, &d).as_deref(), Some("zathura"));
    }

    #[test]
    fn default_app_writes_defaults_and_mimeapps() {
        let r = root("defaults");
        db(&r, &["evince", "zathura", "zathura-pdf-mupdf"]);
        let mut p = Pkg::new(&r);
        p.set_default("pdf", "zathura").unwrap();
        assert_eq!(Defaults::load(&r).roles["pdf"], "zathura");
        let m = std::fs::read_to_string(r.p(MIMEAPPS)).unwrap();
        assert!(m.starts_with("[Default Applications]\n"));
        assert!(
            m.contains("application/pdf=org.pwmt.zathura.desktop;\n"),
            "{m}"
        );
        assert!(
            m.contains("x-scheme-handler/https=firefox-esr.desktop;"),
            "{m}"
        );
        p.set_default("pdf", "evince").unwrap();
        assert!(std::fs::read_to_string(r.p(MIMEAPPS))
            .unwrap()
            .contains("application/pdf=org.gnome.Evince.desktop;\n"));
        // refusals
        assert!(matches!(
            p.set_default("shell", "evince"),
            Err(PkgError::Unknown(_))
        ));
        assert!(matches!(
            p.set_default("pdf", "nope"),
            Err(PkgError::Unknown(_))
        ));
        assert!(
            matches!(p.set_default("pdf", "mousepad"), Err(PkgError::Refused(_))),
            "no such role"
        );
        db(&r, &["evince"]);
        assert!(
            matches!(p.set_default("pdf", "zathura"), Err(PkgError::Refused(_))),
            "not installed"
        );
    }

    #[test]
    fn catalogue_answer_lists_state_and_heals_a_missing_registration() {
        let r = root("answer");
        db(&r, &["mousepad"]);
        let p = Pkg::new(&r);
        let v = p.catalogue_json();
        let e = v["entries"].as_array().unwrap();
        let get = |id: &str| e.iter().find(|x| x["id"] == id).unwrap().clone();
        assert_eq!(get("mousepad")["installed"], true);
        assert_eq!(get("evince")["installed"], false);
        assert_eq!(get("browser")["system"], true);
        assert_eq!(v["busy"], Value::Null);
        assert_eq!(v["reserve_kb"], RESERVE_KB);
        assert_eq!(v["defaults"]["browser"], "browser");
        assert_eq!(v["defaults"]["pdf"], Value::Null);
        assert!(
            r.p("/var/lib/walpad/apps.d/mousepad.json").is_file(),
            "an installed catalogue app without an apps.d entry is registered again"
        );
        assert!(!r.p("/var/lib/walpad/apps.d/font-x.json").exists());
    }

    #[test]
    fn start_refuses_what_it_must_not_run() {
        let r = root("refuse");
        db(&r, &["mousepad"]);
        let mut p = Pkg::new(&r);
        let now = Instant::now();
        assert!(matches!(
            p.start(Op::Install, "nope", now),
            Err(PkgError::Unknown(_))
        ));
        assert!(
            matches!(
                p.start(Op::Install, "browser", now),
                Err(PkgError::Refused(_))
            ),
            "built in"
        );
        assert!(
            matches!(
                p.start(Op::Remove, "browser", now),
                Err(PkgError::Refused(_))
            ),
            "system app"
        );
        assert!(
            matches!(
                p.start(Op::Install, "mousepad", now),
                Err(PkgError::Refused(_))
            ),
            "already installed"
        );
        assert!(
            matches!(
                p.start(Op::Remove, "evince", now),
                Err(PkgError::Refused(_))
            ),
            "not installed"
        );
        // the 3 GiB with 4 GiB free case, through the reserve hook (the host's free space varies)
        let (_, free) = sysinfo::disk_usage(&r).unwrap();
        p.set_reserve_kb(free / 1024 + 1);
        let e = p.start(Op::Install, "evince", now);
        assert!(
            matches!(&e, Err(PkgError::Refused(m)) if m.contains("Not enough free space")),
            "{e:?}"
        );
        assert!(p.job.is_none(), "nothing was started");
    }
}
