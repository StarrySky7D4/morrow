//! Host-owned outbound authority codec and original Store qualification.
use morrow_core::{
    Error, io,
    outbound_authority::{self, Record, proto},
};
use prost::Message;
use sha2::{Digest, Sha256};
fn credential() -> proto::Record {
    proto::Record {
        schema_version: 1,
        reference: vec![1; 32],
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(proto::record::Kind::Credential(proto::Credential {
            provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
            ciphertext: vec![42; 64],
        })),
    }
}
fn endpoint() -> proto::Record {
    let mut v = credential();
    v.reference = vec![2; 32];
    v.kind = Some(proto::record::Kind::Endpoint(proto::Endpoint {
        package_id: "test.plugin".into(),
        package_sha256: vec![3; 32],
        origin: "https://api.example".into(),
        profile: 1,
        methods: vec!["GET".into(), "POST".into()],
        credential_reference: vec![1; 32],
        root_certificate: vec![],
        max_request_bytes: 1024,
        max_response_bytes: 2048,
        max_header_bytes: 4096,
        max_concurrent: 4,
        timeout_ms: 1000,
        max_frame_bytes: 8192,
    }));
    v
}
fn pack(raw: &[u8]) -> Vec<u8> {
    let compressed = lz4_flex::block::compress(raw);
    let mut bytes = outbound_authority::MAGIC.to_vec();
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(compressed.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&Sha256::digest(raw));
    bytes.extend(compressed);
    bytes
}
#[test]
fn roundtrip_lifetime_and_ciphertext_only_provider_record() {
    for value in [credential(), endpoint()] {
        let record = Record::encode(value.clone()).unwrap();
        let decoded = Record::decode(record.container()).unwrap();
        assert_eq!(decoded.value(), &value);
        assert_eq!(decoded.reference().as_slice(), value.reference);
        assert_eq!(decoded.container(), record.container());
        assert!(decoded.check_time(9).is_err());
        decoded.check_time(10).unwrap();
        decoded.check_time(1009).unwrap();
        assert!(decoded.check_time(1010).is_err());
        let mut disabled = value;
        disabled.disabled = true;
        assert!(Record::encode(disabled).unwrap().check_time(10).is_err());
    }
}

