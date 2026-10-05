use super::*;
use morrow_core::store::{EventBudget, Store};
use morrow_editor_draft_model::proto as main_proto;
use std::io::{Cursor, Read};

const CARD: &str = "staging-target";
const DRAFT: &str = "staging-draft";
const BYTES: &[u8] = b"durable selected attachment bytes";

fn reopen(path: &std::path::Path, max_count: u32) -> HostRuntime {
    HostRuntime::new(
        Store::open(
            path,
            EventBudget {
                max_count,
                ..EventBudget::default()
            },
        )
        .unwrap(),
    )
    .unwrap()
}
fn value() -> main_proto::TextValue {
    main_proto::TextValue {
        selection_base: -1,
        selection_extent: -1,
        composing_start: -1,
        composing_end: -1,
        ..Default::default()
    }
}
fn draft(operation: &str, generation: u64) -> main_proto::WriteRequest {
    main_proto::WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: generation,
        source_kind: 1,
        values: Some(main_proto::Values {
            title: Some(value()),
            description: Some(value()),
            hypothesis: Some(value()),
            conclusion: Some(value()),
            todos: Some(value()),
            category: "灵感".into(),
            stage: "待整理".into(),
        }),
        ..Default::default()
    }
}
fn fixture() -> (tempfile::TempDir, std::path::PathBuf, HostRuntime) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let mut host = reopen(&path, 1024);
    crate::editor_draft::save_with_effect_at(
        &mut host,
        &draft("draft-create", 0),
        || 1,
        1,
        &mut "not_committed",
    )
    .unwrap();
    (directory, path, host)
}
fn request(operation: &str, generation: u64, bytes: &[u8]) -> proto::ImportRequest {
    proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: DRAFT.into(),
        operation_id: operation.into(),
        expected_generation: generation,
        name: "chosen.bin".into(),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}
fn start(host: &mut HostRuntime, request: &proto::ImportRequest) -> DraftImportRecord {
    begin(host, request, || 1, 1, &mut "not_committed").unwrap()
}
fn ready(
    host: &mut HostRuntime,
    request: &proto::ImportRequest,
    bytes: &[u8],
) -> DraftImportRecord {
    import_durable(
        host,
        request,
        &mut Cursor::new(bytes),
        || 1,
        1,
        &mut "not_committed",
    )
    .unwrap()
}
fn current(host: &HostRuntime) -> proto::Slot {
    Reader { host }.staging_slot(CARD, DRAFT).unwrap().unwrap()
}
struct Unreadable;
impl Read for Unreadable {
    fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("history or rejected admission must not read the source")
    }
}
fn corrupt_title(host: &mut HostRuntime) {
    let journal = host.store_local().card(&key(CARD, DRAFT)).unwrap().unwrap();
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
            operation_id: format!("corrupt-{}", summary.revision),
            card_id: summary.id,
            expected_revision: summary.revision,
            title: "corrupt private current".into(),
            body: journal.body(),
            preview_text: String::new(),
            attachments: Some(vec![]),
        },
        || 1,
    )
    .unwrap();
    host.disconnect(&connection).unwrap();
}

#[test]
fn pending_restart_ready_and_exact_retry_use_verified_durable_bytes() {
    let (_directory, path, mut host) = fixture();
    let r = request("restart-import", 1, BYTES);
    let pending = start(&mut host, &r);
    assert_eq!(pending.phase, DraftImportPhase::Pending);
    assert!(!pending.current_active && !pending.bytes_retained);
    drop(host);
    let mut host = reopen(&path, 1024);
    let saved = ready(&mut host, &r, BYTES);
    assert_eq!(saved.phase, DraftImportPhase::Ready);
    assert!(saved.current_active && saved.bytes_retained);
    assert_eq!(saved.staging_revision, 2);
    drop(host);
    let mut host = reopen(&path, 1024);
    let mut effect = "unknown";
    let old = import_durable(
        &mut host,
        &r,
        &mut Unreadable,
        || panic!("retry clock"),
        0,
        &mut effect,
    )
    .unwrap();
    assert!(old.repeated && old.current_active);
    assert_eq!(effect, "committed");
    let mut output = vec![];
    let exported = export_verified(&host, CARD, DRAFT, 1, &r.operation_id, &mut output).unwrap();
    assert_eq!(output, BYTES);
    assert_eq!(exported.sha256.as_slice(), r.sha256);
    let (pin, operation) = resolve_durable_draft_import(&host, CARD, DRAFT, &saved.asset_id, 1)
        .unwrap()
        .unwrap();
    assert_eq!(pin.byte_length, r.byte_length);
    assert_eq!(operation, r.operation_id);
    assert!(resolve_durable_draft_import(&host, CARD, DRAFT, &saved.asset_id, 2).is_err());
    assert!(host.store_local().card(CARD).unwrap().is_none());
}

