use super::*;
use crate::{Engine, Request, draft_bridge, hex};
use morrow_core::{content::CardRecord, lifecycle::GrantKind, transaction::{self, Lookup}};
use morrow_workbench_plugin::tasks_v2;
use prost::{Message, encoding::{DecodeContext, decode_key, decode_varint, skip_field}};
use serde_json::{Value, json};

const CARD: &str = "create-todos-card";
const OPERATION: &str = "create-todos-original";

fn request(value: Value) -> Request { serde_json::from_value(value).unwrap() }
fn wire(raw: &str) -> Value {
    json!({"action":"create","id":CARD,"operation":OPERATION,"title":"created title",
        "description":"body","hypothesis":"hypothesis","conclusion":"conclusion",
        "category":"灵感","stage":"待整理","todos":raw})
}
fn engine() -> (tempfile::TempDir, std::path::PathBuf, Engine) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let engine = Engine::open(&path).unwrap();
    (directory, path, engine)
}
fn original_tasks(engine: &Engine) -> Vec<Task> {
    let event = engine.host.store_local().pending(0, 128).unwrap().into_iter().find_map(|(_, bytes)| {
        let (commit, _) = transaction::decode_commit(&bytes).unwrap();
        (commit.operation_id == OPERATION).then_some(commit)
    }).unwrap();
    let command = transaction::decode_command(&event.command).unwrap();
    let transaction::proto::command::Action::CreateCard(bytes) = command.action.unwrap() else { panic!("original atomic create expected") };
    let card = CardRecord::decode(&bytes).unwrap();
    tasks_v2::decode(CARD, &card.summary().title, &card.body()).unwrap().tasks
}
fn field(bytes: &[u8], wanted: u32) -> Option<Vec<u8>> {
    let mut input = bytes;
    while !input.is_empty() {
        let (tag, wire) = decode_key(&mut input).unwrap();
        if tag == wanted {
            assert_eq!(wire, WireType::LengthDelimited);
            let length = usize::try_from(decode_varint(&mut input).unwrap()).unwrap();
            return Some(input[..length].to_vec());
        }
        skip_field(wire, tag, &mut input, DecodeContext::default()).unwrap();
    }
    None
}
fn metadata(engine: &Engine) -> Vec<u8> {
    field(&engine.host.store_local().card(CARD).unwrap().unwrap().body(), RAW_IDENTITY_FIELD).unwrap()
}

#[test]
fn normalizes_actual_flutter_lf_trim_blank_dedup_without_unicode_normalization() {
    let raw = "  beta \r\nalpha\n beta\n\n\u{feff} z\u{feff}\n e\u{301}\n é \n\u{85}\u{a0}wide\u{3000}";
    let prepared = prepare(CARD, OPERATION, raw).unwrap();
    assert_eq!(prepared.tasks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
        vec!["beta", "alpha", "z", "e\u{301}", "é", "wide"]);
    assert!(prepared.tasks.iter().all(|t| t.completion == Completion::Incomplete as i32 && !t.legacy_completed && t.legacy_duplicates == 0));
    assert_eq!(raw, "  beta \r\nalpha\n beta\n\n\u{feff} z\u{feff}\n e\u{301}\n é \n\u{85}\u{a0}wide\u{3000}");
    // Only LF is a row separator; an internal CR/Unicode paragraph separator is
    // ordinary label content. The creation projection does not guess rows.
    assert_eq!(prepare(CARD, OPERATION, "one\rtwo\u{2028}three").unwrap().tasks[0].text, "one\rtwo\u{2028}three");
}

#[test]
fn original_grapheme_and_view_row_budgets_precede_blank_and_duplicate_normalization() {
    assert!(prepare(CARD, OPERATION, &"😀".repeat(1000)).is_ok());
    assert_eq!(prepare(CARD, OPERATION, &"😀".repeat(1001)).err().unwrap(), "EditorFieldGraphemeLimit");
    assert!(prepare(CARD, OPERATION, &vec!["a"; 100].join("\n")).is_ok());
    assert_eq!(prepare(CARD, OPERATION, &vec!["a"; 101].join("\n")).err().unwrap(), "CreateTodosRowLimit");
    assert!(prepare(CARD, OPERATION, &"\n".repeat(99)).unwrap().tasks.is_empty());
    assert_eq!(prepare(CARD, OPERATION, &"\n".repeat(100)).err().unwrap(), "CreateTodosRowLimit");
    assert!(prepare(CARD, OPERATION, "").unwrap().tasks.is_empty());
    assert!(prepare(CARD, OPERATION, &" \u{301}".repeat(1001)).is_err());
}

