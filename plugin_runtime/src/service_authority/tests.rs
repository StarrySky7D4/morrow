use super::*;
#[test]
fn authority_dependency_validates_resource_and_control() {
    use morrow_core::store::{EventBudget, ServiceAuthorityResource};
    use std::sync::atomic::{AtomicBool, Ordering};

    let dir_a = tempfile::tempdir().unwrap();
    let mut store_a = Store::open(&dir_a.path().join("db"), EventBudget::default()).unwrap();
    let guard = store_a.pin_service_authority().unwrap();
    let lease = store_a
        .narrow_service_authority(&guard, &[ServiceAuthorityResource::Outbound([1; 32])])
        .unwrap();

    let flag = Arc::new(AtomicBool::new(true));
    let probe = flag.clone();
    let dep = AuthorityDependency::new(&store_a, lease.clone(), move || {
        probe.load(Ordering::SeqCst)
    })
    .unwrap();
    assert!(dep.check().is_ok());

    let dir_b = tempfile::tempdir().unwrap();
    let store_b = Store::open(&dir_b.path().join("db"), EventBudget::default()).unwrap();
    assert!(AuthorityDependency::new(&store_b, lease.clone(), || true).is_err());

    let unrelated = store_a
        .narrow_service_authority(&guard, &[ServiceAuthorityResource::Outbound([2; 32])])
        .unwrap();
    let other = AuthorityDependency::new(&store_a, unrelated, || true).unwrap();
    store_a
        .service_authority_control()
        .revoke_resource(&ServiceAuthorityResource::Outbound([2; 32]));
    assert!(other.check().is_err());
    assert!(dep.check().is_ok());

    flag.store(false, Ordering::SeqCst);
    assert!(dep.check().is_err());
    flag.store(true, Ordering::SeqCst);
    assert!(dep.check().is_ok());

    store_a
        .service_authority_control()
        .revoke_resource(&ServiceAuthorityResource::Outbound([1; 32]));
    assert!(dep.check().is_err());

    let cloned = dep.clone();
    assert!(cloned.check().is_err());
}