#[test]
fn invalid_streams_roll_back_bytes_but_preserve_committed_pending_intent() {
    for (index, source) in [
        b"short".as_slice(),
        b"durable selected attachment bytez".as_slice(),
        b"durable selected attachment bytes extra".as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let (_directory, _path, mut host) = fixture();
        let r = request(&format!("bad-stream-{index}"), 1, BYTES);
        let mut effect = "unknown";
        assert!(
            import_durable(
                &mut host,
                &r,
                &mut Cursor::new(source),
                || 1,
                1,
                &mut effect
            )
            .is_err()
        );
        assert_eq!(
            effect, "committed",
            "the Pending intent is durable even though its byte transaction rolls back"
        );
        let pending = inspect(&host, CARD, DRAFT, &r.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(pending.phase, DraftImportPhase::Pending);
        assert!(!pending.bytes_retained);
        assert!(
            host.store_local()
                .list_blobs_local("", 16)
                .unwrap()
                .is_empty()
        );
        assert!(
            host.store_local()
                .retained_blob_local(&owner(&r))
                .unwrap()
                .is_none()
        );
        assert_eq!(ready(&mut host, &r, BYTES).phase, DraftImportPhase::Ready);
    }
}

#[test]
fn changed_retry_payload_foreign_operation_and_stale_admission_never_read_source() {
    let (_directory, _path, mut host) = fixture();
    let r = request("bound-operation", 1, BYTES);
    start(&mut host, &r);
    let before = current(&host);
    let mut changed = r.clone();
    changed.name = "different.bin".into();
    let mut effect = "unknown";
    assert!(import_durable(&mut host, &changed, &mut Unreadable, || 1, 1, &mut effect).is_err());
    assert_eq!(effect, "not_committed");
    let mut foreign = r.clone();
    foreign.draft_id = "other-draft".into();
    assert!(begin(&mut host, &foreign, || 1, 1, &mut effect).is_err());
    let stale = request("stale-operation", 2, BYTES);
    assert!(import_durable(&mut host, &stale, &mut Unreadable, || 1, 1, &mut effect).is_err());
    assert_eq!(current(&host), before);
    assert!(matches!(
        host.store_local().lookup("stale-operation").unwrap(),
        Lookup::Absent
    ));
}

#[test]
fn pending_per_draft_and_byte_budgets_reject_before_reading() {
    let (_directory, _path, mut host) = fixture();
    for index in 0..20 {
        start(&mut host, &request(&format!("capacity-{index}"), 1, BYTES));
    }
    let overflow = request("capacity-overflow", 1, BYTES);
    let mut effect = "unknown";
    assert!(import_durable(&mut host, &overflow, &mut Unreadable, || 1, 1, &mut effect).is_err());
    assert_eq!(effect, "not_committed");
    assert_eq!(list(&host, CARD, DRAFT).unwrap().len(), 20);
    assert!(
        inspect(&host, CARD, DRAFT, &overflow.operation_id)
            .unwrap()
            .is_none()
    );
    let (_directory2, _path2, mut host2) = fixture();
    let mut full = request("all-byte-budget", 1, BYTES);
    full.byte_length = MAX_BYTES;
    start(&mut host2, &full);
    assert!(
        import_durable(
            &mut host2,
            &request("one-byte-more", 1, b"x"),
            &mut Unreadable,
            || 1,
            1,
            &mut effect
        )
        .is_err()
    );
    assert_eq!(current(&host2).entries.len(), 1);
    assert!(
        host2
            .store_local()
            .list_blobs_local("", 16)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ready_then_abandon_prune_restart_preserves_terminal_receipts_without_reacquiring_owner() {
    let (_directory, path, mut host) = fixture();
    let r = request("terminal-import", 1, BYTES);
    ready(&mut host, &r, BYTES);
    let mut effect = "unknown";
    let retired = abandon(
        &mut host,
        CARD,
        DRAFT,
        1,
        &r.operation_id,
        "terminal-abandon",
        || 1,
        2,
        &mut effect,
    )
    .unwrap();
    assert_eq!(effect, "committed");
    assert!(retired.bytes_retained);
    reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect).unwrap();
    assert!(list(&host, CARD, DRAFT).unwrap().is_empty());
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_none()
    );
    drop(host);
    let mut host = reopen(&path, 1024);
    let old = import_durable(
        &mut host,
        &r,
        &mut Unreadable,
        || panic!("terminal retry clock"),
        0,
        &mut effect,
    )
    .unwrap();
    assert!(old.repeated);
    assert_eq!(old.phase, DraftImportPhase::Retired);
    assert!(!old.bytes_retained);
    let discarded = abandon(
        &mut host,
        CARD,
        DRAFT,
        1,
        &r.operation_id,
        "terminal-abandon",
        || panic!("abandon retry clock"),
        0,
        &mut effect,
    )
    .unwrap();
    assert!(discarded.repeated);
    assert_eq!(effect, "committed");
    assert!(export_verified(&host, CARD, DRAFT, 1, &r.operation_id, &mut vec![]).is_err());
}

#[test]
fn same_content_owners_are_independent_and_unselected_bytes_collect_only_after_last_release() {
    let (_directory, _path, mut host) = fixture();
    let mut second_draft = draft("draft-two-create", 0);
    second_draft.draft_id = "staging-draft-two".into();
    crate::editor_draft::save_with_effect_at(
        &mut host,
        &second_draft,
        || 1,
        1,
        &mut "not_committed",
    )
    .unwrap();
    let a = request("owner-a", 1, BYTES);
    let mut b = request("owner-b", 1, BYTES);
    b.draft_id = second_draft.draft_id.clone();
    ready(&mut host, &a, BYTES);
    ready(&mut host, &b, BYTES);
    let blob_a = host
        .store_local()
        .retained_blob_local(&owner(&a))
        .unwrap()
        .unwrap();
    let blob_b = host
        .store_local()
        .retained_blob_local(&owner(&b))
        .unwrap()
        .unwrap();
    assert_eq!(blob_a.id, blob_b.id);
    let mut effect = "unknown";
    abandon(
        &mut host,
        CARD,
        DRAFT,
        1,
        &a.operation_id,
        "abandon-a",
        || 1,
        2,
        &mut effect,
    )
    .unwrap();
    reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect).unwrap();
    assert!(
        host.store_local()
            .blob_info_local(&blob_a.id)
            .unwrap()
            .retired_at_unix_ms
            .is_none()
    );
    assert!(
        host.store_local_mut()
            .collect_retired_local(60_002, 60_000)
            .unwrap()
            .is_empty()
    );
    abandon(
        &mut host,
        CARD,
        &b.draft_id,
        1,
        &b.operation_id,
        "abandon-b",
        || 1,
        60_003,
        &mut effect,
    )
    .unwrap();
    reconcile(&mut host, CARD, &b.draft_id, || 1, 60_003, &mut effect).unwrap();
    assert!(
        host.store_local_mut()
            .collect_retired_local(120_002, 60_000)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        host.store_local_mut()
            .collect_retired_local(120_003, 60_000)
            .unwrap(),
        [blob_a.id]
    );
}

