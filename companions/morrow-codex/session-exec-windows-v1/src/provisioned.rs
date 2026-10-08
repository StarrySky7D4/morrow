//! Trusted local provisioning; this module never creates a Core or a Store.
use crate::artifact::LockedArtifact;
use crate::windows_runner::WindowsRunnerProvisioningSpec;
use codex_exec_server::{
    Environment, ExecBackend, ExecParams, ExecServerRuntimeOptions, WindowsSandboxSelection,
};
use codex_http_client::HttpClientFactory;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_sandboxing::SandboxType;
use morrow_agent_session_exec_v1_r2::{Error, Result};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tokio::runtime::{Runtime, RuntimeFlavor};

static NEXT_PROVISIONING: AtomicU64 = AtomicU64::new(1);

/// Holds the actual configured factory, helper lock and scheduler for the
/// complete execution lifetime. Its digest binds this exact factory instance;
/// it is deliberately not a portable authorization or a hash of secret cookies.
pub struct ProvisionedWindowsBackend {
    pub(crate) backend: Arc<dyn ExecBackend>,
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) expected: SandboxType,
    fingerprint: [u8; 32],
    production: bool,
    _options: ExecServerRuntimeOptions,
    _factory: HttpClientFactory,
    _helper: Arc<LockedArtifact>,
    _runner: Option<WindowsRunnerProvisioningSpec>,
}
impl ProvisionedWindowsBackend {
    pub fn production(
        options: ExecServerRuntimeOptions,
        factory: HttpClientFactory,
        helper_sha256: [u8; 32],
        runtime: Arc<Runtime>,
        expected: SandboxType,
    ) -> Result<Arc<Self>> {
        if !matches!(
            expected,
            SandboxType::WindowsRestrictedToken | SandboxType::WindowsMxc
        ) {
            return Err(Error::Denied);
        }
        Self::create(
            options,
            factory,
            helper_sha256,
            runtime,
            expected,
            true,
            None,
            |backend| backend,
        )
    }
    /// Actual ordinary Windows children only; never system sandbox acceptance.
    pub fn ordinary_qualification(
        options: ExecServerRuntimeOptions,
        factory: HttpClientFactory,
        helper_sha256: [u8; 32],
        runtime: Arc<Runtime>,
    ) -> Result<Arc<Self>> {
        Self::create(
            options,
            factory,
            helper_sha256,
            runtime,
            SandboxType::None,
            false,
            None,
            |backend| backend,
        )
    }
    /// Test instrumentation can delay/count the actual provisioned backend.
    /// This entry is always labelled ordinary qualification, never production.
    #[cfg(feature = "qualification-harness")]
    pub fn ordinary_qualification_decorated(
        options: ExecServerRuntimeOptions,
        factory: HttpClientFactory,
        helper_sha256: [u8; 32],
        runtime: Arc<Runtime>,
        decorate: impl FnOnce(Arc<dyn ExecBackend>) -> Arc<dyn ExecBackend>,
    ) -> Result<Arc<Self>> {
        Self::create(
            options,
            factory,
            helper_sha256,
            runtime,
            SandboxType::None,
            false,
            None,
            decorate,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn create(
        options: ExecServerRuntimeOptions,
        factory: HttpClientFactory,
        helper_sha256: [u8; 32],
        runtime: Arc<Runtime>,
        expected: SandboxType,
        production: bool,
        runner: Option<WindowsRunnerProvisioningSpec>,
        decorate: impl FnOnce(Arc<dyn ExecBackend>) -> Arc<dyn ExecBackend>,
    ) -> Result<Arc<Self>> {
        if runtime.handle().runtime_flavor() != RuntimeFlavor::MultiThread
            || runtime.handle().metrics().num_workers() < 2
            || options.codex_linux_sandbox_exe.is_some()
        {
            return Err(Error::Invalid);
        }
        if let Some(runner) = &runner {
            runner.validate_metadata()?;
            if !production || expected != SandboxType::WindowsRestrictedToken {
                return Err(Error::Denied);
            }
        }
        let path = options.codex_self_exe.as_path();
        let helper = path.to_str().ok_or(Error::Invalid)?;
        let cwd = path
            .parent()
            .and_then(|p| p.to_str())
            .ok_or(Error::Invalid)?;
        let helper_lock = Arc::new(LockedArtifact::acquire(helper, helper_sha256, cwd)?);
        let nonce = NEXT_PROVISIONING
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_add(1))
            .map_err(|_| Error::Limit)?;
        // All public runtime options plus an opaque identity for the complete
        // retained HttpClientFactory. Debug never exposes cookie/network data.
        let mut config = serde_json::json!({"helper":helper,"helper_sha":helper_sha256,
            "linux_helper":null,"linux_pid_namespace":format!("{:?}", options.linux_sandbox_pid_namespace),
            "proxy_private_ips":options.proxy_private_ips_via_upstream,
            "expected":format!("{expected:?}"),"production":production,
            "factory_instance":nonce,"process":std::process::id()});
        if let Some(runner) = &runner {
            config["checked_runner"] = runner.identity_json();
        }
        let fingerprint =
            Sha256::digest(serde_json::to_vec(&config).map_err(|_| Error::Invalid)?).into();
        let backend = {
            let _entered = runtime.enter();
            let environment = match &runner {
                Some(runner) => Environment::create_with_windows_runner(
                    options.clone(),
                    factory.clone(),
                    runner.artifact(),
                ),
                None => Environment::create(None, options.clone(), factory.clone()),
            }
            .map_err(|_| Error::Storage)?;
            decorate(environment.get_exec_backend())
        };
        Ok(Arc::new(Self {
            backend,
            runtime,
            expected,
            fingerprint,
            production,
            _options: options,
            _factory: factory,
            _helper: helper_lock,
            _runner: runner,
        }))
    }
    pub fn is_production(&self) -> bool {
        self.production
    }
    pub fn provisioning_sha256(&self) -> [u8; 32] {
        self.fingerprint
    }
    pub(crate) fn domain(&self, params: &ExecParams) -> Result<String> {
        if self.production != params.sandbox.is_some() {
            return Err(Error::Denied);
        }
        if let Some(runner) = &self._runner {
            runner.validate_metadata()?;
            crate::windows_runner::validate_checked_selection(self.expected, params)?;
        }
        if self.production {
            validate_production_params(self.expected, params)?;
        }
        let mut value = serde_json::to_value(params).map_err(|_| Error::Invalid)?;
        value.sort_all_objects();
        let mut digest = Sha256::new();
        let domain: &[u8] = if self._runner.is_some() {
            b"morrow-borrowed-windows-checked-runner-v1\0"
        } else {
            b"morrow-borrowed-windows-provisioned-v1\0"
        };
        digest.update(domain);
        digest.update(self.fingerprint);
        digest.update(serde_json::to_vec(&value).map_err(|_| Error::Invalid)?);
        let sha = digest.finalize();
        Ok(format!(
            "{}-{}",
            if self._runner.is_some() {
                "windows-checked-runner-v1"
            } else {
                "windows-provisioned-v1"
            },
            sha.iter().map(|b| format!("{b:02x}")).collect::<String>()
        ))
    }
}

/// Reject fixed upstream incompatibilities before the original R2 start marker
/// and before executor-local proxy preparation. This does not probe/create a
/// Windows security environment or prove that an OS sandbox was established.
/// MXC's supported full-access profiles remain subject to trusted host approval.
pub(crate) fn validate_production_params(expected: SandboxType, params: &ExecParams) -> Result<()> {
    let context = params.sandbox.as_ref().ok_or(Error::Denied)?;
    let level = match (expected, context.windows_sandbox_selection) {
        (SandboxType::WindowsRestrictedToken, WindowsSandboxSelection::RestrictedToken) => {
            Some(WindowsSandboxLevel::RestrictedToken)
        }
        (SandboxType::WindowsRestrictedToken, WindowsSandboxSelection::Elevated) => {
            Some(WindowsSandboxLevel::Elevated)
        }
        (SandboxType::WindowsMxc, WindowsSandboxSelection::Mxc) => None,
        _ => return Err(Error::Denied),
    };
    if params.argv.is_empty() {
        return Err(Error::Invalid);
    }
    params.cwd.to_abs_path().map_err(|_| Error::Invalid)?;
    let policy_cwd = context.cwd.to_abs_path().map_err(|_| Error::Invalid)?;
    context
        .validate_file_system_paths_for_current_host()
        .map_err(|_| Error::Invalid)?;
    let workspace_roots = context
        .workspace_roots
        .iter()
        .map(|root| root.to_abs_path())
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(|_| Error::Invalid)?;
    if let Some(level) = level {
        if params.enforce_managed_network && level != WindowsSandboxLevel::Elevated {
            return Err(Error::Denied);
        }
        let permissions = context
            .permissions
            .clone()
            .materialize_project_roots_with_workspace_roots(&workspace_roots);
        if codex_sandboxing::unsupported_windows_restricted_token_sandbox_reason(
            expected,
            &permissions,
            &policy_cwd,
            level,
        )
        .is_some()
        {
            return Err(Error::Denied);
        }
    } else {
        if params.arg0.is_some() {
            return Err(Error::Denied);
        }
        // A configured proxy replaces params.managed_network with its prepared
        // context upstream. Do not reject a superseded or unused context.
        if params.enforce_managed_network && params.network_proxy.is_none() {
            let network = params.managed_network.as_ref().ok_or(Error::Denied)?;
            if network.loopback_ports.is_empty()
                || network.loopback_ports.contains(&0)
                || !network.allow_local_binding
            {
                return Err(Error::Denied);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod production_preflight_tests {
    use super::*;
    use codex_exec_server::{FileSystemSandboxContext, ProcessId};
    use codex_protocol::{
        models::{ManagedFileSystemPermissions, PermissionProfile},
        protocol::{NetworkSandboxPolicy, SandboxPolicy},
    };
    use codex_utils_path_uri::PathUri;
    use std::collections::HashMap;

    fn native_cwd() -> PathUri {
        #[cfg(windows)]
        let path = "C:\\morrow-preflight-synthetic";
        #[cfg(not(windows))]
        let path = "/morrow-preflight-synthetic";
        PathUri::from_host_native_path(path).unwrap()
    }
    fn foreign_cwd() -> PathUri {
        #[cfg(windows)]
        let uri = "file:///morrow-preflight-synthetic";
        #[cfg(not(windows))]
        let uri = "file:///C:/morrow-preflight-synthetic";
        PathUri::parse(uri).unwrap()
    }
    fn params(selection: WindowsSandboxSelection) -> ExecParams {
        let mut sandbox = FileSystemSandboxContext::from_permission_profile(
            PermissionProfile::Managed {
                file_system: ManagedFileSystemPermissions::Unrestricted,
                network: NetworkSandboxPolicy::Enabled,
            },
            native_cwd(),
        );
        sandbox.windows_sandbox_selection = selection;
        ExecParams {
            process_id: ProcessId::new("pure-preflight"),
            metadata: None,
            argv: vec!["synthetic.exe".into()],
            cwd: native_cwd(),
            env_policy: None,
            shell_snapshot: None,
            env: HashMap::new(),
            tty: false,
            pipe_stdin: false,
            arg0: None,
            sandbox: Some(sandbox),
            enforce_managed_network: false,
            managed_network: None,
            network_proxy: None,
        }
    }

    #[test]
    fn missing_or_disabled_sandbox_is_denied() {
        let mut p = params(WindowsSandboxSelection::Disabled);
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Denied)
        );
        p.sandbox = None;
        assert_eq!(
            validate_production_params(SandboxType::WindowsRestrictedToken, &p),
            Err(Error::Denied)
        );
    }
    #[test]
    fn expected_backend_family_must_match_selection() {
        let p = params(WindowsSandboxSelection::Mxc);
        assert_eq!(
            validate_production_params(SandboxType::WindowsRestrictedToken, &p),
            Err(Error::Denied)
        );
        for selection in [
            WindowsSandboxSelection::RestrictedToken,
            WindowsSandboxSelection::Elevated,
        ] {
            assert_eq!(
                validate_production_params(SandboxType::WindowsMxc, &params(selection)),
                Err(Error::Denied)
            );
        }
        assert_eq!(
            validate_production_params(SandboxType::None, &p),
            Err(Error::Denied)
        );
    }
    #[test]
    fn token_backends_reject_upstream_unsupported_full_access_profiles() {
        for selection in [
            WindowsSandboxSelection::RestrictedToken,
            WindowsSandboxSelection::Elevated,
        ] {
            let mut p = params(selection);
            let full = p.sandbox.as_ref().unwrap().permissions.clone();
            for profile in [
                full,
                PermissionProfile::Disabled,
                PermissionProfile::External {
                    network: NetworkSandboxPolicy::Enabled,
                },
            ] {
                p.sandbox.as_mut().unwrap().permissions = profile;
                assert_eq!(
                    validate_production_params(SandboxType::WindowsRestrictedToken, &p),
                    Err(Error::Denied)
                );
            }
        }
    }
    #[test]
    fn mxc_keeps_supported_full_access_profiles() {
        let mut p = params(WindowsSandboxSelection::Mxc);
        let full = p.sandbox.as_ref().unwrap().permissions.clone();
        for profile in [
            full,
            PermissionProfile::Disabled,
            PermissionProfile::External {
                network: NetworkSandboxPolicy::Enabled,
            },
        ] {
            p.sandbox.as_mut().unwrap().permissions = profile;
            assert_eq!(
                validate_production_params(SandboxType::WindowsMxc, &p),
                Ok(())
            );
        }
    }
    #[test]
    fn foreign_command_policy_and_workspace_paths_are_invalid() {
        let original = params(WindowsSandboxSelection::Mxc);
        let mut p = original.clone();
        p.cwd = foreign_cwd();
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Invalid)
        );
        p = original.clone();
        p.sandbox.as_mut().unwrap().cwd = foreign_cwd();
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Invalid)
        );
        p = original;
        p.sandbox.as_mut().unwrap().workspace_roots = vec![foreign_cwd()];
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn empty_argv_is_invalid() {
        let mut p = params(WindowsSandboxSelection::Mxc);
        p.argv.clear();
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Invalid)
        );
    }
    #[test]
    fn mxc_custom_arg0_is_denied() {
        let mut p = params(WindowsSandboxSelection::Mxc);
        p.arg0 = Some("custom".into());
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Denied)
        );
    }
    #[test]
    fn unelevated_token_cannot_enforce_managed_network() {
        let mut p = params(WindowsSandboxSelection::RestrictedToken);
        let mut context = FileSystemSandboxContext::from_legacy_sandbox_policy(
            SandboxPolicy::ReadOnly {
                network_access: false,
            },
            native_cwd(),
        )
        .unwrap();
        context.windows_sandbox_selection = WindowsSandboxSelection::RestrictedToken;
        p.sandbox = Some(context);
        assert_eq!(
            validate_production_params(SandboxType::WindowsRestrictedToken, &p),
            Ok(())
        );
        p.enforce_managed_network = true;
        assert_eq!(
            validate_production_params(SandboxType::WindowsRestrictedToken, &p),
            Err(Error::Denied)
        );
    }
    #[test]
    fn mxc_requires_valid_dedicated_managed_network_context() {
        let mut p = params(WindowsSandboxSelection::Mxc);
        p.enforce_managed_network = true;
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Err(Error::Denied)
        );
        for value in [
            serde_json::json!({"loopbackPorts":[],"allowLocalBinding":true}),
            serde_json::json!({"loopbackPorts":[0],"allowLocalBinding":true}),
            serde_json::json!({"loopbackPorts":[1234],"allowLocalBinding":false}),
        ] {
            p.managed_network = Some(serde_json::from_value(value).unwrap());
            assert_eq!(
                validate_production_params(SandboxType::WindowsMxc, &p),
                Err(Error::Denied)
            );
        }
        p.managed_network = Some(
            serde_json::from_value(
                serde_json::json!({"loopbackPorts":[1234],"allowLocalBinding":true}),
            )
            .unwrap(),
        );
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Ok(())
        );
    }
    #[test]
    fn mxc_does_not_validate_unused_managed_network_context() {
        let mut p = params(WindowsSandboxSelection::Mxc);
        p.managed_network = Some(
            serde_json::from_value(
                serde_json::json!({"loopbackPorts":[0],"allowLocalBinding":false}),
            )
            .unwrap(),
        );
        assert_eq!(
            validate_production_params(SandboxType::WindowsMxc, &p),
            Ok(())
        );
    }
}
