use super::catalogue::{
    installed_packages, is_installed, write_atomic, Catalogue, Entry, DEFAULTS, MIMEAPPS,
    ROLE_MIMES,
};
use super::{Pkg, PkgError};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use walpad_hw::Root;
use walpad_proto::{valid_app_id, DEFAULT_ROLES};

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

impl Pkg {
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
