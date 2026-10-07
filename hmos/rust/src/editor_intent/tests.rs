use super::*;
use morrow_workbench_plugin::tasks_v2;
use serde_json::{Value, json};
const CARD: &str = "intent-business-card";
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hmos-development.sqlite");
    let engine = Engine::open(&path).unwrap();
    (dir, path, engine)
}
fn request(value: Value) -> Request {
    let literal = serde_json::to_string(&value).unwrap();
    let mut r: Request = serde_json::from_str(&literal).unwrap();
    r.transport_json = literal;
    r
}
fn run(e: &mut Engine, value: Value) -> Value {
    serde_json::to_value(e.execute(request(value)).unwrap()).unwrap()
}
fn literal(e: &mut Engine, wire: &str) -> Result<Reply> {
    let mut r: Request = serde_json::from_str(wire).map_err(err)?;
    r.transport_json = wire.into();
    e.execute(r)
}
fn text(s: &str) -> Value {
    json!({"text":s,"selection_base":-1,"selection_extent":-1,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1})
}
fn raw_draft(id: &str, operation: &str, expected: &str, description: &str, assets: Value) -> Value {
    json!({"action":"draft_save","draft":{"card_id":CARD,"draft_id":id,"operation_id":operation,"expected_generation":expected,"source_kind":1,"source_revision":"0","assets":assets,"values":{"title":text("intent title"),"description":text(description),"hypothesis":text("h"),"conclusion":text("c"),"todos":text(" one\ntwo "),"category":"灵感","stage":"待整理"}}})
}
fn publish(e: &mut Engine, assets: Value) -> (String, Value) {
    let r = run(
        e,
        raw_draft(
            "intent-parent",
            "intent-pub-save",
            "0",
            "complete 汉字 🧪 é.",
            assets,
        ),
    );
    let record = r["drafts"][0].clone();
    let publication = json!({"draft_id":"intent-parent","generation":"1","save_operation":"intent-pub-save","request_sha256":record["request_sha256"]});
    let wire=serde_json::to_string(&json!({"schema_version":1,"mode":"create","business":{"action":"create","id":CARD,"operation":"intent-business-op","source":"","title":"intent title","description":"complete 汉字 🧪 é.","hypothesis":"h","conclusion":"c","todos":" one\ntwo ","category":"灵感","stage":"待整理"},"publication":publication,"continuation":null})).unwrap();
    (wire, record)
}
fn prepare_json(wire: &str, operation: &str) -> Value {
    json!({"action":"editor_intent_prepare","editor_intent":{"operation_id":operation,"request_json":wire}})
}
fn prepared(e: &mut Engine, wire: &str) -> Value {
    run(e, prepare_json(wire, "intent-prepare-op"))["editor_intents"][0].clone()
}
fn issue_json(p: &Value) -> Value {
    json!({"action":"editor_intent_issue","editor_intent_issue":{"intent":p,"expected_generation":"1"}})
}
fn read_reply(e: &mut Engine, p: &Value, part: &str) -> Value {
    run(
        e,
        json!({"action":"editor_intent_read","editor_intent_ref":{"intent_id":p["intent_id"],"prepare_operation":p["prepare_operation"],"part":part}}),
    )
}
fn read(e: &mut Engine, p: &Value, part: &str) -> Value {
    read_reply(e, p, part)["editor_intents"][0].clone()
}
fn close_json(p: &Value, operation: &str) -> Value {
    json!({"action":"editor_intent_close","editor_intent_close":{"request_json":serde_json::to_string(&json!({"schema_version":1,"intent":p,"expected_generation":"1","operation_id":operation,"disposition":"cancel_prepared"})).unwrap()}})
}
fn usage(e: &Engine) -> (u64, u64) {
    e.host.store_local().pending_usage().unwrap()
}

