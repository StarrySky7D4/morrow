//! Disables sandbox accounts for cleanup and removes their profiles and principals.
//! Callers must restore original flags if preparation for cleanup fails.

use std::ptr::null;

use anyhow::Result;
use anyhow::bail;
use anyhow::ensure;
use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows_sys::Win32::Foundation::ERROR_SUCCESS;
use windows_sys::Win32::NetworkManagement::NetManagement as network;
use windows_sys::Win32::System::Registry as registry;
use windows_sys::Win32::UI::Shell::DeleteProfileW;

use crate::setup::OFFLINE_USERNAME;
use crate::setup::ONLINE_USERNAME;
use crate::setup_provisioning::OwnedHiddenUsersProof;
use crate::setup_provisioning::OwnedPrincipalsProof;
use crate::setup_provisioning::WINLOGON_USERLIST_KEY_PATH;
use crate::winutil::local_user_flags;
use crate::winutil::resolve_sid;
use crate::winutil::set_local_user_flags;
use crate::winutil::string_from_sid_bytes;
use crate::winutil::to_wide;

use super::retained_logons::RetainedLogons;

const ERROR_NONE_MAPPED_WIN32: i32 = 1332;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedPrincipalState {
    pub offline_flags: Option<u32>,
    pub offline_sid_bytes: Option<Vec<u8>>,
    pub online_flags: Option<u32>,
    pub online_sid_bytes: Option<Vec<u8>>,
    pub group_sid_bytes: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedHiddenUserEntry {
    pub username: String,
    pub reg_type: Option<u32>,
    pub reg_dword_value: Option<u32>,
}

pub(crate) fn verify_owned_principals_before_disable(
    proof: &OwnedPrincipalsProof,
    observed: &ObservedPrincipalState,
) -> Result<()> {
    ensure!(
        proof.offline_user.username == OFFLINE_USERNAME
            && proof.online_user.username == ONLINE_USERNAME,
        "owned principals proof usernames do not match canonical sandbox accounts"
    );
    ensure!(
        observed.offline_flags.is_some(),
        "owned sandbox account is missing: {}",
        proof.offline_user.username
    );
    ensure!(
        observed.offline_sid_bytes.as_deref() == Some(proof.offline_user.sid_bytes.as_slice()),
        "owned sandbox account SID mismatch for {}",
        proof.offline_user.username
    );
    ensure!(
        observed.online_flags.is_some(),
        "owned sandbox account is missing: {}",
        proof.online_user.username
    );
    ensure!(
        observed.online_sid_bytes.as_deref() == Some(proof.online_user.sid_bytes.as_slice()),
        "owned sandbox account SID mismatch for {}",
        proof.online_user.username
    );
    ensure!(
        observed.group_sid_bytes.as_deref() == Some(proof.group_sid_bytes.as_slice()),
        "owned sandbox group SID mismatch or missing for {}",
        proof.group_name
    );
    Ok(())
}

pub(crate) fn verify_owned_principals_after_disable(
    proof: &OwnedPrincipalsProof,
    observed: &ObservedPrincipalState,
) -> Result<()> {
    verify_owned_principals_before_disable(proof, observed)?;
    ensure!(
        observed
            .offline_flags
            .is_some_and(|flags| flags & network::UF_ACCOUNTDISABLE != 0),
        "owned sandbox account is not disabled: {}",
        proof.offline_user.username
    );
    ensure!(
        observed
            .online_flags
            .is_some_and(|flags| flags & network::UF_ACCOUNTDISABLE != 0),
        "owned sandbox account is not disabled: {}",
        proof.online_user.username
    );
    Ok(())
}

pub(crate) fn verify_owned_hidden_user_entries_plan(
    proof: &OwnedHiddenUsersProof,
    entries: &[ObservedHiddenUserEntry],
) -> Result<()> {
    ensure!(
        proof.key_path == WINLOGON_USERLIST_KEY_PATH,
        "owned hidden-user proof key path mismatch: {}",
        proof.key_path
    );
    ensure!(
        proof.usernames[0] == OFFLINE_USERNAME
            && proof.usernames[1] == ONLINE_USERNAME
            && proof.expected_dword_value == 0,
        "owned hidden-user proof username or expected_dword_value mismatch"
    );
    ensure!(
        entries.len() == proof.usernames.len(),
        "owned hidden-user observation count mismatch: expected {}, got {}",
        proof.usernames.len(),
        entries.len()
    );
    for expected_name in &proof.usernames {
        let entry = entries
            .iter()
            .find(|candidate| &candidate.username == expected_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "owned Winlogon UserList hidden-user entry missing from observation for {expected_name}"
                )
            })?;
        ensure!(
            entry.reg_type == Some(registry::REG_DWORD)
                && entry.reg_dword_value == Some(proof.expected_dword_value),
            "owned Winlogon UserList hidden-user entry missing or altered for {expected_name}"
        );
    }
    Ok(())
}

