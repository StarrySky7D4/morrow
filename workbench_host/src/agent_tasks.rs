//! Native trusted agent execution on the complete original Workbench owner.
//! This is not the legacy IO/service carrier and has no administrative guest lane.
mod commands;
pub(crate) mod context_lifecycle;
pub(crate) mod preparation;
mod sealed_proposal;
mod trusted;
mod worker;
mod native_session;
pub use native_session::{NativeSessionEndpoint, NativeSessionPort};
use crate::io_tasks::{AccessError, Snapshot, TaskKey};
use crate::{Result, Workbench, WorkbenchState, now};
pub use commands::{AgentCommand, AgentCommandErrorClass, AgentCommandHandle, AgentCommandSnapshot, AgentCommandStage, AgentError, AgentReply};
use morrow_agent_catalog_admin_v1::Revisions;
pub use preparation::{AgentPreparation, PreparedAgentContext, PreparedNativeSessionPackage};
pub use sealed_proposal::SealedProposalSpec;
use std::{sync::Arc, time::Duration};
pub use worker::{
    AgentCleanup, AgentContext, AgentContextFailure, AgentExit, AgentLimits, AgentPhase,
    AgentProgress, AgentSchedulerPhase, AgentSchedulerStatus, AgentSpawnFailure, AgentWorker,
    MAX_AGENT_FRAME_BYTES, MAX_AGENT_SCHEDULERS, agent_scheduler_status,
    reap_agent_scheduler_debts,
};