#[test]
fn business_task_ids_are_deterministic_original_request_ids_not_view_row_ids() {
    let a = prepare(CARD, OPERATION, "one\ntwo").unwrap();
    assert_eq!(a.tasks, prepare(CARD, OPERATION, "one\ntwo").unwrap().tasks);
    assert_ne!(a.tasks[0].id, a.tasks[1].id);
    assert_ne!(a.tasks[0].id, prepare("different-card", OPERATION, "one\ntwo").unwrap().tasks[0].id);
    assert_ne!(a.tasks[0].id, prepare(CARD, "different-operation", "one\ntwo").unwrap().tasks[0].id);
    assert_ne!(a.tasks[0].id, prepare(CARD, OPERATION, " one\ntwo").unwrap().tasks[0].id);
    assert!(a.tasks.iter().all(|t| t.id.starts_with("task-hmos-create-") && t.id.len() < 256));
}

#[test]
fn creates_all_tasks_in_one_authorized_store_operation_and_exact_receipt_after_reopen() {
    let (_directory, path, mut engine) = engine();
    let raw = " beta \nalpha\nbeta\n\n😀";
    let original = wire(raw);
    let saved = engine.execute(request(original.clone())).unwrap();
    assert_eq!(saved.effect, "committed");
    assert_eq!(saved.receipt_revision, "1");
    assert_eq!(saved.cards.len(), 1);
    assert_eq!(saved.cards[0].tasks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), vec!["beta", "alpha", "😀"]);
    assert_eq!(engine.host.store_local().pending_usage().unwrap().0, 1);
    let tasks = original_tasks(&engine);
    let identity = metadata(&engine);
    assert_eq!(&identity[..RAW_DOMAIN.len()], RAW_DOMAIN);
    assert_eq!(&identity[RAW_DOMAIN.len()..], raw_identity(raw));
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    let retry = engine.execute(request(original)).unwrap();
    assert_eq!(retry.receipt_revision, "1");
    assert_eq!(retry.cards[0].tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
    assert_eq!(original_tasks(&engine), tasks);
    assert_eq!(engine.host.store_local().pending_usage().unwrap().0, 1);
    assert_eq!(metadata(&engine), identity);
}

#[test]
fn same_normalized_tasks_different_complete_raw_retries_are_rejected_including_all_blank() {
    for (first, changed) in [("one\none", " one\none"), ("one\none", "one"), (" \n", "\t\n"), ("", " ")] {
        let (_directory, path, mut engine) = engine();
        let original = wire(first);
        engine.execute(request(original.clone())).unwrap();
        let card = engine.host.store_local().card(CARD).unwrap().unwrap().encode();
        drop(engine);
        let mut engine = Engine::open(&path).unwrap();
        let error = engine.execute(request(wire(changed))).unwrap_err();
        assert!(error.to_lowercase().contains("operation"), "{error}");
        // Historical commitment stays proven, even when the edited retry is
        // invalid; that proof never authorizes discarding the original payload.
        assert_eq!(engine.effect, "committed");
        assert_eq!(engine.host.store_local().card(CARD).unwrap().unwrap().encode(), card);
        assert_eq!(engine.host.store_local().pending_usage().unwrap().0, 1);
        assert_eq!(engine.execute(request(original)).unwrap().receipt_revision, "1");
    }
}

