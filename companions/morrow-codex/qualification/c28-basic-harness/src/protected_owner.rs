//! Original public product opening only; no copied Store, key provider or schema.
use anyhow::{Result, anyhow, ensure};
use morrow_workbench_host::Workbench;
use std::path::Path;

pub struct ProtectedOwner {
    pub workbench: Workbench,
    finish_attempted: bool,
    finished: bool,
}
impl ProtectedOwner {
    pub fn open(synthetic_root: &Path) -> Result<Self> {
        crate::preflight::plain_absolute(synthetic_root)?;
        crate::preflight::no_reparse_ancestors(synthetic_root)?;
        let directory = synthetic_root.join("protected-owner");
        // A prior session/database/key cannot be selected, imported or adopted.
        std::fs::create_dir(&directory)?;
        crate::preflight::no_reparse_ancestors(&directory)?;
        let database = directory.join("synthetic.sqlite");
        // The whole directory is new. Original Session owns key naming and
        // protection; the harness does not select or inspect a key file.
        ensure!(
            !database.try_exists()?,
            "fresh original protected state required"
        );
        let workbench = Workbench::open(&database, None)
            .map_err(|e| anyhow!("original protected Workbench open: {e}"))?;
        // Return the same owner even if original maintenance requires recovery.
        // The caller stores it before consulting the original warning/gates.
        Ok(Self {
            workbench,
            finish_attempted: false,
            finished: false,
        })
    }
    pub fn finished(&self) -> bool {
        self.finished
    }
    pub fn require_idle(&mut self) -> Result<()> {
        let task = self
            .workbench
            .agent_status()
            .map_err(|e| anyhow!("original agent status: {e}"))?;
        ensure!(
            task.is_none(),
            "unacknowledged/unfinished original agent remains; retain its owner"
        );
        let scheduler = self.workbench.agent_scheduler_status();
        ensure!(
            scheduler.active_tokens == 0
                && scheduler.retained_debts == 0
                && scheduler.uncertain_debts == 0,
            "actual native scheduler/debt remains; cannot manufacture clean ownership"
        );
        Ok(())
    }
    pub fn status(&mut self) -> Result<serde_json::Value> {
        let task = self
            .workbench
            .agent_status()
            .map_err(|e| anyhow!("original owner status: {e}"))?;
        let scheduler = self.workbench.agent_scheduler_status();
        Ok(
            serde_json::json!({"agent":format!("{task:?}"), "schedulers":format!("{scheduler:?}"),
            "maintenance_warning":self.workbench.maintenance_warning(), "finished":self.finished}),
        )
    }
    pub fn finish(&mut self) -> Result<()> {
        ensure!(
            !self.finish_attempted,
            "original finish already attempted; no blind maintenance retry"
        );
        self.require_idle()?;
        self.finish_attempted = true;
        self.workbench
            .finish()
            .map_err(|e| anyhow!("original protected flush/finish unresolved: {e}"))?;
        ensure!(
            self.workbench.maintenance_warning().is_none(),
            "original protected maintenance is not clean"
        );
        self.finished = true;
        Ok(())
    }
}
