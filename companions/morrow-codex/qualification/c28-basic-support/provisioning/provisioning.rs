//! Dedicated disposable-VM provisioning only. No action is performed by Drop.
//! The default entry is `preflight`; every mutating phase needs an exact confirmation.
#![cfg(windows)]
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf, Prefix};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail, ensure};
use codex_protocol::models::{ManagedFileSystemPermissions, PermissionProfile};
use codex_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSpecialPath,
    NetworkSandboxPolicy,
};
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_windows_sandbox::{
    MatchedRunnerArtifact, OwnedWindowsSandboxSetup, PreparedOwnedWindowsSandboxCleanup,
    ResolvedWindowsSandboxPermissions, SandboxSetupRequest, WindowsSandboxNormalCleanupReport,
    prepare_owned_windows_sandbox_cleanup, registered_core_requested, resolve_exe_for_launch,
    run_owned_elevated_setup_with_matched_helper, sandbox_bin_dir,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const REPARSE: u32 = 0x400;
const USERS: [&str; 2] = ["CodexSandboxOffline", "CodexSandboxOnline"];
const GROUP: &str = "CodexSandboxUsers";
// Exact version of the frozen derived codex-windows-sandbox library, not this harness.
const RESOLVER_LIBRARY_VERSION: &str = "0.0.0";
pub const CREATE_CONFIRMATION: &str = "CREATE_ONLY_FRESH_SYNTHETIC_VM_ROOT";
pub const INVENTORY_CONFIRMATION: &str = "READ_ONLY_FIXED_SANDBOX_IDENTITY_AND_REGISTRATION_INVENTORY";
pub const SETUP_CONFIRMATION: &str = "APPLY_FIXED_SANDBOX_ACCOUNTS_DPAPI_ACL_WFP_IN_DISPOSABLE_VM";
pub const MATERIALIZE_CONFIRMATION: &str = "MATERIALIZE_AND_PIN_CHECKED_RUNNER_IN_OWN_SYNTHETIC_HOME";
pub const ADMIT_EXECUTION_CLEANUP_CONFIRMATION: &str =
    "ADMIT_BOUND_EXECUTION_OWNER_EOF_JOB_KEEPER_QUOTA_CLOSE_ACK_AND_FACTORY_RELEASE";
pub const PREPARE_CLEANUP_CONFIRMATION: &str = "DISABLE_OWNED_SANDBOX_ACCOUNTS_AND_STOP_THEIR_PROCESSES";
pub const FINISH_CLEANUP_CONFIRMATION: &str = "REMOVE_ONLY_OWNED_SANDBOX_RESOURCES_AND_PROTECTIONS";

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactSpec {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvisioningConfig {
    /// Must be absent, with an existing trusted parent. Nothing beneath it may be real user data.
    pub synthetic_root: PathBuf,
    /// Exact current harness image; lexical current_exe ancestors are pinned independently.
    pub harness: ArtifactSpec,
    /// Original reviewed runner source; it is never mistaken for the launched copy.
    pub runner: ArtifactSpec,
    pub setup: ArtifactSpec,
    /// Absolute, reviewed Windows PowerShell/pwsh PE; used only for the fixed inventory script.
    pub inventory_shell: ArtifactSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub name: String,
    pub sid: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub administrator: bool,
    pub manufacturer: String,
    pub model: String,
    pub hypervisor_present: bool,
    pub users: Vec<Principal>,
    pub groups: Vec<Principal>,
    pub registration_exists: bool,
    pub services: Vec<String>,
    pub firewall_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Phase {
    PreflightOnly,
    InventoryVerified,
    CreatingRoots,
    RootsCreated,
    SetupAttempted,
    SetupReturnedOk,
    MaterializingRunner,
    RunnerMaterialized,
    ExecutionCleanupAdmitted,
    CleanupPreparing,
    CleanupPrepared,
    CleanupFinishing,
    Finished,
    Unknown,
}

/// Serialize-only manifest evidence of verified execution-owner cleanup admission.
/// Intentionally not `Deserialize` and never usable to reconstruct authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExecutionCleanupAdmissionEvidence {
    pub runner_path: PathBuf,
    pub runner_sha256: [u8; 32],
    pub codex_home: PathBuf,
    pub setup_child_pid: u32,
    /// Opaque HostBinding has no numeric export. Never fabricate zero as a session id.
    pub owner_session_id: Option<u64>,
    pub runner_pid: u32,
    pub runner_creation_filetime_100ns: String,
    pub runner_raw_exit_dword: u32,
    pub startup_attempt: u64,
    pub keeper_scope: &'static str,
    pub quota_scope: &'static str,
    pub io_eof_observed: bool,
    pub job_active_processes_zero: bool,
    pub input_closed: bool,
    pub keeper_joined: bool,
    pub quota_settled: bool,
    pub close_ack_verified: bool,
    pub owner_finished: bool,
    pub factory_released: bool,
}

/// Consume-only original SDK/owner/factory bridge, associated before execution
/// with THIS lifecycle/setup and original materialized pin. No decoded record,
/// PID/path/digest, public observation or file-share result can construct it.
pub struct BoundExecutionCleanupAdmission {
    scope:ExecutionCleanupScope,
    completion:morrow_codex_session_exec_windows_v1::RetiredExecutionCompletion,
    owner:crate::protected_owner::FinishedOwnerReceipt,
}
impl std::fmt::Debug for BoundExecutionCleanupAdmission {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str("BoundExecutionCleanupAdmission([SEALED_CONSUME_ONLY])")}
}
/// Private pre-execution association with THIS lifecycle, actual owned setup
/// and original materialized image instance. No public scalar constructor.
pub struct ExecutionCleanupScope {
    lifecycle:Arc<()>,setup:Arc<()>,
    review:morrow_codex_session_exec_windows_v1::ReviewedNativeCompletion,
}
fn same_owned_instances(lifecycle:&Arc<()>,setup:&Arc<()>,actual_lifecycle:&Arc<()>,actual_setup:Option<&Arc<()>>)->bool {
    Arc::ptr_eq(lifecycle,actual_lifecycle) && actual_setup.is_some_and(|actual|Arc::ptr_eq(setup,actual))
}

impl BoundExecutionCleanupAdmission {
    pub(crate) fn from_original_completion(scope:ExecutionCleanupScope,
        completion:morrow_codex_session_exec_windows_v1::RetiredExecutionCompletion,
        owner:crate::protected_owner::FinishedOwnerReceipt)->Result<Self> {
        ensure!(completion.is_for_review(&scope.review) && completion.is_for_owner(owner.binding()),
            "foreign review/owner/factory completion; no cleanup right");
        ensure!(completion.terminal_close_ack_verified(),"actual same-attempt authenticated terminal closeACK missing");
        Ok(Self {scope,completion,owner})
    }
    /// Compatibility rejection only; scalar facts are evidence, never authority.
    /// Kept so callers fail before cleanup effects rather than acquiring a new
    /// unchecked way to assemble an owner/factory capability.
    #[allow(clippy::too_many_arguments)]
    pub fn new_from_bound_owner_facts(
        _runner_path: PathBuf,
        _runner_sha256: [u8; 32],
        _codex_home: PathBuf,
        _setup_child_pid: u32,
        _owner_session_id: u64,
        _runner_pid: u32,
        _io_eof_observed: bool,
        _job_active_processes_zero: bool,
        _input_closed: bool,
        _keeper_joined: bool,
        _quota_settled: bool,
        _close_ack_verified: bool,
        _owner_finished: bool,
        _factory_released: bool,
        _unsettled_debt_or_error: bool,
    ) -> Result<Self> {
        bail!("original same-invocation owner/factory cleanup capability NOT_WIRED; public facts cannot authorize cleanup")
    }
}

/// Evidence only: intentionally not Deserialize and never usable to restore authority.
#[derive(Serialize)]
pub struct LifecycleManifest {
    pub configuration: ProvisioningConfig,
    pub phase: Phase,
    pub owned_identities: Option<Inventory>,
    pub owned_setup_proof_held: bool,
    pub setup_child_pid: Option<u32>,
    pub execution_cleanup_admission: Option<ExecutionCleanupAdmissionEvidence>,
    pub cleanup_guard_held: bool,
    pub normal_cleanup_report: Option<WindowsSandboxNormalCleanupReport>,
    pub inventory_script_sha256: &'static str,
    pub actual_runner: Option<ArtifactSpec>,
    pub cleanup_notes: Vec<String>,
    pub staged_runner: Option<ArtifactSpec>,
    pub resolver_library_version: &'static str,
    pub resolver_source_metadata: RunnerSourceMetadata,
}

/// This object and its cleanup guard must remain on the synchronous acquiring thread.
/// It owns no Core, Store, runtime, or production account credential.
pub struct GuestLifecycle {
    execution_instance:Arc<()>,
    setup_instance:Option<Arc<()>>,
    bound_execution:RefCell<Option<morrow_codex_session_exec_windows_v1::ReviewedNativeCompletion>>,
    config: ProvisioningConfig,
    phase: Phase,
    source_runner: Arc<MatchedRunnerArtifact>,
    actual_runner: Option<Arc<MatchedRunnerArtifact>>,
    actual_runner_identity: Option<ArtifactSpec>,
    _harness: MatchedRunnerArtifact,
    _actual_exe: MatchedRunnerArtifact,
    staged_runner: Option<MatchedRunnerArtifact>,
    staged_runner_identity: Option<ArtifactSpec>,
    resolver_source_metadata: RunnerSourceMetadata,
    setup: MatchedRunnerArtifact,
    setup_child_pid: Option<u32>,
    execution_cleanup_admission: Option<ExecutionCleanupAdmissionEvidence>,
    shell: MatchedRunnerArtifact,
    ancestors: Vec<File>,
    root_handles: Vec<File>,
    materialization_handles: Vec<File>,
    owner_marker: Option<File>,
    owned: Option<Inventory>,
    owned_setup: Option<OwnedWindowsSandboxSetup>,
    cleanup: Option<PreparedOwnedWindowsSandboxCleanup>,
    normal_cleanup_report: Option<WindowsSandboxNormalCleanupReport>,
    cleanup_notes: Vec<String>,
}

// Only bounded guest manufacturer/model/hypervisor facts and public names/SIDs are returned. No registry values, passwords,
// tokens, DPAPI blobs, user homes, or service configuration are read.
const INVENTORY: &str = include_str!("inventory.ps1");

// Bind the existing public review field to the actual embedded UTF-8 bytes,
// including their line endings. This digest is metadata, never cleanup proof.
fn inventory_script_sha256() -> &'static str {
    static SHA256: OnceLock<String> = OnceLock::new();
    SHA256.get_or_init(|| Sha256::digest(INVENTORY.as_bytes()).iter()
        .map(|byte| format!("{byte:02x}")).collect()).as_str()
}

