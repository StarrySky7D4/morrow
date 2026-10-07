use super::*;
use morrow_core::{
    dispatch::HostRuntime,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use serde_json::{Value, json};

const CARD: &str = "strict-editor-card";
const ROOT_OP: &str = "strict-original-create";
fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Engine) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let engine = Engine::open(&path).unwrap();
    (directory, path, engine)
}
fn text(value: &str) -> Value {
    json!({"text":value,"selection_base":-1,"selection_extent":-1,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1})
}
fn business(operation: &str, source: &str, raw: &str) -> Value {
    json!({"action":if source.is_empty(){"create"}else{"edit"},"id":CARD,"operation":operation,"source":source,
        "title":"strict title","description":"raw body 汉字 🧪 é.","hypothesis":"hypothesis","conclusion":"conclusion",
        "todos":raw,"category":"灵感","stage":"待整理"})
}
fn publish(engine: &mut Engine, business: &Value, draft: &str, save: &str) -> Value {
    publish_assets(engine, business, draft, save, json!([]))
}
fn publish_assets(
    engine: &mut Engine,
    business: &Value,
    draft: &str,
    save: &str,
    assets: Value,
) -> Value {
    let source = business["source"].as_str().unwrap();
    let revision = if source.is_empty() {
        "0".to_owned()
    } else {
        CardRecord::decode(&unhex(source).unwrap())
            .unwrap()
            .summary()
            .revision
            .to_string()
    };
    let record = engine.execute(request(json!({"action":"draft_save","draft":{"card_id":CARD,
        "draft_id":draft,"operation_id":save,"expected_generation":"0","source_revision":revision,
        "source_kind":if source.is_empty(){1}else{0},"assets":assets,
        "values":{"title":text(business["title"].as_str().unwrap()),"description":text(business["description"].as_str().unwrap()),
            "hypothesis":text(business["hypothesis"].as_str().unwrap()),"conclusion":text(business["conclusion"].as_str().unwrap()),
            "todos":text(business["todos"].as_str().unwrap()),"category":business["category"],"stage":business["stage"]}}}))).unwrap().drafts.remove(0);
    let view = serde_json::to_value(record).unwrap();
    json!({"draft_id":draft,"generation":view["generation"],"save_operation":save,"request_sha256":view["request_sha256"]})
}
fn submission(mode: &str, business: Value, publication: Value, context: Value) -> String {
    serde_json::to_string(&json!({"schema_version":1,"mode":mode,"business":business,"publication":publication,"continuation":context})).unwrap()
}
fn save(engine: &mut Engine, wire: &str) -> Result<Reply> {
    engine.execute(request(
        json!({"action":"editor_save","editor_save":{"request_json":wire}}),
    ))
}
fn inspect(engine: &mut Engine, wire: &str, revision: &str) -> Result<Reply> {
    engine.execute(request(json!({"action":"editor_commit_inspect","editor_commit":{"request_json":wire,"expected_revision":revision}})))
}
fn strict_root(engine: &mut Engine, raw: &str) -> (String, Value) {
    let b = business(ROOT_OP, "", raw);
    let p = publish(engine, &b, "strict-root-draft", "strict-root-save");
    let wire = submission("create", b, p, Value::Null);
    let saved = serde_json::to_value(save(engine, &wire).unwrap()).unwrap();
    (wire, saved["editor_commit"].clone())
}
fn context(root: &str, view: &Value) -> Value {
    json!({"root_request_json":root,"baseline":{"operation":view["operation"],"revision":view["revision"],
        "command_sha256":view["command_sha256"],"content_sha256":view["content_sha256"],"request_sha256":view["request_sha256"],
        "publication_sha256":view["publication_sha256"]}})
}
fn continued(
    engine: &mut Engine,
    root: &str,
    baseline: &Value,
    operation: &str,
    draft: &str,
    raw: &str,
) -> String {
    let b = business(
        operation,
        baseline["historical_card"]["source"].as_str().unwrap(),
        raw,
    );
    let p = publish(engine, &b, draft, &format!("{draft}-save"));
    submission("continued_todos", b, p, context(root, baseline))
}
fn actual(engine: &Engine) -> CardRecord {
    engine.host.store_local().card(CARD).unwrap().unwrap()
}
fn usage(engine: &Engine) -> (u64, u64) {
    engine.host.store_local().pending_usage().unwrap()
}

