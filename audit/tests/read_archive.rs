#![cfg(target_os = "windows")]
use morrow_audit::{
    ChainVerifier, TrustedLog,
    archive::Archive,
    session::{OpenMode, Session, key_path},
    snapshot, verify,
};
use morrow_core::{
    read_archive::{Budget, Finish, Plan},
    store::Store,
};
use std::{
    path::Path,
    process::{Command, Output},
};
const SUBJECT: &str = "test.archive-reader";
fn open(db: &Path) -> Session {
    Session::open(db, Default::default(), OpenMode::Initialize).unwrap()
}
fn cli(trust: &TrustedLog, mode: &str, path: &Path) -> Output {
    let public: String = trust
        .key
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut command = Command::new(env!("CARGO_BIN_EXE_morrow-audit-check"));
    command.arg(public).arg(&trust.id).arg(mode).arg(path);
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}
fn plan(operation: &str) -> Plan {
    Plan {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        request_type: "test.archive.request.v1".into(),
        request: b"frozen request".to_vec(),
        response_type: "test.archive.result.v1".into(),
        budget: Budget {
            max_parts: 8,
            max_bytes: 1024 * 1024,
        },
    }
}
fn prepare(session: &mut Session, operation: &str) -> Vec<Vec<u8>> {
    let store = session.runtime().store_local_mut();
    store.begin_read_archive(&plan(operation)).unwrap();
    let values = [
        b"first original page".to_vec(),
        (0..65536).map(|i| (i % 251) as u8).collect(),
        b"last original page".to_vec(),
    ];
    let mut containers = Vec::new();
    for (i, data) in values.iter().enumerate() {
        store
            .append_read_archive(SUBJECT, operation, i as u32, "test.page.v1", data)
            .unwrap();
        let part = store
            .read_archive_part(SUBJECT, operation, i as u32)
            .unwrap()
            .unwrap();
        assert_eq!(part.data(), data);
        assert_eq!(part.ordinal(), i as u32);
        containers.push(part.container().to_vec());
    }
    let status = store
        .lookup_read_archive(SUBJECT, operation)
        .unwrap()
        .unwrap();
    assert_eq!(status.count, 3);
    assert!(
        status.root.is_none(),
        "prepared fragments are not published reading evidence"
    );
    assert!(store.lookup_read(SUBJECT, operation).unwrap().is_none());
    containers
}
fn publish(session: &mut Session, operation: &str) -> Vec<u8> {
    let store = session.runtime().store_local_mut();
    let status = store
        .lookup_read_archive(SUBJECT, operation)
        .unwrap()
        .unwrap();
    let finished = Finish {
        response: b"ordered result".to_vec(),
        part_count: status.count,
        logical_bytes: status.logical_bytes,
        chain_sha256: status.chain_sha256,
        metadata_type: "test.archive.metadata.v1".into(),
        metadata: b"host facts; no plugin execution claim".to_vec(),
    };
    let receipt = store
        .finish_read_archive_local_authorized(SUBJECT, operation, &finished, &[], || Ok(()))
        .unwrap();
    let observed = store.lookup_read(SUBJECT, operation).unwrap().unwrap();
    assert_eq!(receipt.observation_sha256, observed.digest());
    let manifest = store
        .read_archive_manifest(SUBJECT, operation)
        .unwrap()
        .unwrap();
    assert_eq!(
        observed.data().archive_sha256.as_slice(),
        manifest.digest().as_slice()
    );
    assert_eq!(manifest.metadata_type(), Some("test.archive.metadata.v1"));
    assert!(
        store
            .lookup_read_archive(SUBJECT, operation)
            .unwrap()
            .unwrap()
            .root
            .is_some()
    );
    observed.container().to_vec()
}
fn assert_parts(store: &Store, operation: &str, expected: &[Vec<u8>]) {
    for (i, bytes) in expected.iter().enumerate() {
        let part = store
            .read_archive_part(SUBJECT, operation, i as u32)
            .unwrap()
            .unwrap();
        assert_eq!(part.container(), bytes);
    }
    assert!(
        store
            .read_archive_part(SUBJECT, operation, expected.len() as u32)
            .unwrap()
            .is_none()
    );
}
#[test]
fn complete_archive_publishes_once_and_joins_real_ed25519_chain_and_cli() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("workbench.db");
    let mut session = open(&db);
    let parts = prepare(&mut session, "published");
    assert_eq!(session.store().pending_usage().unwrap(), (0, 0));
    let original = publish(&mut session, "published");
    assert_eq!(session.store().pending_usage().unwrap().0, 1);
    assert_eq!(publish(&mut session, "published"), original);
    assert_eq!(session.store().pending_usage().unwrap().0, 1);
    session.flush(16).unwrap();
    let trust = session.trust();
    let signed = session.store().sealed_segment(1).unwrap().unwrap();
    let verified = verify(&signed, &trust).unwrap();
    assert_eq!(verified.segment().events.len(), 1);
    assert_eq!(verified.segment().events[0].original_commit, original);
    assert_parts(session.store(), "published", &parts);
    session.store().integrity_check().unwrap();
    drop(session);
    let output = cli(&trust, "--core-store", &db);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 signed segments and 1 audit events")
    );
    let retained = Store::open_read_only_audited(&db, trust).unwrap();
    assert_parts(&retained, "published", &parts);
    assert_eq!(
        retained
            .lookup_read(SUBJECT, "published")
            .unwrap()
            .unwrap()
            .container(),
        original
    );
}
#[test]
fn backup_restores_prepared_published_and_sealed_states_after_source_removal() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let mut session = open(&db);
    let sealed_parts = prepare(&mut session, "sealed");
    let sealed_root = publish(&mut session, "sealed");
    session.flush(16).unwrap();
    let signed = session.store().sealed_segment(1).unwrap().unwrap();
    let trust = session.trust();
    let pending_parts = prepare(&mut session, "prepared");
    let ready_parts = prepare(&mut session, "published-unsealed");
    let ready_root = publish(&mut session, "published-unsealed");
    assert_eq!(session.store().pending_usage().unwrap().0, 1);
    let encrypted_key = std::fs::read(key_path(&db).unwrap()).unwrap();
    let backup = dir.path().join("snapshot.morrowbackup");
    session.backup_snapshot(&backup).unwrap();
    let archive_path = dir.path().join("signed-roots.db");
    let mut archive = Archive::open(&archive_path, trust.clone(), true).unwrap();
    archive.append(&signed).unwrap();
    drop(archive);
    drop(session);
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "source");
    std::fs::remove_dir_all(&source).unwrap();
    assert!(cli(&trust, "--archive", &archive_path).status.success());
    // The independent signed archive retains the root event; complete parts are in the backup.
    let restored_dir = dir.path().join("restored");
    snapshot::restore(&backup, &restored_dir).unwrap();
    let restored_db = restored_dir.join("workbench.db");
    let mut restored = Session::open(&restored_db, Default::default(), OpenMode::Existing).unwrap();
    assert_eq!(
        std::fs::read(key_path(&restored_db).unwrap()).unwrap(),
        encrypted_key
    );
    assert_parts(restored.store(), "sealed", &sealed_parts);
    assert_parts(restored.store(), "prepared", &pending_parts);
    assert_parts(restored.store(), "published-unsealed", &ready_parts);
    assert!(
        restored
            .store()
            .lookup_read_archive(SUBJECT, "prepared")
            .unwrap()
            .unwrap()
            .root
            .is_none()
    );
    assert!(
        restored
            .store()
            .lookup_read(SUBJECT, "prepared")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        restored
            .store()
            .lookup_read(SUBJECT, "sealed")
            .unwrap()
            .unwrap()
            .container(),
        sealed_root
    );
    assert_eq!(
        restored
            .store()
            .lookup_read(SUBJECT, "published-unsealed")
            .unwrap()
            .unwrap()
            .container(),
        ready_root
    );
    assert_eq!(restored.store().pending_usage().unwrap().0, 1);
    assert_eq!(restored.store().sealed_segment(1).unwrap().unwrap(), signed);
    restored.flush(16).unwrap();
    let second = restored.store().sealed_segment(2).unwrap().unwrap();
    let mut chain = ChainVerifier::new(trust.clone(), None).unwrap();
    chain.accept(&signed).unwrap();
    chain.accept(&second).unwrap();
    assert_eq!(chain.finish().unwrap(), (2, 2));
    assert!(
        restored
            .store()
            .lookup_read(SUBJECT, "prepared")
            .unwrap()
            .is_none()
    );
    restored.store().integrity_check().unwrap();
    drop(restored);
    assert!(cli(&trust, "--core-store", &restored_db).status.success());
}

