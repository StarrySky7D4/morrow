//! Stops sandbox work under the setup lock before removing its resources and protections.

use std::os::windows::io::BorrowedHandle;
use std::path::Path;

use anyhow::Result;
use anyhow::anyhow;

use crate::setup::OFFLINE_USERNAME;
use crate::setup::ONLINE_USERNAME;

mod firewall;
mod principals;
mod processes;
mod retained_logons;

/// Removes sandbox resources created for one authenticated packaged installation.
/// Keep a supplied home and its ancestors pinned until `clean_up_desktop` starts.
/// That callback removes user-owned desktop files while the sandbox accounts remain disabled.
/// `report` must remain usable after the home and its log files have been removed.
pub fn clean_up_packaged_windows_sandbox(
    codex_home: Option<&Path>,
    report: impl Fn(&str),
    clean_up_desktop: impl FnOnce() -> Result<()>,
) -> Result<()> {
    prepare_packaged_windows_sandbox_cleanup()?.finish(codex_home, report, clean_up_desktop)
}

/// Holds the setup lock after sandbox accounts are disabled, their processes
/// have exited, and all logon tokens except explicitly retained private cleanup
/// tokens have been released.
///
/// Keep this guard on the thread that acquired the setup lock. Callers must not
/// re-enable the accounts or create new sandbox logons before `finish`.
///
/// Dropping the guard only releases the lock. It does not re-enable accounts,
/// remove protections, or undo work performed between the two phases.
#[must_use = "finish cleanup or leave the sandbox accounts disabled with their protections intact"]
pub struct PreparedWindowsSandboxCleanup {
    _setup_lock: crate::setup_mutex::SandboxSetupLock,
    users: principals::DisabledSandboxUsers,
    _retained_logons: retained_logons::RetainedLogons,
}

/// Disables sandbox accounts and waits for their processes and logon tokens to exit.
///
/// No resources or protections are removed here. On failure, restoration of the
/// original account flags is attempted before the setup lock is released; any
/// restoration failure is included in the returned error.
pub fn prepare_packaged_windows_sandbox_cleanup() -> Result<PreparedWindowsSandboxCleanup> {
    prepare_packaged_windows_sandbox_cleanup_with_retained_tokens(&[])
}

