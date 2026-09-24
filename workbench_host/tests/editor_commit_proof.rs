#![cfg(target_os = "windows")]
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_host::{
    Mutation, Workbench, capture_provenance::EditorSnapshot, host_capnp as wire, protocol,
    versioned_record::VersionedRecord,
};
use morrow_workbench_plugin::{Action, Asset, Idea, PACKAGE_VERSION, cards_v2::Fields};

const CARD: &str = "proof-card";

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.editor-commit-proof",
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
            (
                "capture.convert",
                "morrow.capture.request.v1",
                "morrow.capture.response.v1",
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

fn idea(title: &str) -> Idea {
    Idea {
        id: CARD.into(),
        title: title.into(),
        description: "Body".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        todos: vec!["task".into()],
        ..Default::default()
    }
}
fn snapshot(value: &Idea) -> EditorSnapshot {
    EditorSnapshot {
        title: value.title.clone(),
        description: value.description.clone(),
        hypothesis: value.hypothesis.clone(),
        conclusion: value.conclusion.clone(),
        todos: value.todos.join("\n"),
        aliases: vec![],
    }
}
fn inspect(
    host: &mut Workbench,
    id: &str,
    operation: &str,
    extra: bool,
) -> (String, u64, Option<(String, String, Vec<u8>, u64, u64)>) {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::InspectEditorCommit);
    request.set_id(id);
    request.set_operation(operation);
    if extra {
        request.set_selected_path("forbidden");
    }
    let result = protocol::respond(host, &serialize::write_message_to_words(&message)).unwrap();
    let mut raw = result.as_slice();
    let response = serialize::read_message_from_flat_slice(&mut raw, Default::default()).unwrap();
    assert!(raw.is_empty());
    let response = response.get_root::<wire::response::Reader>().unwrap();
    let error = response.get_error().unwrap().to_str().unwrap().to_owned();
    let proof = if response.has_editor_commit_proof() {
        let value = response.get_editor_commit_proof().unwrap();
        Some((
            value.get_id().unwrap().to_str().unwrap().to_owned(),
            value.get_operation().unwrap().to_str().unwrap().to_owned(),
            value.get_digest().unwrap().to_vec(),
            value.get_source_revision(),
            value.get_committed_revision(),
        ))
    } else {
        None
    };
    (error, response.get_revision(), proof)
}

#[test]
fn historical_create_legacy_edit_and_v2_edit_remain_exact_and_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let first = idea("created");
    let scope = host.open_capture_scope(CARD, 0).unwrap();
    host.create_captured("captured-create", first.clone(), &scope, snapshot(&first))
        .unwrap();
    let create_evidence = host.operation_evidence(CARD, "captured-create").unwrap();
    assert_eq!(create_evidence.len(), 1);

    let mut legacy = first.clone();
    legacy.title = "legacy edit".into();
    let scope = host.open_capture_scope(CARD, 1).unwrap();
    host.apply_captured(
        Mutation {
            operation: "captured-legacy",
            id: CARD,
            revision: 1,
            action: Action::Edit,
            proposed: Some(legacy.clone()),
            text: "",
            flag: false,
        },
        &scope,
        snapshot(&legacy),
    )
    .unwrap();

    let migration = host.plan_tasks_migration(CARD).unwrap();
    host.migrate_tasks(&migration.operation, CARD, migration.source_revision)
        .unwrap();
    let source = match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(value) => value,
        VersionedRecord::Legacy(_) => panic!("expected V2 source"),
    };
    let source_revision = source.revision;
    let properties = &source.properties;
    let fields = Fields {
        title: "v2 edit".into(),
        description: properties.description.clone(),
        hypothesis: properties.hypothesis.clone(),
        conclusion: properties.conclusion.clone(),
        icon: properties.icon as u16,
        color: properties.color,
        assets: properties
            .assets
            .iter()
            .map(|asset| Asset {
                id: asset.id.clone(),
                name: asset.name.clone(),
                kind: asset.kind.clone(),
                bytes: asset.bytes,
            })
            .collect(),
    };
    let scope = host.open_capture_scope(CARD, source_revision).unwrap();
    host.edit_card_captured(
        "captured-v2",
        CARD,
        source_revision,
        &fields,
        &scope,
        EditorSnapshot {
            title: fields.title.clone(),
            description: fields.description.clone(),
            hypothesis: fields.hypothesis.clone(),
            conclusion: fields.conclusion.clone(),
            todos: String::new(),
            aliases: vec![],
        },
    )
    .unwrap();
    let current = match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(value) => (value.revision, value.title),
        VersionedRecord::Legacy(_) => panic!("expected V2 source"),
    };
    for (operation, source, committed) in [
        ("captured-create", 0, 1),
        ("captured-legacy", 1, 2),
        ("captured-v2", source_revision, source_revision + 1),
    ] {
        let proof = host.inspect_editor_commit(CARD, operation).unwrap();
        assert_eq!(proof.id, CARD);
        assert_eq!(proof.operation, operation);
        assert_eq!(proof.source_revision, source);
        assert_eq!(proof.committed_revision, committed);
        assert_eq!(proof.digest.len(), 32);
        assert_eq!(
            proof.digest.as_slice(),
            host.operation_evidence(CARD, operation).unwrap()[0].digest()
        );
        let (error, revision, wire) = inspect(&mut host, CARD, operation, false);
        assert!(error.is_empty(), "{error}");
        assert_eq!(revision, committed);
        assert_eq!(
            wire.unwrap(),
            (
                CARD.into(),
                operation.into(),
                proof.digest.to_vec(),
                source,
                committed
            )
        );
    }
    match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(value) => assert_eq!((value.revision, value.title), current),
        VersionedRecord::Legacy(_) => panic!("expected V2 source"),
    }
    for operation in ["captured-create", "captured-legacy", "captured-v2"] {
        let before = host.operation_evidence(CARD, operation).unwrap()[0].digest();
        assert!(!inspect(&mut host, CARD, operation, true).0.is_empty());
        assert_eq!(
            host.operation_evidence(CARD, operation).unwrap()[0].digest(),
            before
        );
    }
    assert!(host.inspect_editor_commit(CARD, "missing").is_err());
    assert!(
        host.inspect_editor_commit("other", "captured-create")
            .is_err()
    );
    assert!(
        host.inspect_editor_commit(CARD, &migration.operation)
            .is_err()
    );
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let (error, revision, proof) = inspect(&mut host, CARD, "captured-create", false);
    assert!(error.is_empty(), "{error}");
    assert_eq!(revision, 1);
    assert_eq!(proof.unwrap().3, 0);
    match host.read_versioned(CARD).unwrap() {
        VersionedRecord::Tasks(value) => assert_eq!((value.revision, value.title), current),
        VersionedRecord::Legacy(_) => panic!("expected V2 source"),
    }
}