#[test]
fn retained_bytes_survive_failed_ready_and_finish_after_newer_main_generation_without_source() {
    let (_directory, path, host) = fixture();
    drop(host);
    let mut host = reopen(&path, 2);
    let r = request("ready-failure", 1, BYTES);
    let mut effect = "unknown";
    assert_eq!(
        import_durable(&mut host, &r, &mut Cursor::new(BYTES), || 1, 1, &mut effect)
            .unwrap_err()
            .to_string(),
        "EventCapacity"
    );
    assert_eq!(effect, "committed");
    let pending = inspect(&host, CARD, DRAFT, &r.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(pending.phase, DraftImportPhase::Pending);
    assert!(pending.bytes_retained);
    drop(host);
    let mut host = reopen(&path, 1024);
    crate::editor_draft::save_with_effect_at(
        &mut host,
        &draft("newer-main-generation", 1),
        || 1,
        2,
        &mut "not_committed",
    )
    .unwrap();
    let restored = import_durable(&mut host, &r, &mut Unreadable, || 1, 0, &mut effect).unwrap();
    assert_eq!(restored.phase, DraftImportPhase::Ready);
    assert!(restored.current_active && restored.repeated);
    assert!(
        resolve_durable_draft_import(&host, CARD, DRAFT, &restored.asset_id, 2)
            .unwrap()
            .is_some()
    );
}

#[test]
fn release_and_prune_failures_resume_without_recreating_snapshot_owner() {
    let (_directory, path, mut host) = fixture();
    let r = request("cleanup-failure", 1, BYTES);
    ready(&mut host, &r, BYTES);
    abandon(
        &mut host,
        CARD,
        DRAFT,
        1,
        &r.operation_id,
        "cleanup-abandon",
        || 1,
        1,
        &mut "not_committed",
    )
    .unwrap();
    drop(host);
    let mut host = reopen(&path, 4);
    let mut effect = "not_committed";
    assert!(reconcile(&mut host, CARD, DRAFT, || 1, 0, &mut effect).is_err());
    assert_eq!(
        effect, "not_committed",
        "Core Invalid(clock) rejects this release before mutation"
    );
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_some()
    );
    assert_eq!(
        reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect)
            .unwrap_err()
            .to_string(),
        "EventCapacity"
    );
    assert_eq!(
        effect, "committed",
        "the independent owner release committed before prune failed"
    );
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_none()
    );
    assert_eq!(current(&host).entries.len(), 1);
    drop(host);
    let mut host = reopen(&path, 1024);
    reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect).unwrap();
    assert_eq!(effect, "committed");
    assert!(current(&host).entries.is_empty());
    assert_eq!(
        import_durable(&mut host, &r, &mut Unreadable, || 1, 0, &mut effect)
            .unwrap()
            .phase,
        DraftImportPhase::Retired
    );
}

