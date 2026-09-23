#![cfg(target_os = "windows")]
//! Real private protocol routes; the fixture needs no guest or existing card.
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_workbench_host::{
    Workbench,
    editor_draft::model::proto::{TextValue, Values, WriteRequest},
    editor_draft_staging_api_capnp as wire, host_capnp as host_wire, protocol,
};
use sha2::{Digest, Sha256};
use std::path::Path;
const CARD: &str = "import-wire-new-card";
const DRAFT: &str = "import-wire-draft";
const DATA: &[u8] = b"durable bytes from a deleted original file\x00\xff";
type Message = capnp::message::Reader<serialize::OwnedSegments>;
fn message(bytes: &[u8]) -> Message {
    serialize::read_message(&mut bytes.as_ref(), ReaderOptions::new()).unwrap()
}
fn text() -> TextValue {
    TextValue {
        text: String::new(),
        selection_base: -1,
        selection_extent: -1,
        affinity: 0,
        directional: false,
        composing_start: -1,
        composing_end: -1,
    }
}
fn draft(op: &str, generation: u64) -> WriteRequest {
    WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: op.into(),
        expected_generation: generation,
        source_revision: 0,
        source_kind: 1,
        predecessor_operation: String::new(),
        predecessor_sha256: vec![],
        assets: vec![],
        values: Some(Values {
            title: Some(text()),
            description: Some(text()),
            hypothesis: Some(text()),
            conclusion: Some(text()),
            todos: Some(text()),
            category: String::new(),
            stage: String::new(),
        }),
    }
}
fn seed(path: &Path) -> Workbench {
    let mut host = Workbench::open(path, None).unwrap();
    host.save_editor_draft(&draft("import-wire-draft-create", 0))
        .unwrap();
    host
}
fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/editor_draft_staging_api.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn proposal(op: &str, name: &str) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::import_request::Builder>();
    r.set_version(1);
    r.set_digest(&digest());
    r.set_card_id(CARD);
    r.set_draft_id(DRAFT);
    r.set_operation(op);
    r.set_expected_generation(1);
    r.set_name(name);
    r.set_kind("file");
    r.set_bytes(DATA.len() as u64);
    r.set_sha256(&Sha256::digest(DATA));
    serialize::write_message_to_words(&message)
}
fn raw(
    host: &mut Workbench,
    action: host_wire::Action,
    fill: impl FnOnce(host_wire::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<host_wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    fill(r);
    protocol::respond(host, &serialize::write_message_to_words(&message)).unwrap()
}
fn call(
    host: &mut Workbench,
    sequence: &mut u64,
    action: host_wire::Action,
    op: &str,
    imported: &str,
    generation: u64,
    body: &[u8],
    path: &str,
) -> Vec<u8> {
    let correlation = format!("draft-request-{}", *sequence);
    *sequence += 1;
    raw(host, action, |mut r| {
        r.set_id(CARD);
        r.set_attachment(DRAFT);
        r.set_operation(op);
        r.set_name(imported);
        r.set_revision(generation);
        r.set_payload(body);
        r.set_selected_path(path);
        r.set_transfer(&correlation);
    })
}
fn error(bytes: &[u8]) -> String {
    message(bytes)
        .get_root::<host_wire::response::Reader>()
        .unwrap()
        .get_error()
        .unwrap()
        .to_str()
        .unwrap()
        .into()
}
fn download(host: &mut Workbench, initial: &[u8]) -> Message {
    assert!(error(initial).is_empty(), "{}", error(initial));
    let first = message(initial);
    let r = first.get_root::<host_wire::response::Reader>().unwrap();
    let token = r.get_transfer().unwrap().to_str().unwrap().to_owned();
    let total = r.get_total_length() as usize;
    let revision = r.get_revision();
    let digest = r.get_sha256().unwrap().to_vec();
    assert!(!token.is_empty() && total > 0 && total <= 8 * 1024 * 1024);
    let mut body = r.get_payload().unwrap().to_vec();
    assert!(body.len() <= 32768);
    while body.len() < total {
        let next = raw(host, host_wire::Action::ReadEditorDraftPart, |mut r| {
            r.set_transfer(&token);
            r.set_offset(body.len() as u64);
        });
        assert!(error(&next).is_empty(), "{}", error(&next));
        let next = message(&next);
        let part = next.get_root::<host_wire::response::Reader>().unwrap();
        assert_eq!(part.get_transfer().unwrap().to_str().unwrap(), token);
        assert_eq!(part.get_offset() as usize, body.len());
        assert_eq!(part.get_revision(), revision);
        assert_eq!(part.get_total_length() as usize, total);
        assert_eq!(part.get_sha256().unwrap(), digest);
        let piece = part.get_payload().unwrap();
        assert!(!piece.is_empty() && piece.len() <= 32768);
        body.extend_from_slice(piece);
    }
    assert_eq!(body.len(), total);
    assert_eq!(Sha256::digest(&body).as_slice(), digest);
    let result = message(&body);
    let e = result.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_version(), 1);
    assert_eq!(e.get_digest().unwrap(), digest_schema());
    assert_eq!(e.get_current_generation(), revision);
    assert_eq!(e.get_card_id().unwrap().to_str().unwrap(), CARD);
    assert_eq!(e.get_draft_id().unwrap().to_str().unwrap(), DRAFT);
    result
}
fn digest_schema() -> [u8; 32] {
    digest()
}
fn record(m: &Message) -> wire::record::Reader<'_> {
    let e = m.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_kind().unwrap(), wire::ResultKind::Record);
    let r = e.get_record().unwrap();
    assert_eq!(r.get_current_generation(), e.get_current_generation());
    assert_eq!(r.get_main_active(), e.get_main_active());
    assert_eq!(r.get_staging_revision(), e.get_staging_revision());
    r
}

