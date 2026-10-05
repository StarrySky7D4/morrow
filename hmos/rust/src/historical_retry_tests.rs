//! Real Engine retries must retain proven historical outcomes even when the
//! current private-journal census cannot be read. No production storage opens.
use super::*;
use morrow_core::{content_change::ContentChange, transaction::Lookup};
use serde_json::{Value, json};

const CARD: &str = "historical-business-card";

fn request(value: Value) -> Request {
    serde_json::from_value(value).unwrap()
}
fn text(value: &str) -> Value {
    json!({"text": value, "selection_base": -1, "selection_extent": -1,
        "composing_start": -1, "composing_end": -1, "affinity": 0, "directional": false})
}
fn values(title: &str) -> Value {
    json!({"title":text(title),"description":text("raw body"),"hypothesis":text("hypothesis"),
        "conclusion":text("conclusion"),"todos":text(""),"category":"灵感","stage":"待整理"})
}
fn seed_draft(
    engine: &mut Engine,
    id: &str,
    kind: u32,
    source_revision: &str,
    title: &str,
) -> draft_bridge::View {
    engine.execute(request(json!({"action":"draft_save","draft":{
        "card_id":id,"draft_id":"historical-draft","source_kind":kind,"source_revision":source_revision,
        "expected_generation":"0","operation_id":"historical-draft-save","values":values(title),"assets":[]
    }}))).unwrap().drafts.remove(0)
}
fn command(action: &str, source: &str, draft: Option<&draft_bridge::View>) -> Value {
    let mut wire = json!({"action":action,"id":CARD,"operation":format!("historical-{action}"),"source":source,
        "title":"saved title","description":"raw body","hypothesis":"hypothesis","conclusion":"conclusion",
        "category":"灵感","stage":"待整理"});
    if let Some(draft) = draft {
        let metadata = serde_json::to_value(draft).unwrap();
        wire["draft_id"] = metadata["scope"]["draft_id"].clone();
        wire["generation"] = metadata["generation"].clone();
        wire["draft_operation"] = metadata["operation_id"].clone();
    }
    wire
}
fn engine() -> (tempfile::TempDir, std::path::PathBuf, Engine) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let engine = Engine::open(&path).unwrap();
    (directory, path, engine)
}
fn corrupt_current_private_journal(engine: &mut Engine) {
    let id = engine
        .host
        .store_local()
        .card_ids_local("", 16)
        .unwrap()
        .into_iter()
        .find(|id| {
            engine
                .host
                .store_local()
                .card(id)
                .unwrap()
                .as_ref()
                .is_some_and(editor_draft::is_journal)
        })
        .unwrap();
    let journal = engine.host.store_local().card(&id).unwrap().unwrap();
    let revision = journal.summary().revision;
    let mut connection = engine.host.connect().unwrap();
    let now = u64::try_from(engine.start.elapsed().as_millis()).unwrap() + 1;
    engine
        .host
        .grant(
            &mut connection,
            GrantKind::EditContent,
            &id,
            now + 30_000,
            now,
        )
        .unwrap();
    let start = engine.start;
    engine
        .host
        .edit_content(
            &connection,
            &ContentChange {
                operation_id: "corrupt-historical-current-journal".into(),
                card_id: id,
                expected_revision: revision,
                title: "malformed private journal".into(),
                body: journal.body(),
                preview_text: String::new(),
                attachments: Some(vec![]),
            },
            || u64::try_from(start.elapsed().as_millis()).unwrap() + 1,
        )
        .unwrap();
    engine.host.disconnect(&connection).unwrap();
    assert!(engine.ids().is_err());
}

