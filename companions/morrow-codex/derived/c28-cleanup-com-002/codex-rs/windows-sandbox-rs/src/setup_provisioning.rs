//! Privileged sandbox setup shared by the setup helper and the service's ProvisionOnly path.
//! Full and read-ACL setup remain helper entrypoints; only they may launch a read-ACL child.
//! Command-line account provisioning cannot enter the service's admitted Core path.

mod firewall;
pub(crate) use firewall::OwnedFirewallProof;
pub(crate) use firewall::OwnedFirewallRuleProof;
pub(crate) use firewall::with_firewall_com_apartment;
mod service;
pub use service::provision_sandbox_in_process;
mod read_acl_mutex;

use crate::DirectoryOpenDisposition;
use crate::SETUP_VERSION;
use crate::SetupErrorCode;
use crate::SetupErrorReport;
use crate::SetupFailure;
use crate::SetupRuntime;
use crate::acquire_sandbox_setup_lock;
use crate::add_deny_write_ace;
use crate::convert_string_sid_to_sid;
use crate::create_directory_guard;
use crate::ensure_allow_mask_aces_with_inheritance;
use crate::ensure_allow_write_aces;
use crate::extract_setup_failure;
use crate::hide_newly_created_users;
use crate::install_wfp_filters;
use crate::local_user_flags;
use crate::log_note;
use crate::log_writer;
use crate::open_directory_no_reparse;
use crate::path_mask_allows;
use crate::path_write_aces_need_refresh;
use crate::resolve_sid;
use crate::sandbox_bin_dir;
use crate::sandbox_dir;
use crate::sandbox_secrets_dir;
use crate::set_local_user_flags;
use crate::setup_error_path;
use crate::setup_log_writer;
use crate::string_from_sid_bytes;
use crate::sync_persistent_deny_read_acls;
use crate::to_wide;
use crate::workspace_write_cap_sid_for_root;
use crate::workspace_write_root_overlaps_path;
use crate::write_file_atomically;
use crate::write_setup_error_report;
use anyhow::Context;
use anyhow::Result;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use codex_otel::StatsigMetricsSettings;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::ffi::c_void;
use std::io::Read;
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsHandle;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::OwnedHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::mpsc;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::HLOCAL;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::NetworkManagement::NetManagement::UF_ACCOUNTDISABLE;
use windows_sys::Win32::Security::ACL;
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::Authorization::EXPLICIT_ACCESS_W;
use windows_sys::Win32::Security::Authorization::GRANT_ACCESS;
use windows_sys::Win32::Security::Authorization::SE_FILE_OBJECT;
use windows_sys::Win32::Security::Authorization::SetEntriesInAclW;
use windows_sys::Win32::Security::Authorization::SetNamedSecurityInfoW;
use windows_sys::Win32::Security::Authorization::SetSecurityInfo;
use windows_sys::Win32::Security::Authorization::TRUSTEE_IS_SID;
use windows_sys::Win32::Security::Authorization::TRUSTEE_W;
use windows_sys::Win32::Security::CONTAINER_INHERIT_ACE;
use windows_sys::Win32::Security::DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Security::OBJECT_INHERIT_ACE;
use windows_sys::Win32::Security::PROTECTED_DACL_SECURITY_INFORMATION;
use windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION;
use windows_sys::Win32::Storage::FileSystem::DELETE;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_EXECUTE;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_READ;
use windows_sys::Win32::Storage::FileSystem::FILE_GENERIC_WRITE;
use windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_DELETE;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ;
use windows_sys::Win32::Storage::FileSystem::FILE_SHARE_WRITE;
use windows_sys::Win32::Storage::FileSystem::FILE_TRAVERSE;
use windows_sys::Win32::Storage::FileSystem::GetFileInformationByHandle;
use windows_sys::Win32::Storage::FileSystem::READ_CONTROL;
use windows_sys::Win32::Storage::FileSystem::WRITE_DAC;
use windows_sys::Win32::System::Threading::INFINITE;

const DENY_ACCESS: i32 = 3;
#[cfg(test)]
const WRITE_ROOT_ALLOW_MASK: u32 =
    FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;

mod sandbox_users;
mod setup_runtime_bin;
use read_acl_mutex::acquire_read_acl_mutex;
pub(crate) use read_acl_mutex::read_acl_mutex_exists;
pub(crate) use sandbox_users::OwnedHiddenUsersProof;
pub(crate) use sandbox_users::OwnedPrincipalsProof;
pub(crate) use sandbox_users::OwnedUserPrincipalProof;
pub(crate) use sandbox_users::WINLOGON_USERLIST_KEY_PATH;
use sandbox_users::commit_setup_marker;
use sandbox_users::commit_setup_marker_with_roots;
use sandbox_users::prepare_setup_marker;
use sandbox_users::provision_owned_sandbox_users;
use sandbox_users::provision_sandbox_users;
use sandbox_users::resolve_sandbox_users_group_sid;
use sandbox_users::sid_bytes_to_psid;

pub(crate) const OWNED_SETUP_PROTOCOL_SCHEMA_V1: u32 = 1;
pub(crate) const OWNED_SETUP_RECEIPT_PREFIX_V1: &str = "CODEX_OWNED_SETUP_RECEIPT_V1:";

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedSetupInvocationWire {
    pub(crate) schema_version: u32,
    pub(crate) nonce_b64: String,
    pub(crate) parent_pid: u32,
    pub(crate) expected_setup_exe_sha256: [u8; 32],
    pub(crate) request_policy_sha256: [u8; 32],
    pub(crate) real_user_sid: String,
    pub(crate) codex_home_identity: OwnedFileIdentity,
    pub(crate) sandbox_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_secrets_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_bin_dir_identity: OwnedFileIdentity,
}

impl std::fmt::Debug for OwnedSetupInvocationWire {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedSetupInvocationWire")
            .field("schema_version", &self.schema_version)
            .field("nonce_b64", &"<redacted>")
            .field("parent_pid", &self.parent_pid)
            .field("expected_setup_exe_sha256", &self.expected_setup_exe_sha256)
            .field("request_policy_sha256", &self.request_policy_sha256)
            .field("real_user_sid", &self.real_user_sid)
            .field("codex_home_identity", &self.codex_home_identity)
            .field("sandbox_dir_identity", &self.sandbox_dir_identity)
            .field(
                "sandbox_secrets_dir_identity",
                &self.sandbox_secrets_dir_identity,
            )
            .field("sandbox_bin_dir_identity", &self.sandbox_bin_dir_identity)
            .finish()
    }
}

// Legacy omits runtime; readers default its absence to Legacy.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Payload {
    version: u32,
    offline_username: String,
    online_username: String,
    codex_home: PathBuf,
    command_cwd: PathBuf,
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
    #[serde(default)]
    deny_read_paths: Vec<PathBuf>,
    #[serde(default)]
    deny_write_paths: Vec<PathBuf>,
    proxy_ports: Vec<u16>,
    #[serde(default)]
    allow_local_binding: bool,
    #[serde(default)]
    otel: Option<StatsigMetricsSettings>,
    real_user: String,
    #[serde(default)]
    mode: SetupMode,
    #[serde(default, skip_serializing_if = "SetupRuntime::is_legacy")]
    runtime: SetupRuntime,
    #[serde(default)]
    refresh_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owned_invocation: Option<OwnedSetupInvocationWire>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
enum SetupMode {
    #[default]
    Full,
    InteractiveProvision,
    ProvisionOnly,
    ReadAclsOnly,
    OwnedFull,
}

impl SetupMode {
    fn provisions_accounts(self, refresh_only: bool) -> bool {
        match self {
            Self::ProvisionOnly | Self::InteractiveProvision => true,
            Self::Full | Self::OwnedFull => !refresh_only,
            Self::ReadAclsOnly => false,
        }
    }
}

#[derive(Clone, Copy)]
enum DaclInheritance {
    Inherited,
    Protected,
}

fn log_line(log: &mut dyn Write, msg: &str) -> Result<()> {
    let ts = chrono::Utc::now().to_rfc3339();
    writeln!(log, "[{ts}] {msg}").map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperLogFailed,
            format!("failed to write setup log line: {err}"),
        ))
    })?;
    Ok(())
}

fn workspace_write_cap_sids_for_path(
    codex_home: &Path,
    command_cwd: &Path,
    write_roots: &[PathBuf],
    path: &Path,
) -> Result<Vec<String>> {
    let mut sid_strs = Vec::new();
    for root in write_roots {
        if workspace_write_root_overlaps_path(root, path) {
            sid_strs.push(workspace_write_cap_sid_for_root(
                codex_home,
                command_cwd,
                root,
            )?);
        }
    }
    if sid_strs.is_empty() {
        if write_roots.is_empty() {
            sid_strs.push(workspace_write_cap_sid_for_root(
                codex_home,
                command_cwd,
                command_cwd,
            )?);
        } else {
            for root in write_roots {
                sid_strs.push(workspace_write_cap_sid_for_root(
                    codex_home,
                    command_cwd,
                    root,
                )?);
            }
        }
    }
    Ok(sid_strs)
}

fn spawn_read_acl_helper(payload: &Payload, _log: &mut dyn Write) -> Result<()> {
    let mut read_payload = payload.clone();
    read_payload.mode = SetupMode::ReadAclsOnly;
    read_payload.refresh_only = true;
    let payload_json = serde_json::to_vec(&read_payload)?;
    let payload_b64 = BASE64.encode(payload_json);
    let exe = std::env::current_exe().context("locate setup helper")?;
    let mut command = Command::new(&exe);
    crate::launch_environment::configure_command(&mut command, &payload_b64)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .context("spawn read ACL helper")?;
    Ok(())
}

struct ReadAclSubjects<'a> {
    sandbox_group_psid: *mut c_void,
    rx_psids: &'a [*mut c_void],
}

fn apply_read_acls(
    read_roots: &[PathBuf],
    subjects: &ReadAclSubjects<'_>,
    log: &mut dyn Write,
    refresh_errors: &mut Vec<String>,
    access_mask: u32,
    access_label: &str,
    inheritance: u32,
) -> Result<()> {
    for root in read_roots {
        if !root.exists() {
            log_line(
                log,
                &format!("{access_label} root {} missing; skipping", root.display()),
            )?;
            continue;
        }
        let builtin_has = read_mask_allows_or_log(
            root,
            subjects.rx_psids,
            /*label*/ None,
            access_mask,
            access_label,
            refresh_errors,
            log,
        )?;
        if builtin_has {
            continue;
        }
        let sandbox_has = read_mask_allows_or_log(
            root,
            &[subjects.sandbox_group_psid],
            Some("sandbox_group"),
            access_mask,
            access_label,
            refresh_errors,
            log,
        )?;
        if sandbox_has {
            continue;
        }
        log_line(
            log,
            &format!(
                "granting {access_label} ACE to {} for sandbox users",
                root.display()
            ),
        )?;
        let result = unsafe {
            ensure_allow_mask_aces_with_inheritance(
                root,
                &[subjects.sandbox_group_psid],
                access_mask,
                inheritance,
            )
        };
        if let Err(err) = result {
            refresh_errors.push(format!(
                "grant {access_label} ACE failed on {} for sandbox_group: {err}",
                root.display()
            ));
            log_line(
                log,
                &format!(
                    "grant {access_label} ACE failed on {} for sandbox_group: {err}",
                    root.display()
                ),
            )?;
        }
    }
    Ok(())
}

