use super::*;
use morrow_core::store::{EventBudget, Store};
use std::io::Cursor;

const CARD: &str = "fork-card";
const PARENT: &str = "fork-parent";
const CHILD: &str = "fork-child";
const FIRST: &str = "fork-first";
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
fn fixture() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    HostRuntime,
    DraftRecord,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hmos-development.sqlite");
    let mut host = reopen(&path, 1024);
    let parent = save_with_effect_at(
        &mut host,
        &request("parent-first", 0),
        || 1,
        10,
        &mut "not_committed",
    )
    .unwrap();
    (directory, path, host, parent)
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
fn request(op: &str, generation: u64) -> proto::WriteRequest {
    proto::WriteRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: PARENT.into(),
        operation_id: op.into(),
        expected_generation: generation,
        source_kind: 1,
        values: Some(proto::Values {
            title: Some(value("")),
            description: Some(value("raw parent")),
            hypothesis: Some(value("")),
            conclusion: Some(value("")),
            todos: Some(value("same\nsame")),
            category: "灵感".into(),
            stage: "待整理".into(),
        }),
        ..Default::default()
    }
}
fn child(parent: &DraftRecord) -> (proto::WriteRequest, proto::DevelopmentForkLink) {
    let original = parent.slot.request.as_ref().unwrap();
    let mut request = original.clone();
    request.draft_id = CHILD.into();
    request.operation_id = FIRST.into();
    request.expected_generation = 0;
    request.values.as_mut().unwrap().description = Some(proto::TextValue {
        text: " A😀中\nlate raw ".into(),
        selection_base: 3,
        selection_extent: 5,
        affinity: 1,
        directional: true,
        composing_start: 2,
        composing_end: 4,
    });
    for asset in &mut request.assets {
        asset.origin = 4;
    }
    (
        request,
        proto::DevelopmentForkLink {
            schema_version: 1,
            parent_draft_id: original.draft_id.clone(),
            parent_generation: parent.slot.generation,
            parent_save_operation: original.operation_id.clone(),
            parent_request_sha256: Sha256::digest(original.encode_to_vec()).to_vec(),
            child_operation: FIRST.into(),
        },
    )
}
fn fork(
    host: &mut HostRuntime,
    request: &proto::WriteRequest,
    link: &proto::DevelopmentForkLink,
) -> DraftRecord {
    fork_with_effect_at(host, request, link, || 1, 20, &mut "not_committed").unwrap()
}
fn marker(link: &proto::DevelopmentForkLink) -> proto::DevelopmentForkRetirement {
    proto::DevelopmentForkRetirement {
        schema_version: 1,
        child_draft_id: CHILD.into(),
        operation_id: "parent-retire".into(),
        fork_link: Some(link.clone()),
    }
}
fn retire(host: &mut HostRuntime, link: &proto::DevelopmentForkLink) -> DraftRecord {
    retire_fork_with_effect_at(host, CARD, &marker(link), || 1, 30, &mut "not_committed").unwrap()
}
fn journal(host: &HostRuntime, draft: &str) -> Vec<u8> {
    host.store_local()
        .card(&key(CARD, draft))
        .unwrap()
        .unwrap()
        .encode()
}
fn import_request(
    parent: &DraftRecord,
    op: &str,
    bytes: &[u8],
) -> crate::editor_draft_staging::proto::ImportRequest {
    let original = parent.slot.request.as_ref().unwrap();
    crate::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: original.draft_id.clone(),
        operation_id: op.into(),
        expected_generation: parent.slot.generation,
        name: format!("{op}.bin"),
        kind: "file".into(),
        byte_length: bytes.len() as u64,
        sha256: Sha256::digest(bytes).to_vec(),
    }
}
fn pin(host: &mut HostRuntime, parent: DraftRecord, op: &str, bytes: &[u8]) -> DraftRecord {
    let import = import_request(&parent, op, bytes);
    let ready = crate::editor_draft_staging::import_durable(
        host,
        &import,
        &mut Cursor::new(bytes),
        || 1,
        10,
        &mut "not_committed",
    )
    .unwrap();
    let mut update = parent.slot.request.unwrap();
    update.expected_generation = parent.slot.generation;
    update.operation_id = format!("select-{op}");
    for a in &mut update.assets {
        a.origin = 3;
    }
    update.assets.push(proto::AssetSelection {
        origin: 2,
        asset_id: ready.asset_id,
        aliases: vec![format!("alias-{op}")],
    });
    save_with_effect_at(host, &update, || 1, 10, &mut "not_committed").unwrap()
}