#[test]
fn complete_retry_checks_retained_identity_before_opening_deleted_path() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db");
    let source = temp.path().join("original.bin");
    std::fs::write(&source, DATA).unwrap();
    let mut host = seed(&path);
    let mut seq = 0;
    let body = proposal("wire-complete", "original.bin");
    let first = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "wire-complete",
        "wire-complete",
        1,
        &body,
        "",
    );
    let pending = download(&mut host, &first);
    assert_eq!(record(&pending).get_phase().unwrap(), wire::Phase::Pending);
    let sql = rusqlite::Connection::open(&path).unwrap();
    sql.execute_batch("CREATE TRIGGER reject_ready BEFORE UPDATE ON cards WHEN OLD.id LIKE 'morrow-host-editor-imports-%' BEGIN SELECT RAISE(ABORT,'ready blocked'); END;").unwrap();
    let attempted = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-complete",
        "wire-complete",
        1,
        &body,
        source.to_str().unwrap(),
    );
    assert!(!error(&attempted).is_empty());
    let inspected = call(
        &mut host,
        &mut seq,
        host_wire::Action::InspectEditorDraftImport,
        "wire-complete",
        "wire-complete",
        0,
        &[],
        "",
    );
    let state = download(&mut host, &inspected);
    assert!(record(&state).get_bytes_retained());
    assert_eq!(record(&state).get_phase().unwrap(), wire::Phase::Pending);
    sql.execute_batch("DROP TRIGGER reject_ready;").unwrap();
    drop(sql);
    host.save_editor_draft(&draft("wire-body-next", 1)).unwrap();
    drop(host);
    std::fs::remove_file(&source).unwrap();
    let mut host = Workbench::open(&path, None).unwrap();
    let retry = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-complete",
        "wire-complete",
        1,
        &body,
        source.to_str().unwrap(),
    );
    let ready = download(&mut host, &retry);
    let r = record(&ready);
    assert_eq!(r.get_phase().unwrap(), wire::Phase::Ready);
    assert!(r.get_current_active() && r.get_bytes_retained());
    assert_eq!(r.get_current_generation(), 2);
    assert_eq!(r.get_request().unwrap().get_expected_generation(), 1);
    let again = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-complete",
        "wire-complete",
        1,
        &body,
        "",
    );
    let again = download(&mut host, &again);
    assert!(record(&again).get_repeated());
    assert_eq!(
        record(&again).get_staging_revision(),
        r.get_staging_revision()
    );
    let changed = proposal("wire-complete", "different-name.bin");
    let rejected = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-complete",
        "wire-complete",
        1,
        &changed,
        "",
    );
    assert!(!error(&rejected).is_empty());
}

