//! Bounded bridge into the original unified-exec dispatcher; no synthetic loop.
use crate::exec::{ExecCapturePolicy, ExecExpiration};
use crate::sandboxing::ExecRequest;
use crate::unified_exec::{NoopSpawnLifecycle, UnifiedExecProcessManager};
use codex_protocol::config_types::{WindowsSandboxLevel, WindowsSandboxProxySettingsMode};
use codex_protocol::models::PermissionProfile;
use codex_sandboxing::SandboxType;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;

/// Exercises one actual prepared-request dispatch. Callers must supply a refusing backend.
/// A returned error proves only this call's propagation, never global spawn isolation.
pub async fn dispatch_refusal_probe(
    environment: &codex_exec_server::Environment,
    tty: bool,
) -> Result<(), String> {
    let request = request();
    UnifiedExecProcessManager::default()
        .open_session_with_prepared_exec_env(
            /*process_id*/ 2,
            &request,
            /*tool_ctx*/ None,
            WindowsSandboxProxySettingsMode::Preserve,
            /*network_policy_decider*/ None,
            tty,
            Box::new(NoopSpawnLifecycle),
            environment,
        )
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn request() -> ExecRequest {
    ExecRequest::new(
        vec![
            "morrow-qualification-never-launch".into(),
            "fixture-argument".into(),
        ],
        AbsolutePathBuf::try_from(r"C:\morrow-qualification-nonexistent")
            .expect("fixed absolute fixture path"),
        HashMap::new(),
        /*network*/ None,
        /*network_environment_id*/ None,
        ExecExpiration::DefaultTimeout,
        ExecCapturePolicy::ShellTool,
        SandboxType::None,
        vec![],
        WindowsSandboxLevel::Disabled,
        PermissionProfile::read_only(),
        /*arg0*/ None,
    )
}

/// Enter the independent Core path; never allow a successful spawn in this build.
pub async fn independent_refusal_probe() -> serde_json::Value {
    let spawned = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = spawned.clone();
    let error = crate::exec::execute_exec_request(
        request(),
        None,
        Some(Box::new(move || {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })),
    )
    .await
    .expect_err("independent execution must refuse");
    serde_json::json!({"error":error.to_string(),"after_spawn_calls":spawned.load(std::sync::atomic::Ordering::SeqCst)})
}

/// Missing injection must refuse before even asking the spawn lifecycle for FDs.
pub async fn default_refusal_probe(tty: bool) -> serde_json::Value {
    use std::sync::{Arc, Mutex};
    #[derive(Debug)]
    struct ObservedLifecycle(Arc<Mutex<Vec<&'static str>>>);
    impl crate::unified_exec::SpawnLifecycle for ObservedLifecycle {
        fn inherited_fds(&self) -> Vec<i32> {
            self.0.lock().unwrap().push("inherited_fds");
            vec![]
        }
        fn after_spawn(&mut self) {
            self.0.lock().unwrap().push("after_spawn");
        }
    }
    let calls = Arc::new(Mutex::new(vec![]));
    let environment = codex_exec_server::Environment::default_for_tests();
    let error = UnifiedExecProcessManager::default()
        .open_session_with_prepared_exec_env(
            2,
            &request(),
            None,
            WindowsSandboxProxySettingsMode::Preserve,
            None,
            tty,
            Box::new(ObservedLifecycle(calls.clone())),
            &environment,
        )
        .await
        .err()
        .expect("default execution must refuse");
    serde_json::json!({"error":error.to_string(),"spawn_lifecycle_calls":*calls.lock().unwrap()})
}