#[test]
fn corrupted_root_part_or_owner_is_rejected_by_audited_open_and_real_cli() {
    for damage in ["root", "part", "owner"] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("workbench.db");
        let mut session = open(&db);
        prepare(&mut session, "published");
        publish(&mut session, "published");
        session.flush(16).unwrap();
        let trust = session.trust();
        let signed = session.store().sealed_segment(1).unwrap().unwrap();
        drop(session);
        let sql = rusqlite::Connection::open(&db).unwrap();
        match damage {
            "root" => {
                let mut root: Vec<u8> = sql
                    .query_row(
                        "SELECT root FROM operation_read_archives WHERE operation_id='published'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap();
                root[0] ^= 1;
                assert_eq!(
                    sql.execute(
                        "UPDATE operation_read_archives SET root=?1 WHERE operation_id='published'",
                        [root]
                    )
                    .unwrap(),
                    1
                );
            }
            "part" => {
                let mut part: Vec<u8> = sql.query_row("SELECT payload FROM read_archive_parts WHERE operation_id='published' AND ordinal=1", [], |r| r.get(0)).unwrap();
                let last = part.len() - 1;
                part[last] ^= 1;
                assert_eq!(sql.execute("UPDATE read_archive_parts SET payload=?1 WHERE operation_id='published' AND ordinal=1", [part]).unwrap(), 1);
            }
            "owner" => {
                assert_eq!(sql.execute("UPDATE read_archives SET subject='another.reader' WHERE operation_id='published'", []).unwrap(), 1);
            }
            _ => unreachable!(),
        }
        drop(sql);
        // Original signed bytes still verify. They do not make damaged linked storage valid.
        verify(&signed, &trust).unwrap();
        assert!(
            Store::open_read_only_audited(&db, trust.clone()).is_err(),
            "{damage}"
        );
        assert!(
            Session::open(&db, Default::default(), OpenMode::Existing).is_err(),
            "{damage}"
        );
        assert!(
            !cli(&trust, "--core-store", &db).status.success(),
            "{damage}"
        );
    }
}
