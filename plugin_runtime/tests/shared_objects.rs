#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, proto::TransformHandler},
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Limits as RuntimeLimits,
    package::PreparedPackage,
    shared_objects::{Error, Limits, SharedObjects, TransformRequest},
};
fn host(d: &tempfile::TempDir) -> HostRuntime {
    HostRuntime::new(Store::open(&d.path().join("db"), Default::default()).unwrap()).unwrap()
}
fn request() -> TransformRequest<'static> {
    TransformRequest {
        task_id: "shared-task",
        handler: "shared.check",
        input_type: "bytes",
        output_type: "bytes",
    }
}
fn fixture(expected: &[u8]) -> PreparedPackage {
    let invocation = Invocation::new_transform(
        "shared-task",
        Transform {
            handler: "shared.check".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: expected.to_vec(),
        },
    )
    .unwrap();
    let output = expected.iter().rev().copied().collect::<Vec<_>>();
    let done = invocation.output_completion(&output).unwrap();
    let data = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    // The guest checks EVERY invocation byte, not merely a constant success completion.
    let module=wat::parse_str(format!(r#"(module
      (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
      (memory(export "memory") 4)
      (data(i32.const 0) "{}") (data(i32.const 16384) "{}")
      (func(export "morrow_run")(result i32)(local $i i32)
        i32.const 65536 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
        (loop $compare
          local.get $i i32.load8_u local.get $i i32.const 65536 i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.tee $i i32.const {} i32.lt_u br_if $compare)
        i32.const 16384 i32.const {} call $done drop i32.const 0))"#,data(invocation.bytes()),data(&done),invocation.bytes().len(),invocation.bytes().len(),done.len())).unwrap();
    let handler = TransformHandler {
        handler: "shared.check".into(),
        input_type: "bytes".into(),
        output_type: "bytes".into(),
        max_input_bytes: 65536,
        max_output_bytes: 65536,
    };
    PreparedPackage::new(
        Package::build(
            Package::manifest_for_transform("test.shared", "1.0.0", &module, vec![handler]),
            &module,
        )
        .unwrap(),
        RuntimeLimits::default(),
    )
    .unwrap()
}
#[test]
fn publication_freezes_producer_buffer_and_hashes_the_fixed_mapping() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let producer = h.connect().unwrap();
    let consumer = h.connect().unwrap();
    let mut broker = SharedObjects::new(&h, Limits::default()).unwrap();
    let mut bytes = vec![1, 2, 3, 0, 255];
    let desc = broker
        .publish(&h, &producer, "workspace:a", &bytes, 1)
        .unwrap();
    bytes.fill(99);
    let lease = broker
        .grant(&h, &consumer, &desc, "workspace:a", 100, 1)
        .unwrap();
    let map = broker.map(&h, &consumer, &lease, 1).unwrap();
    assert_eq!(map.bytes(), [1, 2, 3, 0, 255]);
    assert_eq!(map.descriptor(), &desc);
    assert_eq!(
        desc,
        morrow_core::shared_object::Descriptor::for_bytes(
            desc.arena,
            desc.object,
            desc.generation,
            map.bytes()
        )
        .unwrap()
    );
}
#[test]
fn every_descriptor_component_and_wrong_scope_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let producer = h.connect().unwrap();
    let consumer = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &producer, "a", b"fixed", 1).unwrap();
    for field in 0..7 {
        let mut altered = desc.clone();
        match field {
            0 => altered.arena += 1,
            1 => altered.object += 1,
            2 => altered.generation += 1,
            3 => altered.sha256[0] ^= 1,
            4 => altered.length += 1,
            5 => altered.segments[0].offset = 1,
            _ => altered.segments[0].length -= 1,
        };
        assert!(b.grant(&h, &consumer, &altered, "a", 10, 1).is_err());
    }
    assert!(b.grant(&h, &consumer, &desc, "other", 10, 1).is_err());
    assert_eq!(b.usage().leases, 0);
    assert!(b.grant(&h, &consumer, &desc, "a", 10, 1).is_ok());
}
#[test]
fn foreign_broker_host_and_consumer_cannot_reuse_descriptors_leases_or_mappings() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let producer = h.connect().unwrap();
    let consumer = h.connect().unwrap();
    let other = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let mut foreign = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &producer, "a", b"fixed", 1).unwrap();
    let lease = b.grant(&h, &consumer, &desc, "a", 10, 1).unwrap();
    assert!(foreign.grant(&h, &consumer, &desc, "a", 10, 1).is_err());
    assert!(foreign.map(&h, &consumer, &lease, 1).is_err());
    assert!(foreign.revoke(&lease).is_err());
    assert!(foreign.retire(&desc).is_err());
    assert!(b.map(&h, &other, &lease, 1).is_err());
    let d2 = tempfile::tempdir().unwrap();
    let mut h2 = host(&d2);
    let c2 = h2.connect().unwrap();
    assert!(b.publish(&h2, &c2, "a", b"foreign", 1).is_err());
    assert!(b.map(&h2, &c2, &lease, 1).is_err());
    assert!(b.map(&h2, &consumer, &lease, 1).is_err());
    assert_eq!(b.map(&h, &consumer, &lease, 1).unwrap().bytes(), b"fixed");
}
#[test]
fn expiry_boundary_rejects_new_access_without_unmapping_delivered_bytes() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &c, "a", b"fixed", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 5, 1).unwrap();
    let map = b.map(&h, &c, &lease, 4).unwrap();
    assert!(b.map(&h, &c, &lease, 5).is_err());
    assert_eq!(b.collect(5).unwrap().leases, 0);
    assert_eq!(map.bytes(), b"fixed");
    assert_eq!(b.usage().mappings, 1);
}
#[test]
fn consumer_revocation_blocks_new_maps_but_cannot_retract_delivered_bytes() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let p = h.connect().unwrap();
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &p, "a", b"fixed", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    let map = b.map(&h, &c, &lease, 1).unwrap();
    h.revocation(&c).unwrap().revoke();
    assert!(b.map(&h, &c, &lease, 1).is_err());
    assert_eq!(map.bytes(), b"fixed");
}
#[test]
fn producer_revocation_stops_new_grants_without_revoking_existing_consumer_lease() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let p = h.connect().unwrap();
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &p, "a", b"fixed", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    h.revocation(&p).unwrap().revoke();
    assert!(b.grant(&h, &c, &desc, "a", 10, 1).is_err());
    assert_eq!(b.map(&h, &c, &lease, 1).unwrap().bytes(), b"fixed");
}
#[test]
fn retire_keeps_old_pages_charged_until_last_mapping_and_never_reuses_identity() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(
        &h,
        Limits {
            objects: 1,
            charged_bytes: 65536,
            ..Limits::default()
        },
    )
    .unwrap();
    let desc = b.publish(&h, &c, "a", b"old", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    let m1 = b.map(&h, &c, &lease, 1).unwrap();
    let m2 = b.map(&h, &c, &lease, 1).unwrap();
    b.retire(&desc).unwrap();
    assert!(b.retire(&desc).is_err());
    assert!(b.map(&h, &c, &lease, 1).is_err());
    assert_eq!(b.usage().charged_bytes, 65536);
    assert!(b.publish(&h, &c, "a", b"new", 1).is_err());
    drop(m1);
    assert_eq!(m2.bytes(), b"old");
    assert_eq!(b.usage().objects, 1);
    drop(m2);
    assert_eq!(b.usage().charged_bytes, 0);
    assert_eq!(b.usage().objects, 0);
    assert_eq!(b.usage().mappings, 0);
    let next = b.publish(&h, &c, "a", b"new", 1).unwrap();
    assert_ne!(desc.object, next.object);
    assert!(b.grant(&h, &c, &desc, "a", 10, 1).is_err());
}
#[test]
fn lease_mapping_and_conservative_allocation_limits_are_enforced_and_reclaimed() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(
        &h,
        Limits {
            objects: 2,
            charged_bytes: 65536,
            leases: 1,
            mappings: 1,
        },
    )
    .unwrap();
    let desc = b.publish(&h, &c, "a", b"x", 1).unwrap();
    assert_eq!(b.usage().charged_bytes, 65536);
    assert!(b.publish(&h, &c, "b", b"x", 1).is_err());
    let lease = b.grant(&h, &c, &desc, "a", 5, 1).unwrap();
    assert!(b.grant(&h, &c, &desc, "a", 5, 1).is_err());
    let map = b.map(&h, &c, &lease, 1).unwrap();
    assert!(b.map(&h, &c, &lease, 1).is_err());
    drop(map);
    assert!(b.map(&h, &c, &lease, 1).is_ok());
    b.revoke(&lease).unwrap();
    assert!(b.revoke(&lease).is_err());
    assert_eq!(b.usage().leases, 0);
    assert!(b.grant(&h, &c, &desc, "a", 5, 1).is_ok());
    assert_eq!(b.collect(5).unwrap().leases, 0);
}
#[test]
fn identical_payloads_in_different_scopes_have_distinct_objects() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let a = b.publish(&h, &c, "a", b"same", 1).unwrap();
    let other = b.publish(&h, &c, "b", b"same", 1).unwrap();
    assert_eq!(a.sha256, other.sha256);
    assert_ne!(a.object, other.object);
    assert!(b.grant(&h, &c, &a, "b", 10, 1).is_err());
    let lease = b.grant(&h, &c, &a, "a", 10, 1).unwrap();
    assert_eq!(b.map(&h, &c, &lease, 1).unwrap().descriptor(), &a);
}
#[test]
fn real_wasm_checks_exact_frozen_input_and_foreign_mapping_never_executes() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let p = h.connect().unwrap();
    let package = fixture(b"abc\0z");
    let c = package.connect(&mut h).unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let mut source = b"abc\0z".to_vec();
    let desc = b.publish(&h, &p, "a", &source, 1).unwrap();
    source.fill(42);
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    let map = b.map(&h, &c, &lease, 1).unwrap();
    let report = b
        .run_transform(
            &mut h,
            &c,
            &package,
            &map,
            request(),
            || 2,
            Cancellation::default(),
        )
        .unwrap();
    assert_eq!(report.execution.outcome, Ok(0));
    assert_eq!(report.execution.host_calls, 0);
    assert!(report.response.is_none());
    assert_eq!(report.output.unwrap().bytes, b"z\0cba");
    assert!(h.store_local().pending(0, 10).unwrap().is_empty());
    let mut other = SharedObjects::new(&h, Limits::default()).unwrap();
    assert!(
        other
            .run_transform(
                &mut h,
                &c,
                &package,
                &map,
                request(),
                || 2,
                Cancellation::default()
            )
            .is_err()
    );
}
#[test]
fn final_clock_expiry_or_revocation_discards_real_wasm_output() {
    for revoke in [false, true] {
        let d = tempfile::tempdir().unwrap();
        let mut h = host(&d);
        let p = h.connect().unwrap();
        let package = fixture(b"abc");
        let c = package.connect(&mut h).unwrap();
        let signal = h.revocation(&c).unwrap();
        let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
        let desc = b.publish(&h, &p, "a", b"abc", 1).unwrap();
        let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
        let map = b.map(&h, &c, &lease, 1).unwrap();
        let mut calls = 0;
        let result = b.run_transform(
            &mut h,
            &c,
            &package,
            &map,
            request(),
            || {
                calls += 1;
                if calls == 2 {
                    if revoke {
                        signal.revoke();
                    } else {
                        return 10;
                    }
                }
                2
            },
            Cancellation::default(),
        );
        assert!(result.is_err());
        assert_eq!(calls, 2);
        assert_eq!(map.bytes(), b"abc");
        assert!(h.store_local().pending(0, 10).unwrap().is_empty());
    }
}
#[test]
fn oversized_wasm_input_clock_regression_and_invalid_limits_are_rejected() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let package = fixture(b"abc");
    let c = package.connect(&mut h).unwrap();
    assert!(
        SharedObjects::new(
            &h,
            Limits {
                objects: 0,
                ..Limits::default()
            }
        )
        .is_err()
    );
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    assert!(b.publish(&h, &c, "a", &[], 1).is_err());
    let desc = b.publish(&h, &c, "a", &vec![1; 65537], 2).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 2).unwrap();
    assert!(b.map(&h, &c, &lease, 1).is_err());
    let map = b.map(&h, &c, &lease, 2).unwrap();
    assert!(matches!(
        b.run_transform(
            &mut h,
            &c,
            &package,
            &map,
            request(),
            || 2,
            Cancellation::default()
        ),
        Err(Error::Limit)
    ));
}

