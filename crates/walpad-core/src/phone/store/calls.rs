use super::{read_json, write_json, MAX_CALLS};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRec {
    pub number: String,
    pub ts: u64,
    pub dur_s: u64,
    /// `in`, `out` or `missed`.
    pub kind: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct CallsFile {
    calls: Vec<CallRec>,
}

pub struct CallLog {
    path: PathBuf,
    f: CallsFile,
}

impl CallLog {
    pub fn load(path: PathBuf) -> CallLog {
        CallLog {
            f: read_json(&path),
            path,
        }
    }
    pub fn all(&self) -> &[CallRec] {
        &self.f.calls
    }
    pub fn add(&mut self, r: CallRec) {
        self.f.calls.push(r);
        if self.f.calls.len() > MAX_CALLS {
            let drop = self.f.calls.len() - MAX_CALLS;
            self.f.calls.drain(..drop);
        }
        write_json(&self.path, &self.f);
    }
    pub fn clear(&mut self) {
        self.f.calls.clear();
        write_json(&self.path, &self.f);
    }
}
