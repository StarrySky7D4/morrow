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
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result, bail, ensure};
use codex_protocol::models::{ManagedFileSystemPermissions, PermissionProfile};
use codex_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSpecialPath,
    NetworkSandboxPolicy,
};
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_windows_sandbox::{
    MatchedRunnerArtifact, PreparedWindowsSandboxCleanup, ResolvedWindowsSandboxPermissions,
    SandboxSetupRequest, prepare_packaged_windows_sandbox_cleanup, resolve_exe_for_launch,
    registered_core_requested, run_elevated_setup, sandbox_bin_dir,
};
use serde::{Deserialize, Serialize};

const REPARSE: u32 = 0x400;
const USERS: [&str; 2] = ["CodexSandboxOffline", "CodexSandboxOnline"];
const GROUP: &str = "CodexSandboxUsers";
// Exact version of the frozen derived codex-windows-sandbox library, not this harness.
const RESOLVER_LIBRARY_VERSION: &str = "0.0.0";
pub const CREATE_CONFIRMATION: &str = "CREATE_ONLY_FRESH_SYNTHETIC_VM_ROOT";
pub const INVENTORY_CONFIRMATION: &str = "READ_ONLY_FIXED_SANDBOX_IDENTITY_AND_REGISTRATION_INVENTORY";
pub const SETUP_CONFIRMATION: &str = "APPLY_FIXED_SANDBOX_ACCOUNTS_DPAPI_ACL_WFP_IN_DISPOSABLE_VM";
pub const MATERIALIZE_CONFIRMATION: &str = "MATERIALIZE_AND_PIN_CHECKED_RUNNER_IN_OWN_SYNTHETIC_HOME";
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
    CleanupPreparing,
    CleanupPrepared,
    CleanupFinishing,
    Finished,
    Unknown,
}

/// Evidence only: intentionally not Deserialize and never usable to restore authority.
#[derive(Serialize)]
pub struct LifecycleManifest {
    pub configuration: ProvisioningConfig,
    pub phase: Phase,
    pub owned_identities: Option<Inventory>,
    pub cleanup_guard_held: bool,
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
    _setup: MatchedRunnerArtifact,
    shell: MatchedRunnerArtifact,
    ancestors: Vec<File>,
    root_handles: Vec<File>,
    materialization_handles: Vec<File>,
    owner_marker: Option<File>,
    owned: Option<Inventory>,
    cleanup: Option<PreparedWindowsSandboxCleanup>,
    cleanup_notes: Vec<String>,
}

// Only bounded guest manufacturer/model/hypervisor facts and public names/SIDs are returned. No registry values, passwords,
// tokens, DPAPI blobs, user homes, or service configuration are read.
const INVENTORY: &str = include_str!("inventory.ps1");

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