#[test]
fn outer_binding_and_missing_reply_correlation_fail_before_pending_admission() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = seed(&temp.path().join("db"));
    let body = proposal("wire-not-admitted", "fixed.bin");
    let mut seq = 0;
    let no_correlation = raw(
        &mut host,
        host_wire::Action::BeginEditorDraftImport,
        |mut r| {
            r.set_id(CARD);
            r.set_attachment(DRAFT);
            r.set_operation("wire-not-admitted");
            r.set_name("wire-not-admitted");
            r.set_revision(1);
            r.set_payload(&body);
        },
    );
    assert!(!error(&no_correlation).is_empty());
    let mismatch = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "different-op",
        "wire-not-admitted",
        1,
        &body,
        "",
    );
    assert!(!error(&mismatch).is_empty());
    let mut trailing = body.clone();
    trailing.extend_from_slice(&[0; 8]);
    let malformed = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "wire-not-admitted",
        "wire-not-admitted",
        1,
        &trailing,
        "",
    );
    assert!(!error(&malformed).is_empty());
    let absent = call(
        &mut host,
        &mut seq,
        host_wire::Action::InspectEditorDraftImport,
        "wire-not-admitted",
        "wire-not-admitted",
        0,
        &[],
        "",
    );
    let absent = download(&mut host, &absent);
    assert_eq!(
        absent
            .get_root::<wire::envelope::Reader>()
            .unwrap()
            .get_kind()
            .unwrap(),
        wire::ResultKind::Absent
    );
    assert!(
        host.list_editor_draft_imports(CARD, DRAFT)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn lost_first_list_reply_can_be_aborted_without_revoking_imports_or_new_reply() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = seed(&temp.path().join("db"));
    let mut seq = 0;
    for n in 0..3 {
        let op = format!("wire-list-{n}");
        let body = proposal(&op, &"N".repeat(16000));
        let reply = call(
            &mut host,
            &mut seq,
            host_wire::Action::BeginEditorDraftImport,
            &op,
            &op,
            1,
            &body,
            "",
        );
        download(&mut host, &reply);
    }
    let old_key = format!("draft-request-{seq}");
    let lost = call(
        &mut host,
        &mut seq,
        host_wire::Action::ListEditorDraftImports,
        "",
        "",
        0,
        &[],
        "",
    );
    assert!(
        message(&lost)
            .get_root::<host_wire::response::Reader>()
            .unwrap()
            .get_total_length()
            > 32768
    );
    let abort = raw(
        &mut host,
        host_wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&old_key),
    );
    assert!(error(&abort).is_empty());
    let next = call(
        &mut host,
        &mut seq,
        host_wire::Action::ListEditorDraftImports,
        "",
        "",
        0,
        &[],
        "",
    );
    let stale = raw(
        &mut host,
        host_wire::Action::AbortEditorDraftTransfer,
        |mut r| r.set_transfer(&old_key),
    );
    assert!(error(&stale).is_empty());
    let list = download(&mut host, &next);
    let e = list.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_records().unwrap().len(), 3);
    assert_eq!(e.get_staging_revision(), 3);
    assert_eq!(
        host.list_editor_draft_imports(CARD, DRAFT).unwrap().len(),
        3
    );
}

