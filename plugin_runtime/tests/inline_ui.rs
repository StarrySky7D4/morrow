#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, proto::TransformHandler},
    store::Store,
    task::{Invocation, Transform},
    ui::{Document, Event, EventKind, Kind, Node},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    inline_ui::{Error, Failure, InlineUi},
    package::PreparedPackage,
};
use std::time::Duration;
fn doc(text: &str) -> Document {
    let root = Node::new("root", "", Kind::Column);
    let mut input = Node::new("input", "root", Kind::TextInput);
    input.label = "Preview".into();
    input.action = "preview".into();
    input.max_bytes = 128;
    input.text = text.into();
    Document::new(vec![root, input]).unwrap()
}
fn event(serial: u64) -> Vec<u8> {
    Event {
        view: "view".into(),
        generation: 7,
        revision: 1,
        serial,
        node: "input".into(),
        action: "preview".into(),
        kind: EventKind::EditText,
        text: "changed".into(),
        checked: false,
    }
    .encode()
    .unwrap()
}
fn invocation(id: &str, handler: &str, kind: &str, input: Vec<u8>) -> Invocation {
    Invocation::new_transform(
        id,
        Transform {
            handler: handler.into(),
            input_type: kind.into(),
            output_type: "morrow.ui.document.v1".into(),
            input,
        },
    )
    .unwrap()
}
fn data(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
fn package(mode: &str) -> PreparedPackage {
    let first = invocation("inline-ui-1", "ui.form", "text.utf8", b"seed".to_vec());
    let second = invocation("inline-ui-2", "ui.edit", "morrow.ui.event.v1", event(1));
    let open = first
        .output_completion(&doc("seed").encode().unwrap())
        .unwrap();
    let edit = second
        .output_completion(&doc("changed").encode().unwrap())
        .unwrap();
    let marker = first
        .bytes()
        .windows(b"inline-ui-1".len())
        .position(|v| v == b"inline-ui-1")
        .unwrap()
        + b"inline-ui-".len();
    let edit_code = match mode {
        "trap" => "unreachable".into(),
        "loop" => "(loop $forever br $forever)".into(),
        "exchange" => {
            "i32.const 1 i32.const 1 i32.const 196608 i32.const 65536 call $exchange drop".into()
        }
        _ => format!("i32.const 16384 i32.const {} call $done drop", edit.len()),
    };
    let exit = if mode == "nonzero" { 1 } else { 0 };
    let wasm = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32)(result i32)))
      (import "morrow_v1" "exchange" (func $exchange (param i32 i32 i32 i32)(result i32)))
      (memory(export "memory") 4)
      (data(i32.const 0) "{}") (data(i32.const 16384) "{}")
      (func(export "morrow_run")(result i32)
        i32.const 65536 i32.const 131072 call $read drop
        i32.const {} i32.load8_u i32.const 49 i32.eq
        if i32.const 0 i32.const {} call $done drop else {} end
        i32.const {}))"#,
        data(&open),
        data(&edit),
        65536 + marker,
        open.len(),
        edit_code,
        exit
    ))
    .unwrap();
    let handlers = vec![
        TransformHandler {
            handler: "ui.form".into(),
            input_type: "text.utf8".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 32,
            max_output_bytes: 65536,
        },
        TransformHandler {
            handler: "ui.edit".into(),
            input_type: "morrow.ui.event.v1".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        },
    ];
    PreparedPackage::new(
        Package::build(
            Package::manifest_for_transform("test.inline", "1.0.0", &wasm, handlers),
            &wasm,
        )
        .unwrap(),
        Limits {
            fuel: 10000,
            ..Limits::default()
        },
    )
    .unwrap()
}
fn host(root: &tempfile::TempDir) -> HostRuntime {
    HostRuntime::new(Store::open(&root.path().join("db"), Default::default()).unwrap()).unwrap()
}
#[test]
fn real_wasm_open_and_event_replace_ui_without_business_writes() {
    let root = tempfile::tempdir().unwrap();
    let mut host = host(&root);
    let package = package("success");
    let c = package.connect(&mut host).unwrap();
    let mut ui = InlineUi::new(&package, &host, &c, "view", 7, Duration::from_secs(1)).unwrap();
    let first = ui.open(&package, &mut host, &c, "seed").unwrap();
    assert!(first.failure.is_none());
    assert_eq!(first.revision, 1);
    assert_eq!(first.serial, 0);
    assert_eq!(
        Document::decode(first.document.as_ref().unwrap()).unwrap(),
        doc("seed")
    );
    let update = ui.event(&package, &mut host, &c, &event(1)).unwrap();
    assert!(update.failure.is_none());
    assert_eq!(update.revision, 2);
    assert_eq!(update.serial, 1);
    assert_eq!(
        Document::decode(update.document.as_ref().unwrap()).unwrap(),
        doc("changed")
    );
    assert!(host.store_local().pending(0, 10).unwrap().is_empty());
}
#[test]
fn rejected_events_preserve_serial_and_guest_failures_consume_it_without_revision() {
    for mode in ["trap", "loop", "exchange"] {
        let root = tempfile::tempdir().unwrap();
        let mut host = host(&root);
        let package = package(mode);
        let c = package.connect(&mut host).unwrap();
        let mut ui = InlineUi::new(&package, &host, &c, "view", 7, Duration::from_secs(1)).unwrap();
        assert!(
            ui.open(&package, &mut host, &c, "seed")
                .unwrap()
                .failure
                .is_none()
        );
        for invalid in [vec![1, 2], vec![0; 65537], event(2)] {
            assert!(ui.event(&package, &mut host, &c, &invalid).is_err());
            assert_eq!(ui.serial(), 0);
        }
        let failed = ui.event(&package, &mut host, &c, &event(1)).unwrap();
        assert!(matches!(
            failed.failure,
            Some(Failure::Execution(
                Fault::Trap | Fault::Limits | Fault::TaskProtocol
            ))
        ));
        assert_eq!((failed.revision, failed.serial), (1, 1));
        assert!(failed.document.is_none());
        assert!(ui.event(&package, &mut host, &c, &event(1)).is_err());
        assert_eq!(ui.serial(), 1);
        assert!(host.store_local().pending(0, 10).unwrap().is_empty());
    }
}
#[test]
fn same_package_other_connection_foreign_host_and_revocation_cannot_rebind_view() {
    let root = tempfile::tempdir().unwrap();
    let mut host = host(&root);
    let package = package("success");
    let c = package.connect(&mut host).unwrap();
    let other = package.connect(&mut host).unwrap();
    let mut ui = InlineUi::new(&package, &host, &c, "view", 7, Duration::from_secs(1)).unwrap();
    assert!(matches!(
        ui.open(&package, &mut host, &other, "seed"),
        Err(Error::Binding)
    ));
    let d = tempfile::tempdir().unwrap();
    let mut foreign =
        HostRuntime::new(Store::open(&d.path().join("db"), Default::default()).unwrap()).unwrap();
    assert!(matches!(
        ui.open(&package, &mut foreign, &c, "seed"),
        Err(Error::Unavailable)
    ));
    host.revocation(&c).unwrap().revoke();
    assert!(matches!(
        ui.open(&package, &mut host, &c, "seed"),
        Err(Error::Unavailable)
    ));
    assert_eq!((ui.revision(), ui.serial()), (0, 0));
}
#[test]
fn nonzero_exit_and_closed_view_never_publish_a_document() {
    let root = tempfile::tempdir().unwrap();
    let mut host = host(&root);
    let package = package("nonzero");
    let c = package.connect(&mut host).unwrap();
    let mut ui = InlineUi::new(&package, &host, &c, "view", 7, Duration::from_secs(1)).unwrap();
    let failed = ui.open(&package, &mut host, &c, "seed").unwrap();
    assert!(matches!(
        failed.failure,
        Some(Failure::Execution(Fault::TaskProtocol))
    ));
    assert!(failed.document.is_none());
    assert_eq!(ui.revision(), 0);
    ui.close();
    assert!(matches!(
        ui.open(&package, &mut host, &c, "seed"),
        Err(Error::Closed)
    ));
}
