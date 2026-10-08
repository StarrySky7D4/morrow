#![cfg(all(feature = "package-management", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::Capability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Fault, Limits, Runner,
    manager::{Manager, ManagerError},
};
use std::{collections::BTreeSet, sync::Arc};

const ID: &str = "org.example.managed-agent";
fn wasm(extra: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (import "morrow_agent_session_process_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
        {extra}
        (memory (export "memory") 1)
        (func (export "morrow_run") (result i32) i32.const 0))"#
    ))
    .unwrap()
}
fn package(version: &str, extra: &str, capabilities: Vec<Capability>) -> Package {
    let module = wasm(extra);
    Package::build(
        Package::manifest_for_task(ID, version, &module, capabilities),
        &module,
    )
    .unwrap()
}
fn setup(limits: Limits) -> (tempfile::TempDir, Manager, HostRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::open(&temp.path().join("packages")).unwrap();
    let registry = Registry::open(&temp.path().join("registry"), catalog).unwrap();
    let manager = Manager::new(registry, limits);
    let host = HostRuntime::new(
        Store::open(
            &temp.path().join("synthetic.sqlite"),
            EventBudget::default(),
        )
        .unwrap(),
    )
    .unwrap();
    (temp, manager, host)
}
fn enable(manager: &mut Manager, package: &Package) {
    manager.install_package(package.archive()).unwrap();
    manager.select(package, manager.revision()).unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
}

#[test]
fn wrapper_revoke_stops_all_original_controls_without_publishing_or_reviving_them() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let base = package("1.0.0", "", vec![]);
    enable(&mut manager, &base);
    let revision = manager.revision();
    let snapshot = manager.persisted_snapshot().unwrap();
    let first = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    let second = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    let connection = first.shared_connection();
    let revocation = host.revocation(&connection).unwrap();
    manager
        .revoke_agent_session_process(ID, base.digest(), revision)
        .unwrap();
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.persisted_snapshot().unwrap(), snapshot);
    assert!(manager.selection(ID).unwrap().enabled);
    assert!(revocation.is_revoked());
    assert!(!first.is_active());
    assert!(!second.is_active());
    assert!(
        manager
            .validate_agent_session_process(&first, &host, revision)
            .is_err()
    );
    let fresh = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    assert!(fresh.is_active());
    assert!(!Arc::ptr_eq(&connection, &fresh.shared_connection()));
    assert!(!first.is_active());
    first.close(&mut host).unwrap();
    second.close(&mut host).unwrap();
    fresh.close(&mut host).unwrap();
}

#[test]
fn wrapper_revoke_rejects_stale_or_foreign_selection_without_stopping_controls() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let base = package("1.0.0", "", vec![]);
    enable(&mut manager, &base);
    let revision = manager.revision();
    let instance = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    let connection = instance.shared_connection();
    let revocation = host.revocation(&connection).unwrap();
    let snapshot = manager.persisted_snapshot().unwrap();
    for (id, digest, expected, error) in [
        (ID, base.digest(), revision - 1, Error::RevisionConflict),
        (ID, [0; 32], revision, Error::RevisionConflict),
        (
            "org.example.absent",
            base.digest(),
            revision,
            Error::NotFound,
        ),
    ] {
        assert!(matches!(
            manager.revoke_agent_session_process(id, digest, expected),
            Err(ManagerError::Core(found)) if found == error
        ));
        assert!(instance.is_active());
        assert!(!revocation.is_revoked());
        manager
            .validate_agent_session_process(&instance, &host, revision)
            .unwrap();
    }
    assert_eq!(manager.revision(), revision);
    assert_eq!(manager.persisted_snapshot().unwrap(), snapshot);
    instance.close(&mut host).unwrap();
}

