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
pub struct Session {
    lifecycle: GuestLifecycle,
    journal: Journal,
    files: Config,
    sid_query: Option<Artifact>,
    owner: Option<ProtectedOwner>,
    backend: Option<ProductionBackend>,
    retained_scheduler: Option<Arc<tokio::runtime::Runtime>>,
    reap_attempts: u8,
    blocked: bool,
    formal: Option<crate::formal::Workflow>,
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
        ensure!(
            guest_identity::valid_uuid(&config.expected_guest_uuid),
            "explicit approved guest firmware UUID required"
        );
        crate::preflight::preflight(&config.files)?;
        let lifecycle = GuestLifecycle::preflight(ProvisioningConfig {
            synthetic_root: config.files.synthetic_root.clone(),
            harness: pin(&config.files.helper),
            runner: pin(&config.files.runner),
            setup: pin(&config.files.setup),
            inventory_shell: pin(&config.inventory_shell),
        })?;
        interaction::confirm(
            reader,
            guest_identity::CONFIRMATION,
            "Dedicated guest only. Read pinned local OS inventory and create fresh public evidence beside the new synthetic root. This does not authorize setup. Do not enter passwords.",
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
            lifecycle,
            journal,
            files: config.files,
            sid_query: config.sid_query,
            owner: None,
            backend: None,
            retained_scheduler: None,
            reap_attempts: 0,
            blocked: false,
            formal: None,
            cleanup_attempts: 0,
        }))
    }
    fn finish_backend(&mut self) -> Result<()> {
        if let Some(owner) = &mut self.owner {
            owner.require_idle()?;
        }
        if let Some(ProductionBackend { backend, scheduler }) = self.backend.take() {
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
            return Ok(());
        };
        if tokio::runtime::Handle::try_current().is_ok() {
            self.retained_scheduler = Some(scheduler);
            bail!("synchronous factory shutdown cannot run inside Tokio");
        }
        match Arc::try_unwrap(scheduler) {
            Ok(runtime) => {
                drop(runtime);
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
    fn perform(&mut self, command: &str, reader: &mut impl BufRead) -> Result<()> {
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
                "Disable only recorded sandbox identities and stop their processes using original cleanup guard.",
            ),
            "finish-cleanup" => (
                provisioning::FINISH_CLEANUP_CONFIRMATION,
                "Remove only original guard-owned VM sandbox resources; preserve original cleanup notes.",
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
            ensure!(
                self.backend.is_none()
                    && self.retained_scheduler.is_none()
                    && self.owner.as_ref().is_none_or(ProtectedOwner::finished),
                "keep original owner/native resources until actual finish; no system cleanup can stand in for facts/join"
            );
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
                ensure!(self.owner.is_none(), "original owner already exists");
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
            "production-factory" => {
                ensure!(
                    self.owner.is_some()
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
                self.backend = Some(create_checked_backend_materialized(&self.files, actual)?);
            }
            "release-factory" | "reap-factory-scheduler" => self.finish_backend()?,
            "finish-owner" => {
                ensure!(
                    self.backend.is_none() && self.retained_scheduler.is_none(),
                    "release factory and all real scheduler aliases before protected finish"
                );
                if let Some(owner) = &mut self.owner {
                    owner.finish()?;
                }
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
            return Err(e);
        }
        let result = (|| -> Result<serde_json::Value> {
            if matches!(step, "basic" | "pipe" | "pty") {
                ensure!(
                    self.formal.is_none() && self.backend.is_some(),
                    "one witness workflow per fresh lifecycle/factory"
                );
                self.formal = Some(if step == "basic" {
                    crate::formal::Workflow::new_basic(&self.lifecycle, &self.files.helper,
                        self.sid_query.as_ref().ok_or_else(|| anyhow!("guest whoami full path/SHA NOT_READY; no basic effects"))?)?
                } else {
                    crate::formal::Workflow::new(self.lifecycle.synthetic_root(), &self.files.helper, step == "pty")?
                });
                return Ok(
                    serde_json::json!({"witness":step,"authority_created":false,"system_test_executed":false,"fresh_basic_sentinel_files_created":step == "basic", "owned_loopback_listener_baseline":step == "basic", "guest_sid_query_pin":self.sid_query.as_ref().map(|pin| serde_json::json!({"path":pin.path,"sha256":crate::preflight::hex(&pin.sha256)})), "token_restricted_flags_read":false}),
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
        if result.is_err() {
            self.blocked = true;
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
        println!(
            "Evidence: {}. Commands: inventory roots setup materialize open-owner production-factory; sealed-basic (fixed noninteractive child, pinned guest SID query, fresh owned filesystem sentinels and loopback challenge), or deferred sealed-pipe/sealed-pty, then independent sealed-session/controls/proposal review/install/base-select/base-enable/wrapper-select/approve/wrapper-enable; sealed-session-context/worker/run/join; sealed-native-review/context/worker, sealed-propose, sealed-trusted-review, sealed-trusted-approve-claim, sealed-native-start, sealed-basic-observe (basic) or sealed-controls (deferred), sealed-native-join. Explicit sealed-cleanup/repair-cleanup preserve Unknown. status release-factory reap-factory-scheduler finish-owner prepare-cleanup finish-cleanup exit. No automatic effect/retry.",
            self.journal.directory().display()
        );
        loop {
            let command = interaction::line(reader)?;
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
                ensure!(
                    self.lifecycle.phase() == provisioning::Phase::Finished
                        && self.backend.is_none()
                        && self.retained_scheduler.is_none()
                        && self.owner.as_ref().is_none_or(ProtectedOwner::finished),
                    "unfinished/Unknown resources are held; no automatic cleanup or evidence-based authority restore"
                );
                self.journal.record("FINISHED_EXPLICIT_WORKFLOW_NOT_FULL_ACCEPTANCE", &serde_json::json!({"sealed_native_observed":self.formal.as_ref().is_some_and(crate::formal::Workflow::finished),"production_qualification":"NOT_AUTOMATICALLY_GRANTED","SDK26_G04":"OPEN"}))?;
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
