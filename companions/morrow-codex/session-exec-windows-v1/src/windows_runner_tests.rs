//! Pure metadata/selection tests: no file, helper, DPAPI or sandbox is accessed.
use std::collections::HashMap;

use codex_exec_server::{FileSystemSandboxContext, ProcessId};
use codex_protocol::protocol::SandboxPolicy;
use codex_utils_path_uri::PathUri;
use sha2::{Digest, Sha256};

use super::*;

fn identity() -> RunnerIdentity {
    RunnerIdentity {
        canonical_path: "C:\\synthetic\\runner.exe".into(),
        full_sha256: [1; 32],
        protocol_version: CHECKED_PROTOCOL_VERSION,
        control_schema_sha256: CHECKED_SCHEMA_SHA256,
    }
}

fn params(selection: WindowsSandboxSelection) -> ExecParams {
    let cwd = PathUri::from_host_native_path("C:\\synthetic").unwrap();
    let mut context = FileSystemSandboxContext::from_legacy_sandbox_policy(
        SandboxPolicy::ReadOnly {
            network_access: false,
        },
        cwd.clone(),
    )
    .unwrap();
    context.windows_sandbox_selection = selection;
    ExecParams {
        process_id: ProcessId::new("pure-checked-selection"),
        metadata: None,
        argv: vec!["synthetic.exe".into()],
        cwd,
        env_policy: None,
        shell_snapshot: None,
        env: HashMap::new(),
        tty: false,
        pipe_stdin: true,
        arg0: None,
        sandbox: Some(context),
        enforce_managed_network: false,
        managed_network: None,
        network_proxy: None,
    }
}

#[test]
fn mismatched_version_schema_and_missing_identity_are_denied() {
    let good = identity();
    assert_eq!(good.validate(), Ok(()));
    let mut version = good.clone();
    version.protocol_version = 6;
    assert_eq!(version.validate(), Err(Error::Denied));
    let mut schema = good.clone();
    schema.control_schema_sha256[31] ^= 1;
    assert_eq!(schema.validate(), Err(Error::Denied));
    let mut path = good;
    path.canonical_path.clear();
    assert_eq!(path.validate(), Err(Error::Denied));
}

#[test]
fn identity_encoding_binds_full_path_hash_version_and_schema_independently() {
    let good = identity();
    let digest = |value: &RunnerIdentity| -> [u8; 32] {
        Sha256::digest(serde_json::to_vec(&value.json()).unwrap()).into()
    };
    let baseline = digest(&good);
    let mut path = good.clone();
    path.canonical_path = "C:\\other\\runner.exe".into();
    let mut sha = good.clone();
    sha.full_sha256[0] ^= 1;
    let mut version = good.clone();
    version.protocol_version += 1;
    let mut schema = good.clone();
    schema.control_schema_sha256[0] ^= 1;
    for changed in [path, sha, version, schema] {
        assert_ne!(digest(&changed), baseline);
    }
    assert_eq!(
        good.json(),
        serde_json::json!({
            "canonical_path":"C:\\synthetic\\runner.exe", "full_sha256":good.full_sha256,
            "protocol_version":7,"control_schema_sha256":CHECKED_SCHEMA_SHA256,
        })
    );
}

#[test]
fn checked_route_rejects_none_ordinary_mxc_and_unelevated_before_effects() {
    for selection in [
        WindowsSandboxSelection::Disabled,
        WindowsSandboxSelection::RestrictedToken,
        WindowsSandboxSelection::Mxc,
    ] {
        assert_eq!(
            validate_checked_selection(SandboxType::WindowsRestrictedToken, &params(selection)),
            Err(Error::Denied)
        );
    }
    let mut missing = params(WindowsSandboxSelection::Elevated);
    missing.sandbox = None;
    assert_eq!(
        validate_checked_selection(SandboxType::WindowsRestrictedToken, &missing),
        Err(Error::Denied)
    );
    for expected in [SandboxType::None, SandboxType::WindowsMxc] {
        assert_eq!(
            validate_checked_selection(expected, &params(WindowsSandboxSelection::Elevated)),
            Err(Error::Denied)
        );
    }
}

#[test]
fn checked_selection_still_requires_original_production_policy_preflight() {
    let mut p = params(WindowsSandboxSelection::Elevated);
    assert_eq!(
        validate_checked_selection(SandboxType::WindowsRestrictedToken, &p),
        Ok(())
    );
    assert_eq!(
        crate::provisioned::validate_production_params(SandboxType::WindowsRestrictedToken, &p),
        Ok(())
    );
    p.sandbox.as_mut().unwrap().permissions = codex_protocol::models::PermissionProfile::Disabled;
    // Selection alone is not authority or proof that upstream will sandbox.
    assert_eq!(
        validate_checked_selection(SandboxType::WindowsRestrictedToken, &p),
        Ok(())
    );
    assert_eq!(
        crate::provisioned::validate_production_params(SandboxType::WindowsRestrictedToken, &p),
        Err(Error::Denied)
    );
}
