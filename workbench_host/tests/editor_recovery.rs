#![cfg(target_os = "windows")]
use capnp::{message::Builder, serialize};
use morrow_core::plugin_package::Package;
use morrow_core::plugin_package::proto::{Capability, TransformHandler};
use morrow_workbench_host::{
    Workbench,
    capture_provenance::{EditorSnapshot, PasteEvent, PastePart},
    cards_content::CardAction,
    versioned_record::{TaskRecord, VersionedRecord},
};
use morrow_workbench_plugin::{
    Asset, Idea, PACKAGE_VERSION, capture, capture_capnp, cards_v2::Fields,
};
use std::{path::Path, process::Command};

const OPERATION: &str = "recover-captured-card";
const PAYLOAD: &[u8] = b"pinned across native host restart\x00\xff";

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.editor-recovery",
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

fn idea() -> Idea {
    Idea {
        id: "card".into(),
        title: "Task card".into(),
        description: "old body".into(),
        category: "进行中".into(),
        stage: "计划中".into(),
        todos: vec!["first".into()],
        ..Default::default()
    }
}

fn record(host: &Workbench) -> TaskRecord {
    match host.read_versioned("card").unwrap() {
        VersionedRecord::Tasks(record) => record,
        VersionedRecord::Legacy(_) => panic!("expected migrated V2 card"),
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

fn snapshot(fields: &Fields) -> EditorSnapshot {
    EditorSnapshot {
        title: fields.title.clone(),
        description: fields.description.clone(),
        hypothesis: fields.hypothesis.clone(),
        conclusion: fields.conclusion.clone(),
        todos: String::new(),
        aliases: vec![],
    }
}

fn capture_request(source: &str) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<capture_capnp::request::Builder>();
    request.set_version(1);
    request.set_digest(&capture::digest());
    request.set_format("plain");
    request.set_source(source);
    serialize::write_message_to_words(&message)
}

fn paste(ticket: &str) -> PasteEvent {
    PasteEvent {
        id: "paste-description".into(),
        field: "description".into(),
        before: "old body".into(),
        start_utf16: 0,
        end_utf16: "old body".encode_utf16().count() as u32,
        parts: vec![PastePart {
            ticket: ticket.into(),
            literal: String::new(),
            selection: "outputMarkdown".into(),
        }],
        after: "captured".into(),
    }
}

fn drop_blocker(path: &Path) {
    rusqlite::Connection::open(path)
        .unwrap()
        .execute_batch("DROP TRIGGER review_editor_commit_blocker;")
        .unwrap();
}

fn spawn_failed_save(path: &Path) -> (String, String) {
    let executable = std::env::current_exe().unwrap();
    let mut command = Command::new(executable);
    command
        .arg("--exact")
        .arg("editor_recovery_child_prepare")
        .arg("--nocapture")
        .env("MORROW_EDITOR_RECOVERY_DB", path);
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    let result = command.output().unwrap();
    assert!(
        result.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
    let info = std::fs::read_to_string(path.with_extension("info")).unwrap();
    let mut parts = info.lines();
    (parts.next().unwrap().into(), parts.next().unwrap().into())
}

#[test]
fn editor_recovery_child_prepare() {
    let Ok(path) = std::env::var("MORROW_EDITOR_RECOVERY_DB") else {
        return;
    };
    let path = Path::new(&path);
    let mut host = Workbench::open(path, Some(package())).unwrap();
    host.create("seed", idea()).unwrap();
    let migration = host.plan_tasks_migration("card").unwrap();
    host.migrate_tasks(&migration.operation, "card", migration.source_revision)
        .unwrap();
    assert_eq!(record(&host).revision, 2);
    let asset = host
        .import(
            "card",
            "pin.bin",
            "file",
            &mut &PAYLOAD[..],
            PAYLOAD.len() as u64,
        )
        .unwrap();
    let scope = host.open_capture_scope("card", 2).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("captured"), "")
        .unwrap();
    host.record_paste(&scope, paste(&ticket)).unwrap();
    let mut edit = fields(&record(&host));
    edit.title = "Recovered card".into();
    edit.description = "captured".into();
    edit.assets.push(asset.clone());
    let sql = rusqlite::Connection::open(path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER review_editor_commit_blocker BEFORE UPDATE OF payload ON cards
         WHEN OLD.id='card' BEGIN SELECT RAISE(ABORT,'business card commit blocked'); END;",
    )
    .unwrap();
    let failure = host
        .edit_card_captured(OPERATION, "card", 2, &edit, &scope, snapshot(&edit))
        .err()
        .expect("real card commit should fail after durable proposal");
    assert!(failure.to_string().contains("Storage"), "{failure}");
    let recovery = host.editor_recovery("card").unwrap().unwrap();
    assert!(recovery.active);
    assert_eq!(recovery.operation_id, OPERATION);
    assert_eq!(record(&host).revision, 2);
    assert!(host.operation_evidence("card", OPERATION).is_err());
    std::fs::write(
        path.with_extension("info"),
        format!("{scope}\n{}\n", asset.id),
    )
    .unwrap();
    // Exiting this process discards every live capture scope and staged asset.
}

#[test]
fn process_restart_resumes_exact_proposal_and_pinned_attachment_once() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let (scope, asset_id) = spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let recovery = host.editor_recovery("card").unwrap().unwrap();
    let digest = recovery.evidence_digest;
    assert_eq!(recovery.operation_id, OPERATION);
    assert_eq!(recovery.source_revision, 2);
    assert!(recovery.active);
    assert_eq!(record(&host).revision, 2);
    assert!(host.operation_evidence("card", OPERATION).is_err());
    assert!(
        host.acknowledge_editor_recovery("card", OPERATION, &digest)
            .is_err()
    );
    let wrong = [0u8; 32];
    assert!(
        host.resume_editor_recovery("card", OPERATION, &wrong)
            .is_err()
    );
    assert!(
        host.resume_editor_recovery("card", "new-operation", &digest)
            .is_err()
    );
    let mut raw_edit = fields(&record(&host));
    raw_edit.title = "Recovered card".into();
    raw_edit.description = "captured".into();
    raw_edit.assets.push(Asset {
        id: asset_id.clone(),
        name: "pin.bin".into(),
        kind: "file".into(),
        bytes: PAYLOAD.len() as u64,
    });
    let late = host
        .edit_card_captured(
            "new-operation",
            "card",
            2,
            &raw_edit,
            &scope,
            snapshot(&raw_edit),
        )
        .err()
        .expect("active original must reject a new capture");
    assert!(
        late.to_string()
            .contains("unresolved captured editor proposal")
    );
    raw_edit.description.push('!');
    let changed = host
        .edit_card_captured(OPERATION, "card", 2, &raw_edit, &scope, snapshot(&raw_edit))
        .err()
        .expect("original operation cannot change its saved request");
    assert!(changed.to_string().contains("durable proposal"));
    raw_edit.description.pop();
    drop_blocker(&path);
    let saved = host
        .resume_editor_recovery("card", OPERATION, &digest)
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.committed.revision, 3);
    assert_eq!(record(&host).revision, 3);
    assert_eq!(record(&host).properties.assets[0].id, asset_id);
    let mut exported = Vec::new();
    host.export("card", &asset_id, &mut exported).unwrap();
    assert_eq!(exported, PAYLOAD);
    let evidence = host.operation_evidence("card", OPERATION).unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(evidence[0].digest(), digest);
    let repeated = host
        .resume_editor_recovery("card", OPERATION, &digest)
        .unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.receipt, saved.receipt);
    let historical = host
        .edit_card_captured(OPERATION, "card", 2, &raw_edit, &scope, snapshot(&raw_edit))
        .unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.receipt, saved.receipt);
    assert_eq!(record(&host).revision, 3);
    assert!(
        host.acknowledge_editor_recovery("card", OPERATION, &wrong)
            .is_err()
    );
    let evidence_blob = recovery_evidence_blob(&path);
    host.acknowledge_editor_recovery("card", OPERATION, &digest)
        .unwrap();
    assert!(!host.editor_recovery("card").unwrap().unwrap().active);
    // Acknowledgement releases the active slot, while historical events still
    // retain their original evidence blob.
    assert!(historical_pin_count(&path, &evidence_blob) > 0);
    assert!(host.list_editor_recoveries().unwrap().is_empty());
}