#[test]
fn new_card_complete_late_raw_two_phase_reopen_and_exact_history() {
    let (_dir, path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    let before = journal(&host, PARENT);
    let first = fork(&mut host, &request, &link);
    assert_eq!(first.slot.request.as_ref(), Some(&request));
    assert_eq!(first.slot.source_card, parent.slot.source_card);
    assert_eq!(journal(&host, PARENT), before);
    assert_eq!(list(&host).unwrap().len(), 2);
    assert!(host.store_local().card(CARD).unwrap().is_none());
    drop(host);
    let mut host = reopen(&path, 1024);
    let retired = retire(&mut host, &link);
    assert!(!retired.slot.active);
    assert_eq!(retired.slot.generation, 2);
    assert_eq!(list(&host).unwrap().len(), 1);
    let view =
        serde_json::to_value(crate::draft_bridge::View::from_record(retired.clone()).unwrap())
            .unwrap();
    assert_eq!(
        view["request_sha256"],
        request_sha256(parent.slot.request.as_ref().unwrap())
    );
    assert_eq!(
        view["fork_retirement"]["fork_link"]["parent_generation"],
        "1"
    );
    drop(host);
    let mut host = reopen(&path, 1024);
    let mut next = request.clone();
    next.expected_generation = 1;
    next.operation_id = "child-second".into();
    next.values.as_mut().unwrap().title = Some(value("newer child"));
    save(&mut host, &next, || 1).unwrap();
    let retry = fork_with_effect_at(
        &mut host,
        &request,
        &link,
        || panic!("first outcome retry is read-only"),
        30,
        &mut "not_committed",
    )
    .unwrap();
    assert!(retry.repeated);
    assert_eq!(retry.slot, first.slot);
    assert_eq!(retry.current_generation, 2);
    assert_eq!(
        read(&host, CARD, CHILD).unwrap().unwrap().slot.request,
        Some(next)
    );
    let repeated = retire(&mut host, &link);
    assert!(repeated.repeated);
    assert_eq!(repeated.slot, retired.slot);
    assert!(
        discard(&mut host, CARD, PARENT, 1, "parent-retire", || 1)
            .unwrap_err()
            .contains("DedicatedAction")
    );
    host.store_local().integrity_check().unwrap();
}

#[test]
fn unpublished_selected_pins_export_bytes_after_parent_retirement_and_child_origin3_save() {
    let (_dir, path, mut host, parent) = fixture();
    let bytes = b"private uncommitted bytes \x00\xff";
    let parent = pin(&mut host, parent, "chosen", bytes);
    let (request, link) = child(&parent);
    fork(&mut host, &request, &link);
    retire(&mut host, &link);
    drop(host);
    let mut host = reopen(&path, 1024);
    let mut output = vec![];
    let info = export_asset_verified(
        &host,
        CARD,
        CHILD,
        1,
        &request.assets[0].asset_id,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, bytes);
    assert_eq!(info.sha256, <[u8; 32]>::from(Sha256::digest(bytes)));
    let mut next = request.clone();
    next.expected_generation = 1;
    next.operation_id = "child-pins-next".into();
    next.assets[0].origin = 3;
    let saved = save(&mut host, &next, || 1).unwrap();
    assert_eq!(saved.slot.development_fork_link, Some(link));
    output.clear();
    export_asset_verified(
        &host,
        CARD,
        CHILD,
        2,
        &request.assets[0].asset_id,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, bytes);
    assert!(host.store_local().card(CARD).unwrap().is_none());
    host.store_local().integrity_check().unwrap();
}

#[test]
fn fork_requires_exact_aliases_order_and_parent_owned_pins_but_allows_subset() {
    let (_dir, _path, mut host, parent) = fixture();
    let parent = pin(&mut host, parent, "a", b"A");
    let parent = pin(&mut host, parent, "b", b"B");
    let (request, link) = child(&parent);
    let mut changed = request.clone();
    changed.assets[0].aliases.push("late alias".into());
    assert!(
        fork_with_effect_at(&mut host, &changed, &link, || 1, 20, &mut "not_committed")
            .unwrap_err()
            .contains("AliasesChanged")
    );
    changed = request.clone();
    changed.assets.reverse();
    assert!(
        fork_with_effect_at(&mut host, &changed, &link, || 1, 20, &mut "not_committed")
            .unwrap_err()
            .contains("OrderChanged")
    );
    changed = request.clone();
    changed.assets[0].asset_id = "unowned".into();
    assert!(
        fork_with_effect_at(&mut host, &changed, &link, || 1, 20, &mut "not_committed")
            .unwrap_err()
            .contains("PinMissing")
    );
    let mut subset = request;
    subset.assets.remove(0);
    let accepted = fork(&mut host, &subset, &link);
    assert_eq!(accepted.slot.assets.len(), 1);
    let mut output = vec![];
    export_asset_verified(
        &host,
        CARD,
        CHILD,
        1,
        &subset.assets[0].asset_id,
        &mut output,
    )
    .unwrap();
    assert_eq!(output, b"B");
}

#[test]
fn parent_mutations_and_second_fork_sealed_while_child_can_keep_raw() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    fork(&mut host, &request, &link);
    let before = journal(&host, PARENT);
    let mut late = parent.slot.request.clone().unwrap();
    late.expected_generation = 1;
    late.operation_id = "parent-late".into();
    assert!(
        save(&mut host, &late, || 1)
            .unwrap_err()
            .contains("ParentFrozen")
    );
    assert!(
        discard(&mut host, CARD, PARENT, 1, "parent-discard", || 1)
            .unwrap_err()
            .contains("ParentFrozen")
    );
    let import = import_request(&parent, "new-import", b"unselected");
    assert!(
        crate::editor_draft_staging::begin(&mut host, &import, || 1, 20, &mut "not_committed")
            .unwrap_err()
            .to_string()
            .contains("ParentFrozen")
    );
    let mut second = request.clone();
    second.draft_id = "second-child".into();
    second.operation_id = "second-fork".into();
    let mut second_link = link.clone();
    second_link.child_operation = second.operation_id.clone();
    assert!(
        fork_with_effect_at(
            &mut host,
            &second,
            &second_link,
            || 1,
            20,
            &mut "not_committed"
        )
        .unwrap_err()
        .contains("ParentFrozen")
    );
    assert!(
        discard(&mut host, CARD, CHILD, 1, "child-premature-discard", || 1)
            .unwrap_err()
            .contains("ParentNotRetired")
    );
    let mut next = request;
    next.expected_generation = 1;
    next.operation_id = "child-raw-next".into();
    next.values.as_mut().unwrap().description = Some(value("latest raw still writable"));
    save(&mut host, &next, || 1).unwrap();
    assert_eq!(journal(&host, PARENT), before);
    retire(&mut host, &link);
    discard(&mut host, CARD, CHILD, 2, "child-final-discard", || 1).unwrap();
    assert!(
        fork_with_effect_at(
            &mut host,
            &second,
            &second_link,
            || 1,
            30,
            &mut "not_committed"
        )
        .unwrap_err()
        .contains("ParentFrozen")
    );
}