#[test]
fn canonical_digest_binds_policy_and_protected_credential_even_without_revision_change() {
    for original in [credential(), endpoint()] {
        let record = Record::encode(original.clone()).unwrap();
        let digest = record.canonical_digest();
        assert_eq!(
            Record::decode(record.container())
                .unwrap()
                .canonical_digest(),
            digest
        );
        let mut mutations = Vec::new();
        let mut changed = original.clone();
        changed.created_ms += 1;
        mutations.push(changed);
        let mut changed = original.clone();
        changed.expires_ms += 1;
        mutations.push(changed);
        let mut changed = original.clone();
        changed.disabled = true;
        mutations.push(changed);
        match original.kind.as_ref().unwrap() {
            proto::record::Kind::Credential(_) => {
                let mut changed = original.clone();
                let Some(proto::record::Kind::Credential(value)) = changed.kind.as_mut() else {
                    unreachable!()
                };
                value.ciphertext.push(43);
                mutations.push(changed);
            }
            proto::record::Kind::Endpoint(_) => {
                for field in 0..5 {
                    let mut changed = original.clone();
                    let Some(proto::record::Kind::Endpoint(value)) = changed.kind.as_mut() else {
                        unreachable!()
                    };
                    match field {
                        0 => value.origin = "https://other.example".into(),
                        1 => value.methods = vec!["GET".into()],
                        2 => value.credential_reference = vec![4; 32],
                        3 => value.package_sha256 = vec![5; 32],
                        _ => value.max_response_bytes += 1,
                    }
                    mutations.push(changed);
                }
            }
        }
        for changed in mutations {
            assert_eq!(changed.revision, original.revision);
            assert_eq!(changed.reference, original.reference);
            assert_ne!(Record::encode(changed).unwrap().canonical_digest(), digest);
        }
    }
}
#[test]
fn strict_record_and_credential_bounds_reject_before_decode_allocation() {
    for case in 0..12 {
        let mut value = credential();
        match case {
            0 => value.schema_version = 2,
            1 => value.reference = vec![0; 32],
            2 => value.revision = 0,
            3 => value.revision = u64::MAX,
            4 => value.created_ms = 0,
            5 => value.expires_ms = 10,
            6 => value.expires_ms = 9,
            7 => value.expires_ms = value.created_ms + outbound_authority::MAX_LIFETIME_MS + 1,
            8 => value.kind = None,
            _ => {
                if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
                    match case {
                        9 => c.provider = "plaintext".into(),
                        10 => c.ciphertext.clear(),
                        _ => c.ciphertext = vec![1; outbound_authority::MAX_CIPHERTEXT_BYTES + 1],
                    }
                }
            }
        }
        assert!(Record::encode(value.clone()).is_err(), "{case}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{case}"
        );
    }
}
#[test]
fn endpoint_limits_methods_and_references_are_strict() {
    for case in 0..24 {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            match case {
                0 => e.package_id = "bad/id".into(),
                1 => e.package_sha256 = vec![0; 32],
                2 => e.profile = 0,
                3 => e.profile = 4,
                4 => e.methods.clear(),
                5 => e.methods = vec!["GET".into(); 17],
                6 => e.methods = vec!["GET".into(), "GET".into()],
                7 => e.methods = vec!["POST".into(), "GET".into()],
                8 => e.methods = vec!["get".into()],
                9 => e.methods = vec!["X".repeat(33)],
                10 => e.credential_reference = vec![0; 32],
                11 => e.credential_reference = vec![1; 31],
                12 => e.root_certificate = vec![1; outbound_authority::MAX_CERTIFICATE_BYTES + 1],
                13 => e.max_request_bytes = io::MAX_PAYLOAD_BYTES as u64 + 1,
                14 => e.max_response_bytes = io::MAX_PAYLOAD_BYTES as u64 + 1,
                15 => e.max_header_bytes = io::MAX_HEADER_BYTES as u64 + 1,
                16 => e.max_concurrent = 129,
                17 => e.max_concurrent = 0,
                18 => e.timeout_ms = io::MAX_SUBMIT_DEADLINE_MS + 1,
                19 => e.timeout_ms = 0,
                20 => e.max_frame_bytes = io::MAX_FRAME_BYTES as u64 + 1,
                21 => e.max_frame_bytes = 0,
                22 => e.max_request_bytes = 0,
                _ => e.max_header_bytes = 0,
            }
        }
        assert!(Record::encode(value.clone()).is_err(), "{case}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{case}"
        );
    }
}
#[test]
fn lexical_origin_rejects_credentials_paths_queries_and_noncanonical_ports() {
    for origin in [
        "HTTPS://api.example",
        "https://api.example/",
        "https://api.example?q=1",
        "https://api.example#x",
        "https://user:secret@api.example",
        "https://api.example:443",
        "https://api.example:0443",
        "https://api.example:0",
        "https://api.example:65536",
        "https://API.example",
        "https://api..example",
        "https://[0:0:0:0:0:0:0:1]",
        "https://api.example\\bad",
        "http://api.example",
    ] {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            e.origin = origin.into();
        }
        assert!(Record::encode(value.clone()).is_err(), "{origin}");
        assert!(
            Record::decode(&pack(&value.encode_to_vec())).is_err(),
            "{origin}"
        );
    }
    for (origin, profile) in [
        ("https://api.example", 1),
        ("https://127.0.0.1:8443", 3),
        ("http://127.0.0.1:8080", 2),
        ("https://[::1]:8443", 3),
    ] {
        let mut value = endpoint();
        if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
            e.origin = origin.into();
            e.profile = profile;
        }
        Record::encode(value).unwrap();
    }
}
#[test]
fn duplicate_oneof_unknown_and_noncanonical_wire_fields_fail_closed() {
    for suffix in [
        vec![0x48, 1],
        vec![0x18, 1],
        vec![0x30, 0],
        vec![0x30, 2],
        vec![0x3a, 0],
        vec![0x42, 0],
        vec![0x08, 0xff, 0xff, 0xff, 0xff, 0x1f],
    ] {
        let mut raw = credential().encode_to_vec();
        raw.extend(suffix);
        assert!(Record::decode(&pack(&raw)).is_err());
    }
}
#[test]
fn endpoint_and_ciphertext_hard_maximums_roundtrip() {
    let mut value = credential();
    if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
        c.ciphertext = vec![42; outbound_authority::MAX_CIPHERTEXT_BYTES];
    }
    let record = Record::encode(value).unwrap();
    Record::decode(record.container()).unwrap();
    let mut value = endpoint();
    if let Some(proto::record::Kind::Endpoint(e)) = &mut value.kind {
        e.root_certificate = vec![42; outbound_authority::MAX_CERTIFICATE_BYTES];
        e.max_request_bytes = io::MAX_PAYLOAD_BYTES as u64;
        e.max_response_bytes = io::MAX_PAYLOAD_BYTES as u64;
        e.max_header_bytes = io::MAX_HEADER_BYTES as u64;
        e.max_frame_bytes = io::MAX_FRAME_BYTES as u64;
        e.max_concurrent = 128;
        e.timeout_ms = io::MAX_SUBMIT_DEADLINE_MS;
    }
    let record = Record::encode(value).unwrap();
    Record::decode(record.container()).unwrap();
}
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use morrow_core::{
        content::CardRecord,
        service_authority::{Record as Inbound, proto as inbound},
        store::{EventBudget, SCHEMA_VERSION, Store},
    };
    fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outbound.db");
        let store = Store::open(&path, Default::default()).unwrap();
        (dir, path, store)
    }
    fn inbound() -> Inbound {
        Inbound::encode(inbound::Record {
            schema_version: 1,
            reference: vec![7; 32],
            revision: 1,
            created_ms: 10,
            expires_ms: 1010,
            disabled: false,
            kind: Some(inbound::record::Kind::Authentication(
                inbound::Authentication {
                    principal_id: "alice".into(),
                    token_sha256: vec![8; 32],
                },
            )),
        })
        .unwrap()
    }
    #[test]
    fn cas_reopen_and_immutable_kind_package_identity() {
        let (_dir, path, mut store) = setup();
        let first = Record::encode(endpoint()).unwrap();
        store.save_outbound_authority_local(&first, 0).unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&first, 0),
            Err(Error::RevisionConflict)
        );
        let mut bad = endpoint();
        bad.revision = 2;
        if let Some(proto::record::Kind::Endpoint(e)) = &mut bad.kind {
            e.package_id = "other.plugin".into();
        }
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(bad).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        let mut bad = credential();
        bad.reference = vec![2; 32];
        bad.revision = 2;
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(bad).unwrap(), 1),
            Err(Error::OperationConflict)
        );
        let mut value = endpoint();
        value.revision = 2;
        value.disabled = true;
        let next = Record::encode(value).unwrap();
        store.save_outbound_authority_local(&next, 1).unwrap();
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        drop(store);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .load_outbound_authority(&[2; 32])
                .unwrap()
                .unwrap()
                .value(),
            next.value()
        );
        store.integrity_check().unwrap();
    }
    #[test]
    fn original_pin_blocks_foreign_outbound_writer_and_all_mutations_revoke() {
        let (_dir, path, mut store) = setup();
        store
            .save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0)
            .unwrap();
        let lease = store.pin_service_authority().unwrap();
        let mut other = Store::open_existing(&path, Default::default()).unwrap();
        let mut value = credential();
        value.revision = 2;
        value.disabled = true;
        let update = Record::encode(value).unwrap();
        assert_eq!(
            other.save_outbound_authority_local(&update, 1),
            Err(Error::StorageBusy)
        );
        lease.check().unwrap();
        store.save_outbound_authority_local(&update, 1).unwrap();
        assert!(lease.check().is_err());
        let new = store.pin_service_authority().unwrap();
        store.save_service_authority_local(&inbound(), 0).unwrap();
        assert!(new.check().is_err());
        drop(store);
        other
            .save_outbound_authority_local(&Record::encode(endpoint()).unwrap(), 0)
            .unwrap();
    }
    #[test]
    fn shared_quota_rollback_and_replacement_count_once() {
        let (_dir, path, store) = setup();
        drop(store);
        let original = Record::encode(credential()).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: original.container().len() as u64,
            },
        )
        .unwrap();
        store.save_outbound_authority_local(&original, 0).unwrap();
        let lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_service_authority_local(&inbound(), 0),
            Err(Error::EventCapacity)
        );
        assert!(lease.check().is_err());
        assert_eq!(
            store
                .create_local(
                    "create",
                    &CardRecord::new("one", "test.card", 1, "", b"body".to_vec()).unwrap()
                )
                .err(),
            Some(Error::EventCapacity)
        );
        let mut value = credential();
        value.revision = 2;
        if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
            c.ciphertext = (0..256).map(|i| i as u8).collect();
        }
        let expanded = Record::encode(value).unwrap();
        let lease = store.pin_service_authority().unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&expanded, 1),
            Err(Error::EventCapacity)
        );
        assert!(lease.check().is_err());
        assert_eq!(
            store
                .load_outbound_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .container(),
            original.container()
        );
        drop(store);
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: expanded.container().len() as u64,
            },
        )
        .unwrap();
        store.save_outbound_authority_local(&expanded, 1).unwrap();
        store.integrity_check().unwrap();
    }

    #[test]
    fn scoped_writes_revoke_dependents_but_preserve_unrelated_and_early_cas_conflicts() {
        use morrow_core::store::ServiceAuthorityResource as Resource;
        let (_dir, _path, mut store) = setup();
        store
            .save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0)
            .unwrap();
        store
            .save_outbound_authority_local(&Record::encode(endpoint()).unwrap(), 0)
            .unwrap();
        store.save_service_authority_local(&inbound(), 0).unwrap();
        let guard = store.pin_service_authority().unwrap();
        let endpoint_lease = store
            .narrow_service_authority(
                &guard,
                &[Resource::Outbound([2; 32]), Resource::Outbound([1; 32])],
            )
            .unwrap();
        let credential_lease = store
            .narrow_service_authority(&guard, &[Resource::Outbound([1; 32])])
            .unwrap();
        let unrelated = store
            .narrow_service_authority(&guard, &[Resource::Inbound(inbound().reference())])
            .unwrap();
        let mut value = endpoint();
        value.revision = 2;
        value.disabled = true;
        let updated = Record::encode(value).unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&updated, 0),
            Err(Error::RevisionConflict)
        );
        endpoint_lease.check().unwrap();
        credential_lease.check().unwrap();
        guard.check().unwrap();
        store.save_outbound_authority_local(&updated, 1).unwrap();
        assert!(endpoint_lease.check().is_err());
        assert!(guard.check().is_err());
        credential_lease.check().unwrap();
        unrelated.check().unwrap();
        let mut value = credential();
        value.revision = 2;
        store
            .save_outbound_authority_local(&Record::encode(value).unwrap(), 1)
            .unwrap();
        assert!(credential_lease.check().is_err());
        unrelated.check().unwrap();
        store.service_authority_control().revoke_all();
        assert!(unrelated.check().is_err());
    }

    #[test]
    fn scoped_quota_rollback_never_revives_affected_epoch_or_revokes_unrelated_epoch() {
        use morrow_core::store::ServiceAuthorityResource as Resource;
        let (_dir, path, store) = setup();
        drop(store);
        let original = Record::encode(credential()).unwrap();
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: original.container().len() as u64,
            },
        )
        .unwrap();
        store.save_outbound_authority_local(&original, 0).unwrap();
        let guard = store.pin_service_authority().unwrap();
        let affected = store
            .narrow_service_authority(&guard, &[Resource::Outbound([1; 32])])
            .unwrap();
        let unrelated = store
            .narrow_service_authority(&guard, &[Resource::Outbound([2; 32])])
            .unwrap();
        let mut value = credential();
        value.revision = 2;
        if let Some(proto::record::Kind::Credential(c)) = &mut value.kind {
            c.ciphertext = (0..256).map(|i| i as u8).collect();
        }
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(value).unwrap(), 1),
            Err(Error::EventCapacity)
        );
        assert!(affected.check().is_err());
        unrelated.check().unwrap();
        assert_eq!(
            store
                .load_outbound_authority(&[1; 32])
                .unwrap()
                .unwrap()
                .value()
                .revision,
            1
        );
        let fresh = store.pin_service_authority().unwrap();
        store
            .narrow_service_authority(&fresh, &[Resource::Outbound([1; 32])])
            .unwrap()
            .check()
            .unwrap();
        assert!(affected.check().is_err());
    }
    #[test]
    fn v19_migration_preserves_inbound_identity_and_event_bytes() {
        let (_dir, path, mut store) = setup();
        let inbound = inbound();
        store.save_service_authority_local(&inbound, 0).unwrap();
        store
            .create_local(
                "create",
                &CardRecord::new("one", "test.card", 1, "", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let before = store.pending(0, 128).unwrap();
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        let identity: Vec<u8> = sql
            .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                r.get(0)
            })
            .unwrap();
        sql.execute_batch("PRAGMA user_version=19;").unwrap();
        drop(sql);
        assert!(matches!(
            Store::open_existing(&path, Default::default()),
            Err(Error::Integrity)
        ));
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE IF EXISTS tls_identities; DROP TABLE outbound_authorities;")
            .unwrap();
        drop(sql);
        let store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(store.pending(0, 128).unwrap(), before);
        assert_eq!(
            store
                .load_service_authority(&[7; 32])
                .unwrap()
                .unwrap()
                .container(),
            inbound.container()
        );
        assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        let sql = rusqlite::Connection::open(&path).unwrap();
        let after: Vec<u8> = sql
            .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(identity, after);
        assert_eq!(
            sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
    }
    fn seed_list(store: &mut Store, count: u8) {
        for index in (1..=count).rev() {
            let mut value = if index % 2 == 0 {
                endpoint()
            } else {
                credential()
            };
            value.reference = vec![index; 32];
            store
                .save_outbound_authority_local(&Record::encode(value).unwrap(), 0)
                .unwrap();
        }
    }
    #[test]
    fn authority_listing_empty_reopens_without_writes_or_implicit_pin() {
        let (_dir, path, mut store) = setup();
        let before = store.pending_usage().unwrap();
        let page = store
            .list_outbound_authorities_local(None, None, 16)
            .unwrap();
        assert!(page.records.is_empty());
        assert!(page.next.is_none());
        assert_ne!(page.snapshot, [0; 32]);
        assert_eq!(store.pending_usage().unwrap(), before);
        // A listing must not retain the authority lock or issue a live lease.
        let mut other = Store::open_existing(&path, Default::default()).unwrap();
        let lease = other.pin_service_authority().unwrap();
        let repeated = store
            .list_outbound_authorities_local(None, Some(page.snapshot), 1)
            .unwrap();
        assert_eq!(repeated.snapshot, page.snapshot);
        lease.check().unwrap();
        drop(other);
        drop(store);
        let mut store = Store::open_existing(&path, Default::default()).unwrap();
        assert_eq!(
            store
                .list_outbound_authorities_local(None, None, 16)
                .unwrap()
                .snapshot,
            page.snapshot
        );
    }
    #[test]
    fn authority_listing_orders_both_kinds_and_pages_exact_original_containers() {
        let (_dir, path, mut store) = setup();
        seed_list(&mut store, 33);
        let originals: Vec<Vec<u8>> = (1..=33)
            .map(|index| {
                store
                    .load_outbound_authority(&[index; 32])
                    .unwrap()
                    .unwrap()
                    .container()
                    .to_vec()
            })
            .collect();
        let lease = store.pin_service_authority().unwrap();
        let first = store
            .list_outbound_authorities_local(None, None, 16)
            .unwrap();
        assert_eq!(first.records.len(), 16);
        assert_eq!(first.next, Some([16; 32]));
        let second = store
            .list_outbound_authorities_local(first.next, Some(first.snapshot), 16)
            .unwrap();
        assert_eq!(second.records.len(), 16);
        assert_eq!(second.next, Some([32; 32]));
        assert_eq!(second.snapshot, first.snapshot);
        let third = store
            .list_outbound_authorities_local(second.next, Some(second.snapshot), 16)
            .unwrap();
        assert_eq!(third.records.len(), 1);
        assert!(third.next.is_none());
        assert_eq!(third.snapshot, first.snapshot);
        let mut records = first.records;
        records.extend(second.records);
        records.extend(third.records);
        for (index, record) in records.iter().enumerate() {
            assert_eq!(record.reference(), [index as u8 + 1; 32]);
            assert_eq!(record.container(), originals[index]);
        }
        lease.check().unwrap();
        assert_eq!(store.pending_usage().unwrap(), (0, 0));
        drop(store);
        let mut reopened = Store::open_existing(&path, Default::default()).unwrap();
        let end = reopened
            .list_outbound_authorities_local(Some([33; 32]), Some(first.snapshot), 16)
            .unwrap();
        assert!(end.records.is_empty());
        assert!(end.next.is_none());
        assert_eq!(end.snapshot, first.snapshot);
    }
    #[test]
    fn authority_listing_rejects_missing_snapshot_unknown_cursor_and_bad_limits() {
        let (_dir, _path, mut store) = setup();
        seed_list(&mut store, 2);
        for limit in [0, 17, u32::MAX] {
            assert!(matches!(
                store.list_outbound_authorities_local(None, None, limit),
                Err(Error::Limit)
            ));
        }
        assert!(matches!(
            store.list_outbound_authorities_local(Some([1; 32]), None, 1),
            Err(Error::Invalid(_))
        ));
        let page = store
            .list_outbound_authorities_local(None, None, 1)
            .unwrap();
        assert!(matches!(
            store.list_outbound_authorities_local(Some([99; 32]), Some(page.snapshot), 1),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            store.list_outbound_authorities_local(Some([0; 32]), Some(page.snapshot), 1),
            Err(Error::Invalid(_))
        ));
        assert!(matches!(
            store.list_outbound_authorities_local(None, Some([0; 32]), 1),
            Err(Error::RevisionConflict)
        ));
    }
    #[test]
    fn authority_listing_detects_revision_addition_and_full_container_drift() {
        let (_dir, path, mut store) = setup();
        seed_list(&mut store, 3);
        let first = store
            .list_outbound_authorities_local(None, None, 1)
            .unwrap();
        let mut changed = store
            .load_outbound_authority(&[3; 32])
            .unwrap()
            .unwrap()
            .value()
            .clone();
        changed.revision = 2;
        changed.disabled = true;
        store
            .save_outbound_authority_local(&Record::encode(changed).unwrap(), 1)
            .unwrap();
        assert!(matches!(
            store.list_outbound_authorities_local(first.next, Some(first.snapshot), 1),
            Err(Error::RevisionConflict)
        ));
        let second = store
            .list_outbound_authorities_local(None, None, 1)
            .unwrap();
        let mut added = credential();
        added.reference = vec![4; 32];
        store
            .save_outbound_authority_local(&Record::encode(added).unwrap(), 0)
            .unwrap();
        assert!(matches!(
            store.list_outbound_authorities_local(second.next, Some(second.snapshot), 1),
            Err(Error::RevisionConflict)
        ));
        let third = store
            .list_outbound_authorities_local(None, None, 1)
            .unwrap();
        // Even a valid same-revision container substitution changes the snapshot.
        let mut changed = store
            .load_outbound_authority(&[4; 32])
            .unwrap()
            .unwrap()
            .value()
            .clone();
        changed.expires_ms += 1;
        let changed = Record::encode(changed).unwrap();
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute(
            "UPDATE outbound_authorities SET payload=?1 WHERE reference=?2",
            rusqlite::params![changed.container(), [4u8; 32].as_slice()],
        )
        .unwrap();
        drop(sql);
        assert!(matches!(
            store.list_outbound_authorities_local(third.next, Some(third.snapshot), 1),
            Err(Error::RevisionConflict)
        ));
    }
    #[test]
    fn authority_listing_rejects_corruption_outside_current_page_before_allocating_large_rows() {
        for case in 0..4 {
            let (_dir, path, mut store) = setup();
            seed_list(&mut store, 2);
            let sql = rusqlite::Connection::open(&path).unwrap();
            match case {
                0 => {
                    sql.execute(
                        "UPDATE outbound_authorities SET payload=X'00' WHERE reference=?1",
                        [[2u8; 32].as_slice()],
                    )
                    .unwrap();
                }
                1 => {
                    sql.execute(
                        "UPDATE outbound_authorities SET payload=zeroblob(?1) WHERE reference=?2",
                        rusqlite::params![
                            outbound_authority::MAX_CONTAINER_BYTES as i64 + 1,
                            [2u8; 32].as_slice()
                        ],
                    )
                    .unwrap();
                }
                2 => {
                    sql.execute(
                        "UPDATE outbound_authorities SET subject=?1 WHERE reference=?2",
                        rusqlite::params!["x".repeat(257), [2u8; 32].as_slice()],
                    )
                    .unwrap();
                }
                _ => {
                    sql.execute_batch("ALTER TABLE outbound_authorities RENAME TO originals; CREATE TABLE outbound_authorities(reference,revision,kind,subject,payload); INSERT INTO outbound_authorities SELECT reference,'not-integer',kind,subject,payload FROM originals;").unwrap();
                }
            }
            drop(sql);
            assert!(
                store
                    .list_outbound_authorities_local(None, None, 1)
                    .is_err(),
                "case {case}"
            );
        }
    }
    #[test]
    fn authority_listing_rejects_more_than_the_bounded_table_count() {
        let (_dir, path, mut store) = setup();
        let mut sql = rusqlite::Connection::open(&path).unwrap();
        let tx = sql.transaction().unwrap();
        for index in 0..=outbound_authority::MAX_RECORDS {
            let mut value = credential();
            value.reference[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            let record = Record::encode(value).unwrap();
            tx.execute("INSERT INTO outbound_authorities(reference,revision,kind,subject,payload) VALUES(?1,1,1,?2,?3)",rusqlite::params![record.reference().as_slice(),outbound_authority::WINDOWS_DPAPI_PROVIDER,record.container()]).unwrap();
        }
        tx.commit().unwrap();
        drop(sql);
        assert!(matches!(
            store.list_outbound_authorities_local(None, None, 1),
            Err(Error::Limit)
        ));
    }
    #[test]
    fn existing_inbound_material_consumes_outbound_byte_capacity() {
        let (_dir, path, mut store) = setup();
        let value = inbound();
        store.save_service_authority_local(&value, 0).unwrap();
        drop(store);
        let mut store = Store::open_existing(
            &path,
            EventBudget {
                max_count: 1024,
                max_bytes: value.container().len() as u64,
            },
        )
        .unwrap();
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0),
            Err(Error::EventCapacity)
        );
        assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        store.integrity_check().unwrap();
    }
    #[test]
    fn count_limit_and_corrupted_payload_metadata_fail_closed() {
        let (_dir, path, mut store) = setup();
        for index in 0..outbound_authority::MAX_RECORDS {
            let mut value = credential();
            value.reference[..8].copy_from_slice(&(index as u64 + 1).to_le_bytes());
            store
                .save_outbound_authority_local(&Record::encode(value).unwrap(), 0)
                .unwrap();
        }
        let mut extra = credential();
        extra.reference = vec![99; 32];
        assert_eq!(
            store.save_outbound_authority_local(&Record::encode(extra).unwrap(), 0),
            Err(Error::Limit)
        );
        drop(store);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("UPDATE outbound_authorities SET subject='other-provider';")
            .unwrap();
        drop(sql);
        assert!(Store::open_existing(&path, Default::default()).is_err());
    }
}

