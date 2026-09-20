//! Host-local enumeration remains bounded, consistent and inert.
#![cfg(not(target_arch = "wasm32"))]

use morrow_core::{
    Error,
    service_authority::{self, Record, proto as authority},
    service_config::{self, Config, proto as config},
    store::Store,
};
use prost::Message;
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

fn reference(index: u32) -> [u8; 32] {
    let mut value = [0; 32];
    value[..4].copy_from_slice(&index.to_be_bytes());
    value
}

fn configuration(index: u32) -> Config {
    Config::encode(config::Configuration {
        schema_version: 1,
        id: format!("node-{index:04}"),
        revision: 1,
        namespace: reference(index).to_vec(),
        retention_ms: 1000,
        service: "service.example".into(),
        handler: "api.invoke".into(),
        package_sha256: vec![8; 32],
        disabled: index.is_multiple_of(2),
        principals: vec![],
        approval_references: vec![],
    })
    .unwrap()
}

fn approval(index: u32) -> Record {
    Record::encode(authority::Record {
        schema_version: 1,
        reference: reference(index).to_vec(),
        revision: 1,
        created_ms: 10,
        expires_ms: 1000, // Expired historical approvals remain enumerable.
        disabled: index.is_multiple_of(2),
        kind: Some(if index.is_multiple_of(2) {
            authority::record::Kind::Publication(authority::Publication {
                config_id: format!("node-{index:04}"),
                config_sha256: vec![3; 32],
                listen_address: "127.0.0.1:8080".into(),
                tls_required: false,
                method: "POST".into(),
                path: "/api".into(),
                query_path: "/history".into(),
            })
        } else {
            authority::record::Kind::Authentication(authority::Authentication {
                principal_id: format!("alice-{index}"),
                token_sha256: vec![9; 32],
            })
        }),
    })
    .unwrap()
}

fn setup() -> (tempfile::TempDir, std::path::PathBuf, Store) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("original.db");
    let store = Store::open(&path, Default::default()).unwrap();
    (dir, path, store)
}

fn seed(store: &mut Store, count: u32) {
    for index in (1..=count).rev() {
        store
            .save_service_config_local(&configuration(index), 0)
            .unwrap();
        store
            .save_service_authority_local(&approval(index), 0)
            .unwrap();
    }
}

fn pack(magic: &[u8], raw: &[u8]) -> Vec<u8> {
    let packed = lz4_flex::block::compress(raw);
    let mut out = magic.to_vec();
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&Sha256::digest(raw));
    out.extend_from_slice(&packed);
    out
}

#[test]
fn empty_snapshots_reopen_and_listing_does_not_pin_or_revoke() {
    let (_dir, path, mut store) = setup();
    let configs = store.list_service_configs_local(None, None, 1).unwrap();
    let approvals = store.list_service_authorities_local(None, None, 2).unwrap();
    assert!(configs.configs.is_empty() && configs.next.is_none());
    assert!(approvals.records.is_empty() && approvals.next.is_none());
    assert_ne!(configs.snapshot, approvals.snapshot);
    assert_ne!(configs.snapshot, [0; 32]);
    assert_ne!(approvals.snapshot, [0; 32]);
    let mut other = Store::open_existing(&path, Default::default()).unwrap();
    let lease = other.pin_service_authority().unwrap();
    store
        .list_service_configs_local(None, Some(configs.snapshot), 1)
        .unwrap();
    store
        .list_service_authorities_local(None, Some(approvals.snapshot), 1)
        .unwrap();
    lease.check().unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    drop(other);
    drop(store);
    let mut reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened
            .list_service_configs_local(None, None, 1)
            .unwrap()
            .snapshot,
        configs.snapshot
    );
    assert_eq!(
        reopened
            .list_service_authorities_local(None, None, 2)
            .unwrap()
            .snapshot,
        approvals.snapshot
    );
    // A previous listing must not retain a read lease that prevents writing.
    seed(&mut reopened, 1);
}