#[test]
fn preserves_registered_and_unrelated_unknown_properties_across_edits_and_historical_retry() {
    let (_directory, path, mut engine) = engine();
    let original = wire("one\ntwo");
    let created = engine.execute(request(original.clone())).unwrap().cards.remove(0);
    let task_ids: Vec<_> = created.tasks.iter().map(|t| t.id.clone()).collect();
    let initial_tasks = original_tasks(&engine);
    let identity = metadata(&engine);
    let source = engine.host.store_local().card(CARD).unwrap().unwrap();
    let mut body = source.body();
    encode_key(50_002, WireType::LengthDelimited, &mut body);
    encode_varint(6, &mut body); body.extend_from_slice(b"future");
    let mut connection = engine.host.connect().unwrap();
    let start = engine.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let now = clock();
    engine.host.grant(&mut connection, GrantKind::EditContent, CARD, now + 60_000, now).unwrap();
    engine.host.edit_versioned_content(&connection, &morrow_core::versioned_content_change::VersionedContentChange {
        operation_id: "create-todos-other-unknown".into(), source_card: source.encode(), title: source.summary().title,
        body, preview_text: String::new(), attachments: None,
    }, clock).unwrap();
    engine.host.disconnect(&connection).unwrap();
    let mut current = engine.execute(request(json!({"action":"list"}))).unwrap().cards.remove(0);
    for (index, action) in ["edit", "favorite", "task_add", "task_toggle", "task_rename", "task_reorder"].into_iter().enumerate() {
        let mut command = wire("");
        command["action"] = action.into(); command["source"] = current.source.clone().into();
        command["operation"] = format!("create-todos-mutation-{index}").into();
        command["flag"] = true.into(); command["text"] = "changed".into();
        command["task_id"] = if action == "task_add" { "added-task".into() } else { task_ids[0].clone().into() };
        command["order"] = current.tasks.iter().rev().map(|t| t.id.clone()).collect::<Vec<_>>().into();
        current = engine.execute(request(command)).unwrap().cards.remove(0);
        assert_eq!(metadata(&engine), identity);
        assert_eq!(field(&engine.host.store_local().card(CARD).unwrap().unwrap().body(), 50_002).unwrap(), b"future");
    }
    let before = engine.host.store_local().card(CARD).unwrap().unwrap().encode();
    let pending = engine.host.store_local().pending_usage().unwrap();
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    let retry = engine.execute(request(original)).unwrap();
    assert_eq!(retry.receipt_revision, "1");
    assert_eq!(retry.effect, "committed");
    assert_eq!(engine.host.store_local().card(CARD).unwrap().unwrap().encode(), before);
    assert_eq!(engine.host.store_local().pending_usage().unwrap(), pending);
    assert_eq!(original_tasks(&engine), initial_tasks);
    assert_eq!(metadata(&engine), identity);
}

#[test]
fn old_empty_create_proposal_keeps_exact_core_payload_and_existing_v2_editor_rejects_todos() {
    let (_directory, _path, mut engine) = engine();
    // Persist the old adapter's exact task-free proposal through the unchanged
    // authorized core API, then read it using the newer Request shape.
    let old = tasks_v2::Properties { version: 2, description: "body".into(), category: "灵感".into(),
        stage: "待整理".into(), hypothesis: "hypothesis".into(), conclusion: "conclusion".into(), ..Default::default() };
    let card = CardRecord::new_with_attachments(CARD, "idea", 2, "created title", old.encode_to_vec(), &[]).unwrap();
    let start = engine.start; let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let now = clock(); let mut connection = engine.host.connect().unwrap();
    engine.host.grant(&mut connection, GrantKind::CreateContent, CARD, now + 60_000, now).unwrap();
    engine.host.create_content(&connection, OPERATION, &card, clock).unwrap();
    engine.host.disconnect(&connection).unwrap();
    let mut original = wire(""); original.as_object_mut().unwrap().remove("todos");
    let retry = engine.execute(request(original)).unwrap();
    assert_eq!(retry.receipt_revision, "1");
    assert_eq!(field(&engine.host.store_local().card(CARD).unwrap().unwrap().body(), RAW_IDENTITY_FIELD), None);
    let before = engine.host.store_local().card(CARD).unwrap().unwrap().encode();
    for raw in ["new task", " ", "\n"] {
        let mut edit = wire(raw); edit["action"] = "edit".into();
        edit["source"] = hex(&before).into(); edit["operation"] = "unsupported-new-todos-edit".into();
        assert_eq!(engine.execute(request(edit)).unwrap_err(), "V2EditorTodosUnsupported");
        assert_eq!(engine.effect, "not_committed");
        assert_eq!(engine.host.store_local().card(CARD).unwrap().unwrap().encode(), before);
        assert!(matches!(engine.host.store_local().lookup_for_card(CARD, "unsupported-new-todos-edit").unwrap(), Lookup::Absent));
    }
    assert_eq!(engine.host.store_local().pending_usage().unwrap().0, 1);
}