/// Retains only the trusted runtime finalizer's login tokens while stopping sandbox work.
/// These must be fresh, private logins used only by that SYSTEM cleanup process, never
/// runner tokens or logins shared with sandbox commands. Ordinary processes are still stopped.
/// The returned guard pins their identities until native cleanup finishes.
pub fn prepare_packaged_windows_sandbox_cleanup_with_retained_tokens(
    tokens: &[BorrowedHandle<'_>],
) -> Result<PreparedWindowsSandboxCleanup> {
    let setup_lock = crate::setup_mutex::acquire_sandbox_setup_lock(/*timeout_ms*/ 5_000)?;
    let mut errors = Vec::new();
    let mut users = principals::DisabledSandboxUsers::default();
    let mut retained = retained_logons::RetainedLogons::default();
    if let Err(error) = users.disable().and_then(|()| {
        retained = retained_logons::RetainedLogons::capture(tokens, &users)?;
        processes::stop(&users, &retained)
    }) {
        errors.push(format!("{error:#}"));
        // No network protections have been removed, so failed preparation can restore these flags.
        if let Err(error) = users.restore() {
            errors.push(format!("{error:#}"));
        }
        return Err(anyhow!(errors.join("; ")));
    }

    Ok(PreparedWindowsSandboxCleanup {
        _setup_lock: setup_lock,
        users,
        _retained_logons: retained,
    })
}

impl PreparedWindowsSandboxCleanup {
    /// Removes resources while retaining the setup lock and disabled accounts.
    ///
    /// Keep a supplied home and its ancestors pinned until `clean_up_desktop`
    /// starts. Independent cleanup steps continue after an error, as in
    /// `clean_up_packaged_windows_sandbox`. Retrying keeps the original account
    /// identities; it must not adopt replacement accounts created between attempts.
    /// `report` records each cleanup outcome without relying on files in the home.
    pub fn finish(
        &self,
        codex_home: Option<&Path>,
        report: impl Fn(&str),
        clean_up_desktop: impl FnOnce() -> Result<()>,
    ) -> Result<()> {
        self.users.validate_current()?;
        let mut errors = Vec::new();

        if let Some(codex_home) = codex_home {
            if let Err(error) = crate::logging::release_setup_log() {
                errors.push(format!("release setup log: {error:#}"));
            }
            for (directory, path) in [
                (".sandbox", crate::setup::sandbox_dir(codex_home)),
                (
                    ".sandbox-secrets",
                    crate::setup::sandbox_secrets_dir(codex_home),
                ),
                (".sandbox-bin", crate::setup::sandbox_bin_dir(codex_home)),
            ] {
                match std::fs::remove_dir_all(path) {
                    Ok(()) => report(&format!("removed {directory}")),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        report(&format!("skipping {directory}: not found"));
                    }
                    Err(error) => {
                        let message = format!("remove {directory}: failed, {error}");
                        report(&message);
                        errors.push(message);
                    }
                }
            }
        } else {
            report("skipping sandbox directories: no pinned home");
        }

        if let Err(error) = clean_up_desktop() {
            errors.push(format!("{error:#}"));
        }

        for (operation, result) in [
            ("remove WFP filters", crate::wfp::remove_wfp_filters()),
            ("remove firewall rules", firewall::cleanup_firewall_rules()),
            (
                "remove hidden-user entries",
                crate::hide_users::unhide_sandbox_users(&[OFFLINE_USERNAME, ONLINE_USERNAME]),
            ),
        ] {
            match result {
                Ok(()) => report(&format!("{operation}: completed")),
                Err(error) => {
                    let message = format!("{operation}: failed, {error:#}");
                    report(&message);
                    errors.push(message);
                }
            }
        }
        if let Err(error) = self.users.remove_users(&self._retained_logons, &report) {
            errors.push(format!("{error:#}"));
        }
        // Retry home permission cleanup with the same group; retained runtime users
        // and their group are removed by the finalizer after their profiles unload.
        if errors.is_empty()
            && !self
                .users
                .sids()
                .any(|sid| self._retained_logons.contains_sid(sid))
        {
            match principals::remove_sandbox_principal("CodexSandboxUsers") {
                Ok(()) => report("remove sandbox group: completed"),
                Err(error) => {
                    report(&format!("remove sandbox group: failed, {error:#}"));
                    errors.push(format!("{error:#}"));
                }
            }
        } else {
            report("retaining sandbox group until cleanup finishes");
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(errors.join("; ")))
        }
    }
}

/// Bounded, typed retirement evidence for one setup-owned normal cleanup.
///
/// This report intentionally omits passwords, DPAPI blobs, raw tokens, and raw
/// OS handles, and does not implement `Deserialize`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct WindowsSandboxNormalCleanupReport {
    pub codex_home: std::path::PathBuf,
    pub offline_sid: String,
    pub online_sid: String,
    pub group_sid: String,
    pub removed_directories: Vec<String>,
    pub wfp_retirement: crate::wfp::WfpRetirementReport,
    pub removed_firewall_rules: Vec<String>,
    pub removed_hidden_user_entries: Vec<String>,
    pub removed_users: Vec<String>,
    pub removed_group: String,
}

/// Holds the setup lock and the live setup ownership proof after owned sandbox
/// accounts have been disabled and verified to have zero running processes and
/// zero logon sessions without forceful termination.
///
/// Dropping this guard only releases the lock and directory pins; it never
/// re-enables accounts or removes protections/resources.
#[must_use = "finish normal owned cleanup or leave the sandbox accounts disabled with their protections intact"]
pub struct PreparedOwnedWindowsSandboxCleanup {
    _setup_lock: crate::setup_mutex::SandboxSetupLock,
    setup: crate::setup_provisioning::OwnedWindowsSandboxSetup,
    users: principals::DisabledSandboxUsers,
    retained_logons: retained_logons::RetainedLogons,
}

