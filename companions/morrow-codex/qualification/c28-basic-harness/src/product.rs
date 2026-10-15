//! Production constructor and original public Workbench operations.
//! Calling these live helpers is separate from the default read-only CLI.
use anyhow::{Result, anyhow, ensure};
use morrow_agent_process_control_v1::{
    Capabilities as ProcessCaps,
    host::{Budget, Handle},
};
use morrow_agent_session_exec_v1_r2::{
    Action, Intent, Outcome, Reply, Request,
    authority::{Capabilities, SessionExecHost},
};
use morrow_codex_session_exec_windows_v1::{
    ExecParams, ExecServerRuntimeOptions, HttpClientFactory, OutboundProxyPolicy,
    ProvisionedWindowsBackend, ReviewedBorrowedInvocation, WindowsRunnerProvisioningSpec,
};
use morrow_workbench_host::{
    Workbench,
    agent_tasks::{
        AgentCommand, AgentCommandHandle, AgentContext, AgentError, AgentLimits, AgentReply,
        AgentSnapshot, AgentStart,
    },
    io_tasks::{StoragePhase, TaskKey},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use codex_windows_sandbox::MatchedRunnerArtifact;

use crate::preflight::{Artifact, Config, observe_images, observe_materialized_images};

pub struct PreparedExecution {
    pub context: Option<AgentContext>,
    pub host: Arc<SessionExecHost>,
    pub reviewed: Option<ReviewedBorrowedInvocation>,
    pub intent: Intent,
    pub claim: [u8; 32],
}

pub struct RunningExecution {
    pub key: TaskKey,
    pub handle: Handle,
    pub host: Arc<SessionExecHost>,
    pub intent: Intent,
    pub resources: Arc<morrow_codex_session_exec_windows_v1::BorrowedNativeResources>,
    stop_attempted: bool,
    runner_history: morrow_codex_session_exec_windows_v1::ReviewedRunnerObservation,
    verified_owner_join: Option<crate::native_observation::VerifiedOwnerJoin>,
    native_binding:Option<morrow_codex_session_exec_windows_v1::ReviewedNativeCompletion>,
    native_completion:Option<morrow_codex_session_exec_windows_v1::NativeExecutionCompletion>,
}

/// Consumes the reviewed one-shot claim exactly once. Caller has completed all
/// explicit original catalog steps; no helper or ordinary fallback is provided.
pub fn start_execution(
    workbench: &mut Workbench,
    prepared: &mut PreparedExecution,
    wrapper: &crate::wrapper::ReviewedWrapper,
    capabilities: ProcessCaps,
) -> Result<RunningExecution> {
    let resources = prepared
        .context
        .as_ref()
        .ok_or_else(|| anyhow!("context already transferred; no replay"))?
        .resources
        .clone();
    let host = prepared.host.clone();
    let intent = prepared.intent.clone();
    let revisions = wrapper.approved_revisions()?;
    let key = workbench
        .start_agent(
            AgentStart {
                package_id: wrapper.id.clone(),
                full_sha256: wrapper.full_sha256,
                revisions,
                lifetime: Duration::from_secs(60),
                limits: AgentLimits::default(),
            },
            &mut prepared.context,
        )
        .map_err(|error| {
            anyhow!("original production worker start failed; retain task status: {error}")
        })?;
    let reviewed = prepared
        .reviewed
        .take()
        .ok_or_else(|| anyhow!("reviewed invocation already consumed; no replay"))?;
    let runner_history = reviewed.runner_observation();
    let native_binding=reviewed.native_completion_binding();
    let claim = std::mem::replace(&mut prepared.claim, [0; 32]);
    ensure!(claim != [0; 32], "claim already consumed; no replay");
    let mut command = submit_once(
        workbench,
        key,
        AgentCommand::StartClaimed {
            reviewed: Box::new(reviewed),
            claim,
            capabilities,
            budget: Budget::default(),
        },
    )?;
    let deadline = Instant::now() + Duration::from_secs(35);
    let handle = loop {
        match command
            .try_read()
            .map_err(|error| anyhow!("original native start delivery {:?}; no replay", error))?
        {
            Some(reply) => match &reply {
                AgentReply::Started(handle) => break *handle,
                _ => {
                    return Err(anyhow!(
                        "unexpected start reply; Unknown; inspect original task"
                    ));
                }
            },
            None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            None => {
                command.cancel();
                return Err(anyhow!(
                    "start delivery timeout; Unknown; original task/owner must be recovered explicitly"
                ));
            }
        }
    };
    Ok(RunningExecution {
        key,
        handle,
        host,
        intent,
        resources,
        stop_attempted: false,
        runner_history,
        verified_owner_join: None,
        native_binding,
        native_completion:None,
    })
}

impl RunningExecution {
    /// Serialize-only bounded history from this invocation's actual sealed
    /// completion. Distinct from the legacy logical observation and business
    /// child facts; it cannot reconstruct the capability consumed at finish.
    pub(crate) fn native_v8_completion_evidence(&self)->Option<serde_json::Value> {
        self.native_completion.as_ref().map(|actual|serde_json::json!({
            "schema":"C28_SAME_INVOCATION_V8_NATIVE_SDK_COMPLETION_V1",
            "identity_scope":"RUNNER_NOT_BUSINESS_CHILD",
            "runner_pid":actual.runner_pid(),
            "runner_creation_filetime_100ns":actual.runner_creation_filetime().to_string(),
            "runner_get_exit_code_process_raw_dword":actual.runner_raw_dword(),
            "startup_attempt":actual.startup_attempt(),
            "authenticated_terminal_close_ack":actual.terminal_close_ack_verified(),
            "original_host_and_forwarder_joins_complete":true,
            "same_sdk_monitor_joined_and_slot_retired":true,
            "scope":"SAME_ORIGINAL_INVOCATION_ONLY",
            "protected_owner_finish":"PENDING",
            "factory_native_shutdown":"PENDING",
            "cleanup_authority":false
        }))
    }
    pub(crate) fn take_native_execution_completion(&mut self)->Result<morrow_codex_session_exec_windows_v1::NativeExecutionCompletion> {
        self.native_completion.take().ok_or_else(||anyhow!("original native completion absent or consumed"))
    }
    pub(crate) fn runner_observation(&self, identity: &morrow_agent_session_exec_v1_r2::safe_exec::ToolIdentity)
        -> codex_exec_server::RunnerObservation {
        self.resources.runner_observation(&self.runner_history, identity)
    }
    pub(crate) fn verified_owner_join(&self) -> Option<&crate::native_observation::VerifiedOwnerJoin> {
        self.verified_owner_join.as_ref()
    }
    pub(crate) fn from_started(
        key: TaskKey,
        handle: Handle,
        host: Arc<SessionExecHost>,
        intent: Intent,
        resources: Arc<morrow_codex_session_exec_windows_v1::BorrowedNativeResources>,
        runner_history: morrow_codex_session_exec_windows_v1::ReviewedRunnerObservation,
        native_binding:Option<morrow_codex_session_exec_windows_v1::ReviewedNativeCompletion>,
    ) -> Self {
        Self {
            key,
            handle,
            host,
            intent,
            resources,
            stop_attempted: false,
            runner_history,
            verified_owner_join: None,
            native_binding,
            native_completion:None,
        }
    }
    /// Caller must first drop its backend/scheduler aliases synchronously. This
    /// verifies the original returned owner's real facts after a real OS join.
    /// It never replays a control, native start or uncertain completion CAS.
    pub fn stop_and_verify_returned_owner(
        &mut self,
        workbench: &mut Workbench,
        evidence: &crate::controls::ControlEvidence,
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.stop_and_verify_observed(workbench, evidence.saw_exit, evidence.saw_closed,
            evidence.exit_code, &evidence.stdout, &evidence.stderr)
    }
    /// Read/events-only basic evidence reuses the exact same authoritative
    /// facts, resource, original owner and actual Worker-join checks.
    pub fn stop_and_verify_basic_owner(
        &mut self, workbench: &mut Workbench, evidence: &crate::basic::TerminalEvidence,
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        self.stop_and_verify_observed(workbench, evidence.saw_exit, evidence.saw_closed,
            evidence.exit_code, &evidence.stdout, &evidence.stderr)
    }
    fn stop_and_verify_observed(
        &mut self, workbench: &mut Workbench, saw_exit: bool, saw_closed: bool,
        exit_code: Option<i32>, stdout: &[u8], stderr: &[u8],
    ) -> Result<morrow_agent_session_exec_v1_r2::safe_exec::ToolObservation> {
        ensure!(
            saw_exit && saw_closed && exit_code.is_some(),
            "real Exit and EOF evidence required before clean acceptance"
        );
        ensure!(
            !self.stop_attempted,
            "stop already requested; use explicit same-owner recovery, never repeat controls"
        );
        self.stop_attempted = true;
        if let Some(binding)=&self.native_binding {
            // No Core borrow/owner lock while awaiting this exact original task.
            binding.join_original_monitor_blocking(Instant::now()+Duration::from_secs(20))?;
        }
        stop_original(workbench, self.key)?;
        let deadline = Instant::now() + Duration::from_secs(20);
        let joined_task_results = loop {
            let snapshot = poll_original(workbench, self.key)?;
            if snapshot.task.storage == StoragePhase::Reclaimed {
                let exit = snapshot
                    .exit
                    .ok_or_else(|| anyhow!("original joined task exit missing"))?;
                ensure!(
                    exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok(),
                    "original joined task cleanup or protected maintenance failed"
                );
                break (exit.execution.is_ok(), exit.disconnect.is_ok(), exit.maintenance.is_ok());
            }
            ensure!(
                Instant::now() < deadline,
                "original owner/OS join unresolved; retained debt, no synthetic clean result"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        let resource_tables_clean_observed = self.resources.is_clean()?;
        // Preserve the original short-circuit and error order; no additional query.
        let pending_fact_table_empty_observed =
            resource_tables_clean_observed && self.resources.pending_facts()?.is_empty();
        ensure!(
            resource_tables_clean_observed && pending_fact_table_empty_observed,
            "original native resource/fact debt remains charged"
        );
        let observation = workbench
            .inspect_agent_tool(&self.host, &self.intent.operation_id)
            .map_err(|error| anyhow!("same-owner original tool observation failed: {error}"))?;
        ensure!(
            observation.invocation_started
                && observation.identity.intent_sha256 == self.intent.digest()?,
            "original started identity mismatch"
        );
        let facts = observation
            .latest
            .as_ref()
            .ok_or_else(|| anyhow!("original terminal facts absent"))?;
        ensure!(
            facts.output_closed
                && facts.exit_code == exit_code
                && facts.stdout_bytes == stdout.len() as u64
                && facts.stderr_bytes == stderr.len() as u64
                && facts.stdout_sha256 == morrow_agent_session_exec_v1_r2::hash(stdout)
                && facts.stderr_sha256 == morrow_agent_session_exec_v1_r2::hash(stderr),
            "original terminal facts differ from real sealed-guest observations"
        );
        if let Some(binding)=&self.native_binding {
            self.native_completion=Some(self.resources.take_native_execution_completion(binding)?);
        }
        self.verified_owner_join = Some(crate::native_observation::VerifiedOwnerJoin::captured(
            joined_task_results.0, joined_task_results.1, joined_task_results.2,
            resource_tables_clean_observed, pending_fact_table_empty_observed,
        ));
        Ok(observation)
    }
}

/// Explicit host-approved synthetic proposal and one-shot claim. This does not
/// start the child, install/approve a wrapper, or run the sealed proposal guest.
/// The returned claim is live only under these original connection admissions.
pub fn prepare_execution(
    workbench: &mut Workbench,
    backend: Arc<ProvisionedWindowsBackend>,
    session: &str,
    operation: &str,
    params: ExecParams,
    artifact_sha256: [u8; 32],
) -> Result<PreparedExecution> {
    let mut prepared = None;
    let context = workbench
        .prepare_agent_context(|owner| {
            let host = owner.session_host()?;
            let resources = owner.resources()?;
            let port = owner.bind_port(host.clone(), resources.clone(), backend)?;
            let reviewed =
                port.review_fixed(operation.to_owned(), params, artifact_sha256, 30_000)?;
            let intent = reviewed.intent().clone();
            let connection = owner.connect()?;
            let expires = owner.expires_after(60_000)?;
            // Explicit trusted-host synthetic qualification only: one original
            // connection/admission stays live until the Worker finishes real facts.
            // The guest receives its independent catalog ceiling and connection.
            let trusted_caps = Capabilities {
                session_read: true,
                session_write: true,
                propose: true,
                execute: true,
                retire: false,
            };
            let executor = owner.admit(
                &host,
                &connection,
                trusted_caps,
                trusted_caps,
                vec![session.to_owned()],
                intent.execution_domain.clone(),
                expires,
            )?;
            let generation = owner.generation(&host)?;
            let create = Request::new_for_generation(
                format!("{operation}-create"),
                generation,
                Action::Create {
                    session_id: session.to_owned(),
                    parent: None,
                    parent_tail: 0,
                },
            )?;
            let raw = owner.dispatch(&host, &connection, &executor, create.raw())?;
            if !matches!(
                Reply::decode_for(&create, &raw)?.outcome,
                Outcome::Session(_)
            ) {
                return Err("original synthetic session Create was not accepted; no retry".into());
            }
            let proposal = Request::new_for_generation(
                format!("{operation}-propose"),
                generation,
                Action::Propose {
                    session_id: session.to_owned(),
                    intent: intent.clone(),
                },
            )?;
            let raw = owner.dispatch(&host, &connection, &executor, proposal.raw())?;
            if !matches!(
                Reply::decode_for(&proposal, &raw)?.outcome,
                Outcome::Tool(_)
            ) {
                return Err("original synthetic proposal was not accepted; no retry".into());
            }
            let review = owner.review_tool(&host, operation)?;
            if review.proposal_sha256 != proposal.digest()
                || review.intent_sha256 != intent.digest()?
            {
                return Err("original reviewed proposal/intent identity mismatch".into());
            }
            let permit = owner.approve(
                &host,
                &connection,
                &executor,
                operation,
                proposal.digest(),
                intent.digest()?,
            )?;
            let request = Request::new_for_generation(
                format!("{operation}-claim"),
                generation,
                Action::Claim {
                    operation_id: operation.to_owned(),
                    permit,
                },
            )?;
            let raw = owner.dispatch(&host, &connection, &executor, request.raw())?;
            let claim = match Reply::decode_for(&request, &raw)?.outcome {
                Outcome::Claimed { claim, .. } => claim,
                _ => return Err("original claim was not accepted; no retry".into()),
            };
            let context = owner.context(Default::default(), Some(port))?;
            prepared = Some((host, reviewed, intent, claim));
            Ok(context)
        })
        .map_err(|error| anyhow!("original protected owner preparation failed: {error}"))?;
    let (host, reviewed, intent, claim) =
        prepared.ok_or_else(|| anyhow!("original preparation did not return a claim"))?;
    Ok(PreparedExecution {
        context: Some(context),
        host,
        reviewed: Some(reviewed),
        intent,
        claim,
    })
}

/// Retains the actual configured scheduler through worker/context cleanup.
/// Resource emptiness alone is not proof that native tasks or threads joined.
pub struct ProductionBackend {
    native_close_v8:bool,
    pub backend: Arc<ProvisionedWindowsBackend>,
    pub scheduler: Arc<tokio::runtime::Runtime>,
}
impl ProductionBackend {
    pub(crate) fn is_native_close_v8(&self)->bool {self.native_close_v8}
}

pub fn create_checked_backend(config: &Config) -> Result<ProductionBackend> {
    // Re-observe explicitly pinned inputs; production constructor acquires
    // independent deny-write/delete locks and validates actual runtime policy.
    observe_images(config)?;
    checked_backend(config, &config.runner)
}

/// Uses the original lifecycle's still-held actual launch-image lock. It never
/// treats the staged source, a PATH result, or a caller-selected external runner
/// as the materialized image. The lifecycle also retains its directory locks.
pub fn create_checked_backend_materialized(
    stage: &Config,
    actual: Arc<MatchedRunnerArtifact>,
) -> Result<ProductionBackend> {
    let materialized = Artifact {
        path: actual.path().to_path_buf(),
        sha256: actual.sha256(),
    };
    observe_materialized_images(stage, &materialized)?;
    // Keep `actual` alive across acquisition of the same checked image; the
    // returned provisioning spec/backend then owns its independent exact lock.
    let result = checked_backend(stage, &materialized);
    drop(actual);
    result
}

fn checked_backend(config: &Config, runner: &Artifact) -> Result<ProductionBackend> {
    let spec = WindowsRunnerProvisioningSpec::acquire(&runner.path, runner.sha256)?;
    checked_backend_spec(config,spec)
}

/// Separate explicit preview route; the default production-factory remains v7.
/// The lifecycle keeps its original materialized image lock while this exact
/// independently acquired v8 pin is installed in the real local backend.
pub fn create_native_close_backend_materialized(config:&Config,actual:Arc<MatchedRunnerArtifact>,
    retirement:&mut morrow_codex_session_exec_windows_v1::NativeFactoryRetirement)->Result<ProductionBackend> {
    let materialized=Artifact {path:actual.path().to_path_buf(),sha256:actual.sha256()};
    observe_materialized_images(config,&materialized)?;
    let spec=WindowsRunnerProvisioningSpec::from_locked_native_close_v8(actual.native_close_view())?;
    let (backend,scheduler)=ProvisionedWindowsBackend::production_with_native_close_runner(
        ExecServerRuntimeOptions::new(config.helper.path.clone(),None)?,
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),config.helper.sha256,spec,retirement)?;
    Ok(ProductionBackend {backend,scheduler,native_close_v8:true})
}

fn checked_backend_spec(config:&Config,spec:WindowsRunnerProvisioningSpec)->Result<ProductionBackend> {
    let scheduler = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?,
    );
    let backend = ProvisionedWindowsBackend::production_with_checked_runner(
        ExecServerRuntimeOptions::new(config.helper.path.clone(), None)?,
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault),
        config.helper.sha256,
        scheduler.clone(),
        spec,
    )?;
    ensure!(
        backend.is_production(),
        "production constructor returned a qualification backend"
    );
    Ok(ProductionBackend { backend, scheduler,native_close_v8:false })
}

