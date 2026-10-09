use super::*;
use serde_json::{Value, json};
const CARD: &str = "handoff-card";
fn request(v: Value) -> Request {
    let wire = serde_json::to_string(&v).unwrap();
    let mut r: Request = serde_json::from_str(&wire).unwrap();
    r.transport_json = wire;
    r
}
fn run(e: &mut Engine, v: Value) -> Value {
    serde_json::to_value(e.execute(request(v)).unwrap()).unwrap()
}
fn literal(e: &mut Engine, wire: &str) -> Result<Reply> {
    let mut r: Request = serde_json::from_str(wire).map_err(err)?;
    r.transport_json = wire.into();
    e.execute(r)
}
fn text(s: &str) -> Value {
    json!({"text":s,"selection_base":-1,"selection_extent":-1,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1})
}
fn raw(
    id: &str,
    op: &str,
    g: &str,
    description: &str,
    kind: u32,
    revision: &str,
    assets: Value,
) -> Value {
    json!({"card_id":CARD,"draft_id":id,"operation_id":op,"expected_generation":g,"source_kind":kind,"source_revision":revision,"assets":assets,
 "values":{"title":text("handoff title"),"description":text(description),"hypothesis":text("h"),"conclusion":text("c"),"todos":text(" one\ntwo "),"category":"灵感","stage":"待整理"}})
}
fn draft_proof(v: &Value) -> Value {
    json!({"draft_id":v["scope"]["draft_id"],"generation":v["generation"],"save_operation":v["operation_id"],"request_sha256":v["request_sha256"]})
}
fn read(e: &mut Engine, p: &Value, part: &str) -> Value {
    run(e,json!({"action":"editor_intent_read","editor_intent_ref":{"intent_id":p["intent_id"],"prepare_operation":p["prepare_operation"],"part":part}}))["editor_intents"][0].clone()
}
fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    Engine,
    String,
    Value,
    Value,
) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hmos-development.sqlite");
    let mut e = Engine::open(&path).unwrap();
    let d=run(&mut e,json!({"action":"draft_save","draft":raw("parent","pub-save","0","S1 汉字 🧪 é.",1,"0",json!([]))}))["drafts"][0].clone();
    let wire=serde_json::to_string(&json!({"schema_version":1,"mode":"create","business":{"action":"create","id":CARD,"operation":"original-business","source":"","title":"handoff title","description":"S1 汉字 🧪 é.","hypothesis":"h","conclusion":"c","todos":" one\ntwo ","category":"灵感","stage":"待整理"},"publication":{"draft_id":"parent","generation":"1","save_operation":"pub-save","request_sha256":d["request_sha256"]},"continuation":null})).unwrap();
    let p=run(&mut e,json!({"action":"editor_intent_prepare","editor_intent":{"operation_id":"prepare","request_json":wire}}))["editor_intents"][0]["proof"].clone();
    run(
        &mut e,
        json!({"action":"editor_intent_issue","editor_intent_issue":{"intent":p,"expected_generation":"1"}}),
    );
    let save = read(&mut e, &p, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    literal(&mut e, &save).unwrap();
    (dir, path, e, wire, p, d)
}
fn handoff_json(p: &Value, parent: &Value, description: &str) -> Value {
    json!({"schema_version":1,"intent":p,"parent":draft_proof(parent),"child":raw("child","child-first","0",description,0,"1",json!([])),"plan_operation":"plan","retirement_operation":"retire"})
}
fn handoff_request(h: &Value) -> Value {
    json!({"action":"draft_continue_business","business_handoff":{"request_json":serde_json::to_string(h).unwrap()}})
}
fn retirement_request(e: &mut Engine, p: &Value) -> Value {
    json!({"action":"draft_continue_business_retire","business_retirement":{"request_json":read(e,p,"retirement")["retirement_request_json"]}})
}
fn saved_close(p: &Value, parent: &Value) -> Value {
    json!({"schema_version":1,"intent":p,"expected_generation":"2","operation_id":"saved-close","disposition":"saved_exact","parent":{"proof":draft_proof(parent),"discard_operation":"exact-discard"},"plan_operation":""})
}
fn retired_close(p: &Value) -> Value {
    json!({"schema_version":1,"intent":p,"expected_generation":"3","operation_id":"handoff-close","disposition":"handoff_retired","parent":null,"plan_operation":"plan"})
}
fn close_request(c: &Value) -> Value {
    json!({"action":"editor_intent_close","editor_intent_close":{"request_json":serde_json::to_string(c).unwrap()}})
}
fn usage(e: &Engine) -> (u64, u64) {
    e.host.store_local().pending_usage().unwrap()
}
fn s2(e: &mut Engine) -> Value {
    run(e,json!({"action":"draft_save","draft":raw("parent","late-S2","1","S2 raw\r\n\0漢 🧪",1,"0",json!([]))}))["drafts"][0].clone()
}

