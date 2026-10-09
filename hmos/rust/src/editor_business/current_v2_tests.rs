use super::*;

fn mutate_current(e: &mut Engine, action: &str, operation: &str, fields: Value) {
    let mut request = fields;
    request["action"] = json!(action);
    request["operation"] = json!(operation);
    request["id"] = json!(CARD);
    request["source"] = json!(hex(&actual(e).encode()));
    e.execute(super::request(request)).unwrap();
}
fn current_business(e: &Engine, operation: &str) -> Value {
    let card = actual(e);
    let p = tasks_v2::decode(CARD, &card.summary().title, &card.body()).unwrap();
    let mut value = business(operation, &hex(&card.encode()), "");
    value["title"] = json!("current V2 edited title");
    value["description"] = json!("current V2 edited 正文 🧪 é.");
    value["category"] = json!(p.category);
    value["stage"] = json!(p.stage);
    value
}
fn current_wire(e: &mut Engine, operation: &str, draft: &str) -> String {
    let b = current_business(e, operation);
    let p = publish(e, &b, draft, &format!("{draft}-save"));
    submission("current_v2", b, p, Value::Null)
}
fn native_run(e: &mut Engine, value: Value) -> Value {
    serde_json::to_value(e.execute(request(value)).unwrap()).unwrap()
}
fn issued_wire(e: &mut Engine, wire: &str, operation: &str) -> (Value, String) {
    let prepared = native_run(
        e,
        json!({"action":"editor_intent_prepare","editor_intent":{"operation_id":operation,"request_json":wire}}),
    );
    let proof = prepared["editor_intents"][0]["proof"].clone();
    native_run(
        e,
        json!({"action":"editor_intent_issue","editor_intent_issue":{"intent":proof,"expected_generation":"1"}}),
    );
    let read = native_run(
        e,
        json!({"action":"editor_intent_read","editor_intent_ref":{"intent_id":proof["intent_id"],"prepare_operation":proof["prepare_operation"],"part":"save"}}),
    );
    (
        proof,
        read["editor_intents"][0]["save_request_json"]
            .as_str()
            .unwrap()
            .into(),
    )
}
fn exact_outer(e: &mut Engine, wire: &str) -> Result<Reply> {
    let mut value: Request = serde_json::from_str(wire).unwrap();
    value.transport_json = wire.into();
    e.execute(value)
}

