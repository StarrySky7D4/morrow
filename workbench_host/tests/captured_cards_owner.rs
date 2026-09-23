#![cfg(target_os = "windows")]
use capnp::{message::Builder, serialize};
use morrow_core::{
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    task::Invocation,
    task_evidence::{self, Evidence},
    transaction,
};
use morrow_workbench_host::{
    Workbench,
    capture_provenance::{AttachmentAlias, EditorSnapshot, PasteEvent, PastePart},
    captured_cards,
    cards_content::CardAction,
    versioned_record::{TaskRecord, VersionedRecord},
};
use morrow_workbench_plugin::{
    Asset, Idea, PACKAGE_VERSION, capture, capture_capnp, cards_v2::Fields,
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Output},
};

fn package() -> Package {
    let module = std::fs::read(
        std::env::var("MORROW_WORKBENCH_WASM").expect("real compiled Rust guest path"),
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "test.captured-cards-owner",
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
        todos: vec!["same".into(), "same".into(), "open".into()],
        completed: vec!["same".into()],
        ..Default::default()
    }
}
fn record(host: &Workbench) -> TaskRecord {
    match host.read_versioned("card").unwrap() {
        VersionedRecord::Tasks(record) => record,
        VersionedRecord::Legacy(_) => panic!("expected migrated V2"),
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
fn paste(ticket: &str, before: &str, after: &str) -> PasteEvent {
    PasteEvent {
        id: "replace-description".into(),
        field: "description".into(),
        before: before.into(),
        start_utf16: 0,
        end_utf16: before.encode_utf16().count() as u32,
        parts: vec![PastePart {
            ticket: ticket.into(),
            literal: String::new(),
            selection: "outputMarkdown".into(),
        }],
        after: after.into(),
    }
}
fn seed(path: &std::path::Path) -> Workbench {
    let mut host = Workbench::open(path, Some(package())).unwrap();
    host.create("seed", idea()).unwrap();
    let migration = host.plan_tasks_migration("card").unwrap();
    host.migrate_tasks(&migration.operation, "card", migration.source_revision)
        .unwrap();
    assert_eq!(record(&host).revision, 2);
    host
}

fn commit_bytes(path: &Path, operation: &str) -> Vec<u8> {
    let sql =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    sql.query_row(
        "SELECT payload FROM operations WHERE id=?1",
        [operation],
        |row| row.get(0),
    )
    .unwrap()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn verify_cli(directory: &Path, commit: &[u8], evidence: &Evidence, wrong_pin: bool) -> Output {
    let commit_path = directory.join("captured-commit.bin");
    let evidence_path = directory.join("captured-evidence.bin");
    std::fs::write(&commit_path, commit).unwrap();
    std::fs::write(&evidence_path, evidence.container()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-content-replay"));
    command
        .arg(commit_path)
        .arg(if wrong_pin {
            "0".repeat(64)
        } else {
            hex(&Sha256::digest(commit))
        })
        .arg(evidence_path)
        .arg(hex(&evidence.digest()));
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}
#[test]
fn captured_v2_edit_commits_actual_conversion_and_card_guest_with_historical_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = seed(&path);
    let before = record(&host);
    let scope = host.open_capture_scope("card", 2).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("captured"), "")
        .unwrap();
    host.record_paste(
        &scope,
        paste(&ticket, &before.properties.description, "captured"),
    )
    .unwrap();
    let mut edit = fields(&before);
    edit.description = "captured and rewritten".into();
    edit.title = "Renamed task card".into();
    let endpoint = snapshot(&edit);
    let saved = host
        .edit_card_captured("captured-edit", "card", 2, &edit, &scope, endpoint.clone())
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.committed.revision, 3);
    assert_eq!(saved.committed.title, edit.title);
    assert_eq!(saved.committed.properties.description, edit.description);
    assert_eq!(saved.committed.properties.tasks, before.properties.tasks);
    assert_eq!(saved.committed.properties.origin, before.properties.origin);
    let mut evidence = host.operation_evidence("card", "captured-edit").unwrap();
    assert_eq!(evidence.len(), 1);
    let evidence = evidence.remove(0);
    let batch = evidence.data().batch.as_ref().unwrap();
    assert_eq!(batch.intent_type, captured_cards::INTENT_TYPE);
    assert_eq!(batch.observations.len(), 2);
    let handlers: Vec<_> = batch
        .observations
        .iter()
        .map(|o| {
            Invocation::decode(&o.invocation)
                .unwrap()
                .transform()
                .unwrap()
                .handler
                .clone()
        })
        .collect();
    assert_eq!(handlers, ["capture.convert", "workbench.cards.v2"]);

    host.edit_card("later-favorite", "card", 3, &CardAction::SetFavorite(true))
        .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let repeated = host
        .edit_card_captured("captured-edit", "card", 2, &edit, &scope, endpoint.clone())
        .unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.receipt, saved.receipt);
    assert_eq!(repeated.committed.revision, 3);
    assert_eq!(record(&host).revision, 4);
    assert!(record(&host).properties.favorite);
    assert_eq!(
        host.operation_evidence("card", "captured-edit").unwrap()[0].container(),
        evidence.container()
    );

    let mut changed = edit.clone();
    changed.description.push('!');
    assert!(
        host.edit_card_captured(
            "captured-edit",
            "card",
            2,
            &changed,
            &scope,
            snapshot(&changed)
        )
        .is_err()
    );
    assert!(
        host.edit_card_captured("captured-edit", "card", 3, &edit, &scope, endpoint.clone())
            .is_err()
    );
    assert!(
        host.edit_card_captured(
            "captured-edit",
            "card",
            2,
            &edit,
            "wrong-scope",
            endpoint.clone()
        )
        .is_err()
    );
    let mut changed_endpoint = endpoint.clone();
    changed_endpoint.title.push('!');
    assert!(
        host.edit_card_captured("captured-edit", "card", 2, &edit, &scope, changed_endpoint)
            .is_err()
    );
    let status = host.plugin_status().unwrap();
    host.configure_plugin(status.revision, &status.digest, false)
        .unwrap();
    assert!(host.read_versioned("card").is_ok());
    assert!(
        host.edit_card_captured("captured-edit", "card", 2, &edit, &scope, endpoint.clone())
            .is_err()
    );
    drop(host);
    let mut without_package = Workbench::open(&path, None).unwrap();
    assert!(without_package.read_versioned("card").is_ok());
    assert!(
        without_package
            .edit_card_captured("captured-edit", "card", 2, &edit, &scope, endpoint)
            .is_err()
    );
    drop(without_package);

    let raw = commit_bytes(&path, "captured-edit");
    let verify = tempfile::tempdir().unwrap();
    let matched = verify_cli(verify.path(), &raw, &evidence, false);
    assert!(
        matched.status.success(),
        "{}",
        String::from_utf8_lossy(&matched.stderr)
    );
    assert!(String::from_utf8_lossy(&matched.stdout).starts_with("MATCH:"));
    let wrong_pin = verify_cli(verify.path(), &raw, &evidence, true);
    assert_eq!(wrong_pin.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&wrong_pin.stderr).contains("integrity pin mismatch"));

    let mut altered = evidence.data().clone();
    let observed = &mut altered.batch.as_mut().unwrap().observations[0];
    assert!(observed.fuel_remaining > 0);
    observed.fuel_remaining -= 1;
    let forged = task_evidence::encode(altered).unwrap();
    let projected = captured_cards::derive(&forged).unwrap();
    let self_consistent = transaction::encode_commit_with_evidence(
        projected.command().to_vec(),
        projected.card(),
        &[forged.digest()],
    )
    .unwrap();
    captured_cards::verify_commit(
        &transaction::decode_commit(&self_consistent).unwrap().0,
        &forged,
    )
    .unwrap();
    let mismatch = verify_cli(verify.path(), &self_consistent, &forged, false);
    assert_eq!(mismatch.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&mismatch.stdout).starts_with("MISMATCH:"));
}

