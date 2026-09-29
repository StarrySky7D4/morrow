#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    attachment::{self, RetentionKind},
    store::{EventBudget, Store},
};
use prost::Message;
use sha2::{Digest, Sha256};
use std::io::{self, Cursor, Read};

struct Unreadable;
impl Read for Unreadable {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("confirmed owner retry must not reopen the original input")
    }
}

#[test]
fn retained_owner_preserves_large_valid_unknown_retention_fields() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("db");
    let bytes = b"a confirmed attachment";
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let blob = store
        .stage_blob_retained(
            &mut Cursor::new(bytes),
            bytes.len() as u64,
            digest,
            "import-owner",
            100,
        )
        .unwrap();
    drop(store);

    let mut raw = attachment::proto::Retention {
        schema_version: 1,
        owner_id: "import-owner".into(),
        blob_id: blob.id.clone(),
        kind: RetentionKind::Snapshot as i32,
    }
    .encode_to_vec();
    // Unknown field 100 (length-delimited). Fill the complete allowed raw
    // payload with reproducible noncompressible bytes, making its valid LZ4
    // container larger than the 4096-byte *decoded* metadata limit.
    raw.extend_from_slice(&[0xa2, 0x06]);
    let length = 4096 - raw.len() - 2;
    assert!((128..16384).contains(&length));
    raw.push((length as u8 & 0x7f) | 0x80);
    raw.push((length >> 7) as u8);
    let mut random = 0x4132_756bu32;
    for _ in 0..length {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        raw.push(random as u8);
    }
    assert_eq!(raw.len(), 4096);
    let packed = lz4_flex::block::compress(&raw);
    let mut container = b"MORROWP1".to_vec();
    container.extend_from_slice(&1u16.to_le_bytes());
    container.extend_from_slice(&(raw.len() as u32).to_le_bytes());
    container.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    container.extend_from_slice(&Sha256::digest(&raw));
    container.extend_from_slice(&packed);
    assert!(container.len() > 4096);
    assert_eq!(
        attachment::decode_retention(&container).unwrap().owner_id,
        "import-owner"
    );
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute(
        "UPDATE retentions SET metadata=?1 WHERE owner='import-owner'",
        [&container],
    )
    .unwrap();
    drop(db);

    let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
    assert_eq!(
        store.retained_blob_local("import-owner").unwrap(),
        Some(blob.clone())
    );
    assert_eq!(
        store
            .stage_blob_retained(
                &mut Unreadable,
                bytes.len() as u64,
                digest,
                "import-owner",
                0
            )
            .unwrap(),
        blob
    );
    store.integrity_check().unwrap();
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    let after: Vec<u8> = db
        .query_row(
            "SELECT metadata FROM retentions WHERE owner='import-owner'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        after, container,
        "read-only recovery must preserve unknown metadata fields"
    );
}
