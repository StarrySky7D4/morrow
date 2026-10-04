#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    changes_metadata::{self, Metadata},
    channel,
    content::CardRecord,
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    store::{ChangesBudget, ChangesStart, Store},
};
use sha2::{Digest, Sha256};
const MODULE: &[u8] = b"\0asm\x01\0\0\0";
fn ids() -> Vec<String> {
    vec!["a".into(), "b".into()]
}
fn card(id: &str) -> CardRecord {
    CardRecord::new(
        id,
        "text",
        1,
        "private title",
        b"private body never exported".to_vec(),
    )
    .unwrap()
}
fn store() -> (tempfile::TempDir, Store) {
    let d = tempfile::tempdir().unwrap();
    let s = Store::open(&d.path().join("db"), Default::default()).unwrap();
    (d, s)
}
fn batch(
    s: &Store,
    budget: ChangesBudget,
) -> morrow_core::Result<morrow_core::store::ChangesBatch> {
    let w = s.open_changes_window([7; 32], &ids(), ChangesStart::Beginning, budget, || Ok(()))?;
    s.materialize_changes_window(w, || Ok(()))
}
fn value() -> Metadata {
    Metadata {
        scope_digest: [1; 32],
        window_id: [2; 32],
        card_id: "a".into(),
        operation_id: "create-a".into(),
        revision: 1,
        card_sha256: [3; 32],
    }
}
#[test]
fn canonical_codec_rejects_every_truncation_and_noncanonical_boundary() {
    assert_eq!(
        format!("{:x}", Sha256::digest(changes_metadata::WIRE_SPEC)),
        "07fc0dc4c48eb17f85309207a441d6a07f3fb8e06aab5e2ce337a4b3c98f9089"
    );
    let v = value();
    let b = v.encode().unwrap();
    assert_eq!(Metadata::decode(&b).unwrap(), v);
    for n in 0..b.len() {
        assert!(Metadata::decode(&b[..n]).is_err());
    }
    for case in 0..9 {
        let mut x = b.clone();
        match case {
            0 => x[0] ^= 1,
            1 => x[8] = 2,
            2 => x[10] ^= 1,
            3 => x[42..74].fill(0),
            4 => x[74..106].fill(0),
            5 => x[106..114].fill(0),
            6 => x[146..148].copy_from_slice(&257u16.to_le_bytes()),
            7 => x.push(0),
            _ => x[150] = 0xff,
        };
        assert!(Metadata::decode(&x).is_err(), "case {case}");
    }
    let mut max = v.clone();
    max.card_id = "a".repeat(256);
    max.operation_id = "b".repeat(256);
    assert_eq!(max.encode().unwrap().len(), 662);
    for s in ["", "a/b", "a\\b", "a:b", "a\n", "a\u{0085}"] {
        let mut x = v.clone();
        x.card_id = s.into();
        assert!(x.encode().is_err());
    }
    let mut expected = Sha256::new();
    expected.update(b"Morrow/changes-metadata/cursor/v1\0");
    expected.update(&b);
    assert_eq!(v.cursor().unwrap(), <[u8; 32]>::from(expected.finalize()));
}
#[test]
fn scope_is_order_independent_length_prefixed_and_exact() {
    let scope = changes_metadata::scope_digest([1; 32], [2; 32], &ids()).unwrap();
    assert_eq!(
        scope,
        changes_metadata::scope_digest([1; 32], [2; 32], &["b".into(), "a".into()]).unwrap()
    );
    for cards in [vec![], vec!["a".into(), "a".into()], vec!["x".into(); 33]] {
        assert!(changes_metadata::scope_digest([1; 32], [2; 32], &cards).is_err());
    }
    assert_ne!(
        scope,
        changes_metadata::scope_digest([2; 32], [2; 32], &ids()).unwrap()
    );
    assert_ne!(
        scope,
        changes_metadata::scope_digest([1; 32], [3; 32], &ids()).unwrap()
    );
    assert_ne!(
        changes_metadata::scope_digest([1; 32], [2; 32], &["ab".into(), "c".into()]).unwrap(),
        changes_metadata::scope_digest([1; 32], [2; 32], &["a".into(), "bc".into()]).unwrap()
    );
    assert!(changes_metadata::scope_digest([0; 32], [2; 32], &ids()).is_err());
}
fn manifest() -> morrow_core::plugin_package::proto::Manifest {
    Package::manifest_for_changes_metadata(
        "test.changes",
        "1.0.0",
        MODULE,
        vec![TransformHandler {
            handler: "changes".into(),
            input_type: "morrow.channel.directory.v1".into(),
            output_type: "bytes".into(),
            max_input_bytes: 4096,
            max_output_bytes: 4096,
        }],
    )
}
#[test]
fn new_feature_has_narrow_admission_and_old_package_semantics_remain() {
    assert!(
        Package::build(manifest(), MODULE)
            .unwrap()
            .is_changes_metadata()
    );
    for case in 0..12 {
        let mut m = manifest();
        match case {
            0 => m.required_features.retain(|f| f != channel::FEATURE),
            1 => m.channel_declaration = None,
            2 => m.guest_abi_version = 1,
            3 => m
                .requested_capabilities
                .push(Capability::ReadContent as i32),
            4 => m.channel_declaration.as_mut().unwrap().kinds = vec![1],
            5 => m.channel_declaration.as_mut().unwrap().kinds = vec![1, 2],
            6 => m.required_features.push("dependencies-v1".into()),
            7 => m.required_features.push("io-v1".into()),
            8 => m.required_features.push("changes-metadata-v2".into()),
            9 => m.required_features.push("mutation-v1".into()),
            10 => m.required_features.push("service-resources-v1".into()),
            _ => m.required_features.push(changes_metadata::FEATURE.into()),
        };
        assert!(Package::build(m, MODULE).is_err(), "case {case}");
    }
    let mut old = manifest();
    old.required_features
        .retain(|f| f != changes_metadata::FEATURE);
    old.requested_capabilities
        .push(Capability::ReadContent as i32);
    let p = Package::build(old, MODULE).unwrap();
    assert!(!p.is_changes_metadata());
}
#[test]
fn permanent_feed_filters_before_payload_and_survives_seal() {
    use morrow_core::{
        audit::{self, SigningKey, TrustedLog},
        records::Record,
    };
    let d = tempfile::tempdir().unwrap();
    let key = SigningKey::from_bytes(&[51; 32]);
    let trust = TrustedLog {
        id: "changes.test".into(),
        key: key.verifying_key(),
    };
    let mut s = Store::open_audited(
        &d.path().join("db"),
        Default::default(),
        true,
        trust.clone(),
    )
    .unwrap();
    let a = s.create_local("create-a", &card("a")).unwrap();
    s.create_local("secret-operation", &card("secret-card"))
        .unwrap();
    s.create_record_local(
        "workspace-operation",
        &Record::workspace("private-workspace", "name").unwrap(),
    )
    .unwrap();
    s.create_local("create-b", &card("b")).unwrap();
    assert_eq!(s.create_local("create-a", &card("a")).unwrap(), a);
    assert!(s.create_local("create-a", &card("different")).is_err());
    let signed = audit::sign(
        &audit::from_pending(&trust, 1, [0; 32], &s.pending(0, 20).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap();
    s.seal_pending(&signed).unwrap();
    assert!(s.pending(0, 20).unwrap().is_empty());
    let mut budget = ChangesBudget::default();
    budget.page_candidates = 1;
    let b = batch(&s, budget).unwrap();
    assert_eq!(b.changes().len(), 2);
    assert_eq!(b.changes()[0].card_sha256, a.content_sha256);
    for m in b.into_metadata([8; 32]).unwrap() {
        let bytes = m.encode().unwrap();
        for secret in [
            b"secret-operation".as_slice(),
            b"secret-card",
            b"private-workspace",
            b"private body",
            b"private title",
        ] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret));
        }
    }
    s.integrity_check().unwrap();
}
#[test]
fn actual_create_rename_edit_attachments_and_migration_emit_receipts_only() {
    use morrow_core::{
        content_change::ContentChange,
        content_migration::ContentMigration,
        dispatch::HostRuntime,
        lifecycle::GrantKind,
        response::{Outcome, Response},
        runtime::RenameRequest,
    };
    let (_d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let mut h = HostRuntime::new(s).unwrap();
    let mut c = h.connect().unwrap();
    h.grant(&mut c, GrantKind::Rename, "a", 1000, 0).unwrap();
    h.grant(&mut c, GrantKind::EditContent, "a", 1000, 0)
        .unwrap();
    let r = RenameRequest {
        operation_id: "rename".into(),
        card_id: "a".into(),
        expected_revision: 1,
        title: "new".into(),
    };
    let response = Response::decode(&h.dispatch(&c, &r.encode().unwrap(), || 1).unwrap()).unwrap();
    assert!(!matches!(response.outcome, Outcome::Rejected(_)));
    let edit = ContentChange {
        operation_id: "edit".into(),
        card_id: "a".into(),
        expected_revision: 2,
        title: "edited".into(),
        body: vec![9],
        preview_text: "preview".into(),
        attachments: None,
    };
    h.edit_content(&c, &edit, || 2).unwrap();
    h.edit_content(&c, &edit, || 2).unwrap();
    h.store_local_mut()
        .set_attachments_local("attachments", "a", 3, &[])
        .unwrap();
    let migration = ContentMigration {
        operation_id: "migrate".into(),
        source_card: h.store_local().card("a").unwrap().unwrap().encode(),
        target_format_version: 2,
        body: vec![8],
        preview_text: "migrated".into(),
    };
    h.migrate_content_guarded_with_evidence(&c, &migration, &[], || 3, |_| Ok(()))
        .unwrap();
    let notifications = batch(h.store_local(), Default::default())
        .unwrap()
        .into_metadata([4; 32])
        .unwrap();
    assert_eq!(
        notifications.iter().map(|m| m.revision).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
    assert_eq!(
        notifications.last().unwrap().card_sha256,
        <[u8; 32]>::from(Sha256::digest(
            h.store_local().card("a").unwrap().unwrap().encode()
        ))
    );
    assert_ne!(
        notifications.last().unwrap().card_sha256,
        <[u8; 32]>::from(Sha256::digest([8]))
    );
}
#[test]
fn final_commit_rejection_has_no_permanent_notification() {
    use morrow_core::{content_change::ContentChange, dispatch::HostRuntime, lifecycle::GrantKind};
    let (_d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let mut h = HostRuntime::new(s).unwrap();
    let mut c = h.connect().unwrap();
    h.grant(&mut c, GrantKind::EditContent, "a", 10, 0).unwrap();
    let edit = ContentChange {
        operation_id: "rolled-back".into(),
        card_id: "a".into(),
        expected_revision: 1,
        title: "bad".into(),
        body: vec![9],
        preview_text: "bad".into(),
        attachments: None,
    };
    let mut calls = 0;
    assert!(
        h.edit_content(&c, &edit, || {
            calls += 1;
            if calls == 2 { 10 } else { 1 }
        })
        .is_err()
    );
    assert_eq!(
        batch(h.store_local(), Default::default())
            .unwrap()
            .changes()
            .len(),
        1
    );
    h.store_local().integrity_check().unwrap();
}
#[test]
fn fixed_upper_owned_batch_foreign_store_and_no_transaction_survives() {
    let (d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    s.create_local("create-b", &card("b")).unwrap();
    let b = s.materialize_changes_window(w, || Ok(())).unwrap();
    assert_eq!(b.changes().len(), 1);
    s.validate_changes_batch(&b, [7; 32], &ids()).unwrap();
    assert!(s.validate_changes_batch(&b, [8; 32], &ids()).is_err());
    assert!(
        s.validate_changes_batch(&b, [7; 32], &["a".into()])
            .is_err()
    );
    let other = Store::open_existing(&d.path().join("db"), Default::default()).unwrap();
    assert!(other.validate_changes_batch(&b, [7; 32], &ids()).is_err());
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    assert!(other.materialize_changes_window(w, || Ok(())).is_err());
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    db.execute_batch("BEGIN IMMEDIATE; ROLLBACK;").unwrap();
    assert_eq!(b.changes().len(), 1);
}
#[test]
fn bounded_candidate_pages_empty_selection_and_output_limits() {
    let (_d, mut s) = store();
    for i in 0..70 {
        s.create_local(&format!("secret-op-{i}"), &card(&format!("secret-{i}")))
            .unwrap();
    }
    let mut b = ChangesBudget::default();
    b.page_candidates = 3;
    assert!(batch(&s, b).unwrap().changes().is_empty());
    b.max_candidates = 69;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    b.max_candidates = 71;
    s.create_local("create-a", &card("a")).unwrap();
    assert_eq!(batch(&s, b).unwrap().changes().len(), 1);
    b.max_metadata_bytes = 150;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    let (_d, mut many) = store();
    for i in 0..65 {
        many.create_local(&format!("create-{i}"), &card(&format!("c-{i}")))
            .unwrap();
    }
    // A fixed set is also bounded, before any SQL read.
    let cards = (0..65).map(|i| format!("c-{i}")).collect::<Vec<_>>();
    assert!(
        many.open_changes_window(
            [7; 32],
            &cards,
            ChangesStart::Beginning,
            Default::default(),
            || Ok(())
        )
        .is_err()
    );
}
#[test]
fn compressed_commits_charge_declared_raw_and_per_page_aggregate() {
    let (_d, mut s) = store();
    for id in ["a", "b"] {
        let c = CardRecord::new(id, "text", 1, "title", vec![0; 2 * 1024 * 1024]).unwrap();
        s.create_local(&format!("create-{id}"), &c).unwrap();
    }
    let mut b = ChangesBudget::default();
    b.max_single_decoded_bytes = 1024 * 1024;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    b = ChangesBudget::default();
    b.max_decoded_bytes = 3 * 1024 * 1024;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    b = ChangesBudget::default();
    b.max_page_decoded_bytes = 3 * 1024 * 1024;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    b.page_candidates = 1;
    assert_eq!(batch(&s, b).unwrap().changes().len(), 2);
    b = ChangesBudget::default();
    b.max_container_bytes = 1;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
}
#[test]
fn malformed_and_large_unselected_payloads_are_not_loaded() {
    let (d, mut s) = store();
    s.create_local("secret-op", &card("secret")).unwrap();
    s.create_local("create-a", &card("a")).unwrap();
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    db.execute(
        "UPDATE operations SET payload=zeroblob(20000000) WHERE id='secret-op'",
        [],
    )
    .unwrap();
    assert_eq!(batch(&s, Default::default()).unwrap().changes().len(), 1);
    db.execute(
        "UPDATE operations SET payload=zeroblob(20000000) WHERE id='create-a'",
        [],
    )
    .unwrap();
    assert!(batch(&s, Default::default()).is_err());
}
#[test]
fn verified_anchor_reopens_new_window_and_rejects_substitution() {
    let (d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let anchor = batch(&s, Default::default())
        .unwrap()
        .into_metadata([8; 32])
        .unwrap()
        .remove(0);
    s.create_local("create-b", &card("b")).unwrap();
    drop(s);
    let s = Store::open_existing(&d.path().join("db"), Default::default()).unwrap();
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::After(anchor.clone()),
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    let b = s.materialize_changes_window(w, || Ok(())).unwrap();
    assert_eq!(b.changes()[0].card_id, "b");
    assert_eq!(b.into_metadata([9; 32]).unwrap()[0].window_id, [9; 32]);
    for case in 0..5 {
        let mut a = anchor.clone();
        match case {
            0 => a.scope_digest[0] ^= 1,
            1 => a.card_id = "secret".into(),
            2 => a.operation_id = "missing".into(),
            3 => a.revision += 1,
            _ => a.card_sha256[0] ^= 1,
        };
        assert!(
            s.open_changes_window(
                [7; 32],
                &ids(),
                ChangesStart::After(a),
                Default::default(),
                || Ok(())
            )
            .is_err()
        );
    }
    let (_other_d, other) = store();
    assert!(
        other
            .open_changes_window(
                [7; 32],
                &ids(),
                ChangesStart::After(anchor),
                Default::default(),
                || Ok(())
            )
            .is_err()
    );
}
#[test]
fn authority_and_original_deadline_are_checked_before_release() {
    let (_d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    assert!(
        s.open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Err(Error::Integrity)
        )
        .is_err()
    );
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    let mut calls = 0;
    assert!(
        s.materialize_changes_window(w, || {
            calls += 1;
            if calls == 4 {
                Err(Error::Integrity)
            } else {
                Ok(())
            }
        })
        .is_err()
    );
    let mut budget = ChangesBudget::default();
    budget.max_duration_ms = 50;
    let b = batch(&s, budget).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(70));
    assert!(s.validate_changes_batch(&b, [7; 32], &ids()).is_err());
    assert!(b.into_metadata([9; 32]).is_err());
    let w = s
        .open_changes_window([7; 32], &ids(), ChangesStart::Beginning, budget, || Ok(()))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(70));
    assert!(s.materialize_changes_window(w, || Ok(())).is_err());
}
#[test]
fn receiver_probe_is_foreign_safe_read_only_and_does_not_revive() {
    use morrow_core::dispatch::HostRuntime;
    let (_d, s) = store();
    let mut host = HostRuntime::new(s).unwrap();
    let c = host.connect().unwrap();
    let probe = host.changes_receiver_liveness(&c).unwrap();
    probe.check().unwrap();
    let (_e, t) = store();
    let other = HostRuntime::new(t).unwrap();
    assert!(other.changes_receiver_liveness(&c).is_err());
    host.disconnect(&c).unwrap();
    assert!(probe.check().is_err());
    assert!(host.changes_receiver_liveness(&c).is_err());
    let replacement = host.connect().unwrap();
    host.changes_receiver_liveness(&replacement)
        .unwrap()
        .check()
        .unwrap();
    assert!(probe.check().is_err());
    let live = host.changes_receiver_liveness(&replacement).unwrap();
    drop(host);
    assert!(live.check().is_err());
}

#[test]
fn shared_sdk_vectors_match_core_payload_and_frame_correlation() {
    let data = include_bytes!("../../extensions/changes-metadata-v1/tests/vectors.bin");
    let mut pos = 0;
    let mut cases = 0;
    let mut accepted = 0;
    while pos < data.len() {
        let expected = data[pos] != 0;
        pos += 1;
        let epoch: &[u8] = &data[pos..pos + 32];
        pos += 32;
        let cursor_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let cursor = &data[pos..pos + 32];
        pos += 32;
        let size = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let bytes = &data[pos..pos + size];
        pos += size;
        let actual = Metadata::decode(bytes).is_ok_and(|v| {
            v.window_id == epoch && cursor_len == 32 && v.cursor().is_ok_and(|c| c == cursor)
        });
        assert_eq!(actual, expected, "case {cases}");
        cases += 1;
        accepted += usize::from(actual);
    }
    assert_eq!((cases, accepted), (209, 6));
}
#[test]
fn hard_budget_ceilings_and_retained_event_limit_never_truncate_success() {
    for case in 0..10 {
        let mut b = ChangesBudget::default();
        match case {
            0 => b.max_candidates = 4097,
            1 => b.page_candidates = 65,
            2 => b.max_events = 65,
            3 => b.max_metadata_bytes = 65537,
            4 => b.max_single_decoded_bytes = 16 * 1024 * 1024 + 1,
            5 => b.max_decoded_bytes = 32 * 1024 * 1024 + 1,
            6 => b.max_container_bytes = 32 * 1024 * 1024 + 1,
            7 => b.max_page_decoded_bytes = 32 * 1024 * 1024 + 1,
            8 => b.max_page_container_bytes = 32 * 1024 * 1024 + 1,
            _ => b.max_duration_ms = 60_001,
        };
        assert_eq!(b.validate(), Err(Error::Limit));
    }
    let (_d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    s.set_attachments_local("next", "a", 1, &[]).unwrap();
    let mut b = ChangesBudget::default();
    b.max_events = 1;
    assert!(matches!(batch(&s, b), Err(Error::Limit)));
    b.max_events = 2;
    assert_eq!(batch(&s, b).unwrap().changes().len(), 2);
}
#[test]
fn corrupt_declared_length_and_missing_permanent_anchor_fail_closed() {
    let (d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let anchor = batch(&s, Default::default())
        .unwrap()
        .into_metadata([8; 32])
        .unwrap()
        .remove(0);
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    let mut bytes: Vec<u8> = db
        .query_row(
            "SELECT payload FROM operations WHERE id='create-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    bytes[10..14].copy_from_slice(&u32::MAX.to_le_bytes());
    db.execute(
        "UPDATE operations SET payload=?1 WHERE id='create-a'",
        [bytes],
    )
    .unwrap();
    assert!(matches!(batch(&s, Default::default()), Err(Error::Limit)));
    db.execute("DELETE FROM operation_events WHERE id='create-a'", [])
        .unwrap();
    assert!(matches!(
        s.open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::After(anchor),
            Default::default(),
            || Ok(())
        ),
        Err(Error::NotFound)
    ));
}

#[test]
fn live_store_binding_rejects_replacement_and_batches_do_not_keep_owner_alive() {
    let (d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let binding = s.changes_store_binding().unwrap();
    binding.check_live().unwrap();
    s.validate_changes_store_binding(&binding).unwrap();
    let b = batch(&s, Default::default()).unwrap();
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    let second = Store::open_existing(&d.path().join("db"), Default::default()).unwrap();
    let original = std::mem::replace(&mut s, second);
    binding.check_live().unwrap();
    assert!(s.validate_changes_store_binding(&binding).is_err());
    original.validate_changes_store_binding(&binding).unwrap();
    drop(original);
    assert!(binding.check_live().is_err());
    assert!(s.validate_changes_store_binding(&binding).is_err());
    // Owned windows/batches retain a separate snapshot identity, never owner life.
    assert_eq!(b.changes().len(), 1);
    assert!(b.into_metadata([9; 32]).is_err());
    assert!(s.materialize_changes_window(w, || Ok(())).is_err());
    let fresh = s.changes_store_binding().unwrap();
    s.validate_changes_store_binding(&fresh).unwrap();
    assert!(binding.check_live().is_err());
}
#[test]
fn authority_is_rechecked_after_fetch_before_historical_decompression() {
    let (d, mut s) = store();
    s.create_local("create-a", &card("a")).unwrap();
    let db = rusqlite::Connection::open(d.path().join("db")).unwrap();
    let mut bytes: Vec<u8> = db
        .query_row(
            "SELECT payload FROM operations WHERE id='create-a'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    *bytes.last_mut().unwrap() ^= 0xff;
    db.execute(
        "UPDATE operations SET payload=?1 WHERE id='create-a'",
        [bytes],
    )
    .unwrap();
    let w = s
        .open_changes_window(
            [7; 32],
            &ids(),
            ChangesStart::Beginning,
            Default::default(),
            || Ok(()),
        )
        .unwrap();
    let mut calls = 0;
    let result = s.materialize_changes_window(w, || {
        calls += 1;
        if calls == 6 {
            Err(Error::OperationConflict)
        } else {
            Ok(())
        }
    });
    // A missing pre-decode boundary would instead decompress and fail integrity.
    assert!(matches!(result, Err(Error::OperationConflict)));
    assert_eq!(calls, 6);
}
