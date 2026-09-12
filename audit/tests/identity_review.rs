#![cfg(target_os = "windows")]
//! Independent regression of the documented boundary: ownership is not a head registry.
use morrow_audit::{
    session::{OpenMode, Session},
    snapshot,
};
use morrow_core::{
    content::CardRecord,
    store::{EventBudget, Store},
};
use std::path::Path;

fn open(path: &Path) -> Session {
    Session::open(path, EventBudget::default(), OpenMode::Initialize).unwrap()
}
fn write(session: &mut Session, id: &str) {
    session
        .runtime()
        .store_local_mut()
        .create_local(id, &CardRecord::new(id, "text", 1, id, vec![1]).unwrap())
        .unwrap();
    assert!(!session.flush(16).unwrap().more_pending);
}
#[test]
fn sequential_snapshot_branches_verify_locally_without_independent_newer_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("workbench.db");
    let mut first = open(&original);
    write(&mut first, "common");
    let trust = first.trust();
    let archive = root.path().join("archive");
    first.backup_snapshot(&archive).unwrap();
    let copy_dir = root.path().join("copy");
    snapshot::restore(&archive, &copy_dir).unwrap();
    write(&mut first, "original-only");
    let first_tip = first.store().last_sealed_segment().unwrap().unwrap();
    drop(first);

    // A closed owner does not retain a "latest history" claim in test.23.
    let copied = copy_dir.join("workbench.db");
    let mut second = open(&copied);
    assert!(second.store().card("original-only").unwrap().is_none());
    write(&mut second, "copy-only");
    let second_tip = second.store().last_sealed_segment().unwrap().unwrap();
    drop(second);

    let a = morrow_audit::verify(&first_tip, &trust).unwrap();
    let b = morrow_audit::verify(&second_tip, &trust).unwrap();
    assert_eq!(a.segment().index, b.segment().index);
    assert_ne!(a.digest(), b.digest());
    let original = Store::open_read_only_audited(&original, trust.clone()).unwrap();
    let copied = Store::open_read_only_audited(&copied, trust).unwrap();
    assert!(original.card("original-only").unwrap().is_some());
    assert!(original.card("copy-only").unwrap().is_none());
    assert!(copied.card("copy-only").unwrap().is_some());
    assert!(copied.card("original-only").unwrap().is_none());
}
