use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngCore;
use rand::SeedableRng;
use rand::rngs::SmallRng;
use serde::Serialize;
use std::ffi::OsStr;
use std::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::os::windows::io::FromRawHandle;
use std::path::Path;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER;
use windows_sys::Win32::Foundation::ERROR_PATH_NOT_FOUND;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::Foundation::GENERIC_WRITE;
use windows_sys::Win32::Foundation::GetLastError;
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::Win32::NetworkManagement::NetManagement::LOCALGROUP_INFO_1;
use windows_sys::Win32::NetworkManagement::NetManagement::LOCALGROUP_MEMBERS_INFO_3;
use windows_sys::Win32::NetworkManagement::NetManagement::NERR_Success;
use windows_sys::Win32::NetworkManagement::NetManagement::NetLocalGroupAdd;
use windows_sys::Win32::NetworkManagement::NetManagement::NetLocalGroupAddMembers;
use windows_sys::Win32::NetworkManagement::NetManagement::NetUserAdd;
use windows_sys::Win32::NetworkManagement::NetManagement::NetUserSetInfo;
use windows_sys::Win32::NetworkManagement::NetManagement::UF_DONT_EXPIRE_PASSWD;
use windows_sys::Win32::NetworkManagement::NetManagement::UF_SCRIPT;
use windows_sys::Win32::NetworkManagement::NetManagement::USER_INFO_1;
use windows_sys::Win32::NetworkManagement::NetManagement::USER_INFO_1003;
use windows_sys::Win32::NetworkManagement::NetManagement::USER_PRIV_USER;
use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::Authorization::SDDL_REVISION_1;
use windows_sys::Win32::Security::LookupAccountSidW;
use windows_sys::Win32::Security::PSECURITY_DESCRIPTOR;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Security::SID_NAME_USE;
use windows_sys::Win32::Storage::FileSystem::CREATE_NEW;
use windows_sys::Win32::Storage::FileSystem::CreateFileW;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
use windows_sys::Win32::System::Registry::HKEY;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::Registry::KEY_QUERY_VALUE;
use windows_sys::Win32::System::Registry::KEY_SET_VALUE;
use windows_sys::Win32::System::Registry::REG_DWORD;
use windows_sys::Win32::System::Registry::REG_OPTION_NON_VOLATILE;
use windows_sys::Win32::System::Registry::RegCloseKey;
use windows_sys::Win32::System::Registry::RegCreateKeyExW;
use windows_sys::Win32::System::Registry::RegOpenKeyExW;
use windows_sys::Win32::System::Registry::RegQueryValueExW;
use windows_sys::Win32::System::Registry::RegSetValueExW;

use crate::SANDBOX_USERS_GROUP;
use crate::SETUP_VERSION;
use crate::SetupErrorCode;
use crate::SetupFailure;
use crate::dpapi_protect;
use crate::ensure_sandbox_users_group;
use crate::local_user_flags;
use crate::resolve_sid;
use crate::sandbox_dir;
use crate::sandbox_secrets_dir;
use crate::string_from_sid_bytes;
use crate::to_wide;
use crate::write_file_atomically;

use super::SetupMode;

#[cfg(test)]
#[path = "sandbox_users_tests.rs"]
mod tests;

