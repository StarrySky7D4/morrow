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
fn invalid_raw_shape_unowned_import_and_unsupported_capture_are_rejected_without_a_journal() {
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
            .contains("does not belong")
    );
    let source = seed(&mut host, "captured-card");
    let mut parent_asset = existing("unsupported-parent", 0, &source);
    parent_asset.assets.push(proto::AssetSelection {
        origin: 4,
        asset_id: "parent-asset".into(),
        aliases: vec![],
    });
    assert!(
        save(&mut host, &parent_asset, || 1)
            .unwrap_err()
            .contains("DraftPhaseUnsupported")
    );
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

fn seed_asset(host: &mut HostRuntime, id: &str, bytes: &[u8]) -> CardRecord {
    let blob = host
        .store_local_mut()
        .stage_blob(
            &mut std::io::Cursor::new(bytes),
            bytes.len() as u64,
            Some(Sha256::digest(bytes).into()),
            10,
        )
        .unwrap();
    let attachment = Attachment {
        id: "source-asset".into(),
        display_name: "original.png".into(),
        media_type: "image/png".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    };
    let mut properties = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        description: "formal source".into(),
        ..Default::default()
    };
    properties.assets.push(Default::default());
    let asset = properties.assets.last_mut().unwrap();
    asset.id = attachment.id.clone();
    asset.name = attachment.display_name.clone();
    asset.kind = "image".into();
    asset.bytes = attachment.byte_length;
    let source = CardRecord::new_with_attachments(
        id,
        "idea",
        2,
        "Formal source",
        properties.encode_to_vec(),
        &[attachment],
    )
    .unwrap();
    let mut raw = source.encode();
    raw.extend_from_slice(&[0xaa, 0x06, 0x02, 0x01, 0xff]);
    let source = CardRecord::decode(&raw).unwrap();
    host.store_local_mut()
        .create_local(&format!("seed-{id}"), &source)
        .unwrap();
    source
}
fn selected(origin: u32, id: &str) -> proto::AssetSelection {
    proto::AssetSelection {
        origin,
        asset_id: id.into(),
        aliases: vec!["attachment:alias.png".into(), "😀.png".into()],
    }
}
fn import_request(
    main: &proto::WriteRequest,
    generation: u64,
    operation: &str,
    bytes: &[u8],
) -> crate::editor_draft_staging::proto::ImportRequest {
    crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: main.card_id.clone(),
        draft_id: main.draft_id.clone(),
        operation_id: operation.into(),
        expected_generation: generation,
        name: "imported.gif".into(),
        kind: "gif".into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}
fn durable_import(
    host: &mut HostRuntime,
    request: &crate::editor_draft_staging::proto::ImportRequest,
    bytes: &[u8],
    timestamp: i64,
) -> crate::editor_draft_staging::DraftImportRecord {
    crate::editor_draft_staging::import_durable(
        host,
        request,
        &mut std::io::Cursor::new(bytes),
        || 1,
        timestamp,
        &mut "not_committed",
    )
    .unwrap()
}
fn import_owner(host: &HostRuntime, main: &proto::WriteRequest, operation: &str) -> String {
    let card = host
        .store_local()
        .card(&crate::editor_draft_staging::key(
            &main.card_id,
            &main.draft_id,
        ))
        .unwrap()
        .unwrap();
    let slot = crate::editor_draft_staging::proto::Slot::decode(card.body().as_slice()).unwrap();
    slot.entries
        .iter()
        .find(|e| {
            e.request
                .as_ref()
                .is_some_and(|r| r.operation_id == operation)
        })
        .unwrap()
        .owner
        .clone()
}

