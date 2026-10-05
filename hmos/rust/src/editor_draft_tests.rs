use super::*;
use morrow_core::{
    store::{EventBudget, Store},
    versioned_content_change::VersionedContentChange,
};

fn fixture() -> (tempfile::TempDir, std::path::PathBuf, HostRuntime) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let host = reopen(&path);
    (directory, path, host)
}
fn reopen(path: &std::path::Path) -> HostRuntime {
    HostRuntime::new(Store::open(path, EventBudget::default()).unwrap()).unwrap()
}
fn value(text: &str) -> proto::TextValue {
    proto::TextValue {
        text: text.into(),
        selection_base: -1,
        selection_extent: -1,
        composing_start: -1,
        composing_end: -1,
        ..Default::default()
    }
}
fn request(operation: &str, generation: u64) -> proto::WriteRequest {
    proto::WriteRequest {
        schema_version: 1,
        card_id: "draft-target".into(),
        draft_id: "draft-one".into(),
        operation_id: operation.into(),
        expected_generation: generation,
        source_kind: 1,
        source_revision: 0,
        values: Some(proto::Values {
            title: Some(value("")),
            description: Some(value("raw description")),
            hypothesis: Some(value("raw hypothesis")),
            conclusion: Some(value("raw conclusion")),
            todos: Some(value("same\nsame")),
            category: "灵感".into(),
            stage: "待整理".into(),
        }),
        ..Default::default()
    }
}
fn seed(host: &mut HostRuntime, id: &str) -> CardRecord {
    let properties = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        description: "formal source".into(),
        ..Default::default()
    };
    let card = CardRecord::new(id, "idea", 2, "Formal source", properties.encode_to_vec()).unwrap();
    let mut raw = card.encode();
    // A valid unknown outer property must survive an exact draft source copy.
    raw.extend_from_slice(&[0xaa, 0x06, 0x02, 0x01, 0xff]);
    let card = CardRecord::decode(&raw).unwrap();
    host.store_local_mut()
        .create_local(&format!("seed-{id}"), &card)
        .unwrap();
    card
}
fn existing(operation: &str, generation: u64, source: &CardRecord) -> proto::WriteRequest {
    let mut request = request(operation, generation);
    request.card_id = source.summary().id;
    request.source_kind = 0;
    request.source_revision = source.summary().revision;
    request
}
fn private_journal(host: &HostRuntime, request: &proto::WriteRequest) -> CardRecord {
    host.store_local()
        .card(&key(&request.card_id, &request.draft_id))
        .unwrap()
        .unwrap()
}

#[test]
fn raw_empty_values_utf16_selection_and_composing_survive_restart_without_business_card() {
    let (_directory, path, mut host) = fixture();
    let mut original = request("raw-first", 0);
    original.values.as_mut().unwrap().description = Some(proto::TextValue {
        text: "A😀中文".into(),
        selection_base: 2,
        selection_extent: 4,
        affinity: 1,
        directional: true,
        composing_start: 1,
        composing_end: 3,
    });
    let saved = save(&mut host, &original, || 1).unwrap();
    assert_eq!(saved.slot.request, Some(original.clone()));
    assert_eq!(saved.slot.generation, 1);
    assert!(saved.slot.source_card.is_empty());
    assert!(host.store_local().card("draft-target").unwrap().is_none());
    let journal = private_journal(&host, &original);
    assert!(is_journal(&journal));
    validate_journal(&journal).unwrap();
    assert_eq!(journal.summary().type_id, TYPE);
    assert_eq!(saved.slot.active_bytes, saved.slot.encoded_len() as u64);
    drop(host);
    let host = reopen(&path);
    let restored = read(&host, "draft-target", "draft-one").unwrap().unwrap();
    assert_eq!(restored.slot, saved.slot);
    assert_eq!(list(&host).unwrap().len(), 1);
    assert!(host.store_local().card("draft-target").unwrap().is_none());
}