pub(crate) fn verify_live_owned_principals(
    proof: &crate::setup_provisioning::OwnedPrincipalsProof,
) -> Result<()> {
    principals::verify_live_owned_principals(proof)
}

pub(crate) fn verify_live_owned_hidden_users(
    proof: &crate::setup_provisioning::OwnedHiddenUsersProof,
) -> Result<()> {
    principals::verify_owned_hidden_user_entries(proof)
}

pub(crate) fn verify_live_owned_firewall_rules(
    proof: &crate::setup_provisioning::OwnedFirewallProof,
    expected_offline_sid: &str,
) -> Result<()> {
    firewall::verify_owned_firewall_rules(proof, expected_offline_sid)
}

/// Prepares normal, non-forceful cleanup for resources created by the supplied
/// [`crate::setup_provisioning::OwnedWindowsSandboxSetup`].
///
/// Consumes `setup` so a failed or partially-mutated preparation cannot be
/// replayed. Never requests `PROCESS_TERMINATE` or calls `TerminateProcess` /
/// `TerminateJobObject`. Any live sandbox process, active logon session,
/// missing/altered owned object, or unverified process/token state causes
/// preparation to fail closed and restores account flags while keeping all
/// network/filesystem protections intact.
pub fn prepare_owned_windows_sandbox_cleanup(
    setup: crate::setup_provisioning::OwnedWindowsSandboxSetup,
) -> Result<PreparedOwnedWindowsSandboxCleanup> {
    let setup_lock = crate::setup_mutex::acquire_sandbox_setup_lock(/*timeout_ms*/ 5_000)?;
    anyhow::ensure!(
        !crate::app_package::registered_core_requested(),
        "registered_core_requested is active; normal owned cleanup is not admitted"
    );
    anyhow::ensure!(
        crate::runtime_ownership::load_installation()?.is_none(),
        "packaged installation record exists; normal owned cleanup is not admitted"
    );
    anyhow::ensure!(
        !crate::setup_provisioning::read_acl_mutex_exists()?,
        "Local\\CodexSandboxReadAcl is active; concurrent ACL helper or setup work is still running"
    );
    anyhow::ensure!(
        setup.principals.offline_user.sid == setup.firewall.offline_sid
            && setup.principals.offline_user.sid == setup.wfp.offline_sid
            && setup.principals.offline_user.sid_bytes == setup.wfp.offline_sid_bytes,
        "owned setup proof offline SID is inconsistent across principals, firewall, and WFP proofs"
    );
    crate::setup_provisioning::verify_owned_home_continuity(&setup.home)?;
    principals::verify_owned_hidden_user_entries(&setup.hidden_users)?;
    firewall::verify_owned_firewall_rules(
        &setup.firewall,
        &setup.principals.offline_user.sid,
    )?;

    let mut users = principals::DisabledSandboxUsers::default();
    let retained = retained_logons::RetainedLogons::none_for_normal_cleanup();
    if let Err(error) = users.disable_owned(&setup.principals).and_then(|()| {
        retained.verify_empty_for_normal_cleanup()?;
        processes::verify_no_running_processes_or_logons(&users, &retained)
    }) {
        let mut errors = vec![format!("{error:#}")];
        if let Err(restore_error) = users.restore() {
            errors.push(format!("{restore_error:#}"));
        }
        return Err(anyhow!(errors.join("; ")));
    }

    Ok(PreparedOwnedWindowsSandboxCleanup {
        _setup_lock: setup_lock,
        setup,
        users,
        retained_logons: retained,
    })
}

impl PreparedOwnedWindowsSandboxCleanup {
    pub fn codex_home(&self) -> &Path {
        self.setup.codex_home()
    }

    pub fn offline_username(&self) -> &str {
        self.setup.offline_username()
    }

    pub fn offline_sid(&self) -> &str {
        self.setup.offline_sid()
    }

    pub fn online_username(&self) -> &str {
        self.setup.online_username()
    }

    pub fn online_sid(&self) -> &str {
        self.setup.online_sid()
    }