#[test]
fn unresolved_pending_or_ready_import_blocks_fork_without_freezing_parent() {
    for ready in [false, true] {
        let (_dir, _path, mut host, parent) = fixture();
        let import = import_request(&parent, "not-selected", b"unselected bytes");
        if ready {
            crate::editor_draft_staging::import_durable(
                &mut host,
                &import,
                &mut Cursor::new(b"unselected bytes"),
                || 1,
                10,
                &mut "not_committed",
            )
            .unwrap();
        } else {
            crate::editor_draft_staging::begin(&mut host, &import, || 1, 10, &mut "not_committed")
                .unwrap();
        }
        let before = journal(&host, PARENT);
        let (request, link) = child(&parent);
        let mut effect = "unknown";
        assert!(
            fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut effect)
                .unwrap_err()
                .contains("UnselectedImports")
        );
        assert_eq!(effect, "not_committed");
        assert_eq!(journal(&host, PARENT), before);
        assert!(read(&host, CARD, CHILD).unwrap().is_none());
        crate::editor_draft_staging::abandon(
            &mut host,
            CARD,
            PARENT,
            1,
            "not-selected",
            "explicit-abandon",
            || 1,
            20,
            &mut "not_committed",
        )
        .unwrap();
        fork(&mut host, &request, &link);
    }
}