#[test]
fn exact_history_never_rewinds_newer_snapshot_and_modified_retry_is_rejected() {
    let (_directory, path, mut host) = fixture();
    let first = request("history-first", 0);
    save(&mut host, &first, || 1).unwrap();
    let mut second = request("history-second", 1);
    second.values.as_mut().unwrap().title = Some(value("newer raw text"));
    save(&mut host, &second, || 1).unwrap();
    let current_bytes = private_journal(&host, &first).encode();
    let mut effect = "unknown";
    let historical = save_with_effect(
        &mut host,
        &first,
        || panic!("read-only history must not use a clock"),
        &mut effect,
    )
    .unwrap();
    assert_eq!(effect, "committed");
    assert!(historical.repeated);
    assert_eq!(historical.slot.generation, 1);
    assert_eq!(historical.current_generation, 2);
    assert_eq!(historical.slot.request, Some(first.clone()));
    assert_eq!(private_journal(&host, &first).encode(), current_bytes);
    let mut changed = first.clone();
    changed.values.as_mut().unwrap().description = Some(value("changed payload"));
    assert!(
        save(&mut host, &changed, || 1)
            .unwrap_err()
            .contains("retry payload changed")
    );
    assert_eq!(private_journal(&host, &first).encode(), current_bytes);
    drop(host);
    let host = reopen(&path);
    assert_eq!(
        read(&host, "draft-target", "draft-one")
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(second)
    );
}

#[test]
fn generation_cas_and_baseline_are_fixed_while_original_business_bytes_stay_unchanged() {
    let (_directory, _path, mut host) = fixture();
    let source = seed(&mut host, "existing-card");
    let original_bytes = source.encode();
    let first = existing("existing-first", 0, &source);
    save(&mut host, &first, || 1).unwrap();
    assert_eq!(
        read(&host, "existing-card", "draft-one")
            .unwrap()
            .unwrap()
            .slot
            .source_card,
        original_bytes
    );
    assert_eq!(
        host.store_local()
            .card("existing-card")
            .unwrap()
            .unwrap()
            .encode(),
        original_bytes
    );
    let current = private_journal(&host, &first).encode();
    let stale = existing("existing-stale", 0, &source);
    assert!(
        save(&mut host, &stale, || 1)
            .unwrap_err()
            .contains("generation conflict")
    );
    let mut changed = existing("existing-baseline-change", 1, &source);
    changed.source_revision += 1;
    assert!(
        save(&mut host, &changed, || 1)
            .unwrap_err()
            .contains("baseline cannot silently change")
    );
    assert_eq!(private_journal(&host, &first).encode(), current);
    // Ordinary content advances independently; a later raw journal generation
    // remains anchored to the original source rather than silently rebasing.
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::EditContent,
        "existing-card",
        60_000,
        1,
    )
    .unwrap();
    let change = VersionedContentChange {
        operation_id: "foreign-source-edit".into(),
        source_card: original_bytes.clone(),
        title: "Foreign newer source".into(),
        body: source.body(),
        preview_text: String::new(),
        attachments: None,
    };
    host.edit_versioned_content(&connection, &change, || 1)
        .unwrap();
    host.disconnect(&connection).unwrap();
    let second = existing("existing-second", 1, &source);
    let saved = save(&mut host, &second, || 1).unwrap();
    assert_eq!(saved.slot.source_card, original_bytes);
    assert_eq!(
        host.store_local()
            .card("existing-card")
            .unwrap()
            .unwrap()
            .summary()
            .title,
        "Foreign newer source"
    );
}

#[test]
fn new_card_baseline_remains_empty_after_foreign_create_and_existing_target_rejects_first_write() {
    let (_directory, _path, mut host) = fixture();
    let first = request("new-first", 0);
    save(&mut host, &first, || 1).unwrap();
    let formal = seed(&mut host, "draft-target");
    let second = request("new-second", 1);
    assert!(
        save(&mut host, &second, || 1)
            .unwrap()
            .slot
            .source_card
            .is_empty()
    );
    assert_eq!(
        host.store_local()
            .card("draft-target")
            .unwrap()
            .unwrap()
            .encode(),
        formal.encode()
    );
    let mut another = request("new-collision", 0);
    another.draft_id = "another-draft".into();
    assert!(
        save(&mut host, &another, || 1)
            .unwrap_err()
            .contains("target already exists")
    );
    assert!(
        read(&host, "draft-target", "another-draft")
            .unwrap()
            .is_none()
    );
}