fn inventory(shell: &MatchedRunnerArtifact) -> Result<Inventory> {
    let output = Command::new(shell.path())
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", INVENTORY])
        .output()
        .context("read-only guest inventory process")?;
    ensure!(output.status.success(), "inventory failed; STOP without setup");
    ensure!(output.stdout.len() <= 32_768, "inventory size; STOP");
    let value: Inventory = serde_json::from_slice(&output.stdout).context("strict inventory JSON")?;
    ensure!(value.users.len() <= 2 && value.groups.len() <= 1, "inventory principal bounds");
    ensure!(value.services.len() <= 32 && value.firewall_names.len() <= 5, "inventory resource bounds");
    require_hyperv_guest(&value)?;
    Ok(value)
}

/// Exact public source metadata used by the frozen private dev-name algorithm.
/// This is evidence, not a capability or a persisted resume token.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RunnerSourceMetadata {
    pub length: u64,
    pub modified_seconds: u64,
    pub modified_subsecond_nanos: u32,
}

fn runner_source_metadata(source: &Path) -> Result<RunnerSourceMetadata> {
    let metadata = fs::metadata(source)?;
    ensure!(metadata.is_file() && metadata.file_attributes() & REPARSE == 0,
        "source runner metadata is not a regular file");
    let duration = metadata.modified()?.duration_since(UNIX_EPOCH)
        .context("source runner mtime precedes Unix epoch")?;
    Ok(RunnerSourceMetadata {
        length: metadata.len(), modified_seconds: duration.as_secs(),
        modified_subsecond_nanos: duration.subsec_nanos(),
    })
}

/// Narrow external adapter for pinned helper_materialization.rs:191-213.
/// Only naming is mirrored. Controlled byte-identical copies are validated through the
/// original public materializer; no sandbox/account/IPC implementation is copied.
fn runner_launch_basename(version: &str, source: &RunnerSourceMetadata) -> Result<String> {
    ensure!(!version.is_empty() && version.len() <= 64
        && version.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
        && !version.starts_with('.') && !version.ends_with('.'), "resolver version");
    let suffix = if version == "0.0.0" {
        format!("{}-{:x}", source.length, source.modified_seconds)
    } else {
        version.to_owned()
    };
    Ok(format!("codex-command-runner-{suffix}.exe"))
}

fn require_hyperv_guest(value: &Inventory) -> Result<()> {
    ensure!(value.manufacturer.len() <= 256 && value.model.len() <= 256
        && !value.manufacturer.chars().chain(value.model.chars()).any(char::is_control),
        "guest identity bounds");
    ensure!(value.manufacturer.eq_ignore_ascii_case("Microsoft Corporation")
        && value.model.eq_ignore_ascii_case("Virtual Machine")
        && value.hypervisor_present,
        "dedicated Hyper-V guest required; host HypervisorPresent alone is insufficient");
    Ok(())
}

fn validate_path(path: &Path) -> Result<()> {
    let text = path.to_str().context("path must be UTF-8")?;
    let mut components = path.components();
    ensure!(
        matches!(components.next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_)))
            && matches!(components.next(), Some(Component::RootDir)),
        "only absolute local drive paths are allowed"
    );
    ensure!(components.all(|c| matches!(c, Component::Normal(_))), "path components");
    ensure!(!text.split(['/', '\\']).any(|s| s == "." || s == ".."), "dot segments");
    for part in path.iter().skip(2) {
        let part = part.to_str().context("path component encoding")?;
        ensure!(!part.is_empty() && !part.chars().any(|c| c.is_control() || "<>:\"|?*".contains(c))
            && !part.ends_with(['.', ' ']), "alias or ADS");
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        ensure!(
            !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$")
                && !["COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³"].contains(&stem.as_str())
                && !(stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
            "device path"
        );
    }
    Ok(())
}

fn lock_directory(path: &Path) -> Result<File> {
    let handle = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)?;
    let m = handle.metadata()?;
    ensure!(m.is_dir() && m.file_attributes() & REPARSE == 0, "directory must not be reparse");
    Ok(handle)
}

fn check_expected_launch_path(resolved: &Path, expected: &Path) -> Result<()> {
    ensure!(resolved == expected, "materializer source fallback or wrong full launch path");
    Ok(())
}

fn check_materialized_identity(canonical: &Path, bin: &Path, source: &Path) -> Result<()> {
    ensure!(canonical.parent() == Some(bin) && canonical != source,
        "runner fallback/outside owned .sandbox-bin");
    Ok(())
}

fn check_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(_) => bail!("synthetic root collision; STOP without adoption"),
    }
}

