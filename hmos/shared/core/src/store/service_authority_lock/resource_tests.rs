use super::{tests::id, *};
fn scoped(
    owner: &mut ServiceAuthorityCoordinator,
    dependencies: &[ServiceAuthorityResource],
) -> ServiceAuthorityLease {
    let guard = owner.pin().unwrap();
    owner.narrow(&guard, dependencies).unwrap()
}
#[test]
fn exact_dependencies_shared_credentials_and_namespaces_do_not_alias() {
    use ServiceAuthorityResource::*;
    let mut owner = ServiceAuthorityCoordinator::new(id(), true);
    let legacy = owner.pin().unwrap();
    let a = scoped(&mut owner, &[Outbound([1; 32]), Outbound([9; 32])]);
    let a_copy = a.clone();
    let b = scoped(&mut owner, &[Outbound([2; 32]), Outbound([9; 32])]);
    let inbound = scoped(
        &mut owner,
        &[Inbound([1; 32]), Configuration("config".into())],
    );
    let control = owner.control();
    control.revoke_resource(&Outbound([1; 32]));
    assert!(legacy.check().is_err());
    assert!(a.check().is_err());
    assert!(a_copy.check().is_err());
    b.check().unwrap();
    inbound.check().unwrap();
    let fresh = scoped(&mut owner, &[Outbound([1; 32]), Outbound([9; 32])]);
    fresh.check().unwrap();
    assert!(a.check().is_err());
    control.revoke_resource(&Outbound([9; 32]));
    assert!(fresh.check().is_err());
    assert!(b.check().is_err());
    inbound.check().unwrap();
    control.revoke_resource(&Configuration("other".into()));
    inbound.check().unwrap();
    control.revoke_resource(&Configuration("config".into()));
    assert!(inbound.check().is_err());
}
#[test]
fn revoked_resolution_cannot_mint_fresh_dependencies_and_scoped_lease_cannot_expand() {
    let mut owner = ServiceAuthorityCoordinator::new(id(), true);
    let resources = [ServiceAuthorityResource::Outbound([1; 32])];
    let guard = owner.pin().unwrap();
    owner.control().revoke_resource(&resources[0]);
    assert!(owner.narrow(&guard, &resources).is_err());
    let live = scoped(&mut owner, &resources);
    assert!(owner.narrow(&live, &resources).is_err());
    let other = ServiceAuthorityCoordinator::new(id(), true);
    assert!(other.validate(&live).is_err());
    let guard = owner.pin().unwrap();
    assert!(other.narrow(&guard, &resources).is_err());
    assert!(owner.narrow(&guard, &[]).is_err());
    assert!(
        owner
            .narrow(&guard, &vec![resources[0].clone(); 67])
            .is_err()
    );
    assert!(
        owner
            .narrow(&guard, &[ServiceAuthorityResource::Inbound([0; 32])])
            .is_err()
    );
    owner.control().revoke_all();
    assert!(live.check().is_err());
    assert!(owner.narrow(&guard, &resources).is_err());
}
#[test]
fn original_store_drop_invalidates_scoped_copies_even_after_same_identity_reopen() {
    let identity = id();
    let mut owner = ServiceAuthorityCoordinator::new(identity, true);
    let live = scoped(&mut owner, &[ServiceAuthorityResource::Inbound([3; 32])]);
    let copy = live.clone();
    let control = owner.control();
    drop(owner);
    assert!(live.check().is_err());
    assert!(copy.check().is_err());
    let mut other = ServiceAuthorityCoordinator::new(identity, true);
    let fresh = scoped(&mut other, &[ServiceAuthorityResource::Inbound([3; 32])]);
    control.revoke_all();
    fresh.check().unwrap();
    assert!(other.validate(&live).is_err());
}
#[test]
fn resource_epoch_capacity_is_atomic_and_dead_or_revoked_entries_are_reclaimed() {
    let mut epochs = epochs::Epochs::default();
    let keys: Vec<_> = (0..2048)
        .map(|i| ServiceAuthorityResource::Configuration(format!("key{i}")))
        .collect();
    let live = epochs.bind(&keys).unwrap();
    let extra = ServiceAuthorityResource::Configuration("extra".into());
    assert_eq!(epochs.bind(&[extra.clone()]).err(), Some(Error::Limit));
    assert!(live.iter().all(|v| !v.load(Ordering::Acquire)));
    epochs.revoke(&keys[0]);
    assert!(live[0].load(Ordering::Acquire));
    let replacement = epochs.bind(&[extra]).unwrap();
    assert!(!replacement[0].load(Ordering::Acquire));
    drop(live);
    drop(replacement);
    let fresh = epochs.bind(&keys).unwrap();
    assert_eq!(fresh.len(), 2048);
    epochs.revoke_all();
    assert!(fresh.iter().all(|v| v.load(Ordering::Acquire)));
}