#[test]
fn discard_is_exact_idempotent_inactive_and_old_save_history_remains_read_only() {
    let (_directory, path, mut host) = fixture();
    let first = request("discard-first", 0);
    save(&mut host, &first, || 1).unwrap();
    assert!(
        discard(
            &mut host,
            "draft-target",
            "draft-one",
            0,
            "discard-stale",
            || 1
        )
        .is_err()
    );
    let discarded = discard(
        &mut host,
        "draft-target",
        "draft-one",
        1,
        "discard-once",
        || 1,
    )
    .unwrap();
    assert!(!discarded.slot.active);
    assert_eq!(discarded.slot.generation, 2);
    assert_eq!(discarded.slot.active_bytes, 0);
    assert!(list(&host).unwrap().is_empty());
    let current = private_journal(&host, &first).encode();
    let old = save(&mut host, &first, || 1).unwrap();
    assert!(old.repeated);
    assert_eq!(old.slot.generation, 1);
    assert_eq!(old.current_generation, 2);
    assert!(!old.current_active);
    let mut effect = "unknown";
    let retry = discard_with_effect(
        &mut host,
        "draft-target",
        "draft-one",
        1,
        "discard-once",
        || panic!("history is read-only"),
        &mut effect,
    )
    .unwrap();
    assert!(retry.repeated);
    assert_eq!(effect, "committed");
    assert!(
        discard(
            &mut host,
            "draft-target",
            "draft-one",
            0,
            "discard-once",
            || 1
        )
        .is_err()
    );
    assert!(
        save(&mut host, &request("resurrect", 2), || 1)
            .unwrap_err()
            .contains("discarded draft identity")
    );
    assert_eq!(private_journal(&host, &first).encode(), current);
    drop(host);
    let host = reopen(&path);
    assert!(
        !read(&host, "draft-target", "draft-one")
            .unwrap()
            .unwrap()
            .slot
            .active
    );
}

#[test]
fn foreign_operation_cannot_save_or_discard_another_journal() {
    let (_directory, _path, mut host) = fixture();
    let first = request("shared-operation", 0);
    save(&mut host, &first, || 1).unwrap();
    let mut second = request("second-save", 0);
    second.draft_id = "draft-two".into();
    save(&mut host, &second, || 1).unwrap();
    let before = private_journal(&host, &second).encode();
    assert!(
        discard(
            &mut host,
            "draft-target",
            "draft-two",
            1,
            "shared-operation",
            || 1
        )
        .unwrap_err()
        .contains("another object")
    );
    second.operation_id = "shared-operation".into();
    second.expected_generation = 1;
    assert!(
        save(&mut host, &second, || 1)
            .unwrap_err()
            .contains("another object")
    );
    assert_eq!(private_journal(&host, &second).encode(), before);
}

#[test]
fn sixteen_active_slots_limit_and_discard_frees_only_active_capacity() {
    let (_directory, _path, mut host) = fixture();
    for index in 0..16 {
        let mut proposal = request(&format!("capacity-save-{index}"), 0);
        proposal.draft_id = format!("draft-{index}");
        save(&mut host, &proposal, || 1).unwrap();
    }
    let overflow = request("capacity-overflow", 0);
    assert!(
        save(&mut host, &overflow, || 1)
            .unwrap_err()
            .contains("active capacity")
    );
    assert_eq!(list(&host).unwrap().len(), 16);
    discard(
        &mut host,
        "draft-target",
        "draft-0",
        1,
        "capacity-discard",
        || 1,
    )
    .unwrap();
    save(&mut host, &overflow, || 1).unwrap();
    assert_eq!(list(&host).unwrap().len(), 16);
}

#[test]
fn cumulative_identity_limit_survives_discard_and_restart() {
    let (_directory, path, mut host) = fixture();
    for index in 0..256 {
        let mut proposal = request(&format!("identity-save-{index}"), 0);
        proposal.draft_id = format!("identity-{index}");
        save(&mut host, &proposal, || 1).unwrap();
        discard(
            &mut host,
            "draft-target",
            &proposal.draft_id,
            1,
            &format!("identity-discard-{index}"),
            || 1,
        )
        .unwrap();
    }
    assert!(list(&host).unwrap().is_empty());
    drop(host);
    let mut host = reopen(&path);
    assert!(
        save(&mut host, &request("identity-overflow", 0), || 1)
            .unwrap_err()
            .contains("identity capacity")
    );
    assert!(read(&host, "draft-target", "draft-one").unwrap().is_none());
}

