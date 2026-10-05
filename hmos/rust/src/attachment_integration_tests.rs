//! Actual JSON/stream Engine scenarios, also used by the isolated native runner.
#![allow(dead_code)]
use crate::{Engine, Request};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Read, Write},
    path::Path,
};

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn run(engine: &mut Engine, value: Value) -> Value {
    serde_json::to_value(engine.execute(request(value)).unwrap()).unwrap()
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn raw(text: &str) -> Value {
    json!({"text":text,"selection_base":-1,"selection_extent":-1,"affinity":0,"directional":false,"composing_start":-1,"composing_end":-1})
}
fn values(title: &str, description: &str, category: &str, stage: &str) -> Value {
    json!({"title":raw(title),"description":raw(description),"hypothesis":raw("kept hypothesis"),"conclusion":raw("kept finding"),
        "todos":raw(""),"category":category,"stage":stage})
}
fn draft_save(
    engine: &mut Engine,
    id: &str,
    draft: &str,
    kind: u32,
    revision: &str,
    expected: &str,
    operation: &str,
    values: &Value,
    assets: Value,
) -> Value {
    run(engine,json!({"action":"draft_save","draft":{"card_id":id,"draft_id":draft,"source_kind":kind,"source_revision":revision,
        "expected_generation":expected,"operation_id":operation,"values":values,"assets":assets}}))["drafts"][0].clone()
}
fn card(reply: &Value, id: &str) -> Value {
    reply["cards"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()
        .clone()
}
fn import_wire(
    id: &str,
    draft: &str,
    generation: &str,
    operation: &str,
    name: &str,
    kind: &str,
    bytes: &[u8],
) -> Value {
    json!({"action":"import_file","import_request":{"card_id":id,"draft_id":draft,"expected_generation":generation,"operation_id":operation,
        "name":name,"kind":kind,"byte_length":bytes.len().to_string(),"sha256":digest(bytes)}})
}
fn import_file(engine: &mut Engine, root: &Path, value: Value, bytes: &[u8], name: &str) -> Value {
    let spool = root.join(name);
    fs::write(&spool, bytes).unwrap();
    let reply = {
        let mut file = fs::File::open(&spool).unwrap();
        serde_json::to_value(engine.import_from(request(value), &mut file).unwrap()).unwrap()
    };
    fs::remove_file(&spool).unwrap();
    assert!(!spool.exists());
    reply["imports"][0].clone()
}
fn selection(id: &Value) -> Value {
    json!({"origin":2,"asset_id":id,"aliases":["attachment:raw-alias","😀"]})
}
fn export(engine: &mut Engine, value: Value, expected: &[u8]) {
    let mut bytes = Vec::new();
    let info = serde_json::to_value(engine.export_to(request(value), &mut bytes).unwrap()).unwrap();
    assert_eq!(bytes, expected);
    assert_eq!(info["byte_length"], expected.len().to_string());
    assert_eq!(info["sha256"], digest(expected));
}
fn fresh(root: &Path) -> Engine {
    fs::create_dir(root).expect("requires a NEW isolated fixture directory");
    Engine::open(&root.join("hmos-development.sqlite")).unwrap()
}

fn publication(
    action: &str,
    id: &str,
    source: &Value,
    draft: &Value,
    operation: &str,
    values: &Value,
) -> Value {
    json!({"action":action,"id":id,"source":source,"operation":operation,"draft_id":draft["scope"]["draft_id"],
        "generation":draft["generation"],"draft_operation":draft["operation_id"],"title":values["title"]["text"],
        "description":values["description"]["text"],"hypothesis":values["hypothesis"]["text"],"conclusion":values["conclusion"]["text"],
        "category":values["category"],"stage":values["stage"]})
}
fn reject(engine: &mut Engine, value: Value) -> String {
    engine.execute(request(value)).unwrap_err()
}

/// A complete existing/new-card import, confirmation, publication, retry,
/// removal and restart flow. Every operation uses actual Engine JSON + streams.
pub fn run_round_trip(root: &Path) -> Value {
    let mut engine = fresh(root);
    let first = run(
        &mut engine,
        json!({"action":"create","id":"existing-card","operation":"existing-create","title":"Existing card",
        "description":"original body","hypothesis":"kept hypothesis","conclusion":"kept finding","category":"实验","stage":"验证中"}),
    );
    let mut baseline = card(&first, "existing-card");
    for task in ["task-one", "task-two"] {
        let reply = run(
            &mut engine,
            json!({"action":"task_add","id":"existing-card","operation":format!("add-{task}"),
            "source":baseline["source"],"task_id":task,"text":"same task text"}),
        );
        baseline = card(&reply, "existing-card");
    }
    baseline = card(
        &run(
            &mut engine,
            json!({"action":"task_toggle","id":"existing-card","operation":"complete-one","source":baseline["source"],"task_id":"task-one","flag":true}),
        ),
        "existing-card",
    );
    let earlier_source = baseline["source"].clone();
    baseline = card(
        &run(
            &mut engine,
            json!({"action":"favorite","id":"existing-card","operation":"favorite-existing","source":baseline["source"],"flag":true}),
        ),
        "existing-card",
    );
    let mut edited = values(
        "Existing card",
        "file draft before autosave",
        "实验",
        "验证中",
    );
    draft_save(
        &mut engine,
        "existing-card",
        "existing-draft",
        0,
        baseline["revision"].as_str().unwrap(),
        "0",
        "existing-journal-first",
        &edited,
        json!([]),
    );
    let created_values = values("New file card", "new attachment body", "灵感", "待整理");
    draft_save(
        &mut engine,
        "new-card",
        "new-draft",
        1,
        "0",
        "0",
        "new-journal-first",
        &created_values,
        json!([]),
    );
    let bytes: Vec<u8> = (0..98_321).map(|n| (n % 251) as u8).collect();
    let imported = import_file(
        &mut engine,
        root,
        import_wire(
            "existing-card",
            "existing-draft",
            "1",
            "large-import",
            "跨块原文.bin",
            "file",
            &bytes,
        ),
        &bytes,
        "selected-large.spool",
    );
    let empty = import_file(
        &mut engine,
        root,
        import_wire(
            "existing-card",
            "existing-draft",
            "1",
            "empty-import",
            "empty.bin",
            "file",
            &[],
        ),
        &[],
        "selected-empty.spool",
    );
    assert_eq!(imported["phase"], "ready");
    assert_eq!(empty["request"]["sha256"], digest(&[]));
    export(
        &mut engine,
        json!({"action":"import_export","id":"existing-card","draft_id":"existing-draft","generation":"1","operation":"large-import"}),
        &bytes,
    );
    let selected = json!([
        selection(&imported["asset_id"]),
        selection(&empty["asset_id"])
    ]);
    let pinned = draft_save(
        &mut engine,
        "existing-card",
        "existing-draft",
        0,
        baseline["revision"].as_str().unwrap(),
        "1",
        "existing-journal-pins",
        &edited,
        selected.clone(),
    );
    assert_eq!(pinned["assets"].as_array().unwrap().len(), 2);
    assert_eq!(pinned["consumed_imports"].as_array().unwrap().len(), 2);
    edited["description"] = raw("text autosave keeps both selected pins");
    let confirmed = draft_save(
        &mut engine,
        "existing-card",
        "existing-draft",
        0,
        baseline["revision"].as_str().unwrap(),
        "2",
        "existing-journal-autosave",
        &edited,
        selected.clone(),
    );
    assert_eq!(confirmed["consumed_imports"], json!([]));
    assert_eq!(confirmed["values"]["assets"], selected);
    assert_eq!(
        run(&mut engine, json!({"action":"list"}))["cards"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let query = run(
        &mut engine,
        json!({"action":"query","section":"概览","filter":"全部","text":"","sort":"最近添加"}),
    );
    assert_eq!(query["ids"], json!(["existing-card"]));
    drop(engine);
    let mut engine = Engine::open(&root.join("hmos-development.sqlite")).unwrap();
    export(
        &mut engine,
        json!({"action":"draft_asset_export","id":"existing-card","draft_id":"existing-draft","generation":"3","attachment_id":imported["asset_id"]}),
        &bytes,
    );
    export(
        &mut engine,
        json!({"action":"draft_asset_export","id":"existing-card","draft_id":"existing-draft","generation":"3","attachment_id":empty["asset_id"]}),
        &[],
    );
    let command = publication(
        "edit",
        "existing-card",
        &baseline["source"],
        &confirmed,
        "existing-business-publish",
        &edited,
    );
    let published = run(&mut engine, command.clone());
    let published_card = card(&published, "existing-card");
    assert_eq!(published_card["assets"].as_array().unwrap().len(), 2);
    assert_eq!(published_card["tasks"], baseline["tasks"]);
    assert_eq!(published_card["favorite"], true);
    assert_eq!(published_card["category"], "实验");
    assert_eq!(published_card["stage"], "验证中");
    assert_eq!(published_card["hypothesis"], baseline["hypothesis"]);
    assert_eq!(published_card["conclusion"], baseline["conclusion"]);
    let repeated = run(&mut engine, command.clone());
    assert_eq!(repeated["receipt_revision"], published["receipt_revision"]);
    assert_eq!(repeated["cards"], published["cards"]);
    let mut changed = command.clone();
    changed["description"] = json!("modified retry body");
    assert!(reject(&mut engine, changed).contains("Mismatch"));
    let mut changed = command.clone();
    changed["source"] = earlier_source;
    assert!(reject(&mut engine, changed).contains("source"));
    let mut changed = command.clone();
    changed["generation"] = json!("2");
    assert!(reject(&mut engine, changed).contains("generation"));
    let mut changed = command.clone();
    changed["draft_operation"] = json!("other-draft-save");
    assert!(!reject(&mut engine, changed).is_empty());
    // A lost business reply can coexist with later raw input. The old business
    // retry reconstructs its exact historical draft, never the later generation.
    let mut later_values = edited.clone();
    later_values["description"] = raw("later unsubmitted raw input");
    let later = draft_save(
        &mut engine,
        "existing-card",
        "existing-draft",
        0,
        baseline["revision"].as_str().unwrap(),
        "3",
        "later-raw-generation",
        &later_values,
        selected,
    );
    assert_eq!(later["generation"], "4");
    assert_eq!(
        run(&mut engine, command.clone())["receipt_revision"],
        published["receipt_revision"]
    );
    run(
        &mut engine,
        json!({"action":"draft_discard","id":"existing-card","draft_id":"existing-draft","generation":"4","operation":"retire-existing-draft"}),
    );
    assert_eq!(
        run(&mut engine, command.clone())["receipt_revision"],
        published["receipt_revision"]
    );
    let mut new_operation = command.clone();
    new_operation["operation"] = json!("unproven-new-business");
    assert!(!reject(&mut engine, new_operation).is_empty());
    // Select an imported asset into a new card and retry create after commit.
    let gif = b"GIF89a native new-card payload";
    let new_import = import_file(
        &mut engine,
        root,
        import_wire(
            "new-card",
            "new-draft",
            "1",
            "new-import",
            "新卡.gif",
            "gif",
            gif,
        ),
        gif,
        "new-card.spool",
    );
    let new_pin = draft_save(
        &mut engine,
        "new-card",
        "new-draft",
        1,
        "0",
        "1",
        "new-journal-pins",
        &created_values,
        json!([selection(&new_import["asset_id"])]),
    );
    let create_command = publication(
        "create",
        "new-card",
        &json!(""),
        &new_pin,
        "new-business-publish",
        &created_values,
    );
    let created = run(&mut engine, create_command.clone());
    assert_eq!(card(&created, "new-card")["assets"][0]["kind"], "gif");
    assert_eq!(
        run(&mut engine, create_command.clone())["receipt_revision"],
        "1"
    );
    let mut new_later = created_values.clone();
    new_later["title"] = raw("later raw new card input");
    draft_save(
        &mut engine,
        "new-card",
        "new-draft",
        1,
        "0",
        "2",
        "new-later-raw",
        &new_later,
        json!([selection(&new_import["asset_id"])]),
    );
    assert_eq!(
        run(&mut engine, create_command.clone())["receipt_revision"],
        "1"
    );
    run(
        &mut engine,
        json!({"action":"draft_discard","id":"new-card","draft_id":"new-draft","generation":"3","operation":"retire-new-draft"}),
    );
    assert_eq!(
        run(&mut engine, create_command.clone())["receipt_revision"],
        "1"
    );
    let mut changed = create_command.clone();
    changed["title"] = json!("changed create retry");
    assert!(!reject(&mut engine, changed).is_empty());
    // Remove one selected reference while preserving the empty file, TaskIds
    // and all other business fields through the original V2 transformer.
    let inventory = published_card["assets"].as_array().unwrap();
    let all: Vec<Value> = inventory
        .iter()
        .map(|a| json!({"origin":0,"asset_id":a["id"],"aliases":[]}))
        .collect();
    draft_save(
        &mut engine,
        "existing-card",
        "removal-draft",
        0,
        published_card["revision"].as_str().unwrap(),
        "0",
        "removal-journal-first",
        &edited,
        json!(all),
    );
    let remaining = json!([{"origin":0,"asset_id":empty["asset_id"],"aliases":[]}]);
    let removed = draft_save(
        &mut engine,
        "existing-card",
        "removal-draft",
        0,
        published_card["revision"].as_str().unwrap(),
        "1",
        "removal-journal-selected",
        &edited,
        remaining,
    );
    let removed_card = card(
        &run(
            &mut engine,
            publication(
                "edit",
                "existing-card",
                &published_card["source"],
                &removed,
                "remove-large-reference",
                &edited,
            ),
        ),
        "existing-card",
    );
    assert_eq!(removed_card["assets"].as_array().unwrap().len(), 1);
    assert_eq!(removed_card["assets"][0]["id"], empty["asset_id"]);
    for field in [
        "title",
        "description",
        "hypothesis",
        "conclusion",
        "category",
        "stage",
        "favorite",
        "tasks",
    ] {
        assert_eq!(removed_card[field], published_card[field]);
    }
    assert!(engine.export_to(request(json!({"action":"attachment_export","id":"existing-card","source":removed_card["source"],"generation":removed_card["revision"],"attachment_id":imported["asset_id"]})),&mut Vec::new()).is_err());
    drop(engine);
    let mut engine = Engine::open(&root.join("hmos-development.sqlite")).unwrap();
    let final_cards = run(&mut engine, json!({"action":"list"}));
    let existing = card(&final_cards, "existing-card");
    let new = card(&final_cards, "new-card");
    export(
        &mut engine,
        json!({"action":"attachment_export","id":"existing-card","source":existing["source"],"generation":existing["revision"],"attachment_id":empty["asset_id"]}),
        &[],
    );
    export(
        &mut engine,
        json!({"action":"attachment_export","id":"new-card","source":new["source"],"generation":new["revision"],"attachment_id":new_import["asset_id"]}),
        gif,
    );
    json!({"round_trip":"PASS","cross_32k_bytes":bytes.len(),"empty_payload":"PASS","selected_pin_autosave_restart":"PASS", "existing_and_new_publication":"PASS",
        "business_retry_after_new_raw_generation_and_discard":"PASS","remove_reference_preserves_fields_and_taskids":"PASS","spool_disappearance_export":"PASS"})
}

struct BrokenReader {
    data: Cursor<Vec<u8>>,
    delivered: bool,
}
impl Read for BrokenReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if self.delivered {
            return Err(std::io::Error::other("provider read failed"));
        }
        self.delivered = true;
        let limit = output.len().min(7);
        self.data.read(&mut output[..limit])
    }
}
struct BrokenWriter;
impl Write for BrokenWriter {
    fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::other("output write failed"))
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn run_failed_streams(root: &Path) -> Value {
    let mut engine = fresh(root);
    let original = run(
        &mut engine,
        json!({"action":"create","id":"stream-source","operation":"stream-source-create","title":"Unchanged business",
        "description":"original body","category":"灵感","stage":"待整理"}),
    );
    let source = card(&original, "stream-source");
    let values = values("Unchanged business", "raw draft", "灵感", "待整理");
    let initial = draft_save(
        &mut engine,
        "stream-source",
        "stream-draft",
        0,
        "1",
        "0",
        "stream-owner",
        &values,
        json!([]),
    );
    let bytes = b"provider stream payload";
    let mut bad = import_wire(
        "stream-source",
        "stream-draft",
        "1",
        "bad-hash",
        "bad.bin",
        "file",
        bytes,
    );
    bad["import_request"]["sha256"] = json!("00".repeat(32));
    assert!(
        engine
            .import_from(request(bad), &mut Cursor::new(bytes))
            .is_err()
    );
    let request_value = import_wire(
        "stream-source",
        "stream-draft",
        "1",
        "broken-stream",
        "broken.bin",
        "file",
        bytes,
    );
    assert!(
        engine
            .import_from(
                request(request_value),
                &mut BrokenReader {
                    data: Cursor::new(bytes.to_vec()),
                    delivered: false
                }
            )
            .is_err()
    );
    let short = import_wire(
        "stream-source",
        "stream-draft",
        "1",
        "short-stream",
        "short.bin",
        "file",
        bytes,
    );
    assert!(
        engine
            .import_from(request(short), &mut Cursor::new(&bytes[..3]))
            .is_err()
    );
    let extra = import_wire(
        "stream-source",
        "stream-draft",
        "1",
        "extra-empty",
        "extra.bin",
        "file",
        &[],
    );
    assert!(
        engine
            .import_from(request(extra), &mut Cursor::new(b"unexpected"))
            .is_err()
    );
    let pending = run(
        &mut engine,
        json!({"action":"import_list","id":"stream-source","draft_id":"stream-draft"}),
    );
    assert_eq!(pending["imports"].as_array().unwrap().len(), 4);
    for record in pending["imports"].as_array().unwrap() {
        assert_eq!(record["phase"], "pending");
        assert_eq!(record["bytes_retained"], false);
    }
    let unchanged = run(
        &mut engine,
        json!({"action":"draft_read","id":"stream-source","draft_id":"stream-draft"}),
    );
    assert_eq!(unchanged["drafts"][0], initial);
    assert_eq!(
        run(&mut engine, json!({"action":"list"}))["cards"],
        original["cards"]
    );
    let valid = import_file(
        &mut engine,
        root,
        import_wire(
            "stream-source",
            "stream-draft",
            "1",
            "valid-after-failures",
            "valid.bin",
            "file",
            bytes,
        ),
        bytes,
        "valid.spool",
    );
    let confirmed = draft_save(
        &mut engine,
        "stream-source",
        "stream-draft",
        0,
        "1",
        "1",
        "stream-selected",
        &values,
        json!([selection(&valid["asset_id"])]),
    );
    let mut command = publication(
        "edit",
        "stream-source",
        &source["source"],
        &confirmed,
        "stream-business",
        &values,
    );
    command["title"] = json!("different from confirmed raw");
    assert!(reject(&mut engine, command).contains("Mismatch"));
    assert_eq!(
        run(&mut engine, json!({"action":"list"}))["cards"],
        original["cards"]
    );
    assert!(engine.export_to(request(json!({"action":"draft_asset_export","id":"stream-source","draft_id":"stream-draft","generation":"2","attachment_id":valid["asset_id"]})),&mut BrokenWriter).is_err());
    let published = run(
        &mut engine,
        publication(
            "edit",
            "stream-source",
            &source["source"],
            &confirmed,
            "stream-business",
            &values,
        ),
    );
    let current = card(&published, "stream-source");
    assert!(engine.export_to(request(json!({"action":"attachment_export","id":"stream-source","source":current["source"],"generation":current["revision"],"attachment_id":valid["asset_id"]})),&mut BrokenWriter).is_err());
    assert_eq!(
        run(&mut engine, json!({"action":"list"}))["cards"],
        published["cards"]
    );
    assert!(engine.export_to(request(json!({"action":"attachment_export","id":"stream-source","source":source["source"],"generation":source["revision"],"attachment_id":valid["asset_id"]})),&mut Vec::new()).is_err());
    json!({"invalid_hash_and_stream_faults":"PASS","pending_is_not_admitted_asset":"PASS","business_unchanged_on_failure":"PASS","writer_errors_and_stale_source_export":"PASS"})
}

pub fn run_capacity(root: &Path) -> Value {
    let mut engine = fresh(root);
    let values = values("Unsubmitted owner", "private", "灵感", "待整理");
    draft_save(
        &mut engine,
        "capacity-import-target",
        "capacity-draft",
        1,
        "0",
        "0",
        "capacity-journal",
        &values,
        json!([]),
    );
    let bytes = b"capacity private import";
    import_file(
        &mut engine,
        root,
        import_wire(
            "capacity-import-target",
            "capacity-draft",
            "1",
            "capacity-import",
            "capacity.bin",
            "file",
            bytes,
        ),
        bytes,
        "capacity.spool",
    );
    for index in 0..256 {
        run(
            &mut engine,
            json!({"action":"create","id":format!("capacity-card-{index:03}"),"operation":format!("capacity-create-{index}"),
            "title":format!("Card {index}"),"description":"","category":"灵感","stage":"待整理"}),
        );
    }
    let cards = run(&mut engine, json!({"action":"list"}));
    assert_eq!(cards["cards"].as_array().unwrap().len(), 256);
    let query = run(
        &mut engine,
        json!({"action":"query","section":"概览","filter":"全部","text":"","sort":"标题排序"}),
    );
    assert_eq!(query["ids"].as_array().unwrap().len(), 256);
    assert!(
        query["ids"]
            .as_array()
            .unwrap()
            .iter()
            .all(|id| id.as_str().unwrap().starts_with("capacity-card-"))
    );
    let overflow = reject(
        &mut engine,
        json!({"action":"create","id":"capacity-overflow","operation":"capacity-overflow","title":"Overflow","category":"灵感","stage":"待整理"}),
    );
    assert!(overflow.contains("DevelopmentCardLimit"));
    assert_eq!(
        run(&mut engine, json!({"action":"list"}))["cards"],
        cards["cards"]
    );
    let imports = run(
        &mut engine,
        json!({"action":"import_list","id":"capacity-import-target","draft_id":"capacity-draft"}),
    );
    assert_eq!(imports["imports"].as_array().unwrap().len(), 1);
    json!({"private_imports_excluded_from_query_and_256_limit":"PASS","business_cards":256,"overflow_rejected":"PASS"})
}

#[cfg(test)]
#[test]
fn actual_engine_existing_and_new_asset_journals_publication_history_restart_and_removal() {
    let temp = tempfile::tempdir().unwrap();
    run_round_trip(&temp.path().join("round-trip"));
}
#[cfg(test)]
#[test]
fn actual_engine_stream_failures_preserve_business_and_do_not_admit_pending_imports() {
    let temp = tempfile::tempdir().unwrap();
    run_failed_streams(&temp.path().join("stream-failures"));
}
#[cfg(test)]
#[test]
fn actual_engine_private_imports_do_not_count_toward_256_business_cards() {
    let temp = tempfile::tempdir().unwrap();
    run_capacity(&temp.path().join("capacity"));
}