fn read_mask_allows_or_log(
    root: &Path,
    psids: &[*mut c_void],
    label: Option<&str>,
    read_mask: u32,
    access_label: &str,
    refresh_errors: &mut Vec<String>,
    log: &mut dyn Write,
) -> Result<bool> {
    match path_mask_allows(root, psids, read_mask, /*require_all_bits*/ true) {
        Ok(has) => Ok(has),
        Err(e) => {
            let label_suffix = label
                .map(|value| format!(" for {value}"))
                .unwrap_or_default();
            refresh_errors.push(format!(
                "{access_label} mask check failed on {}{}: {}",
                root.display(),
                label_suffix,
                e
            ));
            log_line(
                log,
                &format!(
                    "{access_label} mask check failed on {}{}: {}; continuing",
                    root.display(),
                    label_suffix,
                    e
                ),
            )?;
            Ok(false)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn lock_sandbox_dir(
    dir: &Path,
    real_user: &str,
    sandbox_group_sid: &[u8],
    sandbox_group_access_mode: i32,
    sandbox_group_mask: u32,
    real_user_mask: u32,
    dacl_inheritance: DaclInheritance,
    setup_mode: SetupMode,
) -> Result<()> {
    // ProvisionOnly accepts another user's CODEX_HOME; keep its ACL mutation
    // bound to a no-reparse handle without changing interactive setup behavior.
    let directory = match setup_mode {
        SetupMode::Full | SetupMode::InteractiveProvision | SetupMode::ReadAclsOnly => {
            std::fs::create_dir_all(dir)?;
            None
        }
        SetupMode::ProvisionOnly | SetupMode::OwnedFull => Some(open_directory_no_reparse(
            dir,
            // SetSecurityInfo can reject a WRITE_DAC-only directory handle.
            READ_CONTROL | WRITE_DAC,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            DirectoryOpenDisposition::OpenOrCreate,
        )?),
    };
    let system_sid = resolve_sid("SYSTEM")?;
    let admins_sid = resolve_sid("Administrators")?;
    let real_sid = resolve_sid(real_user)?;
    let entries = [
        (
            sandbox_group_sid.to_vec(),
            sandbox_group_mask,
            sandbox_group_access_mode,
        ),
        (
            system_sid,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
            GRANT_ACCESS,
        ),
        (
            admins_sid,
            FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
            GRANT_ACCESS,
        ),
        (real_sid, real_user_mask, GRANT_ACCESS),
    ];
    unsafe {
        let mut eas: Vec<EXPLICIT_ACCESS_W> = Vec::new();
        let mut sids: Vec<*mut c_void> = Vec::new();
        for (sid_bytes, mask, access_mode) in entries.iter().map(|(s, m, a)| (s, *m, *a)) {
            let sid_str = string_from_sid_bytes(sid_bytes).map_err(anyhow::Error::msg)?;
            let sid_w = to_wide(OsStr::new(&sid_str));
            let mut psid: *mut c_void = std::ptr::null_mut();
            if ConvertStringSidToSidW(sid_w.as_ptr(), &mut psid) == 0 {
                return Err(anyhow::anyhow!(
                    "ConvertStringSidToSidW failed: {}",
                    GetLastError()
                ));
            }
            sids.push(psid);
            eas.push(EXPLICIT_ACCESS_W {
                grfAccessPermissions: mask,
                grfAccessMode: access_mode,
                grfInheritance: OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                Trustee: TRUSTEE_W {
                    pMultipleTrustee: std::ptr::null_mut(),
                    MultipleTrusteeOperation: 0,
                    TrusteeForm: TRUSTEE_IS_SID,
                    TrusteeType: TRUSTEE_IS_SID,
                    ptstrName: psid as *mut u16,
                },
            });
        }
        let mut new_dacl: *mut ACL = std::ptr::null_mut();
        let set = SetEntriesInAclW(
            eas.len() as u32,
            eas.as_ptr(),
            std::ptr::null_mut(),
            &mut new_dacl,
        );
        if set != 0 {
            return Err(anyhow::anyhow!(
                "SetEntriesInAclW sandbox dir failed: {set}",
            ));
        }
        let security_information = match dacl_inheritance {
            DaclInheritance::Inherited => DACL_SECURITY_INFORMATION,
            DaclInheritance::Protected => {
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION
            }
        };
        let (res, api) = match directory.as_ref() {
            Some(directory) => (
                SetSecurityInfo(
                    directory.as_raw_handle() as _,
                    SE_FILE_OBJECT,
                    security_information,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    new_dacl,
                    std::ptr::null_mut(),
                ),
                "SetSecurityInfo",
            ),
            None => {
                let path_w = to_wide(dir.as_os_str());
                (
                    SetNamedSecurityInfoW(
                        path_w.as_ptr() as *mut u16,
                        SE_FILE_OBJECT,
                        security_information,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        new_dacl,
                        std::ptr::null_mut(),
                    ),
                    "SetNamedSecurityInfoW",
                )
            }
        };
        if res != 0 {
            return Err(anyhow::anyhow!("{api} sandbox dir failed: {res}"));
        }
        if !new_dacl.is_null() {
            LocalFree(new_dacl as HLOCAL);
        }
        for sid in sids {
            if !sid.is_null() {
                LocalFree(sid as HLOCAL);
            }
        }
    }
    Ok(())
}

pub fn main() -> Result<()> {
    let mut setup_mode = None;
    let ret = real_main(&mut setup_mode);
    if let Err(e) = &ret {
        // Best-effort: log unexpected top-level errors.
        if let Ok(codex_home) = std::env::var("CODEX_HOME") {
            let sbx_dir = sandbox_dir(Path::new(&codex_home));
            let _ = std::fs::create_dir_all(&sbx_dir);
            // An unparsed payload must not enable writes to an existing log.
            let mode = setup_mode.unwrap_or(SetupMode::ProvisionOnly);
            if let Ok(mut f) = open_setup_log(&sbx_dir, mode) {
                let _ = writeln!(
                    f,
                    "[{}] top-level error: {}",
                    chrono::Utc::now().to_rfc3339(),
                    e
                );
            }
        }
    }
    ret
}

fn open_setup_log(sbx_dir: &Path, mode: SetupMode) -> Result<Box<dyn Write>> {
    match mode {
        SetupMode::ProvisionOnly => Ok(Box::new(setup_log_writer(sbx_dir)?)),
        SetupMode::Full
        | SetupMode::InteractiveProvision
        | SetupMode::ReadAclsOnly
        | SetupMode::OwnedFull => log_writer(sbx_dir)
            .map(|log| Box::new(log) as Box<dyn Write>)
            .context("open daily sandbox log"),
    }
}

fn real_main(setup_mode: &mut Option<SetupMode>) -> Result<()> {
    let mut args = std::env::args().collect::<Vec<_>>();
    if args.len() != 2 {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            "expected payload argument",
        )));
    }
    let payload_b64 = args.remove(1);
    let payload_b64 = if payload_b64 == crate::launch_environment::ARG {
        crate::environment_transport::decode(std::env::vars_os()).map_err(|err| {
            anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperRequestArgsFailed,
                format!("failed to read payload environment: {err}"),
            ))
        })?
    } else {
        payload_b64
    };
    let payload_json = BASE64.decode(payload_b64).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!("failed to decode payload b64: {err}"),
        ))
    })?;
    let payload: Payload = serde_json::from_slice(&payload_json).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!("failed to parse payload json: {err}"),
        ))
    })?;
    *setup_mode = Some(payload.mode);
    if payload.version != SETUP_VERSION {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            format!(
                "setup version mismatch: expected {SETUP_VERSION}, got {}",
                payload.version
            ),
        )));
    }
    if payload.mode == SetupMode::OwnedFull && payload.refresh_only {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperRequestArgsFailed,
            "owned-full setup mode does not allow refresh_only",
        )));
    }
    // Only this entry parses command-line JSON. Keep admission locked through
    // the entire account-provisioning operation, including initial Full setup.
    // The in-process adapter enters separately after authenticated service admission.
    let _setup_guard = if payload.mode.provisions_accounts(payload.refresh_only) {
        let guard = acquire_sandbox_setup_lock(INFINITE)?;
        anyhow::ensure!(
            payload.runtime == SetupRuntime::Legacy && !crate::registered_core_requested(),
            "registered Core requires service-owned provisioning"
        );
        if payload.mode == SetupMode::OwnedFull {
            anyhow::ensure!(
                crate::runtime_ownership::load_installation()?.is_none(),
                "owned sandbox setup requires no existing installation registration"
            );
        } else {
            anyhow::ensure!(
                crate::runtime_ownership::load_installation()?
                    .is_none_or(|record| record.runtime.is_none()),
                "registered Core owns these sandbox accounts; helper provisioning is not permitted"
            );
        }
        Some(guard)
    } else {
        None
    };
    run_payload(&payload)
}

fn run_payload(payload: &Payload) -> Result<()> {
    let sbx_dir = sandbox_dir(&payload.codex_home);
    std::fs::create_dir_all(&sbx_dir).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSandboxDirCreateFailed,
            format!("failed to create sandbox dir {}: {err}", sbx_dir.display()),
        ))
    })?;
    let mut log = open_setup_log(&sbx_dir, payload.mode).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperLogFailed,
            format!("open log in {} failed: {err}", sbx_dir.display()),
        ))
    })?;
    let result = run_setup(payload, &mut log, &sbx_dir);
    if let Err(err) = &result {
        let _ = log_line(&mut log, &format!("setup error: {err:?}"));
        log_note(&format!("setup error: {err:?}"), Some(sbx_dir.as_path()));
        let failure = extract_setup_failure(err)
            .map(|f| SetupFailure::new(f.code, f.message.clone()))
            .unwrap_or_else(|| {
                SetupFailure::new(SetupErrorCode::HelperUnknownError, err.to_string())
            });
        let report = SetupErrorReport {
            code: failure.code,
            message: failure.message,
        };
        let write_report = match payload.mode {
            SetupMode::ProvisionOnly => serde_json::to_vec_pretty(&report)
                .map_err(anyhow::Error::from)
                .and_then(|json| {
                    write_file_atomically(&setup_error_path(&payload.codex_home), &json)
                }),
            SetupMode::Full
            | SetupMode::InteractiveProvision
            | SetupMode::ReadAclsOnly
            | SetupMode::OwnedFull => write_setup_error_report(&payload.codex_home, &report),
        };
        if let Err(write_err) = write_report {
            let _ = log_line(
                &mut log,
                &format!("setup error report write failed: {write_err}"),
            );
            log_note(
                &format!("setup error report write failed: {write_err}"),
                Some(sbx_dir.as_path()),
            );
        }
    }
    result
}

fn run_setup(payload: &Payload, log: &mut dyn Write, sbx_dir: &Path) -> Result<()> {
    if payload.mode == SetupMode::OwnedFull {
        for existing in [
            crate::setup::setup_marker_path(&payload.codex_home),
            crate::setup::sandbox_users_path(&payload.codex_home),
        ] {
            match std::fs::symlink_metadata(&existing) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
                Ok(_) => {
                    anyhow::bail!(
                        "pre-existing sandbox state file {} found before owned setup; refusing to adopt",
                        existing.display()
                    );
                }
            }
        }
    }
    let writes_setup_marker = !payload.refresh_only && payload.mode != SetupMode::ReadAclsOnly;
    let marker = if writes_setup_marker {
        Some(prepare_setup_marker(
            &payload.codex_home,
            &payload.real_user,
            payload.mode,
        )?)
    } else {
        None
    };
    match payload.mode {
        SetupMode::ReadAclsOnly => run_read_acl_only(payload, log),
        SetupMode::InteractiveProvision | SetupMode::ProvisionOnly => {
            run_provision_only(payload, log, sbx_dir)
        }
        SetupMode::Full => run_setup_full(payload, log, sbx_dir),
        SetupMode::OwnedFull => {
            let marker = marker.ok_or_else(|| {
                anyhow::anyhow!("owned-full setup requires a prepared setup marker")
            })?;
            return run_setup_owned_full(payload, log, sbx_dir, marker);
        }
    }?;
    if let Some(marker) = marker {
        commit_setup_marker(
            marker,
            &payload.codex_home,
            &payload.offline_username,
            &payload.online_username,
            &payload.proxy_ports,
            payload.allow_local_binding,
        )?;
    }
    Ok(())
}

fn run_read_acl_only(payload: &Payload, log: &mut dyn Write) -> Result<()> {
    let _read_acl_guard = match acquire_read_acl_mutex()? {
        Some(guard) => guard,
        None => {
            log_line(log, "read ACL helper already running; skipping")?;
            return Ok(());
        }
    };
    log_line(log, "read-acl-only mode: applying read ACLs")?;
    let sandbox_group_sid = resolve_sandbox_users_group_sid()?;
    let sandbox_group_psid = sid_bytes_to_psid(&sandbox_group_sid)?;
    let mut refresh_errors: Vec<String> = Vec::new();
    if !payload.read_roots.is_empty() {
        let users_sid = resolve_sid("Users")?;
        let users_psid = sid_bytes_to_psid(&users_sid)?;
        let auth_sid = resolve_sid("Authenticated Users")?;
        let auth_psid = sid_bytes_to_psid(&auth_sid)?;
        let everyone_sid = resolve_sid("Everyone")?;
        let everyone_psid = sid_bytes_to_psid(&everyone_sid)?;
        let rx_psids = vec![users_psid, auth_psid, everyone_psid];
        let subjects = ReadAclSubjects {
            sandbox_group_psid,
            rx_psids: &rx_psids,
        };
        apply_read_acls(
            &payload.read_roots,
            &subjects,
            log,
            &mut refresh_errors,
            FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
            "read",
            OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
        )?;
        unsafe {
            if !users_psid.is_null() {
                LocalFree(users_psid as HLOCAL);
            }
            if !auth_psid.is_null() {
                LocalFree(auth_psid as HLOCAL);
            }
            if !everyone_psid.is_null() {
                LocalFree(everyone_psid as HLOCAL);
            }
        }
    }
    unsafe {
        if !sandbox_group_psid.is_null() {
            LocalFree(sandbox_group_psid as HLOCAL);
        }
    }
    if !refresh_errors.is_empty() {
        log_line(
            log,
            &format!("read ACL run completed with errors: {refresh_errors:?}"),
        )?;
        if payload.refresh_only {
            anyhow::bail!("read ACL run had errors");
        }
    }
    log_line(log, "read ACL run completed")?;
    Ok(())
}

fn provision_sandbox(payload: &Payload, log: &mut dyn Write, sbx_dir: &Path) -> Result<()> {
    let _setup_lock = acquire_sandbox_setup_lock(INFINITE)?;
    let mut repairing_disabled_accounts = false;
    for username in [&payload.offline_username, &payload.online_username] {
        if local_user_flags(username)?.is_some_and(|flags| flags & UF_ACCOUNTDISABLE != 0) {
            repairing_disabled_accounts = true;
        }
    }
    // Interrupted cleanup can leave one account missing and the other disabled. Keep any
    // replacement disabled too until this repair has restored the network restrictions.
    let new_user_flags = if repairing_disabled_accounts {
        UF_ACCOUNTDISABLE
    } else {
        0
    };
    let provision_result = provision_sandbox_users(
        &payload.codex_home,
        &payload.offline_username,
        &payload.online_username,
        new_user_flags,
        log,
        payload.mode,
    );
    if let Err(err) = provision_result {
        if extract_setup_failure(&err).is_some() {
            return Err(err);
        }
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUserProvisionFailed,
            format!("provision sandbox users failed: {err}"),
        )));
    }
    let users = vec![
        payload.offline_username.clone(),
        payload.online_username.clone(),
    ];
    hide_newly_created_users(&users, sbx_dir);
    let offline_sid = resolve_sid(&payload.offline_username).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSidResolveFailed,
            format!(
                "resolve SID for offline user {} failed: {err}",
                payload.offline_username
            ),
        ))
    })?;
    let offline_sid_str = string_from_sid_bytes(&offline_sid).map_err(anyhow::Error::msg)?;
    configure_offline_sandbox_network(payload, &offline_sid_str, log)?;
    let wfp_result = install_wfp_filters(
        &payload.codex_home,
        &payload.offline_username,
        payload.otel.as_ref(),
        |message| {
            let _ = log_line(log, message);
        },
    );
    if repairing_disabled_accounts {
        // Ordinary setup keeps its best-effort WFP behavior. Recovery must not reopen logons
        // after cleanup removed protections unless restoring those protections succeeded.
        wfp_result?;
        for username in [&payload.offline_username, &payload.online_username] {
            let flags = local_user_flags(username)?.ok_or_else(|| {
                anyhow::anyhow!("sandbox user {username} disappeared during repair")
            })?;
            set_local_user_flags(username, flags & !UF_ACCOUNTDISABLE)?;
        }
    }
    Ok(())
}

fn configure_offline_sandbox_network(
    payload: &Payload,
    offline_sid_str: &str,
    log: &mut dyn Write,
) -> Result<()> {
    let proxy_allowlist_result = firewall::ensure_offline_proxy_allowlist(
        offline_sid_str,
        &payload.proxy_ports,
        payload.allow_local_binding,
        log,
    );
    if let Err(err) = proxy_allowlist_result {
        if extract_setup_failure(&err).is_some() {
            return Err(err);
        }
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperFirewallRuleCreateOrAddFailed,
            format!("ensure offline proxy allowlist failed: {err}"),
        )));
    }
    let firewall_result = firewall::ensure_offline_network_blocks(offline_sid_str, log);
    if let Err(err) = firewall_result {
        if extract_setup_failure(&err).is_some() {
            return Err(err);
        }
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperFirewallRuleCreateOrAddFailed,
            format!("ensure offline network blocks failed: {err}"),
        )));
    }
    Ok(())
}

fn lock_persistent_sandbox_dirs(payload: &Payload, sandbox_group_sid: &[u8]) -> Result<()> {
    lock_sandbox_dir(
        &sandbox_dir(&payload.codex_home),
        &payload.real_user,
        sandbox_group_sid,
        GRANT_ACCESS,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE,
        DaclInheritance::Inherited,
        payload.mode,
    )
    .map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSandboxLockFailed,
            format!(
                "lock sandbox dir {} failed: {err}",
                sandbox_dir(&payload.codex_home).display()
            ),
        ))
    })?;
    lock_sandbox_dir(
        &sandbox_secrets_dir(&payload.codex_home),
        &payload.real_user,
        sandbox_group_sid,
        DENY_ACCESS,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE,
        DaclInheritance::Inherited,
        payload.mode,
    )
    .map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSandboxLockFailed,
            format!(
                "lock sandbox secrets dir {} failed: {err}",
                sandbox_secrets_dir(&payload.codex_home).display()
            ),
        ))
    })?;
    let legacy_users = sandbox_dir(&payload.codex_home).join("sandbox_users.json");
    if legacy_users.exists() {
        let _ = std::fs::remove_file(&legacy_users);
    }
    Ok(())
}