#[test]
fn invalid_raw_shape_and_unsupported_capture_assets_are_rejected_without_a_journal() {
    let (_directory, _path, mut host) = fixture();
    let original = request("invalid-shape", 0);
    let mut invalid = original.clone();
    invalid
        .values
        .as_mut()
        .unwrap()
        .title
        .as_mut()
        .unwrap()
        .selection_base = 1;
    let mut effect = "unknown";
    assert!(save_with_effect(&mut host, &invalid, || 1, &mut effect).is_err());
    assert_eq!(effect, "not_committed");
    let mut asset = original.clone();
    asset.assets.push(proto::AssetSelection {
        origin: 2,
        asset_id: "asset-one".into(),
        aliases: vec![],
    });
    assert!(
        save(&mut host, &asset, || 1)
            .unwrap_err()
            .contains("DraftPhaseUnsupported")
    );
    let source = seed(&mut host, "captured-card");
    let mut predecessor = existing("captured-operation", 0, &source);
    predecessor.predecessor_operation = "business-commit".into();
    predecessor.predecessor_sha256 = vec![1; 32];
    assert!(
        save(&mut host, &predecessor, || 1)
            .unwrap_err()
            .contains("DraftPhaseUnsupported")
    );
    assert!(list(&host).unwrap().is_empty());
}

#[test]
fn noncanonical_or_corrupt_private_record_is_never_skipped_as_absence() {
    let (_directory, _path, mut host) = fixture();
    let original = request("corruption-first", 0);
    save(&mut host, &original, || 1).unwrap();
    let journal = private_journal(&host, &original);
    let mut body = journal.body();
    body.extend_from_slice(&[0xa0, 0x06, 0x01]);
    let malformed = CardRecord::new(&journal.summary().id, TYPE, 1, TITLE, body).unwrap();
    assert!(is_journal(&malformed));
    assert!(validate_journal(&malformed).is_err());
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::EditContent,
        &journal.summary().id,
        60_000,
        1,
    )
    .unwrap();
    host.edit_content(
        &connection,
        &ContentChange {
            operation_id: "corrupt-journal".into(),
            card_id: journal.summary().id,
            expected_revision: 1,
            title: TITLE.into(),
            body: malformed.body(),
            preview_text: String::new(),
            attachments: Some(Vec::new()),
        },
        || 1,
    )
    .unwrap();
    host.disconnect(&connection).unwrap();
    assert!(read(&host, "draft-target", "draft-one").is_err());
    assert!(list(&host).is_err());
    assert!(save(&mut host, &original, || 1).is_err());
}

#[test]
fn transaction_budget_failure_keeps_unknown_original_and_previous_journal_until_exact_retry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let store = Store::open(
        &path,
        EventBudget {
            max_count: 1,
            max_bytes: 64 * 1024 * 1024,
        },
    )
    .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let first = request("budget-first", 0);
    save(&mut host, &first, || 1).unwrap();
    let prior = private_journal(&host, &first).encode();
    let second = request("budget-second", 1);
    let mut effect = "unknown";
    assert_eq!(
        save_with_effect(&mut host, &second, || 1, &mut effect).unwrap_err(),
        "EventCapacity"
    );
    // EventCapacity is not one of the adapter's four proven rejection classes.
    // The original remains uncertain to the caller, even though this fixture
    // can inspect the authoritative Store and show the previous bytes intact.
    assert_eq!(effect, "unknown");
    assert!(matches!(
        host.store_local().lookup("budget-second").unwrap(),
        Lookup::Absent
    ));
    assert_eq!(private_journal(&host, &first).encode(), prior);
    drop(host);
    let mut host = reopen(&path);
    let saved = save_with_effect(&mut host, &second, || 1, &mut effect).unwrap();
    assert_eq!(effect, "committed");
    assert_eq!(saved.slot.generation, 2);
    assert_eq!(saved.slot.request, Some(second));
}

