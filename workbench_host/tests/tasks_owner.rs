#![cfg(target_os = "windows")]

use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_host::{Mutation, Workbench, versioned_record::VersionedRecord};
use morrow_workbench_plugin::{
    Action, Idea, PACKAGE_VERSION,
    tasks_v2::{Command, Completion},
};

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.tasks-owner",
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

fn idea(id: &str) -> Idea {
    Idea {
        id: id.into(),
        title: format!("Task card {id}"),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["same".into(), "same".into()],
        completed: vec!["same".into()],
        ..Default::default()
    }
}

fn tasks(host: &Workbench, id: &str) -> morrow_workbench_host::versioned_record::TaskRecord {
    match host.read_versioned(id).unwrap() {
        VersionedRecord::Tasks(record) => record,
        VersionedRecord::Legacy(_) => panic!("card remained in legacy format"),
    }
}

#[test]
fn owner_migrates_real_guest_then_edits_seven_commands_and_replays_historical_receipts() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    assert!(host.writable());
    assert_eq!(host.create("seed-card", idea("card")).unwrap().revision, 1);
    assert_eq!(
        host.create("seed-legacy", idea("legacy")).unwrap().revision,
        1
    );
    assert!(matches!(
        host.read_versioned("card").unwrap(),
        VersionedRecord::Legacy(_)
    ));
    let plan = host.plan_tasks_migration("card").unwrap();
    assert_eq!(plan.card_id, "card");
    assert_eq!(plan.source_revision, 1);
    assert!(host.migrate_tasks("wrong-operation", "card", 1).is_err());
    assert_eq!(host.read("card").unwrap().revision, 1);

    let migrated = host
        .migrate_tasks(&plan.operation, "card", plan.source_revision)
        .unwrap();
    assert!(!migrated.repeated);
    assert_eq!(migrated.committed.revision, 2);
    assert_eq!(migrated.receipt.revision, 2);
    assert_eq!(
        host.operation_evidence("card", &plan.operation)
            .unwrap()
            .len(),
        1
    );
    let current = tasks(&host, "card");
    assert_eq!(current.revision, 2);
    assert_eq!(current.properties.tasks.len(), 2);
    let first = current.properties.tasks[0].id.clone();
    let second = current.properties.tasks[1].id.clone();
    assert_ne!(first, second);
    assert!(
        current
            .properties
            .tasks
            .iter()
            .all(|t| t.completion == Completion::LegacyAmbiguous as i32)
    );
    let (page, cursor) = host.page_versioned("", 10).unwrap();
    assert_eq!(page.len(), 2);
    assert_eq!(cursor, "legacy");
    assert!(
        page.iter()
            .any(|r| matches!(r, VersionedRecord::Tasks(t) if t.id == "card"))
    );
    assert!(
        page.iter()
            .any(|r| matches!(r, VersionedRecord::Legacy(t) if t.idea.id == "legacy"))
    );
    assert!(host.read("card").is_err());
    assert!(
        host.apply(Mutation {
            operation: "old-action-on-v2",
            id: "card",
            revision: 2,
            action: Action::Favorite,
            proposed: None,
            text: "",
            flag: true,
        })
        .is_err()
    );
    assert_eq!(tasks(&host, "card").revision, 2);

    let first_command = Command::SetCompletion {
        id: first.clone(),
        complete: true,
    };
    let first_edit = host
        .edit_tasks("edit-first", "card", 2, &first_command)
        .unwrap();
    assert!(!first_edit.repeated);
    assert_eq!(first_edit.committed.revision, 3);
    assert_eq!(
        host.operation_evidence("card", "edit-first").unwrap().len(),
        1
    );
    let after_first = tasks(&host, "card");
    assert_eq!(
        after_first.properties.tasks[0].completion,
        Completion::Complete as i32
    );
    assert_eq!(
        after_first.properties.tasks[1].completion,
        Completion::LegacyAmbiguous as i32
    );

    let commands = [
        ("edit-stage", Command::SetStage("推进中".into())),
        (
            "edit-order",
            Command::Reorder(vec![second.clone(), first.clone()]),
        ),
        (
            "edit-rename",
            Command::Rename {
                id: second.clone(),
                text: "renamed".into(),
            },
        ),
        (
            "edit-add",
            Command::Add {
                id: "added-task".into(),
                text: "same".into(),
            },
        ),
        ("edit-remove", Command::Remove("added-task".into())),
        (
            "edit-complete",
            Command::CompleteAllAndSetStage("已完成".into()),
        ),
    ];
    for (operation, command) in &commands {
        let before = tasks(&host, "card");
        let result = host
            .edit_tasks(operation, "card", before.revision, command)
            .unwrap();
        assert!(!result.repeated);
        assert_eq!(result.committed.revision, before.revision + 1);
        assert_eq!(host.operation_evidence("card", operation).unwrap().len(), 1);
        let after = tasks(&host, "card");
        if *operation == "edit-stage" {
            assert_eq!(after.projection.stage, "推进中");
            assert_eq!(after.projection.ambiguous, before.projection.ambiguous);
            assert_eq!(after.projection.complete, before.projection.complete);
        }
        if *operation == "edit-order" {
            assert_eq!(after.properties.tasks[0].id, second);
            assert_eq!(after.properties.tasks[1].id, first);
        }
        if *operation == "edit-rename" {
            assert_eq!(after.properties.tasks[0].text, "renamed");
        }
        if *operation == "edit-remove" {
            assert!(
                after
                    .properties
                    .retired_task_ids
                    .contains(&"added-task".to_owned())
            );
            assert!(
                host.edit_tasks(
                    "reuse-retired",
                    "card",
                    after.revision,
                    &Command::Add {
                        id: "added-task".into(),
                        text: "again".into()
                    }
                )
                .is_err()
            );
        }
    }
    let latest = tasks(&host, "card");
    assert_eq!(latest.revision, 9);
    assert_eq!(latest.projection.stage, "已完成");
    assert_eq!(latest.projection.complete, 2);
    assert!(latest.properties.tasks.iter().any(|t| t.id == first));
    assert!(latest.properties.tasks.iter().any(|t| t.id == second));
    assert!(
        host.edit_tasks(
            "edit-first",
            "card",
            2,
            &Command::SetCompletion {
                id: second.clone(),
                complete: true
            }
        )
        .is_err()
    );
    assert!(
        host.edit_tasks("edit-first", "card", 3, &first_command)
            .is_err()
    );
    assert!(
        host.edit_tasks("new-stale-edit", "card", 2, &first_command)
            .is_err()
    );
    let historical = host
        .edit_tasks("edit-first", "card", 2, &first_command)
        .unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.receipt, first_edit.receipt);
    assert_eq!(historical.committed.revision, 3);
    assert_eq!(tasks(&host, "card").revision, 9);
    let migration_retry = host.migrate_tasks(&plan.operation, "card", 1).unwrap();
    assert!(migration_retry.repeated);
    assert_eq!(migration_retry.receipt, migrated.receipt);
    assert_eq!(migration_retry.committed.revision, 2);
    assert_eq!(tasks(&host, "card").revision, 9);

    host.finish().unwrap();
    drop(host);
    let mut reopened = Workbench::open(&path, Some(package())).unwrap();
    assert_eq!(tasks(&reopened, "card").revision, 9);
    let retry = reopened
        .edit_tasks("edit-first", "card", 2, &first_command)
        .unwrap();
    assert!(retry.repeated);
    assert_eq!(retry.receipt, first_edit.receipt);
    assert_eq!(tasks(&reopened, "card").revision, 9);
    let migration_retry = reopened.migrate_tasks(&plan.operation, "card", 1).unwrap();
    assert!(migration_retry.repeated);
    assert_eq!(migration_retry.receipt, migrated.receipt);
    assert_eq!(tasks(&reopened, "card").revision, 9);
    reopened.finish().unwrap();
}

