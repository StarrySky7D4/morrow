#![cfg(all(feature = "host", not(target_arch = "wasm32")))]

use morrow_agent_process_control_v1::host::Host;

#[test]
fn identity_follows_the_host_when_moved() {
    let original = Host::default();
    let identity = original.identity();
    let cloned_identity = identity.clone();
    let moved = Box::new(original);
    assert!(identity.matches(&moved));
    assert!(cloned_identity.matches(&moved));
}

#[test]
fn independently_created_hosts_have_distinct_identities() {
    let original = Host::default();
    let other = Host::default();
    let identity = original.identity();
    assert!(identity.matches(&original));
    assert!(!identity.matches(&other));
    assert!(!other.identity().matches(&original));
}

#[test]
fn dropping_the_host_invalidates_all_identity_clones() {
    let identity;
    let cloned_identity;
    {
        let original = Host::default();
        identity = original.identity();
        cloned_identity = identity.clone();
        assert!(identity.matches(&original));
    }
    let replacement = Host::default();
    assert!(!identity.matches(&replacement));
    assert!(!cloned_identity.matches(&replacement));
}
