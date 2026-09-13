#![cfg(target_os = "windows")]
use morrow_audit::{
    ChainVerifier, TrustedLog,
    archive::Archive,
    session::{OpenMode, Session, key_path},
    snapshot, verify,
};
use morrow_core::{content::CardRecord, read_journal::Input, store::Store};
use std::{
    path::Path,
    process::{Command, Output},
};
fn open(db: &Path) -> Session {
    Session::open(db, Default::default(), OpenMode::Initialize).unwrap()
}
fn read(session: &mut Session, operation: &str) -> Vec<u8> {
    // A real trusted local read of the synthetic card; this does not claim plugin execution.
    let card = session.store().card("card").unwrap().unwrap();
    let input = Input {
        operation_id: operation.into(),
        subject: "test.card-reader".into(),
        request_type: "test.read-card.v1".into(),
        request: b"card".to_vec(),
        response_type: "morrow.card.v1".into(),
        response: card.encode(),
    };
    session
        .runtime()
        .store_local_mut()
        .record_read_local_authorized(&input, &[], || Ok(()))
        .unwrap();
    session
        .store()
        .lookup_read("test.card-reader", operation)
        .unwrap()
        .unwrap()
        .container()
        .to_vec()
}
fn cli(trust: &TrustedLog, mode: &str, path: &Path) -> Output {
    let hex: String = trust
        .key
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-audit-check"));
    command.arg(hex).arg(&trust.id).arg(mode).arg(path);
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}
#[test]
fn real_local_reads_join_the_same_signed_chain_without_changing_card_revisions() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut session = open(&db);
    let card = CardRecord::new("card", "test.raw", 1, "original", vec![0, 255, 3]).unwrap();
    session
        .runtime()
        .store_local_mut()
        .create_local("create", &card)
        .unwrap();
    let first = read(&mut session, "read-one");
    assert_eq!(
        session.store().card("card").unwrap().unwrap().encode(),
        card.encode()
    );
    session.flush(16).unwrap();
    let segment_one = session.store().sealed_segment(1).unwrap().unwrap();
    let trust = session.trust();
    let verified = verify(&segment_one, &trust).unwrap();
    assert_eq!(verified.segment().events.len(), 2);
    assert_eq!(verified.segment().events[1].original_commit, first);
    let second = read(&mut session, "read-two");
    session.flush(16).unwrap();
    let segment_two = session.store().sealed_segment(2).unwrap().unwrap();
    assert_eq!(
        verify(&segment_two, &trust).unwrap().segment().events[0].original_commit,
        second
    );
    let mut chain = ChainVerifier::new(trust.clone(), None).unwrap();
    chain.accept(&segment_one).unwrap();
    chain.accept(&segment_two).unwrap();
    assert_eq!(chain.finish().unwrap(), (2, 3));
    session.store().integrity_check().unwrap();
    drop(session);
    let output = cli(&trust, "--core-store", &db);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("2 signed segments and 3 audit events")
    );
    let retained = Store::open_read_only_audited(&db, trust).unwrap();
    assert_eq!(
        retained
            .lookup_read("test.card-reader", "read-one")
            .unwrap()
            .unwrap()
            .container(),
        first
    );
    assert_eq!(
        retained.card("card").unwrap().unwrap().summary().revision,
        1
    );
}
#[test]
fn snapshot_restores_pending_and_sealed_read_originals_and_archive_verifies_without_source() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let mut session = open(&db);
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "create",
            &CardRecord::new("card", "test.raw", 1, "original", vec![7]).unwrap(),
        )
        .unwrap();
    let first = read(&mut session, "sealed-read");
    session.flush(16).unwrap();
    let sealed = session.store().sealed_segment(1).unwrap().unwrap();
    let trust = session.trust();
    let second = read(&mut session, "pending-read");
    let key = std::fs::read(key_path(&db).unwrap()).unwrap();
    let backup = dir.path().join("snapshot.morrowbackup");
    session.backup_snapshot(&backup).unwrap();
    assert_eq!(session.store().pending_usage().unwrap().0, 1);
    let archive_path = dir.path().join("independent-audit.db");
    let mut archive = Archive::open(&archive_path, trust.clone(), true).unwrap();
    archive.append(&sealed).unwrap();
    drop(archive);
    drop(session);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    let output = cli(&trust, "--archive", &archive_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let target = dir.path().join("restored");
    snapshot::restore(&backup, &target).unwrap();
    let restored_db = target.join("workbench.db");
    let mut restored = Session::open(&restored_db, Default::default(), OpenMode::Existing).unwrap();
    assert_eq!(std::fs::read(key_path(&restored_db).unwrap()).unwrap(), key);
    assert_eq!(
        restored
            .store()
            .lookup_read("test.card-reader", "sealed-read")
            .unwrap()
            .unwrap()
            .container(),
        first
    );
    assert_eq!(
        restored
            .store()
            .lookup_read("test.card-reader", "pending-read")
            .unwrap()
            .unwrap()
            .container(),
        second
    );
    assert_eq!(restored.store().pending_usage().unwrap().0, 1);
    assert_eq!(restored.store().sealed_segment(1).unwrap().unwrap(), sealed);
    assert!(
        restored
            .store()
            .read_evidence("test.card-reader", "sealed-read")
            .unwrap()
            .unwrap()
            .is_empty()
    );
    restored.flush(16).unwrap();
    restored.store().integrity_check().unwrap();
    assert_eq!(
        restored
            .store()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .revision,
        1
    );
    drop(restored);
    let output = cli(&trust, "--core-store", &restored_db);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn signed_read_operation_with_changed_subject_index_is_rejected_by_snapshot_and_cli() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut session = open(&db);
    session
        .runtime()
        .store_local_mut()
        .create_local(
            "create",
            &CardRecord::new("card", "test.raw", 1, "original", vec![]).unwrap(),
        )
        .unwrap();
    read(&mut session, "read");
    session.flush(16).unwrap();
    let trust = session.trust();
    drop(session);
    let sql = rusqlite::Connection::open(&db).unwrap();
    sql.execute(
        "UPDATE operations SET card_id='another.reader' WHERE id='read'",
        [],
    )
    .unwrap();
    drop(sql);
    assert!(Store::open_read_only_audited(&db, trust.clone()).is_err());
    assert!(Session::open(&db, Default::default(), OpenMode::Existing).is_err());
    assert!(!cli(&trust, "--core-store", &db).status.success());
}
