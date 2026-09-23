#![cfg(target_os = "windows")]

use morrow_core::plugin_package::{
    Package,
    proto::{Capability, TransformHandler},
};
use morrow_workbench_host::{
    Mutation, Workbench,
    cards_content::CardAction,
    versioned_record::{TaskRecord, VersionedRecord},
};
use morrow_workbench_plugin::{
    Action, Asset, Idea, PACKAGE_VERSION, cards_v2::Fields, tasks_v2::Command as TaskCommand,
};

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

fn idea(id: &str) -> Idea {
    Idea {
        id: id.into(),
        title: format!("Card {id}"),
        description: "original **body**".into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["same".into(), "same".into()],
        completed: vec!["same".into()],
        ..Default::default()
    }
}

fn task_record(host: &Workbench, id: &str) -> TaskRecord {
    match host.read_versioned(id).unwrap() {
        VersionedRecord::Tasks(record) => record,
        VersionedRecord::Legacy(_) => panic!("expected migrated format 2"),
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    host: Workbench,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut host = Workbench::open(&dir.path().join("db"), Some(package())).unwrap();
        assert!(host.writable());
        assert_eq!(host.create("seed-card", idea("card")).unwrap().revision, 1);
        let migration = host.plan_tasks_migration("card").unwrap();
        let committed = host
            .migrate_tasks(&migration.operation, "card", migration.source_revision)
            .unwrap();
        assert_eq!(committed.committed.revision, 2);
        assert_eq!(
            host.operation_evidence("card", &migration.operation)
                .unwrap()
                .len(),
            1
        );
        Self { dir, host }
    }
    fn record(&self) -> TaskRecord {
        task_record(&self.host, "card")
    }
}

fn fields(record: &TaskRecord) -> Fields {
    let p = &record.properties;
    Fields {
        title: record.title.clone(),
        description: p.description.clone(),
        hypothesis: p.hypothesis.clone(),
        conclusion: p.conclusion.clone(),
        icon: p.icon as u16,
        color: p.color,
        assets: p
            .assets
            .iter()
            .map(|a| Asset {
                id: a.id.clone(),
                name: a.name.clone(),
                kind: a.kind.clone(),
                bytes: a.bytes,
            })
            .collect(),
    }
}

fn assert_task_identity(before: &TaskRecord, after: &TaskRecord) {
    assert_eq!(after.properties.tasks, before.properties.tasks);
    assert_eq!(
        after.properties.retired_task_ids,
        before.properties.retired_task_ids
    );
    assert_eq!(after.properties.origin, before.properties.origin);
}

#[test]
fn real_owner_common_edits_keep_task_identity_and_history_exact() {
    let mut f = Fixture::new();
    let before = f.record();
    let mut content = fields(&before);
    content.title = "Renamed card".into();
    content.description = "new **body**".into();
    content.hypothesis = "testable hypothesis".into();
    content.conclusion = "clear conclusion".into();
    content.icon = 3;
    content.color = 0x123456;
    let edit = CardAction::Edit(content.clone());
    let edited = f.host.edit_card("card-edit", "card", 2, &edit).unwrap();
    assert!(!edited.repeated);
    assert_eq!(edited.committed.revision, 3);
    assert_eq!(
        f.host
            .operation_evidence("card", "card-edit")
            .unwrap()
            .len(),
        1
    );
    let after_edit = f.record();
    assert_task_identity(&before, &after_edit);
    assert_eq!(after_edit.title, content.title);
    assert_eq!(after_edit.properties.description, content.description);
    assert_eq!(after_edit.properties.hypothesis, content.hypothesis);
    assert_eq!(after_edit.properties.conclusion, content.conclusion);
    assert_eq!(after_edit.properties.icon, u32::from(content.icon));
    assert_eq!(after_edit.properties.color, content.color);

    let favorite = f
        .host
        .edit_card("card-favorite", "card", 3, &CardAction::SetFavorite(true))
        .unwrap();
    assert_eq!(favorite.committed.revision, 4);
    let after_favorite = f.record();
    assert_task_identity(&before, &after_favorite);
    assert!(after_favorite.properties.favorite);
    assert_eq!(after_favorite.projection.stage, "计划中");

    let category = f
        .host
        .edit_card(
            "card-category",
            "card",
            4,
            &CardAction::SetCategory {
                category: "灵感".into(),
                stage: "待整理".into(),
            },
        )
        .unwrap();
    assert_eq!(category.committed.revision, 5);
    let latest = f.record();
    assert_task_identity(&before, &latest);
    assert_eq!(latest.properties.category, "灵感");
    assert_eq!(latest.projection.stage, "待整理");
    assert_eq!(latest.projection.ambiguous, before.projection.ambiguous);

    assert!(
        f.host
            .apply(Mutation {
                operation: "legacy-edit-on-v2",
                id: "card",
                revision: 5,
                action: Action::Favorite,
                proposed: None,
                text: "",
                flag: true,
            })
            .is_err()
    );
    assert!(
        f.host
            .edit_card("card-edit", "card", 2, &CardAction::SetFavorite(true))
            .is_err()
    );
    let mut changed = content.clone();
    changed.description.push_str(" altered");
    assert!(
        f.host
            .edit_card("card-edit", "card", 2, &CardAction::Edit(changed))
            .is_err()
    );
    assert!(f.host.edit_card("card-edit", "card", 3, &edit).is_err());
    assert!(f.host.edit_card("new-stale", "card", 2, &edit).is_err());
    let historical = f.host.edit_card("card-edit", "card", 2, &edit).unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.receipt, edited.receipt);
    assert_eq!(historical.committed.revision, 3);
    assert_eq!(f.record().revision, 5);
    f.host.finish().unwrap();
    drop(f.host);
    let mut reopened = Workbench::open(&f.dir.path().join("db"), Some(package())).unwrap();
    let retry = reopened.edit_card("card-edit", "card", 2, &edit).unwrap();
    assert!(retry.repeated);
    assert_eq!(retry.receipt, edited.receipt);
    assert_eq!(task_record(&reopened, "card").revision, 5);
    reopened.finish().unwrap();
}

