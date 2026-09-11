#![cfg(not(target_arch = "wasm32"))]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, HostPolicy, InstancePhase},
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::Lookup,
};
#[test]
fn revocation_is_one_way_instance_scoped_and_prevents_pending_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "old", vec![]).unwrap(),
        )
        .unwrap();
    let mut p = HostPolicy::new().unwrap();
    let a = p.activate().unwrap();
    p.ready(a).unwrap();
    let b = p.activate().unwrap();
    p.ready(b).unwrap();
    let ga = p.grant_rename(a, "card", 100, 0).unwrap();
    let request = RenameRequest {
        operation_id: "op".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "new".into(),
    };
    let permit = p.begin(a, ga, &request, 1).unwrap();
    let signal = p.revocation(a).unwrap();
    let copy = signal.clone();
    std::thread::spawn(move || copy.revoke()).join().unwrap();
    assert_eq!(p.phase(a).unwrap(), InstancePhase::Revoked);
    assert_eq!(p.phase(b).unwrap(), InstancePhase::Ready);
    assert!(p.commit_rename(permit, &mut store, || 2).is_err());
    assert!(matches!(
        store.lookup_for_card("card", "op").unwrap(),
        Lookup::Absent
    ));
    assert!(p.grant_rename(a, "card", 100, 2).is_err());
    assert!(p.drain(a, 100, 2).is_err());
    signal.revoke();
    p.safety_stop(a).unwrap();
    p.stop(a).unwrap();
    p.retire(a).unwrap();
    let fresh = p.activate().unwrap();
    p.ready(fresh).unwrap();
    signal.revoke();
    assert_eq!(p.phase(fresh).unwrap(), InstancePhase::Ready);
}
#[test]
fn foreign_host_cannot_obtain_revocation_signal() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let mut h =
        HostRuntime::new(Store::open(&a.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let other =
        HostRuntime::new(Store::open(&b.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let mut c = h.connect().unwrap();
    assert!(other.revocation(&c).is_err());
    h.revocation(&c).unwrap().revoke();
    assert!(
        h.grant(&mut c, GrantKind::ReadSummary, "card", 100, 0)
            .is_err()
    );
}