fn no_registration(i: &Inventory) -> Result<()> {
    ensure!(!i.registration_exists && i.services.is_empty(), "registered Core/service collision; STOP");
    Ok(())
}

fn pristine(i: &Inventory) -> Result<()> {
    require_hyperv_guest(i)?;
    no_registration(i)?;
    ensure!(i.users.is_empty() && i.groups.is_empty() && i.firewall_names.is_empty(),
        "existing fixed sandbox identity/protection; STOP without reset or removal");
    Ok(())
}

// The returned opaque proof belongs to this attempt even when a subsequent
// inventory/path check fails. Retention is not cleanup admission or readiness.
// The caller also checks the empty slot before invoking the setup helper.
fn retain_setup_before_postcheck<T, R>(
    retained: &mut Option<T>,
    returned: T,
    postcheck: impl FnOnce(&T) -> Result<R>,
) -> Result<R> {
    ensure!(retained.is_none(), "original setup proof already retained; STOP without replacement");
    let proof = retained.insert(returned);
    postcheck(proof)
}

fn verify_owned_inventory_against_proof(
    current: &Inventory,
    owned: &Inventory,
    proof_offline_user: &str,
    proof_offline_sid: &str,
    proof_online_user: &str,
    proof_online_sid: &str,
    proof_group_name: &str,
    proof_group_sid: &str,
    proof_firewall_rule_names: &[&str],
    proof_wfp_filter_ids: &[u64],
) -> Result<()> {
    no_registration(current)?;
    ensure!(current.administrator, "cleanup requires explicit guest-admin");
    ensure!(
        current.users.len() == owned.users.len()
            && current.groups.len() == owned.groups.len()
            && owned.users.iter().all(|p| current.users.contains(p))
            && owned.groups.iter().all(|p| current.groups.contains(p)),
        "principal replacement; STOP cleanup"
    );
    ensure!(
        current.users.len() == 2 && current.groups.len() == 1,
        "owned principal count mismatch; STOP cleanup"
    );
    ensure!(
        current
            .users
            .iter()
            .any(|p| p.name == proof_offline_user && p.sid == proof_offline_sid)
            && current
                .users
                .iter()
                .any(|p| p.name == proof_online_user && p.sid == proof_online_sid),
        "owned user SID/name mismatch against setup proof; STOP cleanup"
    );
    ensure!(
        current.groups[0].name == proof_group_name && current.groups[0].sid == proof_group_sid,
        "owned group SID/name mismatch against setup proof; STOP cleanup"
    );
    ensure!(
        !proof_firewall_rule_names.is_empty()
            && proof_firewall_rule_names.len() <= 5
            && proof_firewall_rule_names.iter().enumerate().all(|(index, rule)|
                !proof_firewall_rule_names[..index].contains(rule))
            && current.firewall_names.len() == proof_firewall_rule_names.len()
            && proof_firewall_rule_names
                .iter()
                .all(|rule| current.firewall_names.iter().any(|name| name == *rule)),
        "firewall rule inventory mismatch against setup proof; STOP cleanup"
    );
    ensure!(
        !proof_wfp_filter_ids.is_empty() && proof_wfp_filter_ids.iter().all(|&id| id != 0),
        "owned WFP filter ID proof absent or invalid; STOP cleanup"
    );
    Ok(())
}

impl GuestLifecycle {
    pub fn bind_reviewed_execution(&self,review:morrow_codex_session_exec_windows_v1::ReviewedNativeCompletion)->Result<ExecutionCleanupScope> {
        ensure!(self.phase==Phase::RunnerMaterialized && self.owned_setup.is_some(),"real setup/materialized phase required before execution binding");
        let runner=self.actual_runner.as_ref().context("original materialized runner missing")?;
        ensure!(review.is_for_original_runner(runner),"review did not retain THIS original runner pin instance");
        let setup=self.setup_instance.as_ref().context("original setup instance missing")?;
        let mut bound=self.bound_execution.try_borrow_mut().map_err(|_|anyhow::anyhow!("original execution binding borrowed"))?;
        ensure!(bound.is_none(),"one pre-execution association only; no replay or substitution");
        *bound=Some(review.clone());
        Ok(ExecutionCleanupScope {lifecycle:self.execution_instance.clone(),setup:setup.clone(),review})
    }
    /// File-only: no process/shell execution, directory creation, setup, DPAPI, ACL, or WFP.
    pub fn preflight(config: ProvisioningConfig) -> Result<Self> {
        ensure!(!registered_core_requested(),
            "standalone dedicated-VM lifecycle cannot adopt a registered runtime request");
        validate_path(&config.synthetic_root)?;
        ensure!(config.synthetic_root.components().count() >= 4, "synthetic root too broad");
        for artifact in [&config.harness, &config.runner, &config.setup, &config.inventory_shell] {
            validate_path(&artifact.path)?;
        }
        check_absent(&config.synthetic_root)?;
        let exe = std::env::current_exe()?;
        // Canonical equality alone is insufficient: original resolver reuses this lexical path.
        validate_path(&exe)?;
        let actual_exe = MatchedRunnerArtifact::acquire(&exe, config.harness.sha256)?;
        let harness = MatchedRunnerArtifact::acquire(&config.harness.path, config.harness.sha256)?;
        ensure!(harness.path() == actual_exe.path(), "configured harness is not actual current_exe");
        let exe_parent = exe.parent().context("main executable parent")?;
        // This direct sibling is the original resolver's first choice, with no PATH fallback.
        ensure!(dunce::canonicalize(&config.setup.path)? == dunce::canonicalize(exe_parent.join("codex-windows-sandbox-setup.exe"))?,
            "setup must be the exact original-resolver direct sibling");
        let source_runner = Arc::new(MatchedRunnerArtifact::acquire(&config.runner.path, config.runner.sha256)?);
        ensure!(source_runner.path() == dunce::canonicalize(exe_parent.join("codex-command-runner.exe"))?,
            "source runner must be the exact original resolver first sibling, no resource/PATH route");
        let resolver_source_metadata = runner_source_metadata(source_runner.path())?;
        let setup = MatchedRunnerArtifact::acquire(&config.setup.path, config.setup.sha256)?;
        let shell = MatchedRunnerArtifact::acquire(&config.inventory_shell.path, config.inventory_shell.sha256)?;
        let mut parents = config.synthetic_root.ancestors().skip(1).collect::<Vec<_>>();
        parents.reverse();
        let ancestors = parents.into_iter().map(lock_directory).collect::<Result<Vec<_>>>()?;
        Ok(Self { execution_instance:Arc::new(()),setup_instance:None,bound_execution:RefCell::new(None),
            config, phase: Phase::PreflightOnly, source_runner, actual_runner: None,
            actual_runner_identity: None, _harness: harness, _actual_exe: actual_exe, staged_runner: None,
            staged_runner_identity: None, resolver_source_metadata, setup, setup_child_pid: None,
            execution_cleanup_admission: None, shell,
            ancestors, root_handles: Vec::new(), materialization_handles: Vec::new(), owner_marker: None,
            owned: None, owned_setup: None, cleanup: None, cleanup_notes: Vec::new(),
            normal_cleanup_report: None })
    }

    pub fn phase(&self) -> Phase { self.phase }

