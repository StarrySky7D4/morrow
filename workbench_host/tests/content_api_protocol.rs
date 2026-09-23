#![cfg(target_os = "windows")]
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_host::{
    Workbench, content_api_capnp as content, host_capnp as wire, protocol,
};
use morrow_workbench_plugin::{Idea, PACKAGE_VERSION};
fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.cards-owner",
        PACKAGE_VERSION,
        &module,
        [
            (
                "studio.command",
                "morrow.studio.request.v1",
                "morrow.studio.response.v1",
            ),
            (
                "workbench.command",
                "morrow.workbench.request.v1",
                "morrow.workbench.response.v1",
            ),
            (
                "workbench.tasks.v2",
                "morrow.workbench.tasks.request.v2",
                "morrow.workbench.tasks.response.v2",
            ),
            (
                "workbench.cards.v2",
                "morrow.workbench.cards.request.v2",
                "morrow.workbench.cards.response.v2",
            ),
        ]
        .into_iter()
        .map(|(handler, input_type, output_type)| TransformHandler {
            handler: handler.into(),
            input_type: input_type.into(),
            output_type: output_type.into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        })
        .collect(),
    );
    manifest.requested_capabilities = vec![
        Capability::CreateContent as i32,
        Capability::EditContent as i32,
        Capability::ReadContent as i32,
    ];
    Package::build(manifest, &module).unwrap()
}

fn request(
    action: wire::Action,
    id: &str,
    operation: &str,
    revision: u64,
    capture: bool,
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    request.set_id(id);
    request.set_operation(operation);
    request.set_revision(revision);
    request.set_limit(1);
    if capture {
        request.set_capture_scope("unvalidated-editor-scope");
    }
    serialize::write_message_to_words(&message)
}
fn reply(host: &mut Workbench, request: &[u8]) -> capnp::message::Reader<serialize::OwnedSegments> {
    let bytes = protocol::respond(host, request).unwrap();
    serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap()
}
#[test]
fn private_routes_preserve_migration_receipts_and_reject_incomplete_capture() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    host.create(
        "seed",
        Idea {
            id: "card".into(),
            title: "Card".into(),
            category: "进行中".into(),
            stage: "计划中".into(),
            todos: vec!["same".into(), "same".into()],
            completed: vec!["same".into()],
            ..Default::default()
        },
    )
    .unwrap();
    let plan = host.plan_tasks_migration("card").unwrap();
    for action in [
        wire::Action::ReadVersioned,
        wire::Action::PageVersioned,
        wire::Action::PlanTasksMigration,
        wire::Action::MigrateTasks,
        wire::Action::EditTasks,
        wire::Action::EditCard,
        wire::Action::QueryVersioned,
    ] {
        let response = reply(
            &mut host,
            &request(action, "card", &plan.operation, 1, true),
        );
        let r = response.get_root::<wire::response::Reader>().unwrap();
        assert!(
            r.get_error()
                .unwrap()
                .to_str()
                .unwrap()
                .contains("capture metadata")
        );
        assert!(r.get_payload().unwrap().is_empty());
        assert_eq!(host.read("card").unwrap().revision, 1);
    }
    let frame = request(
        wire::Action::MigrateTasks,
        "card",
        &plan.operation,
        1,
        false,
    );
    for repeated in [false, true] {
        let response = reply(&mut host, &frame);
        let r = response.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(r.get_error().unwrap().to_str().unwrap(), "");
        assert_eq!(r.get_revision(), 2);
        let envelope =
            serialize::read_message(&mut r.get_payload().unwrap(), ReaderOptions::new()).unwrap();
        let e = envelope.get_root::<content::envelope::Reader>().unwrap();
        assert_eq!(e.get_kind().unwrap(), content::EnvelopeKind::Commit);
        assert_eq!(e.get_repeated(), repeated);
        assert_eq!(e.get_source_revision(), 1);
        let record = e.get_record().unwrap();
        assert_eq!(record.get_format_version(), 2);
        assert_eq!(record.get_tasks().unwrap().len(), 2);
        assert_eq!(record.get_ambiguous_count(), 2);
    }
    // Closing the sole owner and reopening without any guest still permits
    // typed reads and enumeration. No migration or plugin execution is needed.
    drop(host);
    let mut readonly = Workbench::open(&path, None).unwrap();
    for action in [wire::Action::ReadVersioned, wire::Action::PageVersioned] {
        let response = reply(&mut readonly, &request(action, "card", "", 0, false));
        let r = response.get_root::<wire::response::Reader>().unwrap();
        assert_eq!(r.get_error().unwrap().to_str().unwrap(), "");
        assert!(r.get_read_only());
        if action == wire::Action::ReadVersioned {
            assert_eq!(r.get_revision(), 2);
        } else {
            assert_eq!(
                r.get_ids().unwrap().get(0).unwrap().to_str().unwrap(),
                "card"
            );
        }
    }
}
