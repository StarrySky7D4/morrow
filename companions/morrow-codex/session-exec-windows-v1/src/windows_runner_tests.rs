//! Metadata/selection tests plus locked-current-PE constructor regressions.
//! Constructor tests provision real local environments/schedulers, but never
//! submit a business Start, execute a helper, or create sandbox accounts.
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
fn native_close_metadata_never_accepts_a_legacy_schema_or_upgrades_the_default() {
    let original=identity();
    assert_eq!(original.protocol_version,7);
    assert_eq!(original.validate(),Ok(()));
    let mut native=original.clone();native.protocol_version=8;
    assert_eq!(native.validate(),Err(Error::Denied));
    native.control_schema_sha256=NATIVE_CLOSE_SCHEMA;
    assert_eq!(native.validate(),Ok(()));
    let mut missing_ack=native.clone();
    missing_ack.control_schema_sha256=*b"morrow.c28.native-close.schema.\x08";
    assert_eq!(missing_ack.validate(),Err(Error::Denied));
    let mut mixed=native.clone();mixed.protocol_version=7;
    assert_eq!(mixed.validate(),Err(Error::Denied));
    for version in [0,6,9,u32::MAX] {
        native.protocol_version=version;
        assert_eq!(native.validate(),Err(Error::Denied));
    }
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

// This ordinary current test PE supplies a genuine file/ancestor lock and full
// SHA check. It is not a matched business runner or a handshake/native witness.
fn locked_constructor_test_image() -> (ExecServerRuntimeOptions, [u8; 32], Arc<MatchedRunnerArtifact>) {
    let path = std::env::current_exe().expect("current test PE path");
    let sha = Sha256::digest(std::fs::read(&path).expect("read public current test PE")).into();
    let pin = Arc::new(MatchedRunnerArtifact::acquire(&path, sha).expect("lock actual test PE and ancestors"));
    let options = ExecServerRuntimeOptions::new(pin.path().to_path_buf(), None).unwrap();
    (options, sha, pin)
}

fn constructor_http_factory() -> HttpClientFactory {
    // Construct only. No HTTP request is issued by these regressions.
    HttpClientFactory::new(codex_http_client::OutboundProxyPolicy::ReqwestDefault)
}

#[test]
fn explicit_v8_production_factory_constructs_same_pin_environment_and_retires_original_scheduler() {
    let (options, sha, original) = locked_constructor_test_image();
    let native_pin = original.native_close_view();
    let spec = WindowsRunnerProvisioningSpec::from_locked_native_close_v8(native_pin.clone()).unwrap();
    let native_refs = Arc::strong_count(&native_pin);
    let mut retirement = crate::NativeFactoryRetirement::new();
    // This is the same public production factory called by the harness, through
    // the SDK create path and the actual derived Environment constructor.
    let (backend, runtime) = ProvisionedWindowsBackend::production_with_native_close_runner(
        options, constructor_http_factory(), sha, spec, &mut retirement,
    ).expect("explicit v8 must reach the actual environment instead of the v7-only gate");
    assert!(backend.is_production());
    assert_eq!(backend.expected, SandboxType::WindowsRestrictedToken);
    let retained = backend._runner.as_ref().expect("explicit runner cannot become None");
    assert_eq!(retained.artifact().protocol_version(), 8);
    assert_eq!(retained.artifact().control_schema_sha256(), NATIVE_CLOSE_SCHEMA);
    assert!(Arc::ptr_eq(&retained.artifact(), &native_pin));
    assert!(retained.artifact().shares_original_pin(&original));
    // Moving the spec into the SDK does not add a reference. This extra
    // original Arc is held by the actual Environment/LocalProcess route.
    assert_eq!(Arc::strong_count(&native_pin), native_refs + 1);
    assert!(Arc::ptr_eq(&backend.runtime, &runtime));
    assert!(matches!(retirement.finish_after_shutdown(), Err(Error::Conflict)));
    // No runner/process Start was submitted. Drop the real backend, then
    // synchronously shut down its unique original runtime before native checks.
    drop(backend);
    assert_eq!(Arc::strong_count(&native_pin), 1);
    let runtime = Arc::try_unwrap(runtime).ok().expect("original runtime must be unique");
    drop(runtime);
    let completion = retirement.finish_after_shutdown()
        .expect("original scheduler native waits and checked closes must complete");
    assert!(matches!(retirement.finish_after_shutdown(), Err(Error::CommitUnknown)));
    drop(completion);
}

#[test]
fn actual_environment_and_sdk_entries_keep_v7_and_v8_separate_without_none_fallback() {
    let (options, sha, original) = locked_constructor_test_image();
    let native_pin = original.native_close_view();
    let checked = WindowsRunnerProvisioningSpec::from_locked(original.clone()).unwrap();
    let native = WindowsRunnerProvisioningSpec::from_locked_native_close_v8(native_pin.clone()).unwrap();
    let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread().worker_threads(2)
        .enable_all().build().unwrap());
    {
        let _entered = runtime.enter();
        let original_refs = Arc::strong_count(&original);
        let native_refs = Arc::strong_count(&native_pin);
        let v7_environment = codex_exec_server::Environment::create_with_windows_runner(
            options.clone(), constructor_http_factory(), original.clone()).unwrap();
        let v8_environment = codex_exec_server::Environment::create_with_native_close_windows_runner(
            options.clone(), constructor_http_factory(), native_pin.clone()).unwrap();
        // Each actual LocalProcess owns the original Arc, not a path reacquire.
        assert_eq!(Arc::strong_count(&original), original_refs + 1);
        assert_eq!(Arc::strong_count(&native_pin), native_refs + 1);
        assert!(matches!(codex_exec_server::Environment::create_with_windows_runner(
            options.clone(), constructor_http_factory(), native_pin.clone()),
            Err(codex_exec_server::ExecServerError::Protocol(_))));
        assert!(matches!(codex_exec_server::Environment::create_with_native_close_windows_runner(
            options.clone(), constructor_http_factory(), original.clone()),
            Err(codex_exec_server::ExecServerError::Protocol(_))));
        drop(v7_environment);
        drop(v8_environment);
        assert_eq!(Arc::strong_count(&original), original_refs);
        assert_eq!(Arc::strong_count(&native_pin), native_refs);
    }
    let v7_backend = ProvisionedWindowsBackend::production_with_checked_runner(
        options.clone(), constructor_http_factory(), sha, runtime.clone(), checked.clone()).unwrap();
    assert!(v7_backend.is_production());
    assert_eq!(v7_backend._runner.as_ref().unwrap().artifact().protocol_version(), 7);
    assert!(Arc::ptr_eq(&v7_backend._runner.as_ref().unwrap().artifact(), &original));
    assert!(matches!(ProvisionedWindowsBackend::production_with_checked_runner(
        options.clone(), constructor_http_factory(), sha, runtime.clone(), native.clone()), Err(Error::Denied)));
    assert!(matches!(ProvisionedWindowsBackend::create(
        options.clone(), constructor_http_factory(), sha, runtime.clone(), SandboxType::WindowsRestrictedToken,
        true, Some(native), |backend| backend), Err(Error::Denied)));
    let mut retirement = crate::NativeFactoryRetirement::new();
    assert!(matches!(ProvisionedWindowsBackend::production_with_native_close_runner(
        options.clone(), constructor_http_factory(), sha, checked.clone(), &mut retirement), Err(Error::Denied)));
    assert!(matches!(retirement.finish_after_shutdown(), Err(Error::CommitUnknown)));
    // Rejected selection is not resettable/replayable, even before runtime creation.
    assert!(matches!(ProvisionedWindowsBackend::production_with_native_close_runner(
        options.clone(), constructor_http_factory(), sha, checked, &mut retirement), Err(Error::CommitUnknown)));
    assert!(matches!(ProvisionedWindowsBackend::create_in_runner_environment(
        options, constructor_http_factory(), sha, runtime.clone(), SandboxType::WindowsRestrictedToken,
        true, None, crate::provisioned::RunnerEnvironmentKind::NativeCloseV8, |backend| backend),
        Err(Error::Denied)));
    drop(v7_backend);
    drop(Arc::try_unwrap(runtime).ok().expect("ordinary original test runtime must be unique"));
}

