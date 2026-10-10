//! Original sealed session/proposal/control lanes, one explicit step at a time.
//! No host-created proposal, test sandbox, permission boolean or replay path.
use crate::{
    controls::ControlEvidence,
    formal_cleanup::{WorkbenchAdapter, WorkflowTracker},
    params::{self, Witness},
    preflight::Artifact,
    product::{self, ProductionBackend, RunningExecution},
    provisioning::GuestLifecycle,
    sealed::{GuestClaim, GuestProposal},
    sealed_preparation::{self, PreparedNativeProposal, PreparedSessionGuest},
    wrapper::{ReviewedWrapper, SelectionStep},
};
use anyhow::{Result, anyhow, bail, ensure};
use morrow_agent_process_control_v1::{Capabilities, host::Budget};
use morrow_agent_session_exec_v1_r2::Intent;
use morrow_workbench_host::{
    Workbench,
    agent_tasks::{AgentCommand, AgentLimits, AgentReply, AgentStart},
    io_tasks::{StoragePhase, TaskKey},
};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SESSION_PUBLIC_R2_V1: &[u8] = include_bytes!(
    "../../c28-basic-fixtures/public-session-r2-v1/morrow_codex_session_exec_guest_r2.wasm"
);
const PROPOSAL: &[u8] = include_bytes!(
    "../../c28-basic-fixtures/morrow_codex_proposal_guest_r2.wasm"
);
const PROCESS: &[u8] = include_bytes!(
    "../../c28-basic-fixtures/morrow_codex_process_control_guest_v1.wasm"
);

