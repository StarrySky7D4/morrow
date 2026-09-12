#![cfg(target_os = "windows")]
use morrow_audit::{
    keys::Key,
    session::{OpenMode, Session, key_path},
    snapshot,
};
use morrow_core::{
    content::{Attachment, CardRecord},
    store::EventBudget,
};
use std::{io::Cursor, path::Path};
fn open(db: &Path) -> Session {
    Session::open(db, EventBudget::default(), OpenMode::Initialize).unwrap()
}
fn seed(root: &Path, large: bool) -> Vec<u8> {
    let mut session = open(&root.join("workbench.db"));
    let mut state = 19u32;
    let bytes = (0..if large { 3 * 1024 * 1024 + 17 } else { 31 })
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 24) as u8
        })
        .collect::<Vec<_>>();
    let store = session.runtime().store_local_mut();
    let blob = store
        .stage_blob(&mut Cursor::new(&bytes), bytes.len() as u64, None, 1)
        .unwrap();
    let record = CardRecord::new_with_attachments(
        "card",
        "unknown.type",
        1,
        "original",
        vec![0, 255],
        &[Attachment {
            id: "file".into(),
            display_name: "original.bin".into(),
            media_type: "application/octet-stream".into(),
            byte_length: blob.byte_length,
            sha256: blob.sha256,
        }],
    )
    .unwrap();
    store.create_local("create", &record).unwrap();
    session.flush(16).unwrap();
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "pending",
            &CardRecord::new("pending", "text", 1, "pending", vec![3]).unwrap(),
        )
        .unwrap();
    bytes
}
#[test]
fn consistent_multichunk_snapshot_preserves_attachments_history_pending_and_identity() {
    let d = tempfile::tempdir().unwrap();
    let expected = seed(d.path(), true);
    let db = d.path().join("workbench.db");
    let mut session = open(&db);
    let before = std::fs::read(&db).unwrap();
    let original_key = std::fs::read(key_path(&db).unwrap()).unwrap();
    let archive = d.path().join("library.morrowbackup");
    session.backup_snapshot(&archive).unwrap();
    assert_eq!(std::fs::read(&db).unwrap(), before);
    assert!(session.backup_snapshot(&archive).is_err());
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "later",
            &CardRecord::new("later", "text", 1, "later", vec![9]).unwrap(),
        )
        .unwrap();
    let restored = d.path().join("restored");
    snapshot::restore(&archive, &restored).unwrap();
    let restored_db = restored.join("workbench.db");
    assert!(matches!(
        Session::open(&restored_db, EventBudget::default(), OpenMode::Existing),
        Err(morrow_audit::session::SessionError::IdentityBusy)
    ));
    assert!(
        session
            .store()
            .snapshot_to(&d.path().join("too-small"), 1)
            .is_err()
    );
    assert!(!d.path().join("too-small").exists());
    drop(session);
    let restored_session = open(&restored_db);
    assert!(restored_session.store().card("later").unwrap().is_none());
    assert!(restored_session.store().card("pending").unwrap().is_some());
    assert_eq!(restored_session.store().pending_usage().unwrap().0, 1);
    assert!(
        restored_session
            .store()
            .last_sealed_segment()
            .unwrap()
            .is_some()
    );
    let mut actual = vec![];
    restored_session
        .store()
        .export_attachment_local("card", "file", &mut actual)
        .unwrap();
    assert_eq!(actual, expected);
    assert_eq!(
        std::fs::read(key_path(&restored_db).unwrap()).unwrap(),
        original_key
    );
    assert!(snapshot::restore(&archive, &restored).is_err());
}
#[test]
fn malformed_truncated_trailing_and_foreign_identity_archives_never_publish() {
    use prost::Message;
    mod proto {
        include!(concat!(env!("OUT_DIR"), "/morrow.snapshot.v1.rs"));
    }
    let d = tempfile::tempdir().unwrap();
    seed(d.path(), false);
    let archive = d.path().join("good");
    open(&d.path().join("workbench.db"))
        .backup_snapshot(&archive)
        .unwrap();
    let original = std::fs::read(&archive).unwrap();
    let mut variants = vec![original[..original.len() - 1].to_vec()];
    let mut trailing = original.clone();
    trailing.push(0);
    variants.push(trailing);
    let mut bad = original.clone();
    let last = bad.len() - 2;
    bad[last] ^= 0x55;
    variants.push(bad);
    let mut huge = original.clone();
    huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    variants.push(huge);
    let end = 12 + u32::from_le_bytes(original[8..12].try_into().unwrap()) as usize;
    let raw = lz4_flex::block::decompress_size_prepended(&original[12..end]).unwrap();
    let mut manifest = proto::Manifest::decode(raw.as_slice()).unwrap();
    let foreign = d.path().join("foreign");
    let key = Key::create(&foreign).unwrap();
    manifest.protected_key = std::fs::read(foreign).unwrap();
    manifest.log_id = key.trust().id;
    manifest.public_key = key.trust().key.as_bytes().to_vec();
    let packed = lz4_flex::block::compress_prepend_size(&manifest.encode_to_vec());
    variants.push(
        [
            b"MORROWS1".as_slice(),
            &(packed.len() as u32).to_le_bytes(),
            &packed,
            &original[end..],
        ]
        .concat(),
    );
    let mut oversized = proto::Manifest::decode(raw.as_slice()).unwrap();
    oversized.database_bytes = 8 * 1024 * 1024 * 1024 + 1;
    let packed = lz4_flex::block::compress_prepend_size(&oversized.encode_to_vec());
    variants.push(
        [
            b"MORROWS1".as_slice(),
            &(packed.len() as u32).to_le_bytes(),
            &packed,
            &original[end..],
        ]
        .concat(),
    );
    let chunk_end =
        end + 4 + u32::from_le_bytes(original[end..end + 4].try_into().unwrap()) as usize;
    let mut chunk = proto::Chunk::decode(
        lz4_flex::block::decompress_size_prepended(&original[end + 4..chunk_end])
            .unwrap()
            .as_slice(),
    )
    .unwrap();
    chunk.index = 9;
    let packed = lz4_flex::block::compress_prepend_size(&chunk.encode_to_vec());
    variants.push(
        [
            &original[..end],
            &(packed.len() as u32).to_le_bytes(),
            &packed,
            &original[chunk_end..],
        ]
        .concat(),
    );
    for (i, bytes) in variants.iter().enumerate() {
        let source = d.path().join(format!("bad-{i}"));
        std::fs::write(&source, bytes).unwrap();
        let target = d.path().join(format!("target-{i}"));
        assert!(snapshot::restore(&source, &target).is_err());
        assert!(!target.exists());
    }
    let existing = d.path().join("existing");
    std::fs::create_dir(&existing).unwrap();
    std::fs::write(existing.join("keep"), b"keep").unwrap();
    assert!(snapshot::restore(&archive, &existing).is_err());
    assert_eq!(std::fs::read(existing.join("keep")).unwrap(), b"keep");
}
#[test]
#[cfg(feature = "fault-injection")]
fn snapshot_child() {
    let Ok(root) = std::env::var("MORROW_SNAPSHOT_TEST_ROOT") else {
        return;
    };
    let root = Path::new(&root);
    if std::env::var("MORROW_AUDIT_CRASH_AT")
        .unwrap()
        .starts_with("snapshot-restore-")
    {
        snapshot::restore(&root.join("archive"), &root.join("restored")).unwrap();
    } else {
        open(&root.join("workbench.db"))
            .backup_snapshot(&root.join("archive"))
            .unwrap();
    }
    panic!("fault hook missed");
}
#[test]
#[cfg(feature = "fault-injection")]
fn process_crashes_publish_whole_archive_or_whole_new_library_and_keep_source() {
    for point in [
        "snapshot-before-publish",
        "snapshot-after-publish",
        "snapshot-restore-before-publish",
        "snapshot-restore-after-publish",
    ] {
        let d = tempfile::tempdir().unwrap();
        seed(d.path(), false);
        let db = d.path().join("workbench.db");
        let original = std::fs::read(&db).unwrap();
        let key = std::fs::read(key_path(&db).unwrap()).unwrap();
        let archive = d.path().join("archive");
        if point.starts_with("snapshot-restore-") {
            open(&db).backup_snapshot(&archive).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "snapshot_child", "--nocapture"])
            .env("MORROW_SNAPSHOT_TEST_ROOT", d.path())
            .env("MORROW_AUDIT_CRASH_AT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86));
        assert_eq!(std::fs::read(&db).unwrap(), original);
        assert_eq!(std::fs::read(key_path(&db).unwrap()).unwrap(), key);
        if point == "snapshot-before-publish" {
            assert!(!archive.exists());
            open(&db).backup_snapshot(&archive).unwrap();
        }
        let restored = d.path().join("restored");
        if point == "snapshot-restore-after-publish" {
            assert!(restored.exists());
        } else {
            assert!(!restored.exists());
            snapshot::restore(&archive, &restored).unwrap();
        }
        let recovered = open(&restored.join("workbench.db"));
        assert!(recovered.store().card("card").unwrap().is_some());
        assert_eq!(
            std::fs::read(key_path(&restored.join("workbench.db")).unwrap()).unwrap(),
            key
        );
    }
}