fn lock_sandbox_bin_dir(payload: &Payload, sandbox_group_sid: &[u8]) -> Result<()> {
    if payload.runtime == SetupRuntime::Registered {
        return Ok(());
    }
    // The owner's unelevated refresh must be able to reapply this protected DACL.
    lock_sandbox_dir(
        &sandbox_bin_dir(&payload.codex_home),
        &payload.real_user,
        sandbox_group_sid,
        GRANT_ACCESS,
        FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
        FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE | WRITE_DAC,
        DaclInheritance::Protected,
        payload.mode,
    )
    .map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSandboxLockFailed,
            format!(
                "lock sandbox bin dir {} failed: {err}",
                sandbox_bin_dir(&payload.codex_home).display()
            ),
        ))
    })
}

fn run_provision_only(payload: &Payload, log: &mut dyn Write, sbx_dir: &Path) -> Result<()> {
    provision_sandbox(payload, log, sbx_dir)?;

    let sandbox_group_sid = resolve_sandbox_users_group_sid().map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSidResolveFailed,
            format!("resolve sandbox users group SID failed: {err}"),
        ))
    })?;

    lock_sandbox_bin_dir(payload, &sandbox_group_sid)?;
    lock_persistent_sandbox_dirs(payload, &sandbox_group_sid)?;
    log_note("setup provisioning binary completed", Some(sbx_dir));
    Ok(())
}

fn run_setup_full(payload: &Payload, log: &mut dyn Write, sbx_dir: &Path) -> Result<()> {
    let refresh_only = payload.refresh_only;
    if !refresh_only {
        provision_sandbox(payload, log, sbx_dir)?;
    }

    let sandbox_group_sid = resolve_sandbox_users_group_sid().map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSidResolveFailed,
            format!("resolve sandbox users group SID failed: {err}"),
        ))
    })?;
    let sandbox_group_psid = sid_bytes_to_psid(&sandbox_group_sid).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSidResolveFailed,
            format!("convert sandbox users group SID to PSID failed: {err}"),
        ))
    })?;
    let sandbox_group_sid_str =
        string_from_sid_bytes(&sandbox_group_sid).map_err(anyhow::Error::msg)?;

    let mut refresh_errors: Vec<String> = Vec::new();

    // Deny-read ACEs must be present before the sandboxed command starts. Apply
    // them synchronously here instead of delegating them to the background
    // helper used for read grants.
    let applied_deny_read_paths = unsafe {
        sync_persistent_deny_read_acls(
            &payload.codex_home,
            &sandbox_group_sid_str,
            &payload.deny_read_paths,
            sandbox_group_psid,
        )
    }
    .context("apply deny-read ACLs")?;
    if !applied_deny_read_paths.is_empty() {
        log_line(
            log,
            &format!("applied {} deny-read ACLs", applied_deny_read_paths.len()),
        )?;
    }

    if payload.read_roots.is_empty() {
        log_line(log, "no read roots to grant; skipping read ACL helper")?;
    } else {
        match read_acl_mutex_exists() {
            Ok(true) => {
                log_line(log, "read ACL helper already running; skipping spawn")?;
            }
            Ok(false) => {
                spawn_read_acl_helper(payload, log).map_err(|err| {
                    anyhow::Error::new(SetupFailure::new(
                        SetupErrorCode::HelperReadAclHelperSpawnFailed,
                        format!("spawn read ACL helper failed: {err}"),
                    ))
                })?;
            }
            Err(err) => {
                log_line(
                    log,
                    &format!("read ACL mutex check failed: {err}; spawning anyway"),
                )?;
                spawn_read_acl_helper(payload, log).map_err(|spawn_err| {
                    anyhow::Error::new(SetupFailure::new(
                        SetupErrorCode::HelperReadAclHelperSpawnFailed,
                        format!(
                            "spawn read ACL helper failed after mutex error {err}: {spawn_err}"
                        ),
                    ))
                })?;
            }
        }
    }

    if refresh_only {
        setup_runtime_bin::ensure_codex_app_runtime_paths_readable(
            sandbox_group_psid,
            &mut refresh_errors,
            log,
        )?;
    }

    let mut grant_tasks: Vec<(PathBuf, String)> = Vec::new();

    let mut seen_deny_paths: HashSet<PathBuf> = HashSet::new();
    let mut seen_write_roots: HashSet<PathBuf> = HashSet::new();
    for root in &payload.write_roots {
        if !seen_write_roots.insert(root.clone()) {
            continue;
        }
        if !root.exists() {
            log_line(
                log,
                &format!("write root {} missing; skipping", root.display()),
            )?;
            continue;
        }
        let root_cap_sid_str =
            workspace_write_cap_sid_for_root(&payload.codex_home, &payload.command_cwd, root)?;
        let root_cap_psid = unsafe {
            convert_string_sid_to_sid(&root_cap_sid_str)
                .ok_or_else(|| anyhow::anyhow!("convert write root capability SID failed"))?
        };
        let need_grant =
            match path_write_aces_need_refresh(root, &[sandbox_group_psid, root_cap_psid]) {
                Ok(needs_refresh) => needs_refresh,
                Err(e) => {
                    refresh_errors.push(format!(
                        "write ACE check failed on {}: {}",
                        root.display(),
                        e
                    ));
                    log_line(
                        log,
                        &format!(
                            "write ACE check failed on {}: {}; continuing",
                            root.display(),
                            e
                        ),
                    )?;
                    true
                }
            };
        unsafe {
            LocalFree(root_cap_psid as HLOCAL);
        }
        if need_grant {
            log_line(
                log,
                &format!(
                    "granting write ACE to {} for sandbox group and capability SID",
                    root.display()
                ),
            )?;
            grant_tasks.push((root.clone(), root_cap_sid_str));
        }
    }

    let (tx, rx) = mpsc::channel::<(PathBuf, Result<bool>)>();
    std::thread::scope(|scope| {
        for (root, root_cap_sid_str) in grant_tasks {
            let sid_strings = vec![sandbox_group_sid_str.clone(), root_cap_sid_str];
            let tx = tx.clone();
            scope.spawn(move || {
                // Convert SID strings to psids locally in this thread.
                let mut psids: Vec<*mut c_void> = Vec::new();
                for sid_str in &sid_strings {
                    if let Some(psid) = unsafe { convert_string_sid_to_sid(sid_str) } {
                        psids.push(psid);
                    } else {
                        let _ = tx.send((root.clone(), Err(anyhow::anyhow!("convert SID failed"))));
                        return;
                    }
                }

                let res = unsafe { ensure_allow_write_aces(&root, &psids) };

                for psid in psids {
                    unsafe {
                        LocalFree(psid as HLOCAL);
                    }
                }
                let _ = tx.send((root, res));
            });
        }
        drop(tx);
        for (root, res) in rx {
            match res {
                Ok(_) => {}
                Err(e) => {
                    refresh_errors.push(format!("write ACE failed on {}: {}", root.display(), e));
                    if log_line(
                        log,
                        &format!("write ACE grant failed on {}: {}", root.display(), e),
                    )
                    .is_err()
                    {
                        // ignore log errors inside scoped thread
                    }
                }
            }
        }
    });

    for path in &payload.deny_write_paths {
        if !seen_deny_paths.insert(path.clone()) {
            continue;
        }

        // These are deny-write carveouts, not deny-read paths. They may come from explicit
        // read-only-under-a-writable-root carveouts in the transformed sandbox policy, or from
        // legacy protected children such as `.git`, `.codex`, and `.agents`.
        //
        // Deny ACEs attach to filesystem objects; if an explicit policy carveout does not exist
        // during setup, the sandbox could otherwise create it later under a writable parent and
        // bypass the carveout. Materialize missing carveouts as directories so the deny-write ACL
        // is present before the command starts. Legacy protected children are filtered before
        // payload creation, so this should not create sentinel directories in a workspace.
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("failed to create deny-write path {}", path.display()))?;
        }

        let deny_sid_strs = workspace_write_cap_sids_for_path(
            &payload.codex_home,
            &payload.command_cwd,
            &payload.write_roots,
            path,
        )?;
        for deny_sid_str in deny_sid_strs {
            let deny_psid = unsafe {
                convert_string_sid_to_sid(&deny_sid_str)
                    .ok_or_else(|| anyhow::anyhow!("convert deny capability SID failed"))?
            };

            match unsafe { add_deny_write_ace(path, deny_psid) } {
                Ok(true) => {
                    log_line(
                        log,
                        &format!("applied deny ACE to protect {}", path.display()),
                    )?;
                }
                Ok(false) => {}
                Err(err) => {
                    refresh_errors.push(format!("deny ACE failed on {}: {err}", path.display()));
                    log_line(
                        log,
                        &format!("deny ACE failed on {}: {err}", path.display()),
                    )?;
                }
            }
            unsafe {
                LocalFree(deny_psid as HLOCAL);
            }
        }
    }

    lock_sandbox_bin_dir(payload, &sandbox_group_sid)?;

    if refresh_only {
        log_line(
            log,
            &format!(
                "setup refresh: processed {} write roots (read roots delegated); errors={:?}",
                payload.write_roots.len(),
                refresh_errors
            ),
        )?;
    }
    if !refresh_only {
        lock_persistent_sandbox_dirs(payload, &sandbox_group_sid)?;
    }

    unsafe {
        if !sandbox_group_psid.is_null() {
            LocalFree(sandbox_group_psid as HLOCAL);
        }
    }
    if refresh_only && !refresh_errors.is_empty() {
        log_line(
            log,
            &format!("setup refresh completed with errors: {refresh_errors:?}"),
        )?;
        anyhow::bail!("setup refresh had errors");
    }
    log_note("setup binary completed", Some(sbx_dir));
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedFileIdentity {
    pub(crate) volume_serial_number: u32,
    pub(crate) file_index_high: u32,
    pub(crate) file_index_low: u32,
    pub(crate) is_directory: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedHomeIdentitiesSnapshot {
    pub(crate) codex_home_handle_id: OwnedFileIdentity,
    pub(crate) codex_home_path_id: OwnedFileIdentity,
    pub(crate) sandbox_dir_handle_id: Option<OwnedFileIdentity>,
    pub(crate) sandbox_dir_path_id: OwnedFileIdentity,
    pub(crate) sandbox_secrets_dir_handle_id: Option<OwnedFileIdentity>,
    pub(crate) sandbox_secrets_dir_path_id: OwnedFileIdentity,
    pub(crate) sandbox_bin_dir_handle_id: Option<OwnedFileIdentity>,
    pub(crate) sandbox_bin_dir_path_id: OwnedFileIdentity,
    pub(crate) setup_marker_id: OwnedFileIdentity,
    pub(crate) setup_marker_sha256: [u8; 32],
    pub(crate) sandbox_users_id: OwnedFileIdentity,
    pub(crate) sandbox_users_sha256: [u8; 32],
}

#[derive(Debug)]
pub(crate) struct OwnedSandboxHomeProof {
    pub(crate) codex_home: PathBuf,
    pub(crate) codex_home_handle: OwnedHandle,
    pub(crate) _codex_home_guard: OwnedHandle,
    pub(crate) codex_home_identity: OwnedFileIdentity,
    pub(crate) sandbox_dir: PathBuf,
    pub(crate) sandbox_dir_handle: Option<OwnedHandle>,
    pub(crate) sandbox_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_secrets_dir: PathBuf,
    pub(crate) sandbox_secrets_dir_handle: Option<OwnedHandle>,
    pub(crate) sandbox_secrets_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_bin_dir: PathBuf,
    pub(crate) sandbox_bin_dir_handle: Option<OwnedHandle>,
    pub(crate) sandbox_bin_dir_identity: OwnedFileIdentity,
    pub(crate) setup_marker_identity: OwnedFileIdentity,
    pub(crate) setup_marker_sha256: [u8; 32],
    pub(crate) sandbox_users_identity: OwnedFileIdentity,
    pub(crate) sandbox_users_sha256: [u8; 32],
    pub(crate) real_user: String,
    pub(crate) real_user_sid: String,
}

impl OwnedSandboxHomeProof {
    pub(crate) fn release_subdir_pins_for_removal(&mut self) {
        self.sandbox_dir_handle = None;
        self.sandbox_secrets_dir_handle = None;
        self.sandbox_bin_dir_handle = None;
    }
}

/// Parent-originated private process and nonce-binding proof for one executed
/// `codex-windows-sandbox-setup.exe` invocation. Non-`Clone` and non-`Deserialize`;
/// `Debug` redacts the private invocation nonce.
pub(crate) struct OwnedSetupProcessProof {
    pub(crate) setup_exe_path: PathBuf,
    pub(crate) setup_exe_sha256: [u8; 32],
    pub(crate) parent_pid: u32,
    pub(crate) child_pid: u32,
    pub(crate) exit_code: i32,
    pub(crate) invocation_nonce: [u8; 32],
    pub(crate) request_policy_sha256: [u8; 32],
    pub(crate) binding_sha256: [u8; 32],
}

impl std::fmt::Debug for OwnedSetupProcessProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnedSetupProcessProof")
            .field("setup_exe_path", &self.setup_exe_path)
            .field("setup_exe_sha256", &self.setup_exe_sha256)
            .field("parent_pid", &self.parent_pid)
            .field("child_pid", &self.child_pid)
            .field("exit_code", &self.exit_code)
            .field("invocation_nonce", &"<redacted>")
            .field("request_policy_sha256", &self.request_policy_sha256)
            .field("binding_sha256", &self.binding_sha256)
            .finish()
    }
}

/// Non-`Clone`, non-`Deserialize` record of the filesystem and network policy applied by
/// `codex-windows-sandbox-setup.exe` and verified by the parent process.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OwnedAppliedPolicyProof {
    pub(crate) command_cwd: PathBuf,
    pub(crate) read_roots: Vec<PathBuf>,
    pub(crate) write_roots: Vec<PathBuf>,
    pub(crate) deny_read_paths: Vec<PathBuf>,
    pub(crate) deny_write_paths: Vec<PathBuf>,
    pub(crate) proxy_ports: Vec<u16>,
    pub(crate) allow_local_binding: bool,
}

/// Non-deserializable, non-cloneable capability proof produced only by executing the
/// pinned `codex-windows-sandbox-setup.exe` helper in `SetupMode::OwnedFull` and
/// verifying its nonce-bound creation receipt and live OS objects in the parent process.
///
/// Carries the pinned helper process proof, applied filesystem/network policy proof,
/// exact created principal SIDs, created firewall rule proofs, created WFP filter
/// runtime `filterId`s and security descriptor blobs, hidden-user registry proofs,
/// and pinned directory/guard handles + file identities for `codex_home`.
#[derive(Debug)]
pub struct OwnedWindowsSandboxSetup {
    pub(crate) process: OwnedSetupProcessProof,
    pub(crate) policy: OwnedAppliedPolicyProof,
    pub(crate) home: OwnedSandboxHomeProof,
    pub(crate) principals: OwnedPrincipalsProof,
    pub(crate) hidden_users: OwnedHiddenUsersProof,
    pub(crate) firewall: OwnedFirewallProof,
    pub(crate) wfp: crate::wfp::OwnedWfpProof,
}

impl OwnedWindowsSandboxSetup {
    pub fn codex_home(&self) -> &Path {
        &self.home.codex_home
    }

    pub fn setup_exe_path(&self) -> &Path {
        &self.process.setup_exe_path
    }

    pub fn setup_exe_sha256(&self) -> [u8; 32] {
        self.process.setup_exe_sha256
    }

    pub fn setup_child_pid(&self) -> u32 {
        self.process.child_pid
    }