pub struct Workflow {
    root: PathBuf,
    sid: String,
    operation: String,
    nonce: String,
    tty: bool,
    basic: bool,
    basic_fixture: Option<crate::basic::Fixture>,
    basic_evidence: Option<crate::basic::TerminalEvidence>,
    session_wrapper: Option<ReviewedWrapper>,
    controls_wrapper: Option<ReviewedWrapper>,
    proposal_wrapper: Option<ReviewedWrapper>,
    session_context: Option<PreparedSessionGuest>,
    session_key: Option<TaskKey>,
    generation: Option<u64>,
    session_ran: bool,
    expected_intent: Option<Intent>,
    native_params: Option<morrow_codex_session_exec_windows_v1::ExecParams>,
    native: Option<PreparedNativeProposal>,
    key: Option<TaskKey>,
    proposal: Option<GuestProposal>,
    claim: Option<GuestClaim>,
    running: Option<RunningExecution>,
    evidence: Option<ControlEvidence>,
    joined: bool,
    attempts: std::collections::BTreeSet<String>,
    cleanup_tracker: WorkflowTracker,
}
impl Workflow {
    pub fn new(root: &Path, helper: &Artifact, tty: bool) -> Result<Self> {
        let mut identity = root
            .to_str()
            .ok_or_else(|| anyhow!("synthetic root Unicode"))?
            .as_bytes()
            .to_vec();
        identity.extend_from_slice(&helper.sha256);
        let nonce = crate::preflight::hex(&morrow_agent_session_exec_v1_r2::hash(&identity)[..16]);
        Ok(Self {
            root: root.into(),
            sid: format!("vm-{nonce}"),
            operation: format!("vm-{nonce}-child"),
            nonce,
            tty,
            basic: false,
            basic_fixture: None,
            basic_evidence: None,
            session_wrapper: None,
            controls_wrapper: None,
            proposal_wrapper: None,
            session_context: None,
            session_key: None,
            generation: None,
            session_ran: false,
            expected_intent: None,
            native_params: None,
            native: None,
            key: None,
            proposal: None,
            claim: None,
            running: None,
            evidence: None,
            joined: false,
            attempts: Default::default(),
            cleanup_tracker: Default::default(),
        })
    }
    pub fn new_basic(lifecycle: &GuestLifecycle, helper: &Artifact, query: &Artifact) -> Result<Self> {
        let mut workflow = Self::new(lifecycle.synthetic_root(), helper, false)?;
        workflow.basic_fixture = Some(crate::basic::Fixture::create(lifecycle, &workflow.nonce, query)?);
        workflow.basic = true;
        Ok(workflow)
    }
    fn caps(&self) -> Capabilities {
        Capabilities {
            read: true,
            events: true,
            write: !self.basic,
            close_input: !self.basic && !self.tty,
            resize_pty: !self.basic && self.tty,
            ..Default::default()
        }
    }
    pub fn finished(&self) -> bool {
        self.joined
    }
    /// Explicit same-owner cleanup only. This never repeats a guest, claim,
    /// child start, close, write or resize. Cleanup is not execution acceptance.
    pub fn cleanup(
        &mut self,
        workbench: &mut Workbench,
        recovery: bool,
    ) -> Result<serde_json::Value> {
        ensure!(
            self.key.is_none() || self.session_key.is_none(),
            "multiple known cleanup tasks"
        );
        let known_key = self.key.or(self.session_key);
        // Reject foreign/unknown tasks before disposing or controlling anything.
        self.cleanup_tracker
            .precheck(&mut WorkbenchAdapter(&mut *workbench), known_key)?;
        if let Some(native) = &mut self.native {
            // Remove this caller's review alias before the original Context's
            // unique scheduler shutdown. Actual native slot/job aliases persist.
            drop(native.reviewed.take());
            if native.context.is_some() {
                workbench
                    .dispose_agent_context(&mut native.context)
                    .map_err(|e| anyhow!("original prepared native cleanup retained: {e}"))?;
            }
        }
        if let Some(prepared) = &mut self.session_context {
            if prepared.context.is_some() {
                workbench
                    .dispose_agent_context(&mut prepared.context)
                    .map_err(|e| anyhow!("original session context cleanup retained: {e}"))?;
            }
        }
        let acknowledged = self.cleanup_tracker.settle_task(
            &mut WorkbenchAdapter(&mut *workbench),
            known_key,
            recovery,
            Duration::from_secs(20),
        )?;
        workbench
            .repair_agent_preparation()
            .map_err(|e| anyhow!("original preparation debt remains: {e}"))?;
        if let Some(native) = &self.native {
            ensure!(
                native.resources.is_clean()? && native.resources.pending_facts()?.is_empty(),
                "actual native resource/fact debt remains charged"
            );
        }
        self.session_key = None;
        self.key = None;
        self.cleanup_tracker.complete();
        Ok(serde_json::json!({
            "cleanup_observed":true,"production_acceptance":false,
            "cleanup_task_key":acknowledged.map(|r| crate::preflight::hex(r.key.as_bytes())),
            "current_original_acknowledged":acknowledged.is_some(),
            "explicit_current_recovery":acknowledged.is_some_and(|r| r.explicit_recovery),
            "execution_status":format!("{:?}",acknowledged.map(|r| r.historical_exit)),
        }))
    }
    fn wrapper_path(&self, name: &str) -> PathBuf {
        self.root.join(format!("{name}.mrowasp1"))
    }
    /// A positive confirmation is not authority. Every method below still uses
    /// original catalog/HostRuntime/gate checks and the original same owner.
    pub fn step(
        &mut self,
        command: &str,
        workbench: &mut Workbench,
        lifecycle: &GuestLifecycle,
        helper: &Artifact,
        factory: &mut Option<ProductionBackend>,
    ) -> Result<serde_json::Value> {
        self.cleanup_tracker
            .bind(&WorkbenchAdapter(&mut *workbench))?;
        ensure!(
            self.attempts.len() < 40 && self.attempts.insert(command.into()),
            "sealed step already attempted or budget exhausted; no replay"
        );
        match command {
            "session-review" => {
                ensure!(
                    self.session_wrapper.is_none(),
                    "session wrapper already exists"
                );
                self.session_wrapper = Some(ReviewedWrapper::session_public_r2_v1(
                    workbench,
                    self.wrapper_path("session-public-r2-v1"),
                    "vm.public.session.r2.v1",
                    SESSION_PUBLIC_R2_V1,
                    &self.sid,
                    100,
                )?);
                let mut evidence = self.wrapper_evidence(self.session_wrapper.as_ref().unwrap());
                evidence["session_fixture"] = serde_json::json!({
                    "id":"public-session-r2-v1",
                    "bytes":crate::sealed::PUBLIC_SESSION_R2_V1_BYTES,
                    "module_sha256":crate::sealed::PUBLIC_SESSION_R2_V1_SHA,
                    "historical_identity_reused":false,
                });
                Ok(evidence)
            }
            "session-context" => {
                self.session_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("session wrapper absent"))?
                    .approved_revisions()?;
                ensure!(
                    self.session_context.is_none() && self.session_key.is_none(),
                    "session context already attempted"
                );
                self.session_context = Some(sealed_preparation::prepare_session_guest(
                    workbench, &self.sid,
                )?);
                self.generation = Some(self.session_context.as_ref().unwrap().generation);
                Ok(
                    serde_json::json!({"generation":self.session_context.as_ref().unwrap().generation,"child_started":false}),
                )
            }
            "session-worker" => {
                let wrapper = self
                    .session_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("session wrapper absent"))?;
                let pair = ReviewedWrapper::current_pair(workbench, &[wrapper], 200)?;
                let prepared = self
                    .session_context
                    .as_mut()
                    .ok_or_else(|| anyhow!("session context absent"))?;
                let generation = prepared.generation;
                let key = self.cleanup_tracker.start_once(
                    &mut WorkbenchAdapter(&mut *workbench),
                    AgentStart {
                        package_id: wrapper.id.clone(),
                        full_sha256: wrapper.full_sha256,
                        revisions: pair,
                        lifetime: Duration::from_secs(60),
                        limits: AgentLimits::default(),
                    },
                    &mut prepared.context,
                    &mut self.session_key,
                )?;
                // Keep generation as data only, not a second context/connection.
                self.expected_intent = None;
                Ok(serde_json::json!({"key":format!("{key:?}"),"generation":generation}))
            }
            "session-run" => {
                let key = self
                    .session_key
                    .ok_or_else(|| anyhow!("session worker absent"))?;
                // Same original generation is read from the active context's
                // fixed session package by the caller-captured value below.
                let generation = self.session_generation()?;
                let nonce = crate::preflight::digest(&format!("{}{}", self.nonce, self.nonce))?;
                let receipt = crate::sealed::run_combined_session(
                    workbench,
                    key,
                    generation,
                    nonce[..16].try_into()?,
                    &self.sid,
                )
                .map_err(anyhow::Error::msg)?;
                self.session_ran = true;
                Ok(
                    serde_json::json!({"generation":receipt.generation,"checkpoint":crate::preflight::hex(&receipt.checkpoint_sha256),"tail":receipt.parent_tail,"imports":7}),
                )
            }
            "session-join" => {
                ensure!(self.session_ran, "no verified session completion");
                let key = self
                    .session_key
                    .ok_or_else(|| anyhow!("session worker absent"))?;
                join_original(workbench, key)?;
                workbench
                    .acknowledge_agent(key)
                    .map_err(|e| anyhow!("original session task acknowledge: {e}"))?;
                self.session_key = None;
                Ok(serde_json::json!({"actual_join":true,"session_persisted_in_same_owner":true}))
            }
            "native-review" => {
                ensure!(
                    self.session_ran && self.session_key.is_none(),
                    "actual session join required"
                );
                let backend = factory
                    .as_ref()
                    .ok_or_else(|| anyhow!("production factory absent"))?;
                let params = params::witness_params(
                    lifecycle,
                    helper,
                    &self.operation,
                    &self.nonce,
                    if self.basic {
                        Witness::Basic(&self.basic_fixture.as_ref().ok_or_else(|| anyhow!("basic security fixture absent"))?.security)
                    } else if self.tty {
                        Witness::ConPtySize
                    } else {
                        Witness::PipeEof
                    },
                )?;
                let reviewed = workbench
                    .review_agent_invocation(
                        backend.backend.clone(),
                        self.operation.clone(),
                        params.clone(),
                        helper.sha256,
                        30_000,
                    )
                    .map_err(|e| anyhow!("same-owner native review: {e}"))?;
                let intent = reviewed.intent().clone();
                self.native_params = Some(params);
                self.expected_intent = Some(intent.clone());
                drop(reviewed); // No preview backend alias survives native task cleanup.
                Ok(
                    serde_json::json!({"operation":intent.operation_id,"domain":intent.execution_domain,"intent_sha256":crate::preflight::hex(&intent.digest()?)}),
                )
            }
            "controls-review" => {
                let intent = self
                    .expected_intent
                    .as_ref()
                    .ok_or_else(|| anyhow!("genuine native domain absent"))?;
                self.controls_wrapper = Some(ReviewedWrapper::controls(
                    workbench,
                    self.wrapper_path("controls"),
                    "vm.sealed.controls",
                    PROCESS,
                    &self.sid,
                    &intent.execution_domain,
                    self.caps(),
                    300,
                )?);
                Ok(self.wrapper_evidence(self.controls_wrapper.as_ref().unwrap()))
            }
            "proposal-review" => {
                self.controls_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("controls wrapper absent"))?
                    .approved_revisions()?;
                let intent = self
                    .expected_intent
                    .as_ref()
                    .ok_or_else(|| anyhow!("genuine native domain absent"))?;
                self.proposal_wrapper = Some(ReviewedWrapper::proposal(
                    workbench,
                    self.wrapper_path("proposal"),
                    "vm.sealed.proposal",
                    PROPOSAL,
                    &self.sid,
                    &intent.execution_domain,
                    400,
                )?);
                Ok(self.wrapper_evidence(self.proposal_wrapper.as_ref().unwrap()))
            }
            "native-context" => {
                let controls = self
                    .controls_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("controls wrapper absent"))?;
                let proposal = self
                    .proposal_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("proposal wrapper absent"))?;
                let pair = ReviewedWrapper::current_pair(workbench, &[controls, proposal], 500)?;
                let backend = factory
                    .as_ref()
                    .ok_or_else(|| anyhow!("production factory absent"))?;
                let prepared = sealed_preparation::prepare_native_proposal(
                    workbench,
                    backend.backend.clone(),
                    &self.sid,
                    &self.operation,
                    &format!("vm-{}-input", self.nonce),
                    self.native_params
                        .take()
                        .ok_or_else(|| anyhow!("fixed params absent"))?,
                    helper.sha256,
                    self.expected_intent
                        .as_ref()
                        .ok_or_else(|| anyhow!("fixed intent absent"))?,
                    proposal,
                    pair,
                )?;
                self.native = Some(prepared);
                // The actual prepared Context/port/SchedulerLease now holds this
                // exact runtime. Release only these external strong aliases;
                // its original Worker owns final synchronous shutdown/join.
                let ProductionBackend { backend, scheduler } = factory
                    .take()
                    .ok_or_else(|| anyhow!("factory transfer absent"))?;
                drop(backend);
                drop(scheduler);
                Ok(
                    serde_json::json!({"actual_request_sha256":crate::preflight::hex(&self.native.as_ref().unwrap().actual.digest()),"factory_transferred":true,"child_started":false}),
                )
            }
            "native-worker" => {
                let wrapper = self
                    .controls_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("controls wrapper absent"))?;
                let proposal = self
                    .proposal_wrapper
                    .as_ref()
                    .ok_or_else(|| anyhow!("proposal wrapper absent"))?;
                let pair = ReviewedWrapper::current_pair(workbench, &[wrapper, proposal], 600)?;
                let prepared = self
                    .native
                    .as_mut()
                    .ok_or_else(|| anyhow!("native context absent"))?;
                ensure!(
                    prepared.context.is_some(),
                    "native context already transferred"
                );
                let key = self.cleanup_tracker.start_once(
                    &mut WorkbenchAdapter(&mut *workbench),
                    AgentStart {
                        package_id: wrapper.id.clone(),
                        full_sha256: wrapper.full_sha256,
                        revisions: pair,
                        lifetime: Duration::from_secs(60),
                        limits: AgentLimits::default(),
                    },
                    &mut prepared.context,
                    &mut self.key,
                )?;
                Ok(serde_json::json!({"key":format!("{key:?}"),"child_started":false}))
            }
            "propose" => {
                let prepared = self
                    .native
                    .as_ref()
                    .ok_or_else(|| anyhow!("original prepared request absent"))?;
                self.proposal = Some(
                    GuestProposal::run_once(
                        workbench,
                        self.key()?,
                        &prepared.input,
                        &prepared.actual,
                    )
                    .map_err(anyhow::Error::msg)?,
                );
                Ok(
                    serde_json::json!({"actual_proposal_sha256":crate::preflight::hex(&self.proposal.as_ref().unwrap().proposal_sha256()),"imports":1,"host_propose":false}),
                )
            }
            "trusted-review" => {
                let key = self.key()?;
                self.proposal
                    .as_mut()
                    .ok_or_else(|| anyhow!("actual sealed proposal absent"))?
                    .review_once(workbench, key)
                    .map_err(anyhow::Error::msg)?;
                Ok(serde_json::json!({"original_record_reviewed":true,"approved":false}))
            }
            "trusted-approve-claim" => {
                let key = self.key()?;
                self.claim = Some(
                    self.proposal
                        .as_mut()
                        .ok_or_else(|| anyhow!("actual sealed proposal absent"))?
                        .approve_claim_once(workbench, key)
                        .map_err(anyhow::Error::msg)?,
                );
                Ok(serde_json::json!({"one_shot_claim_received":true,"child_started":false}))
            }
            "native-start" => {
                let key = self.key()?;
                let caps = self.caps();
                let GuestClaim { observation, claim } = self
                    .claim
                    .take()
                    .ok_or_else(|| anyhow!("one-shot claim absent"))?;
                let prepared = self
                    .native
                    .as_mut()
                    .ok_or_else(|| anyhow!("native reviewed invocation absent"))?;
                ensure!(
                    observation.identity.intent_sha256 == prepared.intent.digest()?,
                    "actual claim intent mismatch"
                );
                let reviewed = prepared
                    .reviewed
                    .take()
                    .ok_or_else(|| anyhow!("reviewed invocation already consumed"))?;
                let mut handle = product::submit_once(
                    workbench,
                    key,
                    AgentCommand::StartClaimed {
                        reviewed: Box::new(reviewed),
                        claim,
                        capabilities: caps,
                        budget: Budget::default(),
                    },
                )?;
                let deadline = Instant::now() + Duration::from_secs(35);
                let process = loop {
                    match handle
                        .try_read()
                        .map_err(|e| anyhow!("native start delivery Unknown: {}; diagnostic={handle:?}; native={:?}", native_start_error_name(e), prepared.resources))?
                    {
                        Some(reply) => {
                            let AgentReply::Started(process) = &reply else {
                                bail!("unexpected native start reply; Unknown")
                            };
                            break *process;
                        }
                        None if Instant::now() < deadline => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        None => {
                            handle.cancel();
                            bail!("native start timeout; Unknown; no replay");
                        }
                    }
                };
                self.running = Some(RunningExecution::from_started(
                    key,
                    process,
                    prepared.host.clone(),
                    prepared.intent.clone(),
                    prepared.resources.clone(),
                ));
                Ok(
                    serde_json::json!({"actual_started_handle":format!("{process:?}"),"claim_replay_allowed":false}),
                )
            }
            "basic-observe" => {
                ensure!(self.basic, "basic observation requires sealed-basic selection");
                let run = self.running.as_ref().ok_or_else(|| anyhow!("real child handle absent"))?;
                let fixture = self.basic_fixture.as_ref().ok_or_else(|| anyhow!("owned basic fixture absent"))?;
                let evidence = crate::basic::observe(workbench, run.key, run.handle, &self.nonce, &fixture.security)?;
                fixture.verify(&self.nonce)?;
                let result = serde_json::json!({"basic_only":true,"stdout_sha256":crate::preflight::hex(&morrow_agent_session_exec_v1_r2::hash(&evidence.stdout)),
                    "stderr_sha256":crate::preflight::hex(&morrow_agent_session_exec_v1_r2::hash(&evidence.stderr)),
                    "seq":evidence.after_seq,"exit":evidence.exit_code,"exit_observed":evidence.saw_exit,"EOF_observed":evidence.saw_closed,
                    "workspace_write_observed":true,"outside_write_permission_denied_observed":true,
                    "guest_commands":evidence.guest_commands,"imports":evidence.host_calls,
                    "actual_offline_sid_observed":true,"actual_offline_sid":fixture.security.expected_sid,
                    "sid_query_path":fixture.security.query.path,"sid_query_sha256":crate::preflight::hex(&fixture.security.query.sha256),
                    "token_restricted_flags_read":false,"network_explicit_denial_observed":true,
                    "network_raw_error":10013,"owned_loopback_port":fixture.security.port,
                    "network_positive_baseline_before_and_after":true,"full_network_isolation_qualified":false});
                self.basic_evidence = Some(evidence);
                Ok(result)
            }
            "sealed-controls" => {
                ensure!(!self.basic, "basic never performs input/PTY controls");
                let run = self
                    .running
                    .as_ref()
                    .ok_or_else(|| anyhow!("real child handle absent"))?;
                let evidence = if self.tty {
                    crate::controls::run_resize(workbench, run.key, run.handle, &self.nonce)
                } else {
                    crate::controls::run_pipe_eof(workbench, run.key, run.handle, &self.nonce)
                }
                .map_err(anyhow::Error::msg)?;
                let result = serde_json::json!({"stdout_sha256":crate::preflight::hex(&morrow_agent_session_exec_v1_r2::hash(&evidence.stdout)),
                    "stderr_sha256":crate::preflight::hex(&morrow_agent_session_exec_v1_r2::hash(&evidence.stderr)),
                    "stdout":String::from_utf8_lossy(&evidence.stdout),"stderr":String::from_utf8_lossy(&evidence.stderr),
                    "seq":evidence.after_seq,"exit":evidence.exit_code,"exit_observed":evidence.saw_exit,"EOF_observed":evidence.saw_closed,
                    "correlated_closed_write":evidence.closed_write_rejected,"guest_commands":evidence.guest_commands,"imports":evidence.host_calls});
                self.evidence = Some(evidence);
                Ok(result)
            }
            "native-join" => {
                let run = self
                    .running
                    .as_mut()
                    .ok_or_else(|| anyhow!("actual running child absent"))?;
                let observation = if self.basic {
                    run.stop_and_verify_basic_owner(workbench, self.basic_evidence.as_ref()
                        .ok_or_else(|| anyhow!("actual basic Exit/EOF evidence absent"))?)?
                } else {
                    run.stop_and_verify_returned_owner(workbench, self.evidence.as_ref()
                        .ok_or_else(|| anyhow!("real Exit/EOF control evidence absent"))?)?
                };
                let result = serde_json::json!({"actual_join":true,"facts":format!("{:?}",observation.latest),"original_identity":format!("{:?}",observation.identity),"record_revision":observation.record_revision});
                workbench
                    .acknowledge_agent(run.key)
                    .map_err(|e| anyhow!("original native task acknowledge: {e}"))?;
                self.key = None;
                self.joined = true;
                Ok(result)
            }
            _ => self.selection_step(workbench, command),
        }
    }
    fn session_generation(&self) -> Result<u64> {
        self.generation
            .ok_or_else(|| anyhow!("actual captured session generation absent"))
    }
    fn key(&self) -> Result<TaskKey> {
        self.key
            .ok_or_else(|| anyhow!("original native Worker absent"))
    }
    fn wrapper_evidence(&self, w: &ReviewedWrapper) -> serde_json::Value {
        serde_json::json!({"id":w.id,"full_sha256":crate::preflight::hex(&w.full_sha256),"base_sha256":crate::preflight::hex(&w.base_sha256),"approval":format!("{:?}",w.approval),"granted":false})
    }
    fn selection_step(
        &mut self,
        workbench: &mut Workbench,
        command: &str,
    ) -> Result<serde_json::Value> {
        let (profile, step) = command
            .split_once('-')
            .ok_or_else(|| anyhow!("unknown sealed command"))?;
        let wrapper = match profile {
            "session" => &mut self.session_wrapper,
            "controls" => &mut self.controls_wrapper,
            "proposal" => &mut self.proposal_wrapper,
            _ => bail!("unknown wrapper profile"),
        };
        let step = match step {
            "install" => SelectionStep::Install,
            "base-select" => SelectionStep::BaseSelect,
            "base-enable" => SelectionStep::BaseEnable,
            "wrapper-select" => SelectionStep::WrapperSelect,
            "approve" => SelectionStep::Approve,
            "wrapper-enable" => SelectionStep::WrapperEnable,
            _ => bail!("unknown explicit selection step"),
        };
        let wrapper = wrapper
            .as_mut()
            .ok_or_else(|| anyhow!("explicit wrapper review required"))?;
        wrapper.step(workbench, step)?;
        Ok(
            serde_json::json!({"id":wrapper.id,"full_sha256":crate::preflight::hex(&wrapper.full_sha256),"explicit_step_returned":command}),
        )
    }
}