#[test]
fn exact_export_retry_preserves_collisions_and_abandon_cleanup_is_separate() {
    let temp = tempfile::tempdir().unwrap();
    let mut host = seed(&temp.path().join("db"));
    let mut seq = 0;
    let source = temp.path().join("source.bin");
    std::fs::write(&source, DATA).unwrap();
    let body = proposal("wire-export", "source.bin");
    let ready = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-export",
        "wire-export",
        1,
        &body,
        source.to_str().unwrap(),
    );
    download(&mut host, &ready);
    let target = temp.path().join("export.bin");
    for _ in 0..2 {
        let reply = call(
            &mut host,
            &mut seq,
            host_wire::Action::ExportEditorDraftImport,
            "wire-export",
            "wire-export",
            1,
            &[],
            target.to_str().unwrap(),
        );
        let reply = download(&mut host, &reply);
        let e = reply.get_root::<wire::envelope::Reader>().unwrap();
        assert_eq!(e.get_kind().unwrap(), wire::ResultKind::Exported);
        assert_eq!(
            e.get_export_sha256().unwrap(),
            Sha256::digest(DATA).as_slice()
        );
        assert_eq!(e.get_export_bytes(), DATA.len() as u64);
    }
    assert_eq!(std::fs::read(&target).unwrap(), DATA);
    std::fs::write(&target, b"do not replace this unrelated user file").unwrap();
    let collision = call(
        &mut host,
        &mut seq,
        host_wire::Action::ExportEditorDraftImport,
        "wire-export",
        "wire-export",
        1,
        &[],
        target.to_str().unwrap(),
    );
    assert!(!error(&collision).is_empty());
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"do not replace this unrelated user file"
    );
    let prepared = call(
        &mut host,
        &mut seq,
        host_wire::Action::PrepareEditorDraftImportDecision,
        "wire-abandon",
        "wire-export",
        1,
        &[],
        "",
    );
    let prepared = download(&mut host, &prepared);
    let decision = prepared
        .get_root::<wire::envelope::Reader>()
        .unwrap()
        .get_decision()
        .unwrap();
    assert_eq!(
        decision.get_status().unwrap(),
        wire::DecisionStatus::Pending
    );
    assert_eq!(decision.get_committed_revision(), 0);
    let abandoned = call(
        &mut host,
        &mut seq,
        host_wire::Action::AbandonEditorDraftImport,
        "wire-abandon",
        "wire-export",
        1,
        &[],
        "",
    );
    let abandoned = download(&mut host, &abandoned);
    assert_eq!(
        record(&abandoned).get_phase().unwrap(),
        wire::Phase::Retired
    );
    assert!(record(&abandoned).get_bytes_retained());
    let cleaned = call(
        &mut host,
        &mut seq,
        host_wire::Action::ReconcileEditorDraftImports,
        "",
        "",
        0,
        &[],
        "",
    );
    let cleaned = download(&mut host, &cleaned);
    assert_eq!(
        cleaned
            .get_root::<wire::envelope::Reader>()
            .unwrap()
            .get_records()
            .unwrap()
            .len(),
        0
    );
    std::fs::remove_file(&source).unwrap();
    let old = call(
        &mut host,
        &mut seq,
        host_wire::Action::CompleteEditorDraftImport,
        "wire-export",
        "wire-export",
        1,
        &body,
        source.to_str().unwrap(),
    );
    let old = download(&mut host, &old);
    assert!(!record(&old).get_current_active());
    assert!(!record(&old).get_bytes_retained());
    let forbidden = temp.path().join("cannot-export.bin");
    let denied = call(
        &mut host,
        &mut seq,
        host_wire::Action::ExportEditorDraftImport,
        "wire-export",
        "wire-export",
        1,
        &[],
        forbidden.to_str().unwrap(),
    );
    assert!(!error(&denied).is_empty());
    assert!(!forbidden.exists());
}

fn decision_page(host: &mut Workbench, sequence: &mut u64, cursor: &str, limit: u32) -> Vec<u8> {
    let correlation = format!("draft-request-{}", *sequence);
    *sequence += 1;
    raw(
        host,
        host_wire::Action::ListEditorDraftImportDecisions,
        |mut r| {
            r.set_id(CARD);
            r.set_attachment(DRAFT);
            r.set_transfer(&correlation);
            r.set_cursor(cursor);
            r.set_limit(limit);
        },
    )
}

