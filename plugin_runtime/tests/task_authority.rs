#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, proto::Capability},
    response::{Failure, Outcome, Response},
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
    task::Invocation,
    transaction::Lookup,
};
use morrow_plugin_runtime::{Cancellation, Fault, Limits, package::PreparedPackage};
fn data(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect()
}
fn setup(path: &std::path::Path) -> HostRuntime {
    let mut s = Store::open(path, EventBudget::default()).unwrap();
    s.create_local(
        "seed",
        &CardRecord::new("card", "note", 1, "old", vec![]).unwrap(),
    )
    .unwrap();
    HostRuntime::new(s).unwrap()
}
#[test]
fn altered_commands_and_forged_completions_never_become_authoritative() {
    let command = Command::Rename(RenameRequest {
        operation_id: "op".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "new".into(),
    });
    let input = Invocation::new("task", &command).unwrap();
    let reference = tempfile::tempdir().unwrap();
    let mut reference_host = setup(&reference.path().join("db"));
    let mut c = reference_host.connect().unwrap();
    reference_host
        .grant(&mut c, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    let actual = reference_host
        .dispatch(&c, input.command_bytes(), || 1)
        .unwrap();
    let good = input.completion(&actual).unwrap();
    let forged = input
        .completion(
            &Response {
                request_id: "op".into(),
                outcome: Outcome::Rejected(Failure::Denied),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    for (calls, altered, completion, after, committed) in [
        (0, false, &good, "i32.const 0", false),
        (1, true, &good, "i32.const 0", false),
        (1, false, &forged, "i32.const 0", true),
        (2, false, &good, "i32.const 0", true),
        (1, false, &good, "unreachable", true),
    ] {
        let mut cmd = command.clone();
        if altered && let Command::Rename(ref mut r) = cmd {
            r.title = "unexpected".into();
        }
        let bytes = cmd.encode().unwrap();
        let call = format!(
            "i32.const 132000 i32.const {} i32.const 196608 i32.const 65536 call $e drop",
            bytes.len()
        );
        let wasm=wat::parse_str(format!(r#"(module (import "morrow_task_v1" "read_input" (func $read (param i32 i32)(result i32))) (import "morrow_task_v1" "complete" (func $done (param i32 i32)(result i32))) (import "morrow_v1" "exchange" (func $e(param i32 i32 i32 i32)(result i32))) (memory(export "memory") 4) (data(i32.const 132000) "{}") (data(i32.const 140000) "{}") (func(export "morrow_run")(result i32) i32.const 0 i32.const 131072 call $read drop {} i32.const 140000 i32.const {} call $done drop {after}))"#,data(&bytes),data(completion),format!("{call} ").repeat(calls),completion.len())).unwrap();
        let p = Package::build(
            Package::manifest_for_task("hostile", "1.0.0", &wasm, vec![Capability::RenameCard]),
            &wasm,
        )
        .unwrap();
        let p = PreparedPackage::new(p, Limits::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut host = setup(&dir.path().join("db"));
        let mut c = p.connect(&mut host).unwrap();
        host.grant(&mut c, GrantKind::Rename, "card", 100, 0)
            .unwrap();
        let r = p.run_task(&mut host, &c, &input, || 1, Cancellation::default());
        assert!(matches!(
            r.execution.outcome,
            Err(Fault::TaskProtocol | Fault::Trap)
        ));
        assert!(r.response.is_none());
        assert_eq!(
            matches!(
                host.store_local().lookup_for_card("card", "op").unwrap(),
                Lookup::Committed(_)
            ),
            committed
        );
        host.store_local().integrity_check().unwrap();
    }
}
