#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    Error,
    audit::{self, SigningKey, TrustedLog},
    content::{Attachment, CardRecord},
    records::{Kind, Record},
    store::{EventBudget, Store},
    transaction::Lookup,
};
fn trust() -> (SigningKey, TrustedLog) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let trust = TrustedLog {
        id: "core-seals-test".into(),
        key: key.verifying_key(),
    };
    (key, trust)
}
fn card(id: &str) -> CardRecord {
    CardRecord::new(id, "text", 1, "Title", vec![1, 2, 3]).unwrap()
}
fn signed(store: &Store, index: u64, previous: [u8; 32], limit: u32) -> Vec<u8> {
    let (key, trust) = trust();
    audit::sign(
        &audit::from_pending(&trust, index, previous, &store.pending(0, limit).unwrap()).unwrap(),
        &trust,
        &key,
    )
    .unwrap()
}
#[test]
fn sealing_releases_budget_preserves_retries_records_and_pinned_reopen() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let (_, trust) = trust();
    let budget = EventBudget {
        max_count: 2,
        max_bytes: 1000000,
    };
    let mut s = Store::open_audited(&p, budget, true, trust.clone()).unwrap();
    let receipt = s.create_local("create", &card("card")).unwrap();
    s.create_record_local("workspace", &Record::workspace("w", "Workspace").unwrap())
        .unwrap();
    assert_eq!(
        s.create_local("second", &card("second")),
        Err(Error::EventCapacity)
    );
    let one = signed(&s, 1, [0; 32], 2);
    assert!(s.seal_pending(&one).unwrap());
    assert!(!s.seal_pending(&one).unwrap());
    assert!(s.pending(0, 10).unwrap().is_empty());
    s.integrity_check().unwrap();
    assert_eq!(s.create_local("create", &card("card")).unwrap(), receipt);
    s.create_local("second", &card("second")).unwrap();
    assert_eq!(s.pending(0, 10).unwrap()[0].0, 3);
    let two = signed(&s, 2, audit::verify(&one, &trust).unwrap().digest(), 10);
    s.seal_pending(&two).unwrap();
    drop(s);
    assert!(matches!(
        Store::open_existing(&p, budget),
        Err(Error::Invalid("audit trust required"))
    ));
    let wrong = TrustedLog {
        key: SigningKey::from_bytes(&[8; 32]).verifying_key(),
        ..trust.clone()
    };
    assert!(Store::open_audited(&p, budget, false, wrong).is_err());
    let s = Store::open_audited(&p, budget, false, trust).unwrap();
    assert!(matches!(s.lookup("create").unwrap(), Lookup::Committed(_)));
    assert!(s.record_local(Kind::Workspace, "w").unwrap().is_some());
    assert_eq!(s.sealed_segment(1).unwrap().unwrap(), one);
    s.integrity_check().unwrap();
}
#[test]
fn invalid_evidence_never_confirms_and_historical_attachment_remains_retained() {
    let d = tempfile::tempdir().unwrap();
    let (_, trust) = trust();
    let (key, _) = self::trust();
    let mut s = Store::open_audited(
        &d.path().join("db"),
        EventBudget::default(),
        true,
        trust.clone(),
    )
    .unwrap();
    let blob = s
        .stage_blob(&mut std::io::Cursor::new(b"original"), 8, None, 0)
        .unwrap();
    let c = CardRecord::new_with_attachments(
        "card",
        "text",
        1,
        "title",
        vec![],
        &[Attachment {
            id: "file".into(),
            display_name: "file".into(),
            media_type: "application/octet-stream".into(),
            byte_length: 8,
            sha256: blob.sha256,
        }],
    )
    .unwrap();
    s.create_local("create", &c).unwrap();
    let one = signed(&s, 1, [0; 32], 10);
    let mut other = Store::open(&d.path().join("other"), EventBudget::default()).unwrap();
    other.create_local("create", &card("foreign")).unwrap();
    let foreign = signed(&other, 1, [0; 32], 10);
    assert!(s.seal_pending(&foreign).is_err()); // valid signature, different original commit
    assert_eq!(s.pending(0, 10).unwrap().len(), 1);
    let mut changed = audit::verify(&one, &trust).unwrap().segment().clone();
    changed.events[0].sequence = 2;
    assert!(
        s.seal_pending(&audit::sign(&changed, &trust, &key).unwrap())
            .is_err()
    );
    assert_eq!(s.pending(0, 10).unwrap().len(), 1);
    let mut changed = one.clone();
    let last = changed.len() - 1;
    changed[last] ^= 1;
    assert!(s.seal_pending(&changed).is_err());
    assert_eq!(s.pending(0, 10).unwrap().len(), 1);
    s.seal_pending(&one).unwrap();
    let mut bytes = Vec::new();
    s.export_attachment_local("card", "file", &mut bytes)
        .unwrap();
    assert_eq!(bytes, b"original");
    s.set_attachments_local("detach", "card", 1, &[]).unwrap();
    let two = signed(&s, 2, audit::verify(&one, &trust).unwrap().digest(), 10);
    s.seal_pending(&two).unwrap();
    let mut historical = Vec::new();
    s.export_blob_local(&blob.id, &mut historical).unwrap();
    assert_eq!(historical, b"original");
    assert_eq!(s.retire_blob_local(&blob.id, 1), Err(Error::Retained));
    s.integrity_check().unwrap();
}
fn legacy(path: &std::path::Path) {
    let mut s = Store::open(path, EventBudget::default()).unwrap();
    s.create_local("create", &card("card")).unwrap();
    drop(s);
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute_batch(
        "DROP TABLE IF EXISTS service_configs; DROP INDEX IF EXISTS io_evidence_kind; DROP TABLE io_evidence; DROP TABLE io_material_reservations; DROP TABLE io_reservations; DROP TABLE io_intents; DROP TABLE read_archive_costs; DROP TABLE read_archive_totals; DROP TABLE read_captures; DROP TABLE operation_read_archives; DROP TABLE read_archive_parts; DROP TABLE read_archives; DROP TABLE task_evidence_chunks; DROP TABLE evidence_chunks; DROP TABLE operation_evidence; DROP TABLE task_evidence; DROP TABLE audit_identity; DROP TABLE operation_events; DROP TABLE sealed_segments; PRAGMA user_version=4;",
    )
    .unwrap();
}
#[test]
fn migration_preserves_v4_records_and_corrupt_migration_rolls_back() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    legacy(&p);
    let s = Store::open_existing(&p, EventBudget::default()).unwrap();
    assert_eq!(s.pending(0, 10).unwrap().len(), 1);
    assert!(s.card("card").unwrap().is_some());
    drop(s);
    let c = rusqlite::Connection::open(&p).unwrap();
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        18
    );
    let bad = d.path().join("bad");
    legacy(&bad);
    let c = rusqlite::Connection::open(&bad).unwrap();
    c.execute("DELETE FROM outbox", []).unwrap();
    assert!(Store::open_existing(&bad, EventBudget::default()).is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name='operation_events'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
#[test]
fn tampered_confirmation_and_missing_local_signed_original_block_reopen() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("db");
    let (_, trust) = trust();
    let mut s = Store::open_audited(&p, EventBudget::default(), true, trust.clone()).unwrap();
    s.create_local("create", &card("card")).unwrap();
    let one = signed(&s, 1, [0; 32], 10);
    s.seal_pending(&one).unwrap();
    drop(s);
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch(
        "PRAGMA foreign_keys=OFF; DROP TRIGGER sealed_no_delete; DELETE FROM sealed_segments;",
    )
    .unwrap();
    assert!(Store::open_audited(&p, EventBudget::default(), false, trust).is_err());
}
#[cfg(feature = "fault-injection")]
#[test]
fn seal_child() {
    let Ok(root) = std::env::var("MORROW_SEAL_ROOT") else {
        return;
    };
    let path = std::path::Path::new(&root);
    let (_, trust) = trust();
    let mut s =
        Store::open_audited(&path.join("db"), EventBudget::default(), false, trust).unwrap();
    if std::env::var("MORROW_TEST_CRASH_AT")
        .unwrap()
        .starts_with("seal-migration-")
    {
        panic!("migration boundary missing")
    }
    s.seal_pending(&std::fs::read(path.join("input")).unwrap())
        .unwrap();
    panic!("seal crash boundary missing");
}
#[cfg(feature = "fault-injection")]
#[test]
fn crash_confirmation_and_migration_are_atomic() {
    let (_, trust) = trust();
    for point in [
        "seal-after-segment",
        "seal-after-link",
        "seal-after-confirm",
        "seal-before-commit",
        "seal-after-commit",
        "seal-migration-after-index",
        "seal-migration-before-commit",
        "seal-migration-after-commit",
    ] {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("db");
        let migration = point.starts_with("seal-migration-");
        if migration {
            legacy(&p);
        } else {
            let mut s =
                Store::open_audited(&p, EventBudget::default(), true, trust.clone()).unwrap();
            s.create_local("create", &card("card")).unwrap();
            s.create_record_local("workspace", &Record::workspace("w", "W").unwrap())
                .unwrap();
            std::fs::write(d.path().join("input"), signed(&s, 1, [0; 32], 10)).unwrap();
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "seal_child", "--nocapture"])
            .env("MORROW_SEAL_ROOT", d.path())
            .env("MORROW_TEST_CRASH_AT", point)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(86), "{point}");
        let mut recovered =
            Store::open_audited(&p, EventBudget::default(), false, trust.clone()).unwrap();
        recovered.integrity_check().unwrap();
        if migration {
            assert_eq!(recovered.pending(0, 10).unwrap().len(), 1);
        } else {
            let committed = point == "seal-after-commit";
            assert_eq!(
                recovered.pending(0, 10).unwrap().len(),
                if committed { 0 } else { 2 }
            );
            assert_eq!(
                recovered
                    .seal_pending(&std::fs::read(d.path().join("input")).unwrap())
                    .unwrap(),
                !committed
            );
            assert!(recovered.pending(0, 10).unwrap().is_empty());
        }
        println!("PASS core crash {point}: recovered and reverified");
    }
}
