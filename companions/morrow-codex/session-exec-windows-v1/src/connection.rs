use crate::{
    NativeR2State, SharedNativeR2State, WindowsProcessProvider, artifact::LockedArtifact,
    process::block_on,
};
use codex_exec_server::{ExecBackend, ExecParams, ProcessControlCapabilities, StartedExecProcess};
use morrow_agent_session_exec_v1_r2::{Environment, Error, Intent, Result};
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Duration};
use tokio::runtime::Handle;

/// Binds every upstream parameter, including sandbox/network/environment policy.
/// Sorted JSON object keys make HashMap insertion order irrelevant.
pub fn policy_domain(params: &ExecParams) -> Result<String> {
    let mut value = serde_json::to_value(params).map_err(|_| Error::Invalid)?;
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value).map_err(|_| Error::Invalid)?;
    let digest = Sha256::digest(bytes);
    Ok(format!(
        "windows-native-v1-{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}
pub fn fixed_intent(
    operation: String,
    params: &ExecParams,
    artifact_sha256: [u8; 32],
    max_runtime_ms: u64,
) -> Result<Intent> {
    if params.env.keys().any(|key| {
        codex_protocol::shell_environment::is_non_inheritable_env_var(key)
            || key.eq_ignore_ascii_case(
                codex_exec_server::CODEX_EXEC_SERVER_EXIT_ON_STDIN_CLOSE_ENV_VAR,
            )
    }) {
        return Err(Error::Invalid);
    }
    let (program, argv) = params.argv.split_first().ok_or(Error::Invalid)?;
    // Shell restoration/argv0 override are not represented by the fixed command.
    // Interactive input is admitted separately by the process-control host.
    if params.arg0.is_some() || params.shell_snapshot.is_some() || params.env_policy.is_some() {
        return Err(Error::Invalid);
    }
    let mut env = params
        .env
        .iter()
        .map(|(name, value)| Environment {
            name: name.clone(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    env.sort_by(|a, b| a.name.cmp(&b.name));
    let intent = Intent {
        operation_id: operation,
        artifact_sha256,
        program: program.clone(),
        argv: argv.to_vec(),
        cwd: params
            .cwd
            .to_path_buf()
            .into_os_string()
            .into_string()
            .map_err(|_| Error::Invalid)?,
        env,
        input: Vec::new(),
        execution_domain: policy_domain(params)?,
        max_runtime_ms,
    };
    intent.validate()?;
    Ok(intent)
}
/// Opaque native review. Neither Wasm nor a serialized request can construct it.
pub struct ReviewedWindowsInvocation {
    intent: Intent,
    params: ExecParams,
    artifact: Arc<LockedArtifact>,
}
/// New trusted integration seam over actual upstream public execution traits.
/// This deliberately does not depend on the old SQLx-based companion adapter.
pub struct WindowsExecutionPort {
    state: SharedNativeR2State,
    backend: Arc<dyn ExecBackend>,
    runtime: Handle,
    registry: Arc<crate::NativeExecutionRegistry>,
}
impl WindowsExecutionPort {
    pub fn new(
        state: SharedNativeR2State,
        backend: Arc<dyn ExecBackend>,
        runtime: Handle,
    ) -> Result<Self> {
        if runtime.runtime_flavor() != tokio::runtime::RuntimeFlavor::MultiThread
            || runtime.metrics().num_workers() < 2
        {
            return Err(Error::Invalid);
        }
        let registry = state
            .lock()
            .map_err(|_| Error::Storage)?
            .native_executions
            .clone();
        Ok(Self {
            registry,
            state,
            backend,
            runtime,
        })
    }
    pub fn approve_fixed(
        &self,
        intent: Intent,
        params: ExecParams,
    ) -> Result<ReviewedWindowsInvocation> {
        if fixed_intent(
            intent.operation_id.clone(),
            &params,
            intent.artifact_sha256,
            intent.max_runtime_ms,
        )? != intent
        {
            return Err(Error::Denied);
        }
        let artifact = Arc::new(LockedArtifact::acquire(
            &intent.program,
            intent.artifact_sha256,
            &intent.cwd,
        )?);
        Ok(ReviewedWindowsInvocation {
            intent,
            params,
            artifact,
        })
    }
    /// Called by the trusted owner only after the original guest proposal was
    /// reviewed, approved and claimed. This method never proposes or claims.
    pub fn start_claimed(
        &self,
        reviewed: ReviewedWindowsInvocation,
        claim: [u8; 32],
    ) -> Result<WindowsProcessProvider> {
        let ReviewedWindowsInvocation {
            intent,
            params,
            artifact,
        } = reviewed;
        let operation = intent.operation_id.clone();
        let slot = self.registry.reserve(operation.clone(), artifact)?;
        let _handoff = slot.job().ok_or(Error::Storage)?;
        let backend = self.backend.clone();
        let runtime = self.runtime.clone();
        let writable = params.pipe_stdin || params.tty;
        let interruptible = !params.tty && params.sandbox.is_none();
        let deadline_ms = intent.max_runtime_ms;
        let mut invoked = false;
        let mut started: Option<(StartedExecProcess, ProcessControlCapabilities)> = None;
        let start_clock = std::time::Instant::now();
        let identity_result = NativeR2State::with_state(&self.state, |state| {
            let before = state.host.inspect_tool_record(&state.runtime, &operation)?;
            if before.identity.intent_sha256 != intent.digest()? || before.invocation_started {
                return Err(Error::Denied);
            }
            let result = state.host.execute_claimed(
                &mut state.runtime,
                &state.executor_connection,
                &state.executor,
                &operation,
                claim,
                || (state.clock)(),
                |committed| {
                    if *committed != intent {
                        return Err(Error::Denied);
                    }
                    invoked = true;
                    let task_slot = slot.clone();
                    let job = slot.job().ok_or(Error::Storage)?;
                    let mut task = runtime.spawn(async move {
                        let _job = job;
                        let result = backend.start(params).await;
                        match &result {
                            Ok(actual) => task_slot.started(actual.process.clone()),
                            Err(_) => task_slot.no_handle_unknown(),
                        }
                        match result {
                            Ok(actual) => {
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
                    let limit = Duration::from_millis(deadline_ms.min(10_000));
                    match block_on(&runtime, async {
                        tokio::time::timeout(limit, &mut task).await
                    }) {
                        Ok(Ok(Ok(actual))) => started = Some(actual),
                        Err(_) => {
                            slot.unknown();
                            let late_slot = slot.clone();
                            let job = slot.job();
                            runtime.spawn(async move {
                                let _job = job;
                                if let Ok(Ok((actual, _controls))) = task.await {
                                    crate::process::cleanup_actual(actual.process, &late_slot)
                                        .await;
                                } else {
                                    late_slot.no_handle_unknown();
                                }
                            });
                        }
                        _ => {
                            slot.no_handle_unknown();
                        }
                    }
                    // A start receipt is never fabricated exit/EOF evidence.
                    Err(Error::CommitUnknown)
                },
            );
            if result != Err(Error::CommitUnknown) {
                return Err(result.err().unwrap_or(Error::CommitUnknown));
            }
            let after = state.host.inspect_tool_record(&state.runtime, &operation)?;
            if !after.invocation_started || after.identity != before.identity {
                return Err(Error::CommitUnknown);
            }
            Ok(after.identity)
        });
        let identity = match identity_result {
            Ok(identity) => identity,
            Err(error) => {
                if !invoked {
                    slot.unstarted();
                }
                if let Some((actual, _controls)) = started.take() {
                    slot.unknown();
                    let cleanup_slot = slot.clone();
                    let job = slot.job();
                    self.runtime.spawn(async move {
                        let _job = job;
                        crate::process::cleanup_actual(actual.process, &cleanup_slot).await;
                    });
                }
                return Err(error);
            }
        };
        let (actual, controls) = started.ok_or(Error::CommitUnknown)?;
        let state_for_monitor = self.state.clone();
        let observed_identity = identity.clone();
        let remaining_ms = deadline_ms.saturating_sub(
            start_clock
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        );
        let provider = WindowsProcessProvider::new(
            actual.process,
            self.runtime.clone(),
            identity.clone(),
            writable,
            interruptible,
            controls,
            remaining_ms,
            actual.sandbox_type,
            move |facts| {
                let _ = NativeR2State::with_state(&state_for_monitor, |state| {
                    let current = state
                        .host
                        .inspect_tool_record(&state.runtime, &observed_identity.operation_id)?;
                    if current.identity != observed_identity || !current.invocation_started {
                        return Err(Error::Denied);
                    }
                    state
                        .host
                        .reconcile_tool_observation(
                            &mut state.runtime,
                            &observed_identity,
                            current.record_revision,
                            &facts,
                        )
                        .map(|_| ())
                });
            },
            slot,
        );
        let live = NativeR2State::with_state(&self.state, |state| {
            state.host.validate_started_tool(
                &state.runtime,
                &state.executor_connection,
                &state.executor,
                &identity,
                || (state.clock)(),
            )
        });
        if live.is_err() {
            // Provider Drop requests trusted cleanup; no active guest handle is delivered.
            drop(provider);
            return Err(Error::CommitUnknown);
        }
        Ok(provider)
    }
    pub fn cleanup_status(&self) -> Result<Vec<crate::NativeExecutionStatus>> {
        self.registry.statuses()
    }
    /// Read-only cleanup reconciliation. An uncertain terminate is not replayed.
    /// Slots remain occupied if actual exit AND output EOF cannot be observed.
    pub fn reap_cleanup(&self) -> Result<usize> {
        let before = self.registry.statuses()?.len();
        for slot in self.registry.slots()? {
            let Some(job) = slot.job() else { continue };
            let process = slot.process.lock().map_err(|_| Error::Storage)?.clone();
            if let Some(process) = process {
                let read = block_on(&self.runtime, async {
                    tokio::time::timeout(
                        Duration::from_millis(200),
                        process.read(None, Some(1), Some(0)),
                    )
                    .await
                });
                if matches!(read,Ok(Ok(r)) if r.exited && r.closed) {
                    slot.terminal();
                }
                drop(process);
            }
            drop(job);
        }
        Ok(before.saturating_sub(self.registry.statuses()?.len()))
    }
}