#[cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
mod crash {
    use super::*;
    use morrow_core::store::Store;
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    #[test]
    #[ignore = "subprocess entry; exercised by parent fault-injection tests"]
    fn outbound_authority_crash_child() {
        let path = std::env::var_os("MORROW_OUTBOUND_CRASH_DB").expect("child database");
        let mut store = Store::open_existing(Path::new(&path), Default::default()).unwrap();
        if std::env::var("MORROW_OUTBOUND_CRASH_MODE").as_deref() == Ok("replace") {
            let mut value = credential();
            value.revision = 2;
            value.disabled = true;
            store
                .save_outbound_authority_local(&Record::encode(value).unwrap(), 1)
                .unwrap();
        }
        panic!("expected boundary was not reached");
    }
    fn run(path: &Path, mode: &str, point: &str) {
        let mut process = Command::new(std::env::current_exe().unwrap());
        process
            .args([
                "--exact",
                "crash::outbound_authority_crash_child",
                "--ignored",
                "--nocapture",
            ])
            .env("MORROW_OUTBOUND_CRASH_DB", path)
            .env("MORROW_OUTBOUND_CRASH_MODE", mode)
            .env("MORROW_TEST_CRASH_AT", point)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            process.creation_flags(0x08000000);
        }
        let mut child = process.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    assert_eq!(status.code(), Some(86), "{point}");
                    return;
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("observe {point}: {error}");
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("timeout {point}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn replacement_process_exit_preserves_whole_old_or_new_credential() {
        for (point, revision) in [
            ("outbound-authority-before-commit", 1),
            ("outbound-authority-after-commit", 2),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("replacement.db");
            let mut store = Store::open(&path, Default::default()).unwrap();
            store
                .save_outbound_authority_local(&Record::encode(credential()).unwrap(), 0)
                .unwrap();
            drop(store);
            run(&path, "replace", point);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            let record = store.load_outbound_authority(&[1; 32]).unwrap().unwrap();
            assert_eq!(record.value().revision, revision);
            assert_eq!(record.value().disabled, revision == 2);
            assert_eq!(store.pending_usage().unwrap(), (0, 0));
            store.integrity_check().unwrap();
        }
    }
    #[test]
    fn migration_process_exit_preserves_original_authority_identity_and_schema() {
        for (point, version) in [
            ("outbound-authority-migration-before-commit", 19),
            ("outbound-authority-migration-after-commit", 20),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("migration.db");
            drop(Store::open(&path, Default::default()).unwrap());
            let sql = rusqlite::Connection::open(&path).unwrap();
            let original: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            sql.execute_batch("DROP TABLE IF EXISTS tls_identities; DROP TABLE outbound_authorities; PRAGMA user_version=19;")
                .unwrap();
            drop(sql);
            run(&path, "migrate", point);
            let sql = rusqlite::Connection::open(&path).unwrap();
            assert_eq!(
                sql.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                version
            );
            let tables: i64 = sql
                .query_row(
                    "SELECT count(*) FROM sqlite_schema WHERE name='outbound_authorities'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(tables, i64::from(version == 20));
            let retained: Vec<u8> = sql
                .query_row("SELECT payload FROM service_authority_identity", [], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(retained, original);
            drop(sql);
            let store = Store::open_existing(&path, Default::default()).unwrap();
            store.integrity_check().unwrap();
            assert!(store.load_outbound_authority(&[1; 32]).unwrap().is_none());
        }
    }
}