#[test]
fn current_v2_edits_actual_revision_after_metadata_and_task_commands_without_lf_replacement() {
    let (_dir, path, mut e) = fixture();
    let (_root, first) = strict_root(&mut e, "one\ntwo\nthree");
    let tasks = first["historical_card"]["tasks"].as_array().unwrap();
    let one = tasks[0]["id"].as_str().unwrap();
    let two = tasks[1]["id"].as_str().unwrap();
    let three = tasks[2]["id"].as_str().unwrap();
    mutate_current(&mut e, "favorite", "current-favorite", json!({"flag":true}));
    mutate_current(
        &mut e,
        "category",
        "current-category",
        json!({"category":"实验","stage":"待验证"}),
    );
    mutate_current(
        &mut e,
        "task_rename",
        "current-rename",
        json!({"task_id":one,"text":"renamed one"}),
    );
    mutate_current(
        &mut e,
        "task_toggle",
        "current-completion",
        json!({"task_id":two,"flag":true}),
    );
    mutate_current(
        &mut e,
        "task_reorder",
        "current-order",
        json!({"order":[three,two,one]}),
    );
    mutate_current(
        &mut e,
        "task_remove",
        "current-retire",
        json!({"task_id":three}),
    );
    let before = actual(&e);
    assert_eq!(before.summary().revision, 7);
    let wire = current_wire(&mut e, "current-body-save", "current-body-draft");
    let (proof, outer) = issued_wire(&mut e, &wire, "current-body-prepare");
    let usage_before = usage(&e);
    let reply = exact_outer(&mut e, &outer).unwrap();
    assert_eq!(reply.receipt_revision, "8");
    assert_eq!(usage(&e).0, usage_before.0 + 1);
    let changed = actual(&e);
    assert_eq!(changed.summary().title, "current V2 edited title");
    assert_eq!(
        tasks_v2::decode(CARD, &changed.summary().title, &changed.body())
            .unwrap()
            .description,
        "current V2 edited 正文 🧪 é."
    );
    assert_ne!(changed.summary().title, before.summary().title);
    for tag in [3, 4, 7, 20, 21, 22, create_todos::RAW_IDENTITY_FIELD] {
        assert_eq!(
            raw_payloads(&changed.body(), tag),
            raw_payloads(&before.body(), tag),
            "property {tag}"
        );
    }
    let p = tasks_v2::decode(CARD, &changed.summary().title, &changed.body()).unwrap();
    assert!(p.favorite);
    assert_eq!(p.tasks[0].id, two);
    assert_eq!(p.tasks[0].completion, 1);
    assert_eq!(p.tasks[1].id, one);
    assert_eq!(p.tasks[1].text, "renamed one");
    assert_eq!(p.retired_task_ids, [three]);
    assert_eq!(
        marker(&changed.body()).unwrap().unwrap().mode,
        Mode::CurrentV2
    );
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let usage_before = usage(&e);
    assert_eq!(exact_outer(&mut e, &outer).unwrap().receipt_revision, "8");
    assert_eq!(
        inspect(&mut e, &wire, "8")
            .unwrap()
            .editor_commit
            .unwrap()
            .live_matches,
        true
    );
    assert_eq!(usage(&e), usage_before);
    assert_eq!(actual(&e).encode(), changed.encode());
    let _ = proof;
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn current_v2_full_source_cas_does_not_rebase_after_issue_or_grant_owned_lf_authority() {
    let (_dir, path, mut e) = fixture();
    let (root, first) = strict_root(&mut e, "one\ntwo");
    mutate_current(
        &mut e,
        "favorite",
        "metadata-after-owned",
        json!({"flag":true}),
    );
    let wire = current_wire(&mut e, "cas-body", "cas-body-draft");
    let (_proof, outer) = issued_wire(&mut e, &wire, "cas-prepare");
    mutate_current(
        &mut e,
        "task_rename",
        "task-after-body-issue",
        json!({"task_id":first["historical_card"]["tasks"][0]["id"],"text":"foreign rename"}),
    );
    let current = actual(&e).encode();
    let before = usage(&e);
    assert!(exact_outer(&mut e, &outer).is_err());
    assert_eq!(e.effect, "not_committed");
    assert_eq!(actual(&e).encode(), current);
    assert_eq!(usage(&e), before);
    assert!(
        !inspect(&mut e, &root, "1")
            .unwrap()
            .editor_commit
            .unwrap()
            .live_matches
    );
    let mut b = current_business(&e, "old-root-continue");
    b["todos"] = json!("one\ntwo\nnew");
    let publication = publish(&mut e, &b, "old-root-draft", "old-root-draft-save");
    let old_baseline = submission("continued_todos", b, publication, context(&root, &first));
    let before = usage(&e);
    assert!(save(&mut e, &old_baseline).is_err());
    assert_eq!(usage(&e), before);
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    assert!(exact_outer(&mut e, &outer).is_err());
    assert_eq!(actual(&e).encode(), current);
    assert_eq!(usage(&e), before);
}

#[test]
fn current_v2_strict_schema_metadata_and_publication_refuse_without_task_loss() {
    let (_dir, _path, mut e) = fixture();
    strict_root(&mut e, "one\ntwo");
    let wire = current_wire(&mut e, "validation-body", "validation-draft");
    let original = actual(&e).encode();
    for (field, value) in [
        ("todos", json!("one\ntwo")),
        ("category", json!("实验")),
        ("stage", json!("计划中")),
    ] {
        let mut b = current_business(&e, &format!("invalid-{field}"));
        b[field] = value;
        let p = publish(
            &mut e,
            &b,
            &format!("invalid-{field}-draft"),
            &format!("invalid-{field}-pub"),
        );
        let changed = submission("current_v2", b, p, Value::Null);
        let before = usage(&e);
        assert!(save(&mut e, &changed).is_err(), "{field}");
        assert_eq!(usage(&e), before);
        assert_eq!(actual(&e).encode(), original);
    }
    let mut modified: Value = serde_json::from_str(&wire).unwrap();
    modified["continuation"] = context(&wire, &json!({}));
    assert!(save(&mut e, &serde_json::to_string(&modified).unwrap()).is_err());
    let mut modified: Value = serde_json::from_str(&wire).unwrap();
    modified["publication"]["request_sha256"] = json!("0".repeat(64));
    assert!(save(&mut e, &serde_json::to_string(&modified).unwrap()).is_err());
    let mut modified: Value = serde_json::from_str(&wire).unwrap();
    modified["mode"] = json!("edit");
    assert_eq!(
        save(&mut e, &serde_json::to_string(&modified).unwrap()).unwrap_err(),
        "EditorOwnedTodosRequireContinuation"
    );
    let before = usage(&e);
    let reply = save(&mut e, &wire).unwrap();
    assert_eq!(reply.receipt_revision, "2");
    assert_eq!(usage(&e).0, before.0 + 1);
    assert!(save(&mut e, &format!(" {wire}")).is_err());
    assert_eq!(e.effect, "committed");
    assert_eq!(
        raw_payloads(&actual(&e).body(), 20),
        raw_payloads(&CardRecord::decode(&original).unwrap().body(), 20)
    );
}

#[test]
fn current_v2_preserves_nested_task_asset_outer_and_unrelated_unknown_bytes() {
    let (_dir, path, mut e) = fixture();
    let blob_bytes = b"actual current-v2 attachment\0\xff";
    let blob = e
        .host
        .store_local_mut()
        .stage_blob(
            &mut std::io::Cursor::new(blob_bytes),
            blob_bytes.len() as u64,
            Some(hash(blob_bytes)),
            1,
        )
        .unwrap();
    let attachment = Attachment {
        id: blob.id.clone(),
        display_name: "current-proof.bin".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob_bytes.len() as u64,
        sha256: hash(blob_bytes),
    };
    let mut body = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        favorite: true,
        retired_task_ids: vec!["retired-original".into()],
        ..Default::default()
    }
    .encode_to_vec();
    for (id, label, completion) in [("same-1", "duplicate", 1), ("same-2", "duplicate", 0)] {
        let mut task = tasks_v2::Task {
            id: id.into(),
            text: label.into(),
            completion,
            ..Default::default()
        }
        .encode_to_vec();
        task.extend(length_field(50101, b"nested opaque task bytes"));
        body.extend(length_field(20, &task));
    }
    body.extend(length_field(50002, b"unknown property bytes"));
    let asset = [
        length_field(1, blob.id.as_bytes()),
        length_field(2, b"current-proof.bin"),
        length_field(3, b"file"),
        varint_field(4, blob_bytes.len() as u64),
        length_field(50102, b"nested opaque asset bytes"),
    ]
    .concat();
    body.extend(length_field(10, &asset));
    // An unrelated/copy marker is never a todos baseline; current source/CAS
    // permits task-preserving body editing without claiming historical ownership.
    body = set_marker(
        &body,
        &Marker {
            mode: Mode::Create,
            wire: [3; 32],
            publication: [4; 32],
        },
    )
    .unwrap();
    let base =
        CardRecord::new_with_attachments(CARD, "idea", 2, "before", body, &[attachment]).unwrap();
    let mut outer = base.encode();
    outer.extend(length_field(50004, b"unknown outer bytes"));
    let base = CardRecord::decode(&outer).unwrap();
    core_create(&mut e, "opaque-seed", &base);
    let b = current_business(&e, "opaque-current-body");
    let p = publish_assets(
        &mut e,
        &b,
        "opaque-current-draft",
        "opaque-current-draft-save",
        json!([{"origin":0,"asset_id":blob.id,"aliases":["opaque asset alias"]}]),
    );
    let wire = submission("current_v2", b, p, Value::Null);
    save(&mut e, &wire).unwrap();
    let changed = actual(&e);
    for tag in [3, 4, 7, 10, 20, 21, 22, 50002] {
        assert_eq!(
            raw_payloads(&changed.body(), tag),
            raw_payloads(&base.body(), tag)
        );
    }
    assert_eq!(
        raw_payloads(&changed.encode(), 8),
        raw_payloads(&base.encode(), 8)
    );
    assert_eq!(
        raw_payloads(&changed.encode(), 50004),
        raw_payloads(&base.encode(), 50004)
    );
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    assert_eq!(
        inspect(&mut e, &wire, "2")
            .unwrap()
            .editor_commit
            .unwrap()
            .qualification,
        "development_editor_wire_v1"
    );
    assert_eq!(usage(&e), before);
    e.host.store_local().integrity_check().unwrap();
    let mut exported = vec![];
    e.host
        .store_local()
        .export_attachment_local(CARD, &blob.id, &mut exported)
        .unwrap();
    assert_eq!(exported, blob_bytes);
}