    /// Caller records this outside the synthetic home after each genuine terminal API result.
    /// An Unknown result must keep this same object/handles reachable; this is not a resume token.
    pub fn manifest(&self) -> LifecycleManifest {
        LifecycleManifest {
            configuration: self.config.clone(), phase: self.phase,
            owned_identities: self.owned.clone(),
            owned_setup_proof_held: self.owned_setup.is_some(),
            setup_child_pid: self.setup_child_pid,
            execution_cleanup_admission: self.execution_cleanup_admission.clone(),
            cleanup_guard_held: self.cleanup.is_some(),
            inventory_script_sha256: inventory_script_sha256(),
            actual_runner: self.actual_runner_identity.clone(), cleanup_notes: self.cleanup_notes.clone(),
            normal_cleanup_report: self.normal_cleanup_report.clone(),
            staged_runner: self.staged_runner_identity.clone(),
            resolver_library_version: RESOLVER_LIBRARY_VERSION,
            resolver_source_metadata: self.resolver_source_metadata.clone(),
        }
    }

    /// Separately authorized read-only process execution; never implicit in default preflight.
    pub fn inspect_guest(&mut self, confirmation: &str) -> Result<Inventory> {
        ensure!(confirmation == INVENTORY_CONFIRMATION && self.phase == Phase::PreflightOnly,
            "inventory phase/confirmation");
        // A failed/indeterminate inventory cannot be retried through this object.
        self.phase = Phase::Unknown;
        let observed = inventory(&self.shell)?;
        pristine(&observed)?;
        self.phase = Phase::InventoryVerified;
        Ok(observed)
    }
    /// Source identity remains distinct from the actual materialized launch image.
    pub fn source_runner(&self) -> Arc<MatchedRunnerArtifact> { Arc::clone(&self.source_runner) }
    pub fn runner(&self) -> Result<Arc<MatchedRunnerArtifact>> {
        ensure!(self.phase == Phase::RunnerMaterialized, "runner is not prepared for execution");
        Ok(Arc::clone(self.actual_runner.as_ref().context("actual runner pin absent")?))
    }
    /// Read-only identity evidence, never approval or a substitute for runner()'s live phase check.
    pub fn actual_runner_identity(&self) -> Option<&ArtifactSpec> {
        self.actual_runner_identity.as_ref()
    }
    pub fn synthetic_root(&self) -> &Path { &self.config.synthetic_root }
    pub fn codex_home(&self) -> PathBuf { self.config.synthetic_root.join("codex-home") }
    pub fn workspace(&self) -> PathBuf { self.config.synthetic_root.join("workspace") }
    pub fn environment(&self) -> HashMap<String, String> {
        let temp = self.workspace().join("temp").to_string_lossy().into_owned();
        HashMap::from([("TEMP".into(), temp.clone()), ("TMP".into(), temp)])
    }
    pub fn permission_profile(&self) -> Result<PermissionProfile> {
        Ok(PermissionProfile::Managed {
            file_system: ManagedFileSystemPermissions::Restricted {
                entries: vec![
                    FileSystemSandboxEntry::new(FileSystemPath::Special { value: FileSystemSpecialPath::Root }, FileSystemAccessMode::Read),
                    FileSystemSandboxEntry::new(AbsolutePathBuf::try_from(self.workspace())?.into(), FileSystemAccessMode::Write),
                ],
                glob_scan_max_depth: None,
            },
            network: NetworkSandboxPolicy::Restricted,
        })
    }