#[test]
fn real_owner_staged_assets_require_this_card_and_export_exact_bytes() {
    let mut f = Fixture::new();
    let bytes = b"real owner attachment\0\xff";
    let asset = f
        .host
        .import(
            "card",
            "note.bin",
            "file",
            &mut &bytes[..],
            bytes.len() as u64,
        )
        .unwrap();
    let mut content = fields(&f.record());
    content.assets.push(asset.clone());
    let add = CardAction::Edit(content.clone());
    let attached = f.host.edit_card("attach", "card", 2, &add).unwrap();
    assert_eq!(attached.committed.revision, 3);
    let current = f.record();
    assert_eq!(current.properties.assets.len(), 1);
    assert_eq!(current.properties.assets[0].id, asset.id);
    let mut exported = Vec::new();
    f.host.export("card", &asset.id, &mut exported).unwrap();
    assert_eq!(exported, bytes);

    let foreign = f
        .host
        .import("other", "foreign.bin", "file", &mut &b"other"[..], 5)
        .unwrap();
    let mut unauthorized = fields(&current);
    unauthorized.assets.push(foreign);
    assert!(
        f.host
            .edit_card("foreign-asset", "card", 3, &CardAction::Edit(unauthorized))
            .is_err()
    );
    let mut invented = fields(&current);
    invented.assets.push(Asset {
        id: "not-staged".into(),
        name: "fake".into(),
        kind: "file".into(),
        bytes: 5,
    });
    assert!(
        f.host
            .edit_card("invented-asset", "card", 3, &CardAction::Edit(invented))
            .is_err()
    );
    assert_eq!(f.record().revision, 3);

    let mut removed = fields(&current);
    removed.assets.clear();
    f.host
        .edit_card("detach", "card", 3, &CardAction::Edit(removed))
        .unwrap();
    assert!(f.record().properties.assets.is_empty());
    assert!(f.host.export("card", &asset.id, &mut Vec::new()).is_err());
    let retry = f.host.edit_card("attach", "card", 2, &add).unwrap();
    assert!(retry.repeated);
    assert_eq!(retry.receipt, attached.receipt);
    assert!(f.record().properties.assets.is_empty());

    // A failed new edit and an unrelated historical receipt must both leave a
    // newly selected attachment available for the user's next real edit.
    let pending_bytes = b"selected after detach";
    let pending = f
        .host
        .import(
            "card",
            "pending.bin",
            "file",
            &mut &pending_bytes[..],
            pending_bytes.len() as u64,
        )
        .unwrap();
    let mut invalid = fields(&f.record());
    invalid.assets.push(pending.clone());
    invalid.assets.push(Asset {
        id: "not-selected".into(),
        name: "unselected.bin".into(),
        kind: "file".into(),
        bytes: 1,
    });
    assert!(
        f.host
            .edit_card("failed-with-pending", "card", 4, &CardAction::Edit(invalid))
            .is_err()
    );
    assert_eq!(f.record().revision, 4);
    let old = f.host.edit_card("attach", "card", 2, &add).unwrap();
    assert!(old.repeated);
    assert_eq!(old.receipt, attached.receipt);
    assert!(f.record().properties.assets.is_empty());
    let mut selected = fields(&f.record());
    selected.assets.push(pending.clone());
    let committed = f
        .host
        .edit_card("attach-pending", "card", 4, &CardAction::Edit(selected))
        .unwrap();
    assert_eq!(committed.committed.revision, 5);
    let mut pending_export = Vec::new();
    f.host
        .export("card", &pending.id, &mut pending_export)
        .unwrap();
    assert_eq!(pending_export, pending_bytes);
    f.host.finish().unwrap();
}