#[test]
fn proof_shape_sha_generation_source_identity_fail_before_child_write() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    for case in 0..9 {
        let mut r = request.clone();
        let mut l = link.clone();
        match case {
            0 => l.parent_request_sha256[0] ^= 1,
            1 => l.parent_generation = 2,
            2 => l.parent_save_operation = "unknown-parent-op".into(),
            3 => r.card_id = "different-card".into(),
            4 => r.source_kind = 0,
            5 => r.source_revision = 1,
            6 => l.parent_draft_id = CHILD.into(),
            7 => l.schema_version = 2,
            8 => r.expected_generation = 1,
            _ => unreachable!(),
        }
        let mut effect = "unknown";
        assert!(
            fork_with_effect_at(
                &mut host,
                &r,
                &l,
                || panic!("proof rejection must not grant write"),
                20,
                &mut effect
            )
            .is_err(),
            "case {case}"
        );
        assert_eq!(effect, "not_committed");
        assert!(read(&host, CARD, CHILD).unwrap().is_none());
    }
    let mut late = parent.slot.request.clone().unwrap();
    late.expected_generation = 1;
    late.operation_id = "parent-new-generation".into();
    save(&mut host, &late, || 1).unwrap();
    assert!(
        fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut "not_committed")
            .unwrap_err()
            .contains("GenerationConflict")
    );
}

#[test]
fn retirement_requires_exact_first_child_link_and_parent_cas() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    assert!(
        retire_fork_with_effect_at(
            &mut host,
            CARD,
            &marker(&link),
            || 1,
            20,
            &mut "not_committed"
        )
        .unwrap_err()
        .contains("ChildMissing")
    );
    fork(&mut host, &request, &link);
    let before = journal(&host, PARENT);
    for case in 0..7 {
        let mut m = marker(&link);
        match case {
            0 => m.child_draft_id = "wrong-child".into(),
            1 => m.fork_link.as_mut().unwrap().child_operation = "wrong-first".into(),
            2 => m.fork_link.as_mut().unwrap().parent_request_sha256[0] ^= 1,
            3 => m.fork_link.as_mut().unwrap().parent_generation = 2,
            4 => m.schema_version = 2,
            5 => m.operation_id = FIRST.into(),
            6 => m.operation_id = "parent-first".into(),
            _ => unreachable!(),
        }
        let mut effect = "unknown";
        assert!(
            retire_fork_with_effect_at(
                &mut host,
                CARD,
                &m,
                || panic!("retirement proof rejection must not write"),
                20,
                &mut effect
            )
            .is_err()
        );
        assert_eq!(
            effect,
            if case == 6 {
                "committed"
            } else {
                "not_committed"
            }
        );
        assert_eq!(journal(&host, PARENT), before);
    }
    retire(&mut host, &link);
}

#[test]
fn committed_exact_retries_preserve_effect_and_never_reactivate_inactive_child() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    fork(&mut host, &request, &link);
    retire(&mut host, &link);
    discard(&mut host, CARD, CHILD, 1, "child-done", || 1).unwrap();
    let before = journal(&host, CHILD);
    let mut changed = request.clone();
    changed.values.as_mut().unwrap().title = Some(value("changed retry"));
    let mut effect = "unknown";
    assert!(
        fork_with_effect_at(&mut host, &changed, &link, || 1, 30, &mut effect)
            .unwrap_err()
            .contains("RetryPayloadChanged")
    );
    assert_eq!(effect, "committed");
    let repeated = fork_with_effect_at(
        &mut host,
        &request,
        &link,
        || panic!("read-only retry"),
        30,
        &mut effect,
    )
    .unwrap();
    assert!(repeated.slot.active);
    assert!(!repeated.current_active);
    assert_eq!(repeated.current_generation, 2);
    assert_eq!(journal(&host, CHILD), before);
    let mut changed_marker = marker(&link);
    changed_marker
        .fork_link
        .as_mut()
        .unwrap()
        .parent_request_sha256[0] ^= 1;
    assert!(
        retire_fork_with_effect_at(&mut host, CARD, &changed_marker, || 1, 30, &mut effect)
            .unwrap_err()
            .contains("RetryChanged")
    );
    assert_eq!(effect, "committed");
}

