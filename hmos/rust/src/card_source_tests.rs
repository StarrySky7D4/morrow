//! Ordinary DTO classification from real Store records, including an actual
//! baseline-bound V1 -> V2 migration. No UI marker or TaskId inference.
use super::*;
use morrow_core::content_migration::ContentMigration;
use morrow_workbench_plugin::{Idea, persistence};
use serde_json::{Value, json};

const CARD: &str = "classified-card";
fn req(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn run(e: &mut Engine, value: Value) -> Value {
    serde_json::to_value(e.execute(req(value)).unwrap()).unwrap()
}
fn actual(e: &Engine) -> CardRecord {
    e.host.store_local().card(CARD).unwrap().unwrap()
}
fn native_properties() -> tasks_v2::Properties {
    tasks_v2::Properties {
        version: 2,
        description: "original body 汉字 🧪".into(),
        category: "灵感".into(),
        stage: "待整理".into(),
        tasks: ["task-looks-migrated", "plain-native-id"]
            .into_iter()
            .map(|id| tasks_v2::Task {
                id: id.into(),
                text: "same text".into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}
fn fixture(migrated: bool) -> (tempfile::TempDir, std::path::PathBuf, Engine) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("hmos-development.sqlite");
    let mut e = Engine::open(&path).unwrap();
    let (version, body) = if migrated {
        let idea = Idea {
            id: CARD.into(),
            title: "original title".into(),
            description: "original body 汉字 🧪".into(),
            category: "灵感".into(),
            stage: "待整理".into(),
            todos: vec!["one".into(), "two".into()],
            completed: vec!["one".into()],
            ..Default::default()
        };
        (1, persistence::encode(&idea, None).unwrap())
    } else {
        (2, native_properties().encode_to_vec())
    };
    let original = CardRecord::new(CARD, "idea", version, "original title", body).unwrap();
    e.host
        .store_local_mut()
        .create_local("seed", &original)
        .unwrap();
    if migrated {
        // Ordinary format1 was already unsupported; classification introduces
        // no migration, read fallback, or implicit legacy editing permission.
        assert_eq!(
            e.execute(req(json!({"action":"list"}))).unwrap_err(),
            "UnsupportedVersion"
        );
        assert_eq!(actual(&e).encode(), original.encode());
        let baseline = tasks_v2::Baseline::capture(CARD, 1, &original.body()).unwrap();
        let migrated_body = tasks_v2::migrate(
            &baseline,
            CARD,
            1,
            &original.summary().title,
            &original.body(),
        )
        .unwrap();
        let mut connection = e.host.connect().unwrap();
        e.host
            .grant(&mut connection, GrantKind::EditContent, CARD, 1000, 0)
            .unwrap();
        e.host
            .migrate_content_guarded_with_evidence(
                &connection,
                &ContentMigration {
                    operation_id: baseline.operation_id(),
                    source_card: original.encode(),
                    target_format_version: 2,
                    body: migrated_body,
                    preview_text: "migrated body".into(),
                },
                &[],
                || 1,
                |_| Ok(()),
            )
            .unwrap();
        e.host.disconnect(&connection).unwrap();
        assert_eq!(actual(&e).summary().revision, 2);
    }
    (dir, path, e)
}
fn list(e: &mut Engine, kind: &str) -> Value {
    let before = e.host.store_local().pending_usage().unwrap();
    let reply = run(e, json!({"action":"list"}));
    assert_eq!(reply["effect"], "not_committed");
    assert_eq!(reply["cards"][0]["content_kind"], kind);
    assert_eq!(reply["cards"][0]["source"], hex(&actual(e).encode()));
    assert_eq!(
        reply["cards"][0]["revision"],
        actual(e).summary().revision.to_string()
    );
    assert_eq!(e.host.store_local().pending_usage().unwrap(), before);
    reply
}
fn text(value: &str) -> Value {
    json!({"text":value,"selection_base":-1,"selection_extent":-1,"affinity":1,"directional":false,"composing_start":-1,"composing_end":-1})
}
fn current_save(e: &mut Engine) -> (String, String, Value, Vec<Value>) {
    let source = actual(e);
    let p = tasks_v2::decode(CARD, &source.summary().title, &source.body()).unwrap();
    let business = json!({"action":"edit","id":CARD,"operation":"current-body",
        "source":hex(&source.encode()),"title":"changed current title","description":"changed current body 漢 🧪",
        "hypothesis":"changed hypothesis","conclusion":"changed conclusion","todos":"",
        "category":p.category,"stage":p.stage});
    let publication_reply = run(
        e,
        json!({"action":"draft_save","draft":{"card_id":CARD,
        "draft_id":"current-draft","operation_id":"publish-current","expected_generation":"0",
        "source_revision":source.summary().revision.to_string(),"source_kind":0,"assets":[],
        "values":{"title":text("changed current title"),"description":text("changed current body 漢 🧪"),
            "hypothesis":text("changed hypothesis"),"conclusion":text("changed conclusion"),
            "todos":text(""),"category":p.category,"stage":p.stage}}}),
    );
    let publication = &publication_reply["drafts"][0];
    let wire = serde_json::to_string(&json!({"schema_version":1,"mode":"current_v2","business":business,
        "publication":{"draft_id":"current-draft","generation":publication["generation"],
            "save_operation":"publish-current","request_sha256":publication["request_sha256"]},"continuation":null})).unwrap();
    let prepared = run(
        e,
        json!({"action":"editor_intent_prepare","editor_intent":{"operation_id":"prepare-current","request_json":wire}}),
    );
    let proof = prepared["editor_intents"][0]["proof"].clone();
    let issued = run(
        e,
        json!({"action":"editor_intent_issue","editor_intent_issue":{"intent":proof,"expected_generation":"1"}}),
    );
    let read = run(
        e,
        json!({"action":"editor_intent_read","editor_intent_ref":{"intent_id":proof["intent_id"],"prepare_operation":proof["prepare_operation"],"part":"save"}}),
    );
    let outer = read["editor_intents"][0]["save_request_json"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut request: Request = serde_json::from_str(&outer).unwrap();
    request.transport_json = outer.clone();
    let reply = serde_json::to_value(e.execute(request).unwrap()).unwrap();
    (
        wire,
        outer,
        reply,
        vec![publication_reply, prepared, issued, read],
    )
}
fn history_keys(reply: &Value) {
    let card = reply["editor_commit"]["historical_card"]
        .as_object()
        .unwrap();
    let mut expected = [
        "id",
        "revision",
        "source",
        "title",
        "description",
        "hypothesis",
        "conclusion",
        "category",
        "stage",
        "favorite",
        "deleted",
        "deleted_at",
        "tasks",
        "assets",
    ]
    .to_vec();
    expected.sort_unstable();
    assert_eq!(
        card.keys().map(String::as_str).collect::<Vec<_>>(),
        expected
    );
    assert!(!card.contains_key("content_kind"));
}
fn exercise(migrated: bool) -> Value {
    let (_dir, path, mut e) = fixture(migrated);
    let kind = if migrated { "legacy" } else { "v2" };
    let initial = list(&mut e, kind);
    let original = actual(&e);
    let p = tasks_v2::decode(CARD, &original.summary().title, &original.body()).unwrap();
    let a = p.tasks[0].id.clone();
    let b = p.tasks[1].id.clone();
    let mut revisions = vec![];
    for (index, (action, fields)) in [
        ("favorite", json!({"flag":true})),
        ("category", json!({"category":"实验","stage":"待验证"})),
        (
            "task_rename",
            json!({"task_id":a,"text":"renamed current task"}),
        ),
        ("task_toggle", json!({"task_id":b,"flag":true})),
        ("task_reorder", json!({"order":[b,a]})),
        ("task_remove", json!({"task_id":a})),
    ]
    .into_iter()
    .enumerate()
    {
        let mut request = fields;
        request["action"] = json!(action);
        request["id"] = json!(CARD);
        request["operation"] = json!(format!("mutation-{index}"));
        request["source"] = json!(hex(&actual(&e).encode()));
        let reply = run(&mut e, request);
        assert_eq!(reply["effect"], "committed");
        assert_eq!(reply["cards"][0]["content_kind"], kind);
        assert_eq!(reply["cards"][0]["source"], hex(&actual(&e).encode()));
        assert_eq!(
            actual(&e).summary().revision,
            original.summary().revision + index as u64 + 1
        );
        revisions.push(reply);
    }
    let before = actual(&e);
    let p_before = tasks_v2::decode(CARD, &before.summary().title, &before.body()).unwrap();
    let (wire, outer, saved, context) = current_save(&mut e);
    history_keys(&saved);
    let changed = actual(&e);
    let p_after = tasks_v2::decode(CARD, &changed.summary().title, &changed.body()).unwrap();
    assert_ne!(changed.summary().title, before.summary().title);
    assert_ne!(p_after.description, p_before.description);
    assert_eq!(p_after.tasks, p_before.tasks);
    assert_eq!(p_after.retired_task_ids, p_before.retired_task_ids);
    assert_eq!(p_after.origin, p_before.origin);
    assert_eq!(p_after.favorite, p_before.favorite);
    assert_eq!(p_after.category, p_before.category);
    assert_eq!(p_after.stage, p_before.stage);
    assert_eq!(p_after.tasks[0].id, b);
    assert_eq!(p_after.tasks[0].completion, 1);
    assert_eq!(p_after.retired_task_ids, [a]);
    let current = list(&mut e, kind);
    let usage = e.host.store_local().pending_usage().unwrap();
    drop(e);
    let mut e = Engine::open(&path).unwrap();
    let reopened = list(&mut e, kind);
    assert_eq!(actual(&e).encode(), changed.encode());
    let inspected = run(
        &mut e,
        json!({"action":"editor_commit_inspect","editor_commit":{"request_json":wire,"expected_revision":changed.summary().revision.to_string()}}),
    );
    history_keys(&inspected);
    assert_eq!(inspected["editor_commit"]["live_matches"], true);
    let mut request: Request = serde_json::from_str(&outer).unwrap();
    request.transport_json = outer.clone();
    let retry = serde_json::to_value(e.execute(request).unwrap()).unwrap();
    history_keys(&retry);
    assert_eq!(retry["editor_commit"], saved["editor_commit"]);
    assert_eq!(e.host.store_local().pending_usage().unwrap(), usage);
    e.host.store_local().integrity_check().unwrap();
    json!({"content_kind":kind,"initial_reply":initial,"mutation_replies":revisions,
        "before_save_source":hex(&before.encode()),"request_json":wire,"save_request_json":outer,
        "publication_and_intent_replies":context,"save_reply":saved,"current_reply":current,
        "reopened_reply":reopened,"inspect_reply":inspected,"retry_reply":retry})
}

#[test]
fn native_v2_classification_follows_current_revisions_and_preserves_historical_contract() {
    exercise(false);
}
#[test]
fn migrated_legacy_classification_follows_actual_migration_metadata_and_current_body_edits() {
    exercise(true);
}
#[test]
fn unknown_type_version_and_corrupt_origin_fail_closed_without_mutation_or_read_fallback() {
    let (_dir, _path, e) = fixture(true);
    let good = tasks_v2::decode(CARD, "original title", &actual(&e).body()).unwrap();
    let mut wrong_version = good.clone();
    wrong_version.version = 1;
    let mut wrong_digest = good.clone();
    wrong_digest.origin.as_mut().unwrap().source_sha256[0] ^= 1;
    let mut wrong_mapping = good.clone();
    wrong_mapping.origin.as_mut().unwrap().mapping[0].task_id = "invented".into();
    let mut wrong_card = good.clone();
    wrong_card.origin.as_mut().unwrap().card_id = "different-card".into();
    let mut unqualified_legacy = native_properties();
    unqualified_legacy.tasks[0].legacy_completed = true;
    for (index, (type_id, format, body)) in [
        ("custom-type", 2, good.encode_to_vec()),
        ("idea", 1, good.encode_to_vec()),
        ("idea", 2, vec![255]),
        ("idea", 2, wrong_version.encode_to_vec()),
        ("idea", 2, wrong_digest.encode_to_vec()),
        ("idea", 2, wrong_mapping.encode_to_vec()),
        ("idea", 2, wrong_card.encode_to_vec()),
        ("idea", 2, unqualified_legacy.encode_to_vec()),
    ]
    .into_iter()
    .enumerate()
    {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmos-development.sqlite");
        let mut e = Engine::open(&path).unwrap();
        let card = CardRecord::new(CARD, type_id, format, "original title", body).unwrap();
        e.host
            .store_local_mut()
            .create_local(&format!("bad-seed-{index}"), &card)
            .unwrap();
        let usage = e.host.store_local().pending_usage().unwrap();
        assert!(
            e.execute(req(json!({"action":"list"}))).is_err(),
            "case {index}"
        );
        assert_eq!(e.effect, "not_committed");
        assert_eq!(actual(&e).encode(), card.encode());
        assert_eq!(e.host.store_local().pending_usage().unwrap(), usage);
        drop(e);
        let mut e = Engine::open(&path).unwrap();
        assert!(
            e.execute(req(json!({"action":"list"}))).is_err(),
            "reopened {index}"
        );
        assert_eq!(actual(&e).encode(), card.encode());
        assert_eq!(e.host.store_local().pending_usage().unwrap(), usage);
        e.host.store_local().integrity_check().unwrap();
    }
}
#[test]
fn empty_native_and_current_editor_marker_remain_v2_without_new_continuation_authority() {
    let (_dir, _path, mut e) = fixture(false);
    // Native TaskIds resembling migration ids were classified v2 above; an
    // empty native schema2 is also v2, independent of task count or any marker.
    let empty_dir = tempfile::tempdir().unwrap();
    let mut e_empty = Engine::open(&empty_dir.path().join("hmos-development.sqlite")).unwrap();
    let reply = run(
        &mut e_empty,
        json!({"action":"create","operation":"empty-native","id":"empty-card","title":"empty","category":"灵感","stage":"待整理"}),
    );
    assert_eq!(reply["cards"][0]["content_kind"], "v2");
    assert_eq!(reply["cards"][0]["tasks"], json!([]));
    let (wire, _outer, _saved, _context) = current_save(&mut e);
    let now = actual(&e);
    let p = tasks_v2::decode(CARD, &now.summary().title, &now.body()).unwrap();
    assert!(p.origin.is_none());
    list(&mut e, "v2");
    // A classification never supplies an owned raw-LF baseline/root. The
    // standalone current_v2 operation cannot become a continued_todos root.
    let mut value: Value = serde_json::from_str(&wire).unwrap();
    value["mode"] = json!("continued_todos");
    value["business"]["operation"] = json!("invented-continuation");
    value["business"]["source"] = json!(hex(&now.encode()));
    value["business"]["todos"] = json!("invented LF");
    let before = e.host.store_local().pending_usage().unwrap();
    assert!(
        e.execute(req(
            json!({"action":"editor_save","editor_save":{"request_json":value.to_string()}})
        ))
        .is_err()
    );
    assert_eq!(actual(&e).encode(), now.encode());
    assert_eq!(e.host.store_local().pending_usage().unwrap(), before);
}
#[test]
#[ignore = "explicit actual-Store DTO output path required"]
fn actual_store_card_source_fixture() {
    let path = std::env::var_os("HMOS_CARD_SOURCE_DTO_PATH").expect("explicit DTO path");
    let value = json!({"schema_version":1,"qualification":"actual-development-store-card-source-v27",
        "cases":[exercise(false),exercise(true)]});
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}