    pub fn request_policy_sha256(&self) -> [u8; 32] {
        self.process.request_policy_sha256
    }

    pub fn command_cwd(&self) -> &Path {
        &self.policy.command_cwd
    }

    pub fn read_roots(&self) -> &[PathBuf] {
        &self.policy.read_roots
    }

    pub fn write_roots(&self) -> &[PathBuf] {
        &self.policy.write_roots
    }

    pub fn deny_read_paths(&self) -> &[PathBuf] {
        &self.policy.deny_read_paths
    }

    pub fn deny_write_paths(&self) -> &[PathBuf] {
        &self.policy.deny_write_paths
    }

    pub fn proxy_ports(&self) -> &[u16] {
        &self.policy.proxy_ports
    }

    pub fn allow_local_binding(&self) -> bool {
        self.policy.allow_local_binding
    }

    pub fn offline_username(&self) -> &str {
        &self.principals.offline_user.username
    }

    pub fn offline_sid(&self) -> &str {
        &self.principals.offline_user.sid
    }

    pub fn online_username(&self) -> &str {
        &self.principals.online_user.username
    }

    pub fn online_sid(&self) -> &str {
        &self.principals.online_user.sid
    }

    pub fn group_name(&self) -> &str {
        &self.principals.group_name
    }

    pub fn group_sid(&self) -> &str {
        &self.principals.group_sid
    }

    pub fn firewall_rule_names(&self) -> Vec<&'static str> {
        self.firewall
            .rules
            .iter()
            .map(|r| r.internal_name)
            .collect()
    }

    pub fn wfp_filter_ids(&self) -> Vec<u64> {
        self.wfp.filters.iter().map(|f| f.filter_id).collect()
    }
}

fn file_identity_from_raw_handle(handle: HANDLE) -> Result<OwnedFileIdentity> {
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(handle, &mut info) };
    anyhow::ensure!(
        ok != 0,
        "GetFileInformationByHandle failed: {}",
        unsafe { GetLastError() }
    );
    anyhow::ensure!(
        (info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) == 0,
        "sandbox file or directory is a reparse point"
    );
    Ok(OwnedFileIdentity {
        volume_serial_number: info.dwVolumeSerialNumber,
        file_index_high: info.nFileIndexHigh,
        file_index_low: info.nFileIndexLow,
        is_directory: (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY) != 0,
    })
}

pub(crate) fn open_pinned_sandbox_directory(
    path: &Path,
    disposition: DirectoryOpenDisposition,
) -> Result<(OwnedHandle, OwnedFileIdentity)> {
    let handle = open_directory_no_reparse(
        path,
        FILE_READ_ATTRIBUTES | FILE_TRAVERSE | READ_CONTROL,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
        disposition,
    )?;
    let identity = file_identity_from_raw_handle(handle.as_raw_handle() as HANDLE)?;
    anyhow::ensure!(
        identity.is_directory,
        "expected directory at {}",
        path.display()
    );
    Ok((handle, identity))
}

fn regular_file_identity_and_sha256(path: &Path) -> Result<(OwnedFileIdentity, [u8; 32])> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(0x0020_0000) // FILE_FLAG_OPEN_REPARSE_POINT
        .open(path)
        .with_context(|| format!("open owned sandbox file {}", path.display()))?;
    let identity = file_identity_from_raw_handle(file.as_raw_handle() as HANDLE)?;
    anyhow::ensure!(
        !identity.is_directory,
        "expected regular file at {}, found directory",
        path.display()
    );
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok((identity, <[u8; 32]>::from(hasher.finalize())))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_owned_home_identities_plan(
    expected_home: OwnedFileIdentity,
    expected_sandbox: OwnedFileIdentity,
    expected_secrets: OwnedFileIdentity,
    expected_bin: OwnedFileIdentity,
    expected_marker: OwnedFileIdentity,
    expected_marker_sha256: [u8; 32],
    expected_users: OwnedFileIdentity,
    expected_users_sha256: [u8; 32],
    observed: &OwnedHomeIdentitiesSnapshot,
) -> Result<()> {
    anyhow::ensure!(
        expected_home.is_directory
            && expected_sandbox.is_directory
            && expected_secrets.is_directory
            && expected_bin.is_directory
            && !expected_marker.is_directory
            && !expected_users.is_directory,
        "owned home proof has invalid directory/file flags"
    );
    anyhow::ensure!(
        observed.codex_home_handle_id == expected_home
            && observed.codex_home_path_id == expected_home,
        "codex_home directory identity changed or was replaced"
    );
    anyhow::ensure!(
        observed.sandbox_dir_handle_id == Some(expected_sandbox)
            && observed.sandbox_dir_path_id == expected_sandbox,
        ".sandbox directory pin missing or directory was replaced"
    );
    anyhow::ensure!(
        observed.sandbox_secrets_dir_handle_id == Some(expected_secrets)
            && observed.sandbox_secrets_dir_path_id == expected_secrets,
        ".sandbox-secrets directory pin missing or directory was replaced"
    );
    anyhow::ensure!(
        observed.sandbox_bin_dir_handle_id == Some(expected_bin)
            && observed.sandbox_bin_dir_path_id == expected_bin,
        ".sandbox-bin directory pin missing or directory was replaced"
    );
    anyhow::ensure!(
        observed.setup_marker_id == expected_marker
            && observed.setup_marker_sha256 == expected_marker_sha256,
        "setup_marker.json identity or digest changed since owned setup"
    );
    anyhow::ensure!(
        observed.sandbox_users_id == expected_users
            && observed.sandbox_users_sha256 == expected_users_sha256,
        "sandbox_users.json identity or digest changed since owned setup"
    );
    Ok(())
}

pub(crate) fn verify_owned_home_continuity(home: &OwnedSandboxHomeProof) -> Result<()> {
    let codex_home_handle_id =
        file_identity_from_raw_handle(home.codex_home_handle.as_raw_handle() as HANDLE)?;
    let (_home_reopen, codex_home_path_id) =
        open_pinned_sandbox_directory(&home.codex_home, DirectoryOpenDisposition::OpenExisting)?;

    let sandbox_dir_handle_id = match home.sandbox_dir_handle.as_ref() {
        Some(h) => Some(file_identity_from_raw_handle(h.as_raw_handle() as HANDLE)?),
        None => None,
    };
    let (_sbx_reopen, sandbox_dir_path_id) =
        open_pinned_sandbox_directory(&home.sandbox_dir, DirectoryOpenDisposition::OpenExisting)?;

    let sandbox_secrets_dir_handle_id = match home.sandbox_secrets_dir_handle.as_ref() {
        Some(h) => Some(file_identity_from_raw_handle(h.as_raw_handle() as HANDLE)?),
        None => None,
    };
    let (_secrets_reopen, sandbox_secrets_dir_path_id) = open_pinned_sandbox_directory(
        &home.sandbox_secrets_dir,
        DirectoryOpenDisposition::OpenExisting,
    )?;

    let sandbox_bin_dir_handle_id = match home.sandbox_bin_dir_handle.as_ref() {
        Some(h) => Some(file_identity_from_raw_handle(h.as_raw_handle() as HANDLE)?),
        None => None,
    };
    let (_bin_reopen, sandbox_bin_dir_path_id) = open_pinned_sandbox_directory(
        &home.sandbox_bin_dir,
        DirectoryOpenDisposition::OpenExisting,
    )?;

    let (setup_marker_id, setup_marker_sha256) =
        regular_file_identity_and_sha256(&crate::setup::setup_marker_path(&home.codex_home))?;
    let (sandbox_users_id, sandbox_users_sha256) =
        regular_file_identity_and_sha256(&crate::setup::sandbox_users_path(&home.codex_home))?;

    let current_real_sid = resolve_sid(&home.real_user)
        .and_then(|bytes| string_from_sid_bytes(&bytes).map_err(anyhow::Error::msg))?;
    anyhow::ensure!(
        current_real_sid == home.real_user_sid,
        "owned setup real user SID changed ({current_real_sid} != {})",
        home.real_user_sid
    );

    let snapshot = OwnedHomeIdentitiesSnapshot {
        codex_home_handle_id,
        codex_home_path_id,
        sandbox_dir_handle_id,
        sandbox_dir_path_id,
        sandbox_secrets_dir_handle_id,
        sandbox_secrets_dir_path_id,
        sandbox_bin_dir_handle_id,
        sandbox_bin_dir_path_id,
        setup_marker_id,
        setup_marker_sha256,
        sandbox_users_id,
        sandbox_users_sha256,
    };
    verify_owned_home_identities_plan(
        home.codex_home_identity,
        home.sandbox_dir_identity,
        home.sandbox_secrets_dir_identity,
        home.sandbox_bin_dir_identity,
        home.setup_marker_identity,
        home.setup_marker_sha256,
        home.sandbox_users_identity,
        home.sandbox_users_sha256,
        &snapshot,
    )
}

