//! Create-only public/synthetic evidence; never key material or a serialized authority.
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

// One documented witness needs 42 attempts plus 3 final owner/system steps.
// Four explicit same-owner cleanups plus idle factory release/reap add at most
// nine; a failed business attempt remains charged. 64 bounds the complete
// supported single-witness chain with headroom, without changing record256.
const MAX_ATTEMPTED_STEPS: usize = 64;

pub struct Journal {
    directory: PathBuf,
    file: File,
    attempted: BTreeSet<String>,
    sequence: u32,
}
impl Journal {
    pub fn create(synthetic_root: &Path) -> Result<Self> {
        crate::preflight::plain_absolute(synthetic_root)?;
        let parent = synthetic_root
            .parent()
            .ok_or_else(|| anyhow::anyhow!("synthetic parent missing"))?;
        crate::preflight::no_reparse_ancestors(parent)?;
        let stem = synthetic_root
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("synthetic name"))?;
        let directory = parent.join(format!("{stem}.evidence"));
        // No existing directory or prior log may be adopted as runtime authority.
        std::fs::create_dir(&directory)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("journal.jsonl"))?;
        let mut result = Self {
            directory,
            file,
            attempted: BTreeSet::new(),
            sequence: 0,
        };
        result.record("acceptance", &serde_json::json!({"VM_run":"PENDING_ACTUAL_STEPS", "SDK26_G04":"OPEN", "release_eligible":false}))?;
        Ok(result)
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    pub fn record(&mut self, kind: &str, payload: &impl Serialize) -> Result<()> {
        ensure!(
            self.sequence < 256,
            "evidence journal budget exhausted; keep original owner, no new effects"
        );
        let raw = serde_json::to_vec(
            &serde_json::json!({"sequence":self.sequence, "kind":kind, "payload":payload}),
        )?;
        ensure!(raw.len() <= 128 * 1024, "public evidence record size");
        self.sequence += 1;
        self.file.write_all(&raw)?;
        self.file.write_all(b"\n")?;
        self.file.sync_all()?;
        Ok(())
    }
    pub fn before(&mut self, step: &str, payload: &impl Serialize) -> Result<()> {
        ensure!(
            self.attempted.len() < MAX_ATTEMPTED_STEPS && self.attempted.insert(step.into()),
            "step already attempted; no replay"
        );
        self.record(
            "ATTEMPT_RESERVED_OUTCOME_UNKNOWN",
            &serde_json::json!({"step":step,"review":payload}),
        )
    }
    pub fn raw(&self, name: &str, bytes: &[u8]) -> Result<()> {
        ensure!(
            name.len() <= 64
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "evidence name"
        );
        ensure!(bytes.len() <= 128 * 1024, "raw evidence limit");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.directory.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