const SID_USERS: &str = "S-1-5-32-545";
const SANDBOX_USERS_GROUP_COMMENT: &str = "Codex sandbox internal group (managed)";
pub(crate) const WINLOGON_USERLIST_KEY_PATH: &str =
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OwnedUserPrincipalProof {
    pub(crate) username: String,
    pub(crate) sid: String,
    pub(crate) sid_bytes: Vec<u8>,
    pub(crate) initial_flags: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OwnedPrincipalsProof {
    pub(crate) offline_user: OwnedUserPrincipalProof,
    pub(crate) online_user: OwnedUserPrincipalProof,
    pub(crate) group_name: String,
    pub(crate) group_sid: String,
    pub(crate) group_sid_bytes: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct OwnedHiddenUsersProof {
    pub(crate) key_path: &'static str,
    pub(crate) usernames: [String; 2],
    pub(crate) expected_dword_value: u32,
}

pub fn resolve_sandbox_users_group_sid() -> Result<Vec<u8>> {
    resolve_sid(SANDBOX_USERS_GROUP)
}

pub(super) fn provision_owned_sandbox_users(
    codex_home: &Path,
    offline_username: &str,
    online_username: &str,
    log: &mut dyn Write,
) -> Result<(OwnedPrincipalsProof, OwnedHiddenUsersProof)> {
    ensure!(
        offline_username != online_username
            && !offline_username.is_empty()
            && !online_username.is_empty(),
        "owned sandbox user provisioning requires distinct non-empty usernames"
    );
    for username in [offline_username, online_username] {
        ensure!(
            local_user_flags(username)?.is_none() && resolve_sid(username).is_err(),
            "pre-existing sandbox user {username} found before owned setup; refusing to reset or adopt"
        );
    }
    ensure!(
        resolve_sid(SANDBOX_USERS_GROUP).is_err(),
        "pre-existing sandbox group {SANDBOX_USERS_GROUP} found before owned setup; refusing to adopt"
    );
    ensure_hidden_user_entries_absent(&[offline_username, online_username])?;

    let group_sid_bytes = create_fresh_sandbox_users_group()?;
    let group_sid = string_from_sid_bytes(&group_sid_bytes).map_err(anyhow::Error::msg)?;

    super::log_line(
        log,
        &format!(
            "creating fresh owned sandbox users offline={offline_username} online={online_username}"
        ),
    )?;
    let offline_password = random_password();
    let online_password = random_password();
    let offline_user = create_fresh_sandbox_user(offline_username, &offline_password, log)?;
    let online_user = create_fresh_sandbox_user(online_username, &online_password, log)?;
    ensure!(
        offline_user.sid != online_user.sid && offline_user.sid != group_sid && online_user.sid != group_sid,
        "owned sandbox principal SIDs must be distinct"
    );

    write_secrets(
        codex_home,
        offline_username,
        &offline_password,
        online_username,
        &online_password,
        SetupMode::ProvisionOnly,
    )?;

    let hidden_users =
        create_owned_hidden_users_registry_entries([offline_username, online_username])?;
    Ok((
        OwnedPrincipalsProof {
            offline_user,
            online_user,
            group_name: SANDBOX_USERS_GROUP.to_string(),
            group_sid,
            group_sid_bytes,
        },
        hidden_users,
    ))
}

fn create_fresh_sandbox_users_group() -> Result<Vec<u8>> {
    let name = to_wide(SANDBOX_USERS_GROUP);
    let comment = to_wide(SANDBOX_USERS_GROUP_COMMENT);
    let info = LOCALGROUP_INFO_1 {
        lgrpi1_name: name.as_ptr() as *mut u16,
        lgrpi1_comment: comment.as_ptr() as *mut u16,
    };
    let mut parameter_error = 0;
    let status = unsafe {
        NetLocalGroupAdd(
            std::ptr::null(),
            1,
            (&raw const info).cast(),
            &raw mut parameter_error,
        )
    };
    if status != NERR_Success {
        bail!(
            "NetLocalGroupAdd for fresh owned group {SANDBOX_USERS_GROUP} failed with code {status} (parm_err={parameter_error})"
        );
    }
    resolve_sid(SANDBOX_USERS_GROUP)
}

fn create_fresh_sandbox_user(
    username: &str,
    password: &str,
    log: &mut dyn Write,
) -> Result<OwnedUserPrincipalProof> {
    let name_w = to_wide(OsStr::new(username));
    let pwd_w = to_wide(OsStr::new(password));
    let expected_flags = UF_SCRIPT | UF_DONT_EXPIRE_PASSWD;
    unsafe {
        let info = USER_INFO_1 {
            usri1_name: name_w.as_ptr() as *mut u16,
            usri1_password: pwd_w.as_ptr() as *mut u16,
            usri1_password_age: 0,
            usri1_priv: USER_PRIV_USER,
            usri1_home_dir: std::ptr::null_mut(),
            usri1_comment: std::ptr::null_mut(),
            usri1_flags: expected_flags,
            usri1_script_path: std::ptr::null_mut(),
        };
        let status = NetUserAdd(
            std::ptr::null(),
            1,
            &info as *const _ as *mut u8,
            std::ptr::null_mut(),
        );
        if status != NERR_Success {
            super::log_line(log, &format!("NetUserAdd failed for {username} code {status}"))?;
            return Err(anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperUserCreateOrUpdateFailed,
                format!("NetUserAdd failed for fresh owned user {username}, code {status}"),
            )));
        }

        if let Ok(group_name) = lookup_account_name_for_sid(SID_USERS) {
            let group = to_wide(OsStr::new(&group_name));
            let member = LOCALGROUP_MEMBERS_INFO_3 {
                lgrmi3_domainandname: name_w.as_ptr() as *mut u16,
            };
            let _ = NetLocalGroupAddMembers(
                std::ptr::null(),
                group.as_ptr(),
                3,
                &member as *const _ as *mut u8,
                1,
            );
        }

        let group_w = to_wide(OsStr::new(SANDBOX_USERS_GROUP));
        let member = LOCALGROUP_MEMBERS_INFO_3 {
            lgrmi3_domainandname: name_w.as_ptr() as *mut u16,
        };
        let member_status = NetLocalGroupAddMembers(
            std::ptr::null(),
            group_w.as_ptr(),
            3,
            &member as *const _ as *mut u8,
            1,
        );
        ensure!(
            member_status == NERR_Success,
            "NetLocalGroupAddMembers({SANDBOX_USERS_GROUP}, {username}) failed with code {member_status}"
        );
    }

    let sid_bytes = resolve_sid(username)?;
    let sid = string_from_sid_bytes(&sid_bytes).map_err(anyhow::Error::msg)?;
    let initial_flags = local_user_flags(username)?
        .ok_or_else(|| anyhow::anyhow!("newly created sandbox user {username} flags unreadable"))?;
    Ok(OwnedUserPrincipalProof {
        username: username.to_string(),
        sid,
        sid_bytes,
        initial_flags,
    })
}

