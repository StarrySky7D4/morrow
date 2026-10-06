//! Real process crashes around the atomic v22 -> v23 staging audit migration.
#![cfg(all(feature = "fault-injection", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    file_content::FileContent,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    io_evidence::{Kind, Material},
    io_intent::Record,
    store::{EventBudget, Store},
};
use sha2::{Digest, Sha256};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const SUBJECT: &str = "content-test";
const OPERATION: &str = "content-crash";
const DATABASE_ENV: &str = "MORROW_FILE_RECEIPT_CRASH_DB";
fn sample() -> (Record, Material, FileContent) {
    let bytes = vec![0x74; 128 * 1024];
    let request = RequestRecord::new(MutationRequest {
        operation_id: OPERATION.into(),
        subject: SUBJECT.into(),
        package_sha256: [1; 32],
        approval_sha256: [2; 32],
        target: Target {
            reference: [3; 32],
            relative_path: None,
        },
        disposition: Disposition::Replace,
        expected_identity: Some([4; 32]),
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(&bytes).into()),
    })
    .unwrap();
    let record = Record::prepared(request.command().unwrap()).unwrap();
    let material = Material::encode(
        Kind::Request,
        OPERATION,
        SUBJECT,
        record.command().request_sha256,
        request.container(),
    )
    .unwrap();
    let content =
        FileContent::new(OPERATION, SUBJECT, record.command().request_sha256, &bytes).unwrap();
    (record, material, content)
}

#[test]
#[ignore = "child harness; parent supplies isolated v22 database and crash boundary"]
fn file_content_receipt_crash_child() {
    let path = std::env::var_os(DATABASE_ENV).unwrap();
    Store::open_existing(Path::new(&path), EventBudget::default()).unwrap();
    panic!("migration fault boundary not reached");
}
fn crash(path: &Path, point: &str) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "file_content_receipt_crash_child",
            "--ignored",
            "--nocapture",
        ])
        .env(DATABASE_ENV, path)
        .env("MORROW_TEST_CRASH_AT", point)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
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
            panic!("child timeout: {point}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn migration_exit_keeps_whole_v22_or_imports_whole_v23_without_live_staging_claim() {
    use morrow_core::file_content_receipt::Source;
    for point in [
        "file-content-receipt-migration-before-commit",
        "file-content-receipt-migration-after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("migration.db");
        let (record, material, content) = sample();
        let mut store = Store::open(&path, EventBudget::default()).unwrap();
        store
            .prepare_file_mutation_local_authorized(&record, &material, || Ok(()))
            .unwrap();
        let before = store.pending(0, 10).unwrap();
        drop(store);
        // Honest v22 retained content: no receipt namespace or fabricated kind-6 event.
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute_batch("DROP TABLE IF EXISTS agent_ledger; DROP TABLE IF EXISTS channel_ack_receipts; DROP TABLE IF EXISTS channel_checkpoints; DROP TABLE file_content_receipts; PRAGMA user_version=22;")
            .unwrap();
        sql.execute("INSERT INTO file_mutation_content(operation_id,subject,request_sha256,content_sha256,content_length,container) VALUES(?1,?2,?3,?4,?5,?6)",rusqlite::params![OPERATION,SUBJECT,content.request_sha256().as_slice(),content.content_sha256().as_slice(),content.content().len() as i64,content.container()]).unwrap();
        drop(sql);
        crash(&path, point);
        let committed = point.ends_with("after-commit");
        let sql = rusqlite::Connection::open(&path).unwrap();
        let version: i64 = sql
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, if committed { 23 } else { 22 });
        let receipts: i64 = sql
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name='file_content_receipts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(receipts, i64::from(committed));
        let events: i64 = sql
            .query_row("SELECT count(*) FROM operations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(events, 1 + i64::from(committed));
        let retained: Vec<u8> = sql
            .query_row("SELECT container FROM file_mutation_content", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(retained, content.container());
        drop(sql);
        let mut store = Store::open_existing(&path, EventBudget::default()).unwrap();
        let receipt = store
            .file_mutation_content_receipt_local_authorized(SUBJECT, OPERATION, || Ok(()))
            .unwrap()
            .unwrap();
        assert_eq!(receipt.source(), Source::LegacyImport);
        let after = store.pending(0, 10).unwrap();
        assert_eq!(after.len(), before.len() + 1);
        assert_eq!(&after[..before.len()], before.as_slice());
        store
            .stage_file_mutation_content_local_authorized(&content, || Ok(()))
            .unwrap();
        assert_eq!(store.pending(0, 10).unwrap(), after);
        assert!(matches!(
            store.claim_io_dispatch_local_authorized(
                &record.propose_dispatch_boundary().unwrap(),
                || Ok(())
            ),
            Err(Error::UnsupportedVersion)
        ));
        store.integrity_check().unwrap();
    }
}