#[test]
fn task_and_aggregate_byte_limits_refuse_before_creating_any_card_operation_or_subset() {
    for (raw, description, expected) in [
        (format!("a{}", "\u{301}".repeat(1024)), "body".into(), "invalid V2 task"),
        ("small".into(), "x".repeat(65_500), "V2 properties budget"),
        ("a".repeat(1001), "body".into(), "EditorFieldGraphemeLimit"),
        (vec!["a"; 101].join("\n"), "body".into(), "CreateTodosRowLimit"),
    ] {
        let (_directory, _path, mut engine) = engine();
        let mut command = wire(&raw); command["description"] = description.into();
        assert_eq!(engine.execute(request(command)).unwrap_err(), expected);
        assert_eq!(engine.effect, "not_committed");
        assert!(engine.host.store_local().card(CARD).unwrap().is_none());
        assert!(matches!(engine.host.store_local().lookup_for_card(CARD, OPERATION).unwrap(), Lookup::Absent));
        assert_eq!(engine.host.store_local().pending_usage().unwrap(), (0, 0));
    }
    let (_directory, _path, mut engine) = engine();
    let raw = format!("a{}", "\u{301}".repeat(1023));
    assert_eq!(raw.len(), 2047);
    assert_eq!(engine.execute(request(wire(&raw))).unwrap().cards[0].tasks[0].text, raw);
}

fn text(value: &str) -> Value {
    json!({"text":value,"selection_base":-1,"selection_extent":-1,"composing_start":-1,"composing_end":-1,"affinity":0,"directional":false})
}
fn save_draft(engine: &mut Engine, raw: &str, expected: &str, operation: &str) -> draft_bridge::View {
    engine.execute(request(json!({"action":"draft_save","draft":{
        "card_id":CARD,"draft_id":"create-todos-draft","source_kind":1,"source_revision":"0",
        "expected_generation":expected,"operation_id":operation,"assets":[],
        "values":{"title":text("created title"),"description":text("body"),"hypothesis":text("hypothesis"),
            "conclusion":text("conclusion"),"todos":text(raw),"category":"灵感","stage":"待整理"}
    }}))).unwrap().drafts.remove(0)
}
fn published(raw: &str, draft: &draft_bridge::View) -> Value {
    let view = serde_json::to_value(draft).unwrap(); let mut command = wire(raw);
    command["draft_id"] = view["scope"]["draft_id"].clone();
    command["generation"] = view["generation"].clone(); command["draft_operation"] = view["operation_id"].clone(); command
}

#[test]
fn new_create_binds_original_raw_draft_values_and_replays_history_after_newer_generation() {
    let (_directory, path, mut engine) = engine();
    let raw = " one \none\n two \n";
    let draft = save_draft(&mut engine, raw, "0", "create-todos-save-1");
    let mismatch = published("one\ntwo", &draft);
    assert_eq!(engine.execute(request(mismatch)).unwrap_err(), "DraftPublicationValuesMismatch");
    assert_eq!(engine.effect, "not_committed");
    assert!(engine.host.store_local().card(CARD).unwrap().is_none());
    let original = published(raw, &draft);
    let saved = engine.execute(request(original.clone())).unwrap();
    assert_eq!(saved.cards[0].tasks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
    let prior = serde_json::to_value(&draft).unwrap();
    save_draft(&mut engine, "newer draft raw", prior["generation"].as_str().unwrap(), "create-todos-save-2");
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    assert_eq!(engine.execute(request(original.clone())).unwrap().receipt_revision, "1");
    let journal = engine.execute(request(json!({"action":"draft_read","id":CARD,"draft_id":"create-todos-draft"}))).unwrap();
    let journal = serde_json::to_value(&journal.drafts[0]).unwrap();
    assert_eq!(journal["values"]["todos"]["text"], "newer draft raw");
    assert_eq!(original_tasks(&engine).iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), vec!["one", "two"]);
    let before = engine.host.store_local().card(CARD).unwrap().unwrap().encode();
    let mut changed = original.clone(); changed["todos"] = "one\ntwo".into();
    assert_eq!(engine.execute(request(changed)).unwrap_err(), "DraftPublicationValuesMismatch");
    assert_eq!(engine.effect, "committed");
    engine.execute(request(json!({"action":"draft_discard","id":CARD,"draft_id":"create-todos-draft",
        "generation":journal["generation"],"operation":"create-todos-discard"}))).unwrap();
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    assert_eq!(engine.execute(request(original)).unwrap().receipt_revision, "1");
    assert_eq!(engine.host.store_local().card(CARD).unwrap().unwrap().encode(), before);
}

