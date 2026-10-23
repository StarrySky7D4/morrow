//! Original public product opening only; no copied Store, key provider or schema.
use anyhow::{Result, anyhow, ensure};
use morrow_workbench_host::Workbench;
use std::path::Path;
use std::sync::Arc;

/// The lifecycle creates this identity before an attempt can acquire a factory.
/// A common HostBinding alone cannot distinguish two attempts of the same owner.
pub(crate) struct OwnerAttemptScope { original: Arc<()>, shared_parent: bool }
impl OwnerAttemptScope {
    pub(crate) fn for_lifecycle(original: Arc<()>, shared_parent: bool) -> Self { Self { original, shared_parent } }
    pub(crate) fn is_for(&self, original: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.original, original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_protected_finish_receipt_rejects_foreign_lifecycle_and_repeat() {
        let suffix=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root=std::env::temp_dir().join(format!("morrow-owner-scope-{}-{suffix}",std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let first=Arc::new(());let foreign=Arc::new(());
        // A failed check retains this original owner instead of implicitly
        // dropping it on unwind. The successful path explicitly consumes it.
        let held=Box::leak(Box::new(None));
        *held=Some(ProtectedOwner::open(&root).unwrap());
        let owner=held.as_mut().unwrap();
        owner.bind_first_attempt(OwnerAttemptScope::for_lifecycle(first.clone(), false)).unwrap();
        assert!(owner.bind_first_attempt(OwnerAttemptScope::for_lifecycle(foreign.clone(), false)).is_err());
        assert!(owner.take_finished_receipt().is_err());
        let original_binding=owner.workbench.agent_owner_binding();
        let original_address=owner.workbench.as_ref() as *const Workbench as usize;
        owner.finish().unwrap();
        let receipt=owner.take_finished_receipt().unwrap();
        assert!(receipt.binding()==original_binding && receipt.is_for(&first));
        assert!(!receipt.is_for(&foreign));
        assert!(owner.take_finished_receipt().is_err());
        assert!(owner.finish().is_err());
        assert_eq!(owner.workbench.as_ref() as *const Workbench as usize,original_address);
        assert!(owner.history.is_empty() && owner.completed.is_empty());
        owner.require_idle().unwrap();
        drop(receipt);
        drop(held.take().unwrap());
        // Actual protected lease release permits a new read of this synthetic
        // store. It supplies no parent/native execution retirement capability.
        let mut reopened=Workbench::open(&root.join("protected-owner/synthetic.sqlite"),None).unwrap();
        reopened.finish().unwrap();
    }
}

#[derive(Default)]
struct AttemptState {
    scope: Option<OwnerAttemptScope>,
    finish_attempted: bool,
    finished: bool,
    receipt_taken: bool,
}

pub struct ProtectedOwner {
    // Moving an attempt between enclosing Sessions never moves this allocation.
    pub workbench: Box<Workbench>,
    attempt: AttemptState,
    history: Vec<AttemptState>,
    completed: Vec<crate::provisioning::CompletedParentAttempt>,
    original_resources: Option<Arc<morrow_codex_session_exec_windows_v1::BorrowedNativeResources>>,
    original_session_host: Option<Arc<morrow_agent_session_exec_v1_r2::authority::SessionExecHost>>,
    original_binding:morrow_core::dispatch::HostBinding,
    original_workbench_address: usize,
    common_finish_attempted: bool,
    common_finished: bool,
}
pub(crate) struct FinishedOwnerReceipt {
    binding:morrow_core::dispatch::HostBinding,
    scope: OwnerAttemptScope,
}
impl FinishedOwnerReceipt {
    pub(crate) fn whole_owner_finished(&self) -> bool { !self.scope.shared_parent }
    pub(crate) fn binding(&self)->morrow_core::dispatch::HostBinding {self.binding}
    pub(crate) fn is_for(&self, lifecycle: &Arc<()>) -> bool { self.scope.is_for(lifecycle) }
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
        let original_binding=workbench.agent_owner_binding();
        let workbench=Box::new(workbench);
        let original_workbench_address=workbench.as_ref() as *const Workbench as usize;
        Ok(Self {
            workbench,
            attempt: AttemptState::default(),
            history: Vec::new(), completed: Vec::new(),
            original_resources: None, original_session_host: None,
            original_binding,original_workbench_address,common_finish_attempted:false,common_finished:false,
        })
    }
    pub(crate) fn bind_first_attempt(&mut self, scope: OwnerAttemptScope) -> Result<()> {
        ensure!(self.attempt.scope.is_none() && !self.attempt.finish_attempted && self.history.is_empty(),
            "original owner attempt already bound or consumed");
        self.attempt.scope = Some(scope);
        Ok(())
    }
    pub(crate) fn retain_original_native_resources(&mut self,
        resources: Arc<morrow_codex_session_exec_windows_v1::BorrowedNativeResources>,
        host: Arc<morrow_agent_session_exec_v1_r2::authority::SessionExecHost>) -> Result<()> {
        ensure!(!self.attempt.finish_attempted && self.attempt.scope.is_some()
            && resources.owner_binding() == self.original_binding,
            "native resources must belong to the original live protected owner");
        match (&self.original_resources, &self.original_session_host) {
            (None, None) => {
                self.original_resources = Some(resources);
                self.original_session_host = Some(host);
            }
            (Some(original), Some(original_host)) => {
                ensure!(Arc::ptr_eq(original, &resources) && Arc::ptr_eq(original_host, &host),
                    "same owner cannot replace its original SDK registry or session host");
            }
            _ => anyhow::bail!("incomplete original SDK owner anchor"),
        }
        Ok(())
    }
    /// Only a sealed completed parent attempt can open a fresh scope. Previous
    /// state is retained verbatim; no finish flag or SDK registry is reset.
    pub(crate) fn begin_next_attempt(&mut self, completed: crate::provisioning::CompletedParentAttempt,
        scope: OwnerAttemptScope) -> Result<()> {
        ensure!(!self.common_finish_attempted && self.attempt.finished && self.attempt.receipt_taken,
            "original attempt finish receipt was not consumed");
        ensure!(self.attempt.scope.as_ref().is_some_and(|old| completed.is_for(old))
            && !completed.is_for(&scope), "foreign or repeated parent attempt completion");
        ensure!(scope.shared_parent && self.attempt.scope.as_ref().is_some_and(|old|old.shared_parent),
            "shared parent scope required on both original attempts");
        ensure!(self.history.len() < 1023 && self.completed.len() == self.history.len(),
            "bounded owner attempt history exhausted or inconsistent");
        ensure!(self.workbench.agent_owner_binding() == self.original_binding
            && self.workbench.maintenance_warning().is_none(), "original protected owner changed");
        self.require_idle()?;
        ensure!(self.original_resources.as_ref().ok_or_else(|| anyhow!("original SDK resources absent"))?.is_clean()?
            && self.original_session_host.is_some(), "original SDK resources still charged");
        self.completed.push(completed);
        let previous = std::mem::replace(&mut self.attempt,
            AttemptState { scope: Some(scope), ..Default::default() });
        self.history.push(previous);
        Ok(())
    }
    pub(crate) fn require_parent_history(&mut self, previous:usize)->Result<()> {
        ensure!(!self.common_finish_attempted && self.attempt.finished && self.attempt.receipt_taken
            && self.history.len()==previous && self.completed.len()==previous
            && self.history.iter().all(|attempt|attempt.finished && attempt.receipt_taken && attempt.scope.is_some()),
            "original owner attempt history incomplete or replaced");
        ensure!(self.workbench.as_ref() as *const Workbench as usize == self.original_workbench_address
            && self.workbench.agent_owner_binding()==self.original_binding
            && self.workbench.maintenance_warning().is_none(),"original protected Workbench changed");
        self.require_idle()?;
        ensure!(self.original_resources.as_ref().ok_or_else(||anyhow!("original SDK registry absent"))?.is_clean()?
            && self.original_session_host.is_some(),"original SDK charge or owner anchor remains unresolved");
        Ok(())
    }
    pub(crate) fn finish_common_lifetime(&mut self, attempts:usize)->Result<()> {
        ensure!(attempts>0,"empty common lifetime is not qualified");
        self.require_parent_history(attempts-1)?;
        self.common_finish_attempted=true;
        self.workbench.finish().map_err(|error|anyhow!("final original protected flush unresolved: {error}"))?;
        ensure!(self.workbench.maintenance_warning().is_none(),"final original protected maintenance unresolved");
        self.common_finished=true;
        Ok(())
    }
    pub fn finished(&self) -> bool {
        self.attempt.finished
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
            "maintenance_warning":self.workbench.maintenance_warning(),
            "finished":self.common_finished || (self.attempt.finished && self.attempt.scope.as_ref().is_some_and(|scope|!scope.shared_parent)),
            "attempt_finished":self.attempt.finished,
            "closed_attempt_history":self.history.len()}),
        )
    }
    pub fn finish(&mut self) -> Result<()> {
        ensure!(
            !self.attempt.finish_attempted,
            "original finish already attempted; no blind maintenance retry"
        );
        self.require_idle()?;
        ensure!(self.workbench.agent_owner_binding()==self.original_binding,"original protected owner instance changed");
        self.attempt.finish_attempted = true;
        let shared_parent=self.attempt.scope.as_ref().is_some_and(|scope|scope.shared_parent);
        if shared_parent {
            let resources=self.original_resources.as_ref().ok_or_else(||anyhow!("original SDK registry absent"))?;
            self.workbench.finish_agent_attempt(resources)
                .map_err(|e|anyhow!("original protected attempt flush unresolved: {e}"))?;
        } else {
            self.workbench.finish().map_err(|e|anyhow!("original protected flush/finish unresolved: {e}"))?;
        }
        ensure!(
            self.workbench.maintenance_warning().is_none(),
            "original protected maintenance is not clean"
        );
        self.attempt.finished = true;
        self.common_finished = !shared_parent;
        Ok(())
    }
    pub(crate) fn take_finished_receipt(&mut self)->Result<FinishedOwnerReceipt> {
        ensure!(self.attempt.finished && !self.attempt.receipt_taken,"original successful finish receipt absent or consumed");
        ensure!(self.workbench.agent_owner_binding()==self.original_binding && self.workbench.maintenance_warning().is_none(),
            "original protected finish changed or maintenance unresolved");
        let scope = self.attempt.scope.as_ref().ok_or_else(|| anyhow!("original lifecycle was not bound before execution"))?;
        let receipt_scope = OwnerAttemptScope::for_lifecycle(scope.original.clone(),scope.shared_parent);
        self.attempt.receipt_taken=true;
        Ok(FinishedOwnerReceipt {binding:self.original_binding, scope: receipt_scope})
    }
}