#[test]
fn fork_and_retirement_unknown_event_capacity_keep_original_for_retry() {
    for phase in ["fork", "retire"] {
        let (_dir, path, mut host, parent) = fixture();
        let (request, link) = child(&parent);
        if phase == "retire" {
            fork(&mut host, &request, &link);
        }
        let previous = journal(&host, PARENT);
        drop(host);
        let mut host = reopen(&path, if phase == "fork" { 1 } else { 2 });
        let mut effect = "not_committed";
        let result = if phase == "fork" {
            fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut effect)
        } else {
            retire_fork_with_effect_at(&mut host, CARD, &marker(&link), || 1, 30, &mut effect)
        };
        assert_eq!(result.unwrap_err(), "EventCapacity");
        assert_eq!(effect, "unknown");
        assert_eq!(journal(&host, PARENT), previous);
        assert!(matches!(
            host.store_local()
                .lookup(if phase == "fork" {
                    FIRST
                } else {
                    "parent-retire"
                })
                .unwrap(),
            Lookup::Absent
        ));
        drop(host);
        let mut host = reopen(&path, 1024);
        if phase == "fork" {
            fork(&mut host, &request, &link);
        } else {
            retire(&mut host, &link);
        }
        host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn child_counts_as_second_active_slot_and_fields_keep_original_limits() {
    let (_dir, _path, mut host, parent) = fixture();
    for i in 0..15 {
        let mut other = request(&format!("fill-{i}"), 0);
        other.draft_id = format!("fill-{i}");
        save(&mut host, &other, || 1).unwrap();
    }
    let (request, link) = child(&parent);
    let mut effect = "unknown";
    assert!(
        fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut effect)
            .unwrap_err()
            .contains("active capacity")
    );
    assert_eq!(effect, "not_committed");
    discard(&mut host, CARD, "fill-0", 1, "remove-fill", || 1).unwrap();
    let mut oversized = request.clone();
    oversized.values.as_mut().unwrap().description = Some(value(&"x".repeat(512 * 1024 + 1)));
    assert!(fork_with_effect_at(&mut host, &oversized, &link, || 1, 20, &mut effect).is_err());
    assert_eq!(effect, "not_committed");
    fork(&mut host, &request, &link);
    assert_eq!(list(&host).unwrap().len(), 16);
}

#[test]
fn grant_expiry_cannot_bypass_fork_or_retirement_authority() {
    let (_dir, path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    let mut calls = 0;
    let mut effect = "not_committed";
    let result = fork_with_effect_at(
        &mut host,
        &request,
        &link,
        || {
            calls += 1;
            if calls == 1 { 1 } else { 30002 }
        },
        20,
        &mut effect,
    );
    assert!(result.is_err());
    assert!(read(&host, CARD, CHILD).unwrap().is_none());
    assert!(read(&host, CARD, PARENT).unwrap().unwrap().slot.active);
    drop(host);
    let mut host = reopen(&path, 1024);
    fork(&mut host, &request, &link);
    calls = 0;
    assert!(
        retire_fork_with_effect_at(
            &mut host,
            CARD,
            &marker(&link),
            || {
                calls += 1;
                if calls == 1 { 1 } else { 30002 }
            },
            30,
            &mut effect
        )
        .is_err()
    );
    assert!(read(&host, CARD, PARENT).unwrap().unwrap().slot.active);
    drop(host);
    let mut host = reopen(&path, 1024);
    retire(&mut host, &link);
}

#[test]
fn existing_source_bytes_are_inherited_without_adopting_new_business_revision() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.sqlite");
    let mut host = reopen(&path, 1024);
    let properties = tasks_v2::Properties {
        version: 2,
        category: "灵感".into(),
        stage: "待整理".into(),
        description: "source".into(),
        ..Default::default()
    };
    let source =
        CardRecord::new(CARD, "idea", 2, "source title", properties.encode_to_vec()).unwrap();
    let mut raw = source.encode();
    raw.extend_from_slice(&[0xaa, 0x06, 0x02, 0x01, 0xff]);
    let source = CardRecord::decode(&raw).unwrap();
    host.store_local_mut()
        .create_local("seed-source", &source)
        .unwrap();
    let mut r = request("source-parent", 0);
    r.source_kind = 0;
    r.source_revision = 1;
    let parent = save(&mut host, &r, || 1).unwrap();
    let mut connection = host.connect().unwrap();
    host.grant(&mut connection, GrantKind::EditContent, CARD, 30001, 1)
        .unwrap();
    host.edit_versioned_content(
        &connection,
        &morrow_core::versioned_content_change::VersionedContentChange {
            operation_id: "external-edit".into(),
            source_card: source.encode(),
            title: "new live title".into(),
            body: properties.encode_to_vec(),
            preview_text: String::new(),
            attachments: None,
        },
        || 1,
    )
    .unwrap();
    host.disconnect(&connection).unwrap();
    let live = host.store_local().card(CARD).unwrap().unwrap().encode();
    let (request, link) = child(&parent);
    let first = fork(&mut host, &request, &link);
    assert_eq!(first.slot.source_card, source.encode());
    assert_ne!(first.slot.source_card, live);
    retire(&mut host, &link);
    assert_eq!(
        host.store_local().card(CARD).unwrap().unwrap().encode(),
        live
    );
    drop(host);
    let host = reopen(&path, 1024);
    assert_eq!(
        read(&host, CARD, CHILD).unwrap().unwrap().slot.source_card,
        source.encode()
    );
}