#[test]
fn failed_pending_transaction_is_unknown_and_exact_original_can_retry_after_restart() {
    let (_directory, path, host) = fixture();
    drop(host);
    let mut host = reopen(&path, 1);
    let r = request("pending-budget-failure", 1, BYTES);
    let mut effect = "not_committed";
    assert_eq!(
        import_durable(&mut host, &r, &mut Unreadable, || 1, 1, &mut effect)
            .unwrap_err()
            .to_string(),
        "EventCapacity"
    );
    assert_eq!(effect, "unknown");
    assert!(
        inspect(&host, CARD, DRAFT, &r.operation_id)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        host.store_local().lookup(&r.operation_id).unwrap(),
        Lookup::Absent
    ));
    drop(host);
    let mut host = reopen(&path, 1024);
    assert_eq!(ready(&mut host, &r, BYTES).phase, DraftImportPhase::Ready);
}

#[test]
fn original_pending_and_abandon_receipts_stay_committed_when_current_readback_is_corrupt() {
    for abandoned in [false, true] {
        let (_directory, _path, mut host) = fixture();
        let r = request("history-before-corrupt", 1, BYTES);
        start(&mut host, &r);
        if abandoned {
            abandon(
                &mut host,
                CARD,
                DRAFT,
                1,
                &r.operation_id,
                "history-abandon",
                || 1,
                1,
                &mut "not_committed",
            )
            .unwrap();
        }
        corrupt_title(&mut host);
        let mut effect = "unknown";
        if abandoned {
            assert!(
                abandon(
                    &mut host,
                    CARD,
                    DRAFT,
                    1,
                    &r.operation_id,
                    "history-abandon",
                    || panic!("read-only receipt"),
                    0,
                    &mut effect
                )
                .is_err()
            );
        } else {
            assert!(
                begin(
                    &mut host,
                    &r,
                    || panic!("read-only receipt"),
                    0,
                    &mut effect
                )
                .is_err()
            );
        }
        assert_eq!(effect, "committed");
        assert!(list(&host, CARD, DRAFT).is_err());
    }
}