#[test]
fn competing_issue_and_cancel_actual_store_transactions_cas_the_same_exact_generation() {
    for issue_wins in [true, false] {
        let (_dir, path, mut e) = fixture();
        let (wire, _) = publish(&mut e, json!([]));
        let p = prepared(&mut e, &wire)["proof"].clone();
        let first = current(&e.host, p["intent_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        let mut other = Engine::open(&path).unwrap();
        let mut issued = first.clone();
        issued.phase = 1;
        issued.generation = 2;
        issued.prepared_record_sha256 = digest(&first.encode_to_vec());
        issued.mutation_operation = issue_operation(&first);
        (issued.save_request_json, issued.inspect_request_json) = plans(&first).unwrap();
        issued.active_bytes = charge(&issued).unwrap();
        let c = close_json(&p, "competing-cancel");
        let mut closed = first.clone();
        closed.phase = 2;
        closed.generation = 2;
        closed.active = false;
        closed.active_bytes = 0;
        closed.prepared_record_sha256 = digest(&first.encode_to_vec());
        closed.close_request_json = c["editor_intent_close"]["request_json"]
            .as_str()
            .unwrap()
            .into();
        closed.mutation_operation = "competing-cancel".into();
        let before = usage(&e);
        let (winner, loser) = if issue_wins {
            (&issued, &closed)
        } else {
            (&closed, &issued)
        };
        write(&mut e, winner, 1).unwrap();
        assert!(write(&mut other, loser, 1).is_err());
        assert_eq!(other.effect, "not_committed");
        assert_eq!(usage(&e).0, before.0 + 1);
        let live = current(&e.host, p["intent_id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(live, winner.clone());
        assert!(
            e.host
                .store_local()
                .operation_commit(p["intent_id"].as_str().unwrap(), &loser.mutation_operation)
                .unwrap()
                .is_none()
        );
    }
}

#[test]
fn closed_intent_contexts_and_draft_identities_share_the_exact_cumulative_256_ceiling() {
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let original: Value = serde_json::from_str(&wire).unwrap();
    for n in 0..255 {
        let mut v = original.clone();
        v["business"]["operation"] = json!(format!("identity-business-{n}"));
        let w = serde_json::to_string(&v).unwrap();
        let p = run(&mut e, prepare_json(&w, &format!("identity-prepare-{n}")))["editor_intents"]
            [0]["proof"]
            .clone();
        run(&mut e, close_json(&p, &format!("identity-cancel-{n}")));
    }
    let before = usage(&e);
    assert_eq!(
        e.execute(request(prepare_json(&wire, "overflow-intent-identity")))
            .unwrap_err(),
        "EditorIntentCombinedIdentityLimit"
    );
    assert_eq!(
        e.execute(request(raw_draft(
            "overflow-draft-identity",
            "overflow-draft-operation",
            "0",
            "raw",
            json!([])
        )))
        .unwrap_err(),
        "EditorIntentCombinedIdentityLimit"
    );
    assert_eq!(usage(&e), before);
    let list = run(
        &mut e,
        json!({"action":"editor_intent_list","editor_intent_query":{"after":"","limit":16,"card_id":""}}),
    );
    assert_eq!(list["editor_intents"].as_array().unwrap().len(), 16);
    assert!(!list["intent_next_after"].as_str().unwrap().is_empty());
    let next = run(
        &mut e,
        json!({"action":"editor_intent_list","editor_intent_query":{"after":list["intent_next_after"],"limit":16,"card_id":""}}),
    );
    assert_ne!(
        next["editor_intents"][0]["proof"]["intent_id"],
        list["editor_intents"][0]["proof"]["intent_id"]
    );
    // Existing draft may grow at the identity ceiling; no identity is reopened.
    run(
        &mut e,
        raw_draft(
            "intent-parent",
            "identity-existing-growth",
            "1",
            "complete changed raw",
            json!([]),
        ),
    );
    assert_eq!(all(&e.host).unwrap().len(), 255);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn selected_blob_bytes_are_double_charged_and_ordinary_draft_growth_cannot_bypass_intent_bytes() {
    let (_dir, _path, mut e) = fixture();
    let bytes = vec![0x37_u8; 31 * 1024 * 1024];
    let blob = e
        .host
        .store_local_mut()
        .stage_blob(&mut bytes.as_slice(), bytes.len() as u64, None, 1)
        .unwrap();
    let outer = Attachment {
        id: "quota-asset".into(),
        display_name: "quota.bin".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    };
    let mut props = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    };
    props.assets.push(Default::default());
    props.assets[0].id = outer.id.clone();
    props.assets[0].name = outer.display_name.clone();
    props.assets[0].kind = "file".into();
    props.assets[0].bytes = outer.byte_length;
    let card = CardRecord::new_with_attachments(
        CARD,
        "idea",
        2,
        "before",
        props.encode_to_vec(),
        &[outer.clone()],
    )
    .unwrap();
    let start = e.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let mut con = e.host.connect().unwrap();
    let now = clock();
    e.host
        .grant(&mut con, GrantKind::CreateContent, CARD, now + 60000, now)
        .unwrap();
    e.host
        .create_content(&con, "quota-core-seed", &card, clock)
        .unwrap();
    e.host.disconnect(&con).unwrap();
    let mut d = raw_draft(
        "quota-parent",
        "quota-parent-save",
        "0",
        "complete 汉字 🧪 é.",
        json!([{"origin":0,"asset_id":outer.id,"aliases":[]}]),
    );
    d["draft"]["source_kind"] = json!(0);
    d["draft"]["source_revision"] = json!("1");
    d["draft"]["values"]["todos"] = text("");
    let saved = run(&mut e, d)["drafts"][0].clone();
    let wire=serde_json::to_string(&json!({"schema_version":1,"mode":"edit","business":{"action":"edit","id":CARD,"operation":"quota-business","source":hex(&card.encode()),"title":"intent title","description":"complete 汉字 🧪 é.","hypothesis":"h","conclusion":"c","todos":"","category":"灵感","stage":"待整理"},"publication":{"draft_id":"quota-parent","generation":"1","save_operation":"quota-parent-save","request_sha256":saved["request_sha256"]},"continuation":null})).unwrap();
    let p = prepared(&mut e, &wire)["proof"].clone();
    let slot = current(&e.host, p["intent_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert!(slot.active_bytes >= bytes.len() as u64);
    let half = "x".repeat(512 * 1024);
    let mut spare = raw_draft("quota-spare", "quota-spare-save", "0", &half, json!([]));
    spare["draft"]["card_id"] = json!("quota-spare-card");
    spare["draft"]["values"]["title"] = text(&half);
    run(&mut e, spare.clone());
    let before = usage(&e);
    spare["draft"]["operation_id"] = json!("quota-spare-growth");
    spare["draft"]["expected_generation"] = json!("1");
    for field in ["hypothesis", "conclusion", "todos"] {
        spare["draft"]["values"][field] = text(&half);
    }
    assert_eq!(
        e.execute(request(spare)).unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(usage(&e), before);
    let mut next: Value = serde_json::from_str(&wire).unwrap();
    next["business"]["operation"] = json!("quota-next-business");
    assert_eq!(
        e.execute(request(prepare_json(
            &serde_json::to_string(&next).unwrap(),
            "quota-next-prepare"
        )))
        .unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(usage(&e), before);
    run(&mut e, close_json(&p, "quota-release-intent"));
    assert!(
        e.host
            .store_local()
            .export_attachment_local(
                p["intent_id"].as_str().unwrap(),
                "intent-pin-0",
                &mut std::io::sink()
            )
            .is_err()
    );
    let mut growth = raw_draft(
        "quota-spare",
        "quota-growth-after-release",
        "1",
        &half,
        json!([]),
    );
    growth["draft"]["card_id"] = json!("quota-spare-card");
    for field in ["title", "hypothesis", "conclusion", "todos"] {
        growth["draft"]["values"][field] = text(&half);
    }
    run(&mut e, growth);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn actual_store_prepare_issue_read_plans_reopen_and_strict_business_are_separate_transactions() {
    let (_dir, path, mut e) = fixture();
    let (wire, record) = publish(&mut e, json!([]));
    let before = usage(&e);
    let view = prepared(&mut e, &wire);
    let p = view["proof"].clone();
    assert_eq!(view["phase"], "prepared");
    assert_eq!(view["current_generation"], "1");
    assert_eq!(usage(&e).0, before.0 + 1);
    assert!(e.host.store_local().card(CARD).unwrap().is_none());
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
    assert_eq!(read(&mut e, &p, "save")["save_request_json"], "");
    let restored = read(&mut e, &p, "publication")["publication"].clone();
    assert_eq!(restored["values"], record["values"]);
    assert_eq!(restored["request_sha256"], record["request_sha256"]);
    let issued = run(&mut e, issue_json(&p))["editor_intents"][0].clone();
    assert_eq!(issued["phase"], "issued");
    assert_eq!(issued["proof"], p);
    assert_eq!(issued["current_generation"], "2");
    assert_eq!(usage(&e).0, before.0 + 2);
    let save = read(&mut e, &p, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let inspect = read(&mut e, &p, "inspect")["inspect_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let parsed: Value = serde_json::from_str(&save).unwrap();
    assert_eq!(parsed["editor_save"]["request_json"], wire);
    assert_eq!(parsed["editor_save"]["intent"], p);
    assert_eq!(
        serde_json::from_str::<Value>(&inspect).unwrap()["editor_commit"]["expected_revision"],
        "1"
    );
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    assert_eq!(read(&mut e, &p, "save")["save_request_json"], save);
    assert_eq!(
        literal(&mut e, &inspect)
            .unwrap()
            .editor_commit
            .unwrap()
            .commit_status,
        "absent"
    );
    let committed = literal(&mut e, &save).unwrap();
    assert_eq!(committed.receipt_revision, "1");
    assert_eq!(
        committed.editor_commit.unwrap().qualification,
        "development_editor_wire_v1"
    );
    assert_eq!(usage(&e).0, before.0 + 3);
    let before = usage(&e);
    assert_eq!(literal(&mut e, &save).unwrap().receipt_revision, "1");
    assert_eq!(
        literal(&mut e, &inspect)
            .unwrap()
            .editor_commit
            .unwrap()
            .commit_status,
        "committed"
    );
    assert_eq!(usage(&e), before);
    // A real Core detail update may advance live; old exact wire still proves
    // history, with explicit live_matches=false rather than current rebasing.
    let card = e.host.store_local().card(CARD).unwrap().unwrap();
    run(
        &mut e,
        json!({"action":"favorite","id":CARD,"operation":"intent-after-favorite","source":hex(&card.encode()),"flag":true}),
    );
    let inspected = literal(&mut e, &save).unwrap().editor_commit.unwrap();
    assert!(!inspected.live_matches);
    assert_eq!(inspected.live_revision, "2");
    assert_eq!(inspected.revision, "1");
    let repeat = run(&mut e, prepare_json(&wire, "intent-prepare-op"))["editor_intents"][0].clone();
    assert_eq!(repeat["phase"], "issued");
    assert_eq!(repeat["current_generation"], "2");
    assert_eq!(repeat["repeated"], true);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn cancel_and_issue_have_one_actual_cas_winner_and_cancel_tombstone_blocks_every_old_save_route() {
    let (_dir, path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let p = prepared(&mut e, &wire)["proof"].clone();
    let mut other = Engine::open(&path).unwrap();
    let closed = run(&mut e, close_json(&p, "intent-cancel"))["editor_intents"][0].clone();
    assert_eq!(closed["phase"], "closed");
    assert_eq!(closed["current_active"], false);
    let before = usage(&e);
    assert_eq!(
        other.execute(request(issue_json(&p))).unwrap_err(),
        "EditorIntentIssueConflict"
    );
    assert_eq!(
        e.execute(request(
            json!({"action":"editor_save","editor_save":{"request_json":wire}})
        ))
        .unwrap_err(),
        "EditorIntentProofRequired"
    );
    let b = serde_json::from_str::<Value>(&wire).unwrap()["business"].clone();
    assert_eq!(
        e.execute(request(b)).unwrap_err(),
        "EditorIntentProofRequired"
    );
    assert_eq!(usage(&e), before);
    assert!(e.host.store_local().card(CARD).unwrap().is_none());
    let repeat = run(&mut e, close_json(&p, "intent-cancel"))["editor_intents"][0].clone();
    assert_eq!(repeat["repeated"], true);
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
    assert!(
        !read(&mut e, &p, "close")["close_request_json"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    let rep = run(&mut e, prepare_json(&wire, "intent-prepare-op"))["editor_intents"][0].clone();
    assert_eq!(rep["phase"], "closed");
    assert_eq!(rep["current_active"], false);
    assert_eq!(usage(&e), before);
}

#[test]
fn issued_even_business_absent_cannot_cancel_and_altered_proof_or_literal_cannot_mutate() {
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let p = prepared(&mut e, &wire)["proof"].clone();
    run(&mut e, issue_json(&p));
    let save = read(&mut e, &p, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = usage(&e);
    assert_eq!(
        e.execute(request(close_json(&p, "cannot-cancel-issued")))
            .unwrap_err(),
        "EditorIntentCancelIssuedOrClosed"
    );
    assert_eq!(
        literal(&mut e, &format!(" {save}")).unwrap_err(),
        "EditorIntentIssuedWireRequired"
    );
    for field in [
        "intent_id",
        "prepare_operation",
        "generation",
        "prepared_record_sha256",
        "request_sha256",
    ] {
        let mut bad: Value = serde_json::from_str(&save).unwrap();
        bad["editor_save"]["intent"][field] = json!("bad");
        assert!(e.execute(request(bad)).is_err(), "{field}");
        assert_eq!(usage(&e), before);
    }
    let mut parsed: Value = serde_json::from_str(&save).unwrap();
    parsed["editor_save"]["request_json"] = json!(format!("{wire} "));
    assert!(e.execute(request(parsed)).is_err());
    assert!(
        e.execute(request(
            json!({"action":"editor_save","editor_save":{"request_json":wire}})
        ))
        .is_err()
    );
    assert_eq!(usage(&e), before);
    assert!(e.host.store_local().card(CARD).unwrap().is_none());
    run(&mut e, issue_json(&p));
    assert_eq!(usage(&e), before);
    assert_eq!(literal(&mut e, &save).unwrap().receipt_revision, "1");
}

#[test]
fn immutable_publication_keeps_complete_s1_and_pins_after_parent_s2_removes_them() {
    let (_dir, path, mut e) = fixture();
    let bytes = b"actual original immutable asset bytes";
    let blob = e
        .host
        .store_local_mut()
        .stage_blob(&mut bytes.as_slice(), bytes.len() as u64, None, 1)
        .unwrap();
    // Use a source-card asset for the initial raw journal instead of inventing
    // staging ownership. The original full card is independently strict edited.
    let (_wire, _) = publish(&mut e, json!([]));
    let outer = Attachment {
        id: "actual-asset".into(),
        display_name: "actual.bin".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    };
    let mut props = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        ..Default::default()
    };
    props.assets.push(Default::default());
    let asset = &mut props.assets[0];
    asset.id = outer.id.clone();
    asset.name = outer.display_name.clone();
    asset.kind = "file".into();
    asset.bytes = outer.byte_length;
    let card = CardRecord::new_with_attachments(
        "intent-asset-card",
        "idea",
        2,
        "before",
        props.encode_to_vec(),
        &[outer.clone()],
    )
    .unwrap();
    let start = e.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let mut con = e.host.connect().unwrap();
    let now = clock();
    e.host
        .grant(
            &mut con,
            GrantKind::CreateContent,
            "intent-asset-card",
            now + 60000,
            now,
        )
        .unwrap();
    e.host
        .create_content(&con, "asset-core-create", &card, clock)
        .unwrap();
    e.host.disconnect(&con).unwrap();
    let mut d = raw_draft(
        "asset-parent",
        "asset-parent-save",
        "0",
        "complete 汉字 🧪 é.",
        json!([{"origin":0,"asset_id":outer.id,"aliases":["keep alias"]}]),
    );
    d["draft"]["card_id"] = json!("intent-asset-card");
    d["draft"]["source_kind"] = json!(0);
    d["draft"]["source_revision"] = json!("1");
    d["draft"]["values"]["todos"] = text("");
    let saved = run(&mut e, d.clone())["drafts"][0].clone();
    let mut business = serde_json::from_str::<Value>(&_wire).unwrap();
    business["mode"] = json!("edit");
    business["business"]["id"] = json!("intent-asset-card");
    business["business"]["operation"] = json!("intent-asset-business");
    business["business"]["action"] = json!("edit");
    business["business"]["source"] = json!(hex(&card.encode()));
    business["business"]["todos"] = json!("");
    business["publication"] = json!({"draft_id":"asset-parent","generation":"1","save_operation":"asset-parent-save","request_sha256":saved["request_sha256"]});
    let wire = serde_json::to_string(&business).unwrap();
    let p = prepared(&mut e, &wire)["proof"].clone();
    d["draft"]["operation_id"] = json!("asset-parent-S2");
    d["draft"]["expected_generation"] = json!("1");
    d["draft"]["assets"] = json!([]);
    d["draft"]["values"]["description"] = text("complete late S2 kept separate");
    run(&mut e, d);
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let current = e
        .host
        .store_local()
        .card(p["intent_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    let slot = validate_card(&current).unwrap();
    let mut exported = vec![];
    e.host
        .store_local()
        .export_attachment_local(
            p["intent_id"].as_str().unwrap(),
            &slot.assets[0].pin_id,
            &mut exported,
        )
        .unwrap();
    assert_eq!(exported, bytes);
    assert_eq!(
        read(&mut e, &p, "publication")["publication"]["values"]["description"]["text"],
        "complete 汉字 🧪 é."
    );
    run(&mut e, issue_json(&p));
    let plan = read(&mut e, &p, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(literal(&mut e, &plan).unwrap().receipt_revision, "2");
    let mut output = vec![];
    e.host
        .store_local()
        .export_attachment_local("intent-asset-card", &outer.id, &mut output)
        .unwrap();
    assert_eq!(output, bytes);
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn original_core_source_cas_still_rejects_a_changed_source_after_intent_issue() {
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let p = prepared(&mut e, &wire)["proof"].clone();
    run(&mut e, issue_json(&p));
    // Another create with a distinct operation has authority but cannot be
    // overwritten by the issued S1 request merely because it owns an intent.
    let mut b = serde_json::from_str::<Value>(&wire).unwrap()["business"].clone();
    b["operation"] = json!("foreign-create");
    run(&mut e, b);
    let before = usage(&e);
    let actual = e.host.store_local().card(CARD).unwrap().unwrap().encode();
    let plan = read(&mut e, &p, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(literal(&mut e, &plan).is_err());
    assert_eq!(usage(&e), before);
    assert_eq!(
        e.host.store_local().card(CARD).unwrap().unwrap().encode(),
        actual
    );
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
}

#[test]
fn combined_active_count_blocks_intent_first_and_reverse_ordinary_draft_bypass() {
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    for n in 0..14 {
        run(
            &mut e,
            raw_draft(
                &format!("spare-{n}"),
                &format!("spare-save-{n}"),
                "0",
                "raw",
                json!([]),
            ),
        );
    }
    let p = prepared(&mut e, &wire)["proof"].clone();
    let before = usage(&e);
    assert_eq!(
        e.execute(request(raw_draft(
            "seventeenth",
            "seventeenth-save",
            "0",
            "raw",
            json!([])
        )))
        .unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(usage(&e), before);
    run(&mut e, close_json(&p, "free-intent-active"));
    run(
        &mut e,
        raw_draft("sixteenth-draft", "sixteenth-save", "0", "raw", json!([])),
    );
    let mut next: Value = serde_json::from_str(&wire).unwrap();
    next["business"]["operation"] = json!("next-intent-business");
    let next = serde_json::to_string(&next).unwrap();
    let before = usage(&e);
    assert_eq!(
        e.execute(request(prepare_json(&next, "next-intent-prepare")))
            .unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(usage(&e), before);
}

#[test]
fn explicit_part_reads_list_bounds_closed_context_and_envelope_budgets_do_not_drop_original_bytes()
{
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let p = prepared(&mut e, &wire)["proof"].clone();
    run(&mut e, close_json(&p, "closed-list-context"));
    let list = run(
        &mut e,
        json!({"action":"editor_intent_list","editor_intent_query":{"after":"","limit":1,"card_id":CARD}}),
    );
    assert_eq!(list["editor_intents"][0]["phase"], "closed");
    assert_eq!(list["editor_intents"][0]["part"], "summary");
    assert_eq!(list["editor_intents"][0]["request_json"], "");
    assert_eq!(list["effect"], "not_committed");
    assert_eq!(list["receipt_revision"], "");
    let before = usage(&e);
    for limit in [0, 17] {
        assert!(e.execute(request(json!({"action":"editor_intent_list","editor_intent_query":{"after":"","limit":limit,"card_id":""}}))).is_err());
    }
    let mut longwire = wire.clone();
    longwire.push_str(&" ".repeat(crate::LIMIT - longwire.len()));
    assert_eq!(
        e.execute(request(prepare_json(&longwire, "huge-prepare")))
            .unwrap_err(),
        "EditorIntentEnvelopeBytesLimit"
    );
    let mut altered = prepare_json(&wire, "different-prepare");
    altered["id"] = json!(CARD);
    assert_eq!(
        e.execute(request(altered)).unwrap_err(),
        "EditorIntentOuterFields"
    );
    assert!(
        e.execute(request(
            json!({"action":"list","editor_intent":{"operation_id":"bad","request_json":wire}})
        ))
        .is_err()
    );
    assert_eq!(usage(&e), before);
    assert!(
        run(&mut e, json!({"action":"list"}))["cards"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Host metadata is neither a user query candidate nor an ordinary target.
    assert!(
        run(
            &mut e,
            json!({"action":"query","section":"概览","filter":"全部","text":"","sort":"最近添加"})
        )["ids"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let journal_id = p["intent_id"].as_str().unwrap();
    let stored = e
        .host
        .store_local()
        .card(journal_id)
        .unwrap()
        .unwrap()
        .encode();
    for action in [
        "create", "edit", "favorite", "task_add", "task_set", "delete",
    ] {
        assert_eq!(
            e.execute(request(
                json!({"action":action,"id":journal_id,"operation":"ordinary-host-target"})
            ))
            .unwrap_err(),
            "HostOwnedIdentity",
            "{action}"
        );
    }
    assert_eq!(
        e.export_to(
            request(json!({"action":"attachment_export","id":journal_id})),
            &mut Vec::new()
        )
        .unwrap_err(),
        "HostOwnedIdentity"
    );
    assert_eq!(
        e.host
            .store_local()
            .card(journal_id)
            .unwrap()
            .unwrap()
            .encode(),
        stored
    );
    assert_eq!(usage(&e), before);
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
    assert_eq!(
        read(&mut e, &p, "publication")["publication"]["generation"],
        "1"
    );
}

#[test]
#[ignore = "explicit output path; actual Store DTO fixture, not native transport/device proof"]
fn actual_store_intent_dto_fixture() {
    let output = std::env::var_os("HMOS_EDITOR_INTENT_FIXTURE").expect("output path");
    let (_dir, _path, mut e) = fixture();
    let (wire, _) = publish(&mut e, json!([]));
    let prepare_request = prepare_json(&wire, "intent-prepare-op");
    let prepare_reply = run(&mut e, prepare_request.clone());
    let p = prepare_reply["editor_intents"][0]["proof"].clone();
    let issue_request = issue_json(&p);
    let issue_reply = run(&mut e, issue_request.clone());
    let mut parts = serde_json::Map::new();
    for part in ["submission", "publication", "save", "inspect", "close"] {
        parts.insert(part.into(), read_reply(&mut e, &p, part));
    }
    let save = parts["save"]["editor_intents"][0]["save_request_json"]
        .as_str()
        .unwrap();
    let inspect = parts["inspect"]["editor_intents"][0]["inspect_request_json"]
        .as_str()
        .unwrap();
    let mut cancel_wire: Value = serde_json::from_str(&wire).unwrap();
    cancel_wire["business"]["operation"] = json!("cancel-fixture-business");
    let cancel_wire = serde_json::to_string(&cancel_wire).unwrap();
    let cancel_prepare = run(&mut e, prepare_json(&cancel_wire, "cancel-fixture-prepare"));
    let cp = cancel_prepare["editor_intents"][0]["proof"].clone();
    let close_request = close_json(&cp, "cancel-fixture-close");
    let close_reply = run(&mut e, close_request.clone());
    let mut cancel_parts = serde_json::Map::new();
    for part in ["submission", "publication", "save", "inspect", "close"] {
        cancel_parts.insert(part.into(), read_reply(&mut e, &cp, part));
    }
    let business_reply = serde_json::to_value(literal(&mut e, save).unwrap()).unwrap();
    let inspect_reply = serde_json::to_value(literal(&mut e, inspect).unwrap()).unwrap();
    std::fs::write(output,serde_json::to_vec_pretty(&json!({"producer":"fresh actual Store","request_json":wire,"prepare_request":prepare_request,"prepare_reply":prepare_reply,"issue_request":issue_request,"issue_reply":issue_reply,"parts":parts,"business_reply":business_reply,"inspect_reply":inspect_reply,"cancel":{"request_json":cancel_wire,"prepare_request":prepare_json(&cancel_wire,"cancel-fixture-prepare"),"prepare_reply":cancel_prepare,"close_request":close_request,"close_reply":close_reply,"parts":cancel_parts}})).unwrap()).unwrap();
}

#[test]
#[ignore = "host-only subprocess crash vectors require --features morrow-core/fault-injection"]
fn actual_store_intent_crashes_keep_fixed_request_and_one_cas_effect() {
    if let Some(path) = std::env::var_os("HMOS_INTENT_CRASH_DB") {
        let plan =
            std::fs::read_to_string(std::env::var_os("HMOS_INTENT_CRASH_PLAN").unwrap()).unwrap();
        let mut e = Engine::open(std::path::Path::new(&path)).unwrap();
        let _ = literal(&mut e, &plan);
        panic!("fault feature/failpoint missing");
    }
    for action in ["prepare", "issue", "cancel", "business"] {
        for boundary in [
            "after-begin",
            "after-card",
            "after-operation",
            "after-event",
            "before-commit",
            "after-commit",
        ] {
            let (dir, path, mut e) = fixture();
            let (wire, _) = publish(&mut e, json!([]));
            let (plan, operation, object) = if action == "prepare" {
                (
                    serde_json::to_string(&prepare_json(&wire, "intent-prepare-op")).unwrap(),
                    "intent-prepare-op".to_string(),
                    key(CARD, "intent-business-op"),
                )
            } else {
                let p = prepared(&mut e, &wire)["proof"].clone();
                if action == "issue" {
                    (
                        serde_json::to_string(&issue_json(&p)).unwrap(),
                        issue_operation(
                            &current(&e.host, p["intent_id"].as_str().unwrap())
                                .unwrap()
                                .unwrap(),
                        ),
                        p["intent_id"].as_str().unwrap().into(),
                    )
                } else if action == "cancel" {
                    (
                        serde_json::to_string(&close_json(&p, "crash-cancel")).unwrap(),
                        "crash-cancel".into(),
                        p["intent_id"].as_str().unwrap().into(),
                    )
                } else {
                    run(&mut e, issue_json(&p));
                    (
                        read(&mut e, &p, "save")["save_request_json"]
                            .as_str()
                            .unwrap()
                            .into(),
                        "intent-business-op".into(),
                        CARD.into(),
                    )
                }
            };
            let before = usage(&e);
            let prior = e
                .host
                .store_local()
                .card(&object)
                .unwrap()
                .map(|c| c.encode());
            let planpath = dir.path().join("fixed-plan.json");
            std::fs::write(&planpath, &plan).unwrap();
            drop(e);
            let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","editor_intent::tests::actual_store_intent_crashes_keep_fixed_request_and_one_cas_effect","--ignored","--nocapture"]).env("HMOS_INTENT_CRASH_DB",&path).env("HMOS_INTENT_CRASH_PLAN",&planpath).env("MORROW_TEST_CRASH_AT",boundary).output().unwrap();
            assert_eq!(
                child.status.code(),
                Some(86),
                "{action}/{boundary}: {}",
                String::from_utf8_lossy(&child.stderr)
            );
            let mut e = Engine::open(&path).unwrap();
            let committed = boundary == "after-commit";
            assert_eq!(
                matches!(
                    e.host
                        .store_local()
                        .lookup_for_card(&object, &operation)
                        .unwrap(),
                    Lookup::Committed(_)
                ),
                committed,
                "{action}/{boundary}"
            );
            assert_eq!(usage(&e).0, before.0 + u64::from(committed));
            if !committed {
                assert_eq!(
                    e.host
                        .store_local()
                        .card(&object)
                        .unwrap()
                        .map(|c| c.encode()),
                    prior
                );
            }
            literal(&mut e, &plan).unwrap();
            assert_eq!(usage(&e).0, before.0 + 1);
            let after = usage(&e);
            literal(&mut e, &plan).unwrap();
            assert_eq!(usage(&e), after);
            e.host.store_local().integrity_check().unwrap();
        }
    }
}
