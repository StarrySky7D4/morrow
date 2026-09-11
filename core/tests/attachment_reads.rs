#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    attachment::{AttachmentChunk, MAX_READ_BYTES},
    content::{Attachment, CardRecord},
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    response::{Failure, Outcome, Response},
    runtime::{Command, ReadAttachment},
    store::{EventBudget, Store},
};
use std::io::Cursor;
fn setup(bytes: &[u8]) -> (tempfile::TempDir, HostRuntime, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    let blob = store
        .stage_blob(&mut Cursor::new(bytes), bytes.len() as u64, None, 0)
        .unwrap();
    let items = ["file", "sibling"].map(|id| Attachment {
        id: id.into(),
        display_name: "secret name".into(),
        media_type: "application/octet-stream".into(),
        byte_length: blob.byte_length,
        sha256: blob.sha256,
    });
    store
        .create_local(
            "seed",
            &CardRecord::new_with_attachments("card", "type", 1, "title", vec![], &items).unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let connection = host.connect().unwrap();
    (dir, host, connection)
}
fn request(offset: u64, length: u32) -> ReadAttachment {
    ReadAttachment {
        request_id: "read".into(),
        card_id: "card".into(),
        attachment_id: "file".into(),
        expected_revision: 1,
        offset,
        length,
    }
}
fn read(
    host: &mut HostRuntime,
    connection: &Connection,
    value: ReadAttachment,
    now: u64,
) -> Outcome {
    Response::decode(
        &host
            .dispatch(
                connection,
                &Command::ReadAttachment(value).encode().unwrap(),
                || now,
            )
            .unwrap(),
    )
    .unwrap()
    .outcome
}
#[test]
fn exact_scope_and_revision_prevent_sibling_reads_and_mixed_transfers() {
    let (_dir, mut host, mut c) = setup(&vec![7; 100_000]);
    host.grant(&mut c, GrantKind::ReadSummary, "card", 100, 0)
        .unwrap();
    assert_eq!(
        read(&mut host, &c, request(0, 32768), 1),
        Outcome::Rejected(Failure::Denied)
    );
    assert!(
        host.grant(&mut c, GrantKind::ReadAttachment, "card", 100, 1)
            .is_err()
    );
    host.grant_attachment(&mut c, "card", "file", 100, 1)
        .unwrap();
    assert!(matches!(
        read(&mut host, &c, request(0, 32768), 2),
        Outcome::AttachmentChunk(_)
    ));
    let mut sibling = request(0, 1);
    sibling.attachment_id = "sibling".into();
    assert_eq!(
        read(&mut host, &c, sibling, 2),
        Outcome::Rejected(Failure::Denied)
    );
    host.store_local_mut()
        .set_attachments_local("clear", "card", 1, &[])
        .unwrap();
    assert_eq!(
        read(&mut host, &c, request(32768, 32768), 3),
        Outcome::Rejected(Failure::RevisionConflict)
    );
    let mut current = request(0, 1);
    current.expected_revision = 2;
    assert_eq!(
        read(&mut host, &c, current, 3),
        Outcome::Rejected(Failure::NotFound)
    );
    host.revoke_attachment(&mut c, "card", "file").unwrap();
    assert_eq!(
        read(&mut host, &c, request(0, 1), 4),
        Outcome::Rejected(Failure::Denied)
    );
}
#[test]
fn block_crossing_partial_and_empty_reads_keep_original_bytes() {
    let bytes: Vec<_> = (0..100_000).map(|i| (i % 251) as u8).collect();
    let (_dir, mut host, mut c) = setup(&bytes);
    host.grant_attachment(&mut c, "card", "file", 100, 0)
        .unwrap();
    for (offset, length) in [(0, 32768), (60_000, 12000), (99_995, 32768), (100_000, 1)] {
        let Outcome::AttachmentChunk(part) = read(&mut host, &c, request(offset, length), 1) else {
            panic!("chunk required")
        };
        let end = bytes.len().min(offset as usize + length as usize);
        assert_eq!(part.bytes, bytes[offset as usize..end]);
        assert_eq!(part.total_length, bytes.len() as u64);
    }
    assert_eq!(
        read(&mut host, &c, request(100_001, 1), 1),
        Outcome::Rejected(Failure::Limit)
    );
    assert!(
        Command::ReadAttachment(request(0, MAX_READ_BYTES + 1))
            .encode()
            .is_err()
    );
    assert!(
        Command::ReadAttachment(request(u64::MAX, 1))
            .encode()
            .is_err()
    );
    let (_empty, mut host, mut c) = setup(&[]);
    host.grant_attachment(&mut c, "card", "file", 100, 0)
        .unwrap();
    let Outcome::AttachmentChunk(part) = read(&mut host, &c, request(0, 1), 1) else {
        panic!("empty chunk")
    };
    assert!(part.bytes.is_empty());
    assert_eq!(part.total_length, 0);
}
#[test]
fn revoked_or_expired_read_never_releases_next_chunk() {
    let (_dir, mut host, mut c) = setup(&vec![1; 100_000]);
    host.grant_attachment(&mut c, "card", "file", 10, 0)
        .unwrap();
    let mut ticks = [9, 10].into_iter();
    let bytes = host
        .dispatch(
            &c,
            &Command::ReadAttachment(request(0, 32768)).encode().unwrap(),
            || ticks.next().unwrap(),
        )
        .unwrap();
    assert_eq!(
        Response::decode(&bytes).unwrap().outcome,
        Outcome::Rejected(Failure::Denied)
    );
    host.grant_attachment(&mut c, "card", "file", 100, 11)
        .unwrap();
    host.disconnect(&c).unwrap();
    assert_eq!(
        read(&mut host, &c, request(0, 32768), 12),
        Outcome::Rejected(Failure::Denied)
    );
}
#[test]
fn corrupted_chunk_index_or_payload_is_rejected_before_delivery() {
    let (dir, host, _c) = setup(&vec![2; 100_000]);
    let db = rusqlite::Connection::open(dir.path().join("db")).unwrap();
    db.execute(
        "UPDATE blob_chunks SET metadata=x'00' WHERE offset=65536",
        [],
    )
    .unwrap();
    // A range reads only its covered storage blocks; whole-file publication requires final SHA verification.
    assert_eq!(
        host.store_local()
            .read_attachment_chunk(&request(0, 32768))
            .unwrap()
            .bytes
            .len(),
        32768
    );
    assert!(
        host.store_local()
            .read_attachment_chunk(&request(65536, 1))
            .is_err()
    );
    assert!(host.store_local().integrity_check().is_err());
    let (dir, host, _c) = setup(&vec![3; 100_000]);
    let db = rusqlite::Connection::open(dir.path().join("db")).unwrap();
    db.execute("UPDATE blobs SET payload=zeroblob(size)", [])
        .unwrap();
    assert_eq!(
        host.store_local().read_attachment_chunk(&request(0, 1)),
        Err(Error::Integrity)
    );
}
#[test]
fn chunk_index_is_deduplicated_and_collected_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let mut store = Store::open(&path, EventBudget::default()).unwrap();
    let bytes = vec![5; 100_000];
    let blob = store
        .stage_blob(&mut Cursor::new(&bytes), bytes.len() as u64, None, 0)
        .unwrap();
    store
        .stage_blob(&mut Cursor::new(&bytes), bytes.len() as u64, None, 1)
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let count = || {
        db.query_row("SELECT count(*) FROM blob_chunks", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    };
    assert_eq!(count(), 2);
    store.retire_blob_local(&blob.id, 2).unwrap();
    store.collect_retired_local(60002, 60000).unwrap();
    assert_eq!(count(), 0);
    store.integrity_check().unwrap();
}
#[test]
fn runtime_chunk_bounds_reject_invalid_empty_or_overflow_packets() {
    let part = AttachmentChunk {
        card_id: "c".into(),
        attachment_id: "a".into(),
        revision: 1,
        offset: 0,
        total_length: 1,
        content_sha256: [0; 32],
        bytes: vec![],
    };
    assert!(part.validate().is_err());
    let mut part = part;
    part.offset = u64::MAX;
    assert!(
        Response {
            request_id: "r".into(),
            outcome: Outcome::AttachmentChunk(part)
        }
        .encode()
        .is_err()
    );
}