#[test]
fn process_restart_rejects_changed_source_without_discarding_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let recovery = host.editor_recovery("card").unwrap().unwrap();
    drop_blocker(&path);
    host.edit_card("advance-source", "card", 2, &CardAction::SetFavorite(true))
        .unwrap();
    assert_eq!(record(&host).revision, 3);
    assert!(
        host.resume_editor_recovery("card", OPERATION, &recovery.evidence_digest)
            .is_err()
    );
    assert_eq!(record(&host).revision, 3);
    assert!(host.operation_evidence("card", OPERATION).is_err());
    let still_pending = host.editor_recovery("card").unwrap().unwrap();
    assert!(still_pending.active);
    assert_eq!(still_pending.evidence_digest, recovery.evidence_digest);
}

#[test]
fn disabled_package_cannot_resume_persisted_editor_proposal() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let recovery = host.editor_recovery("card").unwrap().unwrap();
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    drop_blocker(&path);
    assert!(
        host.resume_editor_recovery("card", OPERATION, &recovery.evidence_digest)
            .is_err()
    );
    assert_eq!(record(&host).revision, 2);
    assert!(host.operation_evidence("card", OPERATION).is_err());
    assert_eq!(
        host.editor_recovery("card")
            .unwrap()
            .unwrap()
            .evidence_digest,
        recovery.evidence_digest
    );
}