fn update_hasher_str(hasher: &mut Sha256, value: &str) {
    let bytes = value.as_bytes();
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

fn update_hasher_path(hasher: &mut Sha256, path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    let wide = path.as_os_str().encode_wide().collect::<Vec<u16>>();
    hasher.update((wide.len() as u64).to_le_bytes());
    for unit in wide {
        hasher.update(unit.to_le_bytes());
    }
}

fn update_hasher_paths(hasher: &mut Sha256, paths: &[PathBuf]) {
    hasher.update((paths.len() as u64).to_le_bytes());
    for path in paths {
        update_hasher_path(hasher, path);
    }
}

fn update_hasher_file_identity(hasher: &mut Sha256, id: OwnedFileIdentity) {
    hasher.update(id.volume_serial_number.to_le_bytes());
    hasher.update(id.file_index_high.to_le_bytes());
    hasher.update(id.file_index_low.to_le_bytes());
    hasher.update([u8::from(id.is_directory)]);
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_owned_setup_policy_sha256(
    version: u32,
    offline_username: &str,
    online_username: &str,
    codex_home: &Path,
    command_cwd: &Path,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
    deny_read_paths: &[PathBuf],
    deny_write_paths: &[PathBuf],
    proxy_ports: &[u16],
    allow_local_binding: bool,
    real_user: &str,
    real_user_sid: &str,
    codex_home_identity: OwnedFileIdentity,
    sandbox_dir_identity: OwnedFileIdentity,
    sandbox_secrets_dir_identity: OwnedFileIdentity,
    sandbox_bin_dir_identity: OwnedFileIdentity,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"CODEX_OWNED_SETUP_POLICY_V1\0");
    hasher.update(version.to_le_bytes());
    update_hasher_str(&mut hasher, offline_username);
    update_hasher_str(&mut hasher, online_username);
    update_hasher_path(&mut hasher, codex_home);
    update_hasher_path(&mut hasher, command_cwd);
    update_hasher_paths(&mut hasher, read_roots);
    update_hasher_paths(&mut hasher, write_roots);
    update_hasher_paths(&mut hasher, deny_read_paths);
    update_hasher_paths(&mut hasher, deny_write_paths);
    hasher.update((proxy_ports.len() as u64).to_le_bytes());
    for port in proxy_ports {
        hasher.update(port.to_le_bytes());
    }
    hasher.update([u8::from(allow_local_binding)]);
    update_hasher_str(&mut hasher, real_user);
    update_hasher_str(&mut hasher, real_user_sid);
    update_hasher_file_identity(&mut hasher, codex_home_identity);
    update_hasher_file_identity(&mut hasher, sandbox_dir_identity);
    update_hasher_file_identity(&mut hasher, sandbox_secrets_dir_identity);
    update_hasher_file_identity(&mut hasher, sandbox_bin_dir_identity);
    <[u8; 32]>::from(hasher.finalize())
}

pub(crate) fn decode_owned_setup_nonce(nonce_b64: &str) -> Result<[u8; 32]> {
    let raw = BASE64
        .decode(nonce_b64)
        .context("decode owned setup invocation nonce")?;
    let nonce: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("owned setup invocation nonce must be 32 bytes"))?;
    anyhow::ensure!(
        nonce != [0_u8; 32],
        "owned setup invocation nonce must not be all zeros"
    );
    Ok(nonce)
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedUserPrincipalReceiptWire {
    pub(crate) username: String,
    pub(crate) sid: String,
    pub(crate) sid_bytes: Vec<u8>,
    pub(crate) initial_flags: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedPrincipalsReceiptWire {
    pub(crate) offline_user: OwnedUserPrincipalReceiptWire,
    pub(crate) online_user: OwnedUserPrincipalReceiptWire,
    pub(crate) group_name: String,
    pub(crate) group_sid: String,
    pub(crate) group_sid_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedHiddenUsersReceiptWire {
    pub(crate) key_path: String,
    pub(crate) usernames: [String; 2],
    pub(crate) expected_dword_value: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedFirewallRuleReceiptWire {
    pub(crate) internal_name: String,
    pub(crate) friendly_desc: String,
    pub(crate) direction: i32,
    pub(crate) action: i32,
    pub(crate) enabled: bool,
    pub(crate) profiles: i32,
    pub(crate) protocol: i32,
    pub(crate) local_user_authorized_list: String,
    pub(crate) offline_sid: String,
    pub(crate) remote_addresses: String,
    pub(crate) remote_ports: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedFirewallReceiptWire {
    pub(crate) offline_sid: String,
    pub(crate) rules: Vec<OwnedFirewallRuleReceiptWire>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedWfpFilterReceiptWire {
    pub(crate) name: String,
    pub(crate) filter_id: u64,
    pub(crate) expected_sd_blob: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedWfpReceiptWire {
    pub(crate) provider_created: bool,
    pub(crate) sublayer_created: bool,
    pub(crate) offline_sid: String,
    pub(crate) offline_sid_bytes: Vec<u8>,
    pub(crate) filters: Vec<OwnedWfpFilterReceiptWire>,
}

/// Wire creation report emitted on stdout by `codex-windows-sandbox-setup.exe` in
/// `SetupMode::OwnedFull`.
///
/// This wire struct is NEVER authority by itself; it has no conversion into
/// [`OwnedWindowsSandboxSetup`] without parent-side private nonce verification,
/// pinned child process verification, pinned directory handles, and independent
/// live OS verification of every created object and applied ACL.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct OwnedSetupReceiptWire {
    pub(crate) schema_version: u32,
    pub(crate) parent_pid: u32,
    pub(crate) helper_pid: u32,
    pub(crate) setup_exe_sha256: [u8; 32],
    pub(crate) request_policy_sha256: [u8; 32],
    pub(crate) codex_home_identity: OwnedFileIdentity,
    pub(crate) sandbox_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_secrets_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_bin_dir_identity: OwnedFileIdentity,
    pub(crate) setup_marker_identity: OwnedFileIdentity,
    pub(crate) setup_marker_sha256: [u8; 32],
    pub(crate) sandbox_users_identity: OwnedFileIdentity,
    pub(crate) sandbox_users_sha256: [u8; 32],
    pub(crate) principals: OwnedPrincipalsReceiptWire,
    pub(crate) hidden_users: OwnedHiddenUsersReceiptWire,
    pub(crate) firewall: OwnedFirewallReceiptWire,
    pub(crate) wfp: OwnedWfpReceiptWire,
    pub(crate) binding_sha256: [u8; 32],
}

pub(crate) fn compute_owned_setup_receipt_binding_sha256(
    nonce: &[u8; 32],
    receipt: &OwnedSetupReceiptWire,
) -> Result<[u8; 32]> {
    let mut unbound = receipt.clone();
    unbound.binding_sha256 = [0_u8; 32];
    let serialized = serde_json::to_vec(&unbound)?;
    let mut hasher = Sha256::new();
    hasher.update(b"CODEX_OWNED_SETUP_RECEIPT_BINDING_V1\0");
    hasher.update(nonce);
    hasher.update((serialized.len() as u64).to_le_bytes());
    hasher.update(&serialized);
    Ok(<[u8; 32]>::from(hasher.finalize()))
}

fn apply_owned_full_filesystem_policy(
    payload: &Payload,
    sandbox_group_sid: &[u8],
    sandbox_group_sid_str: &str,
    log: &mut dyn Write,
) -> Result<()> {
    let sandbox_group_psid = sid_bytes_to_psid(sandbox_group_sid).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSidResolveFailed,
            format!("convert sandbox users group SID to PSID failed: {err}"),
        ))
    })?;
    let result = (|| -> Result<()> {
        let applied_deny_read_paths = unsafe {
            sync_persistent_deny_read_acls(
                &payload.codex_home,
                sandbox_group_sid_str,
                &payload.deny_read_paths,
                sandbox_group_psid,
            )
        }
        .context("apply owned-full deny-read ACLs")?;
        if !applied_deny_read_paths.is_empty() {
            log_line(
                log,
                &format!(
                    "owned-full applied {} deny-read ACLs",
                    applied_deny_read_paths.len()
                ),
            )?;
        }

        {
            let _read_acl_guard = acquire_read_acl_mutex()?
                .ok_or_else(|| anyhow::anyhow!("read ACL mutex is already held during owned-full setup"))?;
            if !payload.read_roots.is_empty() {
                let users_sid = resolve_sid("Users")?;
                let users_psid = sid_bytes_to_psid(&users_sid)?;
                let auth_sid = resolve_sid("Authenticated Users")?;
                let auth_psid = sid_bytes_to_psid(&auth_sid)?;
                let everyone_sid = resolve_sid("Everyone")?;
                let everyone_psid = sid_bytes_to_psid(&everyone_sid)?;
                let rx_psids = vec![users_psid, auth_psid, everyone_psid];
                let subjects = ReadAclSubjects {
                    sandbox_group_psid,
                    rx_psids: &rx_psids,
                };
                let mut refresh_errors = Vec::new();
                let read_res = apply_read_acls(
                    &payload.read_roots,
                    &subjects,
                    log,
                    &mut refresh_errors,
                    FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                    "read",
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                );
                unsafe {
                    if !users_psid.is_null() {
                        LocalFree(users_psid as HLOCAL);
                    }
                    if !auth_psid.is_null() {
                        LocalFree(auth_psid as HLOCAL);
                    }
                    if !everyone_psid.is_null() {
                        LocalFree(everyone_psid as HLOCAL);
                    }
                }
                read_res?;
                anyhow::ensure!(
                    refresh_errors.is_empty(),
                    "owned-full read ACL application had errors: {refresh_errors:?}"
                );
            }
        }

        let mut seen_write_roots: HashSet<PathBuf> = HashSet::new();
        for root in &payload.write_roots {
            if !seen_write_roots.insert(root.clone()) {
                continue;
            }
            if !root.exists() {
                log_line(
                    log,
                    &format!("owned-full write root {} missing; skipping", root.display()),
                )?;
                continue;
            }
            let root_cap_sid_str =
                workspace_write_cap_sid_for_root(&payload.codex_home, &payload.command_cwd, root)?;
            let root_cap_psid = unsafe {
                convert_string_sid_to_sid(&root_cap_sid_str)
                    .ok_or_else(|| anyhow::anyhow!("convert write root capability SID failed"))?
            };
            let grant_res = (|| -> Result<()> {
                let need_grant =
                    path_write_aces_need_refresh(root, &[sandbox_group_psid, root_cap_psid])
                        .with_context(|| {
                            format!("check write ACEs on {}", root.display())
                        })?;
                if need_grant {
                    log_line(
                        log,
                        &format!(
                            "owned-full granting write ACE to {} for sandbox group and capability SID",
                            root.display()
                        ),
                    )?;
                    unsafe { ensure_allow_write_aces(root, &[sandbox_group_psid, root_cap_psid]) }
                        .with_context(|| {
                            format!("grant write ACEs on {}", root.display())
                        })?;
                }
                Ok(())
            })();
            unsafe {
                LocalFree(root_cap_psid as HLOCAL);
            }
            grant_res?;
        }

        let mut seen_deny_paths: HashSet<PathBuf> = HashSet::new();
        for path in &payload.deny_write_paths {
            if !seen_deny_paths.insert(path.clone()) {
                continue;
            }
            if !path.exists() {
                std::fs::create_dir_all(path).with_context(|| {
                    format!("failed to create owned-full deny-write path {}", path.display())
                })?;
            }
            let deny_sid_strs = workspace_write_cap_sids_for_path(
                &payload.codex_home,
                &payload.command_cwd,
                &payload.write_roots,
                path,
            )?;
            for deny_sid_str in deny_sid_strs {
                let deny_psid = unsafe {
                    convert_string_sid_to_sid(&deny_sid_str)
                        .ok_or_else(|| anyhow::anyhow!("convert deny capability SID failed"))?
                };
                let add_res = unsafe { add_deny_write_ace(path, deny_psid) };
                unsafe {
                    LocalFree(deny_psid as HLOCAL);
                }
                if add_res.with_context(|| format!("apply deny-write ACE on {}", path.display()))? {
                    log_line(
                        log,
                        &format!("owned-full applied deny ACE to protect {}", path.display()),
                    )?;
                }
            }
        }
        Ok(())
    })();
    unsafe {
        if !sandbox_group_psid.is_null() {
            LocalFree(sandbox_group_psid as HLOCAL);
        }
    }
    result
}

fn run_setup_owned_full(
    payload: &Payload,
    log: &mut dyn Write,
    sbx_dir: &Path,
    marker: sandbox_users::PreparedSetupMarker,
) -> Result<()> {
    let _setup_lock = acquire_sandbox_setup_lock(INFINITE)?;
    anyhow::ensure!(
        !payload.refresh_only,
        "owned-full setup mode does not allow refresh_only"
    );
    anyhow::ensure!(
        payload.runtime == SetupRuntime::Legacy && !crate::registered_core_requested(),
        "owned-full setup mode is not permitted when registered Core is active"
    );
    anyhow::ensure!(
        crate::runtime_ownership::load_installation()?.is_none(),
        "owned-full setup mode requires no existing installation registration"
    );
    anyhow::ensure!(
        !read_acl_mutex_exists()?,
        "owned-full setup mode cannot start while Local\\CodexSandboxReadAcl is active"
    );
    let invocation = payload.owned_invocation.as_ref().ok_or_else(|| {
        anyhow::anyhow!("owned-full setup mode requires owned_invocation binding")
    })?;
    anyhow::ensure!(
        invocation.schema_version == OWNED_SETUP_PROTOCOL_SCHEMA_V1,
        "owned-full setup invocation schema mismatch: expected {OWNED_SETUP_PROTOCOL_SCHEMA_V1}, got {}",
        invocation.schema_version
    );
    anyhow::ensure!(
        invocation.parent_pid != 0,
        "owned-full setup invocation parent_pid must be non-zero"
    );
    let nonce = decode_owned_setup_nonce(&invocation.nonce_b64)?;

    let current_exe = std::env::current_exe().context("resolve current setup helper exe")?;
    let (_exe_id, current_exe_sha256) = regular_file_identity_and_sha256(&current_exe)?;
    anyhow::ensure!(
        current_exe_sha256 == invocation.expected_setup_exe_sha256,
        "running setup helper executable digest does not match pinned setup artifact"
    );

    let real_user_sid_bytes = resolve_sid(&payload.real_user)?;
    let real_user_sid = string_from_sid_bytes(&real_user_sid_bytes).map_err(anyhow::Error::msg)?;
    anyhow::ensure!(
        real_user_sid == invocation.real_user_sid,
        "owned-full real_user SID mismatch ({real_user_sid} != {})",
        invocation.real_user_sid
    );

    let (_home_h, home_id) =
        open_pinned_sandbox_directory(&payload.codex_home, DirectoryOpenDisposition::OpenExisting)?;
    let (_sbx_h, sbx_id) =
        open_pinned_sandbox_directory(sbx_dir, DirectoryOpenDisposition::OpenExisting)?;
    let secrets_dir = sandbox_secrets_dir(&payload.codex_home);
    let (_sec_h, secrets_id) =
        open_pinned_sandbox_directory(&secrets_dir, DirectoryOpenDisposition::OpenExisting)?;
    let bin_dir = sandbox_bin_dir(&payload.codex_home);
    let (_bin_h, bin_id) =
        open_pinned_sandbox_directory(&bin_dir, DirectoryOpenDisposition::OpenExisting)?;
    anyhow::ensure!(
        home_id == invocation.codex_home_identity
            && sbx_id == invocation.sandbox_dir_identity
            && secrets_id == invocation.sandbox_secrets_dir_identity
            && bin_id == invocation.sandbox_bin_dir_identity,
        "owned-full sandbox directory identities do not match parent-pinned directories"
    );

    let recomputed_policy_sha256 = compute_owned_setup_policy_sha256(
        payload.version,
        &payload.offline_username,
        &payload.online_username,
        &payload.codex_home,
        &payload.command_cwd,
        &payload.read_roots,
        &payload.write_roots,
        &payload.deny_read_paths,
        &payload.deny_write_paths,
        &payload.proxy_ports,
        payload.allow_local_binding,
        &payload.real_user,
        &real_user_sid,
        home_id,
        sbx_id,
        secrets_id,
        bin_id,
    );
    anyhow::ensure!(
        recomputed_policy_sha256 == invocation.request_policy_sha256,
        "owned-full request_policy_sha256 mismatch"
    );

    let (principals, hidden_users) = provision_owned_sandbox_users(
        &payload.codex_home,
        &payload.offline_username,
        &payload.online_username,
        log,
    )?;
    let firewall = firewall::provision_owned_offline_firewall_rules(
        &principals.offline_user.sid,
        &payload.proxy_ports,
        payload.allow_local_binding,
        log,
    )?;
    let wfp = crate::wfp::install_owned_wfp_filters_for_account(
        &principals.offline_user.username,
        &principals.offline_user.sid,
        &principals.offline_user.sid_bytes,
    )?;

    apply_owned_full_filesystem_policy(
        payload,
        &principals.group_sid_bytes,
        &principals.group_sid,
        log,
    )?;
    lock_sandbox_bin_dir(payload, &principals.group_sid_bytes)?;
    lock_persistent_sandbox_dirs(payload, &principals.group_sid_bytes)?;

    commit_setup_marker_with_roots(
        marker,
        &payload.codex_home,
        &payload.offline_username,
        &payload.online_username,
        &payload.proxy_ports,
        payload.allow_local_binding,
        &payload.read_roots,
        &payload.write_roots,
    )?;
    log_note("owned-full setup binary completed", Some(sbx_dir));
    let _ = crate::logging::release_setup_log();
    let _ = crate::setup_error::clear_setup_error_report(&payload.codex_home);

    let (setup_marker_identity, setup_marker_sha256) =
        regular_file_identity_and_sha256(&crate::setup::setup_marker_path(&payload.codex_home))?;
    let (sandbox_users_identity, sandbox_users_sha256) =
        regular_file_identity_and_sha256(&crate::setup::sandbox_users_path(&payload.codex_home))?;

    let helper_pid = std::process::id();
    let mut receipt = OwnedSetupReceiptWire {
        schema_version: OWNED_SETUP_PROTOCOL_SCHEMA_V1,
        parent_pid: invocation.parent_pid,
        helper_pid,
        setup_exe_sha256: current_exe_sha256,
        request_policy_sha256: recomputed_policy_sha256,
        codex_home_identity: home_id,
        sandbox_dir_identity: sbx_id,
        sandbox_secrets_dir_identity: secrets_id,
        sandbox_bin_dir_identity: bin_id,
        setup_marker_identity,
        setup_marker_sha256,
        sandbox_users_identity,
        sandbox_users_sha256,
        principals: OwnedPrincipalsReceiptWire {
            offline_user: OwnedUserPrincipalReceiptWire {
                username: principals.offline_user.username,
                sid: principals.offline_user.sid,
                sid_bytes: principals.offline_user.sid_bytes,
                initial_flags: principals.offline_user.initial_flags,
            },
            online_user: OwnedUserPrincipalReceiptWire {
                username: principals.online_user.username,
                sid: principals.online_user.sid,
                sid_bytes: principals.online_user.sid_bytes,
                initial_flags: principals.online_user.initial_flags,
            },
            group_name: principals.group_name,
            group_sid: principals.group_sid,
            group_sid_bytes: principals.group_sid_bytes,
        },
        hidden_users: OwnedHiddenUsersReceiptWire {
            key_path: hidden_users.key_path.to_string(),
            usernames: hidden_users.usernames,
            expected_dword_value: hidden_users.expected_dword_value,
        },
        firewall: OwnedFirewallReceiptWire {
            offline_sid: firewall.offline_sid,
            rules: firewall
                .rules
                .into_iter()
                .map(|r| OwnedFirewallRuleReceiptWire {
                    internal_name: r.internal_name.to_string(),
                    friendly_desc: r.friendly_desc,
                    direction: r.direction,
                    action: r.action,
                    enabled: r.enabled,
                    profiles: r.profiles,
                    protocol: r.protocol,
                    local_user_authorized_list: r.local_user_authorized_list,
                    offline_sid: r.offline_sid,
                    remote_addresses: r.remote_addresses,
                    remote_ports: r.remote_ports,
                })
                .collect(),
        },
        wfp: OwnedWfpReceiptWire {
            provider_created: wfp.provider_created,
            sublayer_created: wfp.sublayer_created,
            offline_sid: wfp.offline_sid,
            offline_sid_bytes: wfp.offline_sid_bytes,
            filters: wfp
                .filters
                .into_iter()
                .map(|f| OwnedWfpFilterReceiptWire {
                    name: f.name.to_string(),
                    filter_id: f.filter_id,
                    expected_sd_blob: f.expected_sd_blob,
                })
                .collect(),
        },
        binding_sha256: [0_u8; 32],
    };
    receipt.binding_sha256 = compute_owned_setup_receipt_binding_sha256(&nonce, &receipt)?;
    let receipt_json = serde_json::to_vec(&receipt)?;
    let mut stdout = std::io::stdout().lock();
    writeln!(
        stdout,
        "{OWNED_SETUP_RECEIPT_PREFIX_V1}{}",
        BASE64.encode(receipt_json)
    )?;
    stdout.flush()?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn extract_and_verify_owned_setup_receipt(
    stdout_bytes: &[u8],
    exit_code: Option<i32>,
    expected_nonce: &[u8; 32],
    expected_parent_pid: u32,
    expected_child_pid: u32,
    expected_setup_exe_sha256: &[u8; 32],
    expected_policy_sha256: &[u8; 32],
    expected_home_id: OwnedFileIdentity,
    expected_sandbox_id: OwnedFileIdentity,
    expected_secrets_id: OwnedFileIdentity,
    expected_bin_id: OwnedFileIdentity,
) -> Result<(
    OwnedSetupReceiptWire,
    OwnedPrincipalsProof,
    OwnedHiddenUsersProof,
    OwnedFirewallProof,
)> {
    let code = exit_code.ok_or_else(|| {
        anyhow::anyhow!("owned setup helper terminated without an exit code")
    })?;
    anyhow::ensure!(
        code == 0,
        "owned setup helper exited with non-zero status {code}"
    );
    anyhow::ensure!(
        *expected_nonce != [0_u8; 32],
        "parent invocation nonce must not be all zeros"
    );
    anyhow::ensure!(
        expected_parent_pid != 0 && expected_child_pid != 0,
        "expected parent and child PIDs must be non-zero"
    );

    let stdout_str = std::str::from_utf8(stdout_bytes)
        .context("owned setup helper stdout is not valid UTF-8")?;
    let mut receipt_b64: Option<&str> = None;
    for line in stdout_str.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix(OWNED_SETUP_RECEIPT_PREFIX_V1) {
            anyhow::ensure!(
                receipt_b64.is_none(),
                "owned setup helper emitted multiple receipt lines"
            );
            receipt_b64 = Some(rest);
        } else {
            anyhow::bail!("owned setup helper emitted unexpected stdout line");
        }
    }
    let encoded = receipt_b64.ok_or_else(|| {
        anyhow::anyhow!("owned setup helper did not emit a creation receipt on stdout")
    })?;
    let decoded_bytes = BASE64
        .decode(encoded)
        .context("decode owned setup helper receipt base64")?;
    let receipt: OwnedSetupReceiptWire = serde_json::from_slice(&decoded_bytes)
        .context("parse owned setup helper receipt JSON")?;

    anyhow::ensure!(
        receipt.schema_version == OWNED_SETUP_PROTOCOL_SCHEMA_V1,
        "owned setup receipt schema mismatch: expected {OWNED_SETUP_PROTOCOL_SCHEMA_V1}, got {}",
        receipt.schema_version
    );
    anyhow::ensure!(
        receipt.parent_pid == expected_parent_pid,
        "owned setup receipt parent_pid mismatch ({} != {expected_parent_pid})",
        receipt.parent_pid
    );
    anyhow::ensure!(
        receipt.helper_pid == expected_child_pid,
        "owned setup receipt helper_pid mismatch ({} != {expected_child_pid})",
        receipt.helper_pid
    );
    anyhow::ensure!(
        &receipt.setup_exe_sha256 == expected_setup_exe_sha256,
        "owned setup receipt setup_exe_sha256 mismatch"
    );
    anyhow::ensure!(
        &receipt.request_policy_sha256 == expected_policy_sha256,
        "owned setup receipt request_policy_sha256 mismatch"
    );
    anyhow::ensure!(
        receipt.codex_home_identity == expected_home_id
            && receipt.sandbox_dir_identity == expected_sandbox_id
            && receipt.sandbox_secrets_dir_identity == expected_secrets_id
            && receipt.sandbox_bin_dir_identity == expected_bin_id,
        "owned setup receipt directory identities do not match parent-pinned directories"
    );

    let expected_binding = compute_owned_setup_receipt_binding_sha256(expected_nonce, &receipt)?;
    anyhow::ensure!(
        receipt.binding_sha256 == expected_binding,
        "owned setup receipt nonce binding_sha256 verification failed"
    );

    anyhow::ensure!(
        receipt.principals.offline_user.username == crate::setup::OFFLINE_USERNAME
            && receipt.principals.online_user.username == crate::setup::ONLINE_USERNAME
            && receipt.principals.group_name == crate::SANDBOX_USERS_GROUP,
        "owned setup receipt principal names do not match canonical sandbox accounts"
    );
    anyhow::ensure!(
        !receipt.principals.offline_user.sid.is_empty()
            && !receipt.principals.offline_user.sid_bytes.is_empty()
            && !receipt.principals.online_user.sid.is_empty()
            && !receipt.principals.online_user.sid_bytes.is_empty()
            && !receipt.principals.group_sid.is_empty()
            && !receipt.principals.group_sid_bytes.is_empty(),
        "owned setup receipt is missing principal SID identities"
    );
    anyhow::ensure!(
        receipt.principals.offline_user.sid != receipt.principals.online_user.sid
            && receipt.principals.offline_user.sid != receipt.principals.group_sid
            && receipt.principals.online_user.sid != receipt.principals.group_sid,
        "owned setup receipt principal SIDs must be distinct"
    );
    anyhow::ensure!(
        receipt.principals.offline_user.sid == receipt.firewall.offline_sid
            && receipt.principals.offline_user.sid == receipt.wfp.offline_sid
            && receipt.principals.offline_user.sid_bytes == receipt.wfp.offline_sid_bytes,
        "owned setup receipt offline SID is inconsistent across principals, firewall, and WFP"
    );
    anyhow::ensure!(
        receipt.hidden_users.key_path == WINLOGON_USERLIST_KEY_PATH
            && receipt.hidden_users.usernames[0] == crate::setup::OFFLINE_USERNAME
            && receipt.hidden_users.usernames[1] == crate::setup::ONLINE_USERNAME
            && receipt.hidden_users.expected_dword_value == 0,
        "owned setup receipt hidden-user proof is invalid"
    );
    anyhow::ensure!(
        !receipt.firewall.rules.is_empty(),
        "owned setup receipt must include created firewall rules"
    );

    let mut firewall_rules = Vec::with_capacity(receipt.firewall.rules.len());
    for rule in &receipt.firewall.rules {
        let canonical_name = firewall::canonical_owned_firewall_rule_name(&rule.internal_name)?;
        anyhow::ensure!(
            rule.offline_sid == receipt.principals.offline_user.sid
                && rule
                    .local_user_authorized_list
                    .contains(&receipt.principals.offline_user.sid),
            "owned setup receipt firewall rule {} has mismatched offline SID",
            rule.internal_name
        );
        firewall_rules.push(OwnedFirewallRuleProof {
            internal_name: canonical_name,
            friendly_desc: rule.friendly_desc.clone(),
            direction: rule.direction,
            action: rule.action,
            enabled: rule.enabled,
            profiles: rule.profiles,
            protocol: rule.protocol,
            local_user_authorized_list: rule.local_user_authorized_list.clone(),
            offline_sid: rule.offline_sid.clone(),
            remote_addresses: rule.remote_addresses.clone(),
            remote_ports: rule.remote_ports.clone(),
        });
    }

    let principals = OwnedPrincipalsProof {
        offline_user: OwnedUserPrincipalProof {
            username: receipt.principals.offline_user.username.clone(),
            sid: receipt.principals.offline_user.sid.clone(),
            sid_bytes: receipt.principals.offline_user.sid_bytes.clone(),
            initial_flags: receipt.principals.offline_user.initial_flags,
        },
        online_user: OwnedUserPrincipalProof {
            username: receipt.principals.online_user.username.clone(),
            sid: receipt.principals.online_user.sid.clone(),
            sid_bytes: receipt.principals.online_user.sid_bytes.clone(),
            initial_flags: receipt.principals.online_user.initial_flags,
        },
        group_name: receipt.principals.group_name.clone(),
        group_sid: receipt.principals.group_sid.clone(),
        group_sid_bytes: receipt.principals.group_sid_bytes.clone(),
    };
    let hidden_users = OwnedHiddenUsersProof {
        key_path: WINLOGON_USERLIST_KEY_PATH,
        usernames: receipt.hidden_users.usernames.clone(),
        expected_dword_value: receipt.hidden_users.expected_dword_value,
    };
    let firewall = OwnedFirewallProof {
        offline_sid: receipt.firewall.offline_sid.clone(),
        rules: firewall_rules,
    };

    Ok((receipt, principals, hidden_users, firewall))
}

fn verify_owned_filesystem_policy_applied(
    codex_home: &Path,
    policy: &OwnedAppliedPolicyProof,
    sandbox_group_sid: &[u8],
) -> Result<()> {
    anyhow::ensure!(
        !read_acl_mutex_exists()?,
        "Local\\CodexSandboxReadAcl is still active after owned-full setup helper exited"
    );
    let sandbox_group_psid = sid_bytes_to_psid(sandbox_group_sid)?;
    let users_sid = resolve_sid("Users")?;
    let users_psid = sid_bytes_to_psid(&users_sid)?;
    let auth_sid = resolve_sid("Authenticated Users")?;
    let auth_psid = sid_bytes_to_psid(&auth_sid)?;
    let everyone_sid = resolve_sid("Everyone")?;
    let everyone_psid = sid_bytes_to_psid(&everyone_sid)?;

    let verify_res = (|| -> Result<()> {
        let rx_psids = [users_psid, auth_psid, everyone_psid, sandbox_group_psid];
        for root in &policy.read_roots {
            if !root.exists() {
                continue;
            }
            let readable = path_mask_allows(
                root,
                &rx_psids,
                FILE_GENERIC_READ | FILE_GENERIC_EXECUTE,
                /*require_all_bits*/ true,
            )?;
            anyhow::ensure!(
                readable,
                "owned setup read root {} does not grant read/execute access to sandbox users after setup.exe completion",
                root.display()
            );
        }

        let mut seen_write_roots = HashSet::new();
        for root in &policy.write_roots {
            if !seen_write_roots.insert(root.clone()) || !root.exists() {
                continue;
            }
            let root_cap_sid_str =
                workspace_write_cap_sid_for_root(codex_home, &policy.command_cwd, root)?;
            let root_cap_psid = unsafe {
                convert_string_sid_to_sid(&root_cap_sid_str)
                    .ok_or_else(|| anyhow::anyhow!("convert write root capability SID failed"))?
            };
            let needs_refresh =
                path_write_aces_need_refresh(root, &[sandbox_group_psid, root_cap_psid]);
            unsafe {
                LocalFree(root_cap_psid as HLOCAL);
            }
            anyhow::ensure!(
                !needs_refresh?,
                "owned setup write root {} is missing required write ACEs after setup.exe completion",
                root.display()
            );
        }

        for path in &policy.deny_write_paths {
            anyhow::ensure!(
                path.exists(),
                "owned setup deny-write path {} was not materialized by setup.exe",
                path.display()
            );
        }
        Ok(())
    })();

    unsafe {
        if !users_psid.is_null() {
            LocalFree(users_psid as HLOCAL);
        }
        if !auth_psid.is_null() {
            LocalFree(auth_psid as HLOCAL);
        }
        if !everyone_psid.is_null() {
            LocalFree(everyone_psid as HLOCAL);
        }
        if !sandbox_group_psid.is_null() {
            LocalFree(sandbox_group_psid as HLOCAL);
        }
    }
    verify_res
}

pub(crate) struct PinnedOwnedSetupDirectories {
    pub(crate) codex_home: PathBuf,
    pub(crate) codex_home_handle: OwnedHandle,
    pub(crate) codex_home_guard: OwnedHandle,
    pub(crate) codex_home_identity: OwnedFileIdentity,
    pub(crate) sandbox_dir: PathBuf,
    pub(crate) sandbox_dir_handle: OwnedHandle,
    pub(crate) sandbox_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_secrets_dir: PathBuf,
    pub(crate) sandbox_secrets_dir_handle: OwnedHandle,
    pub(crate) sandbox_secrets_dir_identity: OwnedFileIdentity,
    pub(crate) sandbox_bin_dir: PathBuf,
    pub(crate) sandbox_bin_dir_handle: OwnedHandle,
    pub(crate) sandbox_bin_dir_identity: OwnedFileIdentity,
    pub(crate) real_user: String,
    pub(crate) real_user_sid: String,
}

pub(crate) fn pin_fresh_owned_setup_directories(
    codex_home: &Path,
    real_user: String,
    real_user_sid: String,
) -> Result<PinnedOwnedSetupDirectories> {
    crate::validate_local_directory_path(codex_home)
        .context("sandbox provisioning CODEX_HOME")?;
    let (codex_home_handle, codex_home_identity) =
        open_pinned_sandbox_directory(codex_home, DirectoryOpenDisposition::OpenExisting)?;
    // Install relative to this exact original pin; retain BOTH it and the
    // resulting guard in PinnedOwnedSetupDirectories throughout privileged work.
    let codex_home_guard = create_directory_guard(codex_home_handle.as_handle())?;
    let (_guard_reopen, guarded_home_identity) =
        open_pinned_sandbox_directory(codex_home, DirectoryOpenDisposition::OpenExisting)?;
    anyhow::ensure!(
        guarded_home_identity == codex_home_identity
            && file_identity_from_raw_handle(codex_home_handle.as_raw_handle() as HANDLE)?
                == codex_home_identity,
        "codex_home directory identity changed while installing directory guard"
    );

    let sbx_dir = sandbox_dir(codex_home);
    let secrets_dir = sandbox_secrets_dir(codex_home);
    let bin_dir = sandbox_bin_dir(codex_home);
    for existing in [&sbx_dir, &secrets_dir, &bin_dir] {
        match std::fs::symlink_metadata(existing) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
            Ok(_) => {
                anyhow::bail!(
                    "pre-existing sandbox directory {} found; refusing to adopt unknown sandbox state",
                    existing.display()
                );
            }
        }
    }

    let (sandbox_dir_handle, sandbox_dir_identity) =
        open_pinned_sandbox_directory(&sbx_dir, DirectoryOpenDisposition::OpenOrCreate)?;
    let (sandbox_secrets_dir_handle, sandbox_secrets_dir_identity) =
        open_pinned_sandbox_directory(&secrets_dir, DirectoryOpenDisposition::OpenOrCreate)?;
    let (sandbox_bin_dir_handle, sandbox_bin_dir_identity) =
        open_pinned_sandbox_directory(&bin_dir, DirectoryOpenDisposition::OpenOrCreate)?;

    Ok(PinnedOwnedSetupDirectories {
        codex_home: codex_home.to_path_buf(),
        codex_home_handle,
        codex_home_guard,
        codex_home_identity,
        sandbox_dir: sbx_dir,
        sandbox_dir_handle,
        sandbox_dir_identity,
        sandbox_secrets_dir: secrets_dir,
        sandbox_secrets_dir_handle,
        sandbox_secrets_dir_identity,
        sandbox_bin_dir: bin_dir,
        sandbox_bin_dir_handle,
        sandbox_bin_dir_identity,
        real_user,
        real_user_sid,
    })
}

// This entire partial proof is owned by the original bounded Capture. On any
// validation Err/unwind it stays there, including the exact directory/guard owners.
pub(crate) struct OwnedSetupValidation {
    pins: Option<PinnedOwnedSetupDirectories>,
    policy: Option<OwnedAppliedPolicyProof>,
    receipt: Option<(OwnedSetupReceiptWire, OwnedPrincipalsProof, OwnedHiddenUsersProof, OwnedFirewallProof)>,
    home: Option<OwnedSandboxHomeProof>,
    wfp: Option<crate::wfp::OwnedWfpProof>,
    process: Option<OwnedSetupProcessProof>,
}
impl OwnedSetupValidation {
    pub(crate) fn new(pins: PinnedOwnedSetupDirectories, policy: OwnedAppliedPolicyProof) -> Self {
        Self { pins: Some(pins), policy: Some(policy), receipt: None, home: None, wfp: None, process: None }
    }
    pub(crate) fn ready(&self) -> bool {
        self.pins.is_none() && self.policy.is_some() && self.receipt.is_some()
            && self.home.is_some() && self.wfp.is_some() && self.process.is_some()
    }
    // Caller checks readiness and ALL fallible/post-validation steps while still
    // armed. Moves below allocate/query/callback nothing and cannot return Err.
    pub(crate) fn commit(self) -> OwnedWindowsSandboxSetup {
        let Self { pins: _, policy, receipt, home, wfp, process } = self;
        let (_, principals, hidden_users, firewall) = receipt.expect("validated receipt");
        OwnedWindowsSandboxSetup {
            process: process.expect("validated process"), policy: policy.expect("validated policy"),
            home: home.expect("validated original home"), principals, hidden_users, firewall,
            wfp: wfp.expect("validated WFP"),
        }
    }
}

// Floor the actual remaining budget; never pass INFINITE or round past the
// deadline. A sub-millisecond remaining budget permits only a zero-ms probe.
pub(crate) fn owned_setup_remaining_lock_ms(
    deadline: std::time::Instant, now: std::time::Instant,
) -> std::io::Result<u32> {
    let remaining = deadline.checked_duration_since(now).filter(|d| !d.is_zero())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "owned setup validation deadline expired"))?;
    Ok(remaining.as_millis().min((u32::MAX - 1) as u128) as u32)
}
fn check_owned_setup_validation_deadline(deadline: std::time::Instant) -> Result<()> {
    owned_setup_remaining_lock_ms(deadline, std::time::Instant::now())?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_and_bind_owned_windows_sandbox_setup(
    matched_setup_exe: &crate::checked_runner::MatchedRunnerArtifact,
    parent_pid: u32,
    child_pid: u32,
    exit_code: Option<i32>,
    invocation_nonce: [u8; 32],
    request_policy_sha256: [u8; 32],
    stdout_bytes: &[u8],
    validation: &mut OwnedSetupValidation,
    deadline: std::time::Instant,
) -> Result<()> {
    let _setup_lock = crate::setup_mutex::acquire_sandbox_setup_lock_until(deadline)?;
    check_owned_setup_validation_deadline(deadline)?;
    anyhow::ensure!(
        !crate::registered_core_requested(),
        "owned sandbox setup cannot bind when registered Core is requested"
    );
    anyhow::ensure!(
        crate::runtime_ownership::load_installation()?.is_none(),
        "owned sandbox setup requires no existing installation registration"
    );
    anyhow::ensure!(
        !read_acl_mutex_exists()?,
        "owned sandbox setup cannot bind while Local\\CodexSandboxReadAcl is active"
    );
    check_owned_setup_validation_deadline(deadline)?;
    let setup_exe_sha256 = matched_setup_exe.sha256();
    let pins = validation.pins.as_ref().expect("original directory pins still owned");
    validation.receipt = Some(extract_and_verify_owned_setup_receipt(
        stdout_bytes, exit_code, &invocation_nonce, parent_pid, child_pid,
        &setup_exe_sha256, &request_policy_sha256,
        pins.codex_home_identity, pins.sandbox_dir_identity,
        pins.sandbox_secrets_dir_identity, pins.sandbox_bin_dir_identity,
    )?);
    check_owned_setup_validation_deadline(deadline)?;
    let policy = validation.policy.as_ref().expect("original applied policy");
    let expected_settings = crate::WindowsSandboxProvisioningSettings {
        proxy_ports: policy.proxy_ports.clone(), allow_local_binding: policy.allow_local_binding,
    };
    anyhow::ensure!(
        crate::sandbox_setup_is_complete_with_settings(&pins.codex_home, &expected_settings),
        "owned setup helper exited before setup artifacts and provisioned settings were complete"
    );
    check_owned_setup_validation_deadline(deadline)?;
    let (receipt, principals, hidden_users, firewall) = validation.receipt.as_ref().unwrap();
    // Nonfallible moves from pins into home WITHIN the same captured validation
    // owner. Install home before continuity/principal/WFP/ACL/post checks can fail.
    let pinned_dirs = validation.pins.take().unwrap();
    validation.home = Some(OwnedSandboxHomeProof {
        codex_home: pinned_dirs.codex_home,
        codex_home_handle: pinned_dirs.codex_home_handle,
        _codex_home_guard: pinned_dirs.codex_home_guard,
        codex_home_identity: pinned_dirs.codex_home_identity,
        sandbox_dir: pinned_dirs.sandbox_dir,
        sandbox_dir_handle: Some(pinned_dirs.sandbox_dir_handle),
        sandbox_dir_identity: pinned_dirs.sandbox_dir_identity,
        sandbox_secrets_dir: pinned_dirs.sandbox_secrets_dir,
        sandbox_secrets_dir_handle: Some(pinned_dirs.sandbox_secrets_dir_handle),
        sandbox_secrets_dir_identity: pinned_dirs.sandbox_secrets_dir_identity,
        sandbox_bin_dir: pinned_dirs.sandbox_bin_dir,
        sandbox_bin_dir_handle: Some(pinned_dirs.sandbox_bin_dir_handle),
        sandbox_bin_dir_identity: pinned_dirs.sandbox_bin_dir_identity,
        setup_marker_identity: receipt.setup_marker_identity,
        setup_marker_sha256: receipt.setup_marker_sha256,
        sandbox_users_identity: receipt.sandbox_users_identity,
        sandbox_users_sha256: receipt.sandbox_users_sha256,
        real_user: pinned_dirs.real_user, real_user_sid: pinned_dirs.real_user_sid,
    });
    let home = validation.home.as_ref().unwrap();
    verify_owned_home_continuity(home)?;
    check_owned_setup_validation_deadline(deadline)?;
    crate::uninstall_windows::verify_live_owned_principals(principals)?;
    check_owned_setup_validation_deadline(deadline)?;
    crate::uninstall_windows::verify_live_owned_hidden_users(hidden_users)?;
    check_owned_setup_validation_deadline(deadline)?;
    crate::uninstall_windows::verify_live_owned_firewall_rules(firewall, &principals.offline_user.sid)?;
    check_owned_setup_validation_deadline(deadline)?;
    let reported_wfp_filters = receipt.wfp.filters.iter()
        .map(|f| (f.name.clone(), f.filter_id, f.expected_sd_blob.clone())).collect::<Vec<_>>();
    validation.wfp = Some(crate::wfp::verify_and_bind_live_owned_wfp_proof(
        receipt.wfp.provider_created, receipt.wfp.sublayer_created,
        &principals.offline_user.sid, &principals.offline_user.sid_bytes,
        &reported_wfp_filters,
    )?);
    check_owned_setup_validation_deadline(deadline)?;
    verify_owned_filesystem_policy_applied(&home.codex_home, policy, &principals.group_sid_bytes)?;
    check_owned_setup_validation_deadline(deadline)?;
    validation.process = Some(OwnedSetupProcessProof {
        setup_exe_path: matched_setup_exe.path().to_path_buf(), setup_exe_sha256,
        parent_pid, child_pid, exit_code: 0, invocation_nonce,
        request_policy_sha256, binding_sha256: receipt.binding_sha256,
    });
    Ok(())
}

#[cfg(test)]
#[path = "setup_provisioning/acl_tests.rs"]
mod acl_tests;

#[cfg(test)]
#[path = "setup_provisioning/registered_tests.rs"]
mod registered_tests;

#[cfg(test)]
mod tests {
    use super::Payload;
    use super::SETUP_VERSION;
    use super::WRITE_ROOT_ALLOW_MASK;
    use super::convert_string_sid_to_sid;
    use super::workspace_write_cap_sids_for_path;
    use crate::ensure_allow_mask_aces;
    use crate::ensure_allow_write_aces;
    use crate::load_or_create_cap_sids;
    use crate::path_mask_allows;
    use crate::path_write_aces_need_refresh;
    use crate::workspace_write_cap_sid_for_root;
    use codex_otel::StatsigMetricsSettings;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::fs;
    use windows_sys::Win32::Foundation::HLOCAL;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Storage::FileSystem::FILE_DELETE_CHILD;

    #[test]
    fn owned_validation_lock_budget_rejects_expiry_floors_remaining_and_never_uses_infinite() {
        use std::time::{Duration, Instant};
        let now = Instant::now();
        assert_eq!(super::owned_setup_remaining_lock_ms(now, now).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(super::owned_setup_remaining_lock_ms(now-Duration::from_nanos(1), now).unwrap_err().kind(), std::io::ErrorKind::TimedOut);
        assert_eq!(super::owned_setup_remaining_lock_ms(now+Duration::from_micros(999), now).unwrap(), 0);
        assert_eq!(super::owned_setup_remaining_lock_ms(now+Duration::from_micros(1999), now).unwrap(), 1);
        assert_eq!(super::owned_setup_remaining_lock_ms(now+Duration::from_secs(120), now+Duration::from_millis(1)).unwrap(), 119999);
        let far = now+Duration::from_millis(u32::MAX as u64+1000);
        assert_eq!(super::owned_setup_remaining_lock_ms(far, now).unwrap(), u32::MAX-1);
    }

    pub(super) fn payload_json() -> serde_json::Value {
        json!({
            "version": SETUP_VERSION,
            "offline_username": "CodexSandboxOffline",
            "online_username": "CodexSandboxOnline",
            "codex_home": "C:\\codex-home",
            "command_cwd": "C:\\workspace",
            "read_roots": [],
            "write_roots": [],
            "proxy_ports": [],
            "real_user": "User",
        })
    }

    #[test]
    fn payload_defaults_otel_absent() {
        let payload: Payload = serde_json::from_value(payload_json()).expect("payload");

        assert_eq!(payload.otel, None);
    }

    #[test]
    fn payload_accepts_provision_only_mode() {
        let mut payload = payload_json();
        payload["mode"] = json!("provision-only");
        let payload: Payload = serde_json::from_value(payload).expect("payload");

        assert_eq!(payload.mode, super::SetupMode::ProvisionOnly);
    }

    #[test]
    fn payload_accepts_interactive_provision_mode() {
        let mut payload = payload_json();
        payload["mode"] = json!("interactive-provision");
        let payload: Payload = serde_json::from_value(payload).expect("payload");

        assert_eq!(payload.mode, super::SetupMode::InteractiveProvision);
    }

    #[test]
    fn payload_accepts_otel_settings() {
        let mut payload = payload_json();
        payload["otel"] = json!({
            "environment": "prod",
        });
        let payload: Payload = serde_json::from_value(payload).expect("payload");

        assert_eq!(
            payload.otel,
            Some(StatsigMetricsSettings {
                environment: "prod".to_string(),
            })
        );
    }

    #[test]
    fn write_root_refresh_replaces_stale_delete_child_grant() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");

        let sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let psid = unsafe { convert_string_sid_to_sid(&sid).expect("convert workspace sid") };
        let stale_write_mask = WRITE_ROOT_ALLOW_MASK | FILE_DELETE_CHILD;
        let seeded = unsafe { ensure_allow_mask_aces(&workspace, &[psid], stale_write_mask) }
            .expect("seed stale write ACE");
        let needs_refresh_before =
            path_write_aces_need_refresh(&workspace, &[psid]).expect("check stale write ACE");
        let replaced = unsafe { ensure_allow_write_aces(&workspace, &[psid]) }
            .expect("replace stale write ACE");
        let needs_refresh_after =
            path_write_aces_need_refresh(&workspace, &[psid]).expect("check refreshed write ACE");
        unsafe {
            LocalFree(psid as HLOCAL);
        }

        assert_eq!(
            (seeded, needs_refresh_before, replaced, needs_refresh_after),
            (true, true, true, false)
        );
    }

    #[test]
    fn write_root_refresh_checks_each_sid() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let workspace = temp.path().join("workspace");
        let other_root = temp.path().join("other-root");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::create_dir_all(&other_root).expect("create other root");

        let workspace_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let other_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &other_root)
            .expect("other root sid");
        let workspace_psid =
            unsafe { convert_string_sid_to_sid(&workspace_sid).expect("convert workspace sid") };
        let other_psid =
            unsafe { convert_string_sid_to_sid(&other_sid).expect("convert other root sid") };

        let seeded = unsafe { ensure_allow_write_aces(&workspace, &[workspace_psid]) }
            .expect("seed workspace SID");
        let needs_refresh_before =
            path_write_aces_need_refresh(&workspace, &[workspace_psid, other_psid])
                .expect("check both SIDs");
        let refreshed =
            unsafe { ensure_allow_write_aces(&workspace, &[workspace_psid, other_psid]) }
                .expect("refresh both SIDs");
        let needs_refresh_after =
            path_write_aces_need_refresh(&workspace, &[workspace_psid, other_psid])
                .expect("recheck both SIDs");
        unsafe {
            LocalFree(workspace_psid as HLOCAL);
            LocalFree(other_psid as HLOCAL);
        }

        assert_eq!(
            (seeded, needs_refresh_before, refreshed, needs_refresh_after,),
            (true, true, true, false)
        );
    }

    #[test]
    fn write_root_refresh_ignores_inherited_delete_child_grant() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let parent = temp.path().join("parent");
        let workspace = parent.join("workspace");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");

        let sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let psid = unsafe { convert_string_sid_to_sid(&sid).expect("convert workspace sid") };
        let seeded_explicit =
            unsafe { ensure_allow_mask_aces(&workspace, &[psid], WRITE_ROOT_ALLOW_MASK) }
                .expect("seed explicit write ACE");
        let seeded_parent = unsafe {
            ensure_allow_mask_aces(&parent, &[psid], WRITE_ROOT_ALLOW_MASK | FILE_DELETE_CHILD)
        }
        .expect("seed inherited stale write ACE");
        let has_inherited_delete_child = path_mask_allows(
            &workspace,
            &[psid],
            FILE_DELETE_CHILD,
            /*require_all_bits*/ false,
        )
        .expect("check inherited stale write ACE");
        let needs_refresh = path_write_aces_need_refresh(&workspace, &[psid])
            .expect("check inherited stale write ACE");
        let first_refresh = unsafe { ensure_allow_write_aces(&workspace, &[psid]) }
            .expect("first inherited write ACE refresh");
        let second_refresh = unsafe { ensure_allow_write_aces(&workspace, &[psid]) }
            .expect("second inherited write ACE refresh");
        unsafe {
            LocalFree(psid as HLOCAL);
        }

        assert_eq!(
            (
                seeded_explicit,
                seeded_parent,
                has_inherited_delete_child,
                needs_refresh,
                first_refresh,
                second_refresh,
            ),
            (true, true, true, false, false, false)
        );
    }

    #[test]
    fn deny_path_under_active_root_uses_only_matching_root_sid() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let workspace = temp.path().join("workspace");
        let active_root = temp.path().join("active-root");
        let stale_root = temp.path().join("stale-root");
        let deny_path = active_root.join("protected");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::create_dir_all(&active_root).expect("create active root");
        fs::create_dir_all(&stale_root).expect("create stale root");
        fs::create_dir_all(&deny_path).expect("create deny path");

        let stale_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &stale_root)
            .expect("stale sid");
        let active_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &active_root)
            .expect("active sid");
        let workspace_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let caps = load_or_create_cap_sids(&codex_home).expect("load caps");

        let deny_sids = workspace_write_cap_sids_for_path(
            &codex_home,
            &workspace,
            &[workspace.clone(), active_root],
            &deny_path,
        )
        .expect("deny sids");

        assert_eq!(deny_sids, vec![active_sid]);
        assert!(!deny_sids.contains(&workspace_sid));
        assert!(!deny_sids.contains(&stale_sid));
        assert!(!deny_sids.contains(&caps.workspace));
    }

    #[test]
    fn deny_path_outside_active_roots_falls_back_to_all_active_root_sids() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let workspace = temp.path().join("workspace");
        let active_root = temp.path().join("active-root");
        let stale_root = temp.path().join("stale-root");
        let deny_path = temp.path().join("outside-deny");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::create_dir_all(&active_root).expect("create active root");
        fs::create_dir_all(&stale_root).expect("create stale root");
        fs::create_dir_all(&deny_path).expect("create deny path");

        let stale_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &stale_root)
            .expect("stale sid");
        let active_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &active_root)
            .expect("active sid");
        let workspace_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let caps = load_or_create_cap_sids(&codex_home).expect("load caps");

        let deny_sids = workspace_write_cap_sids_for_path(
            &codex_home,
            &workspace,
            &[workspace.clone(), active_root],
            &deny_path,
        )
        .expect("deny sids");

        assert_eq!(deny_sids.len(), 2);
        assert!(deny_sids.contains(&workspace_sid));
        assert!(deny_sids.contains(&active_sid));
        assert!(!deny_sids.contains(&stale_sid));
        assert!(!deny_sids.contains(&caps.workspace));
    }

    #[test]
    fn deny_path_includes_nested_active_root_sid() {
        let temp = tempfile::tempdir().expect("tempdir");
        let codex_home = temp.path().join("codex-home");
        let workspace = temp.path().join("workspace");
        let protected_dir = workspace.join(".codex");
        let nested_root = protected_dir.join("nested-root");
        fs::create_dir_all(&codex_home).expect("create codex home");
        fs::create_dir_all(&workspace).expect("create workspace");
        fs::create_dir_all(&nested_root).expect("create nested root");

        let workspace_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &workspace)
            .expect("workspace sid");
        let nested_sid = workspace_write_cap_sid_for_root(&codex_home, &workspace, &nested_root)
            .expect("nested sid");

        let deny_sids = workspace_write_cap_sids_for_path(
            &codex_home,
            &workspace,
            &[workspace.clone(), nested_root],
            &protected_dir,
        )
        .expect("deny sids");

        assert_eq!(deny_sids, vec![workspace_sid, nested_sid]);
    }

    #[test]
    fn owned_home_continuity_plan_rejects_replaced_directories_and_files() {
        use super::OwnedFileIdentity;
        use super::OwnedHomeIdentitiesSnapshot;
        use super::verify_owned_home_identities_plan;

        let dir_id = |low: u32| OwnedFileIdentity {
            volume_serial_number: 0xAA55_0001,
            file_index_high: 1,
            file_index_low: low,
            is_directory: true,
        };
        let file_id = |low: u32| OwnedFileIdentity {
            volume_serial_number: 0xAA55_0001,
            file_index_high: 2,
            file_index_low: low,
            is_directory: false,
        };
        let home = dir_id(10);
        let sbx = dir_id(11);
        let secrets = dir_id(12);
        let bin = dir_id(13);
        let marker = file_id(21);
        let users = file_id(22);
        let marker_sha = [0x11_u8; 32];
        let users_sha = [0x22_u8; 32];

        let valid = OwnedHomeIdentitiesSnapshot {
            codex_home_handle_id: home,
            codex_home_path_id: home,
            sandbox_dir_handle_id: Some(sbx),
            sandbox_dir_path_id: sbx,
            sandbox_secrets_dir_handle_id: Some(secrets),
            sandbox_secrets_dir_path_id: secrets,
            sandbox_bin_dir_handle_id: Some(bin),
            sandbox_bin_dir_path_id: bin,
            setup_marker_id: marker,
            setup_marker_sha256: marker_sha,
            sandbox_users_id: users,
            sandbox_users_sha256: users_sha,
        };
        assert!(verify_owned_home_identities_plan(
            home, sbx, secrets, bin, marker, marker_sha, users, users_sha, &valid
        )
        .is_ok());

        let mut replaced_home = valid.clone();
        replaced_home.codex_home_path_id = dir_id(99);
        assert!(verify_owned_home_identities_plan(
            home,
            sbx,
            secrets,
            bin,
            marker,
            marker_sha,
            users,
            users_sha,
            &replaced_home
        )
        .is_err());

        let mut unpinned_bin = valid.clone();
        unpinned_bin.sandbox_bin_dir_handle_id = None;
        assert!(verify_owned_home_identities_plan(
            home,
            sbx,
            secrets,
            bin,
            marker,
            marker_sha,
            users,
            users_sha,
            &unpinned_bin
        )
        .is_err());

        let mut tampered_marker = valid;
        tampered_marker.setup_marker_sha256 = [0xFF_u8; 32];
        assert!(verify_owned_home_identities_plan(
            home,
            sbx,
            secrets,
            bin,
            marker,
            marker_sha,
            users,
            users_sha,
            &tampered_marker
        )
        .is_err());
    }

    #[test]
    fn owned_full_protocol_binding_rejects_tampered_nonce_pid_schema_and_identities() {
        use super::BASE64;
        use super::OWNED_SETUP_PROTOCOL_SCHEMA_V1;
        use super::OWNED_SETUP_RECEIPT_PREFIX_V1;
        use super::OwnedFileIdentity;
        use super::OwnedFirewallReceiptWire;
        use super::OwnedFirewallRuleReceiptWire;
        use super::OwnedHiddenUsersReceiptWire;
        use super::OwnedPrincipalsReceiptWire;
        use super::OwnedSetupInvocationWire;
        use super::OwnedSetupProcessProof;
        use super::OwnedSetupReceiptWire;
        use super::OwnedUserPrincipalReceiptWire;
        use super::OwnedWfpFilterReceiptWire;
        use super::OwnedWfpReceiptWire;
        use super::WINLOGON_USERLIST_KEY_PATH;
        use super::compute_owned_setup_policy_sha256;
        use super::compute_owned_setup_receipt_binding_sha256;
        use super::extract_and_verify_owned_setup_receipt;
        use base64::Engine;
        use std::path::PathBuf;

        let dir_id = |low: u32| OwnedFileIdentity {
            volume_serial_number: 0x1234_5678,
            file_index_high: 1,
            file_index_low: low,
            is_directory: true,
        };
        let file_id = |low: u32| OwnedFileIdentity {
            volume_serial_number: 0x1234_5678,
            file_index_high: 2,
            file_index_low: low,
            is_directory: false,
        };
        let home_id = dir_id(101);
        let sbx_id = dir_id(102);
        let secrets_id = dir_id(103);
        let bin_id = dir_id(104);
        let marker_id = file_id(201);
        let users_id = file_id(202);

        let nonce = [0x5A_u8; 32];
        let parent_pid = 4100_u32;
        let child_pid = 4200_u32;
        let setup_exe_sha = [0x33_u8; 32];
        let policy_sha = compute_owned_setup_policy_sha256(
            SETUP_VERSION,
            crate::setup::OFFLINE_USERNAME,
            crate::setup::ONLINE_USERNAME,
            &PathBuf::from(r"C:\codex-home"),
            &PathBuf::from(r"C:\workspace"),
            &[PathBuf::from(r"C:\read-root")],
            &[PathBuf::from(r"C:\workspace")],
            &[PathBuf::from(r"C:\workspace\secret")],
            &[PathBuf::from(r"C:\workspace\.git")],
            &[8080],
            false,
            "User",
            "S-1-5-21-10-20-30-500",
            home_id,
            sbx_id,
            secrets_id,
            bin_id,
        );
        let altered_policy_sha = compute_owned_setup_policy_sha256(
            SETUP_VERSION,
            crate::setup::OFFLINE_USERNAME,
            crate::setup::ONLINE_USERNAME,
            &PathBuf::from(r"C:\codex-home"),
            &PathBuf::from(r"C:\workspace"),
            &[PathBuf::from(r"C:\read-root")],
            &[PathBuf::from(r"C:\workspace")],
            &[PathBuf::from(r"C:\workspace\different-secret")],
            &[PathBuf::from(r"C:\workspace\.git")],
            &[8080],
            false,
            "User",
            "S-1-5-21-10-20-30-500",
            home_id,
            sbx_id,
            secrets_id,
            bin_id,
        );
        assert_ne!(policy_sha, altered_policy_sha);

        let invocation = OwnedSetupInvocationWire {
            schema_version: OWNED_SETUP_PROTOCOL_SCHEMA_V1,
            nonce_b64: BASE64.encode(nonce),
            parent_pid,
            expected_setup_exe_sha256: setup_exe_sha,
            request_policy_sha256: policy_sha,
            real_user_sid: "S-1-5-21-10-20-30-500".to_string(),
            codex_home_identity: home_id,
            sandbox_dir_identity: sbx_id,
            sandbox_secrets_dir_identity: secrets_id,
            sandbox_bin_dir_identity: bin_id,
        };
        let debug_wire = format!("{invocation:?}");
        assert!(
            !debug_wire.contains(&invocation.nonce_b64),
            "Debug on OwnedSetupInvocationWire must redact nonce_b64"
        );
        let process_proof = OwnedSetupProcessProof {
            setup_exe_path: PathBuf::from(r"C:\bin\codex-windows-sandbox-setup.exe"),
            setup_exe_sha256: setup_exe_sha,
            parent_pid,
            child_pid,
            exit_code: 0,
            invocation_nonce: nonce,
            request_policy_sha256: policy_sha,
            binding_sha256: [0x99_u8; 32],
        };
        assert!(
            format!("{process_proof:?}").contains("<redacted>"),
            "Debug on OwnedSetupProcessProof must redact invocation_nonce"
        );

        let offline_sid = "S-1-5-21-10-20-30-1001".to_string();
        let offline_sid_bytes = vec![1, 2, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0, 233, 3, 0, 0];
        let mut receipt = OwnedSetupReceiptWire {
            schema_version: OWNED_SETUP_PROTOCOL_SCHEMA_V1,
            parent_pid,
            helper_pid: child_pid,
            setup_exe_sha256: setup_exe_sha,
            request_policy_sha256: policy_sha,
            codex_home_identity: home_id,
            sandbox_dir_identity: sbx_id,
            sandbox_secrets_dir_identity: secrets_id,
            sandbox_bin_dir_identity: bin_id,
            setup_marker_identity: marker_id,
            setup_marker_sha256: [0xAA_u8; 32],
            sandbox_users_identity: users_id,
            sandbox_users_sha256: [0xBB_u8; 32],
            principals: OwnedPrincipalsReceiptWire {
                offline_user: OwnedUserPrincipalReceiptWire {
                    username: crate::setup::OFFLINE_USERNAME.to_string(),
                    sid: offline_sid.clone(),
                    sid_bytes: offline_sid_bytes.clone(),
                    initial_flags: 0x201,
                },
                online_user: OwnedUserPrincipalReceiptWire {
                    username: crate::setup::ONLINE_USERNAME.to_string(),
                    sid: "S-1-5-21-10-20-30-1002".to_string(),
                    sid_bytes: vec![1, 2, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0, 234, 3, 0, 0],
                    initial_flags: 0x201,
                },
                group_name: crate::SANDBOX_USERS_GROUP.to_string(),
                group_sid: "S-1-5-21-10-20-30-1003".to_string(),
                group_sid_bytes: vec![1, 2, 0, 0, 0, 0, 0, 5, 21, 0, 0, 0, 235, 3, 0, 0],
            },
            hidden_users: OwnedHiddenUsersReceiptWire {
                key_path: WINLOGON_USERLIST_KEY_PATH.to_string(),
                usernames: [
                    crate::setup::OFFLINE_USERNAME.to_string(),
                    crate::setup::ONLINE_USERNAME.to_string(),
                ],
                expected_dword_value: 0,
            },
            firewall: OwnedFirewallReceiptWire {
                offline_sid: offline_sid.clone(),
                rules: vec![OwnedFirewallRuleReceiptWire {
                    internal_name: "codex_sandbox_offline_block_outbound".to_string(),
                    friendly_desc: "Codex Sandbox Offline - Block Non-Loopback Outbound"
                        .to_string(),
                    direction: 2,
                    action: 0,
                    enabled: true,
                    profiles: 0x7fff_ffff,
                    protocol: 256,
                    local_user_authorized_list: format!("O:LSD:(A;;CC;;;{offline_sid})"),
                    offline_sid: offline_sid.clone(),
                    remote_addresses: "0.0.0.0-126.255.255.255".to_string(),
                    remote_ports: None,
                }],
            },
            wfp: OwnedWfpReceiptWire {
                provider_created: true,
                sublayer_created: true,
                offline_sid,
                offline_sid_bytes,
                filters: vec![OwnedWfpFilterReceiptWire {
                    name: "block_outbound_ipv4".to_string(),
                    filter_id: 9001,
                    expected_sd_blob: vec![1, 0, 4, 0x80],
                }],
            },
            binding_sha256: [0_u8; 32],
        };
        receipt.binding_sha256 =
            compute_owned_setup_receipt_binding_sha256(&nonce, &receipt).expect("binding sha256");

        let encode_line = |r: &OwnedSetupReceiptWire| -> Vec<u8> {
            format!(
                "{OWNED_SETUP_RECEIPT_PREFIX_V1}{}\n",
                BASE64.encode(serde_json::to_vec(r).expect("receipt json"))
            )
            .into_bytes()
        };

        let valid_stdout = encode_line(&receipt);
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_ok()
        );

        // Non-zero exit code must fail even with a valid receipt line.
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(1),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Wrong private nonce must fail.
        let wrong_nonce = [0x42_u8; 32];
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &wrong_nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Wrong child PID must fail.
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &nonce,
                parent_pid,
                child_pid + 1,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Wrong setup.exe SHA-256 must fail.
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &[0xFE_u8; 32],
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Wrong policy digest must fail.
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &altered_policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Replaced directory identity must fail.
        assert!(
            extract_and_verify_owned_setup_receipt(
                &valid_stdout,
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                dir_id(999),
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Schema mismatch must fail.
        let mut bad_schema = receipt.clone();
        bad_schema.schema_version = OWNED_SETUP_PROTOCOL_SCHEMA_V1 + 1;
        bad_schema.binding_sha256 =
            compute_owned_setup_receipt_binding_sha256(&nonce, &bad_schema).unwrap();
        assert!(
            extract_and_verify_owned_setup_receipt(
                &encode_line(&bad_schema),
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );

        // Uncreated legacy firewall rule must fail even if signed with the right nonce.
        let mut legacy_fw = receipt;
        legacy_fw.firewall.rules[0].internal_name =
            "codex_sandbox_offline_allow_loopback_proxy".to_string();
        legacy_fw.binding_sha256 =
            compute_owned_setup_receipt_binding_sha256(&nonce, &legacy_fw).unwrap();
        assert!(
            extract_and_verify_owned_setup_receipt(
                &encode_line(&legacy_fw),
                Some(0),
                &nonce,
                parent_pid,
                child_pid,
                &setup_exe_sha,
                &policy_sha,
                home_id,
                sbx_id,
                secrets_id,
                bin_id,
            )
            .is_err()
        );
    }
}