#[test]
fn decision_pagination_recovers_original_operations_and_cancel_seals_late_abandon() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db");
    let mut host = seed(&path);
    let mut seq = 0;
    let body = proposal("wire-decision-import", "source.bin");
    let begun = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "wire-decision-import",
        "wire-decision-import",
        1,
        &body,
        "",
    );
    download(&mut host, &begun);
    let unprepared = call(
        &mut host,
        &mut seq,
        host_wire::Action::AbandonEditorDraftImport,
        "unprepared-decision",
        "wire-decision-import",
        1,
        &[],
        "",
    );
    assert!(!error(&unprepared).is_empty());
    // Cancel decisions are independent of stage revisions and retain exact op identities.
    for i in 0..35 {
        let operation = format!("wire-decision-{i:02}");
        let prepared = call(
            &mut host,
            &mut seq,
            host_wire::Action::PrepareEditorDraftImportDecision,
            &operation,
            "wire-decision-import",
            1,
            &[],
            "",
        );
        let prepared = download(&mut host, &prepared);
        let e = prepared.get_root::<wire::envelope::Reader>().unwrap();
        assert_eq!(e.get_kind().unwrap(), wire::ResultKind::Decision);
        assert_eq!(
            e.get_decision().unwrap().get_status().unwrap(),
            wire::DecisionStatus::Pending
        );
        let cancel = call(
            &mut host,
            &mut seq,
            host_wire::Action::CancelEditorDraftImportDecision,
            &operation,
            "wire-decision-import",
            1,
            &[],
            "",
        );
        let cancel = download(&mut host, &cancel);
        let e = cancel.get_root::<wire::envelope::Reader>().unwrap();
        let d = e.get_decision().unwrap();
        assert_eq!(d.get_status().unwrap(), wire::DecisionStatus::Cancelled);
        assert_eq!(d.get_decision_revision(), 2);
        assert_eq!(d.get_committed_revision(), 0);
        assert_eq!(e.get_staging_revision(), 1);
    }
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    seq = 0;
    let first = decision_page(&mut host, &mut seq, "", 32);
    let first = download(&mut host, &first);
    let e = first.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_kind().unwrap(), wire::ResultKind::Decisions);
    assert_eq!(e.get_request_limit(), 32);
    assert_eq!(e.get_request_cursor().unwrap().to_str().unwrap(), "");
    assert_eq!(e.get_decisions().unwrap().len(), 32);
    let cursor = e.get_next_cursor().unwrap().to_str().unwrap().to_owned();
    assert!(!cursor.is_empty());
    let mut operations = std::collections::BTreeSet::new();
    for d in e.get_decisions().unwrap() {
        assert_eq!(d.get_status().unwrap(), wire::DecisionStatus::Cancelled);
        assert_eq!(
            d.get_request()
                .unwrap()
                .get_operation()
                .unwrap()
                .to_str()
                .unwrap(),
            "wire-decision-import"
        );
        assert!(operations.insert(d.get_operation().unwrap().to_str().unwrap().to_owned()));
    }
    let second = decision_page(&mut host, &mut seq, &cursor, 32);
    let second = download(&mut host, &second);
    let e = second.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_request_cursor().unwrap().to_str().unwrap(), cursor);
    assert_eq!(e.get_decisions().unwrap().len(), 3);
    assert_eq!(e.get_next_cursor().unwrap().to_str().unwrap(), "");
    for d in e.get_decisions().unwrap() {
        assert!(operations.insert(d.get_operation().unwrap().to_str().unwrap().to_owned()));
    }
    assert_eq!(operations.len(), 35);
    for operation in operations {
        let late = call(
            &mut host,
            &mut seq,
            host_wire::Action::AbandonEditorDraftImport,
            &operation,
            "wire-decision-import",
            1,
            &[],
            "",
        );
        assert!(!error(&late).is_empty());
    }
    let invalid = decision_page(&mut host, &mut seq, "", 33);
    assert!(!error(&invalid).is_empty());
    let invalid = decision_page(&mut host, &mut seq, "foreign-cursor", 32);
    assert!(!error(&invalid).is_empty());
    let imported = call(
        &mut host,
        &mut seq,
        host_wire::Action::InspectEditorDraftImport,
        "wire-decision-import",
        "wire-decision-import",
        0,
        &[],
        "",
    );
    let imported = download(&mut host, &imported);
    assert_eq!(record(&imported).get_phase().unwrap(), wire::Phase::Pending);
    assert!(!record(&imported).get_current_active());
}

