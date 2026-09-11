#![cfg(all(not(target_arch = "wasm32"), feature = "fault-injection"))]
use morrow_core::{
    attachment::BlobInfo,
    content::{Attachment, CardRecord},
    store::{EventBudget, Store},
};
use std::{io::Cursor, path::Path, process::Command};
const EXE: &str = env!("CARGO_BIN_EXE_morrow-core-store");
fn call(db: &Path, action: &str, args: &[&str], crash: Option<&str>) -> std::process::Output {
    let mut c = Command::new(EXE);
    c.arg(action).arg(db).args(args);
    if let Some(point) = crash {
        c.env("MORROW_TEST_CRASH_AT", point);
    }
    c.output().unwrap()
}
fn seed(db: &Path) -> BlobInfo {
    let mut store = Store::open(db, EventBudget::default()).unwrap();
    store
        .stage_blob(&mut Cursor::new(b"original"), 8, None, 0)
        .unwrap()
}
fn card(file: &BlobInfo) -> CardRecord {
    CardRecord::new_with_attachments(
        "card",
        "type",
        1,
        "title",
        vec![],
        &[Attachment {
            id: "attachment-1".into(),
            display_name: "raw".into(),
            media_type: "application/octet-stream".into(),
            sha256: file.sha256,
            byte_length: file.byte_length,
        }],
    )
    .unwrap()
}
#[test]
fn stage_recovers_at_four_exit_boundaries() {
    for point in [
        "stage-after-allocation",
        "stage-after-chunk",
        "stage-before-commit",
        "stage-after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let source = dir.path().join("raw");
        let bytes = vec![29; 100_000];
        std::fs::write(&source, &bytes).unwrap();
        Store::open(&db, EventBudget::default()).unwrap();
        let result = call(
            &db,
            "stage-file-local",
            &[source.to_str().unwrap()],
            Some(point),
        );
        assert_eq!(result.status.code(), Some(86), "{point}: {result:?}");
        assert!(call(&db, "check", &[], None).status.success());
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        let entries = store.list_blobs_local("", 128).unwrap();
        assert_eq!(entries.len(), usize::from(point == "stage-after-commit"));
        if let Some(info) = entries.first() {
            let mut raw = vec![];
            store.export_blob_local(&info.id, &mut raw).unwrap();
            assert_eq!(raw, bytes);
        }
        drop(store);
        assert!(
            call(&db, "stage-file-local", &[source.to_str().unwrap()], None)
                .status
                .success()
        );
        assert!(
            call(&db, "stage-file-local", &[source.to_str().unwrap()], None)
                .status
                .success()
        );
        assert_eq!(
            Store::open_existing(&db, EventBudget::default())
                .unwrap()
                .list_blobs_local("", 128)
                .unwrap()
                .len(),
            1
        );
    }
}
#[test]
fn attachment_publication_recovers_at_six_exit_boundaries() {
    for point in [
        "after-card",
        "after-operation",
        "after-event",
        "after-blob-references",
        "before-commit",
        "after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let file = seed(&db);
        let args = ["create", "card", "title", &file.id, "raw"];
        let result = call(&db, "create-attachment-local", &args, Some(point));
        assert_eq!(result.status.code(), Some(86), "{point}: {result:?}");
        assert!(call(&db, "check", &[], None).status.success());
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(
            store.card("card").unwrap().is_some(),
            point == "after-commit"
        );
        assert_eq!(
            store.pending(0, 10).unwrap().len(),
            usize::from(point == "after-commit")
        );
        drop(store);
        assert!(
            call(&db, "create-attachment-local", &args, None)
                .status
                .success()
        );
        assert!(
            call(&db, "create-attachment-local", &args, None)
                .status
                .success()
        );
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(store.pending(0, 10).unwrap().len(), 1);
        let mut raw = vec![];
        store
            .export_attachment_local("card", "attachment-1", &mut raw)
            .unwrap();
        assert_eq!(raw, b"original");
    }
}
#[test]
fn attachment_removal_retains_history_at_six_exit_boundaries() {
    for point in [
        "after-card",
        "after-operation",
        "after-event",
        "after-blob-references",
        "before-commit",
        "after-commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let file = seed(&db);
        Store::open_existing(&db, EventBudget::default())
            .unwrap()
            .create_local("create", &card(&file))
            .unwrap();
        let args = ["remove", "card", "1"];
        let result = call(&db, "clear-attachments-local", &args, Some(point));
        assert_eq!(result.status.code(), Some(86), "{point}: {result:?}");
        assert!(call(&db, "check", &[], None).status.success());
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(
            store.card("card").unwrap().unwrap().has_attachments(),
            point != "after-commit"
        );
        drop(store);
        assert!(
            call(&db, "clear-attachments-local", &args, None)
                .status
                .success()
        );
        assert!(
            call(&db, "clear-attachments-local", &args, None)
                .status
                .success()
        );
        let mut store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(store.pending(0, 10).unwrap().len(), 2);
        assert!(store.retire_blob_local(&file.id, 1).is_err());
        let mut raw = vec![];
        store.export_blob_local(&file.id, &mut raw).unwrap();
        assert_eq!(raw, b"original");
    }
}
#[test]
fn garbage_collection_is_atomic_at_three_exit_boundaries() {
    for point in ["gc-after-delete", "gc-before-commit", "gc-after-commit"] {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("db");
        let file = seed(&db);
        Store::open_existing(&db, EventBudget::default())
            .unwrap()
            .retire_blob_local(&file.id, 1)
            .unwrap();
        let result = call(
            &db,
            "collect-retired-local",
            &["60001", "60000"],
            Some(point),
        );
        assert_eq!(result.status.code(), Some(86), "{point}: {result:?}");
        assert!(call(&db, "check", &[], None).status.success());
        let store = Store::open_existing(&db, EventBudget::default()).unwrap();
        assert_eq!(
            store.blob_info_local(&file.id).is_ok(),
            point != "gc-after-commit"
        );
        drop(store);
        assert!(
            call(&db, "collect-retired-local", &["60001", "60000"], None)
                .status
                .success()
        );
        assert!(
            Store::open_existing(&db, EventBudget::default())
                .unwrap()
                .list_blobs_local("", 128)
                .unwrap()
                .is_empty()
        );
    }
}
