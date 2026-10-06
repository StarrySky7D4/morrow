use morrow_agent_session_exec_v1_r2::{
    Action, Error, Outcome, Reply, Request,
    authority::{Capabilities, SessionExecHost},
};
use morrow_agent_session_exec_v1_r2_host::{
    AgentPackage, Declaration, PreparedAgentPackage, native,
};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::Package,
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{Cancellation, Limits, Runner};
fn caps(write: bool) -> Capabilities {
    Capabilities {
        session_read: true,
        session_write: write,
        ..Capabilities::default()
    }
}
fn module(extra: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
 (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
 (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
 (import "morrow_agent_session_exec_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
 {extra}
 (memory (export "memory") 6)
 (func (export "morrow_run") (result i32) (local $n i32)
  (local.set $n (call $read (i32.const 0) (i32.const 131072)))
  (local.set $n (call $call (i32.const 0) (local.get $n) (i32.const 131072) (i32.const 131072)))
  (drop (call $complete (i32.const 131072) (local.get $n))) (i32.const 0)))"#
    ))
    .unwrap()
}
fn package() -> AgentPackage {
    let module = module("");
    let manifest = Package::manifest_for_task("agent-r2-test", "1.0.0", &module, vec![]);
    AgentPackage::build(
        Package::build(manifest, &module).unwrap(),
        Declaration {
            capabilities: caps(true),
            sessions: vec!["session".into()],
            execution_domain: "fixed-domain".into(),
        },
    )
    .unwrap()
}
fn limits() -> Limits {
    Limits {
        fuel: 1_000_000,
        memory_bytes: 1_048_576,
        host_calls: 16,
    }
}
#[test]
fn actual_reviewed_package_wasm_and_native_use_original_host() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostRuntime::new(
        Store::open(&temp.path().join("core.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let prepared = PreparedAgentPackage::new(package(), limits()).unwrap();
    let approved = prepared
        .approve(
            &mut runtime,
            &host,
            prepared.package().review_sha256(),
            caps(true),
            vec!["session".into()],
            1000,
            0,
        )
        .unwrap();
    let create = Request::new(
        "create",
        Action::Create {
            session_id: "session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let result = prepared
        .run(
            &mut runtime,
            &host,
            &approved,
            create.raw(),
            || 1,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(result.report.outcome, Ok(0));
    let bytes = result.completion.unwrap();
    assert!(matches!(
        Reply::decode_for(&create, &bytes).unwrap().outcome,
        Outcome::Session(_)
    ));
    let snapshot = Request::new(
        "snapshot",
        Action::Snapshot {
            session_id: "session".into(),
            after: 0,
            limit: 1,
        },
    )
    .unwrap();
    let mut frame = Vec::new();
    frame.extend_from_slice(&(snapshot.raw().len() as u32).to_le_bytes());
    frame.extend_from_slice(snapshot.raw());
    let response = native::exchange_frame(&mut runtime, &host, &approved, &frame, || 2).unwrap();
    assert_eq!(
        u32::from_le_bytes(response[..4].try_into().unwrap()) as usize,
        response.len() - 4
    );
    assert!(matches!(
        Reply::decode_for(&snapshot, &response[4..])
            .unwrap()
            .outcome,
        Outcome::Snapshot(_)
    ));
    host.revoke(approved.admission()).unwrap();
    assert!(matches!(
        Reply::decode_for(
            &snapshot,
            &native::exchange(&mut runtime, &host, &approved, snapshot.raw(), || 3).unwrap()
        )
        .unwrap()
        .outcome,
        Outcome::Rejected(Error::Denied)
    ));
}
#[test]
fn declaration_cannot_self_approve_or_expand_scope() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostRuntime::new(
        Store::open(&temp.path().join("core.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let prepared = PreparedAgentPackage::new(package(), limits()).unwrap();
    assert!(matches!(
        prepared.approve(
            &mut runtime,
            &host,
            [9; 32],
            caps(true),
            vec!["session".into()],
            1000,
            0
        ),
        Err(Error::Denied)
    ));
    assert!(matches!(
        prepared.approve(
            &mut runtime,
            &host,
            prepared.package().review_sha256(),
            caps(true),
            vec!["foreign".into()],
            1000,
            0
        ),
        Err(Error::Denied)
    ));
    let elevated = Capabilities {
        execute: true,
        ..caps(true)
    };
    assert!(matches!(
        prepared.approve(
            &mut runtime,
            &host,
            prepared.package().review_sha256(),
            elevated,
            vec!["session".into()],
            1000,
            0
        ),
        Err(Error::Denied)
    ));
    let approved = prepared
        .approve(
            &mut runtime,
            &host,
            prepared.package().review_sha256(),
            caps(false),
            vec!["session".into()],
            1000,
            0,
        )
        .unwrap();
    let create = Request::new(
        "create",
        Action::Create {
            session_id: "session".into(),
            parent: None,
            parent_tail: 0,
        },
    )
    .unwrap();
    let result = prepared
        .run(
            &mut runtime,
            &host,
            &approved,
            create.raw(),
            || 1,
            Cancellation::default(),
        )
        .unwrap();
    assert!(matches!(
        Reply::decode_for(&create, &result.completion.unwrap())
            .unwrap()
            .outcome,
        Outcome::Rejected(Error::Denied)
    ));
}
#[test]
fn legacy_factories_and_mixed_imports_refuse_r2() {
    assert!(Runner::new_task(&module(""), limits()).is_err());
    assert!(Runner::new_io_task(&module(""), limits()).is_err());
    let mixed =
        module(r#"(import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#);
    assert!(Runner::new_agent_session_exec_task(&mixed, limits()).is_err());
    assert!(
        PreparedAgentPackage::new(
            package(),
            Limits {
                fuel: 0,
                ..limits()
            }
        )
        .is_err()
    );
}
#[test]
fn package_and_native_frame_tampering_are_rejected() {
    let package = package();
    let mut raw = package.archive().to_vec();
    *raw.last_mut().unwrap() ^= 1;
    assert!(AgentPackage::decode(&raw).is_err());
    let mut trailing = package.archive().to_vec();
    trailing.push(0);
    assert!(AgentPackage::decode(&trailing).is_err());
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = HostRuntime::new(
        Store::open(&temp.path().join("core.sqlite"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let host = SessionExecHost::new(&mut runtime).unwrap();
    let prepared = PreparedAgentPackage::new(package, limits()).unwrap();
    let approved = prepared
        .approve(
            &mut runtime,
            &host,
            prepared.package().review_sha256(),
            caps(false),
            vec!["session".into()],
            1000,
            0,
        )
        .unwrap();
    assert!(matches!(
        native::exchange_frame(&mut runtime, &host, &approved, &[0, 0, 0, 0], || 1),
        Err(Error::Invalid)
    ));
}
