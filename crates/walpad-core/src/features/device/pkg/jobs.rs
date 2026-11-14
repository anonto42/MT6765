use super::catalogue::{
    check_space, installed_packages, is_installed, write_atomic, Catalogue, Entry, PATH,
};
use super::{clean, Job, Op, Pkg, PkgError};
use serde_json::{json, Value};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;
use walpad_hw::sysinfo;
use walpad_proto::appdefs::APPS_D;

impl Pkg {
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

    pub(super) fn register(&self, e: &Entry) -> std::io::Result<()> {
        let Some(j) = e.app_json() else {
            return Ok(());
        };
        let bytes = serde_json::to_vec_pretty(&j).map_err(std::io::Error::other)?;
        write_atomic(&self.root.p(&format!("{APPS_D}/{}.json", e.id)), &bytes)
    }
}
