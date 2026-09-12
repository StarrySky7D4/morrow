#![cfg(feature = "packages")]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, proto::TransformHandler},
    store::{EventBudget, Store},
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    package::PreparedPackage,
    ui_session::{Error, Failure, UiSession, Update},
};
use std::time::{Duration, Instant};
fn session(module: &[u8]) -> (tempfile::TempDir, UiSession) {
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
    let p = Package::build(
        Package::manifest_for_transform("org.morrow.ui-test", "0.1.0", module, handlers),
        module,
    )
    .unwrap();
    let p = PreparedPackage::new(
        p,
        Limits {
            fuel: 10000,
            ..Limits::default()
        },
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let host =
        HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    (
        dir,
        UiSession::new(p, host, "view", 1, Duration::from_secs(10)).unwrap(),
    )
}
fn finish(s: &mut UiSession) {
    s.close();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(host) = s.try_finish().unwrap() {
            assert!(host.store_local().pending(0, 10).unwrap().is_empty());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn result(s: &mut UiSession) -> Update {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(v) = s.poll() {
            return v;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn reply(id: &str, output: &[u8]) -> Vec<u8> {
    let i = Invocation::new_transform(
        id,
        Transform {
            handler: "ui.form".into(),
            input_type: "text.utf8".into(),
            output_type: "morrow.ui.document.v1".into(),
            input: b"seed".to_vec(),
        },
    )
    .unwrap();
    let completion = i.output_completion(output).unwrap();
    let data = completion
        .iter()
        .map(|b| format!("\\{b:02x}"))
        .collect::<String>();
    wat::parse_str(format!(
        r#"(module
       (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
       (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
       (memory (export "memory") 4)
       (data (i32.const 0) "{data}")
       (func (export "morrow_run") (result i32)
          i32.const 65536 i32.const 131072 call $read drop
          i32.const 0 i32.const {} call $complete drop i32.const 0))"#,
        completion.len()
    ))
    .unwrap()
}
#[test]
fn malformed_document_is_failure_without_revision_or_content() {
    let (_dir, mut s) = session(&reply("ui-1", b"not a UI document"));
    s.open_form("seed").unwrap();
    assert!(matches!(
        result(&mut s),
        Update::Failed {
            failure: Failure::InvalidDocument,
            ..
        }
    ));
    assert_eq!(s.revision(), 0);
    finish(&mut s);
}
#[test]
fn wrong_task_completion_cannot_become_ui() {
    let (_dir, mut s) = session(&reply("wrong-task", b"payload"));
    s.open_form("seed").unwrap();
    assert!(matches!(
        result(&mut s),
        Update::Failed {
            failure: Failure::Execution(Fault::TaskProtocol),
            ..
        }
    ));
    assert_eq!(s.revision(), 0);
    finish(&mut s);
}
#[test]
fn runaway_guest_is_bounded_and_oversized_events_are_rejected() {
    let module=wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) (loop $forever br $forever) i32.const 0))"#).unwrap();
    let (_dir, mut s) = session(&module);
    assert!(matches!(
        s.event(&vec![0; 65537]),
        Err(Error::Core(morrow_core::Error::Limit))
    ));
    s.open_form("seed").unwrap();
    assert!(matches!(
        result(&mut s),
        Update::Failed {
            failure: Failure::Execution(Fault::Limits),
            ..
        }
    ));
    assert_eq!(s.revision(), 0);
    finish(&mut s);
}