fn recovery_evidence_blob(path: &Path) -> String {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    sql.query_row(
        "SELECT blob_id FROM card_blobs
         WHERE card_id LIKE 'morrow-host-editor-recovery-%'
           AND attachment_id='morrow-host-recovery-evidence'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

fn historical_pin_count(path: &Path, blob_id: &str) -> i64 {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    sql.query_row(
        "SELECT COUNT(*) FROM event_blobs WHERE blob_id=?1",
        [blob_id],
        |row| row.get(0),
    )
    .unwrap()
}

fn operation_rows(path: &Path, operation: &str) -> i64 {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    sql.query_row(
        "SELECT COUNT(*) FROM operations WHERE id=?1",
        [operation],
        |row| row.get(0),
    )
    .unwrap()
}

#[test]
fn conflicted_original_can_be_abandoned_exactly_and_never_reused() {
    use morrow_workbench_host::editor_recovery::EditorRecoveryStatus;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let (old_scope, _) = spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let original = host.editor_recovery("card").unwrap().unwrap();
    drop_blocker(&path);
    host.edit_card("advance-source", "card", 2, &CardAction::SetFavorite(true))
        .unwrap();
    assert_eq!(
        host.editor_recovery("card").unwrap().unwrap().status,
        EditorRecoveryStatus::Conflict
    );
    assert_eq!(operation_rows(&path, OPERATION), 0);
    assert!(
        host.abandon_editor_recovery("card", OPERATION, &[0; 32])
            .is_err()
    );
    assert!(
        host.abandon_editor_recovery("card", "wrong-operation", &original.evidence_digest)
            .is_err()
    );
    assert!(host.editor_recovery("card").unwrap().unwrap().active);
    let evidence_blob = recovery_evidence_blob(&path);
    host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
    assert_eq!(record(&host).revision, 3);
    // The active pin is cleared, but the original journal event keeps history.
    assert!(historical_pin_count(&path, &evidence_blob) > 0);
    assert!(record(&host).properties.favorite);
    assert_eq!(operation_rows(&path, OPERATION), 1);
    assert!(!host.editor_recovery("card").unwrap().unwrap().active);
    assert!(host.list_editor_recoveries().unwrap().is_empty());
    assert!(
        host.acknowledge_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    assert!(
        host.resume_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
    assert_eq!(record(&host).revision, 3);
    let scope = host.open_capture_scope("card", 3).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("captured"), "")
        .unwrap();
    host.record_paste(&scope, paste(&ticket)).unwrap();
    let mut replacement = fields(&record(&host));
    replacement.title = "Replacement after abandon".into();
    replacement.description = "captured".into();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER review_editor_commit_blocker BEFORE UPDATE OF payload ON cards
         WHEN OLD.id='card' BEGIN SELECT RAISE(ABORT,'business card commit blocked'); END;",
    )
    .unwrap();
    const NEW_OPERATION: &str = "captured-after-abandon";
    let failure = host
        .edit_card_captured(
            NEW_OPERATION,
            "card",
            3,
            &replacement,
            &scope,
            snapshot(&replacement),
        )
        .err()
        .expect("replacement proposal should persist before business failure");
    assert!(failure.to_string().contains("Storage"), "{failure}");
    let new_recovery = host.editor_recovery("card").unwrap().unwrap();
    assert!(new_recovery.active);
    assert_eq!(new_recovery.operation_id, NEW_OPERATION);
    assert_ne!(new_recovery.evidence_digest, original.evidence_digest);
    assert!(
        host.edit_card_captured(
            OPERATION,
            "card",
            2,
            &replacement,
            &old_scope,
            snapshot(&replacement),
        )
        .is_err()
    );
    assert!(
        host.resume_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    assert!(
        host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    assert_eq!(
        host.editor_recovery("card")
            .unwrap()
            .unwrap()
            .evidence_digest,
        new_recovery.evidence_digest
    );
    drop_blocker(&path);
    let saved = host
        .resume_editor_recovery("card", NEW_OPERATION, &new_recovery.evidence_digest)
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.committed.revision, 4);
    assert_eq!(record(&host).revision, 4);
    assert_eq!(operation_rows(&path, OPERATION), 1);
    host.acknowledge_editor_recovery("card", NEW_OPERATION, &new_recovery.evidence_digest)
        .unwrap();
    assert!(host.list_editor_recoveries().unwrap().is_empty());

    // Use a fresh valid scope and current source. The abandoned operation is
    // refused by the global operation guard before any new guest invocation.
    let fresh_scope = host.open_capture_scope("card", 4).unwrap();
    let mut current_edit = fields(&record(&host));
    current_edit.title = "Must not reuse abandoned operation".into();
    let refused = host
        .edit_card_captured(
            OPERATION,
            "card",
            4,
            &current_edit,
            &fresh_scope,
            snapshot(&current_edit),
        )
        .err()
        .expect("abandoned operation must remain sealed");
    assert_eq!(
        refused.to_string(),
        "captured editor operation already used"
    );
    assert_eq!(record(&host).revision, 4);
    assert!(host.list_editor_recoveries().unwrap().is_empty());
}

#[test]
fn failed_abandon_journal_update_rolls_back_original_tombstone() {
    use morrow_workbench_host::editor_recovery::EditorRecoveryStatus;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let original = host.editor_recovery("card").unwrap().unwrap();
    drop_blocker(&path);
    host.edit_card("advance-source", "card", 2, &CardAction::SetFavorite(true))
        .unwrap();
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER review_abandon_failure BEFORE UPDATE OF payload ON cards
         WHEN OLD.id LIKE 'morrow-host-editor-recovery-%'
         BEGIN SELECT RAISE(ABORT,'journal edit blocked'); END;",
    )
    .unwrap();
    assert!(
        host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    assert_eq!(operation_rows(&path, OPERATION), 0);
    assert_eq!(record(&host).revision, 3);
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let retained = host.editor_recovery("card").unwrap().unwrap();
    assert!(retained.active);
    assert_eq!(retained.status, EditorRecoveryStatus::Conflict);
    assert_eq!(retained.evidence_digest, original.evidence_digest);
    sql.execute_batch("DROP TRIGGER review_abandon_failure;")
        .unwrap();
    host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
    assert_eq!(operation_rows(&path, OPERATION), 1);
    assert_eq!(record(&host).revision, 3);
}

#[test]
fn committed_original_cannot_be_abandoned() {
    use morrow_workbench_host::editor_recovery::EditorRecoveryStatus;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let original = host.editor_recovery("card").unwrap().unwrap();
    drop_blocker(&path);
    let saved = host
        .resume_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(record(&host).revision, 3);
    assert!(
        host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    let retained = host.editor_recovery("card").unwrap().unwrap();
    assert!(retained.active);
    assert_eq!(retained.status, EditorRecoveryStatus::Committed);
    assert_eq!(retained.evidence_digest, original.evidence_digest);
    assert_eq!(record(&host).revision, 3);
    host.acknowledge_editor_recovery("card", OPERATION, &original.evidence_digest)
        .unwrap();
}

#[test]
fn foreign_used_original_operation_cannot_be_abandoned() {
    use morrow_workbench_host::editor_recovery::EditorRecoveryStatus;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let original = host.editor_recovery("card").unwrap().unwrap();
    let mut foreign = idea();
    foreign.id = "foreign-card".into();
    host.create(OPERATION, foreign).unwrap();
    assert_eq!(
        host.editor_recovery("card").unwrap().unwrap().status,
        EditorRecoveryStatus::Conflict
    );
    assert!(
        host.abandon_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    let retained = host.editor_recovery("card").unwrap().unwrap();
    assert!(retained.active);
    assert_eq!(retained.evidence_digest, original.evidence_digest);
    assert_eq!(record(&host).revision, 2);
    assert_eq!(operation_rows(&path, OPERATION), 1);
}

#[test]
fn corrupt_saved_evidence_cannot_commit_or_clear_active_original() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    spawn_failed_save(&path);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let original = host.editor_recovery("card").unwrap().unwrap();
    let evidence_blob = recovery_evidence_blob(&path);
    drop_blocker(&path);
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        sql.execute(
            "UPDATE blobs SET payload=x'00' WHERE id=?1",
            [&evidence_blob],
        )
        .unwrap(),
        1
    );
    assert!(
        host.resume_editor_recovery("card", OPERATION, &original.evidence_digest)
            .is_err()
    );
    assert_eq!(record(&host).revision, 2);
    assert_eq!(operation_rows(&path, OPERATION), 0);
    let retained = host.editor_recovery("card").unwrap().unwrap();
    assert!(retained.active);
    assert_eq!(retained.evidence_digest, original.evidence_digest);
}
