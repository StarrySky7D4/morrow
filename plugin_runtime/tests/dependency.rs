#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    content::CardRecord,
    dispatch::{Connection, HostRuntime},
    lifecycle::GrantKind,
    plugin_package::{
        Package,
        proto::{Capability, TransformHandler},
    },
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Limits as RuntimeLimits,
    dependency::{Dependency, DependencyOutput, Endpoint, Spec},
    package::PreparedPackage,
    proposal::{EditProposal, EditTarget},
    shared_objects::{Limits, Mapping, SharedObjects},
};
const SOURCE: &[u8] = b"fixed \0 dependency \xff input";
fn provider(expected: &[u8]) -> PreparedPackage {
    let invocation = Invocation::new_transform(
        "dependency-task",
        Transform {
            handler: "dependency.reverse".into(),
            input_type: "bytes".into(),
            output_type: "reversed".into(),
            input: expected.to_vec(),
        },
    )
    .unwrap();
    let done = invocation
        .output_completion(&expected.iter().rev().copied().collect::<Vec<_>>())
        .unwrap();
    let data = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    // A different task ID, type, byte, length or source makes this real guest trap.
    // It does not certify input delivery merely by returning a constant completion.
    let module = wat::parse_str(format!(r#"(module
      (import "morrow_task_v1" "read_input" (func $read(param i32 i32)(result i32)))
      (import "morrow_task_v1" "complete" (func $done(param i32 i32)(result i32)))
      (memory(export "memory") 4)
      (data(i32.const 0) "{}") (data(i32.const 16384) "{}")
      (func(export "morrow_run")(result i32)(local $i i32)
        i32.const 65536 i32.const 131072 call $read i32.const {} i32.ne if unreachable end
        (loop $compare
          local.get $i i32.load8_u local.get $i i32.const 65536 i32.add i32.load8_u i32.ne if unreachable end
          local.get $i i32.const 1 i32.add local.tee $i i32.const {} i32.lt_u br_if $compare)
        i32.const 16384 i32.const {} call $done drop i32.const 0))"#,
        data(invocation.bytes()), data(&done), invocation.bytes().len(), invocation.bytes().len(), done.len())).unwrap();
    let manifest = Package::manifest_for_transform(
        "test.dependency.provider",
        "1.0.0",
        &module,
        vec![TransformHandler {
            handler: "dependency.reverse".into(),
            input_type: "bytes".into(),
            output_type: "reversed".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    PreparedPackage::new(
        Package::build(manifest, &module).unwrap(),
        RuntimeLimits::default(),
    )
    .unwrap()
}
fn caller() -> PreparedPackage {
    let module = wat::parse_str(r#"(module (memory(export "memory") 1) (func(export "morrow_run")(result i32) i32.const 0))"#).unwrap();
    PreparedPackage::new(
        Package::build(
            Package::manifest_for_task(
                "test.dependency.caller",
                "1.0.0",
                &module,
                vec![Capability::EditContent],
            ),
            &module,
        )
        .unwrap(),
        RuntimeLimits::default(),
    )
    .unwrap()
}
struct Fixture {
    _root: tempfile::TempDir,
    host: HostRuntime,
    caller: PreparedPackage,
    provider: PreparedPackage,
    c: Connection,
    p: Connection,
    objects: SharedObjects,
    mapping: Mapping,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut host =
            HostRuntime::new(Store::open(&root.path().join("db"), Default::default()).unwrap())
                .unwrap();
        host.store_local_mut()
            .create_local(
                "seed",
                &CardRecord::new("card", "test.card", 1, "original", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let caller = caller();
        let provider = provider(SOURCE);
        let c = caller.connect(&mut host).unwrap();
        let p = provider.connect(&mut host).unwrap();
        let mut objects = SharedObjects::new(&host, Limits::default()).unwrap();
        let mut source = SOURCE.to_vec();
        let desc = objects
            .publish(&host, &c, "workspace:a", &source, 1)
            .unwrap();
        source.fill(0);
        let lease = objects
            .grant(&host, &c, &desc, "workspace:a", 100, 1)
            .unwrap();
        let mapping = objects.map(&host, &c, &lease, 1).unwrap();
        Self {
            _root: root,
            host,
            caller,
            provider,
            c,
            p,
            objects,
            mapping,
        }
    }
    fn c(&self) -> Endpoint<'_> {
        Endpoint {
            package: &self.caller,
            connection: &self.c,
        }
    }
    fn p(&self) -> Endpoint<'_> {
        Endpoint {
            package: &self.provider,
            connection: &self.p,
        }
    }
    fn route(&self) -> Dependency {
        Dependency::bind(&self.host, self.c(), self.p(), spec(), 1).unwrap()
    }
    fn run(
        &mut self,
        route: &Dependency,
        clock: impl FnMut() -> u64,
        cancel: Cancellation,
    ) -> Result<DependencyOutput, morrow_plugin_runtime::dependency::Error> {
        route.run(
            &mut self.objects,
            &mut self.host,
            Endpoint {
                package: &self.caller,
                connection: &self.c,
            },
            Endpoint {
                package: &self.provider,
                connection: &self.p,
            },
            &self.mapping,
            "dependency-task",
            clock,
            cancel,
        )
    }
    fn unchanged(&self) {
        let card = self.host.store_local().card("card").unwrap().unwrap();
        assert_eq!(card.body(), b"old");
        assert_eq!(card.summary().revision, 1);
        assert_eq!(self.host.store_local().pending(0, 10).unwrap().len(), 1);
    }
    fn grant_edit(&mut self) {
        self.host
            .grant(&mut self.c, GrantKind::EditContent, "card", 100, 1)
            .unwrap();
    }
}
fn spec() -> Spec<'static> {
    Spec {
        handler: "dependency.reverse",
        input_type: "bytes",
        output_type: "reversed",
        scope: "workspace:a",
        expires: 10,
    }
}
fn proposal(output: DependencyOutput, id: &str, revision: u64) -> EditProposal {
    EditProposal::new(
        output,
        EditTarget {
            operation_id: id,
            card_id: "card",
            expected_revision: revision,
            title: "changed",
            preview: "dependency preview",
            accepted_output_type: "reversed",
        },
    )
    .unwrap()
}

#[test]
fn real_wasm_receives_exact_frozen_input_and_temporary_resources_are_released() {
    let mut f = Fixture::new();
    let route = f.route();
    let before = f.objects.usage();
    let output = f.run(&route, || 1, Cancellation::default()).unwrap();
    assert_eq!(
        output.bytes(),
        SOURCE.iter().rev().copied().collect::<Vec<_>>()
    );
    assert_eq!(output.output_type(), "reversed");
    assert_eq!(output.input_descriptor(), f.mapping.descriptor());
    output.validate(&f.host, &f.c, 1).unwrap();
    assert_eq!(f.objects.usage().leases, before.leases);
    assert_eq!(f.objects.usage().mappings, before.mappings);
    assert_eq!(f.objects.usage().charged_bytes, before.charged_bytes);
    f.unchanged();
}
#[test]
fn bind_rejects_wrong_contract_and_inactive_or_foreign_endpoints() {
    let f = Fixture::new();
    for wrong in 0..5 {
        let mut s = spec();
        match wrong {
            0 => s.handler = "missing",
            1 => s.input_type = "wrong",
            2 => s.output_type = "wrong",
            3 => s.scope = "",
            _ => s.expires = 1,
        }
        assert!(Dependency::bind(&f.host, f.c(), f.p(), s, 1).is_err());
    }
    let other = Fixture::new();
    assert!(Dependency::bind(&f.host, other.c(), f.p(), spec(), 1).is_err());
    assert!(Dependency::bind(&other.host, f.c(), f.p(), spec(), 1).is_err());
    assert!(
        Dependency::bind(
            &f.host,
            Endpoint {
                package: &f.provider,
                connection: &f.c
            },
            f.p(),
            spec(),
            1
        )
        .is_err()
    );
    f.host.revocation(&f.p).unwrap().revoke();
    assert!(Dependency::bind(&f.host, f.c(), f.p(), spec(), 1).is_err());
}
#[test]
fn run_rejects_substituted_same_package_instances_foreign_host_and_package() {
    let mut f = Fixture::new();
    let route = f.route();
    let other_c = f.caller.connect(&mut f.host).unwrap();
    let other_p = f.provider.connect(&mut f.host).unwrap();
    for which in 0..3 {
        let c = Endpoint {
            package: if which == 2 { &f.provider } else { &f.caller },
            connection: if which == 0 { &other_c } else { &f.c },
        };
        let p = Endpoint {
            package: &f.provider,
            connection: if which == 1 { &other_p } else { &f.p },
        };
        assert!(
            route
                .run(
                    &mut f.objects,
                    &mut f.host,
                    c,
                    p,
                    &f.mapping,
                    "dependency-task",
                    || 1,
                    Cancellation::default()
                )
                .is_err()
        );
    }
    let mut other = Fixture::new();
    assert!(other.run(&route, || 1, Cancellation::default()).is_err());
    assert!(f.run(&route, || 1, Cancellation::default()).is_ok());
    f.unchanged();
}
#[test]
fn wrong_scope_changed_input_and_changed_task_id_never_release_output_or_leak_leases() {
    for mode in 0..3 {
        let mut f = Fixture::new();
        let mut s = spec();
        if mode == 0 {
            s.scope = "workspace:b";
        }
        let route = Dependency::bind(&f.host, f.c(), f.p(), s, 1).unwrap();
        let extra;
        let mapping = if mode == 1 {
            let desc = f
                .objects
                .publish(&f.host, &f.c, "workspace:a", b"different", 1)
                .unwrap();
            let lease = f
                .objects
                .grant(&f.host, &f.c, &desc, "workspace:a", 100, 1)
                .unwrap();
            extra = f.objects.map(&f.host, &f.c, &lease, 1).unwrap();
            &extra
        } else {
            &f.mapping
        };
        let before = f.objects.usage();
        assert!(
            route
                .run(
                    &mut f.objects,
                    &mut f.host,
                    Endpoint {
                        package: &f.caller,
                        connection: &f.c
                    },
                    Endpoint {
                        package: &f.provider,
                        connection: &f.p
                    },
                    mapping,
                    if mode == 2 {
                        "changed-task"
                    } else {
                        "dependency-task"
                    },
                    || 1,
                    Cancellation::default()
                )
                .is_err()
        );
        assert_eq!(f.objects.usage().leases, before.leases);
        assert_eq!(f.objects.usage().mappings, before.mappings);
        f.unchanged();
    }
}
#[test]
fn expiry_revocation_and_cancellation_before_execution_preserve_resources() {
    for mode in 0..5 {
        let mut f = Fixture::new();
        let route = f.route();
        let cancel = Cancellation::default();
        match mode {
            1 => route.revoke(),
            2 => f.host.revocation(&f.c).unwrap().revoke(),
            3 => f.host.revocation(&f.p).unwrap().revoke(),
            4 => cancel.cancel(),
            _ => {}
        }
        let before = f.objects.usage();
        assert!(
            f.run(&route, || if mode == 0 { 10 } else { 1 }, cancel)
                .is_err()
        );
        assert_eq!(f.objects.usage().leases, before.leases);
        assert_eq!(f.objects.usage().mappings, before.mappings);
        f.unchanged();
    }
}
#[test]
fn late_expiry_or_caller_provider_route_revocation_suppresses_actual_guest_output() {
    let mut baseline = Fixture::new();
    let route = baseline.route();
    let mut calls = 0;
    baseline
        .run(
            &route,
            || {
                calls += 1;
                1
            },
            Cancellation::default(),
        )
        .unwrap();
    assert!(calls >= 2);
    for mode in 0..4 {
        let mut f = Fixture::new();
        let route = f.route();
        let signal = f
            .host
            .revocation(if mode == 1 { &f.c } else { &f.p })
            .unwrap();
        let mut ticks = 0;
        let before = f.objects.usage();
        assert!(
            f.run(
                &route,
                || {
                    ticks += 1;
                    if ticks == calls {
                        match mode {
                            1 | 2 => signal.revoke(),
                            3 => route.revoke(),
                            _ => {}
                        }
                    }
                    if ticks == calls && mode == 0 { 10 } else { 1 }
                },
                Cancellation::default()
            )
            .is_err()
        );
        assert_eq!(ticks, calls);
        assert_eq!(f.objects.usage().leases, before.leases);
        assert_eq!(f.objects.usage().mappings, before.mappings);
        f.unchanged();
    }
}
#[test]
fn output_is_bound_to_original_host_caller_and_live_route() {
    for mode in 0..5 {
        let mut f = Fixture::new();
        let route = f.route();
        let output = f.run(&route, || 1, Cancellation::default()).unwrap();
        let other = Fixture::new();
        assert!(output.validate(&other.host, &other.c, 1).is_err());
        assert!(output.validate(&f.host, &f.p, 1).is_err());
        match mode {
            0 => route.revoke(),
            1 => drop(route),
            2 => f.host.revocation(&f.p).unwrap().revoke(),
            3 => f.host.revocation(&f.c).unwrap().revoke(),
            _ => {}
        }
        assert!(
            output
                .validate(&f.host, &f.c, if mode == 4 { 10 } else { 1 })
                .is_err()
        );
        assert!(
            output
                .validate_liveness(if mode == 4 { 10 } else { 1 })
                .is_err()
        );
    }
}
#[test]
fn proposal_requires_callers_content_permission_and_correct_output_type() {
    let mut f = Fixture::new();
    let route = f.route();
    let output = f.run(&route, || 1, Cancellation::default()).unwrap();
    assert!(
        EditProposal::new(
            output,
            EditTarget {
                operation_id: "badtype",
                card_id: "card",
                expected_revision: 1,
                title: "changed",
                preview: "preview",
                accepted_output_type: "other"
            }
        )
        .is_err()
    );
    let output = f.run(&route, || 1, Cancellation::default()).unwrap();
    let p = proposal(output, "edit", 1);
    assert!(p.commit(&mut f.host, &f.c, || 1).is_err());
    assert!(p.commit(&mut f.host, &f.p, || 1).is_err());
    assert!(
        f.host
            .grant(&mut f.p, GrantKind::EditContent, "card", 100, 1)
            .is_err()
    );
    f.unchanged();
    f.grant_edit();
    assert_eq!(p.commit(&mut f.host, &f.c, || 1).unwrap().revision, 2);
}
#[test]
fn proposal_commit_is_idempotent_and_revision_conflicts_never_overwrite_content() {
    let mut f = Fixture::new();
    f.grant_edit();
    let route = f.route();
    let output = f.run(&route, || 1, Cancellation::default()).unwrap();
    let p = proposal(output, "edit", 1);
    let receipt = p.commit(&mut f.host, &f.c, || 1).unwrap();
    assert_eq!(receipt, p.commit(&mut f.host, &f.c, || 1).unwrap());
    let output = f.run(&route, || 1, Cancellation::default()).unwrap();
    let conflict = proposal(output, "stale-edit", 1);
    assert_eq!(
        conflict.commit(&mut f.host, &f.c, || 1),
        Err(morrow_core::Error::RevisionConflict)
    );
    let card = f.host.store_local().card("card").unwrap().unwrap();
    assert_eq!(card.summary().revision, 2);
    assert_eq!(card.body(), p.body());
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 2);
}
#[test]
fn final_commit_guard_checks_provider_route_and_expiration_before_any_write() {
    let mut baseline = Fixture::new();
    baseline.grant_edit();
    let route = baseline.route();
    let p = proposal(
        baseline.run(&route, || 1, Cancellation::default()).unwrap(),
        "edit",
        1,
    );
    let mut calls = 0;
    p.commit(&mut baseline.host, &baseline.c, || {
        calls += 1;
        1
    })
    .unwrap();
    assert!(calls >= 3);
    for mode in 0..4 {
        let mut f = Fixture::new();
        f.grant_edit();
        let route = f.route();
        let p = proposal(
            f.run(&route, || 1, Cancellation::default()).unwrap(),
            "edit",
            1,
        );
        let signal = f
            .host
            .revocation(if mode == 2 { &f.c } else { &f.p })
            .unwrap();
        let mut ticks = 0;
        assert!(
            p.commit(&mut f.host, &f.c, || {
                ticks += 1;
                if ticks == calls {
                    match mode {
                        0 | 2 => signal.revoke(),
                        1 => route.revoke(),
                        _ => {}
                    }
                }
                if ticks == calls && mode == 3 { 10 } else { 1 }
            })
            .is_err()
        );
        assert_eq!(ticks, calls);
        f.unchanged();
    }
}
#[test]
fn committed_edit_is_preserved_after_provider_and_route_revocation() {
    let mut f = Fixture::new();
    f.grant_edit();
    let route = f.route();
    let p = proposal(
        f.run(&route, || 1, Cancellation::default()).unwrap(),
        "edit",
        1,
    );
    let receipt = p.commit(&mut f.host, &f.c, || 1).unwrap();
    route.revoke();
    f.host.revocation(&f.p).unwrap().revoke();
    assert!(p.commit(&mut f.host, &f.c, || 1).is_err());
    assert_eq!(
        f.host.store_local().card("card").unwrap().unwrap().body(),
        p.body()
    );
    assert_eq!(
        f.host
            .store_local()
            .lookup_for_card("card", "edit")
            .unwrap(),
        morrow_core::transaction::Lookup::Committed(receipt)
    );
    assert_eq!(f.host.store_local().pending(0, 10).unwrap().len(), 2);
}