#[test]
fn exact_s1_business_s2_raw_source0_child_retirement_close_and_reopen_are_distinct_durable_writes()
{
    let (_dir, path, mut e, wire, p, _) = fixture();
    let parent = s2(&mut e);
    let mut h = handoff_json(&p, &parent, "S2 raw\r\n\0漢 🧪");
    // Whole values and UTF-16 selection/composition are preserved unformatted.
    h["child"]["values"]["description"]["selection_base"] = json!(4);
    h["child"]["values"]["description"]["selection_extent"] = json!(1);
    h["child"]["values"]["description"]["composing_start"] = json!(1);
    h["child"]["values"]["description"]["composing_end"] = json!(2);
    let before = usage(&e);
    let r = run(&mut e, handoff_request(&h));
    assert_eq!(usage(&e).0, before.0 + 2);
    assert_eq!(r["effect"], "committed");
    assert_eq!(r["receipt_revision"], "1");
    assert_eq!(r["editor_intents"][0]["phase"], "handoff_planned");
    let child = r["drafts"][0].clone();
    assert_eq!(child["scope"]["source_kind"], 0);
    assert_eq!(
        child["values"]["description"],
        h["child"]["values"]["description"]
    );
    assert!(child["fork_link"].is_null());
    assert!(!child["business_link"].is_null());
    let business = e.host.store_local().card(CARD).unwrap().unwrap();
    assert_eq!(child["scope"]["source"], hex(&business.encode()));
    assert_eq!(
        morrow_workbench_plugin::tasks_v2::decode(CARD, "handoff title", &business.body())
            .unwrap()
            .tasks
            .len(),
        2
    );
    assert!(
        draft::read(&e.host, CARD, "parent")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_eq!(e.execute(request(json!({"action":"draft_discard","id":CARD,"draft_id":"child","generation":"1","operation":"premature-child-discard"}))).unwrap_err(),"BusinessParentNotRetired");
    let retired = retirement_request(&mut e, &p);
    let rr = run(&mut e, retired.clone());
    assert_eq!(rr["receipt_revision"], "3");
    assert_eq!(rr["drafts"][0]["active"], false);
    assert_eq!(
        rr["drafts"][0]["business_retirement"]["operation_id"],
        "retire"
    );
    let next = run(
        &mut e,
        json!({"action":"draft_save","draft":raw("child","child-S3","1","S3 raw",0,"1",json!([]))}),
    );
    assert_eq!(next["drafts"][0]["business_link"], child["business_link"]);
    let close = close_request(&retired_close(&p));
    let closed = run(&mut e, close.clone());
    assert_eq!(closed["receipt_revision"], "5");
    assert_eq!(
        closed["editor_intents"][0]["close_disposition"],
        "handoff_retired"
    );
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
    assert!(
        draft::read(&e.host, CARD, "child")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    let replay = run(&mut e, handoff_request(&h));
    assert_eq!(replay["drafts"][0]["generation"], "1");
    assert_eq!(replay["drafts"][0]["current_generation"], "2");
    assert_eq!(replay["drafts"][0]["values"], child["values"]);
    assert_eq!(run(&mut e, retired)["drafts"][0]["repeated"], true);
    assert_eq!(run(&mut e, close)["editor_intents"][0]["repeated"], true);
    assert_eq!(usage(&e), before);
    run(
        &mut e,
        json!({"action":"draft_discard","id":CARD,"draft_id":"child","generation":"2","operation":"explicit-child-discard"}),
    );
    let historical = run(&mut e, handoff_request(&h));
    assert_eq!(historical["drafts"][0]["current_active"], false);
    assert!(e.execute(request(json!({"action":"draft_save","draft":raw("child","reactivation","3","oops",0,"1",json!([]))}))).is_err());
    e.host.store_local().integrity_check().unwrap();
}

#[test]
fn saved_exact_compares_complete_textvalue_selection_and_assets_not_just_strings() {
    let (_dir, path, mut e, wire, p, parent) = fixture();
    let before = usage(&e);
    let close = close_request(&saved_close(&p, &parent));
    let r = run(&mut e, close.clone());
    assert_eq!(usage(&e).0, before.0 + 3);
    assert_eq!(r["receipt_revision"], "4");
    assert_eq!(r["editor_intents"][0]["phase"], "closed");
    assert_eq!(r["editor_intents"][0]["close_disposition"], "saved_exact");
    assert!(
        !draft::read(&e.host, CARD, "parent")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let before = usage(&e);
    run(&mut e, close);
    assert_eq!(usage(&e), before);
    assert_eq!(read(&mut e, &p, "submission")["request_json"], wire);
    // A fresh ordinary draft may open the current accurate business result;
    // closed metadata never revives the old journal or provides asset authority.
    let fresh = run(
        &mut e,
        json!({"action":"draft_save","draft":raw("fresh","fresh-op","0","reopened",0,"1",json!([]))}),
    );
    assert_eq!(fresh["drafts"][0]["scope"]["draft_id"], "fresh");
    for field in [
        "selection_base",
        "affinity",
        "directional",
        "composing_start",
    ] {
        let (_dir, _path, mut e, _wire, p, _) = fixture();
        let mut d = raw(
            "parent",
            "selection-S2",
            "1",
            "S1 汉字 🧪 é.",
            1,
            "0",
            json!([]),
        );
        d["values"]["description"][field] = if field == "directional" {
            json!(true)
        } else {
            json!(0)
        };
        let parent = run(&mut e, json!({"action":"draft_save","draft":d}))["drafts"][0].clone();
        let before = usage(&e);
        assert_eq!(
            e.execute(request(close_request(&saved_close(&p, &parent))))
                .unwrap_err(),
            "BusinessSavedExactChangedRaw"
        );
        assert_eq!(usage(&e), before);
        assert!(
            draft::read(&e.host, CARD, "parent")
                .unwrap()
                .unwrap()
                .slot
                .active
        );
    }
}

#[test]
fn changed_first_literal_same_operation_and_ordinary_parent_save_discard_import_are_rejected() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    let h = handoff_json(&p, &parent, "late raw");
    run(&mut e, handoff_request(&h));
    let before = usage(&e);
    for path in ["raw", "parent", "child", "plan", "retire"] {
        let mut other = h.clone();
        match path {
            "raw" => other["child"]["values"]["description"]["text"] = json!("changed"),
            "parent" => other["parent"]["generation"] = json!("2"),
            "child" => other["child"]["draft_id"] = json!("other"),
            "plan" => other["plan_operation"] = json!("other-plan"),
            _ => other["retirement_operation"] = json!("other-retire"),
        };
        assert!(
            e.execute(request(handoff_request(&other))).is_err(),
            "{path}"
        );
        assert_eq!(usage(&e), before);
    }
    assert!(e.execute(request(json!({"action":"draft_save","draft":raw("parent","blind-S2","1","raw",1,"0",json!([]))}))).is_err());
    assert!(e.execute(request(json!({"action":"draft_discard","id":CARD,"draft_id":"parent","generation":"1","operation":"ordinary-discard"}))).is_err());
    let data = b"new independent import";
    let import = crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: "parent".into(),
        operation_id: "after-plan-import".into(),
        expected_generation: 1,
        name: "late.bin".into(),
        kind: "file".into(),
        byte_length: data.len() as u64,
        sha256: hash(data),
    };
    assert!(
        crate::editor_draft_staging::begin(&mut e.host, &import, || 1, 1, &mut "not_committed")
            .is_err()
    );
    assert_eq!(usage(&e), before);
    let retire = retirement_request(&mut e, &p);
    let mut value = retire.clone();
    value["business_retirement"]["request_json"] = json!(format!(
        " {}",
        retire["business_retirement"]["request_json"]
            .as_str()
            .unwrap()
    ));
    assert!(e.execute(request(value)).is_err());
    assert_eq!(usage(&e), before);
}

#[test]
fn full_confirmed_raw_fork_ancestor_requires_exact_retirement_and_other_session_is_not_ancestor() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    let fork = json!({"action":"draft_fork","fork":{"child":raw("forked","rawfork-first","0","fork raw",1,"0",json!([])),"parent_draft_id":"parent","parent_generation":"1","parent_save_operation":"pub-save","parent_request_sha256":parent["request_sha256"]}});
    let child = run(&mut e, fork)["drafts"][0].clone();
    let h = handoff_json(&p, &child, "late after rawfork");
    assert_eq!(
        e.execute(request(handoff_request(&h))).unwrap_err(),
        "BusinessAncestorParentNotRetired"
    );
    let mut fr = child["fork_link"].clone();
    fr["child_draft_id"] = json!("forked");
    fr["operation_id"] = json!("rawfork-retire");
    let retirement = json!({"action":"draft_fork_retire","fork_retirement":{"card_id":CARD,"child_draft_id":"forked","child_operation":"rawfork-first","parent_draft_id":"parent","parent_generation":"1","parent_save_operation":"pub-save","parent_request_sha256":parent["request_sha256"],"operation_id":"rawfork-retire"}});
    run(&mut e, retirement);
    let first = run(&mut e, handoff_request(&h));
    assert_eq!(
        first["drafts"][0]["business_link"]["parent"]["draft_id"],
        "forked"
    );
    let business_retire = retirement_request(&mut e, &p);
    let retired = run(&mut e, business_retire);
    assert!(!retired["drafts"][0]["fork_link"].is_null());
    assert!(!retired["drafts"][0]["business_retirement"].is_null());
    assert!(retired["drafts"][0]["fork_retirement"].is_null());
    let (_dir, _path, mut e, _wire, p, _) = fixture();
    let mut separate = raw(
        "independent",
        "independent-op",
        "0",
        "same source shape",
        1,
        "0",
        json!([]),
    );
    // A new-card ordinary draft cannot now be created after business exists;
    // clone the known source-less pre-business session through actual host save.
    separate["card_id"] = json!("different-card");
    let other = run(&mut e, json!({"action":"draft_save","draft":separate}))["drafts"][0].clone();
    let before = usage(&e);
    assert!(
        e.execute(request(handoff_request(&handoff_json(&p, &other, "raw"))))
            .is_err()
    );
    assert_eq!(usage(&e), before);
}