fn ensure_hidden_user_entries_absent(usernames: &[&str]) -> Result<()> {
    let mut key: HKEY = 0;
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            to_wide(WINLOGON_USERLIST_KEY_PATH).as_ptr(),
            0,
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    match status {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(()),
        ERROR_SUCCESS => {}
        err => bail!("open Winlogon UserList key for preflight failed: {err}"),
    }
    let check_result = (|| -> Result<()> {
        for username in usernames {
            let name_w = to_wide(OsStr::new(username));
            let mut value_type: u32 = 0;
            let mut data_len: u32 = 0;
            let query = unsafe {
                RegQueryValueExW(
                    key,
                    name_w.as_ptr(),
                    std::ptr::null_mut(),
                    &mut value_type,
                    std::ptr::null_mut(),
                    &mut data_len,
                )
            };
            if query != ERROR_FILE_NOT_FOUND {
                bail!(
                    "pre-existing Winlogon UserList value for {username} found before owned setup (status={query}); refusing to adopt"
                );
            }
        }
        Ok(())
    })();
    unsafe {
        RegCloseKey(key);
    }
    check_result
}

fn create_owned_hidden_users_registry_entries(
    usernames: [&str; 2],
) -> Result<OwnedHiddenUsersProof> {
    let mut key: HKEY = 0;
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_LOCAL_MACHINE,
            to_wide(WINLOGON_USERLIST_KEY_PATH).as_ptr(),
            0,
            std::ptr::null_mut(),
            REG_OPTION_NON_VOLATILE,
            KEY_QUERY_VALUE | KEY_SET_VALUE,
            std::ptr::null_mut(),
            &mut key,
            std::ptr::null_mut(),
        )
    };
    ensure!(
        status == ERROR_SUCCESS,
        "RegCreateKeyExW for Winlogon UserList failed: {status}"
    );
    let write_result = (|| -> Result<()> {
        for username in usernames {
            let name_w = to_wide(OsStr::new(username));
            let expected: u32 = 0;
            let set_status = unsafe {
                RegSetValueExW(
                    key,
                    name_w.as_ptr(),
                    0,
                    REG_DWORD,
                    &expected as *const u32 as *const u8,
                    std::mem::size_of::<u32>() as u32,
                )
            };
            ensure!(
                set_status == ERROR_SUCCESS,
                "RegSetValueExW for hidden user {username} failed: {set_status}"
            );
            let mut actual_type: u32 = 0;
            let mut actual_value: u32 = u32::MAX;
            let mut actual_len: u32 = std::mem::size_of::<u32>() as u32;
            let query_status = unsafe {
                RegQueryValueExW(
                    key,
                    name_w.as_ptr(),
                    std::ptr::null_mut(),
                    &mut actual_type,
                    &mut actual_value as *mut u32 as *mut u8,
                    &mut actual_len,
                )
            };
            ensure!(
                query_status == ERROR_SUCCESS
                    && actual_type == REG_DWORD
                    && actual_len == std::mem::size_of::<u32>() as u32
                    && actual_value == 0,
                "read-back verification of Winlogon UserList entry for {username} failed"
            );
        }
        Ok(())
    })();
    unsafe {
        RegCloseKey(key);
    }
    write_result?;
    Ok(OwnedHiddenUsersProof {
        key_path: WINLOGON_USERLIST_KEY_PATH,
        usernames: [usernames[0].to_string(), usernames[1].to_string()],
        expected_dword_value: 0,
    })
}