#[test]
fn captured_v2_editor_rejects_todos_snapshot_paste_alias_and_changed_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = seed(&path);
    let before = record(&host);
    let scope = host.open_capture_scope("card", 2).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("captured"), "")
        .unwrap();
    let mut todo_event = paste(&ticket, "same\nsame\nopen", "captured");
    todo_event.field = "todos".into();
    assert!(host.record_paste(&scope, todo_event).is_err());
    host.record_paste(
        &scope,
        paste(&ticket, &before.properties.description, "captured"),
    )
    .unwrap();
    let mut edit = fields(&before);
    edit.description = "captured".into();
    let mut with_todos = snapshot(&edit);
    with_todos.todos = "same".into();
    assert!(
        host.edit_card_captured("bad-todos", "card", 2, &edit, &scope, with_todos)
            .is_err()
    );
    let mut bad_alias = snapshot(&edit);
    bad_alias.aliases.push(AttachmentAlias {
        id: "unselected".into(),
        location: "C:/picture.png".into(),
        name: "picture.png".into(),
    });
    assert!(
        host.edit_card_captured("bad-alias", "card", 2, &edit, &scope, bad_alias)
            .is_err()
    );
    assert_eq!(record(&host).revision, 2);
    // A scope is bound to the full source; a later valid edit cannot reuse it.
    host.edit_card("independent", "card", 2, &CardAction::SetFavorite(true))
        .unwrap();
    assert!(
        host.edit_card_captured("late", "card", 2, &edit, &scope, snapshot(&edit))
            .is_err()
    );
    assert_eq!(record(&host).revision, 3);
}