#[test]
fn protected_lineage_and_ordinary_origin4_are_still_rejected() {
    let (_dir, _path, mut host, parent) = fixture();
    let (mut request, link) = child(&parent);
    request.assets.push(proto::AssetSelection {
        origin: 4,
        asset_id: "nonexistent".into(),
        aliases: vec![],
    });
    assert!(save(&mut host, &request, || 1).is_err());
    request.assets.clear();
    request.predecessor_operation = "fake-protected".into();
    request.predecessor_sha256 = vec![0; 32];
    assert!(
        fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut "not_committed").is_err()
    );
    let mut slot = parent.slot;
    slot.parent_link = Some(Default::default());
    assert!(decode_body(&slot.encode_to_vec()).is_err());
}

#[test]
fn selected_blob_is_logically_charged_twice_while_parent_and_child_are_active() {
    let (_dir, _path, mut host, parent) = fixture();
    let bytes = vec![0x5a; 33 * 1024 * 1024];
    let parent = pin(&mut host, parent, "large-pin", &bytes);
    let (request, link) = child(&parent);
    let before = journal(&host, PARENT);
    let mut effect = "unknown";
    assert!(
        fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut effect)
            .unwrap_err()
            .contains("active capacity")
    );
    assert_eq!(effect, "not_committed");
    assert_eq!(journal(&host, PARENT), before);
    assert!(read(&host, CARD, CHILD).unwrap().is_none());
    let mut exported = vec![];
    export_asset_verified(
        &host,
        CARD,
        PARENT,
        parent.slot.generation,
        &request.assets[0].asset_id,
        &mut exported,
    )
    .unwrap();
    assert_eq!(exported, bytes);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn inactive_journals_still_count_against_fork_identity_capacity_after_reopen() {
    let (_dir, path, mut host, parent) = fixture();
    for i in 0..255 {
        let mut old = request(&format!("past-{i}"), 0);
        old.draft_id = format!("past-{i}");
        save(&mut host, &old, || 1).unwrap();
        discard(
            &mut host,
            CARD,
            &old.draft_id,
            1,
            &format!("retire-past-{i}"),
            || 1,
        )
        .unwrap();
    }
    drop(host);
    let mut host = reopen(&path, 1024);
    let (request, link) = child(&parent);
    let mut effect = "unknown";
    assert!(
        fork_with_effect_at(&mut host, &request, &link, || 1, 20, &mut effect)
            .unwrap_err()
            .contains("identity capacity")
    );
    assert_eq!(effect, "not_committed");
    assert!(read(&host, CARD, CHILD).unwrap().is_none());
    assert_eq!(list(&host).unwrap().len(), 1);
}