pub(super) fn provision_sandbox_users(
    codex_home: &Path,
    offline_username: &str,
    online_username: &str,
    new_user_flags: u32,
    log: &mut dyn Write,
    mode: SetupMode,
) -> Result<()> {
    if let Err(err) = ensure_sandbox_users_group() {
        let message = format!("failed to create local group {SANDBOX_USERS_GROUP}: {err}");
        super::log_line(log, &message)?;
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersGroupCreateFailed,
            message,
        )));
    }
    super::log_line(
        log,
        &format!("ensuring sandbox users offline={offline_username} online={online_username}"),
    )?;
    let offline_password = random_password();
    let online_password = random_password();
    ensure_sandbox_user(offline_username, &offline_password, new_user_flags, log)?;
    ensure_sandbox_user(online_username, &online_password, new_user_flags, log)?;
    write_secrets(
        codex_home,
        offline_username,
        &offline_password,
        online_username,
        &online_password,
        mode,
    )?;
    Ok(())
}

pub fn ensure_sandbox_user(
    username: &str,
    password: &str,
    new_user_flags: u32,
    log: &mut dyn Write,
) -> Result<()> {
    ensure_local_user(username, password, new_user_flags, log)?;
    ensure_local_group_member(SANDBOX_USERS_GROUP, username)?;
    Ok(())
}