#[test]
fn unresolved_independent_ready_import_blocks_plan_without_releasing_any_owner() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    let bytes = b"independent not selected";
    let import = crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: "parent".into(),
        operation_id: "unselected".into(),
        expected_generation: 1,
        name: "late.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: hash(bytes),
    };
    {
        let start = e.start;
        let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
        crate::editor_draft_staging::import_durable(
            &mut e.host,
            &import,
            &mut bytes.as_slice(),
            clock,
            1,
            &mut "not_committed",
        )
    }
    .unwrap();
    let before = usage(&e);
    assert_eq!(
        e.execute(request(handoff_request(&handoff_json(&p, &parent, "raw"))))
            .unwrap_err(),
        "BusinessParentUnselectedImports"
    );
    assert_eq!(usage(&e), before);
    assert_eq!(read(&mut e, &p, "submission")["phase"], "issued");
    let mut exported = vec![];
    crate::editor_draft_staging::export_verified(
        &e.host,
        CARD,
        "parent",
        1,
        "unselected",
        &mut exported,
    )
    .unwrap();
    assert_eq!(exported, bytes);
}

#[test]
fn source0_child_pin_order_aliases_and_late_confirmed_assets_survive_parent_retirement() {
    let (_dir, path, mut e, _wire, p, _parent) = fixture();
    let mut ids = vec![];
    for n in 0..2 {
        let bytes = format!("late confirmed asset {n}").into_bytes();
        let import = crate::editor_draft_staging::proto::ImportRequest {
            schema_version: 1,
            card_id: CARD.into(),
            draft_id: "parent".into(),
            operation_id: format!("late-import-{n}"),
            expected_generation: 1,
            name: format!("late{n}.bin"),
            kind: "file".into(),
            byte_length: bytes.len() as u64,
            sha256: hash(&bytes),
        };
        let r = {
            let start = e.start;
            let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
            crate::editor_draft_staging::import_durable(
                &mut e.host,
                &import,
                &mut bytes.as_slice(),
                clock,
                1,
                &mut "not_committed",
            )
        }
        .unwrap();
        ids.push(r.asset_id);
    }
    let selections = json!([{"origin":2,"asset_id":ids[0],"aliases":["first original alias"]},{"origin":2,"asset_id":ids[1],"aliases":["second 汉字"]}]);
    let parent=run(&mut e,json!({"action":"draft_save","draft":raw("parent","confirmed-S2","1","S2 with late pins",1,"0",selections)}))["drafts"][0].clone();
    let mut h = handoff_json(&p, &parent, "late raw with pins");
    h["child"]["assets"] = json!([{"origin":4,"asset_id":ids[0],"aliases":["first original alias"]},{"origin":4,"asset_id":ids[1],"aliases":["second 汉字"]}]);
    let before = usage(&e);
    let mut reversed = h.clone();
    reversed["child"]["assets"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        e.execute(request(handoff_request(&reversed))).unwrap_err(),
        "BusinessParentSelectionChanged"
    );
    assert_eq!(usage(&e), before);
    let mut alias = h.clone();
    alias["child"]["assets"][0]["aliases"] = json!(["changed"]);
    assert!(e.execute(request(handoff_request(&alias))).is_err());
    assert_eq!(usage(&e), before);
    let r = run(&mut e, handoff_request(&h));
    assert_eq!(r["drafts"][0]["assets"].as_array().unwrap().len(), 2);
    let retire = retirement_request(&mut e, &p);
    run(&mut e, retire);
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    for (n, id) in ids.iter().enumerate() {
        let mut bytes = vec![];
        draft::export_asset_verified(&e.host, CARD, "child", 1, id, &mut bytes).unwrap();
        assert_eq!(bytes, format!("late confirmed asset {n}").as_bytes());
    }
    let close = close_request(&retired_close(&p));
    run(&mut e, close);
    for id in &ids {
        draft::export_asset_verified(&e.host, CARD, "child", 1, id, &mut std::io::sink()).unwrap();
    }
}

