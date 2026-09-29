//! One executable, shared lifetime and runtime. This is not the production agent loop.
use crate::{exec, network, store};
use codex_core::morrow_network_qualification::{self as net, Scenario};
use codex_core::morrow_p02_qualification as process;
use codex_protocol::{
    ThreadId,
    models::BaseInstructions,
    protocol::{SessionSource, ThreadHistoryMode, ThreadMemoryMode},
};
use codex_thread_store::*;
use serde_json::{Value, json};
use std::sync::Arc;

pub(crate) async fn coexist() -> Value {
    let (exec_backend, denied, environment) = exec::context();
    let network_backend = network::context();
    let thread_store = store::context();
    let weak_exec = Arc::downgrade(&exec_backend);
    let weak_network = Arc::downgrade(&network_backend);
    let weak_store = Arc::downgrade(&thread_store);
    let weak_denied = Arc::downgrade(&denied);
    let live = store::resume(thread_store.clone())
        .await
        .expect("bounded empty history");
    // All three capabilities and real LiveThread are alive during both Core calls.
    let (exec_result, network_result) = tokio::join!(
        process::dispatch_refusal_probe(&environment, false),
        net::run(network_backend.clone(), Scenario::Http)
    );
    let exec_error = exec_result.expect_err("exec refusal");
    assert!(exec_error.contains("qualification-disconnected"));
    assert!(
        network_result["outcomes"][0]
            .as_str()
            .unwrap()
            .contains("qualification-disconnected-http")
    );
    let persist_error = live
        .persist(PersistContext::Standard)
        .await
        .expect_err("no durable success")
        .to_string();
    assert!(persist_error.contains("persist_thread_requires_M04"));
    let mut guard = LiveThreadInitGuard::new(Some(live));
    guard.discard().await;
    assert!(guard.as_ref().is_none());
    assert_eq!(
        store::calls(&thread_store),
        [
            "resume_thread",
            "load_history",
            "persist_context:Standard",
            "persist_thread_requires_M04",
            "discard_thread"
        ]
    );
    let exec_calls = exec::calls(&exec_backend);
    let network_calls = network::calls(&network_backend);
    assert_eq!(exec_calls.len(), 1);
    assert_eq!(network_calls.len(), 1);
    assert!(denied.0.lock().unwrap().is_empty());
    drop((
        guard,
        environment,
        exec_backend,
        network_backend,
        thread_store,
        denied,
    ));
    let owners = [
        weak_exec.strong_count(),
        weak_network.strong_count(),
        weak_store.strong_count(),
        weak_denied.strong_count(),
    ];
    assert_eq!(owners, [0, 0, 0, 0]);
    json!({"case":"three_seams_shared_lifetime", "assertions":9, "exec_error":exec_error,"network_result":network_result,"persist_error":persist_error,"exec_calls":exec_calls,"network_calls":network_calls,"remaining_strong_owners":owners,"cleanup":"LiveThreadInitGuard.discard awaited once; adapter returned Unsupported; owner release only, writer release NOT established"})
}

fn metadata() -> ThreadPersistenceMetadata {
    ThreadPersistenceMetadata {
        cwd: None,
        model_provider: "qualification-only".into(),
        memory_mode: ThreadMemoryMode::Disabled,
    }
}
fn thread_id() -> ThreadId {
    ThreadId::from_string("00000000-0000-4000-8000-000000000004").unwrap()
}

pub(crate) async fn guards() -> Vec<Value> {
    let mut cases = vec![];
    // Spawn a new task: no inherited task-local policy or injected object is required.
    for tty in [false, true] {
        let result = tokio::spawn(process::default_refusal_probe(tty))
            .await
            .unwrap();
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("restricted-qualification: injected exec required")
        );
        assert_eq!(result["spawn_lifecycle_calls"], json!([]));
        cases
            .push(json!({"case":"default_exec_new_task","tty":tty,"result":result,"assertions":2}));
    }
    let result = tokio::spawn(process::independent_refusal_probe())
        .await
        .unwrap();
    assert!(
        result["error"]
            .as_str()
            .unwrap()
            .contains("restricted-qualification: independent exec disabled")
    );
    assert_eq!(result["after_spawn_calls"], 0);
    cases.push(json!({"case":"independent_exec_new_task","result":result,"assertions":2}));
    for (scenario, expected) in [
        (Scenario::Http, "injected HTTP required"),
        (Scenario::Websocket, "injected WebSocket required"),
    ] {
        let result = tokio::spawn(net::run_without_backend(scenario))
            .await
            .unwrap();
        assert!(
            result["outcomes"][0].as_str().unwrap().contains(expected),
            "{result}"
        );
        cases.push(json!({"case":"missing_network_new_task","expected_guard":expected,"result":result,"assertions":1}));
    }
    let home = std::path::PathBuf::from(std::env::var_os("TEMP").unwrap())
        .join("restricted-never-created-store");
    assert!(!home.exists());
    let local = Arc::new(LocalThreadStore::new(
        LocalThreadStoreConfig {
            codex_home: home.clone(),
            sqlite: codex_state::SqliteConfig::from_sqlite_home(
                codex_utils_absolute_path::AbsolutePathBuf::try_from(home.clone()).unwrap(),
            ),
            default_model_provider_id: "qualification-only".into(),
        },
        None,
    ));
    for mode in [ThreadHistoryMode::Legacy, ThreadHistoryMode::Paginated] {
        let local = local.clone();
        let error = tokio::spawn(async move {
            LiveThread::resume(
                local,
                mode,
                ResumeThreadParams {
                    thread_id: thread_id(),
                    rollout_path: None,
                    history: None,
                    include_archived: false,
                    metadata: metadata(),
                },
            )
            .await
            .err()
            .expect("guard refusal")
        })
        .await
        .unwrap()
        .to_string();
        assert!(error.contains("restricted_qualification_local_resume_before_state_db"));
        cases.push(json!({"case":"local_store_resume_new_task","mode":format!("{mode:?}"),"error":error,"assertions":1}));
    }
    let fixture = store::context();
    let params = CreateThreadParams {
        creator_user_id: None,
        creator_account_id: None,
        session_id: thread_id().into(),
        thread_id: thread_id(),
        extra_config: None,
        forked_from_id: None,
        parent_thread_id: None,
        source: SessionSource::Exec,
        thread_source: None,
        originator: "qualification".into(),
        base_instructions: BaseInstructions::default(),
        dynamic_tools: vec![],
        selected_capability_roots: vec![],
        multi_agent_version: None,
        history_mode: ThreadHistoryMode::Legacy,
        history_base: None,
        subagent_history_start_ordinal: None,
        initial_window_id: "fixture-window".into(),
        runtime_workspace_roots: None,
        metadata: ThreadPersistenceMetadata {
            cwd: Some(home.clone()),
            ..metadata()
        },
    };
    let held = fixture.clone();
    let error = tokio::spawn(async move {
        LiveThread::create(held, params)
            .await
            .err()
            .expect("creation refused before Git")
    })
    .await
    .unwrap()
    .to_string();
    assert!(error.contains("restricted_qualification_create_before_git"));
    assert!(store::calls(&fixture).is_empty());
    assert!(!home.exists());
    cases.push(json!({"case":"create_before_git_new_task","error":error,"adapter_calls":store::calls(&fixture),"fixture_home_created":false,"assertions":4,"limits":"guard order verified in source; no OS-wide filesystem/process tracing"}));
    cases
}
