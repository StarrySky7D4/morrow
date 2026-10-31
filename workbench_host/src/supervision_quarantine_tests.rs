use super::super::ledger_tests::{active, clear_fact, completed, fixture};
use super::super::{params, sha_bytes};
use super::*;
fn ready(owner: &Owner) -> OwnerRecord {
    let mut r = completed(owner.record(), false);
    r.reader_joins = Some(codec::ReaderJoins {
        version: 1,
        stdout_reader_joined: true,
        stderr_reader_joined: true,
        controller_reader_joined: true,
    });
    r
}
#[test]
fn committed_abnormal_token_retains_unknown_and_preserves_exact_digest() {
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let r = ready(&owner);
    let prepared = owner
        .prepare_abnormal_resource_closure(r.clone())
        .unwrap()
        .into_extension();
    assert_eq!(owner.record(), r);
    assert_eq!(
        prepared.committed_digest.as_slice(),
        sha_bytes(&codec::encode_owner(&r).unwrap())
    );
    assert_eq!(prepared.identity, *codec::identity(&r).unwrap());
    assert_eq!(prepared.resources, *codec::facts(&r).unwrap());
    assert!(codec::complete_abnormal_proof(&owner.record()));
}
#[test]
fn every_missing_resource_or_join_refuses_before_write() {
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let before = owner.encoded.clone();
    let full = ready(&owner);
    for n in 0..8 {
        let mut r = full.clone();
        clear_fact(&mut r, n);
        assert!(owner.prepare_abnormal_resource_closure(r).is_err());
    }
    for n in 0..4 {
        let mut r = full.clone();
        match n {
            0 => r.reader_joins = None,
            1 => r.reader_joins.as_mut().unwrap().stdout_reader_joined = false,
            2 => r.reader_joins.as_mut().unwrap().stderr_reader_joined = false,
            _ => r.reader_joins.as_mut().unwrap().controller_reader_joined = false,
        };
        assert!(owner.prepare_abnormal_resource_closure(r).is_err());
    }
    assert_eq!(owner.encoded, before);
    readback(&owner.db, &before).unwrap();
}
#[test]
fn changed_identity_and_normal_success_cannot_prepare_abnormal_token() {
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let full = ready(&owner);
    let before = owner.encoded.clone();
    for n in 0..11 {
        let mut changed = full.clone();
        let id = changed.identity.as_mut().unwrap();
        match n {
            0 => id.profile = vec![11; 32],
            1 => id.generation += 1,
            2 => id.incarnation = vec![12; 32],
            3 => id.host_sha256 = vec![13; 32],
            4 => id.package_sha256 = vec![14; 32],
            5 => id.supervisor_sha256 = vec![15; 32],
            6 => id.supervisor_pid += 1,
            7 => id.supervisor_creation_filetime += 1,
            8 => id.child_pid += 1,
            9 => id.child_creation_filetime += 1,
            _ => id.recovery_history_head = vec![16; 32],
        }
        assert!(owner.prepare_abnormal_resource_closure(changed).is_err());
        readback(&owner.db, &before).unwrap();
    }
    let mut legacy = full.clone();
    legacy.model_version = 1;
    assert!(owner.prepare_abnormal_resource_closure(legacy).is_err());
    let mut changed = full.clone();
    changed.resources.as_mut().unwrap().normal_shutdown = codec::proto::ObservedTruth::Yes as i32;
    assert!(owner.prepare_abnormal_resource_closure(changed).is_err());
    let mut changed = full;
    changed.resources.as_mut().unwrap().child_exit_code = None;
    assert!(owner.prepare_abnormal_resource_closure(changed).is_err());
}
#[test]
fn committed_readback_is_independent_of_in_memory_owner() {
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let r = ready(&owner);
    owner.update(r.clone()).unwrap();
    let mut changed = r.clone();
    changed.resources.as_mut().unwrap().child_exit_code = Some(99);
    let bytes = codec::encode_owner(&changed).unwrap();
    owner
        .db
        .execute(
            "UPDATE owner SET record=?1,digest=?2",
            params![bytes, sha_bytes(&bytes)],
        )
        .unwrap();
    assert!(confirm_committed(&owner, &r).is_err());
    assert_eq!(owner.record(), r);
}
#[test]
fn missing_history_or_schema_drift_never_confirms_commit() {
    for changed_schema in [false, true] {
        let (_dir, _, _, mut owner) = fixture();
        active(&mut owner);
        let r = ready(&owner);
        owner.update(r.clone()).unwrap();
        owner
            .db
            .execute_batch(if changed_schema {
                "PRAGMA user_version=3"
            } else {
                "DROP TABLE owner_recovery"
            })
            .unwrap();
        assert!(confirm_committed(&owner, &r).is_err());
    }
}
#[test]
fn stale_row_before_prepare_and_bad_digest_after_commit_are_not_repaired() {
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let original = ready(&owner);
    let mut changed = owner.record();
    changed.identity.as_mut().unwrap().incarnation = vec![99; 32];
    let bytes = codec::encode_owner(&changed).unwrap();
    owner
        .db
        .execute(
            "UPDATE owner SET record=?1,digest=?2",
            params![bytes, sha_bytes(&bytes)],
        )
        .unwrap();
    assert!(owner.prepare_abnormal_resource_closure(original).is_err());
    readback(&owner.db, &bytes).unwrap();
    let (_dir, _, _, mut owner) = fixture();
    active(&mut owner);
    let original = ready(&owner);
    owner
        .prepare_abnormal_resource_closure(original.clone())
        .unwrap();
    owner
        .db
        .execute("UPDATE owner SET digest=zeroblob(32)", [])
        .unwrap();
    assert!(confirm_committed(&owner, &original).is_err());
    assert_eq!(read_row(&owner.db).unwrap().1, vec![0; 32]);
}