#[test]
fn shorter_source_lease_expiry_at_final_result_clock_denies_delivery() {
    let mut baseline = Fixture::new();
    let route = baseline.route();
    let mut calls = 0;
    baseline
        .run(
            &route,
            || {
                calls += 1;
                1
            },
            Cancellation::default(),
        )
        .unwrap();
    let mut f = Fixture::new();
    let desc = f.mapping.descriptor().clone();
    let lease = f
        .objects
        .grant(&f.host, &f.c, &desc, "workspace:a", 2, 1)
        .unwrap();
    f.mapping = f.objects.map(&f.host, &f.c, &lease, 1).unwrap();
    let route = f.route();
    let mut ticks = 0;
    assert!(
        f.run(
            &route,
            || {
                ticks += 1;
                if ticks == calls { 2 } else { 1 }
            },
            Cancellation::default()
        )
        .is_err(),
        "the source lease expires at the final delivery clock, before the route deadline"
    );
    f.unchanged();
}

#[test]
fn temporary_grant_is_reclaimed_when_provider_mapping_capacity_is_exhausted() {
    let mut f = Fixture::new();
    let mut objects = SharedObjects::new(
        &f.host,
        Limits {
            mappings: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let desc = objects
        .publish(&f.host, &f.c, "workspace:a", SOURCE, 1)
        .unwrap();
    let lease = objects
        .grant(&f.host, &f.c, &desc, "workspace:a", 100, 1)
        .unwrap();
    f.mapping = objects.map(&f.host, &f.c, &lease, 1).unwrap();
    f.objects = objects;
    let route = f.route();
    let before = f.objects.usage();
    assert!(f.run(&route, || 1, Cancellation::default()).is_err());
    assert_eq!(f.objects.usage().leases, before.leases);
    assert_eq!(f.objects.usage().mappings, 1);
    assert_eq!(f.mapping.bytes(), SOURCE);
    f.unchanged();
}
#[test]
fn foreign_broker_mapping_rejection_does_not_advance_the_valid_route_clock() {
    let mut f = Fixture::new();
    let route = f.route();
    let mut foreign = SharedObjects::new(&f.host, Limits::default()).unwrap();
    assert!(
        route
            .run(
                &mut foreign,
                &mut f.host,
                Endpoint {
                    package: &f.caller,
                    connection: &f.c
                },
                Endpoint {
                    package: &f.provider,
                    connection: &f.p
                },
                &f.mapping,
                "dependency-task",
                || 9,
                Cancellation::default()
            )
            .is_err()
    );
    assert_eq!(foreign.usage().leases, 0);
    assert!(f.run(&route, || 1, Cancellation::default()).is_ok());
}
#[test]
fn wrong_scope_with_future_time_cannot_poison_authorized_input_or_route() {
    let mut f = Fixture::new();
    let route = f.route();
    let desc = f
        .objects
        .publish(&f.host, &f.c, "workspace:b", SOURCE, 1)
        .unwrap();
    let lease = f
        .objects
        .grant(&f.host, &f.c, &desc, "workspace:b", 100, 1)
        .unwrap();
    let wrong = f.objects.map(&f.host, &f.c, &lease, 1).unwrap();
    let before = f.objects.usage();
    assert!(
        route
            .run(
                &mut f.objects,
                &mut f.host,
                Endpoint {
                    package: &f.caller,
                    connection: &f.c
                },
                Endpoint {
                    package: &f.provider,
                    connection: &f.p
                },
                &wrong,
                "dependency-task",
                || 9,
                Cancellation::default()
            )
            .is_err()
    );
    assert_eq!(f.objects.usage().leases, before.leases);
    assert_eq!(f.objects.usage().mappings, before.mappings);
    assert!(f.run(&route, || 1, Cancellation::default()).is_ok());
}

#[test]
fn source_expiring_at_sharing_clock_is_rejected_before_provider_mapping() {
    let mut f = Fixture::new();
    let desc = f.mapping.descriptor().clone();
    let lease = f
        .objects
        .grant(&f.host, &f.c, &desc, "workspace:a", 2, 1)
        .unwrap();
    f.mapping = f.objects.map(&f.host, &f.c, &lease, 1).unwrap();
    let route = f.route();
    let before = f.objects.usage();
    let mut ticks = 0;
    assert!(
        f.run(
            &route,
            || {
                ticks += 1;
                if ticks >= 2 { 2 } else { 1 }
            },
            Cancellation::default()
        )
        .is_err()
    );
    assert_eq!(
        ticks, 2,
        "reject at sharing admission before guest execution"
    );
    assert_eq!(f.objects.usage().leases, before.leases);
    assert_eq!(f.objects.usage().mappings, before.mappings);
    f.unchanged();
}