pub fn ensure_local_user(
    name: &str,
    password: &str,
    new_user_flags: u32,
    log: &mut dyn Write,
) -> Result<()> {
    let name_w = to_wide(OsStr::new(name));
    let pwd_w = to_wide(OsStr::new(password));
    unsafe {
        let info = USER_INFO_1 {
            usri1_name: name_w.as_ptr() as *mut u16,
            usri1_password: pwd_w.as_ptr() as *mut u16,
            usri1_password_age: 0,
            usri1_priv: USER_PRIV_USER,
            usri1_home_dir: std::ptr::null_mut(),
            usri1_comment: std::ptr::null_mut(),
            usri1_flags: UF_SCRIPT | UF_DONT_EXPIRE_PASSWD | new_user_flags,
            usri1_script_path: std::ptr::null_mut(),
        };
        let status = NetUserAdd(
            std::ptr::null(),
            1,
            &info as *const _ as *mut u8,
            std::ptr::null_mut(),
        );
        if status != NERR_Success {
            // Reset only the password, preserving account flags owned by enterprise policy.
            // A separate flag update could fail after the reset but before credentials are saved.
            let pw_info = USER_INFO_1003 {
                usri1003_password: pwd_w.as_ptr() as *mut u16,
            };
            let upd = NetUserSetInfo(
                std::ptr::null(),
                name_w.as_ptr(),
                1003,
                &pw_info as *const _ as *mut u8,
                std::ptr::null_mut(),
            );
            if upd != NERR_Success {
                super::log_line(log, &format!("NetUserSetInfo failed for {name} code {upd}"))?;
                return Err(anyhow::Error::new(SetupFailure::new(
                    SetupErrorCode::HelperUserCreateOrUpdateFailed,
                    format!("failed to create/update user {name}, code {status}/{upd}"),
                )));
            }
        }

        // Ensure the principal is a regular local user account.
        if let Ok(group_name) = lookup_account_name_for_sid(SID_USERS) {
            let group = to_wide(OsStr::new(&group_name));
            let member = LOCALGROUP_MEMBERS_INFO_3 {
                lgrmi3_domainandname: name_w.as_ptr() as *mut u16,
            };
            let _ = NetLocalGroupAddMembers(
                std::ptr::null(),
                group.as_ptr(),
                3,
                &member as *const _ as *mut u8,
                1,
            );
        } else {
            super::log_line(
                log,
                "LookupAccountSidW failed for Users SID; skipping Users group membership",
            )?;
        }
    }
    Ok(())
}

pub fn ensure_local_group_member(group_name: &str, member_name: &str) -> Result<()> {
    // If the member is already in the group, NetLocalGroupAddMembers may
    // return an error code. We don't care.
    let group_w = to_wide(OsStr::new(group_name));
    let member_w = to_wide(OsStr::new(member_name));
    unsafe {
        let member = LOCALGROUP_MEMBERS_INFO_3 {
            lgrmi3_domainandname: member_w.as_ptr() as *mut u16,
        };
        let _ = NetLocalGroupAddMembers(
            std::ptr::null(),
            group_w.as_ptr(),
            3,
            &member as *const _ as *mut u8,
            1,
        );
    }
    Ok(())
}

fn lookup_account_name_for_sid(sid_str: &str) -> Result<String> {
    let sid_w = to_wide(OsStr::new(sid_str));
    let mut psid: *mut c_void = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(sid_w.as_ptr(), &mut psid) } == 0 {
        return Err(anyhow::anyhow!(
            "ConvertStringSidToSidW failed for {sid_str}: {}",
            unsafe { GetLastError() }
        ));
    }
    let mut name_len: u32 = 0;
    let mut domain_len: u32 = 0;
    let mut use_type: SID_NAME_USE = 0;
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            psid,
            std::ptr::null_mut(),
            &mut name_len,
            std::ptr::null_mut(),
            &mut domain_len,
            &mut use_type,
        )
    };
    if ok == 0 {
        let err = unsafe { GetLastError() };
        if err != ERROR_INSUFFICIENT_BUFFER {
            unsafe {
                LocalFree(psid as _);
            }
            return Err(anyhow::anyhow!(
                "LookupAccountSidW preflight failed for {sid_str}: {err}"
            ));
        }
    }
    let mut name_buf: Vec<u16> = vec![0u16; name_len as usize];
    let mut domain_buf: Vec<u16> = vec![0u16; domain_len as usize];
    let ok = unsafe {
        LookupAccountSidW(
            std::ptr::null(),
            psid,
            name_buf.as_mut_ptr(),
            &mut name_len,
            domain_buf.as_mut_ptr(),
            &mut domain_len,
            &mut use_type,
        )
    };
    unsafe {
        LocalFree(psid as _);
    }
    if ok == 0 {
        return Err(anyhow::anyhow!(
            "LookupAccountSidW failed for {sid_str}: {}",
            unsafe { GetLastError() }
        ));
    }
    let name = String::from_utf16_lossy(&name_buf);
    Ok(name.trim_end_matches('\0').to_string())
}