fn resolve_optional_sid(name: &str) -> Result<Option<Vec<u8>>> {
    match resolve_sid(name) {
        Ok(sid) => Ok(Some(sid)),
        Err(err) => {
            if err
                .downcast_ref::<std::io::Error>()
                .and_then(std::io::Error::raw_os_error)
                == Some(ERROR_NONE_MAPPED_WIN32)
            {
                Ok(None)
            } else {
                Err(err)
            }
        }
    }
}

fn observe_current_principals(group_name: &str) -> Result<ObservedPrincipalState> {
    let offline_flags = local_user_flags(OFFLINE_USERNAME)?;
    let offline_sid_bytes = if offline_flags.is_some() {
        Some(resolve_sid(OFFLINE_USERNAME)?)
    } else {
        None
    };
    let online_flags = local_user_flags(ONLINE_USERNAME)?;
    let online_sid_bytes = if online_flags.is_some() {
        Some(resolve_sid(ONLINE_USERNAME)?)
    } else {
        None
    };
    let group_sid_bytes = resolve_optional_sid(group_name)?;
    Ok(ObservedPrincipalState {
        offline_flags,
        offline_sid_bytes,
        online_flags,
        online_sid_bytes,
        group_sid_bytes,
    })
}

pub(super) fn verify_live_owned_principals(proof: &OwnedPrincipalsProof) -> Result<()> {
    let observed = observe_current_principals(&proof.group_name)?;
    verify_owned_principals_before_disable(proof, &observed)?;
    ensure!(
        observed.offline_flags == Some(proof.offline_user.initial_flags)
            && (proof.offline_user.initial_flags & network::UF_ACCOUNTDISABLE) == 0,
        "owned sandbox account {} live flags {:?} do not match freshly provisioned initial_flags {}",
        proof.offline_user.username,
        observed.offline_flags,
        proof.offline_user.initial_flags
    );
    ensure!(
        observed.online_flags == Some(proof.online_user.initial_flags)
            && (proof.online_user.initial_flags & network::UF_ACCOUNTDISABLE) == 0,
        "owned sandbox account {} live flags {:?} do not match freshly provisioned initial_flags {}",
        proof.online_user.username,
        observed.online_flags,
        proof.online_user.initial_flags
    );
    Ok(())
}

#[derive(Default)]
pub(super) struct DisabledSandboxUsers {
    users: Vec<SandboxUser>,
}

struct SandboxUser {
    name: &'static str,
    original_flags: u32,
    sid: Vec<u8>,
}

impl DisabledSandboxUsers {
    pub(super) fn disable(&mut self) -> Result<()> {
        for name in [OFFLINE_USERNAME, ONLINE_USERNAME] {
            let Some(original_flags) = local_user_flags(name)? else {
                continue;
            };
            let sid = resolve_sid(name)?;

            self.users.push(SandboxUser {
                name,
                original_flags,
                sid,
            });
            set_local_user_flags(name, original_flags | network::UF_ACCOUNTDISABLE)?;
        }
        Ok(())
    }