#[test]
fn original_revision_reservation_keeps_final_ready_retire_prune_at_256() {
    let (_directory, _path, mut host) = fixture();
    let mut effect = "unknown";
    for index in 0..84 {
        let r = request(&format!("revision-cycle-{index}"), 1, BYTES);
        assert_eq!(start(&mut host, &r).staging_revision, 3 * index + 1);
        abandon(
            &mut host,
            CARD,
            DRAFT,
            1,
            &r.operation_id,
            &format!("revision-abandon-{index}"),
            || 1,
            1,
            &mut effect,
        )
        .unwrap();
        reconcile(&mut host, CARD, DRAFT, || 1, 1, &mut effect).unwrap();
    }
    let r = request("revision-final", 1, BYTES);
    assert_eq!(start(&mut host, &r).staging_revision, 253);
    assert_eq!(ready(&mut host, &r, BYTES).staging_revision, 254);
    assert_eq!(
        abandon(
            &mut host,
            CARD,
            DRAFT,
            1,
            &r.operation_id,
            "revision-final-abandon",
            || 1,
            2,
            &mut effect
        )
        .unwrap()
        .staging_revision,
        255
    );
    reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect).unwrap();
    assert_eq!(current(&host).revision, 256);
    let overflow = request("revision-overflow", 1, BYTES);
    assert!(import_durable(&mut host, &overflow, &mut Unreadable, || 1, 2, &mut effect).is_err());
    assert!(
        inspect(&host, CARD, DRAFT, &overflow.operation_id)
            .unwrap()
            .is_none()
    );
    let old = import_durable(
        &mut host,
        &r,
        &mut Unreadable,
        || panic!("terminal history clock"),
        0,
        &mut effect,
    )
    .unwrap();
    assert!(old.repeated);
    assert_eq!(old.staging_revision, 256);
    assert!(!old.bytes_retained);
}

#[test]
fn selected_import_cleanup_keeps_draft_pin_across_text_save_restart_and_discard() {
    let (_directory, path, mut host) = fixture();
    let r = request("selected-import", 1, BYTES);
    let saved = ready(&mut host, &r, BYTES);
    let blob = host
        .store_local()
        .retained_blob_local(&owner(&r))
        .unwrap()
        .unwrap();
    let mut selected = draft("select-durable-import", 1);
    selected.assets.push(main_proto::AssetSelection {
        origin: 2,
        asset_id: saved.asset_id.clone(),
        aliases: vec![],
    });
    let mut effect = "unknown";
    let main = crate::editor_draft::save_with_effect_at(&mut host, &selected, || 1, 2, &mut effect)
        .unwrap();
    assert_eq!(main.slot.consumed_imports, [r.operation_id.clone()]);
    reconcile(&mut host, CARD, DRAFT, || 1, 2, &mut effect).unwrap();
    assert!(list(&host, CARD, DRAFT).unwrap().is_empty());
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_none()
    );
    assert!(
        host.store_local()
            .blob_info_local(&blob.id)
            .unwrap()
            .retired_at_unix_ms
            .is_none()
    );
    let mut next = selected.clone();
    next.operation_id = "selected-text-autosave".into();
    next.expected_generation = 2;
    next.values
        .as_mut()
        .unwrap()
        .description
        .as_mut()
        .unwrap()
        .text = "typed after import".into();
    let updated =
        crate::editor_draft::save_with_effect_at(&mut host, &next, || 1, 3, &mut effect).unwrap();
    assert!(updated.slot.consumed_imports.is_empty());
    drop(host);
    let mut host = reopen(&path, 1024);
    let mut output = vec![];
    crate::editor_draft::export_asset_verified(&host, CARD, DRAFT, 3, &saved.asset_id, &mut output)
        .unwrap();
    assert_eq!(output, BYTES);
    assert_eq!(
        import_durable(&mut host, &r, &mut Unreadable, || 1, 0, &mut effect)
            .unwrap()
            .phase,
        DraftImportPhase::Retired
    );
    crate::editor_draft::discard_with_effect_at(
        &mut host,
        CARD,
        DRAFT,
        3,
        "discard-selected",
        || 1,
        4,
        &mut effect,
    )
    .unwrap();
    assert!(
        host.store_local_mut()
            .collect_retired_local(60_004, 60_000)
            .unwrap()
            .is_empty(),
        "immutable selected draft event still retains bytes"
    );
    assert!(host.store_local().card(CARD).unwrap().is_none());
}