#[test]
fn current_v2_event_capacity_unknown_keeps_original_wire_and_exact_retry_after_reopen() {
    let (_dir, path, mut e) = fixture();
    strict_root(&mut e, "one\ntwo");
    let wire = current_wire(&mut e, "bounded-current-body", "bounded-current-draft");
    let before = actual(&e).encode();
    drop(e);
    let store = Store::open(
        &path,
        EventBudget {
            max_count: 0,
            max_bytes: 0,
        },
    )
    .unwrap();
    let mut e = Engine {
        host: HostRuntime::new(store).unwrap(),
        start: std::time::Instant::now(),
        effect: "not_committed",
    };
    assert!(save(&mut e, &wire).is_err());
    assert_eq!(e.effect, "unknown");
    assert_eq!(actual(&e).encode(), before);
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    assert_eq!(save(&mut e, &wire).unwrap().receipt_revision, "2");
    let before = usage(&e);
    assert_eq!(save(&mut e, &wire).unwrap().receipt_revision, "2");
    assert_eq!(usage(&e), before);
}

#[test]
#[ignore = "host-only actual subprocess Core faults require morrow-core/fault-injection"]
fn current_v2_actual_store_process_loss_preserves_tasks_and_fixed_retry() {
    if let Some(path) = std::env::var_os("HMOS_CURRENT_V2_CRASH_DB") {
        let wire = std::fs::read_to_string(std::env::var_os("HMOS_CURRENT_V2_CRASH_WIRE").unwrap())
            .unwrap();
        let mut e = Engine::open(std::path::Path::new(&path)).unwrap();
        save(&mut e, &wire).unwrap();
        panic!("fault feature missing");
    }
    for boundary in [
        "after-begin",
        "after-card",
        "after-operation",
        "after-event",
        "after-task-evidence",
        "before-commit",
        "after-commit",
    ] {
        let (dir, path, mut e) = fixture();
        strict_root(&mut e, "one\ntwo");
        let task_id = card_view(&actual(&e)).unwrap().tasks[0].id.clone();
        mutate_current(
            &mut e,
            "task_toggle",
            "crash-current-completion",
            json!({"task_id":task_id,"flag":true}),
        );
        let wire = current_wire(&mut e, "crash-current-body", "crash-current-draft");
        let value = parse(&wire).unwrap();
        let expected = prepare(&e, &value, &wire, false, true).unwrap();
        let before = actual(&e).encode();
        let before_usage = usage(&e);
        let wirepath = dir.path().join("fixed-current-v2.json");
        std::fs::write(&wirepath, &wire).unwrap();
        drop(e);
        let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","editor_business::tests::current_v2_tests::current_v2_actual_store_process_loss_preserves_tasks_and_fixed_retry","--ignored","--nocapture"]).env("HMOS_CURRENT_V2_CRASH_DB",&path).env("HMOS_CURRENT_V2_CRASH_WIRE",&wirepath).env("MORROW_TEST_CRASH_AT",boundary).output().unwrap();
        assert_eq!(
            child.status.code(),
            Some(86),
            "{boundary}: {}",
            String::from_utf8_lossy(&child.stderr)
        );
        let mut e = Engine::open(&path).unwrap();
        let committed = boundary == "after-commit";
        assert_eq!(
            actual(&e).encode(),
            if committed {
                expected.result.encode()
            } else {
                before
            }
        );
        assert_eq!(usage(&e).0, before_usage.0 + u64::from(committed));
        assert_eq!(save(&mut e, &wire).unwrap().receipt_revision, "3");
        assert_eq!(actual(&e).encode(), expected.result.encode());
        assert_eq!(usage(&e).0, before_usage.0 + 1);
        assert_eq!(save(&mut e, &wire).unwrap().receipt_revision, "3");
        assert_eq!(usage(&e).0, before_usage.0 + 1);
        e.host.store_local().integrity_check().unwrap();
        println!("current_v2/{boundary}: PASS");
    }
}