pub fn sid_bytes_to_psid(sid: &[u8]) -> Result<*mut c_void> {
    let sid_str = string_from_sid_bytes(sid).map_err(anyhow::Error::msg)?;
    let sid_w = to_wide(OsStr::new(&sid_str));
    let mut psid: *mut c_void = std::ptr::null_mut();
    if unsafe { ConvertStringSidToSidW(sid_w.as_ptr(), &mut psid) } == 0 {
        return Err(anyhow::anyhow!(
            "ConvertStringSidToSidW failed: {}",
            unsafe { GetLastError() }
        ));
    }
    Ok(psid)
}

fn random_password() -> String {
    const CHARS: &[u8] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()-_=+";
    let mut rng = SmallRng::from_entropy();
    let mut buf = [0u8; 24];
    rng.fill_bytes(&mut buf);
    buf.iter()
        .map(|b| {
            let idx = (*b as usize) % CHARS.len();
            CHARS[idx] as char
        })
        .collect()
}

#[derive(Serialize)]
struct SandboxUserRecord {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct SandboxUsersFile {
    version: u32,
    offline: SandboxUserRecord,
    online: SandboxUserRecord,
}

#[derive(Serialize)]
struct SetupMarker {
    version: u32,
    offline_username: String,
    online_username: String,
    created_at: String,
    proxy_ports: Vec<u16>,
    allow_local_binding: bool,
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
}

fn write_secrets(
    codex_home: &Path,
    offline_user: &str,
    offline_pwd: &str,
    online_user: &str,
    online_pwd: &str,
    mode: SetupMode,
) -> Result<()> {
    let secrets_dir = sandbox_secrets_dir(codex_home);
    std::fs::create_dir_all(&secrets_dir).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!(
                "failed to create secrets dir {}: {err}",
                secrets_dir.display()
            ),
        ))
    })?;
    let offline_blob = dpapi_protect(offline_pwd.as_bytes()).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperDpapiProtectFailed,
            format!("dpapi protect failed for offline user: {err}"),
        ))
    })?;
    let online_blob = dpapi_protect(online_pwd.as_bytes()).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperDpapiProtectFailed,
            format!("dpapi protect failed for online user: {err}"),
        ))
    })?;
    let users = SandboxUsersFile {
        version: SETUP_VERSION,
        offline: SandboxUserRecord {
            username: offline_user.to_string(),
            password: BASE64.encode(offline_blob),
        },
        online: SandboxUserRecord {
            username: online_user.to_string(),
            password: BASE64.encode(online_blob),
        },
    };
    let users_path = secrets_dir.join("sandbox_users.json");
    let users_json = serde_json::to_vec_pretty(&users).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!("serialize sandbox users failed: {err}"),
        ))
    })?;
    let write_result = match mode {
        SetupMode::ProvisionOnly | SetupMode::OwnedFull => {
            write_file_atomically(&users_path, &users_json)
        }
        SetupMode::Full | SetupMode::InteractiveProvision | SetupMode::ReadAclsOnly => {
            std::fs::write(&users_path, users_json).map_err(anyhow::Error::from)
        }
    };
    write_result.map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperUsersFileWriteFailed,
            format!(
                "write sandbox users file {} failed: {err}",
                users_path.display()
            ),
        ))
    })?;
    Ok(())
}

/// Service provisioning retains the marker's exclusive handle; legacy setup reopens it at commit.
pub(super) enum PreparedSetupMarker {
    Retained(File),
    Reopen,
}

