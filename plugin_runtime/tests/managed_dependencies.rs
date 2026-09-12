#![cfg(all(feature = "packages", target_os = "windows"))]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{
        Package,
        catalog::Catalog,
        proto::{Capability, DependencyRequirement, TransformHandler},
        registry::Registry,
    },
    store::Store,
    task::{Invocation, Transform},
};
use morrow_plugin_runtime::{
    Cancellation, Limits as RuntimeLimits,
    dependency::{Dependency, DependencyOutput, Endpoint},
    manager::{ManagedInstance, Manager},
    proposal::{EditProposal, EditTarget},
    shared_objects::{Limits, Mapping, SharedObjects},
};
use std::{collections::BTreeSet, fs};
const SOURCE: &[u8] = b"fixed \0 dependency \xff input";
const CALLER: &str = "test.locked.caller";
const PROVIDER: &str = "test.locked.provider";
const SLOT: &str = "reverse";
fn package(id: &str, version: &str, dependency: Option<bool>) -> Package {
    let expected = SOURCE;
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

    let mut manifest = Package::manifest_for_transform(
        id,
        version,
        &module,
        vec![TransformHandler {
            handler: "dependency.reverse".into(),
            input_type: "bytes".into(),
            output_type: "reversed".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        }],
    );
    manifest.requested_capabilities = vec![
        Capability::EditContent as i32,
        Capability::ReadSummary as i32,
    ];
    if let Some(optional) = dependency {
        manifest.required_features.push("dependencies-v1".into());
        manifest.dependencies.push(DependencyRequirement {
            slot: SLOT.into(),
            handler: "dependency.reverse".into(),
            input_type: "bytes".into(),
            output_type: "reversed".into(),
            provider_version: "^1.0".into(),
            optional,
        });
    }
    Package::build(manifest, &module).unwrap()
}
fn activate(manager: &mut Manager, package: &Package, approvals: &[GrantKind]) {
    let id = &package.manifest().package_id;
    manager.select(package, manager.revision()).unwrap();
    manager
        .approve(
            id,
            package.digest(),
            approvals.iter().copied().collect(),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
}
fn reopen(root: &std::path::Path) -> Manager {
    Manager::new(
        Registry::open(
            &root.join("registry"),
            Catalog::open(&root.join("packages")).unwrap(),
        )
        .unwrap(),
        RuntimeLimits::default(),
    )
}
struct Fixture {
    root: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    cp: Package,
    pp: Package,
    caller: ManagedInstance,
    provider: ManagedInstance,
    objects: SharedObjects,
    mapping: Mapping,
}
impl Fixture {
    fn new(optional: bool, locked: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let cp = package(CALLER, "1.0.0", Some(optional));
        let pp = package(PROVIDER, "1.0.0", None);
        let catalog = Catalog::open(&root.path().join("packages")).unwrap();
        catalog.install(&cp).unwrap();
        catalog.install(&pp).unwrap();
        let registry = Registry::open(&root.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, RuntimeLimits::default());
        activate(&mut manager, &pp, &[]);
        manager.select(&cp, manager.revision()).unwrap();
        manager
            .approve(
                CALLER,
                cp.digest(),
                BTreeSet::from([GrantKind::EditContent]),
                manager.revision(),
            )
            .unwrap();
        if locked {
            manager
                .approve_dependency(
                    CALLER,
                    cp.digest(),
                    SLOT,
                    PROVIDER,
                    pp.digest(),
                    manager.revision(),
                )
                .unwrap();
        }
        manager
            .set_enabled(CALLER, cp.digest(), true, manager.revision())
            .unwrap();
        let mut store = Store::open(&root.path().join("db"), Default::default()).unwrap();
        store
            .create_local(
                "seed",
                &CardRecord::new("card", "test.card", 1, "original", b"old".to_vec()).unwrap(),
            )
            .unwrap();
        let mut host = HostRuntime::new(store).unwrap();
        let caller = manager.connect(CALLER, &mut host).unwrap();
        let provider = manager.connect(PROVIDER, &mut host).unwrap();
        let mut objects = SharedObjects::new(&host, Limits::default()).unwrap();
        let mut source = SOURCE.to_vec();
        let desc = objects
            .publish(&host, caller.connection(), "workspace:a", &source, 1)
            .unwrap();
        source.fill(0);
        let lease = objects
            .grant(&host, caller.connection(), &desc, "workspace:a", 100, 1)
            .unwrap();
        let mapping = objects.map(&host, caller.connection(), &lease, 1).unwrap();
        Self {
            root,
            manager,
            host,
            cp,
            pp,
            caller,
            provider,
            objects,
            mapping,
        }
    }
    fn route(&self) -> Dependency {
        self.manager
            .bind_locked_dependency(
                &self.host,
                &self.caller,
                &self.provider,
                SLOT,
                "workspace:a",
                10,
                1,
            )
            .unwrap()
    }
    fn run(
        &mut self,
        route: &Dependency,
    ) -> Result<DependencyOutput, morrow_plugin_runtime::dependency::Error> {
        route.run(
            &mut self.objects,
            &mut self.host,
            Endpoint {
                package: self.caller.package(),
                connection: self.caller.connection(),
            },
            Endpoint {
                package: self.provider.package(),
                connection: self.provider.connection(),
            },
            &self.mapping,
            "dependency-task",
            || 1,
            Cancellation::default(),
        )
    }
    fn unchanged(&self) {
        assert_eq!(
            self.host
                .store_local()
                .card("card")
                .unwrap()
                .unwrap()
                .body(),
            b"old"
        );
        assert_eq!(self.host.store_local().pending(0, 10).unwrap().len(), 1);
    }
    fn stopped(&self) {
        assert_ne!(
            self.host.connection_phase(self.caller.connection()),
            Ok(InstancePhase::Ready)
        );
        assert_ne!(
            self.host.connection_phase(self.provider.connection()),
            Ok(InstancePhase::Ready)
        );
    }
}
fn proposal(output: DependencyOutput) -> EditProposal {
    EditProposal::new(
        output,
        EditTarget {
            operation_id: "edit",
            card_id: "card",
            expected_revision: 1,
            title: "changed",
            preview: "dependency",
            accepted_output_type: "reversed",
        },
    )
    .unwrap()
}

#[test]
fn persisted_selection_and_lock_rebuild_fresh_instances_and_real_wasm_route() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let previous = f.run(&route).unwrap();
    assert_eq!(
        previous.bytes(),
        SOURCE.iter().rev().copied().collect::<Vec<_>>()
    );
    let saved = f.manager.dependency(CALLER, SLOT).unwrap().clone();
    let revision = f.manager.revision();
    // Replace only the manager; the same business HostRuntime remains uniquely owned.
    let placeholder = tempfile::tempdir().unwrap();
    let old = std::mem::replace(&mut f.manager, reopen(placeholder.path()));
    drop(old);
    assert!(
        previous
            .validate(&f.host, f.caller.connection(), 1)
            .is_err()
    );
    f.manager = reopen(f.root.path());
    assert_eq!(f.manager.revision(), revision);
    assert_eq!(f.manager.dependency(CALLER, SLOT).unwrap(), &saved);
    f.caller.close(&mut f.host).unwrap();
    f.provider.close(&mut f.host).unwrap();
    f.caller = f.manager.connect(CALLER, &mut f.host).unwrap();
    f.provider = f.manager.connect(PROVIDER, &mut f.host).unwrap();
    let desc = f
        .objects
        .publish(&f.host, f.caller.connection(), "workspace:a", SOURCE, 1)
        .unwrap();
    let lease = f
        .objects
        .grant(&f.host, f.caller.connection(), &desc, "workspace:a", 100, 1)
        .unwrap();
    f.mapping = f
        .objects
        .map(&f.host, f.caller.connection(), &lease, 1)
        .unwrap();
    let fresh = f.route();
    assert_eq!(f.run(&fresh).unwrap().bytes(), previous.bytes());
    assert!(f.run(&route).is_err());
    f.unchanged();
}
#[test]
fn provider_upgrade_disable_remove_and_permissions_revoke_required_caller_and_old_output() {
    for mode in 0..4 {
        let mut f = Fixture::new(false, true);
        let route = f.route();
        let output = f.run(&route).unwrap();
        let edit = proposal(output);
        f.host
            .grant(
                f.caller.parts_mut().1,
                GrantKind::EditContent,
                "card",
                100,
                1,
            )
            .unwrap();
        let rev = f.manager.revision();
        match mode {
            0 => {
                let newer = package(PROVIDER, "1.1.0", None);
                Catalog::open(&f.root.path().join("packages"))
                    .unwrap()
                    .install(&newer)
                    .unwrap();
                f.manager.select(&newer, rev).unwrap();
            }
            1 => f
                .manager
                .set_enabled(PROVIDER, f.pp.digest(), false, rev)
                .unwrap(),
            2 => f.manager.remove(PROVIDER, rev).unwrap(),
            _ => f
                .manager
                .approve(
                    PROVIDER,
                    f.pp.digest(),
                    BTreeSet::from([GrantKind::ReadSummary]),
                    rev,
                )
                .unwrap(),
        }
        f.stopped();
        assert!(f.run(&route).is_err());
        assert!(
            edit.commit(&mut f.host, f.caller.connection(), || 1)
                .is_err()
        );
        f.unchanged();
    }
}
#[test]
fn stale_revision_for_all_manager_changes_preserves_live_instances_and_route() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let stale = f.manager.revision() - 1;
    assert!(f.manager.select(&f.pp, stale).is_err());
    assert!(
        f.manager
            .approve(
                PROVIDER,
                f.pp.digest(),
                BTreeSet::from([GrantKind::ReadSummary]),
                stale
            )
            .is_err()
    );
    assert!(
        f.manager
            .set_enabled(PROVIDER, f.pp.digest(), false, stale)
            .is_err()
    );
    assert!(f.manager.remove(PROVIDER, stale).is_err());
    assert!(
        f.manager
            .approve_dependency(CALLER, f.cp.digest(), SLOT, PROVIDER, f.pp.digest(), stale)
            .is_err()
    );
    assert!(f.manager.remove_dependency(CALLER, SLOT, stale).is_err());
    assert_eq!(
        f.host.connection_phase(f.caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(
        f.host.connection_phase(f.provider.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(f.run(&route).is_ok());
}
#[test]
fn failed_durable_publication_stops_transitive_instances_without_rewriting_old_lock() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let output = f.run(&route).unwrap();
    let revision = f.manager.revision();
    let lock = f.manager.dependency(CALLER, SLOT).unwrap().clone();
    let path = f.root.path().join("registry/selection.morrow");
    let saved = f.root.path().join("registry/saved");
    let bytes = fs::read(&path).unwrap();
    fs::rename(&path, &saved).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        f.manager
            .set_enabled(PROVIDER, f.pp.digest(), false, revision)
            .is_err()
    );
    assert_eq!(f.manager.revision(), revision);
    assert_eq!(f.manager.dependency(CALLER, SLOT), Some(&lock));
    assert!(f.manager.selection(PROVIDER).unwrap().enabled);
    f.stopped();
    assert!(output.validate(&f.host, f.caller.connection(), 1).is_err());
    assert!(f.run(&route).is_err());
    fs::remove_dir(&path).unwrap();
    fs::rename(&saved, &path).unwrap();
    assert_eq!(fs::read(path).unwrap(), bytes);
    f.unchanged();
}
#[test]
fn same_package_instances_from_another_manager_cannot_borrow_this_managers_lock() {
    let mut f = Fixture::new(false, true);
    let foreign_root = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(&foreign_root.path().join("packages")).unwrap();
    catalog.install(&f.cp).unwrap();
    catalog.install(&f.pp).unwrap();
    let mut foreign = reopen(foreign_root.path());
    activate(&mut foreign, &f.pp, &[]);
    foreign.select(&f.cp, foreign.revision()).unwrap();
    foreign
        .approve_dependency(
            CALLER,
            f.cp.digest(),
            SLOT,
            PROVIDER,
            f.pp.digest(),
            foreign.revision(),
        )
        .unwrap();
    foreign
        .set_enabled(CALLER, f.cp.digest(), true, foreign.revision())
        .unwrap();
    let c = foreign.connect(CALLER, &mut f.host).unwrap();
    let p = foreign.connect(PROVIDER, &mut f.host).unwrap();
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &c, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &f.caller, &p, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    assert!(
        foreign
            .bind_locked_dependency(&f.host, &f.caller, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    assert!(
        f.manager
            .bind_locked_dependency(
                &f.host,
                &f.caller,
                &f.provider,
                "guest-chosen-slot",
                "workspace:a",
                10,
                1
            )
            .is_err()
    );
    assert!(f.run(&f.route()).is_ok());
}
#[test]
fn optional_unbound_requirement_does_not_block_caller_but_cannot_create_route() {
    let f = Fixture::new(true, false);
    assert_eq!(
        f.host.connection_phase(f.caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(f.manager.dependency(CALLER, SLOT).is_none());
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &f.caller, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
}
#[test]
fn optional_bound_provider_stop_invalidates_route_without_stopping_unrelated_caller() {
    let mut f = Fixture::new(true, true);
    let route = f.route();
    let output = f.run(&route).unwrap();
    f.manager
        .set_enabled(PROVIDER, f.pp.digest(), false, f.manager.revision())
        .unwrap();
    assert_eq!(
        f.host.connection_phase(f.caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(output.validate(&f.host, f.caller.connection(), 1).is_err());
    assert!(f.run(&route).is_err());
}
#[test]
fn removing_required_lock_revokes_caller_and_blocks_reconnect_until_explicit_reapproval() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let output = f.run(&route).unwrap();
    f.manager
        .remove_dependency(CALLER, SLOT, f.manager.revision())
        .unwrap();
    assert!(f.manager.dependency(CALLER, SLOT).is_none());
    assert_ne!(
        f.host.connection_phase(f.caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert_eq!(
        f.host.connection_phase(f.provider.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(output.validate(&f.host, f.caller.connection(), 1).is_err());
    assert!(f.manager.connect(CALLER, &mut f.host).is_err());
    f.manager
        .approve_dependency(
            CALLER,
            f.cp.digest(),
            SLOT,
            PROVIDER,
            f.pp.digest(),
            f.manager.revision(),
        )
        .unwrap();
    assert!(f.manager.connect(CALLER, &mut f.host).is_ok());
}
#[test]
fn required_three_package_chain_revokes_all_live_transitive_consumers() {
    let mut f = Fixture::new(false, true);
    let middle = package("test.locked.middle", "1.0.0", Some(false));
    Catalog::open(&f.root.path().join("packages"))
        .unwrap()
        .install(&middle)
        .unwrap();
    f.manager.select(&middle, f.manager.revision()).unwrap();
    f.manager
        .approve_dependency(
            "test.locked.middle",
            middle.digest(),
            SLOT,
            PROVIDER,
            f.pp.digest(),
            f.manager.revision(),
        )
        .unwrap();
    f.manager
        .set_enabled(
            "test.locked.middle",
            middle.digest(),
            true,
            f.manager.revision(),
        )
        .unwrap();
    f.manager
        .approve_dependency(
            CALLER,
            f.cp.digest(),
            SLOT,
            "test.locked.middle",
            middle.digest(),
            f.manager.revision(),
        )
        .unwrap();
    let caller = f.manager.connect(CALLER, &mut f.host).unwrap();
    let provider = f
        .manager
        .connect("test.locked.middle", &mut f.host)
        .unwrap();
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &caller, &provider, SLOT, "workspace:a", 10, 1)
            .is_ok()
    );
    f.manager
        .set_enabled(PROVIDER, f.pp.digest(), false, f.manager.revision())
        .unwrap();
    assert_ne!(
        f.host.connection_phase(caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert_ne!(
        f.host.connection_phase(provider.connection()),
        Ok(InstancePhase::Ready)
    );
    assert_ne!(
        f.host.connection_phase(f.provider.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(f.manager.connect(CALLER, &mut f.host).is_err());
    assert!(
        f.manager
            .connect("test.locked.middle", &mut f.host)
            .is_err()
    );
}
#[test]
fn provider_change_at_final_proposal_commit_guard_refuses_write() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let edit = proposal(f.run(&route).unwrap());
    f.host
        .grant(
            f.caller.parts_mut().1,
            GrantKind::EditContent,
            "card",
            100,
            1,
        )
        .unwrap();
    let mut clocks = 0;
    assert!(
        edit.commit(&mut f.host, f.caller.connection(), || {
            clocks += 1;
            if clocks == 3 {
                f.manager
                    .set_enabled(PROVIDER, f.pp.digest(), false, f.manager.revision())
                    .unwrap();
            }
            1
        })
        .is_err()
    );
    assert_eq!(clocks, 3);
    f.stopped();
    f.unchanged();
}

#[test]
fn swapping_same_package_connections_cannot_rebind_control_or_close_another_instance() {
    let mut f = Fixture::new(false, true);
    let mut second = f.manager.connect(CALLER, &mut f.host).unwrap();
    std::mem::swap(f.caller.parts_mut().1, second.parts_mut().1);
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &f.caller, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &second, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    assert!(f.caller.run(&mut f.host, || 1).outcome.is_err());
    assert!(second.run(&mut f.host, || 1).outcome.is_err());
    let input = Invocation::new_transform(
        "dependency-task",
        Transform {
            handler: "dependency.reverse".into(),
            input_type: "bytes".into(),
            output_type: "reversed".into(),
            input: SOURCE.to_vec(),
        },
    )
    .unwrap();
    assert!(
        f.caller
            .run_task(&mut f.host, &input, || 1)
            .execution
            .outcome
            .is_err()
    );
    // Closing the first control revokes its ORIGINAL instance (now carried by second),
    // but must not disconnect the unrelated connection now residing in the first wrapper.
    assert!(f.caller.close(&mut f.host).is_err());
    assert_eq!(
        f.host.connection_phase(f.caller.connection()),
        Ok(InstancePhase::Ready)
    );
    assert_ne!(
        f.host.connection_phase(second.connection()),
        Ok(InstancePhase::Ready)
    );
    f.unchanged();
}
#[test]
fn swapped_provider_connection_does_not_revive_output_after_original_control_stop() {
    let mut f = Fixture::new(false, true);
    let route = f.route();
    let output = f.run(&route).unwrap();
    let mut second = f.manager.connect(PROVIDER, &mut f.host).unwrap();
    std::mem::swap(f.provider.parts_mut().1, second.parts_mut().1);
    assert!(
        f.manager
            .bind_locked_dependency(&f.host, &f.caller, &f.provider, SLOT, "workspace:a", 10, 1)
            .is_err()
    );
    f.provider.stop();
    assert_eq!(
        f.host.connection_phase(f.provider.connection()),
        Ok(InstancePhase::Ready)
    );
    assert!(output.validate(&f.host, f.caller.connection(), 1).is_err());
    assert!(f.run(&route).is_err());
    assert!(f.provider.run(&mut f.host, || 1).outcome.is_err());
    f.manager
        .set_enabled(PROVIDER, f.pp.digest(), false, f.manager.revision())
        .unwrap();
    f.stopped();
    assert_ne!(
        f.host.connection_phase(second.connection()),
        Ok(InstancePhase::Ready)
    );
}