#[test]
fn actual_engine_dispatch_round_trip_fork_then_retire_and_history_after_restart() {
    let (_dir, path, host, parent) = fixture();
    let (request, link) = child(&parent);
    drop(host);
    let child_wire = serde_json::json!({"card_id":CARD,"draft_id":CHILD,"operation_id":FIRST,"expected_generation":"0","source_kind":1,
        "source_revision":"0","values":crate::draft_bridge::Values::from(request.values.as_ref().unwrap()), "assets":[]});
    let fork_wire = serde_json::json!({"action":"draft_fork","fork":{"child":child_wire,"parent_draft_id":PARENT,
        "parent_generation":"1","parent_save_operation":"parent-first","parent_request_sha256":crate::hex(&link.parent_request_sha256)}});
    let retire_wire = serde_json::json!({"action":"draft_fork_retire","fork_retirement":{"card_id":CARD,"child_draft_id":CHILD,"child_operation":FIRST,
        "parent_draft_id":PARENT,"parent_generation":"1","parent_save_operation":"parent-first","parent_request_sha256":crate::hex(&link.parent_request_sha256),"operation_id":"parent-retire"}});
    let mut engine = crate::Engine::open(&path).unwrap();
    let first = serde_json::to_value(
        engine
            .execute(serde_json::from_value(fork_wire.clone()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(first["effect"], "committed");
    assert_eq!(first["profile"], "development-unsealed");
    assert_eq!(
        first["drafts"][0]["request_sha256"],
        request_sha256(&request)
    );
    let retired = serde_json::to_value(
        engine
            .execute(serde_json::from_value(retire_wire.clone()).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        retired["drafts"][0]["fork_retirement"]["fork_link"],
        first["drafts"][0]["fork_link"]
    );
    assert_eq!(retired["drafts"][0]["active"], false);
    assert!(engine.ids().unwrap().is_empty());
    drop(engine);
    let mut engine = crate::Engine::open(&path).unwrap();
    let retried = serde_json::to_value(
        engine
            .execute(serde_json::from_value(fork_wire).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(retried["drafts"][0]["repeated"], true);
    assert_eq!(
        retried["drafts"][0]["values"]["description"]["text"],
        request.values.unwrap().description.unwrap().text
    );
    assert_eq!(
        serde_json::to_value(
            engine
                .execute(serde_json::from_value(retire_wire).unwrap())
                .unwrap()
        )
        .unwrap()["drafts"][0]["repeated"],
        true
    );
}

#[test]
fn child_can_fork_again_only_after_exact_parent_retirement() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    let first = fork(&mut host, &request, &link);
    let (mut next, mut next_link) = child(&first);
    next.draft_id = "grandchild".into();
    next.operation_id = "grandchild-first".into();
    next_link.child_operation = next.operation_id.clone();
    assert!(
        fork_with_effect_at(&mut host, &next, &next_link, || 1, 25, &mut "not_committed")
            .unwrap_err()
            .contains("ParentNotRetired")
    );
    retire(&mut host, &link);
    fork_with_effect_at(&mut host, &next, &next_link, || 1, 30, &mut "not_committed").unwrap();
    let m = proto::DevelopmentForkRetirement {
        schema_version: 1,
        child_draft_id: "grandchild".into(),
        operation_id: "child-retire-to-grandchild".into(),
        fork_link: Some(next_link),
    };
    retire_fork_with_effect_at(&mut host, CARD, &m, || 1, 40, &mut "not_committed").unwrap();
    assert_eq!(list(&host).unwrap().len(), 1);
    assert_eq!(
        read(&host, CARD, "grandchild")
            .unwrap()
            .unwrap()
            .slot
            .request,
        Some(next)
    );
}

#[test]
fn strict_json_envelopes_export_complete_proofs_and_canonical_request_sha() {
    let (_dir, _path, mut host, parent) = fixture();
    let (request, link) = child(&parent);
    let view = serde_json::to_value(
        crate::draft_bridge::View::from_record(DraftRecord {
            slot: proto::Slot {
                request: Some(request.clone()),
                ..parent.slot.clone()
            },
            ..parent.clone()
        })
        .unwrap(),
    )
    .unwrap();
    let mut values = view["values"].clone();
    values.as_object_mut().unwrap().remove("assets");
    let child_json = serde_json::json!({"card_id":CARD,"draft_id":CHILD,"operation_id":FIRST,"expected_generation":"0","source_kind":1,"source_revision":"0", "values":values, "assets":[]});
    let wire = serde_json::json!({"child":child_json,"parent_draft_id":PARENT,"parent_generation":"1","parent_save_operation":"parent-first","parent_request_sha256":crate::hex(&link.parent_request_sha256)});
    let decoded: crate::draft_bridge::Fork = serde_json::from_value(wire.clone()).unwrap();
    let (r, l) = decoded.request().unwrap();
    assert_eq!(r, request);
    assert_eq!(l, link);
    for case in ["uppercase", "leadingzero", "unknown"] {
        let mut w = wire.clone();
        match case {
            "uppercase" => {
                w["parent_request_sha256"] = crate::hex(&link.parent_request_sha256)
                    .to_uppercase()
                    .into()
            }
            "leadingzero" => w["parent_generation"] = "01".into(),
            _ => w["parent_link"] = serde_json::json!({}),
        }
        let parsed = serde_json::from_value::<crate::draft_bridge::Fork>(w);
        assert!(parsed.is_err() || parsed.unwrap().request().is_err());
    }
    let first = fork(&mut host, &request, &link);
    let response =
        serde_json::to_value(crate::draft_bridge::View::from_record(first).unwrap()).unwrap();
    assert_eq!(response["request_sha256"], request_sha256(&request));
    assert_eq!(response["fork_link"]["child_operation"], FIRST);
    assert!(response["fork_retirement"].is_null());
}

#[test]
#[ignore = "explicit fault-injection host subprocess test; not an OHOS or cross-process concurrency claim"]
fn actual_store_crash_boundaries_preserve_pins_and_exact_original_outcomes() {
    if let Some(path) = std::env::var_os("HMOS_DRAFT_FORK_CRASH_DB") {
        let mut host = reopen(std::path::Path::new(&path), 1024);
        let parent = read(&host, CARD, PARENT).unwrap().unwrap();
        let (request, link) = child(&parent);
        if std::env::var("HMOS_DRAFT_FORK_CRASH_PHASE").unwrap() == "fork" {
            fork(&mut host, &request, &link);
        } else {
            retire(&mut host, &link);
        }
        panic!("explicit fault feature must terminate subprocess");
    }
    for phase in ["fork", "retire"] {
        for boundary in [
            "after-begin",
            "after-card",
            "after-operation",
            "after-event",
            "after-task-evidence",
            "before-commit",
            "after-commit",
        ] {
            let (_dir, path, mut host, parent) = fixture();
            let parent = pin(&mut host, parent, "crash-pin", b"crash durable bytes");
            let (request, link) = child(&parent);
            // Finish independent consumed-import cleanup before the scoped crash;
            // this isolates the one tested journal transaction boundary.
            crate::editor_draft_staging::reconcile(
                &mut host,
                CARD,
                PARENT,
                || 1,
                20,
                &mut "not_committed",
            )
            .unwrap();
            if phase == "retire" {
                fork(&mut host, &request, &link);
            }
            let parent_before = journal(&host, PARENT);
            drop(host);
            let child = std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact", "editor_draft::fork::tests::actual_store_crash_boundaries_preserve_pins_and_exact_original_outcomes", "--ignored", "--nocapture"])
            .env("HMOS_DRAFT_FORK_CRASH_DB", &path).env("HMOS_DRAFT_FORK_CRASH_PHASE", phase).env("MORROW_TEST_CRASH_AT", boundary).output().unwrap();
            assert_eq!(
                child.status.code(),
                Some(86),
                "{phase}/{boundary}: {}",
                String::from_utf8_lossy(&child.stderr)
            );
            let mut host = reopen(&path, 1024);
            let committed = boundary == "after-commit";
            let operation = if phase == "fork" {
                FIRST
            } else {
                "parent-retire"
            };
            assert_eq!(
                matches!(
                    host.store_local().lookup(operation).unwrap(),
                    Lookup::Committed(_)
                ),
                committed,
                "{phase}/{boundary}"
            );
            if phase == "fork" {
                assert_eq!(read(&host, CARD, CHILD).unwrap().is_some(), committed);
                assert_eq!(journal(&host, PARENT), parent_before);
                fork(&mut host, &request, &link);
            } else {
                assert_eq!(
                    read(&host, CARD, PARENT).unwrap().unwrap().slot.active,
                    !committed
                );
                retire(&mut host, &link);
            }
            let mut bytes = vec![];
            export_asset_verified(
                &host,
                CARD,
                CHILD,
                1,
                &request.assets[0].asset_id,
                &mut bytes,
            )
            .unwrap();
            assert_eq!(bytes, b"crash durable bytes");
            host.store_local().integrity_check().unwrap();
            println!(
                "phase={phase} boundary={boundary} original_committed={committed} exact_retry=PASS exported_bytes={} exported_sha256={}",
                bytes.len(),
                crate::hex(&Sha256::digest(&bytes))
            );
        }
    }
}