// Create the final marker with its protected ACL. The empty file intentionally fails readiness
// checks until setup succeeds. Only service provisioning pins it for the entire operation.
pub(super) fn prepare_setup_marker(
    codex_home: &Path,
    real_user: &str,
    mode: SetupMode,
) -> Result<PreparedSetupMarker> {
    let marker_path = sandbox_dir(codex_home).join("setup_marker.json");
    match std::fs::remove_file(&marker_path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperSetupMarkerWriteFailed,
                format!(
                    "remove setup marker file {} failed: {err}",
                    marker_path.display()
                ),
            )));
        }
    }

    let real_user_sid = resolve_sid(real_user)
        .and_then(|sid| string_from_sid_bytes(&sid).map_err(anyhow::Error::msg))
        .map_err(|err| {
            anyhow::Error::new(SetupFailure::new(
                SetupErrorCode::HelperSetupMarkerWriteFailed,
                format!("resolve real user SID for setup marker failed: {err}"),
            ))
        })?;
    let sddl = to_wide(format!(
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{real_user_sid})"
    ));
    let mut security_descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    let converted = unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut security_descriptor,
            std::ptr::null_mut(),
        )
    };
    if converted == 0 {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!(
                "create setup marker security descriptor failed: {}",
                unsafe { GetLastError() }
            ),
        )));
    }

    let security_attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security_descriptor,
        bInheritHandle: 0,
    };
    let marker_path_wide = to_wide(marker_path.as_os_str());
    let marker_handle = unsafe {
        CreateFileW(
            marker_path_wide.as_ptr(),
            GENERIC_WRITE,
            /*dwsharemode*/ 0,
            &security_attributes,
            CREATE_NEW,
            FILE_ATTRIBUTE_NORMAL,
            /*htemplatefile*/ 0,
        )
    };
    let create_error = unsafe { GetLastError() };
    unsafe {
        LocalFree(security_descriptor as _);
    }
    if marker_handle == INVALID_HANDLE_VALUE {
        return Err(anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!(
                "create protected setup marker file {} failed: {}",
                marker_path.display(),
                create_error
            ),
        )));
    }
    let file = unsafe { File::from_raw_handle(marker_handle as *mut c_void) };
    match mode {
        SetupMode::ProvisionOnly | SetupMode::OwnedFull => Ok(PreparedSetupMarker::Retained(file)),
        SetupMode::Full | SetupMode::InteractiveProvision | SetupMode::ReadAclsOnly => {
            drop(file);
            Ok(PreparedSetupMarker::Reopen)
        }
    }
}

pub(super) fn commit_setup_marker(
    file: PreparedSetupMarker,
    codex_home: &Path,
    offline_user: &str,
    online_user: &str,
    proxy_ports: &[u16],
    allow_local_binding: bool,
) -> Result<()> {
    commit_setup_marker_with_roots(
        file,
        codex_home,
        offline_user,
        online_user,
        proxy_ports,
        allow_local_binding,
        &[],
        &[],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn commit_setup_marker_with_roots(
    file: PreparedSetupMarker,
    codex_home: &Path,
    offline_user: &str,
    online_user: &str,
    proxy_ports: &[u16],
    allow_local_binding: bool,
    read_roots: &[PathBuf],
    write_roots: &[PathBuf],
) -> Result<()> {
    let marker = SetupMarker {
        version: SETUP_VERSION,
        offline_username: offline_user.to_string(),
        online_username: online_user.to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        proxy_ports: proxy_ports.to_vec(),
        allow_local_binding,
        read_roots: read_roots.to_vec(),
        write_roots: write_roots.to_vec(),
    };
    let marker_path = sandbox_dir(codex_home).join("setup_marker.json");
    let marker_json = serde_json::to_vec_pretty(&marker).map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!("serialize setup marker failed: {err}"),
        ))
    })?;
    let write_result = match file {
        PreparedSetupMarker::Retained(mut file) => file.write_all(&marker_json),
        PreparedSetupMarker::Reopen => std::fs::write(&marker_path, marker_json),
    };
    write_result.map_err(|err| {
        anyhow::Error::new(SetupFailure::new(
            SetupErrorCode::HelperSetupMarkerWriteFailed,
            format!(
                "write setup marker file {} failed: {err}",
                marker_path.display()
            ),
        ))
    })?;
    Ok(())
}