    pub(super) fn disable_owned(&mut self, proof: &OwnedPrincipalsProof) -> Result<()> {
        let observed = observe_current_principals(&proof.group_name)?;
        verify_owned_principals_before_disable(proof, &observed)?;
        let offline_flags = observed
            .offline_flags
            .ok_or_else(|| anyhow::anyhow!("verified offline_flags missing"))?;
        let online_flags = observed
            .online_flags
            .ok_or_else(|| anyhow::anyhow!("verified online_flags missing"))?;

        self.users.push(SandboxUser {
            name: OFFLINE_USERNAME,
            original_flags: offline_flags,
            sid: proof.offline_user.sid_bytes.clone(),
        });
        set_local_user_flags(
            OFFLINE_USERNAME,
            offline_flags | network::UF_ACCOUNTDISABLE,
        )?;

        self.users.push(SandboxUser {
            name: ONLINE_USERNAME,
            original_flags: online_flags,
            sid: proof.online_user.sid_bytes.clone(),
        });
        if let Err(err) = set_local_user_flags(
            ONLINE_USERNAME,
            online_flags | network::UF_ACCOUNTDISABLE,
        ) {
            let _ = self.restore();
            self.users.clear();
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn sids(&self) -> impl Iterator<Item = &[u8]> {
        self.users.iter().map(|user| user.sid.as_slice())
    }

    pub(super) fn validate_current(&self) -> Result<()> {
        for name in [OFFLINE_USERNAME, ONLINE_USERNAME] {
            let Some(flags) = local_user_flags(name)? else {
                continue;
            };
            let captured = self.users.iter().find(|user| user.name == name);
            ensure!(
                captured.is_some_and(|user| resolve_sid(name).is_ok_and(|sid| sid == user.sid))
                    && flags & network::UF_ACCOUNTDISABLE != 0,
                "sandbox account changed after being disabled: {name}"
            );
        }
        Ok(())
    }

    pub(super) fn validate_current_owned(&self, proof: &OwnedPrincipalsProof) -> Result<()> {
        ensure!(
            self.users.len() == 2
                && self.users[0].name == OFFLINE_USERNAME
                && self.users[0].sid == proof.offline_user.sid_bytes
                && self.users[1].name == ONLINE_USERNAME
                && self.users[1].sid == proof.online_user.sid_bytes,
            "captured disabled sandbox users do not match owned principals proof"
        );
        let observed = observe_current_principals(&proof.group_name)?;
        verify_owned_principals_after_disable(proof, &observed)
    }

    pub(super) fn restore(&self) -> Result<()> {
        let mut errors = Vec::new();
        for user in &self.users {
            if let Err(error) = set_local_user_flags(user.name, user.original_flags)
                && error
                    .downcast_ref::<std::io::Error>()
                    .and_then(std::io::Error::raw_os_error)
                    != Some(network::NERR_UserNotFound as i32)
            {
                errors.push(format!("{error:#}"));
            }
        }
        if !errors.is_empty() {
            bail!("{}", errors.join("; "));
        }
        Ok(())
    }

    pub(super) fn remove_users(
        &self,
        retained: &RetainedLogons,
        report: impl Fn(&str),
    ) -> Result<()> {
        let mut errors = Vec::new();
        for user in &self.users {
            if retained.contains_sid(&user.sid) {
                report(&format!(
                    "deferring {} profile and account to runtime cleanup",
                    user.name
                ));
                continue;
            }
            match user.remove() {
                Ok(()) => report(&format!("removed {} profile and account", user.name)),
                Err(error) => {
                    report(&format!(
                        "remove {} profile and account: failed, {error:#}",
                        user.name
                    ));
                    errors.push(format!("{error:#}"));
                }
            }
        }
        ensure!(errors.is_empty(), "{}", errors.join("; "));
        Ok(())
    }

    pub(super) fn remove_owned_users_and_group(
        &self,
        proof: &OwnedPrincipalsProof,
        report: impl Fn(&str),
    ) -> Result<(Vec<String>, String)> {
        self.validate_current_owned(proof)?;
        let mut removed_users = Vec::with_capacity(self.users.len());
        let mut errors = Vec::new();
        for user in &self.users {
            match user.remove_owned() {
                Ok(()) => {
                    report(&format!("removed {} profile and account", user.name));
                    removed_users.push(user.name.to_string());
                }
                Err(error) => {
                    report(&format!(
                        "remove {} profile and account: failed, {error:#}",
                        user.name
                    ));
                    errors.push(format!("{error:#}"));
                }
            }
        }
        ensure!(errors.is_empty(), "{}", errors.join("; "));

        match remove_owned_sandbox_principal(&proof.group_name, &proof.group_sid_bytes) {
            Ok(()) => {
                report(&format!("removed local group {}", proof.group_name));
                Ok((removed_users, proof.group_name.clone()))
            }
            Err(error) => {
                report(&format!(
                    "remove local group {}: failed, {error:#}",
                    proof.group_name
                ));
                Err(error)
            }
        }
    }
}

impl SandboxUser {
    fn remove_profile_if_present(&self) -> Result<()> {
        let Self { name, sid, .. } = self;
        let sid = string_from_sid_bytes(sid).map_err(anyhow::Error::msg)?;
        let profile_key = to_wide(format!(
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\{sid}"
        ));
        let mut key = 0;
        let status = unsafe {
            registry::RegOpenKeyExW(
                registry::HKEY_LOCAL_MACHINE,
                profile_key.as_ptr(),
                /*uloptions*/ 0,
                registry::KEY_READ | registry::KEY_WOW64_64KEY,
                &mut key,
            )
        };
        match status {
            ERROR_SUCCESS => {
                unsafe { registry::RegCloseKey(key) };
                // Keep the account's SID available for retry if its profile is still loaded.
                if unsafe { DeleteProfileW(to_wide(sid).as_ptr(), null(), null()) } == 0 {
                    bail!(
                        "remove sandbox profile for {name}: {}",
                        std::io::Error::last_os_error()
                    );
                }
            }
            ERROR_FILE_NOT_FOUND => {}
            status => bail!("open sandbox profile for {name}: {status}"),
        }
        Ok(())
    }

    fn remove(&self) -> Result<()> {
        self.remove_profile_if_present()?;
        remove_sandbox_principal(self.name)
    }

    fn remove_owned(&self) -> Result<()> {
        self.remove_profile_if_present()?;
        remove_owned_sandbox_principal(self.name, &self.sid)
    }
}

#[cfg(test)]
impl DisabledSandboxUsers {
    // In-memory admission fixture only; this does not resolve or change a local account.
    pub(super) fn for_token_test(sid: Vec<u8>) -> Self {
        Self {
            users: vec![SandboxUser {
                name: OFFLINE_USERNAME,
                original_flags: network::UF_ACCOUNTDISABLE,
                sid,
            }],
        }
    }
}

pub(super) fn remove_sandbox_principal(name: &str) -> Result<()> {
    let name_wide = to_wide(name);
    let status = if name == "CodexSandboxUsers" {
        unsafe { network::NetLocalGroupDel(null(), name_wide.as_ptr()) }
    } else {
        unsafe { network::NetUserDel(null(), name_wide.as_ptr()) }
    };
    match status {
        network::NERR_Success | network::NERR_GroupNotFound | network::NERR_UserNotFound => Ok(()),
        status => bail!("remove local sandbox principal {name}: {status}"),
    }
}

fn remove_owned_sandbox_principal(name: &str, expected_sid: &[u8]) -> Result<()> {
    let current_sid = resolve_sid(name)?;
    ensure!(
        current_sid == expected_sid,
        "owned sandbox principal SID changed before removal for {name}"
    );
    let name_wide = to_wide(name);
    let status = if name == "CodexSandboxUsers" {
        unsafe { network::NetLocalGroupDel(null(), name_wide.as_ptr()) }
    } else {
        unsafe { network::NetUserDel(null(), name_wide.as_ptr()) }
    };
    ensure!(
        status == network::NERR_Success,
        "remove owned local sandbox principal {name}: {status}"
    );
    Ok(())
}

pub(super) fn verify_owned_hidden_user_entries(proof: &OwnedHiddenUsersProof) -> Result<()> {
    let key_path = to_wide(proof.key_path);
    let mut key = 0;
    let status = unsafe {
        registry::RegOpenKeyExW(
            registry::HKEY_LOCAL_MACHINE,
            key_path.as_ptr(),
            0,
            registry::KEY_QUERY_VALUE | registry::KEY_WOW64_64KEY,
            &mut key,
        )
    };
    ensure!(
        status == ERROR_SUCCESS,
        "open owned Winlogon UserList key failed: {status}"
    );
    let mut entries = Vec::with_capacity(proof.usernames.len());
    for username in &proof.usernames {
        let value_name = to_wide(username);
        let mut reg_type = 0u32;
        let mut data = 0u32;
        let mut data_len = std::mem::size_of::<u32>() as u32;
        let query_status = unsafe {
            registry::RegQueryValueExW(
                key,
                value_name.as_ptr(),
                null(),
                &mut reg_type,
                (&mut data as *mut u32).cast(),
                &mut data_len,
            )
        };
        if query_status == ERROR_SUCCESS && data_len == std::mem::size_of::<u32>() as u32 {
            entries.push(ObservedHiddenUserEntry {
                username: username.clone(),
                reg_type: Some(reg_type),
                reg_dword_value: Some(data),
            });
        } else if query_status == ERROR_FILE_NOT_FOUND {
            entries.push(ObservedHiddenUserEntry {
                username: username.clone(),
                reg_type: None,
                reg_dword_value: None,
            });
        } else {
            unsafe { registry::RegCloseKey(key) };
            bail!("query owned Winlogon UserList entry {username} failed: {query_status}");
        }
    }
    unsafe { registry::RegCloseKey(key) };
    verify_owned_hidden_user_entries_plan(proof, &entries)
}

pub(super) fn remove_owned_hidden_user_entries(
    proof: &OwnedHiddenUsersProof,
) -> Result<Vec<String>> {
    verify_owned_hidden_user_entries(proof)?;
    let key_path = to_wide(proof.key_path);
    let mut key = 0;
    let status = unsafe {
        registry::RegOpenKeyExW(
            registry::HKEY_LOCAL_MACHINE,
            key_path.as_ptr(),
            0,
            registry::KEY_QUERY_VALUE | registry::KEY_SET_VALUE | registry::KEY_WOW64_64KEY,
            &mut key,
        )
    };
    ensure!(
        status == ERROR_SUCCESS,
        "open owned Winlogon UserList key for removal failed: {status}"
    );
    let mut removed = Vec::with_capacity(proof.usernames.len());
    for username in &proof.usernames {
        let value_name = to_wide(username);
        let delete_status = unsafe { registry::RegDeleteValueW(key, value_name.as_ptr()) };
        if delete_status != ERROR_SUCCESS {
            unsafe { registry::RegCloseKey(key) };
            bail!("delete owned Winlogon UserList entry {username} failed: {delete_status}");
        }
        removed.push(username.clone());
    }
    unsafe { registry::RegCloseKey(key) };
    Ok(removed)
}

#[cfg(test)]
#[path = "principals_tests.rs"]
mod tests;