#[test]
fn combined_active_limit_rejects_before_plan_and_keeps_original_parent_and_intent() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    for n in 0..14 {
        let mut d = raw(
            &format!("spare-{n}"),
            &format!("spare-save-{n}"),
            "0",
            "spare raw",
            1,
            "0",
            json!([]),
        );
        d["card_id"] = json!(format!("spare-card-{n}"));
        run(&mut e, json!({"action":"draft_save","draft":d}));
    }
    let h = handoff_json(&p, &parent, "plan retained first raw");
    let before = usage(&e);
    assert_eq!(
        e.execute(request(handoff_request(&h))).unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(e.effect, "not_committed");
    assert_eq!(usage(&e), before);
    assert!(draft::read(&e.host, CARD, "child").unwrap().is_none());
    assert_eq!(read(&mut e, &p, "handoff")["handoff_request_json"], "");
    assert_eq!(read(&mut e, &p, "handoff")["phase"], "issued");
    assert!(
        draft::read(&e.host, CARD, "parent")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    run(
        &mut e,
        json!({"action":"draft_discard","id":"spare-card-0","draft_id":"spare-0","generation":"1","operation":"free-capacity"}),
    );
    let r = run(&mut e, handoff_request(&h));
    assert_eq!(
        r["drafts"][0]["values"]["description"]["text"],
        "plan retained first raw"
    );
}

#[test]
fn historical_business_result_not_current_card_is_the_only_child_source() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    let original = e.host.store_local().card(CARD).unwrap().unwrap().encode();
    run(
        &mut e,
        json!({"action":"favorite","id":CARD,"operation":"external-favorite","flag":true,"source":hex(&original)}),
    );
    let current = e.host.store_local().card(CARD).unwrap().unwrap();
    assert_ne!(current.encode(), original);
    let r = run(
        &mut e,
        handoff_request(&handoff_json(&p, &parent, "first S2")),
    );
    assert_eq!(r["drafts"][0]["scope"]["source"], hex(&original));
    assert_eq!(r["drafts"][0]["scope"]["source_revision"], "1");
    let inspect = read(&mut e, &p, "inspect")["inspect_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        serde_json::to_value(literal(&mut e, &inspect).unwrap()).unwrap()["editor_commit"]["live_matches"],
        false
    );
}