#[test]
fn development_workbench_list_and_snapshot_query_exclude_valid_private_journals() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let mut engine = crate::Engine::open(&path).unwrap();
    seed(&mut engine.host, "visible-business");
    let mut draft = request("private-draft-save", 0);
    draft.values.as_mut().unwrap().title = Some(value("PrivateOnlyNeedle"));
    save(&mut engine.host, &draft, || 1).unwrap();
    assert_eq!(engine.ids().unwrap(), ["visible-business"]);
    assert_eq!(engine.cards().unwrap().len(), 1);
    let query = |text: &str| {
        serde_json::from_value::<crate::Request>(serde_json::json!({
            "action": "query", "section": "概览", "filter": "全部",
            "text": text, "sort": "最近添加"
        }))
        .unwrap()
    };
    assert!(
        engine
            .execute(query("PrivateOnlyNeedle"))
            .unwrap()
            .ids
            .is_empty()
    );
    assert_eq!(
        engine.execute(query("formal source")).unwrap().ids,
        ["visible-business"]
    );
    assert_eq!(list(&engine.host).unwrap().len(), 1);
    drop(engine);
    let engine = crate::Engine::open(&path).unwrap();
    assert_eq!(engine.ids().unwrap(), ["visible-business"]);
    assert_eq!(list(&engine.host).unwrap().len(), 1);
}

fn corrupt_current_title(host: &mut HostRuntime, original: &proto::WriteRequest) {
    let journal = private_journal(host, original);
    let summary = journal.summary();
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::EditContent,
        &summary.id,
        60_000,
        1,
    )
    .unwrap();
    host.edit_content(
        &connection,
        &ContentChange {
            operation_id: format!("corrupt-current-{}", summary.revision),
            card_id: summary.id,
            expected_revision: summary.revision,
            title: "Corrupt current private journal".into(),
            body: journal.body(),
            preview_text: String::new(),
            attachments: Some(Vec::new()),
        },
        || 1,
    )
    .unwrap();
    host.disconnect(&connection).unwrap();
}

#[test]
fn exact_saved_history_stays_committed_when_current_journal_readback_is_corrupt() {
    let (_directory, _path, mut host) = fixture();
    let original = request("history-before-corrupt-current", 0);
    save(&mut host, &original, || 1).unwrap();
    corrupt_current_title(&mut host, &original);
    let before = private_journal(&host, &original).encode();
    let mut effect = "unknown";
    let failed = save_with_effect(
        &mut host,
        &original,
        || panic!("exact historical retry may not write"),
        &mut effect,
    )
    .unwrap_err();
    assert!(failed.contains("unsupported editor draft journal"));
    assert_eq!(effect, "committed");
    assert_eq!(private_journal(&host, &original).encode(), before);
    assert!(matches!(
        host.store_local().lookup(&original.operation_id).unwrap(),
        Lookup::Committed(_)
    ));
}

#[test]
fn exact_discard_history_stays_committed_when_current_journal_readback_is_corrupt() {
    let (_directory, _path, mut host) = fixture();
    let original = request("discard-history-before-corrupt", 0);
    save(&mut host, &original, || 1).unwrap();
    discard(
        &mut host,
        "draft-target",
        "draft-one",
        1,
        "confirmed-discard-before-corrupt",
        || 1,
    )
    .unwrap();
    corrupt_current_title(&mut host, &original);
    let before = private_journal(&host, &original).encode();
    let mut effect = "unknown";
    let failed = discard_with_effect(
        &mut host,
        "draft-target",
        "draft-one",
        1,
        "confirmed-discard-before-corrupt",
        || panic!("exact discard retry may not write"),
        &mut effect,
    )
    .unwrap_err();
    assert!(failed.contains("unsupported editor draft journal"));
    assert_eq!(effect, "committed");
    assert_eq!(private_journal(&host, &original).encode(), before);
    assert!(matches!(
        host.store_local()
            .lookup("confirmed-discard-before-corrupt")
            .unwrap(),
        Lookup::Committed(_)
    ));
}
