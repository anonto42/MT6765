//! System operations for `Daemon`: battery SoC, charger policy, halt, process helpers, settings.

use crate::logic::{battery, soc};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use walpad_hw::battery::read_battery;
use walpad_proto::ErrorCode;

use super::errors::{internal, read_file, sync_gtk_theme, wall_ms, write_atomic, CmdResult};
use super::{Daemon, BATTERY_EVERY};

impl Daemon {
    pub(super) fn spawn_simple(&self, exe: &str, args: &[&str]) {
        let r = std::process::Command::new(exe)
            .args(args)
            .env("DISPLAY", ":0")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match r {
            Ok(child) => self.helpers.borrow_mut().push(child),
            Err(e) => eprintln!("walpad-core: cannot start {exe}: {e}"),
        }
    }

    /// Step the battery estimator and save the shown percent (at most every 5 min when it changed).
    pub(super) fn soc_tick(&mut self, now: Instant) {
        self.next_soc = now + BATTERY_EVERY;
        let raw = read_battery(&self.root);
        let inp = battery::soc_input(&raw, &self.brain.batt_cfg);
        let ms = now.duration_since(self.started).as_millis() as u64;
        self.soc_out = self.soc.step(ms, &inp);
        self.apply_charger_policy(&raw);
        if let Some(p) = self.soc_out.percent.filter(|_| inp.mv.is_some()) {
            let due = self.soc_saved.is_none_or(|(t, last)| {
                last != p && now.duration_since(t) >= Duration::from_secs(300)
            });
            if due {
                self.soc_saved = Some((now, p));
                self.soc_write();
            }
        }
    }

    fn apply_charger_policy(&mut self, raw: &walpad_hw::battery::RawBattery) {
        let b = battery::model(raw, self.brain.guard.low(), Some(&self.soc_out));
        if let Some(target) = self
            .brain
            .guard
            .charger_step(raw, b.percent, &self.brain.batt_cfg)
        {
            let _ = walpad_hw::battery::set_charger_cmd(&self.root, false, target.disable_charging);
            let _ =
                walpad_hw::battery::set_charger_input_limit_ua(&self.root, target.input_limit_ua);
        }
    }

    fn soc_write(&self) {
        if let Some(p) = self.soc.shown() {
            let path = self.root.p(soc::PERSIST_PATH);
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = write_atomic(&path, soc::to_json(p, wall_ms() / 1000).as_bytes());
        }
    }

    pub(super) fn halt(&self, cmd: &str) {
        self.soc_write();
        if !self.real_root() {
            eprintln!("walpad-core: (test root) would run `{cmd} -f`");
            return;
        }
        rustix::fs::sync();
        self.spawn_simple(cmd, &["-f"]);
    }

    fn pids_by_comm(&self, prefix: &str) -> Vec<i32> {
        let Ok(rd) = std::fs::read_dir(self.root.p("/proc")) else {
            return Vec::new();
        };
        rd.flatten()
            .filter_map(|e| {
                let pid: i32 = e.file_name().to_str()?.parse().ok()?;
                let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
                // a zombie (state Z, after the comm's closing parenthesis) is not running: its
                // parent has not reaped it yet (orphans of an earlier core land on the ramdisk's
                // PID 1, which never reaps)
                let stat = std::fs::read_to_string(e.path().join("stat")).unwrap_or_default();
                let state = stat.rsplit_once(") ").and_then(|(_, r)| r.chars().next());
                (comm.trim().starts_with(prefix) && state != Some('Z')).then_some(pid)
            })
            .collect()
    }