#[test]
fn strict_absence_malformed_proofs_and_complete_reply_budget_refuse_before_plan() {
    let (_dir, _path, mut e, _wire, p, parent) = fixture();
    let h = handoff_json(&p, &parent, "raw");
    let before = usage(&e);
    for field in [
        "intent_id",
        "prepare_operation",
        "prepared_record_sha256",
        "request_sha256",
    ] {
        let mut bad = h.clone();
        bad["intent"][field] = json!("wrong");
        assert!(e.execute(request(handoff_request(&bad))).is_err());
        assert_eq!(usage(&e), before);
    }
    let mut huge = h.clone();
    huge["child"]["values"]["description"]["text"] = json!("汉".repeat(crate::LIMIT / 3));
    assert!(e.execute(request(handoff_request(&huge))).is_err());
    assert_eq!(usage(&e), before);
    let mut outer = handoff_request(&h);
    outer["id"] = json!(CARD);
    assert!(e.execute(request(outer)).is_err());
    assert_eq!(usage(&e), before);
    let mut unexpected = handoff_request(&h);
    unexpected["action"] = json!("query");
    assert!(e.execute(request(unexpected)).is_err());
    assert_eq!(usage(&e), before);
}

#[test]
#[ignore = "explicit DTO fixture export requires HMOS_HANDOFF_DTO_FIXTURE"]
fn export_complete_actual_store_handoff_dto_fixture() {
    let path = std::env::var_os("HMOS_HANDOFF_DTO_FIXTURE").unwrap();
    let (_dir, _path, mut e, wire, p, _) = fixture();
    let mut issued_parts = serde_json::Map::new();
    for part in [
        "submission",
        "publication",
        "save",
        "inspect",
        "close",
        "handoff",
        "retirement",
    ] {
        issued_parts.insert(part.into(), read(&mut e, &p, part));
    }
    let original_inspect_transport = read(&mut e, &p, "inspect")["inspect_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let original_inspect_reply =
        serde_json::to_value(literal(&mut e, &original_inspect_transport).unwrap()).unwrap();
    let parent = s2(&mut e);
    let h = handoff_json(&p, &parent, "S2 DTO raw");
    let child = run(&mut e, handoff_request(&h));
    let handoff = read(&mut e, &p, "handoff");
    let retirement = read(&mut e, &p, "retirement");
    let retire = retirement_request(&mut e, &p);
    let retired = run(&mut e, retire.clone());
    let close = close_request(&retired_close(&p));
    let closed = run(&mut e, close.clone());
    let mut parts = serde_json::Map::new();
    for part in [
        "submission",
        "publication",
        "save",
        "inspect",
        "close",
        "handoff",
        "retirement",
    ] {
        parts.insert(part.into(), read(&mut e, &p, part));
    }
    let (_dir, _path, mut e2, exact_wire, p2, parent2) = fixture();
    let mut exact_issued_parts = serde_json::Map::new();
    for part in [
        "submission",
        "publication",
        "save",
        "inspect",
        "close",
        "handoff",
        "retirement",
    ] {
        exact_issued_parts.insert(part.into(), read(&mut e2, &p2, part));
    }
    let exact_inspect_transport = read(&mut e2, &p2, "inspect")["inspect_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let exact_inspect_reply =
        serde_json::to_value(literal(&mut e2, &exact_inspect_transport).unwrap()).unwrap();
    let exact_close = close_request(&saved_close(&p2, &parent2));
    let exact_closed = run(&mut e2, exact_close.clone());
    let mut exact_parts = serde_json::Map::new();
    for part in [
        "submission",
        "publication",
        "save",
        "inspect",
        "close",
        "handoff",
        "retirement",
    ] {
        exact_parts.insert(part.into(), read(&mut e2, &p2, part));
    }
    let value = json!({"schema_version":1,"source":"actual Store/Core command receipt and immutable history","handoff":{"issued_parts":issued_parts,"original_inspect_transport":original_inspect_transport,"original_inspect_reply":original_inspect_reply,"submission_literal":wire,"intent_proof":p,"parent":parent,"handoff_literal":serde_json::to_string(&h).unwrap(),"handoff_reply":child,"handoff_read":handoff,"retirement_read":retirement,"retirement_transport":retire,"retirement_reply":retired,"close_transport":close,"close_reply":closed,"closed_parts":parts},"saved_exact":{"issued_parts":exact_issued_parts,"original_inspect_transport":exact_inspect_transport,"original_inspect_reply":exact_inspect_reply,"submission_literal":exact_wire,"intent_proof":p2,"parent":parent2,"close_transport":exact_close,"close_reply":exact_closed,"closed_parts":exact_parts}});
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn handoff_plan(e: &Engine, wire: &str) -> intent_proto::Slot {
    let (h, c) = parse_handoff(wire).unwrap();
    let (first, mut live) = intent::immutable(&e.host, &h.intent).unwrap();
    editor_business::intent_result(&e.host, &first).unwrap();
    let parent = current_parent(&e.host, &c.card_id, &h.parent).unwrap();
    ancestor(&e.host, &first, &parent).unwrap();
    live.phase = 3;
    live.generation = 3;
    live.mutation_operation = h.plan_operation.clone();
    live.plan_operation = h.plan_operation.clone();
    live.handoff_request_json = wire.into();
    live.retirement_request_json = retirement_literal(&h, &c, wire).unwrap();
    live.plan_assets = selected_pins(&parent, &c, "intent-plan-pin-").unwrap();
    live.active_bytes = intent::charge(&live).unwrap();
    live
}
fn close_plan(e: &Engine, wire: &str) -> intent_proto::Slot {
    let c: intent::CloseLiteral = serde_json::from_str(wire).unwrap();
    let (first, mut live) = intent::immutable(&e.host, &c.intent).unwrap();
    editor_business::intent_result(&e.host, &first).unwrap();
    live.phase = 4;
    live.generation += 1;
    live.close_request_json = wire.into();
    live.close_plan_operation = close_plan_op(&first, &c.operation_id);
    live.close_disposition = c.disposition;
    live.mutation_operation = live.close_plan_operation.clone();
    live.active_bytes = intent::charge(&live).unwrap();
    live
}
#[test]
fn competing_saved_exact_and_handoff_plans_cas_one_intent_and_freeze_the_same_parent() {
    for close_wins in [false, true] {
        let (_dir, path, mut e, _wire, p, parent) = fixture();
        let h = handoff_json(&p, &parent, "same owner late raw");
        let hwire = serde_json::to_string(&h).unwrap();
        let cwire = serde_json::to_string(&saved_close(&p, &parent)).unwrap();
        let hp = handoff_plan(&e, &hwire);
        let cp = close_plan(&e, &cwire);
        let mut other = Engine::open(&path).unwrap();
        let (winner, loser) = if close_wins { (&cp, &hp) } else { (&hp, &cp) };
        let before = usage(&e);
        intent::write(&mut e, winner, 2).unwrap();
        assert!(intent::write(&mut other, loser, 2).is_err());
        assert_eq!(other.effect, "not_committed");
        assert_eq!(usage(&e).0, before.0 + 1);
        assert!(draft::require_mutable(&e.host, CARD, "parent").is_err());
        assert_eq!(
            intent::current(&e.host, p["intent_id"].as_str().unwrap())
                .unwrap()
                .unwrap(),
            *winner
        );
        let result = if close_wins {
            run(&mut e, close_request(&saved_close(&p, &parent)))
        } else {
            run(&mut e, handoff_request(&h))
        };
        assert_eq!(result["effect"], "committed");
    }
}
#[test]
fn new_source0_draft_can_continue_original_wire_with_true_task_ids_after_handoff() {
    let (_dir, _path, mut e, root, p, parent) = fixture();
    let mut h = handoff_json(&p, &parent, "S2 full body");
    h["child"]["values"]["todos"] = text("one\ntwo\nthree");
    let child = run(&mut e, handoff_request(&h))["drafts"][0].clone();
    let rr = retirement_request(&mut e, &p);
    run(&mut e, rr);
    run(&mut e, close_request(&retired_close(&p)));
    let inspect = read(&mut e, &p, "inspect")["inspect_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let original =
        serde_json::to_value(literal(&mut e, &inspect).unwrap()).unwrap()["editor_commit"].clone();
    let mut b = serde_json::from_str::<Value>(&root).unwrap()["business"].clone();
    b["action"] = json!("edit");
    b["operation"] = json!("S2-business");
    b["source"] = child["scope"]["source"].clone();
    b["description"] = json!("S2 full body");
    b["todos"] = json!("one\ntwo\nthree");
    let wire=serde_json::to_string(&json!({"schema_version":1,"mode":"continued_todos","business":b,"publication":{"draft_id":"child","generation":"1","save_operation":"child-first","request_sha256":child["request_sha256"]},"continuation":{"root_request_json":root,"baseline":{"operation":original["operation"],"revision":original["revision"],"command_sha256":original["command_sha256"],"content_sha256":original["content_sha256"],"request_sha256":original["request_sha256"],"publication_sha256":original["publication_sha256"]}}})).unwrap();
    let p2=run(&mut e,json!({"action":"editor_intent_prepare","editor_intent":{"operation_id":"prepare-S2","request_json":wire}}))["editor_intents"][0]["proof"].clone();
    run(
        &mut e,
        json!({"action":"editor_intent_issue","editor_intent_issue":{"intent":p2,"expected_generation":"1"}}),
    );
    let save = read(&mut e, &p2, "save")["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let before = usage(&e);
    let result = serde_json::to_value(literal(&mut e, &save).unwrap()).unwrap();
    assert_eq!(usage(&e).0, before.0 + 1);
    assert_eq!(result["editor_commit"]["revision"], "2");
    let old = morrow_core::content::CardRecord::decode(
        &unhex(child["scope"]["source"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    let new = e.host.store_local().card(CARD).unwrap().unwrap();
    let oldp =
        morrow_workbench_plugin::tasks_v2::decode(CARD, "handoff title", &old.body()).unwrap();
    let newp =
        morrow_workbench_plugin::tasks_v2::decode(CARD, "handoff title", &new.body()).unwrap();
    assert_eq!(newp.tasks.len(), 3);
    assert_eq!(oldp.tasks[0].id, newp.tasks[0].id);
    assert_eq!(oldp.tasks[1].id, newp.tasks[1].id);
    let recovery=run(&mut e,json!({"action":"draft_fork","fork":{"child":raw("after-business-rawfork","after-business-rawfork-first","0","S3 raw recovery",0,"1",json!([])),"parent_draft_id":"child","parent_generation":"1","parent_save_operation":"child-first","parent_request_sha256":child["request_sha256"]}}))["drafts"][0].clone();
    let retired = run(
        &mut e,
        json!({"action":"draft_fork_retire","fork_retirement":{"card_id":CARD,"child_draft_id":"after-business-rawfork","child_operation":"after-business-rawfork-first","parent_draft_id":"child","parent_generation":"1","parent_save_operation":"child-first","parent_request_sha256":child["request_sha256"],"operation_id":"after-business-rawfork-retire"}}),
    );
    assert!(!retired["drafts"][0]["business_link"].is_null());
    assert!(!retired["drafts"][0]["fork_retirement"].is_null());
    assert!(retired["drafts"][0]["business_retirement"].is_null());
    assert!(recovery["business_link"].is_null());
    assert_eq!(recovery["scope"]["source"], child["scope"]["source"]);
}

#[test]
fn actual_31mib_parent_plan_child_logical_charge_refuses_before_freezing_or_releasing_pins() {
    let (_dir, _path, mut e, _wire, p, _) = fixture();
    let bytes = vec![0x5a; 31 * 1024 * 1024];
    let import = crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: "parent".into(),
        operation_id: "large-late-import".into(),
        expected_generation: 1,
        name: "large.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: hash(&bytes),
    };
    let start = e.start;
    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
    let asset = crate::editor_draft_staging::import_durable(
        &mut e.host,
        &import,
        &mut bytes.as_slice(),
        clock,
        1,
        &mut "not_committed",
    )
    .unwrap()
    .asset_id;
    let parent=run(&mut e,json!({"action":"draft_save","draft":raw("parent","large-confirmed","1","large selected S2",1,"0",json!([{"origin":2,"asset_id":asset,"aliases":["big"]}]))}))["drafts"][0].clone();
    let mut h = handoff_json(&p, &parent, "first full S2");
    h["child"]["assets"] = json!([{"origin":4,"asset_id":asset,"aliases":["big"]}]);
    let before = usage(&e);
    assert_eq!(
        e.execute(request(handoff_request(&h))).unwrap_err(),
        "EditorIntentCombinedActiveLimit"
    );
    assert_eq!(usage(&e), before);
    assert_eq!(read(&mut e, &p, "handoff")["phase"], "issued");
    draft::export_asset_verified(&e.host, CARD, "parent", 2, &asset, &mut std::io::sink()).unwrap();
    assert!(draft::read(&e.host, CARD, "child").unwrap().is_none());
    // A smaller explicitly selected subset can proceed without changing the
    // fixed original business wire or manufacturing a new import permission.
    h["child"]["assets"] = json!([]);
    run(&mut e, handoff_request(&h));
}
#[test]
#[ignore = "actual subprocess transaction crashes require --features morrow-core/fault-injection"]
fn actual_handoff_transaction_crashes_preserve_exact_plan_pins_and_fixed_retry() {
    if let Some(path) = std::env::var_os("HMOS_HANDOFF_CRASH_DB") {
        let plan: Value = serde_json::from_str(
            &std::fs::read_to_string(std::env::var_os("HMOS_HANDOFF_CRASH_PLAN").unwrap()).unwrap(),
        )
        .unwrap();
        let mut e = Engine::open(std::path::Path::new(&path)).unwrap();
        let stage = plan["stage"].as_str().unwrap();
        let wire = plan["literal"].as_str().unwrap();
        if stage == "handoff_plan" {
            let slot = handoff_plan(&e, wire);
            intent::write(&mut e, &slot, 2).unwrap();
        } else if stage == "saved_close_plan" || stage == "handoff_close_plan" {
            let slot = close_plan(&e, wire);
            let previous = slot.generation - 1;
            intent::write(&mut e, &slot, previous).unwrap();
        } else {
            literal(&mut e, wire).unwrap();
        }
        panic!("fault feature/failpoint missing");
    }
    for stage in [
        "handoff_plan",
        "child",
        "retire",
        "saved_close_plan",
        "saved_discard",
        "saved_close",
        "handoff_close_plan",
        "handoff_close",
    ] {
        for boundary in [
            "after-begin",
            "after-card",
            "after-operation",
            "after-event",
            "before-commit",
            "after-commit",
        ] {
            let (dir, path, mut e, _root, p, parent) = fixture();
            let h = handoff_json(&p, &parent, "crash S2 full raw");
            let hwire = serde_json::to_string(&h).unwrap();
            let htransport = serde_json::to_string(&handoff_request(&h)).unwrap();
            let exact = serde_json::to_string(&saved_close(&p, &parent)).unwrap();
            let exacttransport =
                serde_json::to_string(&close_request(&saved_close(&p, &parent))).unwrap();
            let retired = serde_json::to_string(&retired_close(&p)).unwrap();
            let retiredtransport =
                serde_json::to_string(&close_request(&retired_close(&p))).unwrap();
            let (wire, operation, object) = match stage {
                "handoff_plan" => (
                    hwire.clone(),
                    "plan".to_string(),
                    p["intent_id"].as_str().unwrap().to_owned(),
                ),
                "child" => {
                    let slot = handoff_plan(&e, &hwire);
                    intent::write(&mut e, &slot, 2).unwrap();
                    (
                        htransport.clone(),
                        "child-first".into(),
                        draft::key(CARD, "child"),
                    )
                }
                "retire" => {
                    run(&mut e, handoff_request(&h));
                    let r = retirement_request(&mut e, &p);
                    (
                        serde_json::to_string(&r).unwrap(),
                        "retire".into(),
                        draft::key(CARD, "parent"),
                    )
                }
                "saved_close_plan" => {
                    let slot = close_plan(&e, &exact);
                    (
                        exact.clone(),
                        slot.mutation_operation,
                        p["intent_id"].as_str().unwrap().into(),
                    )
                }
                "saved_discard" => {
                    let slot = close_plan(&e, &exact);
                    intent::write(&mut e, &slot, 2).unwrap();
                    (
                        exacttransport.clone(),
                        "exact-discard".into(),
                        draft::key(CARD, "parent"),
                    )
                }
                "saved_close" => {
                    let slot = close_plan(&e, &exact);
                    intent::write(&mut e, &slot, 2).unwrap();
                    let mut parent = current_parent(
                        &e.host,
                        CARD,
                        &serde_json::from_value::<DraftProof>(draft_proof(&parent)).unwrap(),
                    )
                    .unwrap();
                    parent.generation += 1;
                    parent.active = false;
                    parent.active_bytes = 0;
                    let start = e.start;
                    let clock = || u64::try_from(start.elapsed().as_millis()).unwrap() + 1;
                    draft::write_draft_journal(
                        &mut e.host,
                        &draft::key(CARD, "parent"),
                        "exact-discard",
                        1,
                        &parent,
                        clock,
                        &mut "not_committed",
                    )
                    .unwrap();
                    (
                        exacttransport.clone(),
                        "saved-close".into(),
                        p["intent_id"].as_str().unwrap().into(),
                    )
                }
                _ => {
                    run(&mut e, handoff_request(&h));
                    let r = retirement_request(&mut e, &p);
                    run(&mut e, r);
                    let slot = close_plan(&e, &retired);
                    if stage == "handoff_close" {
                        intent::write(&mut e, &slot, 3).unwrap();
                        (
                            retiredtransport.clone(),
                            "handoff-close".into(),
                            p["intent_id"].as_str().unwrap().into(),
                        )
                    } else {
                        (
                            retired.clone(),
                            slot.mutation_operation,
                            p["intent_id"].as_str().unwrap().into(),
                        )
                    }
                }
            };
            let before = usage(&e);
            let prior = e
                .host
                .store_local()
                .card(&object)
                .unwrap()
                .map(|c| c.encode());
            let planpath = dir.path().join("fixed-crash.json");
            std::fs::write(
                &planpath,
                serde_json::to_vec(&json!({"stage":stage,"literal":wire})).unwrap(),
            )
            .unwrap();
            drop(e);
            let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","editor_handoff::tests::actual_handoff_transaction_crashes_preserve_exact_plan_pins_and_fixed_retry","--ignored","--nocapture"]).env("HMOS_HANDOFF_CRASH_DB",&path).env("HMOS_HANDOFF_CRASH_PLAN",&planpath).env("MORROW_TEST_CRASH_AT",boundary).output().unwrap();
            assert_eq!(
                child.status.code(),
                Some(86),
                "{stage}/{boundary}: {}",
                String::from_utf8_lossy(&child.stderr)
            );
            let mut e = Engine::open(&path).unwrap();
            let committed = boundary == "after-commit";
            assert_eq!(
                e.host
                    .store_local()
                    .operation_commit(&object, &operation)
                    .unwrap()
                    .is_some(),
                committed,
                "{stage}/{boundary}"
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
            if stage == "handoff_plan" {
                let slot = handoff_plan(&e, &hwire);
                if !committed {
                    intent::write(&mut e, &slot, 2).unwrap();
                }
                literal(&mut e, &htransport).unwrap();
            } else if stage == "saved_close_plan" {
                if !committed {
                    let slot = close_plan(&e, &exact);
                    intent::write(&mut e, &slot, 2).unwrap();
                }
                literal(&mut e, &exacttransport).unwrap();
            } else if stage == "handoff_close_plan" {
                if !committed {
                    let slot = close_plan(&e, &retired);
                    intent::write(&mut e, &slot, 3).unwrap();
                }
                literal(&mut e, &retiredtransport).unwrap();
            } else {
                literal(&mut e, &wire).unwrap();
            }
            let after = usage(&e);
            if stage == "handoff_plan" || stage == "child" {
                literal(&mut e, &htransport).unwrap();
            } else if stage == "saved_close_plan" {
                literal(&mut e, &exacttransport).unwrap();
            } else if stage == "handoff_close_plan" {
                literal(&mut e, &retiredtransport).unwrap();
            } else {
                literal(&mut e, &wire).unwrap();
            }
            assert_eq!(usage(&e), after);
            e.host.store_local().integrity_check().unwrap();
            println!(
                "PASS {stage}/{boundary}; fixed operation {operation}; independent transaction"
            );
        }
    }
}

mod reopen_history_tests { include!("reopen_history_tests.rs"); }