#[test]
fn foreign_or_invalid_requests_with_future_timestamps_do_not_poison_the_host_clock() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let c = h.connect().unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &c, "a", b"fixed", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    let d2 = tempfile::tempdir().unwrap();
    let mut foreign = host(&d2);
    let other = foreign.connect().unwrap();
    assert!(b.map(&foreign, &other, &lease, 1000000).is_err());
    let mut changed = desc.clone();
    changed.sha256[0] ^= 1;
    assert!(b.grant(&h, &c, &changed, "a", 2000000, 1000000).is_err());
    assert!(b.grant(&h, &c, &desc, "wrong", 2000000, 1000000).is_err());
    assert!(b.publish(&h, &c, "", b"invalid", 1000000).is_err());
    assert!(b.publish(&h, &c, "a", &[], 1000000).is_err());
    assert_eq!(b.usage().leases, 1);
    assert_eq!(b.map(&h, &c, &lease, 2).unwrap().bytes(), b"fixed");
}

#[test]
fn revoked_or_retired_mapping_cannot_be_used_for_new_wasm_work() {
    let d = tempfile::tempdir().unwrap();
    let mut h = host(&d);
    let p = h.connect().unwrap();
    let package = fixture(b"abc");
    let c = package.connect(&mut h).unwrap();
    let mut b = SharedObjects::new(&h, Limits::default()).unwrap();
    let desc = b.publish(&h, &p, "a", b"abc", 1).unwrap();
    let lease = b.grant(&h, &c, &desc, "a", 10, 1).unwrap();
    let old = b.map(&h, &c, &lease, 1).unwrap();
    b.revoke(&lease).unwrap();
    assert!(
        b.run_transform(
            &mut h,
            &c,
            &package,
            &old,
            request(),
            || 2,
            Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(old.bytes(), b"abc");
    let fresh_lease = b.grant(&h, &c, &desc, "a", 10, 2).unwrap();
    let fresh = b.map(&h, &c, &fresh_lease, 2).unwrap();
    b.retire(&desc).unwrap();
    assert!(
        b.run_transform(
            &mut h,
            &c,
            &package,
            &fresh,
            request(),
            || 2,
            Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(fresh.bytes(), b"abc");
    assert_eq!(b.usage().charged_bytes, 65536);
}