#[test]
#[ignore = "explicit complete current-v2 actual Store DTO export requires HMOS_CURRENT_V2_DTO_FIXTURE"]
fn export_current_v2_actual_store_dto_fixture() {
    let (_dir, path, mut e) = fixture();
    let (root, first) = strict_root(&mut e, "one\ntwo\nthree");
    mutate_current(&mut e, "favorite", "fixture-favorite", json!({"flag":true}));
    mutate_current(
        &mut e,
        "category",
        "fixture-category",
        json!({"category":"实验","stage":"待验证"}),
    );
    mutate_current(
        &mut e,
        "task_rename",
        "fixture-rename",
        json!({"task_id":first["historical_card"]["tasks"][0]["id"],"text":"renamed step"}),
    );
    mutate_current(
        &mut e,
        "task_toggle",
        "fixture-completion",
        json!({"task_id":first["historical_card"]["tasks"][1]["id"],"flag":true}),
    );
    let source = serde_json::to_value(card_view(&actual(&e)).unwrap()).unwrap();
    let wire = current_wire(&mut e, "fixture-current-body", "fixture-current-draft");
    let publication = native_run(
        &mut e,
        json!({"action":"draft_read","id":CARD,"draft_id":"fixture-current-draft"}),
    )["drafts"][0]
        .clone();
    let (proof, outer) = issued_wire(&mut e, &wire, "fixture-current-prepare");
    let issued_parts=["submission","publication","save","inspect","close"].into_iter().map(|part|{
        native_run(&mut e,json!({"action":"editor_intent_read","editor_intent_ref":{"intent_id":proof["intent_id"],"prepare_operation":proof["prepare_operation"],"part":part}}))["editor_intents"][0].clone()
    }).collect::<Vec<_>>();
    let saved = serde_json::to_value(exact_outer(&mut e, &outer).unwrap()).unwrap();
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let inspected = serde_json::to_value(inspect(&mut e, &wire, "6").unwrap()).unwrap();
    let root_inspected = serde_json::to_value(inspect(&mut e, &root, "1").unwrap()).unwrap();
    let fixture = json!({"schema_version":1,"kind":"actual_store_current_v2_v26","card_id":CARD,"current_source":source,
        "original_create_request_json":root,"original_create_commit":first,"publication":publication,"current_request_json":wire,
        "proof":proof,"issued_parts":issued_parts,"registered_save_request_json":outer,"saved_reply":saved,
        "reopened_inspect_reply":inspected,"original_create_inspect_after_mutations_reply":root_inspected});
    let output = std::env::var_os("HMOS_CURRENT_V2_DTO_FIXTURE").expect("explicit output required");
    std::fs::write(output, serde_json::to_vec_pretty(&fixture).unwrap()).unwrap();
}