#[test]
fn owner_reads_existing_v2_after_disable_or_missing_plugin_without_allowing_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    host.create("seed-card", idea("card")).unwrap();
    host.create("seed-legacy", idea("legacy")).unwrap();
    let plan = host.plan_tasks_migration("card").unwrap();
    host.migrate_tasks(&plan.operation, "card", plan.source_revision)
        .unwrap();
    let id = tasks(&host, "card").properties.tasks[0].id.clone();
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(!host.writable());
    assert_eq!(tasks(&host, "card").revision, 2);
    assert!(matches!(
        host.read_versioned("legacy").unwrap(),
        VersionedRecord::Legacy(_)
    ));
    let (page, _) = host.page_versioned("", 10).unwrap();
    assert_eq!(page.len(), 2);
    assert!(
        page.iter()
            .any(|item| matches!(item, VersionedRecord::Legacy(_)))
    );
    assert!(
        page.iter()
            .any(|item| matches!(item, VersionedRecord::Tasks(_)))
    );
    assert!(
        host.edit_tasks(
            "disabled-edit",
            "card",
            2,
            &Command::SetCompletion {
                id: id.clone(),
                complete: true
            }
        )
        .is_err()
    );
    assert!(host.plan_tasks_migration("card").is_err());
    host.finish().unwrap();
    drop(host);
    let mut no_bundle = Workbench::open(&path, None).unwrap();
    assert!(!no_bundle.writable());
    assert_eq!(tasks(&no_bundle, "card").revision, 2);
    assert!(matches!(
        no_bundle.read_versioned("legacy").unwrap(),
        VersionedRecord::Legacy(_)
    ));
    assert_eq!(no_bundle.page_versioned("", 10).unwrap().0.len(), 2);
    assert!(
        no_bundle
            .edit_tasks(
                "missing-plugin-edit",
                "card",
                2,
                &Command::SetCompletion { id, complete: true }
            )
            .is_err()
    );
    no_bundle.finish().unwrap();
}