#[test]
fn strict_create_marker_and_full_historical_receipt_are_one_actual_store_commit() {
    let (_directory, path, mut engine) = fixture();
    let before = usage(&engine);
    let (wire, view) = strict_root(&mut engine, " beta \nalpha\nbeta\n\n😀");
    assert_eq!(usage(&engine).0 - before.0, 2); // one raw journal + one business command
    assert_eq!(view["qualification"], "development_editor_wire_v1");
    assert_eq!(view["source_revision"], "0");
    assert_eq!(view["request_sha256"], hex(&hash(wire.as_bytes())));
    assert_eq!(view["live_matches"], true);
    assert_eq!(view["revision"], "1");
    assert_eq!(
        view["historical_card"]["tasks"].as_array().unwrap().len(),
        3
    );
    let original = historical(&engine, CARD, ROOT_OP).unwrap().unwrap();
    assert_eq!(
        view["content_sha256"],
        hex(&hash(&original.result.encode()))
    );
    assert_eq!(original.result.encode(), actual(&engine).encode());
    assert!(marker(&original.result.body()).unwrap().is_some());
    let before = usage(&engine);
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    assert_eq!(
        serde_json::to_value(save(&mut engine, &wire).unwrap()).unwrap()["editor_commit"],
        view
    );
    assert_eq!(
        serde_json::to_value(inspect(&mut engine, &wire, "1").unwrap()).unwrap()["editor_commit"],
        view
    );
    assert_eq!(usage(&engine), before);
    engine.host.store_local().integrity_check().unwrap();
}

#[test]
fn historical_inspect_and_exact_retry_never_mix_current_advanced_or_retired_raw_values() {
    let (_directory, path, mut engine) = fixture();
    let (wire, view) = strict_root(&mut engine, "one\ntwo");
    let source = view["historical_card"]["source"].as_str().unwrap();
    engine.execute(request(json!({"action":"favorite","id":CARD,"operation":"foreign-favorite","source":source,"flag":true}))).unwrap();
    engine.execute(request(json!({"action":"draft_discard","id":CARD,"draft_id":"strict-root-draft","generation":"1","operation":"strict-root-discard"}))).unwrap();
    let current = actual(&engine).encode();
    let before = usage(&engine);
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    for reply in [
        inspect(&mut engine, &wire, "1").unwrap(),
        save(&mut engine, &wire).unwrap(),
    ] {
        assert!(reply.cards.is_empty());
        let checked = serde_json::to_value(reply.editor_commit.unwrap()).unwrap();
        assert_eq!(checked["historical_card"], view["historical_card"]);
        assert_eq!(checked["revision"], "1");
        assert_eq!(checked["live_matches"], false);
        assert_eq!(checked["live_revision"], "2");
    }
    assert_eq!(actual(&engine).encode(), current);
    assert_eq!(usage(&engine), before);
}

#[test]
fn byte_identical_original_json_and_exact_publication_proof_are_required_after_commit() {
    let (_directory, _path, mut engine) = fixture();
    let (wire, _) = strict_root(&mut engine, "one\none");
    let original = actual(&engine).encode();
    let before = usage(&engine);
    assert!(save(&mut engine, &format!(" {wire}")).is_err());
    assert_eq!(engine.effect, "committed");
    for (path, changed) in [
        ("raw", json!(" one\none")),
        ("proof", json!("0".repeat(64))),
        ("malformed", json!("bad")),
    ] {
        let mut value: Value = serde_json::from_str(&wire).unwrap();
        if path == "raw" {
            value["business"]["todos"] = changed;
        } else {
            value["publication"]["request_sha256"] = changed;
        }
        assert!(save(&mut engine, &serde_json::to_string(&value).unwrap()).is_err());
        assert_eq!(engine.effect, "committed", "{path}");
    }
    assert!(inspect(&mut engine, &wire, "2").is_err());
    assert_eq!(engine.effect, "committed");
    assert_eq!(actual(&engine).encode(), original);
    assert_eq!(usage(&engine), before);
}

