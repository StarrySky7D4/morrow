use super::*;

fn history_request(draft: &str, operation: &str, generation: &str) -> Value {
    json!({"action":"draft_read_history","id":CARD,"draft_id":draft,"draft_operation":operation,"generation":generation})
}
fn current_card(e: &Engine) -> crate::CardView {
    // Capture the authoritative current bytes directly, independently of any
    // intent marker or immutable source represented by the restored context.
    let card = e.host.store_local().card(CARD).unwrap().unwrap();
    let s = card.summary();
    let p = morrow_workbench_plugin::tasks_v2::decode(CARD, &s.title, &card.body()).unwrap();
    crate::CardView {
        id: s.id,
        revision: s.revision.to_string(),
        source: hex(&card.encode()),
        content_kind: None,
        title: s.title,
        description: p.description,
        hypothesis: p.hypothesis,
        conclusion: p.conclusion,
        category: p.category,
        stage: p.stage,
        favorite: p.favorite,
        deleted: p.deleted,
        deleted_at: p.deleted_at.to_string(),
        assets: vec![],
        tasks: p
            .tasks
            .into_iter()
            .map(|t| crate::TaskView {
                id: t.id,
                text: t.text,
                completion: t.completion,
            })
            .collect(),
    }
}

#[test]
fn readonly_history_is_exact_s2_parent_and_actual_current_flags_before_and_after_retirement() {
    let (_dir, path, mut e, _wire, p, pub_s1) = fixture();
    let parent = s2(&mut e);
    let before = usage(&e);
    let active = run(&mut e, history_request("parent", "late-S2", "2"));
    assert_eq!(active["effect"], "not_committed");
    assert_eq!(active["receipt_revision"], "");
    assert_eq!(active["drafts"][0]["values"], parent["values"]);
    assert_ne!(parent["values"], pub_s1["values"]);
    assert_eq!(
        active["drafts"][0]["request_sha256"],
        parent["request_sha256"]
    );
    assert_eq!(active["drafts"][0]["current_generation"], "2");
    assert_eq!(active["drafts"][0]["current_active"], true);
    assert_eq!(usage(&e), before);
    let h = handoff_json(&p, &parent, "S2 raw\r\n\0漢 🧪");
    let first = run(&mut e, handoff_request(&h))["drafts"][0].clone();
    run(
        &mut e,
        json!({"action":"draft_save","draft":raw("child","child-S3-recovery","1","S3 after first ACK",0,"1",json!([]))}),
    );
    let retired = retirement_request(&mut e, &p);
    let retirement = run(&mut e, retired)["drafts"][0].clone();
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    let observed = run(&mut e, history_request("parent", "late-S2", "2"))["drafts"][0].clone();
    assert_eq!(observed["generation"], "2");
    assert_eq!(observed["active"], true);
    assert_eq!(observed["current_generation"], "3");
    assert_eq!(observed["current_active"], false);
    assert_eq!(observed["values"], parent["values"]);
    assert_eq!(observed["business_retirement"], Value::Null);
    let actual_retirement =
        run(&mut e, history_request("parent", "retire", "3"))["drafts"][0].clone();
    for key in [
        "generation",
        "active",
        "current_generation",
        "current_active",
        "values",
        "business_retirement",
        "request_sha256",
    ] {
        assert_eq!(actual_retirement[key], retirement[key], "retirement {key}");
    }
    let original_child =
        run(&mut e, history_request("child", "child-first", "1"))["drafts"][0].clone();
    assert_eq!(original_child["values"], first["values"]);
    assert_eq!(original_child["current_generation"], "2");
    assert_eq!(original_child["current_active"], true);
    assert_eq!(usage(&e), before);
    // Observing immutable history never grants reactivation, discard or export.
    assert!(e.execute(request(json!({"action":"draft_save","draft":raw("parent","must-not-reactivate","3","new",1,"0",json!([]))}))).is_err());
    assert!(e.execute(request(json!({"action":"draft_discard","id":CARD,"draft_id":"parent","generation":"2","operation":"must-not-discard-history"}))).is_err());
    assert_eq!(usage(&e), before);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn readonly_history_exact_generation_identity_shape_and_budget_fail_closed() {
    let (_dir, _path, mut e, _wire, _p, _parent) = fixture();
    let before = usage(&e);
    for generation in ["0", "2", "01", "9007199254740993", "18446744073709551616"] {
        assert!(
            e.execute(request(history_request("parent", "pub-save", generation)))
                .is_err(),
            "{generation}"
        );
        assert_eq!(e.effect, "not_committed");
        assert_eq!(usage(&e), before);
    }
    for (field, value) in [
        ("operation", json!("unwanted-write-op")),
        ("source", json!("00")),
        ("draft", raw("x", "x", "0", "x", 1, "0", json!([]))),
        ("flag", json!(true)),
        ("attachment_id", json!("asset")),
    ] {
        let mut q = history_request("parent", "pub-save", "1");
        q[field] = value;
        assert!(e.execute(request(q)).is_err(), "outer {field}");
        assert_eq!(usage(&e), before);
    }
    assert!(
        e.execute(request(history_request("foreign-parent", "pub-save", "1")))
            .is_err()
    );
    assert!(
        e.execute(request(history_request("parent", "unknown-operation", "1")))
            .is_err()
    );
    assert_eq!(usage(&e), before);
}

#[test]
fn readonly_history_complete_reply_budget_keeps_oversized_raw_and_current_bytes() {
    let (_dir, _path, mut e, _root, _proof, _s1) = fixture();
    let large = "\"".repeat(300_000);
    let saved = run(&mut e, json!({"action":"draft_save","draft":raw("parent","large-history-S2","1",&large,1,"0",json!([]))}))["drafts"][0].clone();
    let before = usage(&e);
    assert_eq!(
        e.execute(request(history_request("parent", "large-history-S2", "2")))
            .unwrap_err(),
        "DraftHistoryReplyBytesLimit"
    );
    assert_eq!(e.effect, "not_committed");
    assert_eq!(usage(&e), before);
    let observed = draft::read_history(&e.host, CARD, "parent", "large-history-S2").unwrap();
    assert_eq!(
        observed
            .slot
            .request
            .unwrap()
            .values
            .unwrap()
            .description
            .unwrap()
            .text,
        large
    );
    assert_eq!(saved["current_generation"], "2");
}

#[test]
fn readonly_retired_parent_history_retains_pin_metadata_but_grants_no_export() {
    let (_dir, path, mut e, _root, p, _s1) = fixture();
    let bytes = b"history pin bytes\0\xff";
    let import = crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: "parent".into(),
        operation_id: "history-pin-import".into(),
        expected_generation: 1,
        name: "history.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: hash(bytes),
    };
    let start = e.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let imported = crate::editor_draft_staging::import_durable(
        &mut e.host,
        &import,
        &mut bytes.as_slice(),
        clock,
        1,
        &mut "not_committed",
    )
    .unwrap();
    let selections = json!([{"origin":2,"asset_id":imported.asset_id,"aliases":["history 汉字"]}]);
    let parent=run(&mut e,json!({"action":"draft_save","draft":raw("parent","history-pinned-S2","1","pinned S2",1,"0",selections)}))["drafts"][0].clone();
    let mut h = handoff_json(&p, &parent, "pinned raw child");
    h["child"]["assets"] =
        json!([{"origin":4,"asset_id":imported.asset_id,"aliases":["history 汉字"]}]);
    run(&mut e, handoff_request(&h));
    let retire = retirement_request(&mut e, &p);
    run(&mut e, retire);
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    let history =
        run(&mut e, history_request("parent", "history-pinned-S2", "2"))["drafts"][0].clone();
    assert_eq!(history["assets"], parent["assets"]);
    assert_eq!(history["values"], parent["values"]);
    assert_eq!(history["current_active"], false);
    assert_eq!(history["current_generation"], "3");
    assert!(
        draft::export_asset_verified(
            &e.host,
            CARD,
            "parent",
            2,
            &imported.asset_id,
            &mut std::io::sink()
        )
        .is_err()
    );
    let mut exported = vec![];
    draft::export_asset_verified(&e.host, CARD, "child", 1, &imported.asset_id, &mut exported)
        .unwrap();
    assert_eq!(exported, bytes);
    assert_eq!(usage(&e), before);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn closed_saved_exact_original_inspect_can_qualify_fresh_owned_todos_without_new_authority_contract()
 {
    let (_dir, path, mut e, root, p, parent) = fixture();
    run(&mut e, close_request(&saved_close(&p, &parent)));
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    let checked=run(&mut e,json!({"action":"editor_commit_inspect","editor_commit":{"request_json":root,"expected_revision":"1"}}))["editor_commit"].clone();
    assert_eq!(checked["qualification"], "development_editor_wire_v1");
    assert_eq!(checked["live_matches"], true);
    let card = current_card(&e);
    assert_eq!(
        checked["historical_card"],
        serde_json::to_value(&card).unwrap()
    );
    assert_eq!(usage(&e), before);
    assert_eq!(read(&mut e, &p, "submission")["request_json"], root);
    let mut fresh_raw = raw(
        "closed-reopen",
        "closed-reopen-pub",
        "0",
        "reopened body",
        0,
        "1",
        json!([]),
    );
    fresh_raw["values"]["todos"] = text("two\nthree\none");
    let fresh = run(&mut e, json!({"action":"draft_save","draft":fresh_raw}))["drafts"][0].clone();
    let business = json!({"action":"edit","id":CARD,"operation":"closed-reopen-business","source":card.source,
        "title":"handoff title","description":"reopened body","hypothesis":"h","conclusion":"c","todos":"two\nthree\none","category":"灵感","stage":"待整理"});
    let continuation = json!({"root_request_json":root,"baseline":{"operation":checked["operation"],"revision":checked["revision"],"command_sha256":checked["command_sha256"],"content_sha256":checked["content_sha256"],"request_sha256":checked["request_sha256"],"publication_sha256":checked["publication_sha256"]}});
    let wire=serde_json::to_string(&json!({"schema_version":1,"mode":"continued_todos","business":business,"publication":draft_proof(&fresh),"continuation":continuation})).unwrap();
    let before = usage(&e);
    let next = run(
        &mut e,
        json!({"action":"editor_save","editor_save":{"request_json":wire}}),
    )["editor_commit"]
        .clone();
    assert_eq!(next["revision"], "2");
    assert_eq!(usage(&e).0, before.0 + 1);
    assert_eq!(
        next["historical_card"]["tasks"][0]["id"],
        checked["historical_card"]["tasks"][1]["id"]
    );
    assert_eq!(
        next["historical_card"]["tasks"][2]["id"],
        checked["historical_card"]["tasks"][0]["id"]
    );
    let after=run(&mut e,json!({"action":"editor_commit_inspect","editor_commit":{"request_json":root,"expected_revision":"1"}}))["editor_commit"].clone();
    assert_eq!(after["live_matches"], false);
    assert_eq!(after["live_revision"], "2");
    assert!(
        !draft::read(&e.host, CARD, "parent")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    e.host.store_local().integrity_check().unwrap();
}

#[test]
#[ignore = "explicit complete v26 Store DTO export requires HMOS_REOPEN_HISTORY_DTO_FIXTURE"]
fn export_actual_store_reopen_history_dto_fixture() {
    let (_dir, path, mut e, root, p, pub_s1) = fixture();
    let parent = s2(&mut e);
    let h = handoff_json(&p, &parent, "S2 raw\r\n\0漢 🧪");
    let child_first = run(&mut e, handoff_request(&h));
    let parent_history_active = run(&mut e, history_request("parent", "late-S2", "2"));
    let child_current = run(
        &mut e,
        json!({"action":"draft_save","draft":raw("child","recovery-current-child","1","actual latest S3",0,"1",json!([]))}),
    );
    let retirement_request = retirement_request(&mut e, &p);
    let retirement_reply = run(&mut e, retirement_request.clone());
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let parent_history_retired = run(&mut e, history_request("parent", "late-S2", "2"));
    let retirement_history = run(&mut e, history_request("parent", "retire", "3"));
    let child_history = run(&mut e, history_request("child", "child-first", "1"));
    let current_read = run(
        &mut e,
        json!({"action":"draft_read","id":CARD,"draft_id":"child"}),
    );
    let inspect = run(
        &mut e,
        json!({"action":"editor_commit_inspect","editor_commit":{"request_json":root,"expected_revision":"1"}}),
    );
    let parts = [
        "submission",
        "publication",
        "save",
        "inspect",
        "handoff",
        "retirement",
        "close",
    ]
    .into_iter()
    .map(|part| read(&mut e, &p, part))
    .collect::<Vec<_>>();
    let fixed_close = close_request(&retired_close(&p));
    let close_reply = run(&mut e, fixed_close.clone());
    let closed_parts = [
        "submission",
        "publication",
        "save",
        "inspect",
        "handoff",
        "retirement",
        "close",
    ]
    .into_iter()
    .map(|part| read(&mut e, &p, part))
    .collect::<Vec<_>>();
    let value = json!({"schema_version":1,"kind":"actual_store_reopen_history_v26","card_id":CARD,"original_request_json":root,
        "proof":p,"publication_s1":pub_s1,"fixed_parent_s2":parent,"handoff_request":handoff_request(&h),
        "child_first_reply":child_first,"parent_history_active_reply":parent_history_active,"child_current_reply":child_current,
        "retirement_request":retirement_request,"retirement_reply":retirement_reply,"parent_history_retired_reply":parent_history_retired,
        "retirement_history_reply":retirement_history,"child_first_history_reply":child_history,"child_current_read_reply":current_read,
        "original_inspect_reply":inspect,"planned_parts":parts,"close_request":fixed_close,"close_reply":close_reply,"closed_parts":closed_parts});
    let output =
        std::env::var_os("HMOS_REOPEN_HISTORY_DTO_FIXTURE").expect("explicit output required");
    std::fs::write(output, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}