#[test]
fn failed_storage_commit_keeps_original_v2_capture_pending_until_exact_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = seed(&path);
    let before = record(&host);
    let scope = host.open_capture_scope("card", 2).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("retained"), "")
        .unwrap();
    host.record_paste(
        &scope,
        paste(&ticket, &before.properties.description, "retained"),
    )
    .unwrap();
    let mut edit = fields(&before);
    edit.description = "retained".into();
    let endpoint = snapshot(&edit);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TRIGGER review_captured_v2_failure BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'synthetic captured v2 failure'); END;"
    ).unwrap();
    let failure = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint.clone())
        .err()
        .expect("synthetic storage failure");
    assert!(
        failure.to_string().contains("Storage"),
        "actual failure: {failure}"
    );
    assert_eq!(record(&host).revision, 2);
    assert!(host.operation_evidence("card", "pending-v2").is_err());
    let mut changed = edit.clone();
    changed.description.push('!');
    assert!(
        host.edit_card_captured(
            "pending-v2",
            "card",
            2,
            &changed,
            &scope,
            snapshot(&changed)
        )
        .is_err()
    );
    assert!(
        host.edit_card_captured(
            "other-operation",
            "card",
            2,
            &edit,
            &scope,
            endpoint.clone()
        )
        .is_err()
    );
    assert!(
        host.capture_scoped(&scope, capture_request("late"), "")
            .is_err()
    );
    assert!(
        host.record_paste(&scope, paste(&ticket, "retained", "late"))
            .is_err()
    );
    sql.execute_batch("DROP TRIGGER review_captured_v2_failure;")
        .unwrap();
    drop(sql);
    let saved = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint.clone())
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.committed.revision, 3);
    let evidence = host.operation_evidence("card", "pending-v2").unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(
        evidence[0]
            .data()
            .batch
            .as_ref()
            .unwrap()
            .observations
            .len(),
        2
    );
    let repeated = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint)
        .unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.receipt, saved.receipt);
    assert_eq!(
        host.operation_evidence("card", "pending-v2").unwrap()[0].container(),
        evidence[0].container()
    );
}

#[test]
fn deferred_commit_failure_keeps_original_v2_capture_pending_until_exact_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut host = seed(&path);
    let before = record(&host);
    let scope = host.open_capture_scope("card", 2).unwrap();
    let (ticket, _) = host
        .capture_scoped(&scope, capture_request("retained"), "")
        .unwrap();
    host.record_paste(
        &scope,
        paste(&ticket, &before.properties.description, "retained"),
    )
    .unwrap();
    let mut edit = fields(&before);
    edit.description = "retained".into();
    let endpoint = snapshot(&edit);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch(
        "CREATE TABLE review_v2_parent(id INTEGER PRIMARY KEY); CREATE TABLE review_v2_deferred(id INTEGER PRIMARY KEY,parent INTEGER REFERENCES review_v2_parent(id) DEFERRABLE INITIALLY DEFERRED); CREATE TRIGGER review_captured_v2_failure AFTER INSERT ON outbox BEGIN INSERT INTO review_v2_deferred(id,parent) VALUES(1,99); END;"
    ).unwrap();
    let failure = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint.clone())
        .err()
        .expect("synthetic storage failure");
    assert!(
        failure.to_string().contains("CommitUnknown"),
        "actual failure: {failure}"
    );
    assert_eq!(record(&host).revision, 2);
    assert!(host.operation_evidence("card", "pending-v2").is_err());
    let mut changed = edit.clone();
    changed.description.push('!');
    assert!(
        host.edit_card_captured(
            "pending-v2",
            "card",
            2,
            &changed,
            &scope,
            snapshot(&changed)
        )
        .is_err()
    );
    assert!(
        host.edit_card_captured(
            "other-operation",
            "card",
            2,
            &edit,
            &scope,
            endpoint.clone()
        )
        .is_err()
    );
    assert!(
        host.capture_scoped(&scope, capture_request("late"), "")
            .is_err()
    );
    assert!(
        host.record_paste(&scope, paste(&ticket, "retained", "late"))
            .is_err()
    );
    sql.execute_batch("DROP TRIGGER review_captured_v2_failure; DROP TABLE review_v2_deferred; DROP TABLE review_v2_parent;")
        .unwrap();
    drop(sql);
    let saved = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint.clone())
        .unwrap();
    assert!(!saved.repeated);
    assert_eq!(saved.committed.revision, 3);
    let evidence = host.operation_evidence("card", "pending-v2").unwrap();
    assert_eq!(evidence.len(), 1);
    assert_eq!(
        evidence[0]
            .data()
            .batch
            .as_ref()
            .unwrap()
            .observations
            .len(),
        2
    );
    let repeated = host
        .edit_card_captured("pending-v2", "card", 2, &edit, &scope, endpoint)
        .unwrap();
    assert!(repeated.repeated);
    assert_eq!(repeated.receipt, saved.receipt);
    assert_eq!(
        host.operation_evidence("card", "pending-v2").unwrap()[0].container(),
        evidence[0].container()
    );
}