#[test]
fn prepared_decision_survives_baseline_change_and_cancel_does_not_abandon_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db");
    let mut host = seed(&path);
    let mut seq = 0;
    let body = proposal("wire-conflict-import", "source.bin");
    let begun = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "wire-conflict-import",
        "wire-conflict-import",
        1,
        &body,
        "",
    );
    download(&mut host, &begun);
    let prepared = call(
        &mut host,
        &mut seq,
        host_wire::Action::PrepareEditorDraftImportDecision,
        "wire-conflict-decision",
        "wire-conflict-import",
        1,
        &[],
        "",
    );
    download(&mut host, &prepared);
    host.save_editor_draft(&draft("wire-conflict-advance", 1))
        .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    seq = 0;
    // Exact prepare retry must not silently rebase or repeat the business mutation.
    let again = call(
        &mut host,
        &mut seq,
        host_wire::Action::PrepareEditorDraftImportDecision,
        "wire-conflict-decision",
        "wire-conflict-import",
        1,
        &[],
        "",
    );
    let again = download(&mut host, &again);
    let e = again.get_root::<wire::envelope::Reader>().unwrap();
    let d = e.get_decision().unwrap();
    assert_eq!(d.get_status().unwrap(), wire::DecisionStatus::Conflict);
    assert_eq!(d.get_expected_generation(), 1);
    assert_eq!(d.get_current_generation(), 2);
    assert_eq!(d.get_decision_revision(), 1);
    let cancel = call(
        &mut host,
        &mut seq,
        host_wire::Action::CancelEditorDraftImportDecision,
        "wire-conflict-decision",
        "wire-conflict-import",
        1,
        &[],
        "",
    );
    let cancel = download(&mut host, &cancel);
    assert_eq!(
        cancel
            .get_root::<wire::envelope::Reader>()
            .unwrap()
            .get_decision()
            .unwrap()
            .get_status()
            .unwrap(),
        wire::DecisionStatus::Cancelled
    );
    let late = call(
        &mut host,
        &mut seq,
        host_wire::Action::AbandonEditorDraftImport,
        "wire-conflict-decision",
        "wire-conflict-import",
        1,
        &[],
        "",
    );
    assert!(!error(&late).is_empty());
}

#[test]
fn global_scope_route_discovers_inactive_draft_and_rejects_scope_smuggling() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("db");
    let mut host = seed(&path);
    let mut seq = 0;
    let body = proposal("global-scope-import", "source.bin");
    let begun = call(
        &mut host,
        &mut seq,
        host_wire::Action::BeginEditorDraftImport,
        "global-scope-import",
        "global-scope-import",
        1,
        &body,
        "",
    );
    download(&mut host, &begun);
    let prepared = call(
        &mut host,
        &mut seq,
        host_wire::Action::PrepareEditorDraftImportDecision,
        "global-scope-abandon",
        "global-scope-import",
        1,
        &[],
        "",
    );
    download(&mut host, &prepared);
    host.discard_editor_draft(CARD, DRAFT, 1, "global-scope-discard")
        .unwrap();
    drop(host);
    let mut host = Workbench::open(&path, None).unwrap();
    let result = raw(
        &mut host,
        host_wire::Action::ListEditorDraftImportDecisionScopes,
        |mut r| {
            r.set_limit(32);
            r.set_transfer("draft-request-0");
        },
    );
    assert!(error(&result).is_empty(), "{}", error(&result));
    let outer = message(&result);
    let response = outer.get_root::<host_wire::response::Reader>().unwrap();
    assert_eq!(response.get_revision(), 0);
    let content = message(response.get_payload().unwrap());
    let e = content.get_root::<wire::envelope::Reader>().unwrap();
    assert_eq!(e.get_kind().unwrap(), wire::ResultKind::Scopes);
    assert_eq!(e.get_digest().unwrap(), digest_schema());
    assert_eq!(e.get_card_id().unwrap().to_str().unwrap(), "");
    assert_eq!(e.get_draft_id().unwrap().to_str().unwrap(), "");
    assert_eq!(e.get_current_generation(), 0);
    assert_eq!(e.get_staging_revision(), 0);
    assert!(!e.get_main_active());
    assert_eq!(e.get_request_limit(), 32);
    assert_eq!(e.get_scopes().unwrap().len(), 1);
    let scope = e.get_scopes().unwrap().get(0);
    assert_eq!(scope.get_card_id().unwrap().to_str().unwrap(), CARD);
    assert_eq!(scope.get_draft_id().unwrap().to_str().unwrap(), DRAFT);
    let invalid = raw(
        &mut host,
        host_wire::Action::ListEditorDraftImportDecisionScopes,
        |mut r| {
            r.set_id(CARD);
            r.set_limit(32);
            r.set_transfer("draft-request-1");
        },
    );
    assert!(!error(&invalid).is_empty());
}
