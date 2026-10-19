//! Single-process explicit VM workflow. Default CLI never enters this module.
//! Evidence is not authority and cannot deserialize/resume live resources.
use crate::{
    evidence::Journal,
    guest_identity, interaction,
    preflight::{Artifact, Config},
    product::{ProductionBackend, create_checked_backend_materialized},
    protected_owner::ProtectedOwner,
    provisioning::{self, ArtifactSpec, GuestLifecycle, ProvisioningConfig},
};
use anyhow::{Result, anyhow, bail, ensure};
use std::{io::BufRead, sync::Arc};

pub struct LiveConfig {
    pub files: Config,
    pub sid_query: Option<Artifact>,
    pub inventory_shell: Artifact,
    pub expected_guest_uuid: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NormalCleanupAdmissionState {
    owner_opened: bool,
    owner_finished: bool,
    factory_created: bool,
    factory_released: bool,
    backend_present: bool,
    retained_scheduler_present: bool,
    formal_physically_closed: bool,
    formal_unsettled_failure: bool,
    phase_eligible: bool,
    proof_or_guard_held: bool,
}

fn verify_normal_cleanup_admission(state: &NormalCleanupAdmissionState) -> Result<()> {
    ensure!(
        state.phase_eligible,
        "lifecycle is not in an eligible setup/runner/cleanup-prepared phase; preserve Unknown"
    );
    ensure!(
        state.proof_or_guard_held,
        "non-forgeable setup or prepared-cleanup ownership proof absent; STOP cleanup"
    );
    ensure!(
        state.owner_opened && state.owner_finished,
        "keep original owner/native resources until actual finish; original protected owner must have been opened and genuinely finished"
    );
    ensure!(
        state.factory_created
            && state.factory_released
            && !state.backend_present
            && !state.retained_scheduler_present,
        "keep original owner/native resources until actual finish; production factory and its unique scheduler must be genuinely released"
    );
    ensure!(
        state.formal_physically_closed && !state.formal_unsettled_failure,
        "formal workflow has unjoined tasks, undisposed contexts, or unsettled failure; physical closure required before normal cleanup"
    );
    Ok(())
}

// A prior command permanently consumes the probe-only entry, even if it was
// status, unknown, failed, or produced no native resources.
#[derive(Default)]
struct CallerCommandOrder { seen_any: bool, first_probe: bool }
impl CallerCommandOrder {
    fn record(&mut self, command: &str) -> bool {
        self.first_probe = !self.seen_any && command == "caller-prerequisites-once";
        self.seen_any = true;
        self.first_probe
    }
}
#[derive(Clone, Copy)]
struct CallerProbeReturnState { first_probe: bool, pristine: bool, known_closed: bool, blocked: bool }
fn caller_probe_return_allowed(state: CallerProbeReturnState) -> bool {
    state.first_probe && state.pristine && state.known_closed && !state.blocked
}

pub struct Session {
    caller_prerequisites: Box<codex_windows_sandbox::CallerPrerequisiteAttempt>,
    caller_command_order: CallerCommandOrder,
    caller_output: Box<[u8; codex_windows_sandbox::CALLER_PREREQUISITE_OUTPUT_BYTES]>,
    native_factory_retirement:Option<morrow_codex_session_exec_windows_v1::NativeFactoryRetirement>,
    native_factory_completion:Option<morrow_codex_session_exec_windows_v1::NativeFactoryCompletion>,
    normal_native_parts:Option<(provisioning::ExecutionCleanupScope,morrow_codex_session_exec_windows_v1::NativeExecutionCompletion)>,
    lifecycle: GuestLifecycle,
    journal: Journal,
    files: Config,
    sid_query: Option<Artifact>,
    owner: Option<ProtectedOwner>,
    owner_open_attempted: bool,
    backend: Option<ProductionBackend>,
    factory_acquire_attempted: bool,
    factory_created: bool,
    factory_transferred: bool,
    factory_released: bool,
    retained_scheduler: Option<Arc<tokio::runtime::Runtime>>,
    reap_attempts: u8,
    blocked: bool,
    formal: Option<crate::formal::Workflow>,
    formal_physically_closed: bool,
    formal_unsettled_failure: bool,
    cleanup_attempts: u8,
}

fn pin(artifact: &Artifact) -> ArtifactSpec {
    ArtifactSpec {
        path: artifact.path.clone(),
        sha256: artifact.sha256,
    }
}
impl Session {
    pub fn begin(config: LiveConfig, reader: &mut impl BufRead) -> Result<Box<Self>> {
        Self::begin_inner(config,reader,None)
    }
    pub fn begin_asuser(config:LiveConfig,reader:&mut impl BufRead,origin:Arc<codex_windows_sandbox::AsUserParentOrigin>)->Result<Box<Self>> {
        Self::begin_inner(config,reader,Some(origin))
    }
    fn begin_inner(config:LiveConfig,reader:&mut impl BufRead,origin:Option<Arc<codex_windows_sandbox::AsUserParentOrigin>>)->Result<Box<Self>> {
        // Actual read-only C admission BEFORE setup/account/factory effects.
        // Its SAME box remains in the imported owner on failure. The later
        // native-start box is fresh on that actual thread, not a diagnostic
        // replay or an expired observation reinterpreted as a current permit.
        let startup=origin.as_ref().map(|value|value.startup_observation()).unwrap_or_default();
        let validate=startup.enter(codex_windows_sandbox::FrontendStage::ValidateBeforeSetup);
        if let Some(original)=&origin{
            match original.validate_before_setup() {
                Ok(())=>{},
                Err(error)=>{validate.io_error(&error);return Err(error.into());}
            }
        }
        validate.returned();
        let uuid=startup.enter(codex_windows_sandbox::FrontendStage::FirmwareUuidCheck);
        ensure!(
            guest_identity::valid_uuid(&config.expected_guest_uuid),
            "explicit approved guest firmware UUID required"
        );
        uuid.returned();
        let preflight=startup.enter(codex_windows_sandbox::FrontendStage::ConfigPreflight);
        crate::preflight::preflight(&config.files)?;preflight.returned();
        let provisioning_step=startup.enter(codex_windows_sandbox::FrontendStage::ProvisioningConfigBuild);
        let provisioning=ProvisioningConfig {
            synthetic_root: config.files.synthetic_root.clone(),
            harness: pin(&config.files.helper),
            runner: pin(&config.files.runner),
            setup: pin(&config.files.setup),
            inventory_shell: pin(&config.inventory_shell),
        };
        provisioning_step.returned();
        // Outer call boundary only: provisioning internals/authority unchanged.
        let lifecycle_step=startup.enter(codex_windows_sandbox::FrontendStage::LifecyclePreflight);
        let lifecycle=if let Some(origin)=origin {GuestLifecycle::preflight_asuser(provisioning,origin)?}
            else{GuestLifecycle::preflight(provisioning)?};
        lifecycle_step.returned();
        interaction::confirm_first_observed(
            reader,
            guest_identity::CONFIRMATION,
            "Dedicated guest only. Read pinned local OS inventory and create fresh public evidence beside the new synthetic root. This does not authorize setup. Do not enter passwords.",
            &startup,
        )?;
        let mut journal = Journal::create(&config.files.synthetic_root)?;
        journal.before("guest-identity", &serde_json::json!({"expected_uuid":config.expected_guest_uuid,
            "shell":config.inventory_shell.path, "shell_sha256":crate::preflight::hex(&config.inventory_shell.sha256)}))?;
        let observed = guest_identity::observe(
            &config.inventory_shell,
            &config.expected_guest_uuid,
            &mut journal,
        )?;
        journal.record("guest-identity-validated", &observed.identity)?;
        journal.record("lifecycle-initial", &lifecycle.manifest())?;
        Ok(Box::new(Self {
            caller_prerequisites: codex_windows_sandbox::CallerPrerequisiteAttempt::new(),
            caller_command_order: CallerCommandOrder::default(),
            caller_output: Box::new([0; codex_windows_sandbox::CALLER_PREREQUISITE_OUTPUT_BYTES]),
            native_factory_retirement:None,
            native_factory_completion:None,
            normal_native_parts:None,
            lifecycle,
            journal,
            files: config.files,
            sid_query: config.sid_query,
            owner: None,
            owner_open_attempted: false,
            backend: None,
            factory_acquire_attempted: false,
            factory_created: false,
            factory_transferred: false,
            factory_released: false,
            retained_scheduler: None,
            reap_attempts: 0,
            blocked: false,
            formal: None,
            formal_physically_closed: true,
            formal_unsettled_failure: false,
            cleanup_attempts: 0,
        }))
    }
    fn cleanup_admission_state(&self, for_finish_cleanup: bool) -> NormalCleanupAdmissionState {
        let manifest = self.lifecycle.manifest();
        let (phase_eligible, proof_or_guard_held) = if for_finish_cleanup {
            (
                self.lifecycle.phase() == provisioning::Phase::CleanupPrepared,
                manifest.cleanup_guard_held,
            )
        } else {
            (
                matches!(
                    self.lifecycle.phase(),
                    provisioning::Phase::SetupReturnedOk | provisioning::Phase::RunnerMaterialized
                        | provisioning::Phase::ExecutionCleanupAdmitted
                ),
                manifest.owned_setup_proof_held,
            )
        };
        NormalCleanupAdmissionState {
            owner_opened: self.owner_open_attempted && self.owner.is_some(),
            owner_finished: self.owner.as_ref().is_some_and(ProtectedOwner::finished),
            factory_created: self.factory_acquire_attempted && self.factory_created,
            factory_released: self.factory_released,
            backend_present: self.backend.is_some(),
            retained_scheduler_present: self.retained_scheduler.is_some(),
            formal_physically_closed: self.formal_physically_closed,
            formal_unsettled_failure: self.formal_unsettled_failure,
            phase_eligible,
            proof_or_guard_held,
        }
    }
    fn finish_backend(&mut self) -> Result<()> {
        if self.native_factory_retirement.is_some(){return self.finish_native_backend();}
        ensure!(
            self.factory_acquire_attempted && self.factory_created,
            "production factory was not acquired"
        );
        let owner = self
            .owner
            .as_mut()
            .ok_or_else(|| anyhow!("original protected owner absent"))?;
        owner.require_idle()?;
        ensure!(
            self.formal_physically_closed && !self.formal_unsettled_failure,
            "formal workflow has unjoined tasks, undisposed contexts, or unsettled failure; retain factory"
        );
        if let Some(ProductionBackend { backend, scheduler,.. }) = self.backend.take() {
            ensure!(
                self.retained_scheduler.is_none(),
                "one factory scheduler only"
            );
            // The owner has no unacknowledged agent or scheduler debt. Preserve
            // the scheduler while dropping this factory's native/backend alias.
            self.retained_scheduler = Some(scheduler);
            drop(backend);
        }
        let Some(scheduler) = self.retained_scheduler.take() else {
            ensure!(
                self.factory_transferred || self.factory_released,
                "factory scheduler absent without transfer or prior release"
            );
            self.factory_released = true;
            return Ok(());
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            self.retained_scheduler = Some(scheduler);
            bail!("synchronous factory shutdown cannot run inside Tokio");
        }
        match Arc::try_unwrap(scheduler) {
            Ok(runtime) => {
                drop(runtime);
                self.factory_released = true;
                Ok(())
            } // Synchronous shutdown outside Tokio; never shutdown_timeout as proof.
            Err(scheduler) => {
                // This actual Arc remains reachable from the acquiring thread.
                // It is not fabricated into Workbench's scheduler-debt ledger.
                self.retained_scheduler = Some(scheduler);
                bail!("factory scheduler still aliased; original Arc retained for explicit reap");
            }
        }
    }
    fn finish_native_backend(&mut self)->Result<()> {
        ensure!(self.formal_physically_closed && !self.formal_unsettled_failure,
            "same native/formal owner not physically closed");
        let owner=self.owner.as_mut().ok_or_else(||anyhow!("original owner absent"))?;
        if !owner.finished(){owner.require_idle()?;}
        if let Some(ProductionBackend {backend,scheduler,..})=self.backend.take(){
            ensure!(self.retained_scheduler.is_none(),"one original scheduler only");
            self.retained_scheduler=Some(scheduler);drop(backend);
        }
        // Staging aliases is not a release proof. Native thread verification is
        // deferred until this exact protected owner's real finish/flush.
        if !owner.finished(){return Ok(());}
          ensure!(tokio::runtime::Handle::try_current().is_err(),"original scheduler shutdown requires acquiring thread");
          if let Some(scheduler)=self.retained_scheduler.take(){
            match Arc::try_unwrap(scheduler){
                Ok(runtime)=>drop(runtime),
                Err(scheduler)=>{self.retained_scheduler=Some(scheduler);bail!("same original scheduler still aliased; retained");}
            }
        }
        ensure!(self.native_factory_completion.is_none() && !self.factory_released,"native factory completion already consumed");
        let completion=self.native_factory_retirement.as_mut().unwrap().finish_after_shutdown()?;
        self.native_factory_completion=Some(completion);self.factory_released=true;
        Ok(())
    }
    fn caller_pristine(&self) -> bool {
        let manifest = self.lifecycle.manifest();
        self.lifecycle.phase() == provisioning::Phase::PreflightOnly
            && manifest.owned_identities.is_none() && !manifest.owned_setup_proof_held
            && manifest.setup_child_pid.is_none() && manifest.execution_cleanup_admission.is_none()
            && !manifest.cleanup_guard_held && manifest.normal_cleanup_report.is_none()
            && manifest.actual_runner.is_none() && manifest.staged_runner.is_none()
            && manifest.cleanup_notes.is_empty()
            && self.owner.is_none() && !self.owner_open_attempted
            && self.backend.is_none() && !self.factory_acquire_attempted
            && !self.factory_created && !self.factory_transferred && !self.factory_released
            && self.retained_scheduler.is_none() && self.reap_attempts == 0
            && self.formal.is_none() && self.formal_physically_closed
            && !self.formal_unsettled_failure && self.cleanup_attempts == 0
            && self.native_factory_retirement.is_none() && self.native_factory_completion.is_none()
            && self.normal_native_parts.is_none() && !self.blocked
    }
    fn caller_prerequisites_only(&mut self, reader: &mut impl BufRead, first: bool) -> Result<()> {
        ensure!(!self.lifecycle.has_asuser_origin(),"A1 parent C0 requires full original47/9; pure diagnostic cannot consume it");
        let result: Result<()> = (|| {
            if !(first && self.caller_command_order.first_probe && self.caller_pristine()
                && !self.caller_prerequisites.unresolved()) {
                self.caller_prerequisites.retain_admission_failure();
                bail!("caller prerequisite command must be first on the original pristine Session");
            }
            interaction::confirm(reader, "OBSERVE_ORIGINAL_HARNESS_CALLER_PREREQUISITES_ONCE",
                "Read only this original harness caller's finite token facts once. No login, privilege change, child, setup, owner, factory or business cleanup. Unknown retains this same Session.")?;
            ensure!(self.caller_prerequisites.reserve(), "caller prerequisite attempt unavailable; no replay");
            self.journal.before("caller-prerequisites-once", &serde_json::json!({
                "scope":"ORIGINAL_HARNESS_CALLER_PREREQUISITES_ONLY", "deadline_seconds":10,
                "A1_target_token":"NOT_PROVEN", "production_qualified":false}))?;
            let snapshot = self.caller_prerequisites.observe_once();
            let count = snapshot.write_json(&mut self.caller_output[..])
                .map_err(|error| anyhow!(error))?;
            self.journal.raw("caller-prerequisites046.json", &self.caller_output[..count])?;
            ensure!(caller_probe_return_allowed(CallerProbeReturnState {
                first_probe: self.caller_command_order.first_probe,
                pristine: self.caller_pristine(), known_closed: self.caller_prerequisites.known_closed(),
                blocked: self.blocked,
            }), "caller prerequisite observation not known closed; original Session retained; no replay");
            self.journal.record("CALLER_PREREQUISITES_ONLY_NORMAL_RETURN_NOT_BUSINESS_COMPLETION",
                &serde_json::json!({"scope":"ORIGINAL_HARNESS_CALLER_PREREQUISITES_ONLY",
                    "A1_target_token":"NOT_PROVEN", "production_qualified":false,
                    "business_cleanup_performed":false, "SDK26_G04":"OPEN"}))?;
            Ok(())
        })();
        if let Err(error) = &result {
            self.blocked = true;
            let raw = error.downcast_ref::<std::io::Error>().and_then(std::io::Error::raw_os_error);
            self.caller_prerequisites.retain_delivery_failure(raw);
        }
        // Preserve finite original numeric facts in the attempt, never emit an
        // arbitrary OS/journal/exception body through the existing main handler.
        result.map_err(|_| anyhow!("CALLER046_RETAINED_ORIGINAL_SESSION_NO_REPLAY"))
    }
    fn perform(&mut self, command: &str, reader: &mut impl BufRead) -> Result<()> {
        ensure!(!self.caller_prerequisites.unresolved(),
            "caller prerequisite originals retained; no replay or normal cleanup");
        if let Some(step) = command.strip_prefix("sealed-") {
            return self.formal_step(step, reader);
        }
        let (confirmation, description) = match command {
            "inventory" => (
                provisioning::INVENTORY_CONFIRMATION,
                "Read fixed sandbox account/group/registration inventory; STOP on existing state.",
            ),
            "roots" => (
                provisioning::CREATE_CONFIRMATION,
                "Create only this new synthetic VM root; no existing database/key adoption.",
            ),
            "setup" => (
                provisioning::SETUP_CONFIRMATION,
                "ONE ATTEMPT: fixed sandbox accounts, machine DPAPI, ACL/WFP/firewall and security configuration in the exclusively managed disposable guest. No production isolation claim.",
            ),
            "materialize" => (
                provisioning::MATERIALIZE_CONFIRMATION,
                "Pin the real launcher-resolved versioned runner in the owned synthetic home; no legacy/PATH fallback.",
            ),
            "open-owner" => (
                "OPEN_NEW_ORIGINAL_PROTECTED_WORKBENCH_IN_SYNTHETIC_ROOT",
                "Initialize original protected Session/key/SQLite through public Workbench only, under the synthetic root.",
            ),
            "production-factory" => (
                "CREATE_REAL_CHECKED_PRODUCTION_FACTORY_WITHOUT_STARTING_CHILD",
                "Construct real checked backend and its two-worker scheduler; no guest/child is started.",
            ),
            "production-factory-v8" => (
                "CREATE_REAL_EXPLICIT_V8_NATIVE_CLOSE_FACTORY_WITHOUT_STARTING_CHILD",
                "Pin the same materialized image for the explicit v8 handshake/native-close route; no child or cleanup authority is created.",
            ),
            "release-factory" => (
                "RELEASE_IDLE_FACTORY_AND_SYNCHRONOUSLY_JOIN_ITS_UNIQUE_SCHEDULER",
                "Require original owner idle, then release factory and join only an actually unique scheduler. An aliased scheduler stays reachable.",
            ),
            "reap-factory-scheduler" => (
                "REAP_ONLY_THE_SAME_RETAINED_FACTORY_SCHEDULER",
                "Observe the retained scheduler Arc; synchronously shut it down only if uniquely owned. No new execution/CAS or blind control retry.",
            ),
            "finish-owner" => (
                "FINISH_ORIGINAL_OWNER_ONLY_AFTER_REAL_JOINS",
                "Require factory released and original owner/debt status clean before original protected finish.",
            ),
            "prepare-cleanup" => (
                provisioning::PREPARE_CLEANUP_CONFIRMATION,
                "Disable only setup-proved sandbox identities and verify zero live processes or retained logons without PROCESS_TERMINATE.",
            ),
            "finish-cleanup" => (
                provisioning::FINISH_CLEANUP_CONFIRMATION,
                "Retire only setup-proved WFP filters, firewall rules, principals, and sandbox-home artifacts; preserve shared protections and cleanup report.",
            ),
            "sealed-flow" => bail!(
                "DEPENDENT_GATE_OPEN: original sealed proposal wrapper approval/lease and multi-guest Worker bridge are under separate review; no host Propose substitution or ordinary fallback"
            ),
            _ => bail!("unknown command; no effect"),
        };
        ensure!(
            !self.blocked
                || matches!(
                    command,
                    "release-factory"
                        | "reap-factory-scheduler"
                        | "finish-owner"
                        | "prepare-cleanup"
                        | "finish-cleanup"
                ),
            "earlier outcome Unknown; only explicit original cleanup remains allowed"
        );
        if matches!(command, "prepare-cleanup" | "finish-cleanup") {
            verify_normal_cleanup_admission(
                &self.cleanup_admission_state(command == "finish-cleanup"),
            )?;
        }
        interaction::confirm(reader, confirmation, description)?;
        let step = if command == "reap-factory-scheduler" {
            ensure!(
                self.retained_scheduler.is_some() && self.reap_attempts < 4,
                "no retained scheduler or explicit observation budget exhausted"
            );
            self.reap_attempts += 1;
            format!("reap-factory-scheduler-{}", self.reap_attempts)
        } else {
            command.into()
        };
        if let Err(error) = self.journal.before(&step, &self.lifecycle.manifest()) {
            self.blocked = true;
            return Err(error);
        }
        let result = self.effect(command);
        // Fail closed before any fallible post-effect evidence write. If the
        // result cannot be delivered, its attempt remains reserved Unknown.
        if result.is_err() {
            self.blocked = true;
        }
        if let Err(error) = self
            .journal
            .record("lifecycle-after-attempt", &self.lifecycle.manifest())
        {
            self.blocked = true;
            return Err(error);
        }
        match result {
            Ok(()) => {
                if command == "finish-cleanup" {
                    if let Some(report) = self.lifecycle.normal_cleanup_report() {
                        if let Err(error) = self.journal.record("normal-cleanup-retired", report) {
                            self.blocked = true;
                            return Err(error);
                        }
                    }
                }
                if let Err(error) = self
                    .journal
                    .record("STEP_RETURNED_OK_NOT_FULL_ACCEPTANCE", &command)
                {
                    self.blocked = true;
                    return Err(error);
                }
                Ok(())
            }
            Err(error) => {
                self.journal.record(
                    "UNKNOWN_OR_REJECTED_NO_REPLAY",
                    &serde_json::json!({"step":command,"message":error.to_string()}),
                )?;
                Err(error)
            }
        }
    }
    fn effect(&mut self, command: &str) -> Result<()> {
        match command {
            "inventory" => {
                self.lifecycle
                    .inspect_guest(provisioning::INVENTORY_CONFIRMATION)?;
            }
            "roots" => self
                .lifecycle
                .create_roots(provisioning::CREATE_CONFIRMATION)?,
            "setup" => self.lifecycle.setup(provisioning::SETUP_CONFIRMATION)?,
            "materialize" => self
                .lifecycle
                .materialize_checked_runner(provisioning::MATERIALIZE_CONFIRMATION)?,
            "open-owner" => {
                self.lifecycle.runner()?;
                ensure!(
                    !self.owner_open_attempted && self.owner.is_none(),
                    "original owner already exists or was already attempted"
                );
                self.owner_open_attempted = true;
                self.owner = Some(ProtectedOwner::open(self.lifecycle.synthetic_root())?);
                ensure!(
                    self.owner
                        .as_ref()
                        .unwrap()
                        .workbench
                        .maintenance_warning()
                        .is_none(),
                    "original protected maintenance requires recovery; same owner retained"
                );
            }
            "production-factory" | "production-factory-v8" => {
                ensure!(!self.lifecycle.has_asuser_origin()||command=="production-factory-v8",
                    "original A1 inlet cannot fall back to v7/WithLogon factory");
                ensure!(
                    self.owner.is_some()
                        && !self.factory_acquire_attempted
                        && !self.factory_created
                        && self.backend.is_none()
                        && self.retained_scheduler.is_none(),
                    "one original protected owner/factory only"
                );
                let owner = self.owner.as_mut().unwrap();
                ensure!(
                    !owner.finished() && owner.workbench.maintenance_warning().is_none(),
                    "finished/unresolved original protected owner cannot acquire a factory"
                );
                owner.require_idle()?;
                let actual = self.lifecycle.runner()?;
                self.factory_acquire_attempted = true;
                self.backend = Some(if command=="production-factory-v8" {
                    self.native_factory_retirement=Some(morrow_codex_session_exec_windows_v1::NativeFactoryRetirement::new());
                    if self.lifecycle.has_asuser_origin(){
                        crate::product::create_asuser_native_close_backend_materialized(&self.files,&self.lifecycle,
                            self.native_factory_retirement.as_mut().unwrap())?
                    }else{crate::product::create_native_close_backend_materialized(&self.files,actual,
                        self.native_factory_retirement.as_mut().unwrap())?}
                }else{create_checked_backend_materialized(&self.files,actual)?});
                self.factory_created = true;
            }
            "release-factory" | "reap-factory-scheduler" => self.finish_backend()?,
            "finish-owner" => {
                ensure!(
                    self.factory_acquire_attempted
                        && self.factory_created
                        && self.backend.is_none()
                        && self.retained_scheduler.is_none()
                        && self.formal_physically_closed
                        && !self.formal_unsettled_failure,
                    "release factory, settle formal tasks, and clear all real scheduler aliases before protected finish"
                );
                if self.native_factory_retirement.is_some() && self.normal_native_parts.is_none(){
                    self.normal_native_parts=Some(self.formal.as_mut().ok_or_else(||anyhow!("same native formal workflow absent"))?
                        .take_normal_cleanup_parts()?);
                }
                let owner = self
                    .owner
                    .as_mut()
                    .ok_or_else(|| anyhow!("original protected owner absent"))?;
                owner.finish()?;
                if self.native_factory_retirement.is_some(){
                    self.finish_backend()?;
                    let (scope,execution)=self.normal_native_parts.take().ok_or_else(||anyhow!("same sealed native completion absent"))?;
                    let factory=self.native_factory_completion.take().ok_or_else(||anyhow!("real factory native completion absent"))?;
                    let retired=execution.retire_factory(factory)?;
                    let owner=self.owner.as_mut().unwrap().take_finished_receipt()?;
                    let admission=provisioning::BoundExecutionCleanupAdmission::from_original_completion(scope,retired,owner)?;
                    self.lifecycle.admit_execution_cleanup(provisioning::ADMIT_EXECUTION_CLEANUP_CONFIRMATION,admission)?;
                    return Ok(());
                }
                if self.factory_transferred {
                    self.factory_released = true;
                }
                ensure!(
                    self.factory_released,
                    "production factory release not confirmed before protected finish"
                );
            }
            "prepare-cleanup" => self
                .lifecycle
                .prepare_cleanup(provisioning::PREPARE_CLEANUP_CONFIRMATION)?,
            "finish-cleanup" => self
                .lifecycle
                .finish_cleanup(provisioning::FINISH_CLEANUP_CONFIRMATION)?,
            _ => bail!("unsupported step; no effect"),
        }
        Ok(())
    }
    fn formal_step(&mut self, step: &str, reader: &mut impl BufRead) -> Result<()> {
        ensure!(!self.lifecycle.has_asuser_origin()||!matches!(step,"pipe"|"pty"),
            "A1 parent qualification requires the original sealed-basic chain; extensions remain deferred");
        let cleanup = matches!(step, "cleanup" | "repair-cleanup");
        ensure!(
            !self.blocked || cleanup,
            "Unknown step retained; only explicit same-owner cleanup is allowed"
        );
        ensure!(
            self.owner.as_ref().is_some_and(|o| !o.finished()),
            "original protected owner must remain live"
        );
        let mut confirmation = format!(
            "EXECUTE_ONE_ORIGINAL_SEALED_STEP_{}",
            step.to_ascii_uppercase().replace('-', "_")
        );
        if step == "basic" {
            confirmation = "CREATE_FRESH_OWNED_BASIC_FILES_AND_LOOPBACK_LISTENER_WITH_PINNED_GUEST_SID_QUERY".into();
        }
        if step == "native-start" {
            confirmation =
                "START_ONE_REVIEWED_CHECKED_SANDBOX_CHILD_IN_APPROVED_DISPOSABLE_GUEST".into();
        }
        if step == "trusted-approve-claim" {
            confirmation = "TRUSTED_APPROVE_AND_CLAIM_ONLY_THE_ACTUAL_SEALED_GUEST_PROPOSAL".into();
        }
        interaction::confirm(
            reader,
            &confirmation,
            "Guest-only original-owner step. Review its full wrapper/intent evidence first. This confirms one attempt, grants no extra scope, and never authorizes replay. System/Protected/SDK acceptance remains pending.",
        )?;
        let journal_step = if cleanup {
            ensure!(
                self.cleanup_attempts < 4,
                "explicit cleanup observation budget exhausted"
            );
            self.cleanup_attempts += 1;
            format!("sealed-{step}-{}", self.cleanup_attempts)
        } else {
            format!("sealed-{step}")
        };
        if let Err(e) = self.journal.before(
            &journal_step,
            &serde_json::json!({"step":step,"SDK26_G04":"OPEN"}),
        ) {
            self.blocked = true;
            self.formal_unsettled_failure = true;
            self.formal_physically_closed = false;
            return Err(e);
        }
        if !cleanup
            && matches!(
                step,
                "session-context"
                    | "session-worker"
                    | "session-run"
                    | "native-context"
                    | "native-worker"
                    | "propose"
                    | "trusted-review"
                    | "trusted-approve-claim"
                    | "native-start"
                    | "basic-observe"
                    | "sealed-controls"
            )
        {
            self.formal_physically_closed = false;
        }
        let result = (|| -> Result<serde_json::Value> {
            if matches!(step, "basic" | "pipe" | "pty") {
                ensure!(
                    self.formal.is_none() && self.backend.is_some(),
                    "one witness workflow per fresh lifecycle/factory"
                );
                let native_basic=step=="basic" && self.backend.as_ref().is_some_and(ProductionBackend::is_native_close_v8);
                self.formal = Some(if native_basic {
                    crate::formal::Workflow::new_basic_native_v8(&self.lifecycle,&self.files.helper)?
                }else if step == "basic" {
                    crate::formal::Workflow::new_basic(&self.lifecycle, &self.files.helper,
                        self.sid_query.as_ref().ok_or_else(|| anyhow!("guest whoami full path/SHA NOT_READY; no basic effects"))?)?
                } else {
                    crate::formal::Workflow::new(self.lifecycle.synthetic_root(), &self.files.helper, step == "pty")?
                });
                return Ok(
                    serde_json::json!({"witness":step,"authority_created":false,"system_test_executed":false,"fresh_basic_sentinel_files_created":step == "basic", "owned_loopback_listener_baseline":step == "basic",
                        "identity_source":if step!="basic" {None}else if native_basic {Some("current_process_primary_token")}else{Some("fixed_guest_whoami_child")},
                        "guest_sid_query_pin":if native_basic || step!="basic" {None}else{self.sid_query.as_ref().map(|pin|serde_json::json!({"path":pin.path,"sha256":crate::preflight::hex(&pin.sha256)}))},
                        "token_restricted_flags_read":false}),
                );
            }
            let workflow = self
                .formal
                .as_mut()
                .ok_or_else(|| anyhow!("explicit sealed-basic, sealed-pipe or sealed-pty selection required"))?;
            let owner = &mut self.owner.as_mut().unwrap().workbench;
            if cleanup {
                workflow.cleanup(owner, step == "repair-cleanup")
            } else {
                workflow.step(
                    step,
                    owner,
                    &self.lifecycle,
                    &self.files.helper,
                    &mut self.backend,
                )
            }
        })();
        match &result {
            Ok(_) if cleanup => {
                self.formal_unsettled_failure = false;
                self.formal_physically_closed = true;
            }
            Ok(_) => {
                if step == "native-context" {
                    self.factory_transferred = true;
                    self.formal_physically_closed = false;
                } else if step == "session-join"
                    && !self.factory_transferred
                    && !self.formal_unsettled_failure
                {
                    self.formal_physically_closed = true;
                } else if step == "native-join"
                    && !self.formal_unsettled_failure
                    && self
                        .formal
                        .as_ref()
                        .is_some_and(crate::formal::Workflow::finished)
                {
                    self.formal_physically_closed = true;
                }
            }
            Err(_) => {
                self.blocked = true;
                self.formal_unsettled_failure = true;
                self.formal_physically_closed = false;
            }
        }
        match result {
            Ok(evidence) => {
                if let Err(e) = self.journal.record(
                    "ORIGINAL_SEALED_STEP_RETURNED_NOT_FULL_ACCEPTANCE",
                    &evidence,
                ) {
                    self.blocked = true;
                    return Err(e);
                }
                Ok(())
            }
            Err(error) => {
                self.journal
                    .record("SEALED_UNKNOWN_OR_REJECTED_NO_REPLAY", &error.to_string())?;
                Err(error)
            }
        }
    }
    pub fn interact(&mut self, reader: &mut impl BufRead) -> Result<()> {
        println!("Explicit native-close preview: select production-factory-v8 before sealed-basic; its fixed business witness reads its own primary token without whoami or a SID-query image. Use the same existing catalog/claim/start/observe/native-join sequence. After its real native/task ACK, release-factory stages only original aliases; finish-owner performs original finish/flush, verifies original scheduler native shutdown, and consumes the pre-bound admission. Only then prepare-cleanup, finish-cleanup, exit. Every failure remains Unknown/noReplay; ordinary production-factory stays v7.");
        println!(
            "Evidence: {}. Commands: inventory roots setup materialize open-owner production-factory; sealed-basic (fixed noninteractive child; v7 pinned guest whoami or explicit v8 current primary token; fresh owned filesystem sentinels and loopback challenge), or deferred sealed-pipe/sealed-pty, then independent sealed-session/controls/proposal review/install/base-select/base-enable/wrapper-select/approve/wrapper-enable; sealed-session-context/worker/run/join; sealed-native-review/context/worker, sealed-propose, sealed-trusted-review, sealed-trusted-approve-claim, sealed-native-start, sealed-basic-observe (basic) or sealed-controls (deferred), sealed-native-join. Explicit sealed-cleanup/repair-cleanup preserve Unknown. status release-factory reap-factory-scheduler finish-owner prepare-cleanup finish-cleanup exit. No automatic effect/retry.",
            self.journal.directory().display()
        );
        loop {
            let command = interaction::line(reader)?;
            let first_probe = self.caller_command_order.record(&command);
            if command == "caller-prerequisites-once" {
                // This return is scoped observation only, never Finished/cleanup.
                // Any Err goes directly to main's existing retaining Session path.
                return self.caller_prerequisites_only(reader, first_probe);
            }
            if command == "status" {
                self.journal
                    .record("lifecycle-observed", &self.lifecycle.manifest())?;
                if let Some(owner) = &mut self.owner {
                    self.journal
                        .record("original-owner-observed", &owner.status()?)?;
                }
                println!(
                    "{:?}; blocked={}; SDK26/G04 OPEN",
                    self.lifecycle.phase(),
                    self.blocked
                );
            } else if command == "exit" {
                ensure!(!self.lifecycle.has_asuser_origin()
                    ||(self.formal.as_ref().is_some_and(crate::formal::Workflow::finished)&&!self.blocked),
                    "A1 original normal return requires its real joined sealed-basic workflow without an unsettled step");
                ensure!(
                    self.lifecycle.phase() == provisioning::Phase::Finished
                        && self.lifecycle.normal_cleanup_report().is_some()
                        && self.owner_open_attempted
                        && self.owner.as_ref().is_some_and(ProtectedOwner::finished)
                        && self.factory_acquire_attempted
                        && self.factory_created
                        && self.factory_released
                        && self.backend.is_none()
                        && self.retained_scheduler.is_none()
                        && self.formal_physically_closed
                        && !self.formal_unsettled_failure
                        && !self.caller_prerequisites.unresolved(),
                    "unfinished/Unknown resources are held; no automatic cleanup or evidence-based authority restore"
                );
                self.journal.record(
                    "FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE",
                    &serde_json::json!({
                        "sealed_native_observed": self.formal.as_ref().is_some_and(crate::formal::Workflow::finished) && !self.blocked,
                        "prior_business_or_step_blocked": self.blocked,
                        "normal_cleanup_report": self.lifecycle.normal_cleanup_report(),
                        "production_qualification": "NOT_AUTOMATICALLY_GRANTED",
                        "SDK26_G04": "OPEN"
                    }),
                )?;
                self.lifecycle.finish_asuser_origin()?;
                return Ok(());
            } else if let Err(error) = self.perform(&command, reader) {
                eprintln!("{error}; original same-process state retained. No automatic retry.");
            }
        }
    }
    /// Lost interactive input/evidence is not a clean exit. Keep this same
    /// lifecycle and original owner reachable on the acquiring thread; no Drop
    /// cleanup, automatic execution, authority restoration, or new thread.
    /// External termination remains an unresolved outcome, not acceptance.
    pub fn retain_after_disconnect(&mut self, error: &anyhow::Error) -> ! {
        self.blocked = true;
        let _ = self.journal.record(
            "INTERACTIVE_OR_EVIDENCE_LOST_STATE_RETAINED",
            &error.to_string(),
        );
        eprintln!(
            "Interactive/evidence channel ended: {error}. Same lifecycle is held; external termination is unresolved. No automatic cleanup or resume from logs."
        );
        loop {
            std::thread::park();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_admission_state() -> NormalCleanupAdmissionState {
        NormalCleanupAdmissionState {
            owner_opened: true,
            owner_finished: true,
            factory_created: true,
            factory_released: true,
            backend_present: false,
            retained_scheduler_present: false,
            formal_physically_closed: true,
            formal_unsettled_failure: false,
            phase_eligible: true,
            proof_or_guard_held: true,
        }
    }

    #[test]
    fn normal_cleanup_admission_requires_finished_owner_released_factory_and_closed_formal() {
        let base = valid_admission_state();
        assert!(verify_normal_cleanup_admission(&base).is_ok());

        let mut unopened_owner = base;
        unopened_owner.owner_opened = false;
        assert!(verify_normal_cleanup_admission(&unopened_owner).is_err());

        let mut unfinished_owner = base;
        unfinished_owner.owner_finished = false;
        assert!(verify_normal_cleanup_admission(&unfinished_owner).is_err());

        let mut no_factory = base;
        no_factory.factory_created = false;
        assert!(verify_normal_cleanup_admission(&no_factory).is_err());

        let mut unreleased_factory = base;
        unreleased_factory.factory_released = false;
        assert!(verify_normal_cleanup_admission(&unreleased_factory).is_err());

        let mut live_backend = base;
        live_backend.backend_present = true;
        assert!(verify_normal_cleanup_admission(&live_backend).is_err());

        let mut retained_sched = base;
        retained_sched.retained_scheduler_present = true;
        assert!(verify_normal_cleanup_admission(&retained_sched).is_err());

        let mut unclosed_formal = base;
        unclosed_formal.formal_physically_closed = false;
        assert!(verify_normal_cleanup_admission(&unclosed_formal).is_err());

        let mut unsettled_failure = base;
        unsettled_failure.formal_unsettled_failure = true;
        assert!(verify_normal_cleanup_admission(&unsettled_failure).is_err());

        let mut missing_proof = base;
        missing_proof.proof_or_guard_held = false;
        assert!(verify_normal_cleanup_admission(&missing_proof).is_err());

        let mut wrong_phase = base;
        wrong_phase.phase_eligible = false;
        assert!(verify_normal_cleanup_admission(&wrong_phase).is_err());
    }

    #[test]
    fn caller046_first_command_tombstone_never_reopens_after_status_unknown_or_probe() {
        for prior in ["status", "not-a-command", "inventory", "caller-prerequisites-once"] {
            let mut order = CallerCommandOrder::default();
            assert_eq!(order.record(prior), prior == "caller-prerequisites-once");
            assert!(!order.record("caller-prerequisites-once"));
            assert!(!order.record("caller-prerequisites-once"));
            assert!(order.seen_any);
        }
        let mut original = CallerCommandOrder::default();
        assert!(original.record("caller-prerequisites-once"));
        assert!(original.first_probe);
    }

    #[test]
    fn caller046_probe_return_cannot_replace_business_cleanup_or_accept_unknown_debt() {
        let original = CallerProbeReturnState {
            first_probe: true, pristine: true, known_closed: true, blocked: false,
        };
        // Known negative is intentionally allowed once its SAME originals closed.
        assert!(caller_probe_return_allowed(original));
        assert!(!caller_probe_return_allowed(CallerProbeReturnState { first_probe: false, ..original }));
        assert!(!caller_probe_return_allowed(CallerProbeReturnState { pristine: false, ..original }));
        assert!(!caller_probe_return_allowed(CallerProbeReturnState { known_closed: false, ..original }));
        assert!(!caller_probe_return_allowed(CallerProbeReturnState { blocked: true, ..original }));
        // The separate diagnostic return never satisfies normal owner/factory cleanup.
        let business = NormalCleanupAdmissionState {
            owner_opened: false, owner_finished: false, factory_created: false,
            factory_released: false, backend_present: false, retained_scheduler_present: false,
            formal_physically_closed: true, formal_unsettled_failure: false,
            phase_eligible: false, proof_or_guard_held: false,
        };
        assert!(verify_normal_cleanup_admission(&business).is_err());
    }
}