#[test]
fn source_asset_pins_survive_restart_and_previous_pin_adoption_preserves_exact_bytes() {
    let (_directory, path, mut host) = fixture();
    let bytes = b"source image bytes";
    let source = seed_asset(&mut host, "asset-source", bytes);
    let mut first = existing("source-pins-first", 0, &source);
    first.assets.push(selected(0, "source-asset"));
    let saved = save_with_effect_at(&mut host, &first, || 1, 20, &mut "not_committed").unwrap();
    assert_eq!(saved.slot.source_card, source.encode());
    assert_eq!(
        saved.slot.active_bytes,
        saved.slot.encoded_len() as u64 + bytes.len() as u64
    );
    assert_eq!(
        private_journal(&host, &first).attachments()[0].id,
        "draft-asset-0"
    );
    drop(host);
    let mut host = reopen(&path);
    let mut exported = Vec::new();
    export_asset_verified(
        &host,
        &first.card_id,
        &first.draft_id,
        1,
        "source-asset",
        &mut exported,
    )
    .unwrap();
    assert_eq!(exported, bytes);
    let mut second = first.clone();
    second.operation_id = "source-pins-second".into();
    second.expected_generation = 1;
    second.assets[0].origin = 3;
    second.values.as_mut().unwrap().description = Some(value("new raw text"));
    let current = save_with_effect_at(&mut host, &second, || 1, 21, &mut "not_committed").unwrap();
    assert_eq!(
        current.slot.assets[0].selection,
        Some(second.assets[0].clone())
    );
    assert_eq!(
        current.slot.assets[0].sha256,
        Sha256::digest(bytes).to_vec()
    );
    let historical =
        save_with_effect_at(&mut host, &first, || 1, 22, &mut "not_committed").unwrap();
    assert!(historical.repeated);
    assert_eq!(historical.current_generation, 2);
    assert_eq!(historical.slot, saved.slot);
    assert!(
        export_asset_verified(
            &host,
            &first.card_id,
            &first.draft_id,
            1,
            "source-asset",
            &mut Vec::new()
        )
        .is_err()
    );
    assert_eq!(
        host.store_local()
            .card(&first.card_id)
            .unwrap()
            .unwrap()
            .encode(),
        source.encode()
    );
}