#[test]
fn owner_plan_is_fixed_to_the_exact_legacy_baseline() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    host.create("seed-plan", idea("planned")).unwrap();

    let original = host.plan_tasks_migration("planned").unwrap();
    let repeated_plan = host.plan_tasks_migration("planned").unwrap();
    assert_eq!(original.operation, repeated_plan.operation);
    assert_eq!(original.source_revision, repeated_plan.source_revision);

    let changed = host
        .apply(Mutation {
            operation: "change-v1-before-migration",
            id: "planned",
            revision: 1,
            action: Action::Favorite,
            proposed: None,
            text: "",
            flag: true,
        })
        .unwrap();
    assert_eq!(changed.revision, 2);
    assert!(changed.idea.favorite);
    assert!(
        host.migrate_tasks(&original.operation, "planned", 1)
            .is_err()
    );
    assert!(
        host.migrate_tasks(&original.operation, "planned", 2)
            .is_err()
    );
    assert_eq!(host.read("planned").unwrap().revision, 2);

    let current = host.plan_tasks_migration("planned").unwrap();
    assert_eq!(current.source_revision, 2);
    assert_ne!(current.operation, original.operation);
    let migrated = host
        .migrate_tasks(&current.operation, "planned", 2)
        .unwrap();
    assert_eq!(migrated.receipt.revision, 3);
    assert!(migrated.committed.properties.favorite);
    assert!(host.plan_tasks_migration("planned").is_err());
    assert!(
        host.migrate_tasks(&original.operation, "planned", 1)
            .is_err()
    );
    assert_eq!(tasks(&host, "planned").revision, 3);
    host.finish().unwrap();
}

#[test]
fn owner_migrated_attachment_matches_outer_card_and_remains_exportable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let bytes = b"owner attachment \0\xff";
    let asset = host
        .import(
            "asset-card",
            "notes.txt",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut draft = idea("asset-card");
    draft.assets.push(asset.clone());
    host.create("seed-asset", draft).unwrap();

    let plan = host.plan_tasks_migration("asset-card").unwrap();
    let migrated = host
        .migrate_tasks(&plan.operation, "asset-card", plan.source_revision)
        .unwrap();
    assert_eq!(migrated.committed.properties.assets.len(), 1);
    let inner = &migrated.committed.properties.assets[0];
    assert_eq!(
        (&inner.id, &inner.name, &inner.kind, inner.bytes),
        (&asset.id, &asset.name, &asset.kind, asset.bytes)
    );
    let current = tasks(&host, "asset-card");
    assert_eq!(
        current.properties.assets,
        migrated.committed.properties.assets
    );
    let mut exported = Vec::new();
    host.export("asset-card", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, bytes);

    let task_id = current.properties.tasks[0].id.clone();
    let edit = host
        .edit_tasks(
            "asset-task-edit",
            "asset-card",
            current.revision,
            &Command::SetCompletion {
                id: task_id,
                complete: true,
            },
        )
        .unwrap();
    assert_eq!(edit.committed.properties.assets, current.properties.assets);
    exported.clear();
    host.export("asset-card", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, bytes);
    host.finish().unwrap();
    drop(host);

    let mut readonly = Workbench::open(&path, None).unwrap();
    assert_eq!(tasks(&readonly, "asset-card").properties.assets.len(), 1);
    exported.clear();
    readonly
        .export("asset-card", &asset.id, &mut exported)
        .unwrap();
    assert_eq!(exported, bytes);
    readonly.finish().unwrap();
}
