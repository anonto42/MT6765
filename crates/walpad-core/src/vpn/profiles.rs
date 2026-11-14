//! VPN profile store management: listing, importing, removing, and scanning candidates.

use super::config::{
    kind_ext, kind_of_name, name_from_path, read_candidate, render_settings, validate_openvpn,
    validate_wireguard,
};
use super::{Vpn, VpnError};
use crate::hotspot::write_600;
use walpad_proto::{valid_vpn_name, VpnKind, VpnProfile, VpnState};

impl Vpn {
    pub(crate) fn list_profiles(&self) -> Vec<VpnProfile> {
        let mut v: Vec<VpnProfile> = std::fs::read_dir(&self.store)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let f = e.file_name().to_string_lossy().into_owned();
                let kind = kind_of_name(&f)?;
                let name = f.rsplit_once('.')?.0.to_string();
                valid_vpn_name(&name).then_some(VpnProfile { name, kind })
            })
            .collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    /// `vpn_import`: validate and store a profile (0600 file in the 0700 store).
    pub fn import(&mut self, path: &str, name: Option<&str>) -> Result<VpnProfile, VpnError> {
        let kind =
            kind_of_name(path).ok_or_else(|| VpnError::Invalid("not a profile file".into()))?;
        let name = name.map_or_else(|| name_from_path(path), str::to_string);
        if !valid_vpn_name(&name) {
            return Err(VpnError::Invalid("invalid profile name".into()));
        }
        let text = read_candidate(&self.root.p(path)).map_err(VpnError::Invalid)?;
        match kind {
            VpnKind::Wireguard => validate_wireguard(&text),
            VpnKind::Openvpn => validate_openvpn(&text),
        }
        .map_err(VpnError::Invalid)?;
        if self.st.profiles.iter().any(|p| p.name == name) {
            return Err(VpnError::Invalid(
                "a profile with that name exists: remove it first".into(),
            ));
        }
        self.ensure_store()
            .map_err(|e| VpnError::Failed(format!("cannot create the profile store: {e}")))?;
        let dest = self.store.join(format!("{name}.{}", kind_ext(kind)));
        write_600(&dest, text.as_bytes())
            .map_err(|e| VpnError::Failed(format!("cannot store the profile: {e}")))?;
        self.load();
        // the first profile becomes the selected one
        if self.st.profile.is_none() {
            let mut s = self.settings();
            s.profile = Some(name.clone());
            let _ = write_600(&self.conf, render_settings(&s).as_bytes());
            self.load();
        }
        Ok(VpnProfile { name, kind })
    }

    fn ensure_store(&self) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(&self.store)?;
        std::fs::set_permissions(&self.store, std::fs::Permissions::from_mode(0o700))
    }

    /// `vpn_remove`: not the running profile.
    pub fn remove(&mut self, name: &str) -> Result<(), VpnError> {
        if self.st.state != VpnState::Off && self.st.profile.as_deref() == Some(name) {
            return Err(VpnError::Invalid("turn the VPN off first".into()));
        }
        let p = self
            .st
            .profiles
            .iter()
            .find(|p| p.name == name)
            .cloned()
            .ok_or_else(|| VpnError::Invalid("no such VPN profile".into()))?;
        std::fs::remove_file(self.store.join(format!("{name}.{}", kind_ext(p.kind))))
            .map_err(|e| VpnError::Failed(format!("cannot remove the profile: {e}")))?;
        if self.st.profile.as_deref() == Some(name) {
            let mut s = self.settings();
            s.profile = None;
            let _ = write_600(&self.conf, render_settings(&s).as_bytes());
        }
        self.load();
        Ok(())
    }

    /// Importable profile files in the user folders: (name, path, kind). Only files that pass
    /// the import validation are listed.
    pub fn candidates(&self) -> Vec<(String, String, VpnKind)> {
        let mut dirs: Vec<String> = vec![
            "/root/Downloads".into(),
            "/root/Documents".into(),
            "/var/lib/walpad/import".into(),
        ];
        for base in ["/media", "/mnt"] {
            for e in std::fs::read_dir(self.root.p(base))
                .into_iter()
                .flatten()
                .flatten()
            {
                dirs.push(format!("{base}/{}", e.file_name().to_string_lossy()));
            }
        }
        let mut out = Vec::new();
        for d in dirs {
            for e in std::fs::read_dir(self.root.p(&d))
                .into_iter()
                .flatten()
                .flatten()
            {
                let f = e.file_name().to_string_lossy().into_owned();
                let Some(kind) = kind_of_name(&f) else {
                    continue;
                };
                let path = format!("{d}/{f}");
                if !walpad_proto::valid_vpn_import_path(&path) {
                    continue;
                }
                let Ok(text) = read_candidate(&self.root.p(&path)) else {
                    continue;
                };
                let ok = match kind {
                    VpnKind::Wireguard => validate_wireguard(&text),
                    VpnKind::Openvpn => validate_openvpn(&text),
                };
                if ok.is_ok() {
                    out.push((name_from_path(&path), path, kind));
                }
                if out.len() >= 30 {
                    return out;
                }
            }
        }
        out
    }
}