#[test]
fn continued_todos_full_field_order_membership_and_retirement_are_one_business_transaction() {
    let (_directory, path, mut engine) = fixture();
    let (root, first) = strict_root(&mut engine, "one\ntwo\nthree");
    let ids: Vec<String> = first["historical_card"]["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap().into())
        .collect();
    let wire = continued(
        &mut engine,
        &root,
        &first,
        "strict-continue-1",
        "continue-draft-1",
        " three \nfour\none\nfour\n",
    );
    let before = usage(&engine);
    let second =
        serde_json::to_value(save(&mut engine, &wire).unwrap()).unwrap()["editor_commit"].clone();
    assert_eq!(usage(&engine).0 - before.0, 1);
    assert_eq!(second["revision"], "2");
    let tasks = second["historical_card"]["tasks"].as_array().unwrap();
    assert_eq!(
        tasks
            .iter()
            .map(|t| t["text"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["three", "four", "one"]
    );
    assert_eq!(tasks[0]["id"], ids[2]);
    assert_eq!(tasks[2]["id"], ids[0]);
    let new_id = tasks[1]["id"].as_str().unwrap();
    assert!(new_id.starts_with("task-hmos-continued-"));
    let p = tasks_v2::decode(
        CARD,
        &actual(&engine).summary().title,
        &actual(&engine).body(),
    )
    .unwrap();
    assert_eq!(p.retired_task_ids, [ids[1].clone()]);
    assert!(p.tasks.iter().all(|t| t.completion == 0));
    let wire3 = continued(
        &mut engine,
        &root,
        &second,
        "strict-continue-2",
        "continue-draft-2",
        "two\nfour",
    );
    let third =
        serde_json::to_value(save(&mut engine, &wire3).unwrap()).unwrap()["editor_commit"].clone();
    assert_ne!(third["historical_card"]["tasks"][0]["id"], ids[1]);
    assert_eq!(third["historical_card"]["tasks"][1]["id"], new_id);
    let live = actual(&engine).encode();
    let before = usage(&engine);
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    assert_eq!(save(&mut engine, &wire).unwrap().receipt_revision, "2");
    assert_eq!(actual(&engine).encode(), live);
    assert_eq!(usage(&engine), before);
}

#[test]
fn changed_continuation_baseline_or_root_never_mutates_and_current_is_not_a_rebase() {
    let (_directory, _path, mut engine) = fixture();
    let (root, first) = strict_root(&mut engine, "one");
    let wire = continued(
        &mut engine,
        &root,
        &first,
        "strict-bad-proof",
        "bad-proof-draft",
        "one\ntwo",
    );
    let before = usage(&engine);
    let original = actual(&engine).encode();
    for name in [
        "operation",
        "revision",
        "command_sha256",
        "content_sha256",
        "request_sha256",
        "publication_sha256",
    ] {
        let mut changed: Value = serde_json::from_str(&wire).unwrap();
        changed["continuation"]["baseline"][name] = json!(if name == "operation" {
            "wrong-operation".into()
        } else if name == "revision" {
            "2".into()
        } else {
            "0".repeat(64)
        });
        assert!(
            save(&mut engine, &serde_json::to_string(&changed).unwrap()).is_err(),
            "{name}"
        );
        assert_eq!(actual(&engine).encode(), original);
        assert_eq!(usage(&engine), before);
    }
    let mut changed: Value = serde_json::from_str(&wire).unwrap();
    changed["continuation"]["root_request_json"] = json!(format!(" {root}"));
    assert!(save(&mut engine, &serde_json::to_string(&changed).unwrap()).is_err());
    assert_eq!(usage(&engine), before);
    engine.execute(request(json!({"action":"favorite","id":CARD,"operation":"foreign-before-continuation","source":hex(&original),"flag":true}))).unwrap();
    let current = actual(&engine).encode();
    let before = usage(&engine);
    assert!(save(&mut engine, &wire).is_err());
    assert_eq!(engine.effect, "not_committed");
    assert_eq!(actual(&engine).encode(), current);
    assert_eq!(usage(&engine), before);
}

#[test]
fn copied_marker_foreign_task_cannot_become_owned_baseline_or_strict_edit() {
    let (_directory, _path, mut engine) = fixture();
    let (root, first) = strict_root(&mut engine, "one");
    engine.execute(request(json!({"action":"task_add","id":CARD,"operation":"foreign-task","source":first["historical_card"]["source"],"task_id":"foreign-task-id","text":"foreign"}))).unwrap();
    let foreign = historical(&engine, CARD, "foreign-task").unwrap().unwrap();
    let identity = marker(&foreign.result.body()).unwrap().unwrap();
    let forged = json!({"operation":"foreign-task","revision":"2","command_sha256":hex(&hash(&foreign.command)),
        "content_sha256":hex(&foreign.receipt.content_sha256),"request_sha256":hex(&identity.wire),"publication_sha256":hex(&identity.publication),
        "historical_card":{"source":hex(&foreign.result.encode())}});
    let wire = continued(
        &mut engine,
        &root,
        &forged,
        "foreign-continue",
        "foreign-continue-draft",
        "one\nforeign",
    );
    let before = usage(&engine);
    assert!(save(&mut engine, &wire).is_err());
    assert_eq!(usage(&engine), before);
    let b = business("foreign-strict-edit", &hex(&foreign.result.encode()), "");
    let p = publish(&mut engine, &b, "foreign-edit-draft", "foreign-edit-save");
    let before = usage(&engine);
    assert_eq!(
        save(&mut engine, &submission("edit", b, p, Value::Null)).unwrap_err(),
        "EditorOwnedTodosRequireContinuation"
    );
    assert_eq!(usage(&engine), before);
}

#[test]
fn legacy_history_is_semantic_only_and_absence_inspection_is_read_only() {
    let (_directory, _path, mut engine) = fixture();
    let b = business(ROOT_OP, "", "one\ntwo");
    let p = publish(&mut engine, &b, "legacy-draft", "legacy-save");
    let mut old = b.clone();
    old["draft_id"] = p["draft_id"].clone();
    old["generation"] = p["generation"].clone();
    old["draft_operation"] = p["save_operation"].clone();
    engine.execute(request(old)).unwrap();
    let wire = submission("create", b, p, Value::Null);
    let before = usage(&engine);
    let view =
        serde_json::to_value(inspect(&mut engine, &wire, "1").unwrap()).unwrap()["editor_commit"]
            .clone();
    assert_eq!(view["qualification"], "legacy_semantic_only");
    assert_eq!(view["request_sha256"], "");
    assert_eq!(view["publication_sha256"], "");
    assert_eq!(
        save(&mut engine, &wire).unwrap_err(),
        "EditorLegacyCommitUnqualified"
    );
    assert_eq!(engine.effect, "committed");
    assert_eq!(usage(&engine), before);
    let mut changed: Value = serde_json::from_str(&wire).unwrap();
    changed["business"]["operation"] = json!("never-submitted");
    let view = inspect(&mut engine, &serde_json::to_string(&changed).unwrap(), "1")
        .unwrap()
        .editor_commit
        .unwrap();
    assert_eq!(view.commit_status, "absent");
    assert!(view.historical_card.is_none());
    assert!(view.revision.is_empty());
    assert_eq!(usage(&engine), before);
}

#[test]
fn event_capacity_unknown_reopens_and_retries_same_complete_original_wire() {
    let (_directory, path, mut engine) = fixture();
    let b = business(ROOT_OP, "", "one\ntwo");
    let p = publish(&mut engine, &b, "capacity-draft", "capacity-save");
    let wire = submission("create", b, p, Value::Null);
    drop(engine);
    let mut engine = Engine {
        host: HostRuntime::new(
            Store::open(
                &path,
                EventBudget {
                    max_count: 1,
                    ..Default::default()
                },
            )
            .unwrap(),
        )
        .unwrap(),
        start: std::time::Instant::now(),
        effect: "not_committed",
    };
    assert!(save(&mut engine, &wire).is_err());
    assert_eq!(engine.effect, "unknown");
    assert!(matches!(
        engine
            .host
            .store_local()
            .lookup_for_card(CARD, ROOT_OP)
            .unwrap(),
        Lookup::Absent
    ));
    assert!(engine.host.store_local().card(CARD).unwrap().is_none());
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    assert_eq!(save(&mut engine, &wire).unwrap().receipt_revision, "1");
    assert_eq!(usage(&engine).0, 2);
    assert_eq!(save(&mut engine, &wire).unwrap().receipt_revision, "1");
    assert_eq!(usage(&engine).0, 2);
}

#[test]
fn complete_final_fit_is_accepted_but_final_overflow_retains_raw_and_changes_no_business() {
    let (_directory, _path, mut engine) = fixture();
    let mut b = business(ROOT_OP, "", "one");
    b["description"] = json!("e\u{301}\u{301}".repeat(12980));
    let p = publish(&mut engine, &b, "large-root-draft", "large-root-save");
    let root = submission("create", b, p, Value::Null);
    let first =
        serde_json::to_value(save(&mut engine, &root).unwrap()).unwrap()["editor_commit"].clone();
    let large_label = "x\u{301}".repeat(330);
    let raw = format!("one\n{large_label}");
    let old = actual(&engine);
    // A sequential task-first application rejects this intermediate body.
    assert!(
        tasks_v2::apply(
            CARD,
            &old.summary().title,
            &old.body(),
            tasks_v2::Command::Add {
                id: "intermediate-only-id".into(),
                text: large_label.clone()
            }
        )
        .is_err()
    );
    let mut b = business("complete-final-fit", &hex(&old.encode()), &raw);
    for (field, value) in [
        ("title", "all fields atomically"),
        ("description", "short"),
        ("hypothesis", "next hypothesis"),
        ("conclusion", "next conclusion"),
        ("category", "进行中"),
        ("stage", "推进中"),
    ] {
        b[field] = json!(value);
    }
    let p = publish(&mut engine, &b, "final-fit-draft", "final-fit-save");
    let wire = submission("continued_todos", b.clone(), p, context(&root, &first));
    let before = usage(&engine);
    let next =
        serde_json::to_value(save(&mut engine, &wire).unwrap()).unwrap()["editor_commit"].clone();
    assert_eq!(usage(&engine).0 - before.0, 1);
    for field in [
        "title",
        "description",
        "hypothesis",
        "conclusion",
        "category",
        "stage",
    ] {
        assert_eq!(next["historical_card"][field], b[field]);
    }
    assert_eq!(
        next["historical_card"]["tasks"].as_array().unwrap().len(),
        2
    );
    let mut b = business(
        "complete-final-overflow",
        next["historical_card"]["source"].as_str().unwrap(),
        &raw,
    );
    b["description"] = json!("e\u{301}\u{301}".repeat(13000));
    let p = publish(
        &mut engine,
        &b,
        "final-overflow-draft",
        "final-overflow-save",
    );
    let wire = submission("continued_todos", b, p, context(&root, &next));
    let before = usage(&engine);
    let original = actual(&engine).encode();
    assert!(save(&mut engine, &wire).is_err());
    assert_eq!(engine.effect, "not_committed");
    assert_eq!(usage(&engine), before);
    assert_eq!(actual(&engine).encode(), original);
    assert!(matches!(
        engine
            .host
            .store_local()
            .lookup_for_card(CARD, "complete-final-overflow")
            .unwrap(),
        Lookup::Absent
    ));
    let drafts = engine
        .execute(request(json!({"action":"draft_list","id":CARD})))
        .unwrap();
    assert!(
        serde_json::to_value(drafts).unwrap()["drafts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["scope"]["draft_id"] == "final-overflow-draft")
    );
}

fn core_create(engine: &mut Engine, operation: &str, card: &CardRecord) {
    let mut connection = engine.host.connect().unwrap();
    engine
        .host
        .grant(&mut connection, GrantKind::CreateContent, CARD, 60_001, 1)
        .unwrap();
    engine
        .host
        .create_content(&connection, operation, card, || 2)
        .unwrap();
    engine.host.disconnect(&connection).unwrap();
}
fn raw_payloads(bytes: &[u8], tag: u32) -> Vec<Vec<u8>> {
    fields(bytes)
        .unwrap()
        .into_iter()
        .filter(|field| field.0 == tag)
        .map(|field| field.3.to_vec())
        .collect()
}

#[test]
fn strict_edit_retains_unknown_task_asset_outer_and_verified_attachment_bytes() {
    let (_directory, path, mut engine) = fixture();
    let bytes = b"\0immutable actual attachment\xff\n";
    let blob = engine
        .host
        .store_local_mut()
        .stage_blob(
            &mut std::io::Cursor::new(bytes),
            bytes.len() as u64,
            Some(hash(bytes)),
            1,
        )
        .unwrap();
    let attachment = Attachment {
        id: blob.id.clone(),
        display_name: "proof.bin".into(),
        media_type: "application/octet-stream".into(),
        byte_length: bytes.len() as u64,
        sha256: hash(bytes),
    };
    let task_unknown = length_field(50101, b"nested task exact bytes");
    let asset_unknown = length_field(50102, b"nested asset exact bytes");
    let mut task = tasks_v2::Task {
        id: "preserved-task-id".into(),
        text: "completed legacy-independent task".into(),
        completion: 1,
        ..Default::default()
    }
    .encode_to_vec();
    task.extend(&task_unknown);
    let mut asset = [
        length_field(1, blob.id.as_bytes()),
        length_field(2, b"proof.bin"),
        length_field(3, b"file"),
        varint_field(4, bytes.len() as u64),
    ]
    .concat();
    asset.extend(&asset_unknown);
    let mut body = tasks_v2::Properties {
        version: 2,
        description: "before".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
    .encode_to_vec();
    body.extend(length_field(20, &task));
    body.extend(length_field(10, &asset));
    body.extend(length_field(50002, b"unrelated property"));
    let card =
        CardRecord::new_with_attachments(CARD, "idea", 2, "before", body, &[attachment]).unwrap();
    let mut outer = Vec::new();
    for (tag, _, raw, payload) in fields(&card.encode()).unwrap() {
        if tag == 8 {
            let mut payload = payload.to_vec();
            payload.extend(length_field(50103, b"blobref unknown"));
            outer.extend(length_field(8, &payload));
        } else {
            outer.extend(raw);
        }
    }
    outer.extend(length_field(
        9,
        &[
            length_field(1, b"related-card-id"),
            length_field(2, b"source"),
            length_field(50104, b"relation unknown"),
        ]
        .concat(),
    ));
    outer.extend(length_field(50002, b"outer unknown"));
    let original = CardRecord::decode(&outer).unwrap();
    core_create(&mut engine, "unknown-seed", &original);
    let b = business(
        "strict-edit-unknown-preservation",
        &hex(&original.encode()),
        "",
    );
    let p = publish_assets(
        &mut engine,
        &b,
        "unknown-edit-draft",
        "unknown-edit-save",
        json!([{"origin":0,"asset_id":blob.id,"aliases":[]}]),
    );
    let wire = submission("edit", b, p, Value::Null);
    let before = usage(&engine);
    let reply = save(&mut engine, &wire).unwrap();
    assert_eq!(reply.receipt_revision, "2");
    assert_eq!(usage(&engine).0 - before.0, 1);
    let changed = actual(&engine);
    assert_eq!(raw_payloads(&changed.body(), 20), vec![task]);
    assert_eq!(raw_payloads(&changed.body(), 10), vec![asset]);
    assert_eq!(
        raw_payloads(&changed.body(), 50002),
        raw_payloads(&original.body(), 50002)
    );
    for tag in [8, 9, 50002] {
        assert_eq!(
            raw_payloads(&changed.encode(), tag),
            raw_payloads(&original.encode(), tag)
        );
    }
    assert_eq!(
        tasks_v2::decode(CARD, &changed.summary().title, &changed.body())
            .unwrap()
            .tasks[0]
            .completion,
        1
    );
    let before = usage(&engine);
    drop(engine);
    let mut engine = Engine::open(&path).unwrap();
    let mut exported = Vec::new();
    engine
        .host
        .store_local()
        .export_attachment_local(CARD, &blob.id, &mut exported)
        .unwrap();
    assert_eq!(exported, bytes);
    assert_eq!(save(&mut engine, &wire).unwrap().receipt_revision, "2");
    assert_eq!(usage(&engine), before);
    engine.host.store_local().integrity_check().unwrap();
}

#[test]
fn continued_projection_keeps_completed_task_raw_and_retired_ids_without_position_rename() {
    let unknown = length_field(50009, b"full retained task unknown");
    let mut completed = tasks_v2::Task {
        id: "complete-id".into(),
        text: "one".into(),
        completion: 1,
        ..Default::default()
    }
    .encode_to_vec();
    completed.extend(unknown);
    let removed = tasks_v2::Task {
        id: "removed-id".into(),
        text: "two".into(),
        ..Default::default()
    }
    .encode_to_vec();
    let mut body = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        retired_task_ids: vec!["old-retired-id".into()],
        ..Default::default()
    }
    .encode_to_vec();
    body.extend(length_field(20, &completed));
    body.extend(length_field(20, &removed));
    let old = CardRecord::new(CARD, "idea", 2, "before", body).unwrap();
    let r: Business = serde_json::from_value(business(
        "projection-current-op",
        &hex(&old.encode()),
        "three\none",
    ))
    .unwrap();
    let projected =
        project_todos(CARD, "before", &old.body(), ROOT_OP, &r.operation, &r.todos).unwrap();
    assert_eq!(projected.tasks[1], completed);
    assert_eq!(projected.newly_retired, ["removed-id"]);
    let result = final_properties(&old, &r, &[], Some(projected)).unwrap();
    let p = tasks_v2::decode(CARD, &r.title, &result).unwrap();
    assert_eq!(p.tasks[1].id, "complete-id");
    assert_eq!(p.tasks[1].completion, 1);
    assert_eq!(p.retired_task_ids, ["old-retired-id", "removed-id"]);
    let id = continued_id(CARD, ROOT_OP, &r.operation, &r.todos, 0);
    let mut collision = old.body();
    collision.extend(length_field(22, id.as_bytes()));
    assert_eq!(
        project_todos(CARD, "before", &collision, ROOT_OP, &r.operation, &r.todos)
            .err()
            .unwrap(),
        "EditorTaskIdAlreadyUsed"
    );
}

#[test]
fn full_grapheme_rows_task_bytes_and_reply_envelope_limits_fail_before_business_mutation() {
    for (name, field, raw) in [
        ("title61", "title", "a".repeat(61)),
        ("description20001", "description", "a".repeat(20001)),
        ("hypothesis5001", "hypothesis", "a".repeat(5001)),
        ("conclusion10001", "conclusion", "a".repeat(10001)),
        ("todos1001", "todos", "a".repeat(1001)),
        ("rows101", "todos", vec!["a"; 101].join("\n")),
        (
            "single-task2049bytes",
            "todos",
            format!("e{}", "\u{301}".repeat(1024)),
        ),
    ] {
        let (_directory, _path, mut engine) = fixture();
        let mut b = business(name, "", "");
        b[field] = json!(raw);
        let p = publish(&mut engine, &b, "limits-draft", "limits-save");
        let wire = submission("create", b, p, Value::Null);
        let before = usage(&engine);
        assert!(save(&mut engine, &wire).is_err(), "{name}");
        assert_eq!(engine.effect, "not_committed");
        assert_eq!(usage(&engine), before);
        assert!(engine.host.store_local().card(CARD).unwrap().is_none());
    }
    let (_directory, _path, mut engine) = fixture();
    let b = business("envelope-overflow", "", "");
    let p = publish(&mut engine, &b, "envelope-draft", "envelope-save");
    let mut wire = submission("create", b, p, Value::Null);
    wire.push_str(&" ".repeat(crate::LIMIT - wire.len()));
    assert_eq!(wire.len(), crate::LIMIT);
    let before = usage(&engine);
    assert_eq!(
        save(&mut engine, &wire).unwrap_err(),
        "EditorEnvelopeBytesLimit"
    );
    assert_eq!(usage(&engine), before);
}

#[test]
fn publication_full_values_composition_kind_and_outer_shape_are_not_borrowable() {
    let (_directory, _path, mut engine) = fixture();
    let b = business(ROOT_OP, "", "one");
    let p = publish(&mut engine, &b, "guard-draft", "guard-save");
    let wire = submission("create", b.clone(), p.clone(), Value::Null);
    let before = usage(&engine);
    for field in [
        "title",
        "description",
        "hypothesis",
        "conclusion",
        "todos",
        "category",
        "stage",
    ] {
        let mut altered: Value = serde_json::from_str(&wire).unwrap();
        altered["business"][field] = json!("mismatch");
        assert!(
            save(&mut engine, &serde_json::to_string(&altered).unwrap()).is_err(),
            "{field}"
        );
        assert_eq!(usage(&engine), before);
    }
    let mut wrongouter =
        json!({"action":"editor_save","id":CARD,"editor_save":{"request_json":wire}});
    assert_eq!(
        engine.execute(request(wrongouter.clone())).unwrap_err(),
        "EditorOuterFields"
    );
    wrongouter["action"] = json!("list");
    assert_eq!(
        engine.execute(request(wrongouter)).unwrap_err(),
        "EditorEnvelopeActionMismatch"
    );
    let mut lone: Value = serde_json::from_str(&wire).unwrap();
    lone["business"]["title"] = json!("UNICODEPLACEHOLDER");
    let invalid = serde_json::to_string(&lone)
        .unwrap()
        .replace("UNICODEPLACEHOLDER", "\\ud800");
    assert_eq!(
        save(&mut engine, &invalid).unwrap_err(),
        "EditorSubmissionSchema"
    );
    assert_eq!(usage(&engine), before);
    // Native raw persistence permits a complete live composition; strict
    // business publication independently refuses it rather than truncating.
    let mut draft_json = json!({"action":"draft_save","draft":{"card_id":CARD,"draft_id":"composing-draft","operation_id":"composing-save","expected_generation":"0","source_revision":"0","source_kind":1,"assets":[],
        "values":{"title":text(b["title"].as_str().unwrap()),"description":text(b["description"].as_str().unwrap()),"hypothesis":text(b["hypothesis"].as_str().unwrap()),"conclusion":text(b["conclusion"].as_str().unwrap()),"todos":text("one"),"category":"灵感","stage":"待整理"}}});
    draft_json["draft"]["values"]["title"]["composing_start"] = json!(0);
    draft_json["draft"]["values"]["title"]["composing_end"] = json!(1);
    let record =
        serde_json::to_value(engine.execute(request(draft_json)).unwrap()).unwrap()["drafts"][0]
            .clone();
    let p = json!({"draft_id":"composing-draft","generation":"1","save_operation":"composing-save","request_sha256":record["request_sha256"]});
    let before = usage(&engine);
    assert_eq!(
        save(
            &mut engine,
            &submission("create", b, p.clone(), Value::Null)
        )
        .unwrap_err(),
        "EditorPublicationComposing"
    );
    assert_eq!(usage(&engine), before);
    let (root, first) = strict_root(&mut engine, "one");
    let b = business(
        "kind1-cannot-continue",
        first["historical_card"]["source"].as_str().unwrap(),
        "one",
    );
    let wire = submission("continued_todos", b, p, context(&root, &first));
    let before = usage(&engine);
    assert!(save(&mut engine, &wire).is_err());
    assert_eq!(usage(&engine), before);
}

#[test]
fn registered_marker_collision_and_legacy_edit_inspection_scope_are_explicit() {
    let identity = Marker {
        mode: Mode::Edit,
        wire: [1; 32],
        publication: [2; 32],
    };
    let body = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    }
    .encode_to_vec();
    for bad in [
        varint_field(MARKER_FIELD, 1),
        length_field(MARKER_FIELD, b"foreign payload"),
        [
            set_marker(&body, &identity).unwrap(),
            length_field(MARKER_FIELD, b"duplicate"),
        ]
        .concat(),
    ] {
        assert!(set_marker(&bad, &identity).is_err());
    }
    let (_directory, _path, mut engine) = fixture();
    let b = business("legacy-create-for-edit", "", "");
    engine.execute(request(b)).unwrap();
    let old = actual(&engine);
    let mut b = business("legacy-edit-publication", "", "");
    b["action"] = json!("edit");
    b["source"] = json!(hex(&old.encode()));
    // The old gate accepted pending legacy todo text although business edit
    // supplied empty todos; strict historical inspection does not weaken its
    // five-field gate to grant qualification to this historical submission.
    let mut raw = b.clone();
    raw["todos"] = json!("unsubmitted todo");
    let p = publish(&mut engine, &raw, "old-edit-draft", "old-edit-save");
    let mut old_request = b.clone();
    old_request["draft_id"] = p["draft_id"].clone();
    old_request["generation"] = p["generation"].clone();
    old_request["draft_operation"] = p["save_operation"].clone();
    engine.execute(request(old_request)).unwrap();
    let before = usage(&engine);
    let wire = submission("edit", b, p, Value::Null);
    assert_eq!(
        inspect(&mut engine, &wire, "2").unwrap_err(),
        "EditorPublicationValuesMismatch"
    );
    assert_eq!(engine.effect, "committed");
    assert_eq!(usage(&engine), before);
}

#[test]
#[ignore = "requires explicit output path; exports real Store receipts for actual ETS cross-layer tests"]
fn actual_store_dto_fixture() {
    let output = std::env::var_os("HMOS_EDITOR_BUSINESS_FIXTURE").expect("explicit output path");
    let (_directory, _path, mut engine) = fixture();
    let b = business(ROOT_OP, "", " beta \nalpha\nbeta\n😀");
    let p = publish(&mut engine, &b, "fixture-root-draft", "fixture-root-save");
    let root = submission("create", b, p, Value::Null);
    let record = serde_json::to_value(
        engine
            .execute(request(
                json!({"action":"draft_read","id":CARD,"draft_id":"fixture-root-draft"}),
            ))
            .unwrap(),
    )
    .unwrap()["drafts"][0]
        .clone();
    let reply = serde_json::to_value(save(&mut engine, &root).unwrap()).unwrap();
    let wire = continued(
        &mut engine,
        &root,
        &reply["editor_commit"],
        "fixture-continued",
        "fixture-continued-draft",
        "😀\nnew\nalpha",
    );
    let continued_record = serde_json::to_value(
        engine
            .execute(request(
                json!({"action":"draft_read","id":CARD,"draft_id":"fixture-continued-draft"}),
            ))
            .unwrap(),
    )
    .unwrap()["drafts"][0]
        .clone();
    let next = serde_json::to_value(save(&mut engine, &wire).unwrap()).unwrap();
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"producer":"fresh actual Store","cases":[
        {"name":"create","request_json":root,"publication":record,"reply":reply},
        {"name":"continued_todos","request_json":wire,"publication":continued_record,"reply":next}]})).unwrap()).unwrap();
}