#[test]
fn inactive_main_cleans_every_unselected_pending_and_ready_import() {
    let (_directory, _path, mut host) = fixture();
    let a = request("inactive-pending", 1, BYTES);
    let b = request("inactive-ready", 1, BYTES);
    start(&mut host, &a);
    ready(&mut host, &b, BYTES);
    let mut effect = "unknown";
    crate::editor_draft::discard_with_effect_at(
        &mut host,
        CARD,
        DRAFT,
        1,
        "discard-all-imports",
        || 1,
        2,
        &mut effect,
    )
    .unwrap();
    assert_eq!(effect, "committed");
    assert!(list(&host, CARD, DRAFT).unwrap().is_empty());
    for r in [a, b] {
        let record = inspect(&host, CARD, DRAFT, &r.operation_id)
            .unwrap()
            .unwrap();
        assert_eq!(record.phase, DraftImportPhase::Retired);
        assert!(!record.bytes_retained && !record.current_active);
        assert_eq!(
            import_durable(&mut host, &r, &mut Unreadable, || 1, 0, &mut effect)
                .unwrap()
                .phase,
            DraftImportPhase::Retired
        );
    }
}

#[test]
fn journal_filtering_is_private_fail_closed_and_schema_is_canonical() {
    let (_directory, _path, mut host) = fixture();
    start(&mut host, &request("private-staging-import", 1, BYTES));
    let journal = host.store_local().card(&key(CARD, DRAFT)).unwrap().unwrap();
    assert!(is_journal(&journal));
    validate_journal(&journal).unwrap();
    let wrong_type =
        CardRecord::new(&journal.summary().id, "idea", 1, TITLE, journal.body()).unwrap();
    assert!(is_journal(&wrong_type));
    assert!(validate_journal(&wrong_type).is_err());
    let wrong_id = CardRecord::new("unreserved-id", TYPE, 1, TITLE, journal.body()).unwrap();
    assert!(is_journal(&wrong_id));
    assert!(validate_journal(&wrong_id).is_err());
    let mut unknown = journal.body();
    unknown.extend_from_slice(&[0xa0, 0x06, 0x01]);
    assert!(decode_body(&unknown).is_err());
    assert!(validate_request(&proto::ImportRequest::default()).is_err());
    assert!(DraftImportPhase::parse(3).is_err());
}