    pub fn group_name(&self) -> &str {
        self.setup.group_name()
    }

    pub fn group_sid(&self) -> &str {
        self.setup.group_sid()
    }

    pub fn firewall_rule_names(&self) -> Vec<&'static str> {
        self.setup.firewall_rule_names()
    }

    pub fn wfp_filter_ids(&self) -> Vec<u64> {
        self.setup.wfp_filter_ids()
    }

    /// Retires only the exact resources and protections proven to have been
    /// created by this case's [`crate::setup_provisioning::OwnedWindowsSandboxSetup`].
    ///
    /// Consumes `self` so `finish` cannot be replayed. Shared WFP provider and
    /// sublayer objects are retired only if this case created them and no other
    /// references remain; uncreated legacy firewall rules are never touched.
    pub fn finish(
        mut self,
        report: impl Fn(&str),
    ) -> Result<WindowsSandboxNormalCleanupReport> {
        anyhow::ensure!(
            !crate::app_package::registered_core_requested(),
            "registered_core_requested is active; normal owned cleanup finish is not admitted"
        );
        anyhow::ensure!(
            crate::runtime_ownership::load_installation()?.is_none(),
            "packaged installation record exists; normal owned cleanup finish is not admitted"
        );
        anyhow::ensure!(
            !crate::setup_provisioning::read_acl_mutex_exists()?,
            "Local\\CodexSandboxReadAcl is active during normal owned cleanup finish"
        );
        self.users.validate_current_owned(&self.setup.principals)?;
        self.retained_logons.verify_empty_for_normal_cleanup()?;
        processes::verify_no_running_processes_or_logons(&self.users, &self.retained_logons)?;
        crate::setup_provisioning::verify_owned_home_continuity(&self.setup.home)?;
        principals::verify_owned_hidden_user_entries(&self.setup.hidden_users)?;
        firewall::verify_owned_firewall_rules(
            &self.setup.firewall,
            &self.setup.principals.offline_user.sid,
        )?;

        let wfp_retirement = crate::wfp::retire_owned_wfp_filters(&self.setup.wfp)?;
        report("remove owned WFP filters: completed");

        let removed_firewall_rules = firewall::remove_owned_firewall_rules(
            &self.setup.firewall,
            &self.setup.principals.offline_user.sid,
        )?
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        report("remove owned firewall rules: completed");

        let removed_hidden_user_entries =
            principals::remove_owned_hidden_user_entries(&self.setup.hidden_users)?;
        report("remove owned hidden-user entries: completed");

        crate::logging::release_setup_log()
            .map_err(|error| anyhow!("release setup log: {error:#}"))?;
        crate::setup_provisioning::verify_owned_home_continuity(&self.setup.home)?;
        let codex_home = self.setup.home.codex_home.clone();
        self.setup.home.release_subdir_pins_for_removal();

        let mut removed_directories = Vec::with_capacity(3);
        for (directory, path) in [
            (".sandbox", crate::setup::sandbox_dir(&codex_home)),
            (
                ".sandbox-secrets",
                crate::setup::sandbox_secrets_dir(&codex_home),
            ),
            (".sandbox-bin", crate::setup::sandbox_bin_dir(&codex_home)),
        ] {
            std::fs::remove_dir_all(&path)
                .map_err(|error| anyhow!("remove owned {directory}: {error}"))?;
            report(&format!("removed {directory}"));
            removed_directories.push(directory.to_string());
        }

        let (removed_users, removed_group) = self
            .users
            .remove_owned_users_and_group(&self.setup.principals, &report)?;
        report("remove owned sandbox group: completed");

        Ok(WindowsSandboxNormalCleanupReport {
            codex_home,
            offline_sid: self.setup.principals.offline_user.sid,
            online_sid: self.setup.principals.online_user.sid,
            group_sid: self.setup.principals.group_sid,
            removed_directories,
            wfp_retirement,
            removed_firewall_rules,
            removed_hidden_user_entries,
            removed_users,
            removed_group,
        })
    }
}

