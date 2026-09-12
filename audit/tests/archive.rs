use morrow_audit::{
    archive::{Append, Archive, ArchiveError},
    *,
};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
use prost::Message;
fn trust() -> (SigningKey, TrustedLog) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let log = TrustedLog {
        id: "archive-test".into(),
        key: key.verifying_key(),
    };
    (key, log)
}
fn segments() -> Vec<Vec<u8>> {
    let d = tempfile::tempdir().unwrap();
    let mut store = Store::open(&d.path().join("core.db"), EventBudget::default()).unwrap();
    for i in 0..3 {
        store
            .create_local(
                &format!("create-{i}"),
                &CardRecord::new(&format!("card-{i}"), "text", 1, "Test", vec![i]).unwrap(),
            )
            .unwrap();
    }
    let (key, log) = trust();
    let events = store.pending(0, 10).unwrap();
    let mut first = from_pending(&log, 1, [0; 32], &events[..2])
        .unwrap()
        .encode_to_vec();
    first.extend_from_slice(&[0xa0, 0x06, 7]); // original unknown field survives archive
    let first = sign_raw(&first, &log, &key).unwrap();
    let second = sign(
        &from_pending(
            &log,
            2,
            verify(&first, &log).unwrap().digest(),
            &events[2..],
        )
        .unwrap(),
        &log,
        &key,
    )
    .unwrap();
    vec![first, second]
}
#[test]
fn durable_idempotent_original_bytes_and_read_only_checkpoint() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("archive.db");
    let (_, trust) = trust();
    let data = segments();
    let mut a = Archive::open(&p, trust.clone(), true).unwrap();
    assert_eq!(a.check(None).unwrap(), None);
    assert!(matches!(a.append(&data[0]).unwrap(), Append::Stored(_)));
    assert!(matches!(
        a.append(&data[0]).unwrap(),
        Append::AlreadyStored(_)
    ));
    assert!(matches!(a.append(&data[1]).unwrap(), Append::Stored(_)));
    assert!(matches!(
        a.append(&data[0]).unwrap(),
        Append::AlreadyStored(_)
    ));
    assert_eq!(a.check(None).unwrap().unwrap().last_sequence, 3);
    drop(a);
    let mut a = Archive::open_read_only(&p, trust).unwrap();
    for (i, data) in data.iter().enumerate() {
        assert_eq!(a.segment(i as u64 + 1).unwrap().unwrap().container(), data);
    }
    let checkpoint = Checkpoint::from_verified(&a.segment(2).unwrap().unwrap());
    assert_eq!(a.check(Some(checkpoint)).unwrap().unwrap().index, 2);
    assert_eq!(a.append(&data[0]), Err(ArchiveError::ReadOnly));
}
#[test]
fn rejects_gap_fork_duplicate_event_wrong_key_and_preserves_archive() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("archive.db");
    let (key, trust) = trust();
    let data = segments();
    let mut a = Archive::open(&p, trust.clone(), true).unwrap();
    assert_eq!(a.append(&data[1]), Err(ArchiveError::Conflict));
    a.append(&data[0]).unwrap();
    let mut fork = verify(&data[0], &trust).unwrap().segment().clone();
    fork.previous_sha256[0] = 7;
    assert_eq!(
        a.append(&sign(&fork, &trust, &key).unwrap()),
        Err(ArchiveError::Conflict)
    );
    // Signed and sequence-correct, but reuses an existing operation identity.
    let mut duplicate = verify(&data[1], &trust).unwrap().segment().clone();
    duplicate.events[0] = verify(&data[0], &trust).unwrap().segment().events[0].clone();
    duplicate.events[0].sequence = 3;
    assert_eq!(
        a.append(&sign(&duplicate, &trust, &key).unwrap()),
        Err(ArchiveError::Conflict)
    );
    assert_eq!(a.check(None).unwrap().unwrap().last_sequence, 2);
    assert!(a.segment(2).unwrap().is_none()); // segment INSERT rolled back with event conflict
    a.append(&data[1]).unwrap();
    drop(a);
    let wrong = TrustedLog {
        key: SigningKey::from_bytes(&[8; 32]).verifying_key(),
        ..trust
    };
    assert!(Archive::open(&p, wrong, false).is_err());
}
#[test]
fn two_writers_reread_head_and_fail_busy_without_mutation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("archive.db");
    let (_, trust) = trust();
    let data = segments();
    let mut a = Archive::open(&p, trust.clone(), true).unwrap();
    let mut b = Archive::open(&p, trust, false).unwrap();
    a.append(&data[0]).unwrap();
    assert!(matches!(
        b.append(&data[0]).unwrap(),
        Append::AlreadyStored(_)
    ));
    let lock = rusqlite::Connection::open(&p).unwrap();
    lock.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert_eq!(b.append(&data[1]), Err(ArchiveError::Busy));
    lock.execute_batch("ROLLBACK").unwrap();
    b.append(&data[1]).unwrap();
    assert_eq!(a.check(None).unwrap().unwrap().index, 2);
}
#[test]
fn immutable_rows_tamper_index_and_unrelated_database_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("archive.db");
    let (_, trust) = trust();
    let data = segments();
    let mut a = Archive::open(&p, trust.clone(), true).unwrap();
    a.append(&data[0]).unwrap();
    let external = rusqlite::Connection::open(&p).unwrap();
    assert!(
        external
            .execute("UPDATE segments SET payload=x'00'", [])
            .is_err()
    );
    assert!(external.execute("DELETE FROM events", []).is_err());
    // Simulate corrupted/restored metadata. Cryptography alone cannot validate it.
    external
        .execute_batch("DROP TRIGGER immutable_events_delete; DELETE FROM events WHERE sequence=1")
        .unwrap();
    assert!(a.check(None).is_err());
    assert!(a.append(&data[0]).is_err()); // retry cannot confirm corrupt event index
    drop(a);
    assert!(Archive::open(&p, trust.clone(), false).is_err());
    let unrelated = d.path().join("other.db");
    let c = rusqlite::Connection::open(&unrelated).unwrap();
    c.execute_batch("CREATE TABLE content(value TEXT); INSERT INTO content VALUES('keep')")
        .unwrap();
    assert!(matches!(
        Archive::open(&unrelated, trust, true),
        Err(ArchiveError::Unsupported)
    ));
    assert_eq!(
        c.query_row("SELECT value FROM content", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
}
#[test]
fn pinned_later_checkpoint_rejects_valid_but_truncated_history() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("archive.db");
    let (_, trust) = trust();
    let data = segments();
    let checkpoint = Checkpoint::from_verified(&verify(&data[1], &trust).unwrap());
    let mut a = Archive::open(&p, trust, true).unwrap();
    assert!(a.check(Some(checkpoint.clone())).is_err());
    a.append(&data[0]).unwrap();
    assert!(a.check(Some(checkpoint.clone())).is_err());
    a.append(&data[1]).unwrap();
    a.check(Some(checkpoint)).unwrap();
}
#[cfg(feature = "fault-injection")]
#[test]
fn crash_child() {
    let Ok(root) = std::env::var("MORROW_ARCHIVE_CRASH_ROOT") else {
        return;
    };
    let (_, trust) = trust();
    let mut archive = Archive::open(
        &std::path::Path::new(&root).join("archive.db"),
        trust,
        false,
    )
    .unwrap();
    archive
        .append(&std::fs::read(std::path::Path::new(&root).join("input.maudit")).unwrap())
        .unwrap();
    panic!("crash boundary was not reached");
}
#[cfg(feature = "fault-injection")]
#[test]
fn process_crashes_recover_atomic_segment_and_event_index() {
    let data = segments();
    let (_, trust) = trust();
    for target in 0..2 {
        for point in [
            "after-segment",
            "after-event",
            "before-commit",
            "after-commit",
        ] {
            let d = tempfile::tempdir().unwrap();
            let p = d.path().join("archive.db");
            let mut a = Archive::open(&p, trust.clone(), true).unwrap();
            for prefix in &data[..target] {
                a.append(prefix).unwrap();
            }
            drop(a);
            std::fs::write(d.path().join("input.maudit"), &data[target]).unwrap();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "crash_child", "--nocapture"])
                .env("MORROW_ARCHIVE_CRASH_ROOT", d.path())
                .env("MORROW_AUDIT_CRASH_AT", point)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(86), "{point}");
            let mut recovered = Archive::open(&p, trust.clone(), false).unwrap();
            let committed = point == "after-commit";
            let expected = if committed { target + 1 } else { target } as u64;
            assert_eq!(
                recovered.check(None).unwrap().map(|r| r.index),
                if expected == 0 { None } else { Some(expected) }
            );
            let retry = recovered.append(&data[target]).unwrap();
            assert!(matches!(
                (&retry, committed),
                (Append::AlreadyStored(_), true) | (Append::Stored(_), false)
            ));
            assert_eq!(
                recovered.check(None).unwrap().unwrap().last_sequence,
                if target == 0 { 2 } else { 3 }
            );
            println!("PASS crash prefix={target} {point}: committed={committed}; retry={retry:?}");
        }
    }
}
