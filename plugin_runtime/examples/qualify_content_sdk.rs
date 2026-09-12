//! Real Rust/C/C++ Wasm SDK content commands against the same authorized core.
use morrow_core::{
    content_change::ContentChange,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::{Package, proto::Capability},
    response::{Failure, Outcome},
    runtime::{Command, CreateContent, ReadContent},
    store::{EventBudget, Store},
    task::Invocation,
};
use morrow_plugin_runtime::{Cancellation, Limits, package::PreparedPackage};
fn main() {
    let paths: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(paths.len(), 3);
    for path in paths {
        let wasm = std::fs::read(&path).unwrap();
        let package = Package::build(
            Package::manifest_for_task(
                "org.test.content",
                "0.1.9-test.11",
                &wasm,
                vec![
                    Capability::CreateContent,
                    Capability::EditContent,
                    Capability::ReadContent,
                ],
            ),
            &wasm,
        )
        .unwrap();
        let prepared = PreparedPackage::new(package, Limits::default()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let mut conn = prepared.connect(&mut host).unwrap();
        for kind in [
            GrantKind::CreateContent,
            GrantKind::EditContent,
            GrantKind::ReadContent,
        ] {
            host.grant(&mut conn, kind, "card", 100, 0).unwrap();
        }
        let commands = [
            Command::CreateContent(CreateContent {
                operation_id: "create".into(),
                card_id: "card".into(),
                type_id: "org.test.note".into(),
                format_version: 1,
                title: "create".into(),
                body: vec![0, 255],
            }),
            Command::EditContent(ContentChange {
                operation_id: "edit".into(),
                card_id: "card".into(),
                expected_revision: 1,
                title: "edit".into(),
                body: vec![42; 32768],
                preview_text: "preview".into(),
                attachments: None,
            }),
            Command::ReadContent(ReadContent {
                request_id: "read".into(),
                card_id: "card".into(),
                expected_revision: 2,
                offset: 32760,
                length: 32,
            }),
        ];
        for (i, command) in commands.iter().enumerate() {
            let input = Invocation::new(&format!("task-{i}"), command).unwrap();
            let result = prepared.run_task(&mut host, &conn, &input, || 1, Cancellation::default());
            assert_eq!(result.execution.outcome, Ok(0));
            assert_eq!(result.execution.host_calls, 1);
            match result.response.unwrap().outcome {
                Outcome::ContentCommitted(v) => assert_eq!(v.revision, i as u64 + 1),
                Outcome::ContentChunk(v) => {
                    assert_eq!(i, 2);
                    assert_eq!(v.bytes, vec![42; 8]);
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        host.revoke(&mut conn, GrantKind::ReadContent, "card")
            .unwrap();
        let input = Invocation::new("denied", &commands[2]).unwrap();
        let result = prepared.run_task(&mut host, &conn, &input, || 2, Cancellation::default());
        assert_eq!(result.execution.outcome, Ok(0));
        assert_eq!(
            result.response.unwrap().outcome,
            Outcome::Rejected(Failure::Denied)
        );
        host.store_local().integrity_check().unwrap();
        assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
        println!(
            "PASS: {path}: create, edit, binary body slice, authoritative completion and revocation"
        );
    }
}
