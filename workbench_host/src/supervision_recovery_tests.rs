use super::super::ledger_tests::{active, clear_fact, completed, fixture};
use super::*;
fn closing(owner: &mut Owner, reused_pid: bool) {
    active(owner);
    owner.update(completed(owner.record(), false)).unwrap();
    if reused_pid {
        let mut r = owner.record();
        r.identity.as_mut().unwrap().supervisor_creation_filetime = 1;
        let bytes = codec::encode_owner(&r).unwrap();
        owner
            .db
            .execute(
                "UPDATE owner SET record=?1,digest=?2",
                params![bytes, sha_bytes(&bytes)],
            )
            .unwrap();
    }
}
fn token(p: &Preview) -> String {
    hex(&p.token)
}
#[test]
fn live_original_writer_and_each_missing_proof_refuse_recovery() {
    let (dir, host, package, mut owner) = fixture();
    closing(&mut owner, false);
    let p = preview(dir.path(), &host, &package).unwrap();
    assert_eq!(p.reason, "original_supervisor_still_live");
    assert!(recover(dir.path(), &host, &package, &token(&p), true).is_err());
    for n in 0..8 {
        let mut r = owner.record();
        clear_fact(&mut r, n);
        assert!(!complete_abnormal_proof(&r));
    }
    let mut r = owner.record();
    r.resources.as_mut().unwrap().normal_shutdown = codec::proto::ObservedTruth::Yes as i32;
    assert!(!complete_abnormal_proof(&r));
    r = owner.record();
    r.model_version = 1;
    assert!(!complete_abnormal_proof(&r));
}
#[test]
fn explicit_recovery_preserves_unknown_and_only_new_supervisor_may_claim() {
    let (dir, host, package, mut owner) = fixture();
    closing(&mut owner, true);
    let p = preview(dir.path(), &host, &package).unwrap();
    assert!(p.eligible);
    assert!(recover(dir.path(), &host, &package, &token(&p), false).is_err());
    let result = recover(dir.path(), &host, &package, &token(&p), true).unwrap();
    assert_eq!(result.record.phase, OwnerPhase::Recovered as i32);
    assert!(complete_abnormal_proof(&result.record));
    assert!(guard(&dir.path().join("library.db"), false, None).is_err());
    assert!(recover(dir.path(), &host, &package, &token(&p), true).is_err());
    assert!(owner.update(completed(owner.record(), true)).is_err());
    let next = Owner::claim(dir.path(), &host, &package).unwrap();
    assert_eq!(codec::identity(&next.record()).unwrap().generation, 2);
    assert_ne!(
        codec::identity(&next.record()).unwrap().incarnation,
        codec::identity(&result.record).unwrap().incarnation
    );
    let (bytes, digest): (Vec<u8>, Vec<u8>) = next
        .db
        .query_row(
            "SELECT original_record,original_digest FROM owner_recovery",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(digest, p.original_digest);
    assert_eq!(decode(bytes, digest).unwrap(), p.record);
}
#[test]
fn repreview_confirms_resource_recovery_without_normal_release_or_changed_artifacts() {
    let (dir, host, package, mut owner) = fixture();
    closing(&mut owner, true);
    let p = preview(dir.path(), &host, &package).unwrap();
    recover(dir.path(), &host, &package, &token(&p), true).unwrap();
    let confirmed = preview(dir.path(), &host, &package).unwrap();
    assert!(!confirmed.eligible && confirmed.resource_recovery_confirmed);
    assert_eq!(confirmed.recovery_original_digest, Some(p.original_digest));
    assert!(complete_abnormal_proof(&confirmed.record));
    std::fs::write(&package, b"changed").unwrap();
    let changed = preview(dir.path(), &host, &package).unwrap();
    assert_eq!(changed.reason, "artifact_binding_mismatch");
    assert!(!changed.resource_recovery_confirmed);
    std::fs::write(&package, b"bound package artifact").unwrap();
    let mut next = Owner::claim(dir.path(), &host, &package).unwrap();
    closing(&mut next, false);
    next.update(completed(next.record(), true)).unwrap();
    let normal = preview(dir.path(), &host, &package).unwrap();
    assert_eq!(normal.reason, "already_normal_released");
    assert!(!normal.resource_recovery_confirmed);
}
#[test]
fn new_session_cannot_release_after_archive_or_table_loss() {
    for drop_table in [false, true] {
        let (dir, host, package, mut owner) = fixture();
        closing(&mut owner, true);
        let p = preview(dir.path(), &host, &package).unwrap();
        recover(dir.path(), &host, &package, &token(&p), true).unwrap();
        let mut next = Owner::claim(dir.path(), &host, &package).unwrap();
        closing(&mut next, false);
        next.db
            .execute_batch(if drop_table {
                "DROP TABLE owner_recovery"
            } else {
                "DELETE FROM owner_recovery"
            })
            .unwrap();
        assert!(next.update(completed(next.record(), true)).is_err());
        assert!(Owner::claim(dir.path(), &host, &package).is_err());
    }
}
#[test]
fn changed_container_after_preview_never_commits_recovery() {
    let (dir, host, package, mut owner) = fixture();
    closing(&mut owner, true);
    let p = preview(dir.path(), &host, &package).unwrap();
    let mut r = p.record.clone();
    r.resources.as_mut().unwrap().child_exit_code = Some(73);
    let bytes = codec::encode_owner(&r).unwrap();
    owner
        .db
        .execute(
            "UPDATE owner SET record=?1,digest=?2",
            params![bytes, sha_bytes(&bytes)],
        )
        .unwrap();
    assert!(recover(dir.path(), &host, &package, &token(&p), true).is_err());
    assert_eq!(
        owner
            .db
            .query_row("SELECT count(*) FROM owner_recovery", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
#[test]
fn modified_or_deleted_archive_cannot_unlock_recovered_or_next_generation() {
    for deletion in [false, true] {
        let (dir, host, package, mut owner) = fixture();
        closing(&mut owner, true);
        let p = preview(dir.path(), &host, &package).unwrap();
        recover(dir.path(), &host, &package, &token(&p), true).unwrap();
        owner
            .db
            .execute_batch(if deletion {
                "DELETE FROM owner_recovery"
            } else {
                "UPDATE owner_recovery SET receipt_digest=zeroblob(32)"
            })
            .unwrap();
        assert!(preview(dir.path(), &host, &package).is_err());
        assert!(Owner::claim(dir.path(), &host, &package).is_err());
    }
}