#[test]
#[ignore = "requires explicit morrow-core/fault-injection; fourteen real subprocess transaction boundaries"]
fn actual_store_process_loss_is_all_or_nothing_and_retries_the_original_wire() {
    if let Some(path) = std::env::var_os("HMOS_EDITOR_BUSINESS_CRASH_DB") {
        let wire =
            std::fs::read_to_string(std::env::var_os("HMOS_EDITOR_BUSINESS_CRASH_WIRE").unwrap())
                .unwrap();
        let mut engine = Engine::open(std::path::Path::new(&path)).unwrap();
        save(&mut engine, &wire).unwrap();
        panic!("fault-injection did not terminate the child");
    }
    for mode in ["create", "continued_todos"] {
        for boundary in [
            "after-begin",
            "after-card",
            "after-operation",
            "after-event",
            "after-task-evidence",
            "before-commit",
            "after-commit",
        ] {
            let (directory, path, mut engine) = fixture();
            let wire = if mode == "create" {
                let b = business(ROOT_OP, "", "one\ntwo\nthree");
                let p = publish(&mut engine, &b, "crash-create-draft", "crash-create-save");
                submission("create", b, p, Value::Null)
            } else {
                let (root, first) = strict_root(&mut engine, "one\ntwo\nthree");
                continued(
                    &mut engine,
                    &root,
                    &first,
                    "crash-continued",
                    "crash-continued-draft",
                    "three\nfour\none",
                )
            };
            let value = parse(&wire).unwrap();
            let expected = prepare(&engine, &value, &wire, false, true).unwrap();
            let before = usage(&engine);
            let prior = engine
                .host
                .store_local()
                .card(CARD)
                .unwrap()
                .map(|c| c.encode());
            let wirepath = directory.path().join("exact-original.json");
            std::fs::write(&wirepath, &wire).unwrap();
            drop(engine);
            let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","editor_business::tests::actual_store_process_loss_is_all_or_nothing_and_retries_the_original_wire","--ignored","--nocapture"])
            .env("HMOS_EDITOR_BUSINESS_CRASH_DB",&path).env("HMOS_EDITOR_BUSINESS_CRASH_WIRE",&wirepath).env("MORROW_TEST_CRASH_AT",boundary).output().unwrap();
            assert_eq!(
                child.status.code(),
                Some(86),
                "{mode}/{boundary}: {}",
                String::from_utf8_lossy(&child.stderr)
            );
            let mut engine = Engine::open(&path).unwrap();
            let committed = boundary == "after-commit";
            assert_eq!(
                matches!(
                    engine
                        .host
                        .store_local()
                        .lookup_for_card(CARD, &value.business.operation)
                        .unwrap(),
                    Lookup::Committed(_)
                ),
                committed,
                "{mode}/{boundary}"
            );
            assert_eq!(usage(&engine).0, before.0 + u64::from(committed));
            assert_eq!(
                engine
                    .host
                    .store_local()
                    .card(CARD)
                    .unwrap()
                    .map(|c| c.encode()),
                if committed {
                    Some(expected.result.encode())
                } else {
                    prior
                }
            );
            assert_eq!(
                save(&mut engine, &wire).unwrap().receipt_revision,
                expected.result.summary().revision.to_string()
            );
            assert_eq!(actual(&engine).encode(), expected.result.encode());
            assert_eq!(usage(&engine).0, before.0 + 1);
            assert_eq!(
                save(&mut engine, &wire).unwrap().receipt_revision,
                expected.result.summary().revision.to_string()
            );
            assert_eq!(usage(&engine).0, before.0 + 1);
            engine.host.store_local().integrity_check().unwrap();
        }
    }
}