#[test]
#[ignore = "requires a fresh actual Dart VM normalization capture"]
fn actual_dart_vm_complete_normalization_outputs_match() {
    let path = std::env::var_os("HMOS_CREATE_TODOS_DART_REFERENCE").expect("actual fresh Dart capture required");
    let reference: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(reference["characters_version"], "1.4.1");
    assert_eq!(reference["unicode_version"], "16.0.0");
    let records = reference["records"].as_array().unwrap();
    assert_eq!(records.len(), 50);
    for record in records {
        let raw = record["raw"].as_str().unwrap();
        let id = record["id"].as_str().unwrap();
        assert_eq!(raw.len() as u64, record["utf8_bytes"].as_u64().unwrap(), "{id}");
        assert_eq!(raw.encode_utf16().count() as u64, record["utf16_units"].as_u64().unwrap(), "{id}");
        assert_eq!(format!("{:x}", Sha256::digest(raw.as_bytes())), record["sha256"].as_str().unwrap(), "{id}");
        assert_eq!(crate::editor_field::grapheme_count(raw) as u64, record["graphemes"].as_u64().unwrap(), "{id}");
        assert_eq!(if raw.is_empty() { 0 } else { raw.split('\n').count() } as u64, record["raw_rows"].as_u64().unwrap(), "{id}");
        let result = prepare(CARD, OPERATION, raw);
        assert_eq!(result.is_ok(), record["field_and_rows_accepted"].as_bool().unwrap(), "{id}");
        if let Ok(prepared) = result {
            let expected: Vec<_> = record["normalized"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
            assert_eq!(prepared.tasks.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), expected, "{id}");
        }
    }
}

#[test]
#[ignore = "requires explicit morrow-core/fault-injection; subprocess loss-of-outcome qualification"]
fn actual_store_process_loss_is_all_or_nothing_and_reconciles_the_same_create() {
    if let Some(path) = std::env::var_os("HMOS_CREATE_TODOS_CRASH_DB") {
        let mut engine = Engine::open(std::path::Path::new(&path)).unwrap();
        let _ = engine.execute(request(wire("one\ntwo\nthree"))).unwrap();
        panic!("fault-injection did not terminate the child");
    }
    for boundary in ["after-begin", "after-card", "after-operation", "after-event", "after-task-evidence", "before-commit", "after-commit"] {
        let (_directory, path, engine) = engine(); drop(engine);
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "create_todos::tests::actual_store_process_loss_is_all_or_nothing_and_reconciles_the_same_create", "--ignored", "--nocapture"])
            .env("HMOS_CREATE_TODOS_CRASH_DB", &path).env("MORROW_TEST_CRASH_AT", boundary).output().unwrap();
        assert_eq!(child.status.code(), Some(86), "{boundary}: {}", String::from_utf8_lossy(&child.stderr));
        let mut engine = Engine::open(&path).unwrap();
        let committed = boundary == "after-commit";
        let lookup = engine.host.store_local().lookup_for_card(CARD, OPERATION).unwrap();
        assert_eq!(matches!(lookup, Lookup::Committed(_)), committed, "{boundary}");
        let card = engine.host.store_local().card(CARD).unwrap();
        assert_eq!(card.is_some(), committed, "{boundary}");
        assert_eq!(engine.host.store_local().pending_usage().unwrap().0, u64::from(committed), "{boundary}");
        if let Some(card) = card { assert_eq!(tasks_v2::decode(CARD, &card.summary().title, &card.body()).unwrap().tasks.len(), 3); }
        let reply = engine.execute(request(wire("one\ntwo\nthree"))).unwrap();
        assert_eq!(reply.receipt_revision, "1"); assert_eq!(reply.cards[0].tasks.len(), 3);
        assert_eq!(engine.host.store_local().pending_usage().unwrap().0, 1);
        let original = original_tasks(&engine);
        assert_eq!(original.len(), 3);
        assert_eq!(reply.cards[0].tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), original.iter().map(|t| t.id.as_str()).collect::<Vec<_>>());
        engine.host.store_local().integrity_check().unwrap();
    }
}