pub struct AgentStart {
    pub package_id: String,
    pub full_sha256: [u8; 32],
    pub revisions: Revisions,
    pub lifetime: Duration,
    pub limits: AgentLimits,
}
#[derive(Clone, Copy, Debug)]
pub struct AgentExitStatus {
    pub execution: std::result::Result<(), AgentError>,
    pub disconnect: std::result::Result<(), AgentError>,
    pub maintenance: std::result::Result<(), AgentError>,
}
#[derive(Clone, Copy, Debug)]
pub struct AgentSnapshot {
    pub task: Snapshot,
    pub progress: Option<AgentProgress>,
    pub exit: Option<AgentExitStatus>,
}
impl WorkbenchState {
    /// A single R2 authority travels with the entire original owner. Creating
    /// another ledger owner would silently invalidate externally held contexts.
    fn agent_session_host(
        &mut self,
    ) -> Result<Arc<morrow_agent_session_exec_v1_r2::authority::SessionExecHost>> {
        if self.agent_session_host_failed {
            return Err("original agent authority initialization failed; no replacement".into());
        }
        if self.agent_session_host.is_none() {
            match morrow_agent_session_exec_v1_r2::authority::SessionExecHost::new(&mut self.host) {
                Ok(host) => self.agent_session_host = Some(Arc::new(host)),
                Err(_) => {
                    self.agent_session_host_failed = true;
                    return Err("original agent authority initialization failed; explicit recovery required".into());
                }
            }
        }
        let host = self
            .agent_session_host
            .as_ref()
            .ok_or("original agent authority unavailable")?;
        host.generation(&self.host)
            .map_err(|e| format!("original agent authority: {e:?}"))?;
        Ok(host.clone())
    }
}
impl Workbench {
    /// Original owner identity only: no authorization, owner-return or task-success proof.
    /// Reads the retained ledger binding without touching the Store or a ledger lock.
    pub fn agent_owner_binding(&self) -> morrow_core::dispatch::HostBinding {
        self.state.agent_contexts.owner_binding()
    }
    /// Synchronous trusted-native preparation on the existing protected owner.
    /// The callback has no runtime getter or OS-start method. Failed business
    /// preparation is never replayed; exact connection cleanup may be retained.
    pub fn prepare_agent_context(
        &mut self,
        build: impl FnOnce(&mut AgentPreparation<'_>) -> Result<PreparedAgentContext>,
    ) -> Result<AgentContext> {
        self.state.try_reclaim()?;
        self.state.ensure_agent_admission()?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let ledger = self.state.agent_contexts.clone();
        ledger.ensure_capacity()?;
        let state = self.state.local_mut()?;
        state.host.prepare_write()?;
        gate.check()?;
        let host = state.agent_session_host()?;
        let mut reserved = vec!["org.morrow.workbench".to_owned()];
        if let Some(bundle) = &state.bundle {
            reserved.push(bundle.manifest().package_id.clone());
        }
        reserved.sort();
        reserved.dedup();
        let mut preparation = AgentPreparation::with_catalog(
            &mut state.host,
            state.start,
            state.manager.as_mut(),
            state.agent_catalog.as_mut(),
            reserved,
            host,
            ledger,
        );
        let prepared =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| build(&mut preparation)));
        let failure: Box<dyn std::error::Error> = match prepared {
            Ok(Ok(prepared)) => match gate
                .check()
                .and_then(|()| preparation.release_context(prepared))
            {
                Ok(context) => return Ok(context),
                Err(error) => error,
            },
            Ok(Err(error)) => error,
            Err(_) => "agent preparation panicked; no execution replay".into(),
        };
        let mut debt = preparation.into_debt(None);
        if debt.cleanup(&mut state.host).is_err() {
            self.state.agent_preparation_debt = Some(debt);
            return Err(
                "agent preparation cleanup retained; explicit repair required, no replay".into(),
            );
        }
        Err(failure)
    }
    /// Trusted dual-package preparation on the same protected owner. The
    /// callback prepares the original executor and optional sealed proposer;
    /// this method attaches the approved session-only package before tracking.
    /// The original gate, panic handling, failed-preparation debt and production
    /// port checks remain in prepare_agent_context. No new owner is opened.
    pub fn prepare_agent_context_with_native_session(
        &mut self,
        package: &mut Option<morrow_agent_session_process_v1_host::catalog::CatalogManagedPackage>,
        build: impl FnOnce(&mut AgentPreparation<'_>) -> Result<(
            morrow_agent_process_control_v1::host::Host,
            Option<morrow_codex_session_exec_windows_v1::BorrowedWindowsExecutionPort>,
        )>,
    ) -> Result<AgentContext> {
        self.prepare_agent_context(|preparation| {
            let (processes, port) = build(preparation)?;
            preparation.context_with_native_session_package(package, processes, port)
        })
    }
    /// Release an unstarted context on its original owner. Before attachment,
    /// errors preserve the caller's Option; afterwards any cleanup debt stays
    /// in this Workbench for explicit repair, never a repeated preparation.
    pub fn dispose_agent_context(&mut self, context: &mut Option<AgentContext>) -> Result<()> {
        self.state.try_reclaim()?;
        // Cleanup only: a stopped business gate cannot prevent original retirement.
        if self.state.agent_preparation_debt.is_some() {
            return Err(AccessError::RecoveryRequired.into());
        }
        let ledger = self.state.agent_contexts.clone();
        let state = self.state.local_mut()?;
        let retained = context.as_ref().ok_or("prepared context unavailable")?;
        ledger.validate_external(retained, state.host.binding())?;
        if retained.resources.owner_binding() != state.host.binding() {
            return Err("foreign prepared context".into());
        }
        let mut debt = preparation::AgentPreparationDebt::for_context(
            context.take().ok_or("prepared context unavailable")?,
        );
        if debt
            .cleanup(&mut state.host)
            .and_then(|()| state.host.flush_pending())
            .is_err()
        {
            self.state.agent_preparation_debt = Some(debt);
            return Err("prepared context cleanup retained; explicit repair required".into());
        }
        Ok(())
    }
    /// Fixed native review only. It derives the domain from the original
    /// provisioned backend and pins the artifact. The original R2 authority is
    /// initialized once on first use and retained; no context is invalidated,
    /// no connection is granted, no tool proposed and no child started.
    pub fn review_agent_invocation(
        &mut self,
        backend: Arc<morrow_codex_session_exec_windows_v1::ProvisionedWindowsBackend>,
        operation: String,
        params: morrow_codex_session_exec_windows_v1::ExecParams,
        artifact_sha256: [u8; 32],
        max_runtime_ms: u64,
    ) -> Result<morrow_codex_session_exec_windows_v1::ReviewedBorrowedInvocation> {
        self.state.try_reclaim()?;
        self.state.ensure_agent_admission()?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let state = self.state.local_mut()?;
        state.host.prepare_write()?;
        if !backend.is_production() {
            return Err("production review backend required".into());
        }
        let host = state.agent_session_host()?;
        let resources =
            morrow_codex_session_exec_windows_v1::BorrowedNativeResources::for_owner(&state.host)
                .map_err(|e| format!("original native review resources: {e:?}"))?;
        let port = morrow_codex_session_exec_windows_v1::BorrowedWindowsExecutionPort::new(
            &state.host,
            host,
            resources,
            backend,
        )
        .map_err(|e| format!("original native review port: {e:?}"))?;
        let review = port
            .review_fixed(operation, params, artifact_sha256, max_runtime_ms)
            .map_err(|e| format!("original native fixed review: {e:?}"))?;
        gate.check()?;
        Ok(review)
    }
    /// Explicitly clean only the exact failed preparation; never rerun its
    /// callback, session command, proposal, approval, claim, or OS execution.
    pub fn repair_agent_preparation(&mut self) -> Result<()> {
        self.state.try_reclaim()?;
        // No callback, new admission, native execution or business replay.
        self.state.local()?;
        let ledger = self.state.agent_contexts.clone();
        loop {
            let binding = self.state.local()?.host.binding();
            let mut debt = match self.state.agent_preparation_debt.take() {
                Some(debt) => debt,
                None => match ledger.take_abandoned(binding)? {
                    Some(context) => preparation::AgentPreparationDebt::for_context(context),
                    None => return Ok(()),
                },
            };
            let state = self.state.local_mut()?;
            if debt
                .cleanup(&mut state.host)
                .and_then(|()| state.host.flush_pending())
                .is_err()
            {
                self.state.agent_preparation_debt = Some(debt);
                return Err(
                    "agent preparation cleanup or maintenance remains pending; no replay".into(),
                );
            }
        }
    }
    /// Read historical tool facts only after the actual owner has returned.
    /// This does not restore authority, CAS facts, or retry an OS effect.
    pub fn inspect_agent_tool(
        &mut self,
        host: &morrow_agent_session_exec_v1_r2::authority::SessionExecHost,
        operation: &str,
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let observation = host
            .inspect_tool_record(&self.state.local()?.host, operation)
            .map_err(|e| format!("original owner tool observation: {e:?}"))?;
        gate.check()?;
        Ok(observation)
    }
    pub fn agent_scheduler_status(&self) -> AgentSchedulerStatus {
        agent_scheduler_status()
    }
    /// Explicit runtime debt cleanup only; this never restores a missing owner
    /// or marks StateSlot/ProtectedSession maintenance repaired.
    pub fn reap_agent_schedulers(&mut self) -> Result<AgentSchedulerStatus> {
        reap_agent_scheduler_debts().map_err(|e| format!("agent scheduler cleanup: {e:?}"))?;
        Ok(agent_scheduler_status())
    }
    /// Host-only production entry. Catalog approval is only a ceiling; context
    /// must carry the same original live executor grant and provisioned backend.
    /// Ordinary qualification uses generic AgentWorker, never this entry.
    /// Before Worker attachment, every rejection leaves the original context
    /// in the caller's Option for explicit disposal. Only actual attachment
    /// consumes it; spawn failure retains the exact owner/bridge in StateSlot.
    pub fn start_agent(
        &mut self,
        options: AgentStart,
        prepared: &mut Option<AgentContext>,
    ) -> Result<TaskKey> {
        self.state.try_reclaim()?;
        self.state.require_writable()?;
        if options.package_id.is_empty()
            || options.package_id.len() > 256
            || options.lifetime.is_zero()
            || options.lifetime > Duration::from_secs(60)
        {
            return Err("invalid agent task bounds".into());
        }
        self.state.ensure_agent_admission()?;
        options
            .limits
            .validate()
            .map_err(|_| "invalid agent budgets")?;
        let mut raw_key = [0; 32];
        getrandom::fill(&mut raw_key)?;
        if raw_key == [0; 32] {
            return Err("agent task identity unavailable".into());
        }
        let key = TaskKey::from_bytes(&raw_key)?;
        let gate = self.state.product_gate.clone();
        gate.check()?;
        let ledger = self.state.agent_contexts.clone();
        let context = prepared
            .as_ref()
            .ok_or("prepared agent context unavailable")?;
        let state = self.state.local_mut()?;
        ledger.validate_external(context, state.host.binding())?;
        if context.resources.owner_binding() != state.host.binding()
            || context.port().is_some_and(|port| {
                !port.is_production() || port.owner_binding() != state.host.binding()
            })
        {
            return Err("agent backend or resource owner is not production-bound".into());
        }
        if !context.resources.is_clean()? {
            return Err(AccessError::RecoveryRequired.into());
        }
        let start = state.start;
        let time = now(start);
        let expires = time
            .checked_add(options.lifetime.as_millis().try_into()?)
            .ok_or("agent lifetime overflow")?;
        state.host.prepare_write()?;
        gate.check()?;
        let mut reserved = vec!["org.morrow.workbench".to_owned()];
        if let Some(bundle) = &state.bundle {
            reserved.push(bundle.manifest().package_id.clone());
        }
        reserved.sort();
        reserved.dedup();
        let context_host = context.host.clone();
        let catalog = state
            .agent_catalog
            .as_mut()
            .ok_or("agent catalog unavailable")?;
        let manager = state.manager.as_mut().ok_or("plugin manager unavailable")?;
        prepared
            .as_mut()
            .expect("validated prepared context")
            .attach_lifecycle()?;
        let package_result = catalog.connect(
            manager,
            &mut state.host,
            &context_host,
            &options.package_id,
            options.full_sha256,
            options.revisions,
            expires,
            time,
            reserved,
        );
        let package = match package_result {
            Ok(package) => package,
            Err(error) => {
                if let Some(ticket) = &prepared
                    .as_ref()
                    .expect("original prepared context")
                    .lifecycle
                {
                    ticket.detach_before_spawn()?;
                }
                return Err(error.into());
            }
        };
        let owner = match self.state.take_agent_owner() {
            Ok(owner) => owner,
            Err(error) => {
                // Unreachable under the exclusive validated StateSlot borrow,
                // but retain both real grants if owner movement ever fails.
                self.state.agent_preparation_debt =
                    Some(preparation::AgentPreparationDebt::for_package(
                        prepared.take().expect("validated original context"),
                        package,
                    ));
                return Err(error);
            }
        };
        // No callback receives `prepared` during the exclusive validation above.
        // Admission and local owner access also proved StateSlot has no task.
        let context = prepared
            .take()
            .expect("validated prepared context under exclusive borrow");
        let clock: Arc<dyn Fn() -> u64 + Send + Sync> = Arc::new(move || now(start));
        match AgentWorker::spawn(
            owner,
            package,
            context,
            gate,
            options.limits,
            expires,
            clock,
        ) {
            Ok(worker) => {
                self.state.install_agent_worker(key, worker);
                Ok(key)
            }
            Err(failure) => {
                self.state.restore_agent_failure(key, *failure);
                Err("agent worker did not start; inspect this task before retrying".into())
            }
        }
    }
    pub fn submit_agent(&self, key: TaskKey, command: AgentCommand) -> Result<AgentCommandHandle> {
        self.state
            .agent_worker(key)?
            .submit(command)
            .map_err(|e| format!("agent command: {e:?}").into())
    }
    /// Concrete native R2 session port on this exact already running original owner.
    /// Wrong/busy tasks are rejected by the original StateSlot; no owner is seized.
    pub fn native_session_port(&self, key: TaskKey) -> Result<NativeSessionPort> {
        self.state.agent_worker(key)?.native_session_port()
            .map_err(|error| format!("native session port: {error:?}").into())
    }
    /// Actual join is the only route back to local original owner access.
    pub fn poll_agent(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        self.state.check_agent_task(key)?;
        self.state.try_reclaim()?;
        self.state.agent_snapshot(key)
    }
    /// Includes the retained task key if an OS spawn failed after preparation.
    pub fn agent_status(&mut self) -> Result<Option<AgentSnapshot>> {
        self.state.try_reclaim()?;
        self.state.current_agent_snapshot()
    }
    pub fn cancel_agent(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        self.state.agent_worker(key)?.stop();
        self.poll_agent(key)
    }
    /// Explicitly retries same-owner cleanup/facts only. No guest/start replay.
    pub fn recover_agent(&mut self, key: TaskKey) -> Result<AgentSnapshot> {
        self.state.check_agent_task(key)?;
        if let Ok(worker) = self.state.agent_worker(key) {
            worker.retry_cleanup();
        } else {
            self.repair_io(key)?;
        }
        self.poll_agent(key)
    }
    pub fn acknowledge_agent(&mut self, key: TaskKey) -> Result<()> {
        self.state.check_agent_task(key)?;
        self.acknowledge_io(key)
    }
}
