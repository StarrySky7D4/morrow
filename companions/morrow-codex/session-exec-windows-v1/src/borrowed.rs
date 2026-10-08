//! Short borrows of the original owner; asynchronous work sees only native
//! process handles and bounded facts, never Core or a protected key lease.
use crate::{
    BorrowedNativeResources, ProvisionedWindowsBackend, WindowsProcessProvider,
    artifact::LockedArtifact, process::block_on,
};
use codex_exec_server::{ExecParams, ProcessControlCapabilities, StartedExecProcess};
use morrow_agent_session_exec_v1_r2::{
    Error, Intent, Result,
    authority::{Admission, SessionExecHost},
    safe_exec::ToolIdentity,
};
use morrow_core::dispatch::{Connection, HostBinding, HostRuntime};
use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};


// Original-owner historical observation only; no payloads, authority or sink.
// Clone retains only this atomic cell, never a native resource/backend alias.
#[derive(Clone)]
pub(crate) struct NativeStartDiagnostic(Arc<AtomicU64>);
impl Default for NativeStartDiagnostic {
    fn default() -> Self { Self(Arc::new(AtomicU64::new(0))) }
}
impl NativeStartDiagnostic {
    fn stage(&self, stage: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00 != 0 { return None; }
            Some((old & !0xff) | u64::from(stage))
        });
    }
    fn fail(&self, stage: u8, class: u8) {
        let _ = self.0.fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
            if old & 0xff00 != 0 { return None; }
            Some((old & !0xffff) | u64::from(stage) | (u64::from(class) << 8))
        });
    }
    fn flag(&self, flag: u64) { self.0.fetch_or(flag << 16, Ordering::AcqRel); }
}
fn start_stage_name(stage: u8) -> &'static str {
    match stage {
        1=>"OWNER_CHECK", 2=>"ORIGINAL_TOOL_CHECK", 3=>"REGISTRY_RESERVE", 4=>"FACT_RESERVE",
        5=>"R2_EXECUTE_CLAIMED", 6=>"CALLBACK_ENTERED", 7=>"BACKEND_START_TASK", 8=>"BACKEND_START_RETURNED",
        9=>"START_WAIT_TIMEOUT", 10=>"START_TASK_OR_RESULT_ERROR", 11=>"PREINVOKE_REJECTED",
        12=>"OBSERVER_REJECTED", 13=>"STARTED_TOOL_READ", 14=>"VALIDATE_STARTED_TOOL", 15=>"POSTSTART_VETO",
        16=>"PROVIDER_READY", _=>"NOT_OBSERVED",
    }
}
fn start_error_class(error: Error) -> u8 {
    match error {
        Error::Invalid=>1, Error::Contract=>2, Error::Limit=>3, Error::Correlation=>4,
        Error::Denied=>5, Error::Conflict=>6, Error::NotFound=>7, Error::CommitUnknown=>8, Error::Storage=>9,
    }
}
fn start_error_name(class: u8) -> &'static str {
    match class {
        1=>"R2Invalid", 2=>"R2Contract", 3=>"R2Limit", 4=>"R2Correlation", 5=>"R2Denied",
        6=>"R2Conflict", 7=>"R2NotFound", 8=>"R2CommitUnknown", 9=>"R2Storage",
        10=>"BackendError", 11=>"StartWaitTimeout", 12=>"TaskOrResultError", 13=>"ObserverRejected",
        14=>"PostStartVeto", _=>"NOT_OBSERVED",
    }
}
impl std::fmt::Debug for NativeStartDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = self.0.load(Ordering::Acquire);
        f.debug_struct("NativeStartDiagnosticV1")
            .field("stage", &start_stage_name(value as u8))
            .field("error_class", &start_error_name((value >> 8) as u8))
            .field("invocation_entered_observed", &(value & (1 << 16) != 0))
            .field("backend_task_entered_observed", &(value & (2 << 16) != 0))
            .field("actual_handle_observed", &(value & (4 << 16) != 0))
            .field("provider_ready_observed", &(value & (8 << 16) != 0)).finish()
    }
}

