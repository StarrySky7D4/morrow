use super::*;
use prost::Message;
use sha2::{Digest, Sha256};

fn export_history(host: &mut Workbench, draft: &str) -> Vec<u8> {
    use capnp::{message::Builder, message::ReaderOptions, serialize};
    use morrow_workbench_host::{
        editor_draft_api_capnp as draft_wire, host_capnp as wire, protocol,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    static EXPORT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = EXPORT_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    assert!(sequence < 1000);
    let correlation = format!("draft-request-{sequence}");
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("history.json");
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::ExportEditorDraftHistory);
    request.set_id(CARD);
    request.set_attachment(draft);
    request.set_revision(2);
    request.set_transfer(&correlation);
    request.set_selected_path(destination.to_str().unwrap());
    let raw = protocol::respond(host, &serialize::write_message_to_words(&message)).unwrap();
    let response = serialize::read_message(&mut raw.as_slice(), ReaderOptions::new()).unwrap();
    let reply = response.get_root::<wire::response::Reader>().unwrap();
    assert!(
        reply.get_error().unwrap().to_str().unwrap().is_empty(),
        "{}",
        reply.get_error().unwrap().to_str().unwrap()
    );
    let payload = reply.get_payload().unwrap();
    assert_eq!(reply.get_total_length(), payload.len() as u64);
    assert_eq!(
        Sha256::digest(payload).as_slice(),
        reply.get_sha256().unwrap()
    );
    let mut payload_cursor: &[u8] = payload;
    let envelope = serialize::read_message(&mut payload_cursor, ReaderOptions::new()).unwrap();
    assert!(payload_cursor.is_empty());
    assert_eq!(
        envelope
            .get_root::<draft_wire::envelope::Reader>()
            .unwrap()
            .get_kind()
            .unwrap(),
        draft_wire::ResultKind::Exported
    );
    assert!(!reply.get_transfer().unwrap().to_str().unwrap().is_empty());
    let mut release = Builder::new_default();
    let mut request = release.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::AbortEditorDraftTransfer);
    request.set_transfer(&correlation);
    let raw = protocol::respond(host, &serialize::write_message_to_words(&release)).unwrap();
    let response = serialize::read_message(&mut raw.as_slice(), ReaderOptions::new()).unwrap();
    assert!(
        response
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    std::fs::read(destination).unwrap()
}

fn request_hash(record: &morrow_workbench_host::editor_draft::EditorDraftRecord) -> Vec<u8> {
    Sha256::digest(record.slot.request.as_ref().unwrap().encode_to_vec()).to_vec()
}

fn pending_import_for(
    host: &mut Workbench,
    draft: &str,
    operation: &str,
) -> morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
    let value = morrow_workbench_host::editor_draft_staging::proto::ImportRequest {
        schema_version: 1,
        card_id: CARD.into(),
        draft_id: draft.into(),
        operation_id: operation.into(),
        expected_generation: 1,
        name: "capacity.bin".into(),
        kind: "file".into(),
        byte_length: FILE_BYTES.len() as u64,
        sha256: Sha256::digest(FILE_BYTES).to_vec(),
    };
    host.begin_editor_draft_import(&value).unwrap();
    value
}