#[test]
fn task_historical_retry_after_later_delete_returns_receipt_without_revival() {
    let mut f = Fixture::new();
    let task_id = f.record().properties.tasks[0].id.clone();
    let command = TaskCommand::SetCompletion {
        id: task_id,
        complete: true,
    };
    let edited = f
        .host
        .edit_tasks("task-before-delete", "card", 2, &command)
        .unwrap();
    assert_eq!(edited.committed.revision, 3);
    f.host
        .edit_card("delete-after-task", "card", 3, &CardAction::Delete)
        .unwrap();
    assert!(f.record().properties.deleted);
    assert_eq!(f.record().revision, 4);
    let historical = f
        .host
        .edit_tasks("task-before-delete", "card", 2, &command)
        .unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.receipt, edited.receipt);
    assert_eq!(historical.committed.revision, 3);
    assert!(f.record().properties.deleted);
    assert_eq!(f.record().revision, 4);

    f.host.finish().unwrap();
    drop(f.host);
    let mut reopened = Workbench::open(&f.dir.path().join("db"), Some(package())).unwrap();
    let again = reopened
        .edit_tasks("task-before-delete", "card", 2, &command)
        .unwrap();
    assert!(again.repeated);
    assert_eq!(again.receipt, edited.receipt);
    assert_eq!(task_record(&reopened, "card").revision, 4);
    assert!(task_record(&reopened, "card").properties.deleted);
    reopened.finish().unwrap();
}

#[test]
fn real_owner_delete_restore_is_ephemeral_but_historical_receipt_survives_restart() {
    let mut f = Fixture::new();
    let before = f.record();
    let first_id = before.properties.tasks[0].id.clone();
    let deleted = f
        .host
        .edit_card("delete-one", "card", 2, &CardAction::Delete)
        .unwrap();
    assert_eq!(deleted.committed.revision, 3);
    assert!(f.record().properties.deleted);
    assert_task_identity(&before, &f.record());
    assert!(
        f.host
            .edit_tasks(
                "edit-deleted",
                "card",
                3,
                &TaskCommand::SetCompletion {
                    id: first_id,
                    complete: true
                }
            )
            .is_err()
    );
    assert!(
        f.host
            .edit_card(
                "favorite-deleted",
                "card",
                3,
                &CardAction::SetFavorite(true)
            )
            .is_err()
    );
    assert!(
        f.host
            .edit_card(
                "edit-deleted-card",
                "card",
                3,
                &CardAction::Edit(fields(&before))
            )
            .is_err()
    );
    assert!(
        f.host
            .edit_card("restore-wrong-revision", "card", 2, &CardAction::Restore)
            .is_err()
    );

    let restored = f
        .host
        .edit_card("restore-one", "card", 3, &CardAction::Restore)
        .unwrap();
    assert_eq!(restored.committed.revision, 4);
    assert!(!f.record().properties.deleted);
    assert_task_identity(&before, &f.record());
    f.host
        .edit_card("delete-two", "card", 4, &CardAction::Delete)
        .unwrap();
    assert!(f.record().properties.deleted);
    std::thread::sleep(std::time::Duration::from_millis(8_050));
    assert!(
        f.host
            .edit_card("expired-restore", "card", 5, &CardAction::Restore)
            .is_err()
    );
    assert_eq!(f.record().revision, 5);
    f.host.finish().unwrap();
    drop(f.host);

    let mut reopened = Workbench::open(&f.dir.path().join("db"), Some(package())).unwrap();
    assert!(task_record(&reopened, "card").properties.deleted);
    assert!(
        reopened
            .edit_card("fresh-restore", "card", 5, &CardAction::Restore)
            .is_err()
    );
    let historical = reopened
        .edit_card("restore-one", "card", 3, &CardAction::Restore)
        .unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.receipt, restored.receipt);
    assert_eq!(historical.committed.revision, 4);
    assert!(task_record(&reopened, "card").properties.deleted);
    reopened.finish().unwrap();
}

#[test]
fn real_owner_v2_remains_readable_when_plugin_disabled_or_missing() {
    let mut f = Fixture::new();
    let status = f.host.plugin_status().unwrap();
    f.host
        .configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(!f.host.writable());
    assert_eq!(f.record().revision, 2);
    assert!(
        f.host
            .edit_card("disabled-card", "card", 2, &CardAction::SetFavorite(true))
            .is_err()
    );
    f.host.finish().unwrap();
    drop(f.host);
    let mut no_bundle = Workbench::open(&f.dir.path().join("db"), None).unwrap();
    assert!(!no_bundle.writable());
    assert_eq!(task_record(&no_bundle, "card").revision, 2);
    assert!(
        no_bundle
            .edit_card("missing-card", "card", 2, &CardAction::SetFavorite(true))
            .is_err()
    );
    no_bundle.finish().unwrap();
}