#[test]
fn committed_create_skips_new_census_and_stays_committed_after_current_draft_corruption() {
    for publish_draft in [false, true] {
        let (_directory, path, mut engine) = engine();
        let draft = seed_draft(&mut engine, CARD, 1, "0", "saved title");
        let wire = command("create", "", publish_draft.then_some(&draft));
        let saved = engine.execute(request(wire.clone())).unwrap();
        assert_eq!(saved.receipt_revision, "1");
        let card_before = engine
            .host
            .store_local()
            .card(CARD)
            .unwrap()
            .unwrap()
            .encode();
        corrupt_current_private_journal(&mut engine);
        drop(engine);
        let mut engine = Engine::open(&path).unwrap();
        // The exact Core replay succeeds; the final current card list fails.
        // That failure must never tell the UI to discard the original proposal.
        let error = engine.execute(request(wire)).unwrap_err();
        assert!(error.contains("draft"), "{error}");
        assert_eq!(engine.effect, "committed");
        assert_eq!(
            engine
                .host
                .store_local()
                .card(CARD)
                .unwrap()
                .unwrap()
                .encode(),
            card_before
        );
        assert!(
            matches!(engine.host.store_local().lookup_for_card(CARD, "historical-create").unwrap(), Lookup::Committed(receipt) if receipt.revision == 1)
        );
    }
}

#[test]
fn committed_edit_and_other_business_retry_keep_proof_on_later_current_read_failure() {
    for action in ["edit", "favorite"] {
        let (_directory, path, mut engine) = engine();
        let baseline = engine
            .execute(request(
                json!({"action":"create","id":CARD,"operation":"seed-business",
            "title":"baseline","category":"灵感","stage":"待整理"}),
            ))
            .unwrap()
            .cards
            .remove(0);
        let draft = seed_draft(&mut engine, CARD, 0, &baseline.revision, "saved title");
        let mut wire = command(
            action,
            &baseline.source,
            (action == "edit").then_some(&draft),
        );
        if action == "favorite" {
            wire["flag"] = true.into();
        }
        let saved = engine.execute(request(wire.clone())).unwrap();
        assert_eq!(saved.receipt_revision, "2");
        let card_before = engine
            .host
            .store_local()
            .card(CARD)
            .unwrap()
            .unwrap()
            .encode();
        corrupt_current_private_journal(&mut engine);
        drop(engine);
        let mut engine = Engine::open(&path).unwrap();
        assert!(engine.execute(request(wire)).unwrap_err().contains("draft"));
        assert_eq!(engine.effect, "committed");
        assert_eq!(
            engine
                .host
                .store_local()
                .card(CARD)
                .unwrap()
                .unwrap()
                .encode(),
            card_before
        );
        assert!(
            matches!(engine.host.store_local().lookup_for_card(CARD, &format!("historical-{action}")).unwrap(), Lookup::Committed(receipt) if receipt.revision == 2)
        );
    }
}

#[test]
fn authoritative_absence_and_failed_lookup_have_distinct_pre_write_effects() {
    let (_directory, _path, mut engine) = engine();
    seed_draft(
        &mut engine,
        "unpublished-draft-owner",
        1,
        "0",
        "saved title",
    );
    corrupt_current_private_journal(&mut engine);
    let absent = command("create", "", None);
    assert!(engine.execute(request(absent)).is_err());
    assert_eq!(engine.effect, "not_committed");
    assert!(engine.host.store_local().card(CARD).unwrap().is_none());
    assert!(matches!(
        engine
            .host
            .store_local()
            .lookup_for_card(CARD, "historical-create")
            .unwrap(),
        Lookup::Absent
    ));
    let mut lookup_error = command("create", "", None);
    lookup_error["operation"] = "".into();
    assert!(engine.execute(request(lookup_error)).is_err());
    assert_eq!(
        engine.effect, "unknown",
        "a failed lookup must never be treated as authoritative absence"
    );
    assert!(engine.host.store_local().card(CARD).unwrap().is_none());
}

#[test]
fn committed_operation_still_checks_complete_core_command_and_never_accepts_changed_payload() {
    let (_directory, _path, mut engine) = engine();
    let original = command("create", "", None);
    engine.execute(request(original.clone())).unwrap();
    let before = engine
        .host
        .store_local()
        .card(CARD)
        .unwrap()
        .unwrap()
        .encode();
    let mut changed = original.clone();
    changed["description"] = "different payload using the same operation".into();
    assert!(
        engine
            .execute(request(changed))
            .unwrap_err()
            .contains("OperationConflict")
    );
    assert_eq!(
        engine
            .host
            .store_local()
            .card(CARD)
            .unwrap()
            .unwrap()
            .encode(),
        before
    );
    let replay = engine.execute(request(original)).unwrap();
    assert_eq!(replay.effect, "committed");
    assert_eq!(replay.receipt_revision, "1");
}
