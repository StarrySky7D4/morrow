#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    Error,
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::{GrantKind, InstancePhase},
    plugin_package::{Package, catalog::Catalog, proto::Capability, registry::Registry},
    response::{Failure, Outcome, Response},
    runtime::{Command, RenameRequest},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    manager::{Manager, ManagerError},
};
use std::{collections::BTreeSet, fs};
const ID: &str = "org.example.managed";
fn package(id: &str, version: &str) -> Package {
    let module = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 7))"#).unwrap();
    Package::build(
        Package::manifest_for(
            id,
            version,
            &module,
            vec![Capability::RenameCard, Capability::ReadSummary],
        ),
        &module,
    )
    .unwrap()
}
fn setup() -> (tempfile::TempDir, Manager, HostRuntime, Package) {
    let dir = tempfile::tempdir().unwrap();
    let package = package(ID, "1.0.0");
    let catalog = Catalog::open(&dir.path().join("packages")).unwrap();
    catalog.install(&package).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let manager = Manager::new(registry, Limits::default());
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new("card", "text", 1, "secret title", b"body".to_vec()).unwrap(),
        )
        .unwrap();
    let host = HostRuntime::new(store).unwrap();
    (dir, manager, host, package)
}
fn activate(manager: &mut Manager, package: &Package, approvals: &[GrantKind]) {
    manager.select(package, manager.revision()).unwrap();
    manager
        .approve(
            ID,
            package.digest(),
            approvals.iter().copied().collect(),
            manager.revision(),
        )
        .unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
}
fn read() -> Vec<u8> {
    Command::ReadSummary {
        request_id: "query".into(),
        card_id: "card".into(),
    }
    .encode()
    .unwrap()
}
#[test]
fn enabled_registry_approval_is_bound_to_the_real_connection() {
    let (_dir, mut manager, mut host, package) = setup();
    assert!(manager.connect(ID, &mut host).is_err());
    manager.select(&package, manager.revision()).unwrap();
    assert!(manager.connect(ID, &mut host).is_err());
    activate(&mut manager, &package, &[GrantKind::ReadSummary]);
    let mut instance = manager.connect(ID, &mut host).unwrap();
    let (_, connection) = instance.parts_mut();
    assert!(
        host.grant(connection, GrantKind::Rename, "card", 100, 0)
            .is_err()
    );
    host.grant(connection, GrantKind::ReadSummary, "card", 100, 0)
        .unwrap();
    assert!(matches!(
        Response::decode(&host.dispatch(instance.connection(), &read(), || 1).unwrap())
            .unwrap()
            .outcome,
        Outcome::Summary(_)
    ));
    assert_eq!(instance.run(&mut host, || 1).outcome, Ok(7));
    let another = manager.connect(ID, &mut host).unwrap();
    assert_ne!(
        instance.connection().binding(),
        another.connection().binding()
    );
    assert_eq!(
        format!("{:?}", instance.connection().binding()),
        "ConnectionBinding(..)"
    );
    instance.close(&mut host).unwrap();
    another.close(&mut host).unwrap();
}
#[test]
fn every_selection_or_approval_change_revokes_all_prior_instances() {
    for change in ["disable", "narrow", "upgrade", "remove"] {
        let (dir, mut manager, mut host, package) = setup();
        activate(
            &mut manager,
            &package,
            &[GrantKind::Rename, GrantKind::ReadSummary],
        );
        let mut one = manager.connect(ID, &mut host).unwrap();
        let two = manager.connect(ID, &mut host).unwrap();
        host.grant(one.parts_mut().1, GrantKind::ReadSummary, "card", 100, 0)
            .unwrap();
        match change {
            "disable" => manager
                .set_enabled(ID, package.digest(), false, manager.revision())
                .unwrap(),
            "narrow" => manager
                .approve(
                    ID,
                    package.digest(),
                    BTreeSet::from([GrantKind::Rename]),
                    manager.revision(),
                )
                .unwrap(),
            "upgrade" => {
                let next = self::package(ID, "2.0.0");
                Catalog::open(&dir.path().join("packages"))
                    .unwrap()
                    .install(&next)
                    .unwrap();
                manager.select(&next, manager.revision()).unwrap();
            }
            "remove" => manager.remove(ID, manager.revision()).unwrap(),
            _ => unreachable!(),
        }
        for old in [&one, &two] {
            assert_eq!(
                host.connection_phase(old.connection()).unwrap(),
                InstancePhase::Revoked
            );
            let report = old.run(&mut host, || panic!("revoked guest must not call host"));
            assert_eq!(report.outcome, Err(Fault::InactiveConnection));
            assert_eq!(report.host_calls, 0);
        }
        assert!(
            host.grant(one.parts_mut().1, GrantKind::ReadSummary, "card", 100, 1)
                .is_err()
        );
        assert_eq!(
            Response::decode(&host.dispatch(one.connection(), &read(), || 1).unwrap())
                .unwrap()
                .outcome,
            Outcome::Rejected(Failure::Denied)
        );
        if change == "narrow" {
            let mut new = manager.connect(ID, &mut host).unwrap();
            assert!(
                host.grant(new.parts_mut().1, GrantKind::ReadSummary, "card", 100, 1)
                    .is_err()
            );
            host.grant(new.parts_mut().1, GrantKind::Rename, "card", 100, 1)
                .unwrap();
            new.close(&mut host).unwrap();
        } else {
            assert!(manager.connect(ID, &mut host).is_err());
        }
        one.close(&mut host).unwrap();
        two.close(&mut host).unwrap();
    }
}
#[test]
fn disable_at_read_release_refuses_late_data() {
    let (_dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[GrantKind::ReadSummary]);
    let mut instance = manager.connect(ID, &mut host).unwrap();
    host.grant(
        instance.parts_mut().1,
        GrantKind::ReadSummary,
        "card",
        100,
        0,
    )
    .unwrap();
    let mut ticks = 0;
    let response = host
        .dispatch(instance.connection(), &read(), || {
            ticks += 1;
            if ticks == 2 {
                manager
                    .set_enabled(ID, package.digest(), false, manager.revision())
                    .unwrap();
            }
            1
        })
        .unwrap();
    assert_eq!(ticks, 2);
    assert_eq!(
        Response::decode(&response).unwrap().outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert!(
        !response
            .windows(b"secret title".len())
            .any(|b| b == b"secret title")
    );
    instance.close(&mut host).unwrap();
}
#[test]
fn failed_persistence_keeps_old_state_but_never_revives_the_stopped_instance() {
    let (dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[GrantKind::ReadSummary]);
    let instance = manager.connect(ID, &mut host).unwrap();
    let revision = manager.revision();
    let path = dir.path().join("registry/selection.morrow");
    let saved = dir.path().join("registry/saved");
    fs::rename(&path, &saved).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(
        manager
            .approve(ID, package.digest(), BTreeSet::new(), revision)
            .is_err()
    );
    assert_eq!(manager.revision(), revision);
    assert_eq!(
        manager.selection(ID).unwrap().approved,
        BTreeSet::from([GrantKind::ReadSummary])
    );
    assert_eq!(
        instance.run(&mut host, || 1).outcome,
        Err(Fault::InactiveConnection)
    );
    fs::remove_dir(&path).unwrap();
    fs::rename(saved, path).unwrap();
    manager
        .approve(ID, package.digest(), BTreeSet::new(), revision)
        .unwrap();
    let mut new = manager.connect(ID, &mut host).unwrap();
    assert!(
        host.grant(new.parts_mut().1, GrantKind::ReadSummary, "card", 100, 1)
            .is_err()
    );
    assert_eq!(
        instance.run(&mut host, || 1).outcome,
        Err(Fault::InactiveConnection)
    );
    instance.close(&mut host).unwrap();
    new.close(&mut host).unwrap();
}
#[test]
fn stale_confirmation_does_not_stop_a_valid_instance() {
    let (_dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[GrantKind::ReadSummary]);
    let instance = manager.connect(ID, &mut host).unwrap();
    assert!(matches!(
        manager.remove(ID, manager.revision() - 1),
        Err(ManagerError::Core(Error::RevisionConflict))
    ));
    assert!(matches!(
        manager.approve(ID, [9; 32], BTreeSet::new(), manager.revision()),
        Err(ManagerError::Core(Error::RevisionConflict))
    ));
    assert_eq!(instance.run(&mut host, || 1).outcome, Ok(7));
    instance.close(&mut host).unwrap();
}
#[test]
fn manager_drop_stops_escaped_instances_and_foreign_hosts_cannot_grant() {
    let (_dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[GrantKind::ReadSummary]);
    let mut instance = manager.connect(ID, &mut host).unwrap();
    let (_other_dir, _other_manager, mut other_host, _other_package) = setup();
    assert!(
        other_host
            .grant(
                instance.parts_mut().1,
                GrantKind::ReadSummary,
                "card",
                100,
                0
            )
            .is_err()
    );
    assert_eq!(
        instance.run(&mut other_host, || 1).outcome,
        Err(Fault::InactiveConnection)
    );
    drop(manager);
    assert_eq!(
        instance.run(&mut host, || 1).outcome,
        Err(Fault::InactiveConnection)
    );
    instance.close(&mut host).unwrap();
}
#[test]
fn stopping_never_rolls_back_an_already_committed_operation() {
    let (_dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[GrantKind::Rename]);
    let mut instance = manager.connect(ID, &mut host).unwrap();
    host.grant(instance.parts_mut().1, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    let command = RenameRequest {
        operation_id: "rename".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "committed title".into(),
    }
    .encode()
    .unwrap();
    let result = host
        .dispatch(instance.connection(), &command, || 1)
        .unwrap();
    assert!(matches!(
        Response::decode(&result).unwrap().outcome,
        Outcome::Renamed(_)
    ));
    manager
        .set_enabled(ID, package.digest(), false, manager.revision())
        .unwrap();
    assert_eq!(
        host.store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .title,
        "committed title"
    );
    assert_eq!(
        Response::decode(
            &host
                .dispatch(instance.connection(), &command, || 1)
                .unwrap()
        )
        .unwrap()
        .outcome,
        Outcome::Rejected(Failure::Denied)
    );
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
    instance.close(&mut host).unwrap();
}

#[test]
fn revoked_pure_transform_cannot_release_results_or_use_a_new_host() {
    use morrow_core::{
        plugin_package::proto::TransformHandler,
        task::{Invocation, Transform},
    };
    let (dir, mut manager, mut host, _) = setup();
    let input = Invocation::new_transform(
        "job",
        Transform {
            handler: "echo".into(),
            input_type: "bytes".into(),
            output_type: "bytes".into(),
            input: vec![1],
        },
    )
    .unwrap();
    let done = input.output_completion(&[2]).unwrap();
    let data = done
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect::<String>();
    let module = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (memory (export "memory") 4) (data (i32.const 0) "{data}")
        (func (export "morrow_run") (result i32)
         i32.const 65536 i32.const 131072 call $read drop
         i32.const 0 i32.const {} call $done drop i32.const 0))"#,
        done.len()
    ))
    .unwrap();
    let package = Package::build(
        Package::manifest_for_transform(
            ID,
            "1.0.0",
            &module,
            vec![TransformHandler {
                handler: "echo".into(),
                input_type: "bytes".into(),
                output_type: "bytes".into(),
                max_input_bytes: 8,
                max_output_bytes: 8,
            }],
        ),
        &module,
    )
    .unwrap();
    Catalog::open(&dir.path().join("packages"))
        .unwrap()
        .install(&package)
        .unwrap();
    activate(&mut manager, &package, &[]);
    let instance = manager.connect(ID, &mut host).unwrap();
    let result = instance.run_task(&mut host, &input, || {
        panic!("pure transform has no content calls")
    });
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.output.unwrap().bytes, vec![2]);
    let (_other_dir, _other_manager, mut other_host, _other_package) = setup();
    let other = instance.run_task(&mut other_host, &input, || 1);
    assert_eq!(other.execution.outcome, Err(Fault::InactiveConnection));
    assert!(other.output.is_none());
    manager
        .set_enabled(ID, package.digest(), false, manager.revision())
        .unwrap();
    let stopped = instance.run_task(&mut host, &input, || 1);
    assert_eq!(stopped.execution.outcome, Err(Fault::InactiveConnection));
    assert!(stopped.output.is_none());
    assert_eq!(stopped.execution.host_calls, 0);
    instance.close(&mut host).unwrap();
}
#[test]
fn managed_guest_trap_after_commit_preserves_receipt_without_automatic_retry() {
    let (dir, mut manager, mut host, _) = setup();
    let request = RenameRequest {
        operation_id: "op".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "after trap".into(),
    }
    .encode()
    .unwrap();
    let data = request
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect::<String>();
    let module = wat::parse_str(format!(
        r#"(module
      (import "morrow_v1" "exchange" (func $call (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 2) (data (i32.const 0) "{data}")
      (func (export "morrow_run") (result i32)
        i32.const 0 i32.const {} i32.const 65536 i32.const 65536 call $call drop unreachable))"#,
        request.len()
    ))
    .unwrap();
    let package = Package::build(
        Package::manifest_for(ID, "1.0.0", &module, vec![Capability::RenameCard]),
        &module,
    )
    .unwrap();
    Catalog::open(&dir.path().join("packages"))
        .unwrap()
        .install(&package)
        .unwrap();
    activate(&mut manager, &package, &[GrantKind::Rename]);
    let mut instance = manager.connect(ID, &mut host).unwrap();
    host.grant(instance.parts_mut().1, GrantKind::Rename, "card", 100, 0)
        .unwrap();
    let report = instance.run(&mut host, || 1);
    assert_eq!(report.outcome, Err(Fault::Trap));
    assert_eq!(report.host_calls, 1);
    assert!(
        matches!(host.store_local().lookup_for_card("card","op").unwrap(), morrow_core::transaction::Lookup::Committed(r) if r.revision == 2)
    );
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
    manager
        .set_enabled(ID, package.digest(), false, manager.revision())
        .unwrap();
    assert_eq!(
        host.store_local()
            .card("card")
            .unwrap()
            .unwrap()
            .summary()
            .title,
        "after trap"
    );
    instance.close(&mut host).unwrap();
}
#[test]
fn close_retires_bounded_host_records_and_corrupt_package_cannot_reconnect() {
    let (dir, mut manager, mut host, package) = setup();
    activate(&mut manager, &package, &[]);
    for _ in 0..140 {
        let instance = manager.connect(ID, &mut host).unwrap();
        instance.close(&mut host).unwrap();
    }
    let path = Catalog::open(&dir.path().join("packages"))
        .unwrap()
        .install(&package)
        .unwrap();
    fs::write(path, b"corrupt").unwrap();
    assert!(manager.connect(ID, &mut host).is_err());
}