#[test]
fn inactive_discard_reports_committed_when_missing_ready_owner_blocks_cleanup_until_repaired() {
    let (_directory, path, mut host) = fixture();
    let r = request("missing-ready-owner", 1, BYTES);
    ready(&mut host, &r, BYTES);
    let blob = host
        .store_local()
        .retained_blob_local(&owner(&r))
        .unwrap()
        .unwrap();
    host.store_local_mut()
        .release_retention_local(&blob.id, &owner(&r), RetentionKind::Snapshot, 2)
        .unwrap();
    let mut effect = "unknown";
    let failed = crate::editor_draft::discard_with_effect_at(
        &mut host,
        CARD,
        DRAFT,
        1,
        "discard-missing-owner",
        || 1,
        3,
        &mut effect,
    );
    assert!(failed.is_err());
    assert_eq!(
        effect, "committed",
        "inactive main journal committed before staging integrity failure"
    );
    assert!(
        !crate::editor_draft::read(&host, CARD, DRAFT)
            .unwrap()
            .unwrap()
            .slot
            .active
    );
    assert_eq!(current(&host).entries.len(), 1);
    assert_eq!(
        current(&host).entries[0].phase,
        DraftImportPhase::Ready.raw()
    );
    drop(host);
    let mut host = reopen(&path, 1024);
    assert!(
        crate::editor_draft::discard_with_effect_at(
            &mut host,
            CARD,
            DRAFT,
            1,
            "discard-missing-owner",
            || 1,
            3,
            &mut effect
        )
        .is_err()
    );
    assert_eq!(effect, "committed");
    host.store_local_mut()
        .retain_blob_local(&blob.id, &owner(&r), RetentionKind::Snapshot)
        .unwrap();
    let recovered = crate::editor_draft::discard_with_effect_at(
        &mut host,
        CARD,
        DRAFT,
        1,
        "discard-missing-owner",
        || 1,
        3,
        &mut effect,
    )
    .unwrap();
    assert!(recovered.repeated && !recovered.current_active);
    assert!(current(&host).entries.is_empty());
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_none()
    );
}

#[test]
fn consumed_binding_and_original_ready_identity_reject_tampered_metadata() {
    let (_directory, _path, mut host) = fixture();
    let r = request("consumed-binding", 1, BYTES);
    let import = ready(&mut host, &r, BYTES);
    let mut selected = draft("select-binding", 1);
    selected.assets.push(main_proto::AssetSelection {
        origin: 2,
        asset_id: import.asset_id,
        aliases: vec![],
    });
    let saved = crate::editor_draft::save_with_effect_at(
        &mut host,
        &selected,
        || 1,
        2,
        &mut "not_committed",
    )
    .unwrap();
    let entry = current(&host).entries[0].clone();
    Reader { host: &host }
        .verify_consumed(&saved.slot, &entry)
        .unwrap();
    let mut changed = saved.slot.clone();
    changed.assets[0].display_name = "changed.bin".into();
    assert!(
        Reader { host: &host }
            .verify_consumed(&changed, &entry)
            .is_err()
    );
    changed = saved.slot.clone();
    changed.assets[0].selection.as_mut().unwrap().origin = 3;
    assert!(
        Reader { host: &host }
            .verify_consumed(&changed, &entry)
            .is_err()
    );
    changed = saved.slot;
    changed.assets.push(changed.assets[0].clone());
    assert!(
        Reader { host: &host }
            .verify_consumed(&changed, &entry)
            .is_err()
    );
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_some()
    );

    // Canonical protobuf alone is insufficient to rewrite the original import name.
    let mut tampered = current(&host);
    tampered.entries[0].request.as_mut().unwrap().name = "rewritten.bin".into();
    tampered.revision += 1;
    tampered.last_mutation.as_mut().unwrap().operation_id = "tampered-ready-identity".into();
    Writer {
        host: &mut host,
        clock: || 1,
        blob_unix_ms: 2,
        effect: &mut "not_committed",
    }
    .write_staging_slot(&tampered, tampered.revision - 1, "tampered-ready-identity")
    .unwrap();
    assert!(inspect(&host, CARD, DRAFT, &r.operation_id).is_err());
    assert!(resolve_durable_draft_import(&host, CARD, DRAFT, &entry.asset_id, 2).is_err());
    assert!(reconcile(&mut host, CARD, DRAFT, || 1, 3, &mut "not_committed").is_err());
    assert!(
        host.store_local()
            .retained_blob_local(&owner(&r))
            .unwrap()
            .is_some()
    );
}
