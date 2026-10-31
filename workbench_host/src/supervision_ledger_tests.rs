use super::*;
pub(super) fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, Owner) {
    let dir = tempfile::tempdir().unwrap();
    let host = dir.path().join("host.bin");
    let package = dir.path().join("package.bin");
    std::fs::write(&host, b"bound host artifact").unwrap();
    std::fs::write(&package, b"bound package artifact").unwrap();
    let owner = Owner::claim(dir.path(), &host, &package).unwrap();
    (dir, host, package, owner)
}
pub(super) fn active(owner: &mut Owner) {
    let mut r = owner.record();
    r.phase = OwnerPhase::Active as i32;
    let id = r.identity.as_mut().unwrap();
    id.child_pid = 101;
    id.child_creation_filetime = 1;
    owner.update(r).unwrap();
}
pub(super) fn closing(owner: &mut Owner) {
    let mut r = owner.record();
    r.phase = OwnerPhase::Closing as i32;
    r.resources.as_mut().unwrap().gate_closed = true;
    owner.update(r).unwrap();
}
pub(super) fn completed(mut r: OwnerRecord, normal: bool) -> OwnerRecord {
    r.phase = if normal {
        OwnerPhase::Released
    } else {
        OwnerPhase::Closing
    } as i32;
    r.resources = Some(ResourceFacts {
        resource_reclaimed: true,
        gate_closed: true,
        tree_empty: true,
        child_exit: true,
        stdout_eof: true,
        stderr_eof: true,
        control_reaped: true,
        business_gate_revoked: !normal,
        normal_shutdown: if normal {
            codec::proto::ObservedTruth::Yes
        } else {
            codec::proto::ObservedTruth::No
        } as i32,
        child_exit_code: Some(if normal { 0 } else { 72 }),
    });
    r
}
pub(super) fn clear_fact(r: &mut OwnerRecord, index: usize) {
    let f = r.resources.as_mut().unwrap();
    match index {
        0 => f.resource_reclaimed = false,
        1 => f.gate_closed = false,
        2 => f.tree_empty = false,
        3 => f.child_exit = false,
        4 => f.stdout_eof = false,
        5 => f.stderr_eof = false,
        6 => f.control_reaped = false,
        7 => f.business_gate_revoked = false,
        _ => unreachable!(),
    }
}
#[test]
fn concurrent_claims_and_direct_host_cannot_take_active_owner() {
    let (dir, host, package, mut owner) = fixture();
    active(&mut owner);
    let before = owner.encoded.clone();
    assert!(Owner::claim(dir.path(), &host, &package).is_err());
    assert!(guard(&dir.path().join("library.db"), false, None).is_err());
    readback(&owner.db, &before).unwrap();
}
#[test]
fn release_requires_complete_normal_proof_then_generation_increases() {
    let (dir, host, package, mut owner) = fixture();
    active(&mut owner);
    closing(&mut owner);
    let mut missing = owner.record();
    missing.phase = OwnerPhase::Released as i32;
    assert!(owner.update(missing).is_err());
    let full = completed(owner.record(), true);
    for n in 0..7 {
        let mut missing = full.clone();
        clear_fact(&mut missing, n);
        assert!(owner.update(missing).is_err());
    }
    for truth in [
        codec::proto::ObservedTruth::No,
        codec::proto::ObservedTruth::TruthUnspecified,
    ] {
        let mut missing = full.clone();
        missing.resources.as_mut().unwrap().normal_shutdown = truth as i32;
        assert!(owner.update(missing).is_err());
    }
    for code in [None, Some(2)] {
        let mut missing = full.clone();
        missing.resources.as_mut().unwrap().child_exit_code = code;
        assert!(owner.update(missing).is_err());
    }
    owner.update(full).unwrap();
    drop(owner);
    let next = Owner::claim(dir.path(), &host, &package).unwrap();
    assert_eq!(codec::identity(&next.record()).unwrap().generation, 2);
}
#[test]
fn deleted_owner_row_and_missing_ledger_fail_closed() {
    let (dir, host, package, owner) = fixture();
    owner.db.execute("DELETE FROM owner", []).unwrap();
    drop(owner);
    assert!(Owner::claim(dir.path(), &host, &package).is_err());
    std::fs::remove_file(dir.path().join("workbench-supervisor.sqlite")).unwrap();
    assert!(Owner::claim(dir.path(), &host, &package).is_err());
}
#[test]
fn changed_binding_digest_or_schema_cannot_persist_release() {
    for mutation in 0..3 {
        let (dir, host, package, mut owner) = fixture();
        active(&mut owner);
        let before = owner.record();
        match mutation {
            0 => {
                let mut r = before.clone();
                r.identity.as_mut().unwrap().generation = 9;
                let bytes = codec::encode_owner(&r).unwrap();
                owner
                    .db
                    .execute(
                        "UPDATE owner SET record=?1,digest=?2",
                        params![bytes, sha_bytes(&bytes)],
                    )
                    .unwrap();
            }
            1 => {
                owner
                    .db
                    .execute("UPDATE owner SET digest=zeroblob(32)", [])
                    .unwrap();
            }
            _ => {
                owner.db.execute_batch("CREATE TRIGGER injected AFTER UPDATE ON owner BEGIN UPDATE owner SET digest=zeroblob(32); END;").unwrap();
            }
        }
        let mut r = before;
        r.phase = OwnerPhase::Closing as i32;
        assert!(owner.update(r).is_err());
        assert!(Owner::claim(dir.path(), &host, &package).is_err());
    }
}
#[test]
fn changed_or_removed_marker_cannot_become_fresh_owner() {
    let (dir, host, package, owner) = fixture();
    drop(owner);
    std::fs::write(dir.path().join(MARKER_NAME), b"replacement").unwrap();
    assert!(Owner::claim(dir.path(), &host, &package).is_err());
    std::fs::remove_file(dir.path().join(MARKER_NAME)).unwrap();
    assert!(Owner::claim(dir.path(), &host, &package).is_err());
}
#[test]
fn legacy_record_is_not_opened_migrated_or_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join(LEGACY_MARKER_NAME);
    let db = dir.path().join("workbench-supervisor.sqlite");
    let bytes = b"retained legacy ledger; not even an SQLite database";
    std::fs::write(&db, bytes).unwrap();
    std::fs::write(&marker, b"Morrow supervised workbench v1\n").unwrap();
    assert!(
        Owner::claim(
            dir.path(),
            &dir.path().join("absent-host"),
            &dir.path().join("absent-package")
        )
        .is_err()
    );
    assert!(guard(&dir.path().join("library.db"), false, None).is_err());
    assert_eq!(std::fs::read(db).unwrap(), bytes);
    assert!(!dir.path().join(MARKER_NAME).exists());
}
#[test]
fn binary_storage_and_exact_cas_refuse_equivalent_container_tampering() {
    let (_dir, _host, _package, mut owner) = fixture();
    active(&mut owner);
    let types: (String, String) = owner
        .db
        .query_row("SELECT typeof(record),typeof(digest) FROM owner", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(types, ("blob".into(), "blob".into()));
    let mut changed = owner.encoded.clone();
    changed.push(0);
    owner
        .db
        .execute(
            "UPDATE owner SET record=?1,digest=?2",
            params![changed, sha_bytes(&changed)],
        )
        .unwrap();
    let mut r = owner.record();
    r.phase = OwnerPhase::Closing as i32;
    assert!(owner.update(r).is_err());
}
