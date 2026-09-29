use morrow_core::{
    content::CardRecord,
    lifecycle::{CommitState, HostPolicy, InstancePhase},
    runtime::RenameRequest,
};
fn request() -> RenameRequest {
    RenameRequest {
        operation_id: "op-1".into(),
        card_id: "card-1".into(),
        expected_revision: 1,
        title: "fixed".into(),
    }
}
fn card() -> CardRecord {
    CardRecord::new("card-1", "unknown.type", 1, "old", vec![255]).unwrap()
}
#[test]
fn preparation_cannot_grant_or_execute() {
    let mut host = HostPolicy::new().unwrap();
    let instance = host.activate().unwrap();
    assert_eq!(host.phase(instance).unwrap(), InstancePhase::Preparing);
    assert!(host.grant_rename(instance, "card-1", 10, 0).is_err());
    host.ready(instance).unwrap();
    assert!(host.ready(instance).is_err());
}
#[test]
fn requests_are_fixed_and_completion_consumes_permit() {
    let mut host = HostPolicy::new().unwrap();
    let instance = host.activate().unwrap();
    host.ready(instance).unwrap();
    let grant = host.grant_rename(instance, "card-1", 10, 0).unwrap();
    let mut input = request();
    let permit = host.begin(instance, grant, &input, 1).unwrap();
    input.title = "changed alias".into();
    assert_eq!(
        host.complete(permit, &card(), 2).unwrap().summary().title,
        "fixed"
    );
    assert!(host.complete(permit, &card(), 3).is_err());
}
#[test]
fn wrong_object_or_other_instance_does_not_inherit_grant() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    let b = host.activate().unwrap();
    host.ready(a).unwrap();
    host.ready(b).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 0).unwrap();
    assert!(host.begin(b, grant, &request(), 1).is_err());
    let mut wrong = request();
    wrong.card_id = "card-2".into();
    assert!(host.begin(a, grant, &wrong, 1).is_err());
}
#[test]
fn revocation_between_check_and_completion_blocks_proposal() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 0).unwrap();
    let permit = host.begin(a, grant, &request(), 1).unwrap();
    host.revoke(grant).unwrap();
    assert!(host.complete(permit, &card(), 2).is_err());
}
#[test]
fn exact_expiry_and_clock_regression_are_rejected() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 5).unwrap();
    assert!(host.begin(a, grant, &request(), 4).is_err());
    let permit = host.begin(a, grant, &request(), 9).unwrap();
    assert!(host.complete(permit, &card(), 10).is_err());
    assert!(host.begin(a, grant, &request(), 10).is_err());
}
#[test]
fn normal_stop_refuses_new_tasks_and_waits_for_old_tasks() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 0).unwrap();
    let permit = host.begin(a, grant, &request(), 1).unwrap();
    host.drain(a, 10, 1).unwrap();
    assert!(host.begin(a, grant, &request(), 2).is_err());
    assert!(host.stop(a).is_err());
    host.complete(permit, &card(), 3).unwrap();
    host.stop(a).unwrap();
    host.retire(a).unwrap();
    assert!(host.phase(a).is_err());
}
#[test]
fn security_stop_revokes_before_cleanup_and_late_result_is_invalid() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 0).unwrap();
    let permit = host.begin(a, grant, &request(), 1).unwrap();
    host.safety_stop(a).unwrap();
    assert_eq!(host.phase(a).unwrap(), InstancePhase::Revoked);
    assert!(host.complete(permit, &card(), 2).is_err());
    host.stop(a).unwrap();
    host.retire(a).unwrap();
    let next = host.activate().unwrap();
    host.ready(next).unwrap();
    assert_ne!(next, a);
    assert!(host.begin(next, grant, &request(), 3).is_err());
}
#[test]
fn hosts_cannot_use_each_others_handles() {
    let mut a = HostPolicy::new().unwrap();
    let mut b = HostPolicy::new().unwrap();
    let instance = a.activate().unwrap();
    assert!(b.ready(instance).is_err());
}
#[test]
fn cancellation_frees_drain_without_executing() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 10, 0).unwrap();
    let permit = host.begin(a, grant, &request(), 1).unwrap();
    host.drain(a, 10, 1).unwrap();
    host.cancel(permit).unwrap();
    host.stop(a).unwrap();
    assert!(host.complete(permit, &card(), 2).is_err());
}
#[test]
fn timeout_does_not_claim_no_commit_or_undo_observed_commit() {
    let pending = CommitState::NotSubmitted.submitted().unwrap();
    assert_eq!(pending.timeout(), CommitState::AwaitingResult);
    assert!(pending.sealed().is_err());
    let committed = pending.resolved(true).unwrap();
    assert_eq!(committed.timeout(), CommitState::LocallyCommitted);
    assert!(committed.witnessed().is_err());
    assert_eq!(
        committed.sealed().unwrap().witnessed().unwrap(),
        CommitState::Witnessed
    );
    assert_eq!(pending.resolved(false).unwrap(), CommitState::NotCommitted);
}
#[test]
fn instance_capacity_can_be_reused_without_reusing_identity() {
    let mut host = HostPolicy::new().unwrap();
    let mut instances = Vec::new();
    for _ in 0..128 {
        instances.push(host.activate().unwrap());
    }
    assert!(host.activate().is_err());
    let old = instances[0];
    host.safety_stop(old).unwrap();
    host.stop(old).unwrap();
    host.retire(old).unwrap();
    assert_ne!(host.activate().unwrap(), old);
}

#[test]
fn drain_deadline_revokes_before_late_completion_and_stop_stays_stopped() {
    let mut host = HostPolicy::new().unwrap();
    let a = host.activate().unwrap();
    host.ready(a).unwrap();
    let grant = host.grant_rename(a, "card-1", 100, 0).unwrap();
    let permit = host.begin(a, grant, &request(), 1).unwrap();
    host.drain(a, 5, 2).unwrap();
    assert!(host.complete(permit, &card(), 5).is_err());
    assert_eq!(host.phase(a).unwrap(), InstancePhase::Revoked);
    host.stop(a).unwrap();
    host.safety_stop(a).unwrap();
    assert_eq!(host.phase(a).unwrap(), InstancePhase::Stopped);
}