/// Exactly one command submission. Errors after submission remain Unknown;
/// callers retain the original task and request explicit trusted cleanup.
pub fn submit_once(
    workbench: &Workbench,
    key: TaskKey,
    command: AgentCommand,
) -> Result<AgentCommandHandle> {
    workbench
        .submit_agent(key, command)
        .map_err(|error| anyhow!("original agent submission rejected: {error}"))
}

pub fn poll_original(workbench: &mut Workbench, key: TaskKey) -> Result<AgentSnapshot> {
    workbench
        .poll_agent(key)
        .map_err(|error| anyhow!("original owner poll failed: {error}"))
}

pub fn stop_original(workbench: &mut Workbench, key: TaskKey) -> Result<AgentSnapshot> {
    workbench
        .cancel_agent(key)
        .map_err(|error| anyhow!("original owner stop failed: {error}"))
}

/// This is an explicitly requested same-owner recovery step, not a blind retry
/// of an OS effect or a failed fact CAS. A pending result keeps the owner charged.
pub fn recover_original(workbench: &mut Workbench, key: TaskKey) -> Result<AgentSnapshot> {
    workbench
        .recover_agent(key)
        .map_err(|error| anyhow!("original owner cleanup unresolved: {error}"))
}

pub fn is_unknown(error: AgentError) -> bool {
    error == AgentError::Unknown
}