fn join_original(workbench: &mut Workbench, key: TaskKey) -> Result<()> {
    product::stop_original(workbench, key)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status = product::poll_original(workbench, key)?;
        if status.task.storage == StoragePhase::Reclaimed {
            let exit = status
                .exit
                .ok_or_else(|| anyhow!("actual original task exit missing"))?;
            ensure!(
                exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok(),
                "original VMjoin/disconnect/maintenance unresolved"
            );
            return Ok(());
        }
        ensure!(
            Instant::now() < deadline,
            "actual original owner/join pending; explicit cleanup required"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

// Existing fixed error variants only; never format a backend error or payload.
fn native_start_error_name(error: morrow_workbench_host::agent_tasks::AgentError) -> &'static str {
    use morrow_workbench_host::agent_tasks::AgentError;
    use morrow_agent_session_exec_v1_r2::Error;
    match error {
        AgentError::Invalid=>"Invalid", AgentError::Limit=>"Limit", AgentError::Busy=>"Busy",
        AgentError::Cancelled=>"Cancelled", AgentError::Unknown=>"Unknown", AgentError::Unavailable=>"Unavailable",
        AgentError::Maintenance=>"Maintenance", AgentError::Disconnect=>"Disconnect", AgentError::Unsupported=>"Unsupported",
        AgentError::Session(error)=>match error {
            Error::Invalid=>"Session(Invalid)", Error::Contract=>"Session(Contract)", Error::Limit=>"Session(Limit)",
            Error::Correlation=>"Session(Correlation)", Error::Denied=>"Session(Denied)", Error::Conflict=>"Session(Conflict)",
            Error::NotFound=>"Session(NotFound)", Error::CommitUnknown=>"Session(CommitUnknown)", Error::Storage=>"Session(Storage)",
        },
    }
}