#[test]
fn durable_selected_import_is_pinned_consumed_once_and_text_autosave_does_not_reopen_import() {
    let (_directory, path, mut host) = fixture();
    let initial = request("owner-first", 0);
    save_with_effect_at(&mut host, &initial, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"GIF89a imported payload";
    let imported = import_request(&initial, 1, "durable-selected", bytes);
    let ready = durable_import(&mut host, &imported, bytes, 101);
    let owner = import_owner(&host, &initial, &imported.operation_id);
    let mut first = request("pin-import-first", 1);
    first.assets.push(selected(2, &ready.asset_id));
    let pinned = save_with_effect_at(&mut host, &first, || 1, 102, &mut "not_committed").unwrap();
    assert_eq!(
        pinned.slot.consumed_imports,
        vec![imported.operation_id.clone()]
    );
    assert!(
        host.store_local()
            .retained_blob_local(&owner)
            .unwrap()
            .is_some()
    );
    drop(host);
    let mut host = reopen(&path);
    let mut next = first.clone();
    next.expected_generation = 2;
    next.operation_id = "pin-import-text".into();
    next.values.as_mut().unwrap().description = Some(value("keep same selected asset and aliases"));
    let saved = save_with_effect_at(&mut host, &next, || 1, 103, &mut "not_committed").unwrap();
    assert!(saved.slot.consumed_imports.is_empty());
    assert_eq!(
        saved.slot.assets[0].selection,
        Some(first.assets[0].clone())
    );
    assert!(
        host.store_local()
            .retained_blob_local(&owner)
            .unwrap()
            .is_none()
    );
    assert!(
        crate::editor_draft_staging::list(&host, &first.card_id, &first.draft_id)
            .unwrap()
            .is_empty()
    );
    let mut output = Vec::new();
    export_asset_verified(
        &host,
        &first.card_id,
        &first.draft_id,
        3,
        &ready.asset_id,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, bytes);
    let old = save_with_effect_at(&mut host, &first, || 1, 104, &mut "not_committed").unwrap();
    assert!(old.repeated);
    assert_eq!(old.current_generation, 3);
    assert_eq!(old.slot.consumed_imports, pinned.slot.consumed_imports);
    assert!(host.store_local().card(&first.card_id).unwrap().is_none());
}

#[test]
fn failed_consumed_cleanup_keeps_previous_generation_and_main_effect_not_committed_until_explicit_retry()
 {
    let (_directory, _path, mut host) = fixture();
    let owner = request("cleanup-fail-owner", 0);
    save_with_effect_at(&mut host, &owner, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"cleanup retry bytes";
    let imported = import_request(&owner, 1, "cleanup-fail-import", bytes);
    let ready = durable_import(&mut host, &imported, bytes, 101);
    let retained_owner = import_owner(&host, &owner, &imported.operation_id);
    let mut pinned = request("cleanup-fail-pinned", 1);
    pinned.assets.push(selected(2, &ready.asset_id));
    let previous =
        save_with_effect_at(&mut host, &pinned, || 1, 102, &mut "not_committed").unwrap();
    let mut next = pinned.clone();
    next.expected_generation = 2;
    next.operation_id = "cleanup-fail-next".into();
    next.values.as_mut().unwrap().description =
        Some(value("new input must survive a cleanup error"));
    let mut effect = "unknown";
    let error = save_with_effect_at(&mut host, &next, || 1, 50, &mut effect).unwrap_err();
    assert!(error.contains("clock regression"));
    assert_eq!(effect, "not_committed");
    assert_eq!(
        read(&host, &pinned.card_id, &pinned.draft_id)
            .unwrap()
            .unwrap()
            .slot,
        previous.slot
    );
    assert!(
        host.store_local()
            .retained_blob_local(&retained_owner)
            .unwrap()
            .is_some()
    );
    assert!(
        host.store_local()
            .operation_commit(&key(&pinned.card_id, &pinned.draft_id), &next.operation_id)
            .unwrap()
            .is_none()
    );
    let confirmed = save_with_effect_at(&mut host, &next, || 1, 103, &mut effect).unwrap();
    assert_eq!(effect, "committed");
    assert_eq!(confirmed.slot.generation, 3);
    assert_eq!(confirmed.slot.request.as_ref(), Some(&next));
    assert!(confirmed.slot.consumed_imports.is_empty());
    assert!(
        host.store_local()
            .retained_blob_local(&retained_owner)
            .unwrap()
            .is_none()
    );
}

#[test]
fn post_discard_cleanup_failure_preserves_proven_committed_outcome_and_exact_retry_finishes_cleanup()
 {
    let (_directory, path, mut host) = fixture();
    let owner = request("post-discard-owner", 0);
    save_with_effect_at(&mut host, &owner, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"unselected import";
    let imported = import_request(&owner, 1, "post-discard-import", bytes);
    durable_import(&mut host, &imported, bytes, 101);
    let retained_owner = import_owner(&host, &owner, &imported.operation_id);
    let mut effect = "not_committed";
    assert!(
        discard_with_effect_at(
            &mut host,
            &owner.card_id,
            &owner.draft_id,
            1,
            "post-discard-exact",
            || 1,
            50,
            &mut effect
        )
        .unwrap_err()
        .contains("clock regression")
    );
    assert_eq!(effect, "committed");
    let inactive = read(&host, &owner.card_id, &owner.draft_id)
        .unwrap()
        .unwrap();
    assert!(!inactive.slot.active);
    assert_eq!(inactive.slot.generation, 2);
    assert!(
        host.store_local()
            .retained_blob_local(&retained_owner)
            .unwrap()
            .is_some()
    );
    drop(host);
    let mut host = reopen(&path);
    let reconciled = discard_with_effect_at(
        &mut host,
        &owner.card_id,
        &owner.draft_id,
        1,
        "post-discard-exact",
        || 1,
        103,
        &mut effect,
    )
    .unwrap();
    assert!(reconciled.repeated);
    assert_eq!(effect, "committed");
    assert_eq!(reconciled.slot, inactive.slot);
    assert!(
        host.store_local()
            .retained_blob_local(&retained_owner)
            .unwrap()
            .is_none()
    );
    assert!(
        crate::editor_draft_staging::list(&host, &owner.card_id, &owner.draft_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn discard_releases_main_pins_and_all_pending_and_unselected_ready_import_owners() {
    let (_directory, path, mut host) = fixture();
    let initial = request("cleanup-owner", 0);
    save_with_effect_at(&mut host, &initial, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"kept only until discard";
    let a = import_request(&initial, 1, "selected-before-discard", bytes);
    let ready = durable_import(&mut host, &a, bytes, 101);
    let owner_a = import_owner(&host, &initial, &a.operation_id);
    let b = import_request(&initial, 1, "unselected-before-discard", bytes);
    durable_import(&mut host, &b, bytes, 102);
    let owner_b = import_owner(&host, &initial, &b.operation_id);
    let c = import_request(&initial, 1, "pending-before-discard", bytes);
    crate::editor_draft_staging::begin(&mut host, &c, || 1, 103, &mut "not_committed").unwrap();
    let mut pinned = request("cleanup-pin", 1);
    pinned.assets.push(selected(2, &ready.asset_id));
    save_with_effect_at(&mut host, &pinned, || 1, 104, &mut "not_committed").unwrap();
    let record = discard_with_effect_at(
        &mut host,
        &pinned.card_id,
        &pinned.draft_id,
        2,
        "cleanup-discard",
        || 1,
        105,
        &mut "not_committed",
    )
    .unwrap();
    assert!(!record.slot.active);
    assert_eq!(record.slot.active_bytes, 0);
    assert!(private_journal(&host, &pinned).attachments().is_empty());
    for owner in [&owner_a, &owner_b] {
        assert!(
            host.store_local()
                .retained_blob_local(owner)
                .unwrap()
                .is_none()
        );
    }
    assert!(
        crate::editor_draft_staging::list(&host, &pinned.card_id, &pinned.draft_id)
            .unwrap()
            .is_empty()
    );
    drop(host);
    let mut host = reopen(&path);
    assert!(
        discard_with_effect_at(
            &mut host,
            &pinned.card_id,
            &pinned.draft_id,
            2,
            "cleanup-discard",
            || 1,
            106,
            &mut "not_committed"
        )
        .unwrap()
        .repeated
    );
    assert!(
        export_asset_verified(
            &host,
            &pinned.card_id,
            &pinned.draft_id,
            3,
            &ready.asset_id,
            &mut Vec::new()
        )
        .is_err()
    );
}

#[test]
fn publication_requires_current_exact_generation_operation_and_complete_source_cas() {
    let (_directory, _path, mut host) = fixture();
    let bytes = b"proof checked bytes";
    let source = seed_asset(&mut host, "publication-source", bytes);
    let mut saved = existing("publication-draft", 0, &source);
    saved.assets.push(selected(0, "source-asset"));
    let record = save_with_effect_at(&mut host, &saved, || 1, 20, &mut "not_committed").unwrap();
    let publication = publish_assets(
        &host,
        &saved.card_id,
        &saved.draft_id,
        1,
        &saved.operation_id,
        &source.encode(),
    )
    .unwrap();
    assert_eq!(publication.assets[0].id, "source-asset");
    assert_eq!(publication.assets[0].kind, "image");
    assert_eq!(publication.attachments, source.attachments());
    assert_eq!(publication.values, saved.values.clone().unwrap());
    assert_eq!(record.slot.source_card, source.encode());
    assert!(
        publish_assets(
            &host,
            &saved.card_id,
            &saved.draft_id,
            0,
            &saved.operation_id,
            &source.encode()
        )
        .is_err()
    );
    assert!(
        publish_assets(
            &host,
            &saved.card_id,
            &saved.draft_id,
            1,
            "different-operation",
            &source.encode()
        )
        .is_err()
    );
    let lossy = CardRecord::new_with_attachments(
        &saved.card_id,
        "idea",
        2,
        "Formal source",
        source.body(),
        &source.attachments(),
    )
    .unwrap();
    assert_ne!(lossy.encode(), source.encode());
    assert!(
        publish_assets(
            &host,
            &saved.card_id,
            &saved.draft_id,
            1,
            &saved.operation_id,
            &lossy.encode()
        )
        .is_err()
    );
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::EditContent,
        &saved.card_id,
        100,
        1,
    )
    .unwrap();
    host.edit_versioned_content(
        &connection,
        &VersionedContentChange {
            operation_id: "foreign-body-revision".into(),
            source_card: source.encode(),
            title: "Revised title".into(),
            body: source.body(),
            preview_text: String::new(),
            attachments: Some(source.attachments()),
        },
        || 1,
    )
    .unwrap();
    host.disconnect(&connection).unwrap();
    // Pins remain publishable for exact original business retries. A new
    // business operation must still pass the core's complete source CAS.
    let publication = publish_assets(
        &host,
        &saved.card_id,
        &saved.draft_id,
        1,
        &saved.operation_id,
        &source.encode(),
    )
    .unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::EditContent,
        &saved.card_id,
        100,
        1,
    )
    .unwrap();
    let attempted = VersionedContentChange {
        operation_id: "new-operation-with-stale-source".into(),
        source_card: source.encode(),
        title: "Attempted edit".into(),
        body: source.body(),
        preview_text: String::new(),
        attachments: Some(publication.attachments),
    };
    assert_eq!(
        host.edit_versioned_content(&connection, &attempted, || 1)
            .unwrap_err(),
        morrow_core::Error::RevisionConflict
    );
    host.disconnect(&connection).unwrap();
    let mut output = Vec::new();
    export_asset_verified(
        &host,
        &saved.card_id,
        &saved.draft_id,
        1,
        "source-asset",
        &mut output,
    )
    .unwrap();
    assert_eq!(output, bytes); // source conflict does not destroy raw journal ownership.
}

#[test]
fn pinned_publication_allows_same_business_operation_retries_and_core_rejects_changed_payload() {
    let (_directory, _path, mut host) = fixture();
    let mut owner = request("business-retry-owner", 0);
    owner.values.as_mut().unwrap().title = Some(value("Published card"));
    save_with_effect_at(&mut host, &owner, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"GIF89a durable business attachment";
    let imported = import_request(&owner, 1, "business-retry-import", bytes);
    let ready = durable_import(&mut host, &imported, bytes, 101);
    let mut pinned = owner.clone();
    pinned.expected_generation = 1;
    pinned.operation_id = "business-retry-pinned".into();
    pinned.assets.push(selected(2, &ready.asset_id));
    save_with_effect_at(&mut host, &pinned, || 1, 102, &mut "not_committed").unwrap();
    let publication = publish_assets(
        &host,
        &pinned.card_id,
        &pinned.draft_id,
        2,
        &pinned.operation_id,
        &[],
    )
    .unwrap();
    assert_eq!(publication.assets[0].kind, "gif");
    let mut properties = tasks_v2::Properties {
        version: 2,
        category: publication.values.category,
        stage: publication.values.stage,
        description: publication.values.description.unwrap().text,
        ..Default::default()
    };
    for selected in &publication.assets {
        properties.assets.push(Default::default());
        let asset = properties.assets.last_mut().unwrap();
        asset.id = selected.id.clone();
        asset.name = selected.name.clone();
        asset.kind = selected.kind.clone();
        asset.bytes = selected.bytes;
    }
    let business = CardRecord::new_with_attachments(
        &pinned.card_id,
        "idea",
        2,
        "Published card",
        properties.encode_to_vec(),
        &publication.attachments,
    )
    .unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(
        &mut connection,
        GrantKind::CreateContent,
        &pinned.card_id,
        100,
        1,
    )
    .unwrap();
    let first = host
        .create_content(&connection, "published-business-op", &business, || 1)
        .unwrap();
    // Revalidation after create must not reject the already existing live card.
    let later = publish_assets(
        &host,
        &pinned.card_id,
        &pinned.draft_id,
        2,
        &pinned.operation_id,
        &[],
    )
    .unwrap();
    assert_eq!(later.attachments, publication.attachments);
    assert_eq!(
        host.create_content(&connection, "published-business-op", &business, || 1)
            .unwrap(),
        first
    );
    let changed = CardRecord::new_with_attachments(
        &pinned.card_id,
        "idea",
        2,
        "Changed retry",
        business.body(),
        &later.attachments,
    )
    .unwrap();
    assert!(
        host.create_content(&connection, "published-business-op", &changed, || 1)
            .is_err()
    );
    host.disconnect(&connection).unwrap();
}

#[test]
fn import_identity_cannot_cross_drafts_or_select_a_pending_unverified_payload() {
    let (_directory, _path, mut host) = fixture();
    let owner = request("foreign-owner", 0);
    save_with_effect_at(&mut host, &owner, || 1, 100, &mut "not_committed").unwrap();
    let bytes = b"owned payload";
    let a = import_request(&owner, 1, "owned-import", bytes);
    let imported = durable_import(&mut host, &a, bytes, 101);
    let mut foreign = request("foreign-slot", 0);
    foreign.draft_id = "other-draft".into();
    foreign.assets.push(selected(2, &imported.asset_id));
    assert!(save_with_effect_at(&mut host, &foreign, || 1, 102, &mut "not_committed").is_err());
    assert!(
        read(&host, &foreign.card_id, &foreign.draft_id)
            .unwrap()
            .is_none()
    );
    let pending = import_request(&owner, 1, "pending-import", bytes);
    let record =
        crate::editor_draft_staging::begin(&mut host, &pending, || 1, 103, &mut "not_committed")
            .unwrap();
    let mut attempted = request("pending-selection", 1);
    attempted.assets.push(selected(2, &record.asset_id));
    assert!(save_with_effect_at(&mut host, &attempted, || 1, 104, &mut "not_committed").is_err());
    assert_eq!(
        read(&host, &owner.card_id, &owner.draft_id)
            .unwrap()
            .unwrap()
            .slot
            .generation,
        1
    );
    let mut orphan = request("orphan-prior", 1);
    orphan.assets.push(selected(3, "absent-pin"));
    assert!(save_with_effect_at(&mut host, &orphan, || 1, 105, &mut "not_committed").is_err());
}

#[test]
fn stored_pin_metadata_and_blob_charge_are_not_optional_and_stream_failure_is_not_success() {
    let (_directory, _path, mut host) = fixture();
    let source = seed_asset(&mut host, "pin-check-source", b"valid bytes");
    let mut first = existing("pin-check-save", 0, &source);
    first.assets.push(selected(0, "source-asset"));
    let saved = save_with_effect_at(&mut host, &first, || 1, 20, &mut "not_committed").unwrap();
    let mut malformed = saved.slot.clone();
    malformed.assets[0].selection.as_mut().unwrap().asset_id = "different-id".into();
    assert!(decode_body(&malformed.encode_to_vec()).is_err());
    let mut malformed = saved.slot.clone();
    malformed.assets[0].pin_id = "injected-pin".into();
    assert!(decode_body(&malformed.encode_to_vec()).is_err());
    let mut malformed = saved.slot.clone();
    malformed.active_bytes -= 1;
    assert!(decode_body(&malformed.encode_to_vec()).is_err());
    let mut malformed = saved.slot.clone();
    malformed.assets[0].byte_length = MAX_ACTIVE_BYTES;
    assert!(charge(&malformed).is_err());
    let mut malformed = saved.slot.clone();
    malformed.assets[0].byte_length = u64::MAX;
    assert!(charge(&malformed).is_err());
    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _bytes: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("output write failed"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(
        export_asset_verified(
            &host,
            &first.card_id,
            &first.draft_id,
            1,
            "source-asset",
            &mut FailingWriter
        )
        .is_err()
    );
    assert!(
        export_asset_verified(
            &host,
            &first.card_id,
            &first.draft_id,
            1,
            "absent",
            &mut Vec::new()
        )
        .is_err()
    );
}

#[test]
fn draft_bridge_keeps_original_asset_selection_and_u64_metadata_without_accepting_client_pins() {
    let (_directory, _path, mut host) = fixture();
    let source = seed_asset(&mut host, "bridge-source", b"bridge bytes");
    let mut first = existing("bridge-assets-save", 0, &source);
    first.assets.push(selected(0, "source-asset"));
    let record = save_with_effect_at(&mut host, &first, || 1, 20, &mut "not_committed").unwrap();
    let reply =
        serde_json::to_value(crate::draft_bridge::View::from_record(record).unwrap()).unwrap();
    assert_eq!(reply["assets"][0]["selection"]["aliases"][1], "😀.png");
    assert_eq!(reply["assets"][0]["byte_length"], "12");
    assert_eq!(
        reply["assets"][0]["sha256"],
        crate::hex(&Sha256::digest(b"bridge bytes"))
    );
    assert_eq!(reply["values"]["assets"][0]["asset_id"], "source-asset");
    let mut wire = reply["values"].clone();
    let selected = wire.as_object_mut().unwrap().remove("assets").unwrap();
    let mut draft = serde_json::json!({"card_id": first.card_id, "draft_id": first.draft_id, "source_kind":0,
        "source_revision":"1", "expected_generation":"0", "operation_id":first.operation_id,
        "values":wire, "assets":selected });
    assert_eq!(
        serde_json::from_value::<crate::draft_bridge::Write>(draft.clone())
            .unwrap()
            .request()
            .unwrap(),
        first
    );
    draft["assets"][0]["pin_id"] = "client-pin".into();
    assert!(serde_json::from_value::<crate::draft_bridge::Write>(draft).is_err());
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