    pub(super) fn terminate(&self, prefix: &str) -> usize {
        let mut n = 0;
        for pid in self.pids_by_comm(prefix) {
            if let Some(p) = rustix::process::Pid::from_raw(pid) {
                if rustix::process::kill_process(p, rustix::process::Signal::TERM).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    pub(super) fn screenshot(&self) -> CmdResult {
        let src = self.root.p("/run/walpad/xfb/Xvfb_screen0");
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let virt = format!("/root/screenshots/shot-{t}.xwd");
        let dst = self.root.p(&virt);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(internal)?;
        }
        std::fs::copy(src, &dst).map_err(|e| (ErrorCode::Internal, format!("screenshot: {e}")))?;
        Ok(json!({"path": virt}))
    }

    pub(super) fn reload_cfg(&mut self) {
        self.brain.reload(
            &read_file(&self.root, "/etc/walpad/devd.json"),
            &read_file(&self.root, "/etc/walpad/keys.json"),
            &read_file(&self.root, "/etc/walpad/touch.json"),
        );
        let raw = read_battery(&self.root);
        self.apply_charger_policy(&raw);
    }

    pub(super) fn open_map(&self, lat: Option<f64>, lon: Option<f64>) -> CmdResult {
        if lat.is_some() != lon.is_some()
            || lat.is_some_and(|v| !v.is_finite() || !(-90.0..=90.0).contains(&v))
            || lon.is_some_and(|v| !v.is_finite() || !(-180.0..=180.0).contains(&v))
        {
            return Err((
                ErrorCode::InvalidArgs,
                "lat must be -90..90 and lon -180..180".into(),
            ));
        }
        let url = match (lat, lon) {
            (Some(lat), Some(lon)) => format!(
                "https://www.openstreetmap.org/?mlat={lat:.5}&mlon={lon:.5}#map=17/{lat:.5}/{lon:.5}"
            ),
            _ => "https://www.openstreetmap.org/".to_string(),
        };
        self.spawn_simple("firefox-esr", &[&url]);
        Ok(json!({"url": url}))
    }

    pub(super) fn set_time(&self, datetime: &str) -> CmdResult {
        if !walpad_proto::valid_datetime(datetime) {
            return Err((
                ErrorCode::InvalidArgs,
                "datetime must be YYYY-MM-DD HH:MM or YYYY-MM-DD HH:MM:SS".into(),
            ));
        }
        let out = std::process::Command::new("date")
            .args(["-s", datetime])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| (ErrorCode::Internal, format!("date: {e}")))?;
        if !out.status.success() {
            let msg = String::from_utf8_lossy(&out.stderr);
            return Err((
                ErrorCode::Internal,
                format!("date -s failed: {}", msg.trim()),
            ));
        }
        let _ = std::process::Command::new("hwclock")
            .args(["-w", "-u"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        Ok(json!({"datetime": datetime}))
    }

    pub(super) fn set_setting(&mut self, key: &str, value: Value) -> CmdResult {
        let virt = if matches!(
            key,
            "clock_24h"
                | "theme"
                | "wallpaper"
                | "wallpaper_path"
                | "pinned_apps"
                | "windows_mode"
                | "split_ratio"
                | "split_swap"
        ) {
            "/etc/walpad/shell.json"
        } else if matches!(key, "touch_gestures" | "multitouch") {
            "/etc/walpad/touch.json"
        } else if key.starts_with("key.") {
            "/etc/walpad/keys.json"
        } else {
            "/etc/walpad/devd.json"
        };
        let path = self.root.p(virt);
        if let Some(d) = path.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let mut obj: serde_json::Map<String, Value> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if key == "touch_gestures" {
            let mut g = obj
                .remove("gestures")
                .and_then(|v| match v {
                    Value::Object(m) => Some(m),
                    _ => None,
                })
                .unwrap_or_default();
            g.insert("enabled".to_string(), value.clone());
            obj.insert("gestures".to_string(), Value::Object(g));
        } else if let Some(rest) = key.strip_prefix("key.") {
            if let Some((btn, gesture)) = rest.split_once('.') {
                let mut b = obj
                    .remove(btn)
                    .and_then(|v| match v {
                        Value::Object(m) => Some(m),
                        _ => None,
                    })
                    .unwrap_or_default();
                b.insert(gesture.to_string(), value.clone());
                obj.insert(btn.to_string(), Value::Object(b));
            }
        } else {
            obj.insert(key.to_string(), value.clone());
        }
        let bytes = serde_json::to_vec_pretty(&obj).map_err(internal)?;
        write_atomic(&path, &bytes)
            .map_err(|e| (ErrorCode::Internal, format!("cannot write {virt}: {e}")))?;
        if key == "theme" {
            let light = value.as_str() == Some("light");
            sync_gtk_theme(&self.root, light);
        }
        if key == "net_hostname" {
            if let Some(hn) = value.as_str().filter(|s| !s.is_empty()) {
                let line = format!("{hn}\n");
                let _ = write_atomic(&self.root.p("/etc/hostname"), line.as_bytes());
                let _ = std::fs::write(self.root.p("/proc/sys/kernel/hostname"), line.as_bytes());
            }
        }
        if key.starts_with("net_") {
            let run_dir = self.root.p("/run/walpad");
            let _ = std::fs::create_dir_all(&run_dir);
            let _ = std::fs::write(run_dir.join("wifi-reapply"), b"1\n");
        }
        self.reload_cfg();
        Ok(json!({"key": key, "value": value}))
    }
}
