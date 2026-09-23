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
    Workbench, content_api, content_api_capnp as content, host_capnp as wire, protocol,
};
use morrow_workbench_plugin::{Idea, PACKAGE_VERSION};

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.versioned-no-commit",
        PACKAGE_VERSION,
        &module,
        [
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

fn edit_card_request() -> Vec<u8> {
    let mut payload = Builder::new_default();
    {
        let mut action = payload.init_root::<content::card_edit::Builder>();
        action.set_version(1);
        action.set_digest(&content_api::digest());
        action.set_action(content::CardAction::SetFavorite);
        action.set_favorite(true);
    }
    let mut request = Builder::new_default();
    {
        let mut outer = request.init_root::<wire::request::Builder>();
        outer.set_version(1);
        outer.set_digest(&protocol::digest());
        outer.set_action(wire::Action::EditCard);
        outer.set_id("card");
        outer.set_operation("sqlite-commit-failure");
        outer.set_revision(2);
        outer.set_payload(&serialize::write_message_to_words(&payload));
    }
    serialize::write_message_to_words(&request)
}

#[test]
fn sqlite_error_inside_commit_remains_unknown_even_when_operation_is_absent() {
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
            ..Default::default()
        },
    )
    .unwrap();
    let plan = host.plan_tasks_migration("card").unwrap();
    host.migrate_tasks(&plan.operation, "card", 1).unwrap();

    // The storage failure occurs after commit_card_edit is entered. SQLite
    // rolls it back, but absence alone must not upgrade this to NoCommit.
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER versioned_commit_failure BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'synthetic versioned commit failure'); END;",
    )
    .unwrap();
    let frame = edit_card_request();
    let bytes = protocol::respond(&mut host, &frame).unwrap();
    let message = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert_ne!(response.get_error().unwrap().to_str().unwrap(), "");
    assert_eq!(response.get_ui_code(), 0);
    assert_eq!(response.get_revision(), 0);
    assert!(response.get_payload().unwrap().is_empty());
    let morrow_workbench_host::versioned_record::VersionedRecord::Tasks(record) =
        host.read_versioned("card").unwrap()
    else {
        panic!("expected migrated card");
    };
    assert_eq!(record.revision, 2);
    let count: i64 = sql
        .query_row(
            "SELECT count(*) FROM operations WHERE id=?1 AND card_id=?2",
            ["sqlite-commit-failure", "card"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);

    sql.execute_batch("DROP TRIGGER versioned_commit_failure;")
        .unwrap();
    let bytes = protocol::respond(&mut host, &frame).unwrap();
    let message = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(response.get_error().unwrap().to_str().unwrap(), "");
    assert_eq!(response.get_revision(), 3);
    assert_eq!(response.get_ui_code(), 0);
}