    /// Only creates this fresh synthetic tree. Failure is sticky; no recursive delete or retry.
    pub fn create_roots(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == CREATE_CONFIRMATION && self.phase == Phase::InventoryVerified, "create phase/confirmation");
        self.phase = Phase::Unknown;
        check_absent(&self.config.synthetic_root)?;
        pristine(&inventory(&self.shell)?)?;
        self.phase = Phase::CreatingRoots;
        let result = (|| -> Result<()> {
            fs::create_dir(&self.config.synthetic_root)?;
            self.root_handles.push(lock_directory(&self.config.synthetic_root)?);
            let marker = OpenOptions::new().write(true).create_new(true).share_mode(0)
                .open(self.config.synthetic_root.join("vm-harness-owner.lock"))?;
            self.owner_marker = Some(marker);
            for path in [self.codex_home(), self.workspace(), self.workspace().join("temp")] {
                fs::create_dir(&path)?;
                self.root_handles.push(lock_directory(&path)?);
            }
            Ok(())
        })();
        self.phase = if result.is_ok() { Phase::RootsCreated } else { Phase::Unknown };
        result
    }

    pub fn setup(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == SETUP_CONFIRMATION && self.phase == Phase::RootsCreated, "setup phase/confirmation");
        ensure!(self.owned_setup.is_none() && self.setup_instance.is_none() && self.setup_child_pid.is_none(),
            "original setup attempt already retained; STOP without replay");
        self.phase = Phase::Unknown;
        let before = inventory(&self.shell)?;
        pristine(&before)?;
        ensure!(before.administrator, "explicit elevated guest-admin process required; no UAC fallback");
        let workspace_abs = AbsolutePathBuf::from_absolute_path(self.workspace())?;
        let permissions =
            ResolvedWindowsSandboxPermissions::try_from_permission_profile_for_workspace_roots(
                &self.permission_profile()?,
                std::slice::from_ref(&workspace_abs),
            )?;
        permissions.validate_elevated_filesystem_policy(&self.workspace())?;
        self.phase = Phase::SetupAttempted; // Latch before the first original system effect.
        let result = run_owned_elevated_setup_with_matched_helper(
            SandboxSetupRequest {
                permissions: &permissions,
                command_cwd: &self.workspace(),
                env_map: &self.environment(),
                codex_home: &self.codex_home(),
                proxy_enforced: false,
            },
            &self.setup,
        )
        .and_then(|owned_setup| {
            // Capture only the actual returned proof's identity. These observations
            // remain available after an Err, without granting cleanup or retry.
            self.setup_child_pid = Some(owned_setup.setup_child_pid());
            self.setup_instance = Some(Arc::new(()));
            let workspace = self.workspace();
            let codex_home = self.codex_home();
            let setup = &self.setup;
            let shell = &self.shell;
            let after = retain_setup_before_postcheck(&mut self.owned_setup, owned_setup, |owned_setup| {
                ensure!(
                    owned_setup.setup_exe_path() == setup.path()
                        && owned_setup.setup_exe_sha256() == setup.sha256()
                        && owned_setup.setup_child_pid() != 0,
                    "owned setup proof is not bound to the pinned codex-windows-sandbox-setup.exe process"
                );
                let canonical_workspace = dunce::canonicalize(&workspace)?;
                let canonical_bin = dunce::canonicalize(sandbox_bin_dir(&codex_home))?;
                ensure!(
                    owned_setup.command_cwd() == workspace
                        && owned_setup.write_roots().contains(&canonical_workspace)
                        && owned_setup.read_roots().contains(&canonical_bin),
                    "owned setup proof did not apply requested workspace write and helper read policy roots"
                );
                let after = inventory(shell)?;
                no_registration(&after)?;
                ensure!(after.users.len() == 2 && after.groups.len() == 1, "setup identity evidence incomplete");
                for name in USERS { ensure!(after.users.iter().any(|p| p.name == name && !p.sid.is_empty()), "setup user evidence"); }
                ensure!(after.groups[0].name == GROUP && !after.groups[0].sid.is_empty(), "setup group evidence");
                let rule_names = owned_setup.firewall_rule_names();
                let filter_ids = owned_setup.wfp_filter_ids();
                verify_owned_inventory_against_proof(
                    &after,
                    &after,
                    owned_setup.offline_username(),
                    owned_setup.offline_sid(),
                    owned_setup.online_username(),
                    owned_setup.online_sid(),
                    owned_setup.group_name(),
                    owned_setup.group_sid(),
                    &rule_names,
                    &filter_ids,
                )?;
                Ok(after)
            })?;
            self.owned = Some(after);
            Ok(())
        });
        self.phase = if result.is_ok() { Phase::SetupReturnedOk } else { Phase::Unknown };
        // Ok means the pinned setup.exe process returned Ok with verified non-forgeable ownership proof.
        result
    }

    /// Explicit file-copy effect after original setup. A fallback source path is rejected.
    /// No PE is executed here. Unknown never permits another materialization attempt.
    pub fn materialize_checked_runner(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == MATERIALIZE_CONFIRMATION && self.phase == Phase::SetupReturnedOk,
            "materialize phase/confirmation");
        self.phase = Phase::Unknown;
        self.validate_owned()?;
        self.phase = Phase::MaterializingRunner;
        let result = (|| -> Result<()> {
            ensure!(!registered_core_requested(), "registered runtime route changed");
            ensure!(runner_source_metadata(self.source_runner.path())? == self.resolver_source_metadata,
                "original resolver source metadata changed; no new layout or replay");
            let basename = runner_launch_basename(RESOLVER_LIBRARY_VERSION, &self.resolver_source_metadata)?;
            let bin = sandbox_bin_dir(&self.codex_home());
            validate_path(&bin)?;
            self.materialization_handles.push(lock_directory(&bin)?);
            let destination = bin.join(&basename);
            check_absent(&destination)?; // Never overwrite/adopt an already materialized runner.
            let staging = self.synthetic_root().join("runner-staging");
            validate_path(&staging)?;
            check_absent(&staging)?;
            fs::create_dir(&staging)?;
            self.root_handles.push(lock_directory(&staging)?);
            let staged = staging.join(&basename);
            let source_modified = fs::metadata(self.source_runner.path())?.modified()?;
            {
                let mut source = OpenOptions::new().read(true).share_mode(1)
                    .custom_flags(0x00200000).open(self.source_runner.path())?;
                let mut target = OpenOptions::new().write(true).create_new(true).share_mode(0)
                    .custom_flags(0x00200000).open(&staged)?;
                let copied = std::io::copy(&mut source, &mut target)?;
                ensure!(copied == self.resolver_source_metadata.length, "staging copy length");
                target.sync_all()?;
                // Preserve original metadata so the original freshness check will reuse, not recopy.
                target.set_times(FileTimes::new().set_modified(source_modified))?;
                target.sync_all()?;
            }
            let staged_pin = MatchedRunnerArtifact::acquire(&staged, self.config.runner.sha256)?;
            ensure!(runner_source_metadata(staged_pin.path())? == self.resolver_source_metadata,
                "staged metadata differs from exact original source");
            self.staged_runner_identity = Some(ArtifactSpec {
                path: staged_pin.path().to_path_buf(), sha256: staged_pin.sha256(),
            });
            self.staged_runner = Some(staged_pin);
            // Reserve the final name atomically too. No existing destination can be adopted
            // or removed by the original materializer's freshness/remove race.
            {
                let mut input = OpenOptions::new().read(true).share_mode(1)
                    .custom_flags(0x00200000).open(
                        self.staged_runner.as_ref().context("staged runner pin absent")?.path(),
                    )?;
                let mut target = OpenOptions::new().write(true).create_new(true).share_mode(0)
                    .custom_flags(0x00200000).open(&destination)?;
                let copied = std::io::copy(&mut input, &mut target)?;
                ensure!(copied == self.resolver_source_metadata.length, "launch copy length");
                target.sync_all()?;
                target.set_times(FileTimes::new().set_modified(source_modified))?;
                target.sync_all()?;
            }
            let canonical = dunce::canonicalize(&destination)?;
            let expected_bin = dunce::canonicalize(&bin)?;
            check_materialized_identity(&canonical, &expected_bin, self.source_runner.path())?;
            ensure!(canonical == expected_bin.join(&basename), "wrong canonical full launch path");
            let artifact = Arc::new(MatchedRunnerArtifact::acquire(&destination, self.config.runner.sha256)?);
            ensure!(artifact.path() == canonical
                && runner_source_metadata(artifact.path())? == self.resolver_source_metadata
                && runner_source_metadata(self.source_runner.path())? == self.resolver_source_metadata,
                "materialized SHA/identity/metadata changed");
            self.actual_runner_identity = Some(ArtifactSpec {
                path: artifact.path().to_path_buf(), sha256: artifact.sha256(),
            });
            self.actual_runner = Some(artifact);
            // With destination already exact, fresh and locked, the original public materializer
            // must use Reused. Any attempt to replace it fails, and source fallback is rejected.
            let resolved = resolve_exe_for_launch(
                self.staged_runner.as_ref().context("staged runner pin absent")?.path(),
                &self.codex_home(),
            );
            validate_path(&resolved)?;
            check_expected_launch_path(&resolved, &destination)?;
            ensure!(dunce::canonicalize(&resolved)? == canonical
                && runner_source_metadata(&resolved)? == self.resolver_source_metadata
                && runner_source_metadata(self.source_runner.path())? == self.resolver_source_metadata,
                "public materializer full-path/freshness mismatch");
            // Source, staging, directory and actual launch artifact remain held through execution.
            Ok(())
        })();
        self.phase = if result.is_ok() { Phase::RunnerMaterialized } else { Phase::Unknown };
        result
    }

    fn validate_owned(&self) -> Result<()> {
        let current = inventory(&self.shell)?;
        let owned = self.owned.as_ref().context("no exact successful-setup ownership")?;
        match (self.owned_setup.as_ref(), self.cleanup.as_ref()) {
            (Some(setup), None) => {
                let rule_names = setup.firewall_rule_names();
                let filter_ids = setup.wfp_filter_ids();
                verify_owned_inventory_against_proof(
                    &current,
                    owned,
                    setup.offline_username(),
                    setup.offline_sid(),
                    setup.online_username(),
                    setup.online_sid(),
                    setup.group_name(),
                    setup.group_sid(),
                    &rule_names,
                    &filter_ids,
                )
            }
            (None, Some(prepared)) => {
                let rule_names = prepared.firewall_rule_names();
                let filter_ids = prepared.wfp_filter_ids();
                verify_owned_inventory_against_proof(
                    &current,
                    owned,
                    prepared.offline_username(),
                    prepared.offline_sid(),
                    prepared.online_username(),
                    prepared.online_sid(),
                    prepared.group_name(),
                    prepared.group_sid(),
                    &rule_names,
                    &filter_ids,
                )
            }
            _ => bail!("missing non-forgeable setup ownership proof; STOP cleanup"),
        }
    }

    /// Supplementary local `Arc` / `FILE_SHARE_DELETE` check on the materialized runner pin.
    /// This check is never sufficient proof of execution-owner completion; when a runner is
    /// materialized, `admit_execution_cleanup` must first supply bound execution-owner facts.
    fn verify_and_repinnable_runner_released(&mut self, reacquire: bool) -> Result<()> {
        if let Some(pin) = self.actual_runner.take() {
            match Arc::try_unwrap(pin) {
                Ok(artifact) => drop(artifact),
                Err(pin) => {
                    self.actual_runner = Some(pin);
                    bail!("actual runner still borrowed; preserve Unknown and all cleanup state");
                }
            }
        }
        if let Some(spec) = self.actual_runner_identity.as_ref() {
            let probe = OpenOptions::new()
                .access_mode(0x0001_0000 | 0x0080)
                .share_mode(1 | 2 | 4)
                .custom_flags(0x0020_0000)
                .open(&spec.path);
            if reacquire {
                if let Ok(reacquired) = MatchedRunnerArtifact::acquire(&spec.path, spec.sha256) {
                    self.actual_runner = Some(Arc::new(reacquired));
                }
            }
            probe.context(
                "actual runner still held by active backend or process without FILE_SHARE_DELETE; preserve Unknown and all cleanup state",
            )?;
            if reacquire && self.actual_runner.is_none() {
                bail!("failed to re-pin materialized runner after share check; preserve Unknown");
            }
        }
        Ok(())
    }

    /// Admit only the private pre-bound relation and consumed actual runtime,
    /// protected-owner finish and original factory completion. Public scalar
    /// compatibility calls and serialize-only evidence remain non-authority.
    pub fn admit_execution_cleanup(
        &mut self,
        confirmation: &str,
        admission: BoundExecutionCleanupAdmission,
    ) -> Result<()> {
        ensure!(confirmation==ADMIT_EXECUTION_CLEANUP_CONFIRMATION && self.phase==Phase::RunnerMaterialized
            && self.execution_cleanup_admission.is_none(),"normal cleanup admission phase/one-shot confirmation");
        ensure!(same_owned_instances(&admission.scope.lifecycle,&admission.scope.setup,
                &self.execution_instance,self.setup_instance.as_ref())
            && self.owned_setup.is_some(),"foreign lifecycle or actual owned setup instance");
        let bound=self.bound_execution.try_borrow().map_err(|_|anyhow::anyhow!("original execution association borrowed"))?;
        ensure!(bound.as_ref().is_some_and(|review|review.same_review(&admission.scope.review))
            && admission.completion.is_for_review(&admission.scope.review)
            && admission.completion.is_for_owner(admission.owner.binding()),"foreign or incomplete execution/owner/factory capability");
        let runner=self.actual_runner_identity.as_ref().context("original materialized runner absent")?;
        // All true evidence below is derived only after consuming the sealed
        // runtime/owner/factory capabilities. It cannot reconstruct admission.
        let evidence=ExecutionCleanupAdmissionEvidence {
            runner_path:runner.path.clone(),runner_sha256:runner.sha256,codex_home:self.codex_home(),
            setup_child_pid:self.setup_child_pid.context("original setup instance absent")?,
            owner_session_id:None,
            runner_pid:admission.completion.runner_pid(),io_eof_observed:true,job_active_processes_zero:true,
            runner_creation_filetime_100ns:admission.completion.runner_creation_filetime().to_string(),
            runner_raw_exit_dword:admission.completion.runner_raw_dword(),
            startup_attempt:admission.completion.startup_attempt(),
            keeper_scope:"ORIGINAL_PIPE_FORWARDERS_AND_INVOCATION_OWNED_DESKTOP",
            quota_scope:"SAME_NATIVE_EXECUTION_REGISTRY_SLOT_ONLY",
            input_closed:true,keeper_joined:true,quota_settled:true,
            close_ack_verified:admission.completion.terminal_close_ack_verified(),
            owner_finished:true,factory_released:true,
        };
        drop(bound);drop(admission);
        self.execution_cleanup_admission=Some(evidence);self.phase=Phase::ExecutionCleanupAdmitted;
        Ok(())
    }

    /// Scoped non-forceful normal cleanup preparation consuming the non-forgeable setup proof.
    /// Never requests `PROCESS_TERMINATE` or calls `TerminateProcess`.
    /// When a runner was materialized, requires `Phase::ExecutionCleanupAdmitted` rather than
    /// accepting `Phase::RunnerMaterialized` on file-share probes alone.
    pub fn prepare_cleanup(&mut self, confirmation: &str) -> Result<()> {
        let initial_phase = self.phase;
        self.phase = Phase::Unknown;
        ensure!(
            confirmation == PREPARE_CLEANUP_CONFIRMATION,
            "prepare cleanup confirmation"
        );
        validate_prepare_cleanup_phase_and_admission(
            initial_phase,
            self.actual_runner_identity.is_some() || self.staged_runner_identity.is_some(),
            self.execution_cleanup_admission.is_some(),
        )?;
        self.validate_owned()?;
        self.verify_and_repinnable_runner_released(true)?;
        let owned_setup = self
            .owned_setup
            .take()
            .context("missing non-forgeable OwnedWindowsSandboxSetup proof; STOP cleanup")?;
        self.phase = Phase::CleanupPreparing;
        match prepare_owned_windows_sandbox_cleanup(owned_setup) {
            Ok(guard) => {
                self.cleanup = Some(guard);
                self.phase = Phase::CleanupPrepared;
                Ok(())
            }
            Err(error) => {
                self.phase = Phase::Unknown;
                Err(error)
            }
        }
    }

    /// No arbitrary deletion callback: the synthetic workspace is retained for evidence.
    /// If a runner was materialized, requires that `admit_execution_cleanup` was verified prior to `prepare_cleanup`.
    pub fn finish_cleanup(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == FINISH_CLEANUP_CONFIRMATION && self.phase == Phase::CleanupPrepared, "finish cleanup phase/confirmation");
        self.phase = Phase::Unknown;
        if self.actual_runner_identity.is_some() || self.staged_runner_identity.is_some() {
            ensure!(
                self.execution_cleanup_admission.is_some(),
                "finish_cleanup requires bound execution cleanup admission when a runner was materialized"
            );
        }
        self.validate_owned()?;
        self.verify_and_repinnable_runner_released(false)?;
        self.materialization_handles.clear();
        let cleanup = self
            .cleanup
            .take()
            .context("missing prepared owned cleanup guard")?;
        self.phase = Phase::CleanupFinishing;
        let notes = RefCell::new(Vec::new());
        let result = cleanup.finish(|note| notes.borrow_mut().push(note.to_owned()));
        self.cleanup_notes.extend(notes.into_inner());
        match result {
            Ok(report) => {
                self.normal_cleanup_report = Some(report);
                self.phase = Phase::Finished;
                Ok(())
            }
            Err(error) => {
                self.phase = Phase::Unknown;
                Err(error)
            }
        }
    }

    /// Public SID-only ownership facts, never passwords or DPAPI data.
    pub fn owned_identities(&self) -> Option<&Inventory> { self.owned.as_ref() }
    pub fn normal_cleanup_report(&self) -> Option<&WindowsSandboxNormalCleanupReport> {
        self.normal_cleanup_report.as_ref()
    }
    pub fn held_ancestor_count(&self) -> usize { self.ancestors.len() }
}