pub struct ReviewedBorrowedInvocation {
    intent: Intent,
    params: ExecParams,
    artifact: Arc<LockedArtifact>,
    backend: Arc<ProvisionedWindowsBackend>,
    binding: HostBinding,
}
impl ReviewedBorrowedInvocation {
    pub fn intent(&self) -> &Intent {
        &self.intent
    }
}
pub struct BorrowedWindowsExecutionPort {
    host: Arc<SessionExecHost>,
    resources: Arc<BorrowedNativeResources>,
    backend: Arc<ProvisionedWindowsBackend>,
    stopped: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct BorrowedStopHandle {
    stopped: Arc<AtomicBool>,
    resources: Arc<BorrowedNativeResources>,
}
impl BorrowedStopHandle {
    pub fn request_stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.resources.request_stop();
    }
}
struct Lifetime {
    _artifact: Arc<LockedArtifact>,
    _resources: Arc<BorrowedNativeResources>,
    _backend: Arc<ProvisionedWindowsBackend>,
}
impl BorrowedWindowsExecutionPort {
    pub fn new(
        runtime: &HostRuntime,
        host: Arc<SessionExecHost>,
        resources: Arc<BorrowedNativeResources>,
        backend: Arc<ProvisionedWindowsBackend>,
    ) -> Result<Self> {
        resources.bind_host(host.clone(), runtime)?;
        Ok(Self {
            host,
            resources,
            backend,
            stopped: Arc::new(AtomicBool::new(false)),
        })
    }
    pub fn owner_binding(&self) -> HostBinding {
        self.resources.owner_binding()
    }
    pub fn is_production(&self) -> bool {
        self.backend.is_production()
    }
    /// The original configured scheduler, for a separate synchronous owner
    /// anchor. Resource emptiness does not prove asynchronous tasks are joined.
    /// No new scheduler, Core, execution authority or guest handle is created.
    pub fn scheduler(&self) -> Arc<tokio::runtime::Runtime> {
        self.backend.runtime.clone()
    }
    pub fn stop_handle(&self) -> BorrowedStopHandle {
        BorrowedStopHandle {
            stopped: self.stopped.clone(),
            resources: self.resources.clone(),
        }
    }
    /// Review only: creates no proposal, approval, claim or admission.
    pub fn review_fixed(
        &self,
        operation: String,
        params: ExecParams,
        artifact_sha256: [u8; 32],
        max_runtime_ms: u64,
    ) -> Result<ReviewedBorrowedInvocation> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err(Error::Denied);
        }
        let mut intent = crate::fixed_intent(operation, &params, artifact_sha256, max_runtime_ms)?;
        intent.execution_domain = self.backend.domain(&params)?;
        intent.validate()?;
        let artifact = Arc::new(LockedArtifact::acquire(
            &intent.program,
            intent.artifact_sha256,
            &intent.cwd,
        )?);
        Ok(ReviewedBorrowedInvocation {
            intent,
            params,
            artifact,
            backend: self.backend.clone(),
            binding: self.owner_binding(),
        })
    }
    // These are independent trusted inputs; none is fabricated from wire IDs.
    #[allow(clippy::too_many_arguments)]
    pub fn start_claimed(
        &self,
        runtime: &mut HostRuntime,
        executor: Arc<Connection>,
        admission: &Admission,
        reviewed: ReviewedBorrowedInvocation,
        claim: [u8; 32],
        clock: impl FnMut() -> u64,
        live: impl Fn() -> bool,
    ) -> Result<WindowsProcessProvider> {
        let diagnostic = &self.resources.start_diagnostic;
        diagnostic.stage(1);
        self.resources.check(runtime)?;
        if self.stopped.load(Ordering::SeqCst)
            || !live()
            || reviewed.binding != runtime.binding()
            || !Arc::ptr_eq(&reviewed.backend, &self.backend)
        {
            return Err(Error::Denied);
        }
        self.host.generation(runtime)?;
        let ReviewedBorrowedInvocation {
            intent,
            params,
            artifact,
            ..
        } = reviewed;
        if intent.execution_domain != self.backend.domain(&params)? {
            return Err(Error::Denied);
        }
        diagnostic.stage(2);
        let before = self
            .host
            .inspect_tool_record(runtime, &intent.operation_id)?;
        if before.invocation_started || before.identity.intent_sha256 != intent.digest()? {
            return Err(Error::Denied);
        }
        let identity = before.identity;
        diagnostic.stage(3);
        let slot = self.resources.registry.reserve(
            intent.operation_id.clone(),
            Arc::new(Lifetime {
                _artifact: artifact,
                _resources: self.resources.clone(),
                _backend: self.backend.clone(),
            }),
        )?;
        let Some(_handoff) = slot.job() else {
            slot.unstarted();
            return Err(Error::Storage);
        };
        diagnostic.stage(4);
        if let Err(error) =
            self.resources
                .reserve_fact(identity.clone(), slot.clone(), self.backend.clone())
        {
            slot.unstarted();
            diagnostic.fail(4, start_error_class(error));
            return Err(error);
        }
        let writable = params.pipe_stdin || params.tty;
        let interruptible = !params.tty && params.sandbox.is_none();
        let deadline_ms = intent.max_runtime_ms;
        let started_at = Instant::now();
        let backend = self.backend.clone();
        let handle = backend.runtime.handle().clone();
        let clock = RefCell::new(clock);
        let mut invoked = false;
        let mut started = None;
        diagnostic.stage(5);
        let result = self.host.execute_claimed(
            runtime,
            &executor,
            admission,
            &intent.operation_id,
            claim,
            || (clock.borrow_mut())(),
            |committed| {
                if *committed != intent
                    || self.stopped.load(Ordering::SeqCst)
                    || !live()
                    || slot.stop_requested()
                {
                    return Err(Error::Denied);
                }
                // The original R2 invocation_started CAS precedes this callback.
                // Never inspect/reenter the original R2 fence from here.
                let job = slot.job().ok_or(Error::Storage)?;
                invoked = true;
                diagnostic.stage(6);
                diagnostic.flag(1);
                let task_slot = slot.clone();
                let task_backend = backend.clone();
                let task_diagnostic = diagnostic.clone();
                let mut task = handle.spawn(async move {
                    task_diagnostic.stage(7);
                    task_diagnostic.flag(2);
                    let _job = job;
                    if task_slot.stop_requested() {
                        task_slot.no_handle_unknown();
                        return Err(Error::CommitUnknown);
                    }
                    let actual = task_backend
                        .backend
                        .start(params)
                        .await
                        .map_err(|_| { task_diagnostic.fail(8, 10); Error::CommitUnknown });
                    match &actual {
                        Ok(process) => { task_diagnostic.flag(4); task_diagnostic.stage(8); task_slot.started(process.process.clone()) },
                        Err(_) => task_slot.no_handle_unknown(),
                    }
                    match actual {
                        Ok(actual) => {
                            // Sample the exact started process under the same
                            // charged async job, never infer support from TTY.
                            // Discovery is read-only and cancellation-safe. A stalled
                            // adapter must not delay installation of the exit/EOF observer.
                            let controls = tokio::time::timeout(
                                Duration::from_millis(200),
                                actual.process.checked_control_capabilities(),
                            )
                            .await
                            .ok()
                            .and_then(std::result::Result::ok)
                            .unwrap_or_default();
                            Ok((actual, controls))
                        }
                        Err(error) => Err(error),
                    }
                });
                match block_on(&handle, async {
                    tokio::time::timeout(Duration::from_millis(deadline_ms.min(10_000)), &mut task)
                        .await
                }) {
                    Ok(Ok(Ok(actual))) => started = Some(actual),
                    Err(_) => {
                        diagnostic.fail(9, 11);
                        slot.unknown();
                        let late_slot = slot.clone();
                        let late_resources = self.resources.clone();
                        let late_backend = backend.clone();
                        let late_identity = identity.clone();
                        let job = slot.job();
                        handle.spawn(async move {
                            let _job = job;
                            if let Ok(Ok((actual, controls))) = task.await {
                                // Attach the actual event observer before trusted
                                // cleanup; prefix gaps keep the receipt Unknown.
                                let provider = observer(
                                    actual,
                                    controls,
                                    late_backend.clone(),
                                    late_resources,
                                    late_identity,
                                    writable,
                                    interruptible,
                                    0,
                                    late_slot.clone(),
                                );
                                if let Some(provider) = provider {
                                    drop(provider);
                                } else if let Some(process) =
                                    late_slot.process.lock().ok().and_then(|p| p.clone())
                                {
                                    BorrowedNativeResources::cleanup_once(
                                        late_backend,
                                        late_slot.clone(),
                                        process,
                                    );
                                }
                            } else {
                                late_slot.no_handle_unknown();
                            }
                        });
                    }
                    _ => { diagnostic.fail(10, 12); slot.no_handle_unknown() },
                }
                // Start completion is not verified output/exit evidence.
                Err(Error::CommitUnknown)
            },
        );
        if !invoked {
            diagnostic.fail(11, start_error_class(result.as_ref().err().copied().unwrap_or(Error::CommitUnknown)));
            self.resources.cancel_unstarted(&identity);
            return Err(result.err().unwrap_or(Error::CommitUnknown));
        }
        let Some((actual, controls)) = started else {
            diagnostic.fail(10, 12);
            return Err(Error::CommitUnknown);
        };
        let remaining = deadline_ms.saturating_sub(
            started_at
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );
        let Some(provider) = observer(
            actual,
            controls,
            self.backend.clone(),
            self.resources.clone(),
            identity.clone(),
            writable,
            interruptible,
            remaining,
            slot.clone(),
        ) else {
            diagnostic.fail(12, 13);
            slot.unknown();
            let process = slot.process.lock().map_err(|_| Error::Storage)?.clone();
            if let Some(process) = process {
                BorrowedNativeResources::cleanup_once(self.backend.clone(), slot.clone(), process);
            }
            return Err(Error::CommitUnknown);
        };
        diagnostic.stage(13);
        let observed = self
            .host
            .inspect_tool_record(runtime, &identity.operation_id);
        let valid = matches!(observed,Ok(ref current) if current.identity==identity && current.invocation_started);
        diagnostic.stage(14);
        let authorization =
            self.host
                .validate_started_tool(runtime, &executor, admission, &identity, || {
                    (clock.borrow_mut())()
                });
        if result != Err(Error::CommitUnknown)
            || !valid
            || authorization.is_err()
            || self.stopped.load(Ordering::SeqCst)
            || !live()
            || slot.stop_requested()
        {
            diagnostic.fail(15, 14);
            slot.unknown();
            drop(provider);
            return Err(Error::CommitUnknown);
        }
        diagnostic.stage(16);
        diagnostic.flag(8);
        Ok(provider)
    }
}
#[allow(clippy::too_many_arguments)]
fn observer(
    actual: StartedExecProcess,
    controls: ProcessControlCapabilities,
    backend: Arc<ProvisionedWindowsBackend>,
    resources: Arc<BorrowedNativeResources>,
    identity: ToolIdentity,
    writable: bool,
    interruptible: bool,
    deadline: u64,
    slot: Arc<crate::registry::ExecutionSlot>,
) -> Option<WindowsProcessProvider> {
    if actual.sandbox_type != Some(backend.expected) {
        slot.unknown();
        return None;
    }
    let monitor_identity = identity.clone();
    Some(WindowsProcessProvider::new(
        actual.process,
        backend.runtime.handle().clone(),
        identity,
        writable,
        interruptible,
        controls,
        deadline,
        actual.sandbox_type,
        move |facts| resources.publish(&monitor_identity, facts),
        slot,
    ))
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn native_first_error_survives_late_actual_handle_observation() {
        let trace = NativeStartDiagnostic::default();
        trace.fail(9, 11); let late = trace.clone(); late.flag(4); late.stage(16);
        let value = trace.0.load(Ordering::Acquire);
        assert_eq!(value & 0xffff, 9 | (11 << 8));
        assert!(format!("{trace:?}").contains("actual_handle_observed: true"));
    }
    #[test]
    fn native_original_error_classes_are_fixed_and_complete() {
        let classes = [Error::Invalid, Error::Contract, Error::Limit, Error::Correlation, Error::Denied,
            Error::Conflict, Error::NotFound, Error::CommitUnknown, Error::Storage];
        for (index, error) in classes.into_iter().enumerate() { assert_eq!(start_error_class(error), (index + 1) as u8); }
    }
    #[test]
    fn native_format_failure_preserves_original_cell() {
        struct Refuse;
        impl std::fmt::Write for Refuse { fn write_str(&mut self, _: &str) -> std::fmt::Result { Err(std::fmt::Error) } }
        let trace = NativeStartDiagnostic::default(); trace.stage(5); trace.flag(1);
        let before = trace.0.load(Ordering::Acquire);
        assert!(std::fmt::write(&mut Refuse, format_args!("{trace:?}")).is_err());
        assert_eq!(trace.0.load(Ordering::Acquire), before);
    }
    #[test]
    fn native_snapshot_is_bounded_and_has_no_owner_payload() {
        let trace = NativeStartDiagnostic::default(); trace.fail(15, 14); trace.flag(15);
        let text = format!("{trace:?}"); assert!(text.len() < 384);
        assert_eq!(text, "NativeStartDiagnosticV1 { stage: \"POSTSTART_VETO\", error_class: \"PostStartVeto\", invocation_entered_observed: true, backend_task_entered_observed: true, actual_handle_observed: true, provider_ready_observed: true }");
        assert_eq!(std::sync::Arc::strong_count(&trace.0), 1);
    }
}