#[test]
#[ignore = "explicit Windows archive capacity qualification"]
fn actual_4096_main_archives_preserve_overflow_and_all_history_on_restart() {
    use std::collections::BTreeSet;
    const LIMIT: usize = 4096;
    let directory = tempfile::Builder::new()
        .prefix("archive-main-capacity-")
        .tempdir()
        .unwrap()
        .keep();
    let path = directory.join("workbench.db");
    eprintln!("ARCHIVE_MAIN_STORE={}", path.display());
    let mut host = seed(&path);
    let mut originals = Vec::new();
    for index in 0..LIMIT {
        let mut value = request(&format!("main-limit-save-{index:04}"), 0, "retained");
        value.draft_id = format!("main-limit-{index:04}");
        let saved = host.save_editor_draft(&value).unwrap();
        let request_digest = request_hash(&saved);
        let retired = host
            .discard_editor_draft(
                CARD,
                &value.draft_id,
                1,
                &format!("main-limit-retire-{index:04}"),
            )
            .unwrap();
        let operation = format!("main-limit-archive-{index:04}");
        let archived = host
            .archive_editor_draft(CARD, &value.draft_id, 2, &request_digest, &operation)
            .unwrap();
        assert!(!archived.repeated);
        let raw = retired.slot.encode_to_vec();
        assert_eq!(archived.slot_sha256, Sha256::digest(&raw).to_vec());
        originals.push((value.draft_id, request_digest, raw, operation));
        if (index + 1) % 128 == 0 {
            eprintln!("ARCHIVE_MAIN_PROGRESS archived={}", index + 1);
        }
    }
    let full = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!((full.archived, full.unarchived), (LIMIT as u32, 0));
    // The production identity limit is reached independently of the byte and
    // auxiliary limits. No lower test limits or direct database writes are used.
    assert!(full.archived_bytes < 64 * 1024 * 1024);
    let first_export = export_history(&mut host, &originals[0].0);
    let last_export = export_history(&mut host, &originals[LIMIT - 1].0);
    let mut overflow = request("main-limit-overflow-save", 0, "must survive refusal");
    overflow.draft_id = "main-limit-overflow".into();
    let saved = host.save_editor_draft(&overflow).unwrap();
    let retired = host
        .discard_editor_draft(CARD, &overflow.draft_id, 1, "main-limit-overflow-retire")
        .unwrap();
    let overflow_raw = retired.slot.encode_to_vec();
    let overflow_export = export_history(&mut host, &overflow.draft_id);
    assert!(
        host.archive_editor_draft(
            CARD,
            &overflow.draft_id,
            2,
            &request_hash(&saved),
            "main-limit-overflow-archive",
        )
        .unwrap_err()
        .to_string()
        .contains("draft archive capacity reached")
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, &overflow.draft_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        export_history(&mut host, &overflow.draft_id),
        overflow_export
    );
    let first = &originals[0];
    assert!(
        host.archive_editor_draft(CARD, &first.0, 2, &first.1, &first.3)
            .unwrap()
            .repeated
    );
    let repeated = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!(
        (
            repeated.archived,
            repeated.unarchived,
            repeated.archived_bytes
        ),
        (LIMIT as u32, 1, full.archived_bytes)
    );
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let mut cursor = String::new();
    let mut seen = BTreeSet::new();
    let expected: BTreeSet<_> = originals
        .iter()
        .map(|row| row.0.clone())
        .chain(std::iter::once(overflow.draft_id.clone()))
        .collect();
    loop {
        let page = host.list_editor_draft_history(&cursor, 32).unwrap();
        assert_eq!(
            (page.archived, page.unarchived, page.archived_bytes),
            (LIMIT as u32, 1, full.archived_bytes)
        );
        assert!(!page.rows.is_empty());
        for row in page.rows {
            assert_eq!(row.card_id, CARD);
            assert!(seen.insert(row.draft_id.clone()));
            assert_eq!(
                row.archive_operation.is_empty(),
                row.draft_id == overflow.draft_id
            );
        }
        if page.next_cursor.is_empty() {
            break;
        }
        assert!(page.next_cursor > cursor);
        cursor = page.next_cursor;
    }
    assert_eq!(seen, expected);
    for (index, (draft, digest, raw, operation)) in originals.iter().enumerate() {
        let retained = host.read_editor_draft(CARD, draft).unwrap().unwrap();
        assert_eq!(retained.slot.encode_to_vec(), *raw);
        let marker = host
            .inspect_editor_draft_archive(CARD, draft)
            .unwrap()
            .unwrap();
        assert_eq!(marker.request_sha256, *digest);
        assert_eq!(marker.slot_sha256, Sha256::digest(raw).to_vec());
        assert_eq!(marker.operation, *operation);
        if (index + 1) % 512 == 0 {
            eprintln!("ARCHIVE_MAIN_RESTART_VERIFIED={}", index + 1);
        }
    }
    assert_eq!(export_history(&mut host, &originals[0].0), first_export);
    assert_eq!(
        export_history(&mut host, &originals[LIMIT - 1].0),
        last_export
    );
    assert_eq!(
        host.read_editor_draft(CARD, &overflow.draft_id)
            .unwrap()
            .unwrap()
            .slot
            .encode_to_vec(),
        overflow_raw
    );
    assert_eq!(
        export_history(&mut host, &overflow.draft_id),
        overflow_export
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, &overflow.draft_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(business_revision(&host), 2);
    eprintln!(
        "ARCHIVE_MAIN_LIMIT_PASS archived=4096 rejected=1 all_original_slots_and_markers_preserved=true restart_preserved=true"
    );
}

// These invoke the real host, store, and compiled guest at the production
// limits. They are explicitly selected by the Windows capacity qualification.
#[test]
#[ignore = "explicit Windows archive capacity qualification"]
fn actual_4096_archived_auxiliary_limit_preserves_rejected_history_on_restart() {
    use morrow_workbench_host::editor_draft_import_decision::DecisionStatus;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let mut original_slots = Vec::new();
    for batch in 0..16 {
        let draft = format!("global-aux-{batch:02}");
        let mut value = request(
            &format!("global-aux-save-{batch:02}"),
            0,
            "retained history",
        );
        value.draft_id = draft.clone();
        let saved = host.save_editor_draft(&value).unwrap();
        let imported =
            pending_import_for(&mut host, &draft, &format!("global-aux-import-{batch:02}"));
        for index in 0..255 {
            let operation = format!("global-aux-decision-{batch:02}-{index:03}");
            assert_eq!(
                host.prepare_editor_draft_import_decision(
                    CARD,
                    &draft,
                    1,
                    &imported.operation_id,
                    &operation,
                )
                .unwrap()
                .status,
                DecisionStatus::Pending
            );
            assert_eq!(
                host.cancel_editor_draft_import_decision(
                    CARD,
                    &draft,
                    1,
                    &imported.operation_id,
                    &operation,
                )
                .unwrap()
                .status,
                DecisionStatus::Cancelled
            );
        }
        let retired = host
            .discard_editor_draft(CARD, &draft, 1, &format!("global-aux-retire-{batch:02}"))
            .unwrap();
        host.reconcile_editor_draft_imports(CARD, &draft).unwrap();
        let archived = host
            .archive_editor_draft(
                CARD,
                &draft,
                2,
                &request_hash(&saved),
                &format!("global-aux-archive-{batch:02}"),
            )
            .unwrap();
        assert!(!archived.repeated);
        let exported: serde_json::Value =
            serde_json::from_slice(&export_history(&mut host, &draft)).unwrap();
        assert_eq!(
            exported["auxiliary_journals"].as_array().unwrap().len(),
            256
        );
        original_slots.push((draft, imported.operation_id, retired.slot.encode_to_vec()));
        eprintln!(
            "ARCHIVE_AUX_BATCH={} CLOSED_AUXILIARY={}",
            batch + 1,
            (batch + 1) * 256
        );
    }
    let before = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!((before.archived, before.unarchived), (16, 0));
    assert!(before.archived_bytes < 8 * 1024 * 1024);

    let mut next = request("global-aux-overflow-save", 0, "must remain auditable");
    next.draft_id = "global-aux-overflow".into();
    let saved = host.save_editor_draft(&next).unwrap();
    let imported = pending_import_for(&mut host, &next.draft_id, "global-aux-overflow-import");
    let retired = host
        .discard_editor_draft(CARD, &next.draft_id, 1, "global-aux-overflow-retire")
        .unwrap();
    host.reconcile_editor_draft_imports(CARD, &next.draft_id)
        .unwrap();
    let export_before = export_history(&mut host, &next.draft_id);
    assert!(
        host.archive_editor_draft(
            CARD,
            &next.draft_id,
            2,
            &request_hash(&saved),
            "global-aux-overflow-archive"
        )
        .unwrap_err()
        .to_string()
        .contains("draft archive capacity reached")
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, &next.draft_id)
            .unwrap()
            .is_none()
    );
    assert_eq!(export_history(&mut host, &next.draft_id), export_before);
    drop(host);

    let mut host = Workbench::open(&path, Some(package())).unwrap();
    let after = host.list_editor_draft_history("", 32).unwrap();
    assert_eq!(
        (after.archived, after.unarchived, after.archived_bytes),
        (16, 1, before.archived_bytes)
    );
    assert_eq!(
        host.read_editor_draft(CARD, &next.draft_id)
            .unwrap()
            .unwrap()
            .slot
            .encode_to_vec(),
        retired.slot.encode_to_vec()
    );
    assert_eq!(export_history(&mut host, &next.draft_id), export_before);
    assert!(
        host.inspect_editor_draft_archive(CARD, &next.draft_id)
            .unwrap()
            .is_none()
    );
    let retained_import = host
        .inspect_editor_draft_import(CARD, &next.draft_id, &imported.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(retained_import.request, imported);
    assert_eq!(
        retained_import.phase,
        morrow_workbench_host::editor_draft_staging::DraftImportPhase::Retired
    );
    assert!(!retained_import.bytes_retained);
    for (batch, (draft, import_operation, raw)) in original_slots.iter().enumerate() {
        assert_eq!(
            host.read_editor_draft(CARD, draft)
                .unwrap()
                .unwrap()
                .slot
                .encode_to_vec(),
            *raw
        );
        let exported: serde_json::Value =
            serde_json::from_slice(&export_history(&mut host, draft)).unwrap();
        assert_eq!(
            exported["auxiliary_journals"].as_array().unwrap().len(),
            256
        );
        for index in 0..255 {
            assert_eq!(
                host.inspect_editor_draft_import_decision(
                    CARD,
                    draft,
                    1,
                    import_operation,
                    &format!("global-aux-decision-{batch:02}-{index:03}"),
                )
                .unwrap()
                .unwrap()
                .status,
                DecisionStatus::Cancelled
            );
        }
    }
    assert_eq!(business_revision(&host), 2);
    eprintln!(
        "ARCHIVE_AUX_LIMIT_PASS archived_auxiliary=4096 rejected_auxiliary=1 original_decisions=4080 restart_preserved=true"
    );
}

#[test]
#[ignore = "explicit Windows archive capacity qualification"]
fn actual_64_mib_archive_budget_refuses_without_changing_originals() {
    const LIMIT: u64 = 64 * 1024 * 1024;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("workbench.db");
    let mut host = seed(&path);
    let description = "x".repeat(512 * 1024);
    let mut originals = Vec::new();
    let mut rejected = None;
    for index in 0..160 {
        let draft = format!("archive-byte-{index:03}");
        let mut value = request(&format!("archive-byte-save-{index:03}"), 0, "retained");
        value.draft_id = draft.clone();
        value.values.as_mut().unwrap().description = Some(text_value(&description));
        let saved = host.save_editor_draft(&value).unwrap();
        let retired = host
            .discard_editor_draft(CARD, &draft, 1, &format!("archive-byte-retire-{index:03}"))
            .unwrap();
        let raw = retired.slot.encode_to_vec();
        let before = host.list_editor_draft_history("", 1).unwrap();
        match host.archive_editor_draft(
            CARD,
            &draft,
            2,
            &request_hash(&saved),
            &format!("archive-byte-store-{index:03}"),
        ) {
            Ok(receipt) => {
                assert!(!receipt.repeated);
                originals.push((draft, Sha256::digest(&raw).to_vec()));
                let page = host.list_editor_draft_history("", 1).unwrap();
                assert_eq!(page.archived as usize, originals.len());
                assert!(
                    page.archived_bytes > before.archived_bytes && page.archived_bytes <= LIMIT
                );
                if (index + 1) % 16 == 0 {
                    eprintln!(
                        "ARCHIVE_BYTE_BATCH={} charged_bytes={}",
                        index + 1,
                        page.archived_bytes
                    );
                }
            }
            Err(error) => {
                assert!(
                    error.to_string().contains("draft archive capacity reached"),
                    "{error}"
                );
                assert!(before.archived < 4096 && before.archived_bytes > 63 * 1024 * 1024);
                assert!(raw.len() as u64 > LIMIT - before.archived_bytes);
                assert!(
                    host.inspect_editor_draft_archive(CARD, &draft)
                        .unwrap()
                        .is_none()
                );
                assert_eq!(
                    host.read_editor_draft(CARD, &draft)
                        .unwrap()
                        .unwrap()
                        .slot
                        .encode_to_vec(),
                    raw
                );
                rejected = Some((draft, raw, before.archived, before.archived_bytes));
                break;
            }
        }
    }
    let (draft, raw, count, charged) =
        rejected.expect("real configured archive budget must be reached");
    drop(host);
    let host = Workbench::open(&path, None).unwrap();
    let page = host.list_editor_draft_history("", 1).unwrap();
    assert_eq!(
        (page.archived, page.unarchived, page.archived_bytes),
        (count, 1, charged)
    );
    assert!(
        host.inspect_editor_draft_archive(CARD, &draft)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        host.read_editor_draft(CARD, &draft)
            .unwrap()
            .unwrap()
            .slot
            .encode_to_vec(),
        raw
    );
    for (id, original_hash) in originals {
        let saved = host.read_editor_draft(CARD, &id).unwrap().unwrap();
        assert_eq!(
            Sha256::digest(saved.slot.encode_to_vec()).to_vec(),
            original_hash
        );
        assert!(
            host.inspect_editor_draft_archive(CARD, &id)
                .unwrap()
                .is_some()
        );
    }
    assert_eq!(business_revision(&host), 2);
    eprintln!(
        "ARCHIVE_BYTE_LIMIT_PASS configured_bytes={LIMIT} archived={count} charged_bytes={charged} restart_preserved=true"
    );
}