fn validate_prepare_cleanup_phase_and_admission(
    phase: Phase,
    has_materialized_runner: bool,
    has_execution_admission: bool,
) -> Result<()> {
    match phase {
        Phase::SetupReturnedOk => {
            ensure!(
                !has_materialized_runner,
                "materialized runner cannot bypass bound execution cleanup admission via SetupReturnedOk"
            );
            Ok(())
        }
        Phase::ExecutionCleanupAdmitted => {
            ensure!(
                has_materialized_runner && has_execution_admission,
                "ExecutionCleanupAdmitted requires both materialized runner and bound execution cleanup admission"
            );
            Ok(())
        }
        Phase::RunnerMaterialized => {
            bail!(
                "materialized runner requires admit_execution_cleanup with bound owner/IO/Job/input/keeper/quota/closeACK and factory release facts before prepare_cleanup; file-share probes alone are not proof"
            )
        }
        _ => bail!("prepare cleanup phase/confirmation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_inventory_sha256_binds_exact_embedded033_bytes_without_stale_metadata() {
        assert_eq!(inventory_script_sha256(),
            "d7660d924acf23e9b1dcc1f4af33f25284070930133c87f1d00b12d7c137284f");
        assert_ne!(inventory_script_sha256(),
            "cbc6acc098360610aa00c9de5878293a39ea269d7c406709061cc5a020f1483d");
    }

    #[test]
    fn postcheck_failure_retains_same_returned_object_and_original_error_without_cleanup_admission() {
        // Tests the production retention combinator, not a fabricated native
        // proof. An Arc identity cannot authorize any setup/cleanup operation.
        let original = Arc::new(());
        let mut retained = None;
        let result: Result<()> = retain_setup_before_postcheck(
            &mut retained,
            Arc::clone(&original),
            |proof| {
                assert!(Arc::ptr_eq(proof, &original));
                Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                    "original postcheck mismatch").into())
            },
        );
        let error = result.unwrap_err();
        let original_error = error.downcast_ref::<std::io::Error>().unwrap();
        assert_eq!(original_error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(original_error.to_string(), "original postcheck mismatch");
        assert!(Arc::ptr_eq(retained.as_ref().unwrap(), &original));
        assert_eq!(Arc::strong_count(&original), 2);
        // A retained resource does not make a failed attempt cleanup-ready,
        // even if all public admission-presence flags are claimed.
        for materialized in [false, true] {
            for admitted in [false, true] {
                assert!(validate_prepare_cleanup_phase_and_admission(
                    Phase::Unknown, materialized, admitted).is_err());
            }
        }
    }

    #[test]
    fn postcheck_success_keeps_same_object_and_replacement_does_not_run_postcheck() {
        let original = Arc::new(());
        let mut retained = None;
        let value = retain_setup_before_postcheck(&mut retained, Arc::clone(&original), |proof| {
            assert!(Arc::ptr_eq(proof, &original));
            Ok(17)
        }).unwrap();
        assert_eq!(value, 17);
        assert!(Arc::ptr_eq(retained.as_ref().unwrap(), &original));
        let replacement = Arc::new(());
        let called = std::cell::Cell::new(false);
        let result = retain_setup_before_postcheck(&mut retained, replacement, |_| {
            called.set(true);
            Ok(())
        });
        assert!(result.is_err());
        assert!(!called.get());
        assert!(Arc::ptr_eq(retained.as_ref().unwrap(), &original));
    }

    #[test]
    fn fixed_com_rule_inventory_requires_exact_complete_unique_proof_set() {
        let rules = [
            "codex_sandbox_offline_block_outbound",
            "codex_sandbox_offline_block_inbound",
            "codex_sandbox_offline_block_loopback_tcp",
            "codex_sandbox_offline_block_loopback_udp",
        ];
        let mut owned = owned_inventory();
        owned.firewall_names = rules.iter().map(|name| (*name).to_owned()).collect();
        let verify = |current: &Inventory, proof: &[&str]| {
            verify_owned_inventory_against_proof(current, &owned,
                USERS[0], &owned.users[0].sid, USERS[1], &owned.users[1].sid,
                GROUP, &owned.groups[0].sid, proof, &[42])
        };
        assert!(verify(&owned, &rules).is_ok());
        let mut reordered = owned.clone();
        reordered.firewall_names.reverse();
        assert!(verify(&reordered, &rules).is_ok());
        let mut missing = owned.clone();
        missing.firewall_names.pop();
        assert!(verify(&missing, &rules).is_err());
        let mut extra = owned.clone();
        extra.firewall_names.push("codex_sandbox_offline_allow_loopback_proxy".into());
        assert!(verify(&extra, &rules).is_err());
        for replacement in [rules[1], "{3B319DFE-9CDB-46D6-94ED-321F53AD8CFB}",
            "Codex_Sandbox_Offline_Block_Outbound", "codex_sandbox_offline_block_outbound_suffix"] {
            let mut replaced = owned.clone();
            replaced.firewall_names[0] = replacement.into();
            assert!(verify(&replaced, &rules).is_err());
        }
        let duplicate_proof = [rules[0], rules[0], rules[2], rules[3]];
        let mut duplicate_inventory = owned.clone();
        duplicate_inventory.firewall_names[1] = rules[0].into();
        assert!(verify(&duplicate_inventory, &duplicate_proof).is_err());
        let mut absent = owned.clone();
        absent.firewall_names.clear();
        assert!(verify(&absent, &[]).is_err());
    }

    #[test]
    fn cleanup_instance_relation_rejects_foreign_lifecycle_setup_and_missing_setup_without_authority() {
        // Instance relation only: no setup/process/native capability is minted.
        let lifecycle=Arc::new(());let setup=Arc::new(());
        assert!(same_owned_instances(&lifecycle,&setup,&lifecycle,Some(&setup)));
        assert!(same_owned_instances(&lifecycle.clone(),&setup.clone(),&lifecycle,Some(&setup)));
        assert!(!same_owned_instances(&Arc::new(()),&setup,&lifecycle,Some(&setup)));
        assert!(!same_owned_instances(&lifecycle,&Arc::new(()),&lifecycle,Some(&setup)));
        assert!(!same_owned_instances(&lifecycle,&setup,&lifecycle,None));
    }

    fn empty() -> Inventory { Inventory { administrator: false,
        manufacturer: "Microsoft Corporation".into(), model: "Virtual Machine".into(),
        hypervisor_present: true, users: vec![], groups: vec![],
        registration_exists: false, services: vec![], firewall_names: vec![] } }

    fn owned_inventory() -> Inventory {
        Inventory {
            administrator: true,
            manufacturer: "Microsoft Corporation".into(),
            model: "Virtual Machine".into(),
            hypervisor_present: true,
            users: vec![
                Principal {
                    name: USERS[0].into(),
                    sid: "S-1-5-21-100-200-300-1001".into(),
                },
                Principal {
                    name: USERS[1].into(),
                    sid: "S-1-5-21-100-200-300-1002".into(),
                },
            ],
            groups: vec![Principal {
                name: GROUP.into(),
                sid: "S-1-5-21-100-200-300-1003".into(),
            }],
            registration_exists: false,
            services: vec![],
            firewall_names: vec!["codex_sandbox_offline_block_outbound_all".into()],
        }
    }

    #[test]
    fn existing_fixed_identity_stops_without_adoption() {
        let mut i = empty();
        i.users.push(Principal { name: USERS[0].into(), sid: "S-1-5-21-synthetic".into() });
        assert!(pristine(&i).is_err());
    }
    #[test]
    fn registration_or_service_stops() {
        let mut i = empty(); i.registration_exists = true; assert!(pristine(&i).is_err());
        i.registration_exists = false; i.services.push("codex-windows-sandbox-setup".into());
        assert!(pristine(&i).is_err());
    }
    #[test]
    fn owned_inventory_proof_rejects_wrong_sid_or_foreign_protection() {
        let owned = owned_inventory();
        let rules = ["codex_sandbox_offline_block_outbound_all"];
        let filters = [42_u64];
        assert!(verify_owned_inventory_against_proof(
            &owned,
            &owned,
            USERS[0],
            "S-1-5-21-100-200-300-1001",
            USERS[1],
            "S-1-5-21-100-200-300-1002",
            GROUP,
            "S-1-5-21-100-200-300-1003",
            &rules,
            &filters,
        )
        .is_ok());

        // Wrong SID against setup proof is rejected even if current matches snapshot.
        assert!(verify_owned_inventory_against_proof(
            &owned,
            &owned,
            USERS[0],
            "S-1-5-21-100-200-300-9999",
            USERS[1],
            "S-1-5-21-100-200-300-1002",
            GROUP,
            "S-1-5-21-100-200-300-1003",
            &rules,
            &filters,
        )
        .is_err());

        // Extra or replaced firewall rule in live inventory is rejected.
        let mut extra_fw = owned.clone();
        extra_fw.firewall_names.push("foreign_rule".into());
        assert!(verify_owned_inventory_against_proof(
            &extra_fw,
            &owned,
            USERS[0],
            "S-1-5-21-100-200-300-1001",
            USERS[1],
            "S-1-5-21-100-200-300-1002",
            GROUP,
            "S-1-5-21-100-200-300-1003",
            &rules,
            &filters,
        )
        .is_err());

        // Missing or zero WFP filter ID proof is rejected.
        assert!(verify_owned_inventory_against_proof(
            &owned,
            &owned,
            USERS[0],
            "S-1-5-21-100-200-300-1001",
            USERS[1],
            "S-1-5-21-100-200-300-1002",
            GROUP,
            "S-1-5-21-100-200-300-1003",
            &rules,
            &[],
        )
        .is_err());
    }
    #[test]
    fn materialized_runner_rejects_source_fallback_and_outside_home() {
        let source = Path::new(r"C:\public\codex-command-runner.exe");
        let bin = Path::new(r"C:\synthetic\codex-home\.sandbox-bin");
        assert!(check_materialized_identity(source, bin, source).is_err());
        assert!(check_materialized_identity(Path::new(r"C:\other\codex-command-runner.exe"), bin, source).is_err());
        assert!(check_materialized_identity(Path::new(r"C:\synthetic\codex-home\.sandbox-bin\nested\runner.exe"), bin, source).is_err());
        assert!(check_materialized_identity(Path::new(r"C:\synthetic\codex-home\.sandbox-bin\codex-command-runner.exe"), bin, source).is_ok());
    }

    #[test]
    fn original_dev_and_release_resolver_names_match_exact_algorithm() {
        let metadata = RunnerSourceMetadata { length: 26988032, modified_seconds: 0x1234,
            modified_subsecond_nanos: 987654300 };
        assert_eq!(runner_launch_basename("0.0.0", &metadata).unwrap(),
            "codex-command-runner-26988032-1234.exe");
        assert_eq!(runner_launch_basename("1.2.3", &metadata).unwrap(),
            "codex-command-runner-1.2.3.exe");
        for invalid in ["", "../x", r"x:y", "v/x", "x ", ".alias"] {
            assert!(runner_launch_basename(invalid, &metadata).is_err());
        }
    }

    #[test]
    fn resolver_name_changes_on_source_length_or_second_and_never_plain_basename() {
        let a = RunnerSourceMetadata { length: 100, modified_seconds: 0x10, modified_subsecond_nanos: 0 };
        let mut b = a.clone(); b.length += 1;
        let mut c = a.clone(); c.modified_seconds += 1;
        let mut d = a.clone(); d.modified_subsecond_nanos += 1;
        let name = runner_launch_basename("0.0.0", &a).unwrap();
        assert_ne!(name, runner_launch_basename("0.0.0", &b).unwrap());
        assert_ne!(name, runner_launch_basename("0.0.0", &c).unwrap());
        // Original private algorithm ignores subsecond precision in the name.
        assert_eq!(name, runner_launch_basename("0.0.0", &d).unwrap());
        assert_ne!(name, "codex-command-runner.exe");
    }

    #[test]
    fn exact_launch_path_rejects_same_basename_elsewhere_and_wrong_suffix() {
        let expected = Path::new(r"C:\vm\codex-home\.sandbox-bin\codex-command-runner-100-10.exe");
        assert!(check_expected_launch_path(expected, expected).is_ok());
        assert!(check_expected_launch_path(
            Path::new(r"C:\other\.sandbox-bin\codex-command-runner-100-10.exe"), expected).is_err());
        assert!(check_expected_launch_path(
            Path::new(r"C:\vm\codex-home\.sandbox-bin\codex-command-runner-100-11.exe"), expected).is_err());
        assert!(check_expected_launch_path(
            Path::new(r"C:\source\codex-command-runner.exe"), expected).is_err());
    }

    #[test]
    fn existing_destination_is_a_collision_not_adopted() {
        // Genuine existing regular file metadata only; no write, PE/helper execution or TempDir.
        assert!(check_absent(&std::env::current_exe().unwrap()).is_err());
    }

    #[test]
    fn hypervisor_capable_physical_host_is_not_a_hyperv_guest() {
        let mut host = empty(); host.manufacturer = "Dell Inc.".into(); host.model = "Physical Host".into();
        assert!(host.hypervisor_present);
        assert!(require_hyperv_guest(&host).is_err());
        host.manufacturer = "Microsoft Corporation".into(); host.model = "Surface".into();
        assert!(require_hyperv_guest(&host).is_err());
        let mut guest = empty(); assert!(require_hyperv_guest(&guest).is_ok());
        guest.hypervisor_present = false; assert!(require_hyperv_guest(&guest).is_err());
    }

    #[test]
    fn drive_relative_namespace_ads_and_alias_paths_rejected() {
        for path in [r"C:relative", r"\\.\pipe\x", r"C:\x\..\y", r"C:\x\data:stream", r"C:\x\NUL", r"C:\x\alias."] {
            assert!(validate_path(Path::new(path)).is_err(), "{path}");
        }
        assert!(validate_path(Path::new(r"C:\vm-synthetic\run-001")).is_ok());
    }

    #[test]
    fn prepare_cleanup_refuses_materialized_runner_without_bound_execution_admission() {
        // SetupReturnedOk without a materialized runner is allowed.
        assert!(
            validate_prepare_cleanup_phase_and_admission(
                Phase::SetupReturnedOk,
                /*has_materialized_runner*/ false,
                /*has_execution_admission*/ false,
            )
            .is_ok()
        );
        // SetupReturnedOk with a materialized runner is rejected.
        assert!(
            validate_prepare_cleanup_phase_and_admission(
                Phase::SetupReturnedOk,
                /*has_materialized_runner*/ true,
                /*has_execution_admission*/ false,
            )
            .is_err()
        );
        // RunnerMaterialized directly is rejected (file-share probes alone cannot admit cleanup).
        assert!(
            validate_prepare_cleanup_phase_and_admission(
                Phase::RunnerMaterialized,
                /*has_materialized_runner*/ true,
                /*has_execution_admission*/ false,
            )
            .is_err()
        );
        // ExecutionCleanupAdmitted requires both materialized runner and bound admission.
        assert!(
            validate_prepare_cleanup_phase_and_admission(
                Phase::ExecutionCleanupAdmitted,
                /*has_materialized_runner*/ true,
                /*has_execution_admission*/ false,
            )
            .is_err()
        );
        assert!(
            validate_prepare_cleanup_phase_and_admission(
                Phase::ExecutionCleanupAdmitted,
                /*has_materialized_runner*/ true,
                /*has_execution_admission*/ true,
            )
            .is_ok()
        );
    }

    #[test]
    fn public_complete_or_mismatched_owner_facts_never_construct_cleanup_authority() {
        // Pure data, no filesystem/PID access. Even all eight true claims with
        // plausible distinct PIDs and a digest cannot mint the missing ability.
        for runner_pid in [0, 4100, 4200] {
            for digest in [[0x33; 32], [0x44; 32]] {
                let error = BoundExecutionCleanupAdmission::new_from_bound_owner_facts(
                    PathBuf::from(r"C:\vm-synthetic\runner.exe"), digest,
                    PathBuf::from(r"C:\vm-synthetic\codex-home"), 4100, 77, runner_pid,
                    true, true, true, true, true, true, true, true, false,
                ).unwrap_err();
                assert!(error.to_string().contains("NOT_WIRED"));
            }
        }
    }

    #[test]
    fn every_public_boolean_combination_including_sticky_debt_remains_non_authority() {
        for bits in 0u16..512 {
            let claimed = |bit: u32| (bits & (1u16 << bit)) != 0u16;
            let result = BoundExecutionCleanupAdmission::new_from_bound_owner_facts(
                PathBuf::from(r"C:\vm-synthetic\runner.exe"), [0x33; 32],
                PathBuf::from(r"C:\vm-synthetic\codex-home"), 4100, 77, 4200,
                claimed(0), claimed(1), claimed(2), claimed(3), claimed(4),
                claimed(5), claimed(6), claimed(7), claimed(8),
            );
            assert!(result.is_err(), "public boolean bits {bits} must not create authority");
        }
    }

}