#[test]
fn wrapper_revoke_conservatively_stops_an_ordinary_instance_of_the_same_base() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let module = wat::parse_str(
        r#"(module (memory (export "memory") 1)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let base =
        Package::build(Package::manifest_for(ID, "1.0.0", &module, vec![]), &module).unwrap();
    enable(&mut manager, &base);
    let revision = manager.revision();
    let ordinary = manager.connect(ID, &mut host).unwrap();
    assert_eq!(ordinary.run(&mut host, || 1).outcome, Ok(0));
    manager
        .revoke_agent_session_process(ID, base.digest(), revision)
        .unwrap();
    assert_eq!(manager.revision(), revision);
    assert!(ordinary.run(&mut host, || 1).outcome.is_err());
    ordinary.close(&mut host).unwrap();
}

#[test]
fn exact_selection_limits_and_original_shared_connection() {
    let (_temp, mut manager, mut host) = setup(Limits {
        fuel: 9000,
        memory_bytes: 131072,
        host_calls: 8,
    });
    let module = wasm("");
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &module, vec![]);
    let budget = manifest.budget.as_mut().unwrap();
    budget.fuel = 5000;
    budget.memory_bytes = 65536;
    budget.host_calls = 4;
    let package = Package::build(manifest, &module).unwrap();
    enable(&mut manager, &package);
    assert!(manager.connect(ID, &mut host).is_err());
    let revision = manager.revision();
    let instance = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    assert_eq!(instance.package().digest(), package.digest());
    assert_eq!(instance.limits().fuel, 5000);
    assert_eq!(instance.limits().memory_bytes, 65536);
    assert_eq!(instance.limits().host_calls, 4);
    let first = instance.shared_connection();
    assert!(Arc::ptr_eq(&first, &instance.shared_connection()));
    assert_eq!(first.package_digest(), Some(package.digest()));
    assert!(instance.is_active());
    manager
        .validate_agent_session_process(&instance, &host, revision)
        .unwrap();
    instance.close(&mut host).unwrap();
    assert!(!instance.is_active());
    assert!(
        manager
            .validate_agent_session_process(&instance, &host, revision)
            .is_err()
    );
}

#[test]
fn stale_revision_wrong_manager_and_wrong_host_cannot_validate() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let package = package("1.0.0", "", vec![]);
    enable(&mut manager, &package);
    let revision = manager.revision();
    assert!(matches!(
        manager.connect_agent_session_process(ID, &mut host, revision - 1),
        Err(ManagerError::Core(Error::RevisionConflict))
    ));
    let instance = manager
        .connect_agent_session_process(ID, &mut host, revision)
        .unwrap();
    let (_other_temp, other, mut other_host) = setup(Limits::default());
    assert!(
        other
            .validate_agent_session_process(&instance, &host, other.revision())
            .is_err()
    );
    assert!(
        manager
            .validate_agent_session_process(&instance, &other_host, revision)
            .is_err()
    );
    assert!(instance.close(&mut other_host).is_err());
    assert!(!instance.is_active());
}

#[test]
fn selection_mutations_stop_original_control_and_never_revive_it() {
    for mutation in ["disable", "upgrade", "remove"] {
        let (_temp, mut manager, mut host) = setup(Limits::default());
        let original = package("1.0.0", "", vec![]);
        enable(&mut manager, &original);
        let revision = manager.revision();
        let instance = manager
            .connect_agent_session_process(ID, &mut host, revision)
            .unwrap();
        match mutation {
            "disable" => manager
                .set_enabled(ID, original.digest(), false, revision)
                .unwrap(),
            "upgrade" => {
                let next = package("2.0.0", "", vec![]);
                manager.install_package(next.archive()).unwrap();
                manager.select(&next, revision).unwrap();
            }
            "remove" => manager.remove(ID, revision).unwrap(),
            _ => unreachable!(),
        }
        assert!(!instance.is_active());
        assert!(
            manager
                .validate_agent_session_process(&instance, &host, manager.revision())
                .is_err()
        );
        if mutation == "remove" {
            manager.select(&original, manager.revision()).unwrap();
        }
        let selected_digest = manager.selection(ID).unwrap().digest;
        manager
            .set_enabled(ID, selected_digest, true, manager.revision())
            .unwrap();
        assert!(!instance.is_active());
    }
}