impl GuestLifecycle {
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
        Ok(Self { config, phase: Phase::PreflightOnly, source_runner, actual_runner: None,
            actual_runner_identity: None, _harness: harness, _actual_exe: actual_exe, staged_runner: None,
            staged_runner_identity: None, resolver_source_metadata, _setup: setup, shell,
            ancestors, root_handles: Vec::new(), materialization_handles: Vec::new(), owner_marker: None, owned: None, cleanup: None, cleanup_notes: Vec::new() })
    }

    pub fn phase(&self) -> Phase { self.phase }

    /// Caller records this outside the synthetic home after each genuine terminal API result.
    /// An Unknown result must keep this same object/handles reachable; this is not a resume token.
    pub fn manifest(&self) -> LifecycleManifest {
        LifecycleManifest {
            configuration: self.config.clone(), phase: self.phase,
            owned_identities: self.owned.clone(), cleanup_guard_held: self.cleanup.is_some(),
            inventory_script_sha256: "cbc6acc098360610aa00c9de5878293a39ea269d7c406709061cc5a020f1483d",
            actual_runner: self.actual_runner_identity.clone(), cleanup_notes: self.cleanup_notes.clone(),
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
        self.phase = Phase::Unknown;
        let before = inventory(&self.shell)?;
        pristine(&before)?;
        ensure!(before.administrator, "explicit elevated guest-admin process required; no UAC fallback");
        let permissions = ResolvedWindowsSandboxPermissions::try_from_permission_profile(&self.permission_profile()?)?;
        permissions.validate_elevated_filesystem_policy(&self.workspace())?;
        self.phase = Phase::SetupAttempted; // Latch before the first original system effect.
        let result = run_elevated_setup(SandboxSetupRequest {
            permissions: &permissions, command_cwd: &self.workspace(), env_map: &self.environment(),
            codex_home: &self.codex_home(), proxy_enforced: false,
        }).and_then(|()| {
            let after = inventory(&self.shell)?;
            no_registration(&after)?;
            ensure!(after.users.len() == 2 && after.groups.len() == 1, "setup identity evidence incomplete");
            for name in USERS { ensure!(after.users.iter().any(|p| p.name == name && !p.sid.is_empty()), "setup user evidence"); }
            ensure!(after.groups[0].name == GROUP && !after.groups[0].sid.is_empty(), "setup group evidence");
            self.owned = Some(after);
            Ok(())
        });
        self.phase = if result.is_ok() { Phase::SetupReturnedOk } else { Phase::Unknown };
        // Ok means the original setup returned Ok, never WFP/OS ACK qualification.
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
        no_registration(&current)?;
        ensure!(current.administrator, "cleanup requires explicit guest-admin");
        let owned = self.owned.as_ref().context("no exact successful-setup ownership")?;
        ensure!(current.users.len() == owned.users.len() && current.groups.len() == owned.groups.len()
            && owned.users.iter().all(|p| current.users.contains(p))
            && owned.groups.iter().all(|p| current.groups.contains(p)), "principal replacement; STOP cleanup");
        Ok(())
    }

    /// Dedicated exclusive VM only: original cleanup also mutates shared firewall/WFP resources.
    /// Exact captured account SIDs do not prove general ownership of global protections.
    pub fn prepare_cleanup(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == PREPARE_CLEANUP_CONFIRMATION
            && matches!(self.phase, Phase::SetupReturnedOk | Phase::RunnerMaterialized), "prepare cleanup phase/confirmation");
        self.phase = Phase::Unknown;
        self.validate_owned()?;
        self.phase = Phase::CleanupPreparing;
        match prepare_packaged_windows_sandbox_cleanup() {
            Ok(guard) => { self.cleanup = Some(guard); self.phase = Phase::CleanupPrepared; Ok(()) }
            Err(error) => { self.phase = Phase::Unknown; Err(error) }
        }
    }

    /// No arbitrary deletion callback: the synthetic workspace is retained for evidence.
    /// Caller must have completed real child/EOF/facts/worker joins before requesting this phase.
    pub fn finish_cleanup(&mut self, confirmation: &str) -> Result<()> {
        ensure!(confirmation == FINISH_CLEANUP_CONFIRMATION && self.phase == Phase::CleanupPrepared, "finish cleanup phase/confirmation");
        self.phase = Phase::Unknown;
        self.validate_owned()?;
        // Original cleanup removes .sandbox-bin. Caller must have joined all execution owners;
        // reject outstanding artifact borrowers, then release our destination guards once.
        if let Some(pin) = self.actual_runner.take() {
            match Arc::try_unwrap(pin) {
                Ok(artifact) => drop(artifact),
                Err(pin) => {
                    self.actual_runner = Some(pin);
                    bail!("actual runner still borrowed; preserve Unknown and all cleanup state");
                }
            }
        }
        self.materialization_handles.clear();
        self.phase = Phase::CleanupFinishing;
        let notes = RefCell::new(Vec::new());
        let result = self.cleanup.as_ref().context("missing original cleanup guard")?
            .finish(Some(&self.codex_home()), |note| notes.borrow_mut().push(note.to_owned()), || Ok(()));
        self.cleanup_notes.extend(notes.into_inner());
        self.phase = if result.is_ok() { Phase::Finished } else { Phase::Unknown };
        result
    }

    /// Public SID-only ownership facts, never passwords or DPAPI data.
    pub fn owned_identities(&self) -> Option<&Inventory> { self.owned.as_ref() }
    pub fn held_ancestor_count(&self) -> usize { self.ancestors.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty() -> Inventory { Inventory { administrator: false,
        manufacturer: "Microsoft Corporation".into(), model: "Virtual Machine".into(),
        hypervisor_present: true, users: vec![], groups: vec![],
        registration_exists: false, services: vec![], firewall_names: vec![] } }

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
}
