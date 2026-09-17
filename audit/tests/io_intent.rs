//! Real Windows protected-key sealing, archive and backup qualification for
//! synthetic IO histories. No external request or response authenticity is proved.
#![cfg(target_os = "windows")]
use morrow_audit::{
    ChainVerifier, TrustedLog,
    archive::{Append, Archive},
    session::{OpenMode, Session},
    snapshot, verify,
};
use morrow_core::{
    io_intent::{Command, ObservationSource, Record, Recovery},
    plugin_package::io,
    store::Store,
};
use std::{
    collections::BTreeSet,
    path::Path,
    process::{Command as ProcessCommand, Output},
};

fn prepared() -> Record {
    Record::prepared(Command {
        operation_id: "one-io-operation".into(),
        subject: "test.io-plugin".into(),
        package_sha256: [1; 32],
        capability: io::IoCapability::HttpRequest,
        protocol_sha256: io::schema_digest(),
        request_sha256: [2; 32],
        approval_sha256: [3; 32],
        target_sha256: [4; 32],
        request_bytes: 27,
        response_limit: 4096,
    })
    .unwrap()
}
fn cli(trust: &TrustedLog, mode: &str, path: &Path) -> Output {
    let hex: String = trust
        .key
        .to_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let mut command = ProcessCommand::new(env!("CARGO_BIN_EXE_morrow-audit-check"));
    command.arg(hex).arg(&trust.id).arg(mode).arg(path);
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
    command.output().unwrap()
}
fn assert_cli(trust: &TrustedLog, mode: &str, path: &Path) {
    let output = cli(trust, mode, path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("3 signed segments and 3 audit events")
    );
}

#[test]
fn actual_io_revisions_seal_archive_restore_and_verify_without_source_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("synthetic-source");
    std::fs::create_dir(&source).unwrap();
    let db = source.join("workbench.db");
    let backup = dir.path().join("io.morrowbackup");
    let archive_path = dir.path().join("audit-archive.db");
    let mut session = Session::open(&db, Default::default(), OpenMode::Initialize).unwrap();
    let first = prepared();
    let second = first.propose_dispatch_boundary().unwrap();
    let third = second
        .propose_observation([5; 32], ObservationSource::Reconciliation)
        .unwrap();
    let records = [&first, &second, &third];
    let ids: BTreeSet<_> = records.iter().map(|record| record.event_id()).collect();
    assert_eq!(ids.len(), 3); // Shared command ID must not collide with audit event IDs.
    let trust = session.trust();
    let mut segments = Vec::new();
    let mut chain = ChainVerifier::new(trust.clone(), None).unwrap();
    let mut archive = Archive::open(&archive_path, trust.clone(), true).unwrap();
    for (index, record) in records.into_iter().enumerate() {
        let store = session.runtime().store_local_mut();
        // The dispatch boundary requires the pre-send follow-up reservation.
        if index == 1 {
            store
                .reserve_io_intent_followup(first.command(), || Ok(()))
                .unwrap();
        }
        let committed = store
            .append_io_intent_local_authorized(record, || Ok(()))
            .unwrap();
        assert_eq!(committed.container(), record.container());
        session.flush(1).unwrap();
        let segment = session
            .store()
            .sealed_segment(index as u64 + 1)
            .unwrap()
            .unwrap();
        let verified = verify(&segment, &trust).unwrap();
        let events = &verified.segment().events;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].operation_id, record.event_id());
        assert_eq!(events[0].original_commit, record.container());
        chain.accept(&segment).unwrap();
        assert!(matches!(
            archive.append(&segment).unwrap(),
            Append::Stored(_)
        ));
        assert!(matches!(
            archive.append(&segment).unwrap(),
            Append::AlreadyStored(_)
        ));
        segments.push(segment);
    }
    assert_eq!(chain.finish().unwrap(), (3, 3));
    for record in records {
        let historical = session
            .runtime()
            .store_local_mut()
            .append_io_intent_local_authorized(record, || Ok(()))
            .unwrap();
        assert_eq!(historical.container(), record.container());
    }
    assert_eq!(session.store().pending_usage().unwrap(), (0, 0));
    session.backup_snapshot(&backup).unwrap();
    session.store().integrity_check().unwrap();
    drop(archive);
    drop(session);
    // Only this test-owned source directory is removed, including its credentials.
    assert!(source.starts_with(dir.path()) && source.file_name().unwrap() == "synthetic-source");
    std::fs::remove_dir_all(&source).unwrap();
    assert!(!db.exists());
    assert_cli(&trust, "--archive", &archive_path);
    let archived = Archive::open_read_only(&archive_path, trust.clone()).unwrap();
    let receipt = archived.check(None).unwrap().unwrap();
    assert_eq!((receipt.index, receipt.last_sequence), (3, 3));
    for (index, expected) in records.into_iter().enumerate() {
        let segment = archived.segment(index as u64 + 1).unwrap().unwrap();
        assert_eq!(segment.container(), segments[index]);
        let retained = Record::decode(&segment.segment().events[0].original_commit).unwrap();
        assert_eq!(retained.container(), expected.container());
        assert_eq!(retained.raw(), expected.raw());
        assert_eq!(retained.digest(), expected.digest());
        if index == 1 {
            assert_eq!(retained.recovery(), Recovery::ReconcileOnly);
        }
    }
    drop(archived);
    let restored_path = dir.path().join("restored");
    snapshot::restore(&backup, &restored_path).unwrap();
    let restored_db = restored_path.join("workbench.db");
    assert_cli(&trust, "--core-store", &restored_db);
    let restored = Store::open_read_only_audited(&restored_db, trust.clone()).unwrap();
    assert_eq!(
        restored
            .lookup_io_intent("test.io-plugin", "one-io-operation")
            .unwrap()
            .unwrap()
            .container(),
        third.container()
    );
    assert_eq!(restored.pending_usage().unwrap(), (0, 0));
    for (index, original) in segments.iter().enumerate() {
        assert_eq!(
            restored.sealed_segment(index as u64 + 1).unwrap().unwrap(),
            *original
        );
    }
    drop(restored);
    let wrong = TrustedLog {
        id: "different-log".into(),
        key: trust.key,
    };
    assert!(!cli(&wrong, "--archive", &archive_path).status.success());
    assert!(!cli(&wrong, "--core-store", &restored_db).status.success());
}