#[test]
fn instance_and_manager_drop_stop_the_original_shared_connection() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let package = package("1.0.0", "", vec![]);
    enable(&mut manager, &package);
    let instance = manager
        .connect_agent_session_process(ID, &mut host, manager.revision())
        .unwrap();
    let connection = instance.shared_connection();
    let revocation = host.revocation(&connection).unwrap();
    let cancellation = instance.cancellation();
    drop(instance);
    assert!(revocation.is_revoked());
    let module = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let runner = Runner::new(&module, Limits::default()).unwrap();
    assert_eq!(
        runner.run(&mut |_| Err(()), cancellation).outcome,
        Err(Fault::Cancelled)
    );
    let instance = manager
        .connect_agent_session_process(ID, &mut host, manager.revision())
        .unwrap();
    let revocation = host.revocation(&instance.shared_connection()).unwrap();
    drop(manager);
    assert!(!instance.is_active());
    assert!(revocation.is_revoked());
}

#[test]
fn old_imports_and_core_capabilities_cannot_mix_into_fixed_profile() {
    for extra in [
        r#"(import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_mutation_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
        r#"(import "morrow_agent_session_exec_v1" "call" (func (param i32 i32 i32 i32) (result i32)))"#,
    ] {
        let (_temp, mut manager, mut host) = setup(Limits::default());
        enable(&mut manager, &package("1.0.0", extra, vec![]));
        assert!(matches!(
            manager.connect_agent_session_process(ID, &mut host, manager.revision()),
            Err(ManagerError::Prepare(Fault::UnsupportedAbi))
        ));
    }
    let (_temp, mut manager, mut host) = setup(Limits::default());
    let package = package("1.0.0", "", vec![Capability::ReadSummary]);
    enable(&mut manager, &package);
    manager
        .approve(ID, package.digest(), BTreeSet::new(), manager.revision())
        .unwrap();
    assert!(
        manager
            .connect_agent_session_process(ID, &mut host, manager.revision())
            .is_err()
    );
}

#[test]
fn fixed_and_ordinary_instances_share_the_original_total_quota() {
    let (_temp, mut manager, mut host) = setup(Limits::default());
    enable(&mut manager, &package("1.0.0", "", vec![]));
    let module = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let ordinary = Package::build(
        Package::manifest_for("org.example.ordinary", "1.0.0", &module, vec![]),
        &module,
    )
    .unwrap();
    manager.install_package(ordinary.archive()).unwrap();
    manager.select(&ordinary, manager.revision()).unwrap();
    manager
        .set_enabled(
            "org.example.ordinary",
            ordinary.digest(),
            true,
            manager.revision(),
        )
        .unwrap();
    let ordinary_instances: Vec<_> = (0..64)
        .map(|_| manager.connect("org.example.ordinary", &mut host).unwrap())
        .collect();
    let mut agents: Vec<_> = (0..64)
        .map(|_| {
            manager
                .connect_agent_session_process(ID, &mut host, manager.revision())
                .unwrap()
        })
        .collect();
    assert!(matches!(
        manager.connect("org.example.ordinary", &mut host),
        Err(ManagerError::Core(Error::Limit))
    ));
    assert!(matches!(
        manager.connect_agent_session_process(ID, &mut host, manager.revision()),
        Err(ManagerError::Core(Error::Limit))
    ));
    agents.pop().unwrap().close(&mut host).unwrap();
    let replacement = manager
        .connect_agent_session_process(ID, &mut host, manager.revision())
        .unwrap();
    assert!(replacement.is_active());
    assert_eq!(ordinary_instances.len(), 64);
}
