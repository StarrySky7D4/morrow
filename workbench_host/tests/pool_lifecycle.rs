#![cfg(target_os = "windows")]
mod common;
use morrow_core::{
    plugin_package::{Package, proto::TransformHandler},
    ui::{Document, Event, EventKind},
};
use morrow_plugin_runtime::{Fault, inline_ui::Failure};
use morrow_workbench_host::{Mutation, Workbench};
use morrow_workbench_plugin::Action;

fn package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.package_version = "0.1.9-test.33".into();
    manifest.transform_handlers.extend([
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
    ]);
    Package::build(manifest, original.module()).unwrap()
}
fn event(generation: u64, revision: u64, serial: u64, text: &str) -> Vec<u8> {
    Event {
        view: "workbench-tools".into(),
        generation,
        revision,
        serial,
        node: "text".into(),
        action: "text.edit".into(),
        kind: EventKind::EditText,
        text: text.into(),
        checked: false,
    }
    .encode()
    .unwrap()
}
fn configure(host: &mut Workbench, enabled: bool) {
    let s = host.plugin_status();
    host.configure_plugin(s.revision, &s.digest, enabled)
        .unwrap();
}
fn favorite(
    host: &mut Workbench,
    revision: u64,
    operation: &str,
) -> morrow_workbench_host::Result<morrow_workbench_host::Record> {
    host.apply(Mutation {
        operation,
        id: "card",
        revision,
        action: Action::Favorite,
        proposed: None,
        text: "",
        flag: true,
    })
}
// Real executable Wasm, assembled from standard sections: () -> i32 morrow_run.
// No mocked runtime, missing file, or module preparation failure stands in for execution.
fn fault_package(trap: bool) -> Package {
    let mut module = vec![
        0, 97, 115, 109, 1, 0, 0, 0, 1, 5, 1, 96, 0, 1, 127, 3, 2, 1, 0, 5, 3, 1, 0, 1, 7, 23, 2,
        10, b'm', b'o', b'r', b'r', b'o', b'w', b'_', b'r', b'u', b'n', 0, 0, 6, b'm', b'e', b'm',
        b'o', b'r', b'y', 2, 0,
    ];
    if trap {
        module.extend([10, 5, 1, 3, 0, 0, 11]);
    } else {
        module.extend([10, 6, 1, 4, 0, 65, 0, 11]);
    }
    let good = package();
    let mut manifest = Package::manifest_for_transform(
        "org.morrow.workbench",
        "0.1.9-test.34",
        &module,
        good.manifest().transform_handlers.clone(),
    );
    manifest.requested_capabilities = good.manifest().requested_capabilities.clone();
    Package::build(manifest, &module).unwrap()
}
fn seeded_fault(trap: bool) -> (tempfile::TempDir, Workbench) {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open_managed(dir.path(), Some(fault_package(trap))).unwrap();
    assert!(!host.writable(), "upgrades require explicit approval");
    configure(&mut host, true);
    assert!(
        host.writable(),
        "fault module must actually prepare and start"
    );
    (dir, host)
}
fn assert_readable_and_stopped(host: &mut Workbench) {
    assert!(!host.writable(), "faulted session must not remain Ready");
    assert_eq!(host.read("card").unwrap().revision, 1);
    assert_eq!(host.page("", 100).unwrap().0.len(), 1);
    assert!(host.create("after-fault", common::idea("denied")).is_err());
    assert!(favorite(host, 1, "after-fault-edit").is_err());
    assert!(host.query("概览", "全部", "", "最近添加").is_err());
    host.refresh_plugin_state().unwrap();
    assert!(
        !host.writable(),
        "refresh must not implicitly restart failed guest"
    );
}
#[test]
fn actual_content_and_online_ui_share_lifecycle_without_crossing_content_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    let first = host.ui_open("abc").unwrap();
    assert!(first.failure.is_none());
    let changed = host
        .ui_event(
            first.generation,
            &event(first.generation, first.revision, 1, "mixed text"),
        )
        .unwrap();
    assert!(changed.failure.is_none());
    let document = Document::decode(changed.document.as_ref().unwrap()).unwrap();
    assert_eq!(
        document
            .nodes()
            .iter()
            .find(|n| n.id == "preview")
            .unwrap()
            .text,
        "MIXED TEXT"
    );
    assert_eq!(host.read("card").unwrap().revision, 1);
    let saved = favorite(&mut host, 1, "favorite").unwrap();
    assert!(saved.idea.favorite);
    assert_eq!(saved.revision, 2);
    assert!(favorite(&mut host, 1, "conflict").is_err());
    assert!(host.writable(), "content CAS conflict is not a guest crash");
    assert_eq!(
        host.query("概览", "全部", "", "最近添加").unwrap(),
        ["card"]
    );
    host.ui_close(first.generation);
    assert!(
        host.writable(),
        "closing a view must not close the content session"
    );
    host.create("second", common::idea("second")).unwrap();
    host.finish().unwrap();
    drop(host);
    let reopened = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert_eq!(reopened.read("card").unwrap().revision, 2);
    assert_eq!(reopened.page("", 100).unwrap().0.len(), 2);
}
#[test]
fn disable_reenable_and_stale_close_cannot_reuse_old_ui_authority() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    host.create("seed", common::idea("card")).unwrap();
    let first = host.ui_open("first").unwrap();
    let s = host.plugin_status();
    assert!(
        host.configure_plugin(s.revision + 1, &s.digest, false)
            .is_err()
    );
    assert!(
        host.ui_event(
            first.generation,
            &event(first.generation, 1, 1, "still live")
        )
        .unwrap()
        .failure
        .is_none()
    );
    configure(&mut host, false);
    assert!(!host.writable());
    assert!(
        host.ui_event(first.generation, &event(first.generation, 2, 2, "old"))
            .is_err()
    );
    configure(&mut host, true);
    let second = host.ui_open("second").unwrap();
    assert!(second.generation > first.generation);
    host.ui_close(first.generation);
    assert!(
        host.ui_event(second.generation, &event(second.generation, 1, 1, "fresh"))
            .unwrap()
            .failure
            .is_none()
    );
    assert!(
        host.ui_event(first.generation, &event(first.generation, 2, 2, "stale"))
            .is_err()
    );
    configure(&mut host, false);
    host.finish().unwrap();
    drop(host);
    let mut host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert!(!host.writable());
    assert!(!host.plugin_status().enabled);
    assert_eq!(host.read("card").unwrap().revision, 1);
    configure(&mut host, true);
    assert_eq!(favorite(&mut host, 1, "after-restart").unwrap().revision, 2);
}
#[test]
fn finish_closes_ui_and_writes_but_keeps_durable_cards_and_attachments_readable() {
    let dir = tempfile::tempdir().unwrap();
    let mut host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    let original = b"synthetic attachment\0\xff";
    let asset = host
        .import(
            "card",
            "sample.bin",
            "file",
            &mut &original[..],
            original.len() as u64,
        )
        .unwrap();
    let mut draft = common::idea("card");
    draft.assets.push(asset.clone());
    host.create("seed", draft).unwrap();
    let ui = host.ui_open("before finish").unwrap();
    host.finish().unwrap();
    host.finish().unwrap();
    assert_readable_and_stopped(&mut host);
    assert!(
        host.ui_event(ui.generation, &event(ui.generation, 1, 1, "late"))
            .is_err()
    );
    assert!(host.ui_open("closed").is_err());
    assert!(
        host.import(
            "card",
            "late.bin",
            "file",
            &mut &original[..],
            original.len() as u64
        )
        .is_err()
    );
    let mut exported = Vec::new();
    host.export("card", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, original);
    drop(host);
    let host = Workbench::open_managed(dir.path(), Some(package())).unwrap();
    assert!(
        host.writable(),
        "finish must not persist an explicit user disable"
    );
    assert_eq!(host.read("card").unwrap().revision, 1);
}
#[test]
fn actual_command_trap_quarantines_session_without_mutating_existing_content() {
    let (_dir, mut host) = seeded_fault(true);
    let error = host
        .create("trap", common::idea("trap"))
        .err()
        .expect("actual guest trap");
    assert!(error.to_string().contains("Trap"), "{error}");
    assert_readable_and_stopped(&mut host);
}
#[test]
fn actual_guest_missing_completion_is_a_protocol_fault_not_a_healthy_session() {
    let (_dir, mut host) = seeded_fault(false);
    let error = host
        .create("missing-completion", common::idea("bad"))
        .err()
        .expect("missing completion must fail");
    assert!(error.to_string().contains("TaskProtocol"), "{error}");
    assert_readable_and_stopped(&mut host);
}
#[test]
fn actual_ui_trap_also_quarantines_the_shared_business_session() {
    let (_dir, mut host) = seeded_fault(true);
    let reply = host.ui_open("trap").unwrap();
    assert!(
        matches!(reply.failure, Some(Failure::Execution(Fault::Trap))),
        "{:?}",
        reply.failure
    );
    assert_readable_and_stopped(&mut host);
    assert!(
        host.ui_event(reply.generation, &event(reply.generation, 1, 1, "late"))
            .is_err()
    );
}