#[test]
fn sorted_pages_preserve_original_containers_and_historical_rows() {
    let (_dir, path, mut store) = setup();
    seed(&mut store, 3);
    let lease = store.pin_service_authority().unwrap();
    let mut cursor = None;
    let mut snapshot = None;
    for index in 1..=3 {
        let page = store
            .list_service_configs_local(cursor.as_deref(), snapshot, 1)
            .unwrap();
        assert_eq!(page.configs.len(), 1);
        assert_eq!(
            page.configs[0].container(),
            configuration(index).container()
        );
        if let Some(expected) = snapshot {
            assert_eq!(page.snapshot, expected);
        }
        assert_eq!(page.next, (index < 3).then(|| format!("node-{index:04}")));
        cursor = page.next;
        snapshot = Some(page.snapshot);
    }
    let first = store.list_service_authorities_local(None, None, 2).unwrap();
    assert_eq!(first.records.len(), 2);
    assert_eq!(first.next, Some(reference(2)));
    let second = store
        .list_service_authorities_local(first.next, Some(first.snapshot), 2)
        .unwrap();
    assert_eq!(second.records.len(), 1);
    assert_eq!(second.snapshot, first.snapshot);
    assert!(second.next.is_none());
    for (index, record) in first.records.iter().chain(&second.records).enumerate() {
        assert_eq!(record.container(), approval(index as u32 + 1).container());
    }
    lease.check().unwrap();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    drop(store);
    let mut reopened = Store::open_existing(&path, Default::default()).unwrap();
    assert!(
        reopened
            .list_service_configs_local(Some("node-0003"), snapshot, 1)
            .unwrap()
            .configs
            .is_empty()
    );
    assert!(
        reopened
            .list_service_authorities_local(Some(reference(3)), Some(first.snapshot), 1)
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn invalid_limits_cursors_and_snapshots_are_rejected() {
    let (_dir, _path, mut store) = setup();
    seed(&mut store, 1);
    for limit in [0, 2, u32::MAX] {
        assert!(matches!(
            store.list_service_configs_local(None, None, limit),
            Err(Error::Limit)
        ));
    }
    for limit in [0, 3, u32::MAX] {
        assert!(matches!(
            store.list_service_authorities_local(None, None, limit),
            Err(Error::Limit)
        ));
    }
    let configs = store
        .list_service_configs_local(None, None, 1)
        .unwrap()
        .snapshot;
    let approvals = store
        .list_service_authorities_local(None, None, 1)
        .unwrap()
        .snapshot;
    assert!(
        store
            .list_service_configs_local(Some("node-0001"), None, 1)
            .is_err()
    );
    assert!(
        store
            .list_service_authorities_local(Some(reference(1)), None, 1)
            .is_err()
    );
    for cursor in ["node-0002", "../bad", ""] {
        assert!(
            store
                .list_service_configs_local(Some(cursor), Some(configs), 1)
                .is_err()
        );
    }
    for cursor in [[0; 32], reference(2)] {
        assert!(
            store
                .list_service_authorities_local(Some(cursor), Some(approvals), 1)
                .is_err()
        );
    }
    assert!(matches!(
        store.list_service_configs_local(None, Some([0; 32]), 1),
        Err(Error::RevisionConflict)
    ));
    assert!(matches!(
        store.list_service_authorities_local(None, Some([0; 32]), 1),
        Err(Error::RevisionConflict)
    ));
}

#[test]
fn snapshots_reject_updates_additions_and_same_revision_container_changes() {
    for case in 0..3 {
        let (_dir, path, mut store) = setup();
        seed(&mut store, 3);
        let configs = store.list_service_configs_local(None, None, 1).unwrap();
        let approvals = store.list_service_authorities_local(None, None, 2).unwrap();
        if case == 0 {
            let mut c = configuration(3).value().clone();
            c.revision = 2;
            c.disabled = true;
            let mut a = approval(3).value().clone();
            a.revision = 2;
            a.disabled = true;
            store
                .save_service_config_local(&Config::encode(c).unwrap(), 1)
                .unwrap();
            store
                .save_service_authority_local(&Record::encode(a).unwrap(), 1)
                .unwrap();
        } else if case == 1 {
            store
                .save_service_config_local(&configuration(4), 0)
                .unwrap();
            store.save_service_authority_local(&approval(4), 0).unwrap();
        } else {
            // Even drift that illegally leaves the SQL and protobuf revisions
            // unchanged must invalidate continuation through container identity.
            let db = Connection::open(&path).unwrap();
            let mut c = configuration(3).value().clone();
            c.retention_ms += 1;
            let mut a = approval(3).value().clone();
            a.expires_ms += 1;
            db.execute(
                "UPDATE service_configs SET payload=?1 WHERE id='node-0003'",
                [Config::encode(c).unwrap().container()],
            )
            .unwrap();
            db.execute(
                "UPDATE service_authorities SET payload=?1 WHERE reference=?2",
                params![
                    Record::encode(a).unwrap().container(),
                    reference(3).as_slice()
                ],
            )
            .unwrap();
        }
        assert!(
            matches!(
                store.list_service_configs_local(
                    configs.next.as_deref(),
                    Some(configs.snapshot),
                    1
                ),
                Err(Error::RevisionConflict)
            ),
            "case {case}"
        );
        assert!(
            matches!(
                store.list_service_authorities_local(approvals.next, Some(approvals.snapshot), 2),
                Err(Error::RevisionConflict)
            ),
            "case {case}"
        );
    }
}

#[test]
fn config_corruption_beyond_first_page_is_never_skipped() {
    for case in 0..7 {
        let (_dir, path, mut store) = setup();
        seed(&mut store, 3);
        let db = Connection::open(&path).unwrap();
        match case {
            0 => {
                db.execute(
                    "UPDATE service_configs SET payload=X'00' WHERE id='node-0003'",
                    [],
                )
                .unwrap();
            }
            1 => {
                db.execute(
                    "UPDATE service_configs SET payload=zeroblob(?1) WHERE id='node-0003'",
                    [service_config::MAX_CONTAINER_BYTES as i64 + 1],
                )
                .unwrap();
            }
            2 => {
                db.execute(
                    "UPDATE service_configs SET id=?1 WHERE id='node-0003'",
                    ["z".repeat(257)],
                )
                .unwrap();
            }
            3 => {
                db.execute(
                    "UPDATE service_configs SET revision=2 WHERE id='node-0003'",
                    [],
                )
                .unwrap();
            }
            4 => {
                db.execute(
                    "UPDATE service_configs SET namespace=?1 WHERE id='node-0003'",
                    [reference(4).as_slice()],
                )
                .unwrap();
            }
            5 => {
                let mut raw = configuration(3).value().encode_to_vec();
                raw.extend_from_slice(&[8, 1]);
                db.execute(
                    "UPDATE service_configs SET payload=?1 WHERE id='node-0003'",
                    [pack(service_config::MAGIC, &raw)],
                )
                .unwrap();
            }
            _ => {
                db.execute_batch("ALTER TABLE service_configs RENAME TO corrupt_configs; CREATE TABLE service_configs AS SELECT * FROM corrupt_configs; UPDATE service_configs SET revision='oops' WHERE id='node-0003'").unwrap();
            }
        }
        assert!(
            store.list_service_configs_local(None, None, 1).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn authority_corruption_beyond_first_page_is_never_skipped() {
    for case in 0..8 {
        let (_dir, path, mut store) = setup();
        seed(&mut store, 3);
        let db = Connection::open(&path).unwrap();
        let key = reference(3);
        match case {
            0 => {
                db.execute(
                    "UPDATE service_authorities SET payload=X'00' WHERE reference=?1",
                    [key.as_slice()],
                )
                .unwrap();
            }
            1 => {
                db.execute(
                    "UPDATE service_authorities SET payload=zeroblob(?1) WHERE reference=?2",
                    params![
                        service_authority::MAX_CONTAINER_BYTES as i64 + 1,
                        key.as_slice()
                    ],
                )
                .unwrap();
            }
            2 => {
                db.execute(
                    "UPDATE service_authorities SET subject=?1 WHERE reference=?2",
                    params!["z".repeat(257), key.as_slice()],
                )
                .unwrap();
            }
            3 => {
                db.execute(
                    "UPDATE service_authorities SET revision=2 WHERE reference=?1",
                    [key.as_slice()],
                )
                .unwrap();
            }
            4 => {
                db.execute(
                    "UPDATE service_authorities SET kind=2 WHERE reference=?1",
                    [key.as_slice()],
                )
                .unwrap();
            }
            5 => {
                db.execute(
                    "UPDATE service_authorities SET reference=?1 WHERE reference=?2",
                    params![reference(4).as_slice(), key.as_slice()],
                )
                .unwrap();
            }
            6 => {
                let mut raw = approval(3).value().encode_to_vec();
                raw.extend_from_slice(&[8, 1]);
                db.execute(
                    "UPDATE service_authorities SET payload=?1 WHERE reference=?2",
                    params![pack(service_authority::MAGIC, &raw), key.as_slice()],
                )
                .unwrap();
            }
            _ => {
                db.execute_batch("ALTER TABLE service_authorities RENAME TO corrupt_authorities; CREATE TABLE service_authorities AS SELECT * FROM corrupt_authorities").unwrap();
                db.execute(
                    "UPDATE service_authorities SET kind='oops' WHERE reference=?1",
                    [key.as_slice()],
                )
                .unwrap();
            }
        }
        assert!(
            store.list_service_authorities_local(None, None, 2).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn maximum_counts_are_readable_but_one_extra_row_fails_closed() {
    let (_dir, path, mut store) = setup();
    let mut db = Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    for index in 1..=service_config::MAX_CONFIGS as u32 {
        let c = configuration(index);
        tx.execute(
            "INSERT INTO service_configs(id,revision,namespace,payload) VALUES(?1,1,?2,?3)",
            params![c.value().id, c.value().namespace, c.container()],
        )
        .unwrap();
    }
    for index in 1..=service_authority::MAX_RECORDS as u32 {
        let a = approval(index);
        let (kind, subject) = match a.value().kind.as_ref().unwrap() {
            authority::record::Kind::Authentication(v) => (1, &v.principal_id),
            authority::record::Kind::Publication(v) => (2, &v.config_id),
        };
        tx.execute("INSERT INTO service_authorities(reference,revision,kind,subject,payload) VALUES(?1,1,?2,?3,?4)", params![a.value().reference,kind,subject,a.container()]).unwrap();
    }
    tx.commit().unwrap();
    assert_eq!(
        store
            .list_service_configs_local(None, None, 1)
            .unwrap()
            .configs
            .len(),
        1
    );
    assert_eq!(
        store
            .list_service_authorities_local(None, None, 2)
            .unwrap()
            .records
            .len(),
        2
    );
    let c = configuration(service_config::MAX_CONFIGS as u32 + 1);
    db.execute(
        "INSERT INTO service_configs(id,revision,namespace,payload) VALUES(?1,1,?2,?3)",
        params![c.value().id, c.value().namespace, c.container()],
    )
    .unwrap();
    let a = approval(service_authority::MAX_RECORDS as u32 + 1);
    let authority::record::Kind::Authentication(subject) = a.value().kind.as_ref().unwrap() else {
        panic!("odd fixture")
    };
    db.execute("INSERT INTO service_authorities(reference,revision,kind,subject,payload) VALUES(?1,1,1,?2,?3)", params![a.value().reference,subject.principal_id,a.container()]).unwrap();
    assert!(matches!(
        store.list_service_configs_local(None, None, 1),
        Err(Error::Limit)
    ));
    assert!(matches!(
        store.list_service_authorities_local(None, None, 2),
        Err(Error::Limit)
    ));
}