#[test]
fn explicit_v8_factory_rejects_wrong_version_schema_or_identity_against_actual_locked_pin() {
    let (options, sha, original) = locked_constructor_test_image();
    let spec = WindowsRunnerProvisioningSpec::from_locked_native_close_v8(original.native_close_view()).unwrap();
    let mut invalid = Vec::new();
    for (version, schema) in [(7, CHECKED_SCHEMA_SHA256), (8, CHECKED_SCHEMA_SHA256),
        (8, [0; 32]), (9, NATIVE_CLOSE_SCHEMA)] {
        let mut changed = spec.clone();
        changed.identity.protocol_version = version;
        changed.identity.control_schema_sha256 = schema;
        invalid.push(changed);
    }
    let mut wrong_hash = spec.clone();
    wrong_hash.identity.full_sha256[0] ^= 1;
    invalid.push(wrong_hash);
    let mut wrong_path = spec;
    wrong_path.identity.canonical_path.push_str(".other");
    invalid.push(wrong_path);
    for changed in invalid {
        let mut retirement = crate::NativeFactoryRetirement::new();
        // Call the production factory; rejection precedes original scheduler
        // creation. No fabricated MatchedRunnerArtifact or OS identity is used.
        assert!(matches!(ProvisionedWindowsBackend::production_with_native_close_runner(
            options.clone(), constructor_http_factory(), sha, changed, &mut retirement), Err(Error::Denied)));
        assert!(matches!(retirement.finish_after_shutdown(), Err(Error::CommitUnknown)));
    }
}
