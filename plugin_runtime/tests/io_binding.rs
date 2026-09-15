#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    instance_pool::{Limits as PoolLimits, Pool},
    io_binding::{Error, IoBinding, Usage},
    manager::{ManagedInstance, Manager},
};
use std::collections::BTreeSet;
const ID: &str = "org.example.io.binding";
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileRead, IoCapability::HttpRequest])
}
fn package() -> Package {
    let wasm = wat::parse_str(r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#).unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["api.invoke".into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_resources = 2;
    budget.max_jobs = 2;
    budget.max_bytes = 10;
    budget.max_job_bytes = 6;
    budget.max_duration_ms = 50;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    Package::build(manifest, &wasm).unwrap()
}
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    package: Package,
}
impl Fixture {
    fn new(approve: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        if approve {
            manager
                .approve_io(ID, package.digest(), caps(), manager.revision())
                .unwrap();
        }
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        Self {
            dir,
            manager,
            host,
            package,
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn bind(&self, instance: &ManagedInstance) -> IoBinding {
        self.manager
            .bind_io(
                &self.host,
                instance,
                self.package.digest(),
                self.manager.revision(),
                &caps(),
                40,
                1,
            )
            .unwrap()
    }
}

#[test]
fn declaration_and_live_connection_do_not_imply_io_approval() {
    let mut f = Fixture::new(false);
    let old = f.connect();
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &old,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                40,
                1
            )
            .is_err()
    );
    // An IO-declaring package has no escape through the ordinary pure execution entry.
    assert!(old.run(&mut f.host, || 1).outcome.is_err());
    f.manager
        .approve_io(ID, f.package.digest(), caps(), f.manager.revision())
        .unwrap();
    assert!(f.host.revocation(old.connection()).unwrap().is_revoked());
    let current = f.connect();
    f.bind(&current)
        .check(&f.manager, &f.host, &current, 2)
        .unwrap();
    old.close(&mut f.host).unwrap();
    current.close(&mut f.host).unwrap();
}

#[test]
fn invalid_or_stale_approval_never_stops_a_healthy_instance() {
    let mut f = Fixture::new(true);
    let instance = f.connect();
    let binding = f.bind(&instance);
    let rev = f.manager.revision();
    let mut invalid = caps();
    invalid.insert(IoCapability::FileDelete);
    assert!(
        f.manager
            .approve_io(ID, f.package.digest(), invalid, rev)
            .is_err()
    );
    assert!(f.manager.approve_io(ID, [9; 32], caps(), rev).is_err());
    assert!(
        f.manager
            .approve_io(ID, f.package.digest(), BTreeSet::new(), rev - 1)
            .is_err()
    );
    assert_eq!(f.manager.revision(), rev);
    binding.check(&f.manager, &f.host, &instance, 2).unwrap();
    assert!(
        !f.host
            .revocation(instance.connection())
            .unwrap()
            .is_revoked()
    );
    f.manager
        .approve_io(ID, f.package.digest(), caps(), rev)
        .unwrap();
    binding.check(&f.manager, &f.host, &instance, 3).unwrap();
    instance.close(&mut f.host).unwrap();
}

#[test]
fn repeated_bindings_share_resources_jobs_and_nonrefundable_byte_budget() {
    let mut f = Fixture::new(true);
    let instance = f.connect();
    let a = f.bind(&instance);
    let b = f.bind(&instance);
    let first = a
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            2,
            6,
            2,
        )
        .unwrap();
    assert_eq!(
        b.usage(),
        Usage {
            resources: 2,
            jobs: 1,
            bytes: 6
        }
    );
    assert!(matches!(
        b.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            1,
            0,
            3
        ),
        Err(Error::Limit)
    ));
    let second = b
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            0,
            3,
        )
        .unwrap();
    assert!(matches!(
        a.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            0,
            0,
            4
        ),
        Err(Error::Limit)
    ));
    drop(first);
    drop(second);
    assert_eq!(
        a.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 6
        }
    );
    drop(b);
    let c = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &caps(),
            40,
            4,
        )
        .unwrap();
    assert!(matches!(
        c.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            1,
            5,
            5
        ),
        Err(Error::Limit)
    ));
    let last = c
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            1,
            4,
            5,
        )
        .unwrap();
    drop(last);
    assert_eq!(c.usage().bytes, 10);
    assert!(matches!(
        a.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            0,
            1,
            6
        ),
        Err(Error::Limit)
    ));
    instance.close(&mut f.host).unwrap();
}

#[test]
fn invalid_budget_deadline_and_clock_leave_shared_state_unchanged() {
    let mut f = Fixture::new(true);
    let instance = f.connect();
    let b = f.bind(&instance);
    assert!(matches!(
        b.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            0,
            7,
            30
        ),
        Err(Error::Limit)
    ));
    assert!(matches!(
        b.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            u32::MAX,
            u64::MAX,
            30
        ),
        Err(Error::Limit)
    ));
    assert!(matches!(
        b.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileDelete,
            0,
            0,
            30
        ),
        Err(Error::Denied)
    ));
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                100,
                2
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                2,
                2
            )
            .is_err()
    );
    assert!(matches!(
        b.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            0,
            0,
            40
        ),
        Err(Error::Expired)
    ));
    let lease = b
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            1,
            1,
            2,
        )
        .unwrap();
    assert!(matches!(
        b.check(&f.manager, &f.host, &instance, 1),
        Err(Error::Clock)
    ));
    lease.check(&f.manager, &f.host, &instance, 3).unwrap();
    drop(lease);
    assert_eq!(
        b.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 1
        }
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn foreign_manager_host_connection_and_binding_attempts_cannot_poison_owner() {
    let mut f = Fixture::new(true);
    let mut other = Fixture::new(true);
    let mut instance = f.connect();
    let foreign = other.connect();
    let b = f.bind(&instance);
    for (manager, host, target) in [
        (&other.manager, &f.host, &instance),
        (&f.manager, &other.host, &instance),
        (&f.manager, &f.host, &foreign),
    ] {
        assert!(
            b.admit(
                manager,
                host,
                target,
                IoCapability::FileRead,
                2,
                6,
                1_000_000
            )
            .is_err()
        );
    }
    assert!(
        other
            .manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                other.manager.revision(),
                &caps(),
                1_000_010,
                1_000_000
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &other.host,
                &instance,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                1_000_010,
                1_000_000
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                [0; 32],
                f.manager.revision(),
                &caps(),
                40,
                30
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                f.manager.revision() - 1,
                &caps(),
                40,
                30
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                f.manager.revision(),
                &BTreeSet::new(),
                40,
                30
            )
            .is_err()
    );
    // Swapping a real connection from the same host/package is still not this Control.
    let mut replacement = f.connect();
    std::mem::swap(instance.parts_mut().1, replacement.parts_mut().1);
    assert!(b.check(&f.manager, &f.host, &instance, 1_000_000).is_err());
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.package.digest(),
                f.manager.revision(),
                &caps(),
                1_000_010,
                1_000_000
            )
            .is_err()
    );
    std::mem::swap(instance.parts_mut().1, replacement.parts_mut().1);
    let lease = b
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            1,
            1,
            2,
        )
        .unwrap();
    assert_eq!(b.usage().bytes, 1);
    drop(lease);
    instance.close(&mut f.host).unwrap();
    replacement.close(&mut f.host).unwrap();
    foreign.close(&mut other.host).unwrap();
}

#[test]
fn narrowing_approval_and_stop_revoke_held_jobs_and_old_bindings() {
    for stop in [true, false] {
        let mut f = Fixture::new(true);
        let instance = f.connect();
        let b = f.bind(&instance);
        let lease = b
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileRead,
                1,
                1,
                2,
            )
            .unwrap();
        if stop {
            instance.stop();
        } else {
            f.manager
                .approve_io(
                    ID,
                    f.package.digest(),
                    BTreeSet::from([IoCapability::FileRead]),
                    f.manager.revision(),
                )
                .unwrap();
        }
        assert!(lease.check(&f.manager, &f.host, &instance, 3).is_err());
        assert!(
            b.admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileRead,
                0,
                0,
                3
            )
            .is_err()
        );
        drop(lease);
        assert_eq!(
            b.usage(),
            Usage {
                resources: 0,
                jobs: 0,
                bytes: 1
            }
        );
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn actual_manager_drop_revokes_control_even_when_binding_and_instance_survive() {
    let mut f = Fixture::new(true);
    let instance = f.connect();
    let b = f.bind(&instance);
    let catalog = Catalog::open(&f.dir.path().join("other-catalog")).unwrap();
    let substitute = Manager::new(
        Registry::open(&f.dir.path().join("other-registry"), catalog).unwrap(),
        Limits::default(),
    );
    let original = std::mem::replace(&mut f.manager, substitute);
    drop(original);
    assert!(
        f.host
            .revocation(instance.connection())
            .unwrap()
            .is_revoked()
    );
    assert!(b.check(&f.manager, &f.host, &instance, 2).is_err());
    instance.close(&mut f.host).unwrap();
}

#[test]
fn pool_restart_requires_fresh_binding_and_old_session_drop_revokes_immediately() {
    let mut f = Fixture::new(true);
    let mut pool = Pool::new(&f.host, PoolLimits::default()).unwrap();
    let rev = f.manager.revision();
    let old = pool
        .start(&mut f.manager, &mut f.host, ID, &[], rev)
        .unwrap();
    let binding = pool
        .bind_root_io(
            &f.manager,
            &f.host,
            &old,
            f.package.digest(),
            rev,
            &caps(),
            40,
            1,
        )
        .unwrap();
    let lease = binding
        .admit(
            &f.manager,
            &f.host,
            pool.root(&old).unwrap(),
            IoCapability::FileRead,
            1,
            6,
            2,
        )
        .unwrap();
    let old_root = pool.root(&old).unwrap();
    old_root.stop();
    assert!(lease.check(&f.manager, &f.host, old_root, 3).is_err());
    let fresh = pool
        .restart(&mut f.manager, &mut f.host, &old, rev, 3)
        .unwrap();
    assert!(
        binding
            .check(&f.manager, &f.host, pool.root(&fresh).unwrap(), 4)
            .is_err()
    );
    assert!(
        pool.bind_root_io(
            &f.manager,
            &f.host,
            &old,
            f.package.digest(),
            rev,
            &caps(),
            40,
            4
        )
        .is_err()
    );
    let current = pool
        .bind_root_io(
            &f.manager,
            &f.host,
            &fresh,
            f.package.digest(),
            rev,
            &caps(),
            40,
            4,
        )
        .unwrap();
    assert_eq!(current.usage(), Usage::default());
    let job = current
        .admit(
            &f.manager,
            &f.host,
            pool.root(&fresh).unwrap(),
            IoCapability::FileRead,
            1,
            6,
            5,
        )
        .unwrap();
    drop(lease);
    drop(old);
    drop(job);
    let fresh_root = pool.root(&fresh).unwrap();
    drop(fresh);
    assert!(current.check(&f.manager, &f.host, fresh_root, 6).is_err());
    pool.maintain(&f.manager, &mut f.host).unwrap();
    pool.close_all(&mut f.host).unwrap();
    assert_eq!(pool.usage().sessions, 0);
}

#[test]
fn capability_subsets_share_budget_but_never_widen_a_binding() {
    let mut f = Fixture::new(true);
    let instance = f.connect();
    let files = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &BTreeSet::from([IoCapability::FileRead]),
            40,
            1,
        )
        .unwrap();
    let http = f
        .manager
        .bind_io(
            &f.host,
            &instance,
            f.package.digest(),
            f.manager.revision(),
            &BTreeSet::from([IoCapability::HttpRequest]),
            40,
            1,
        )
        .unwrap();
    assert!(matches!(
        files.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            0,
            30
        ),
        Err(Error::Denied)
    ));
    let job = files
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::FileRead,
            1,
            6,
            2,
        )
        .unwrap();
    drop(job);
    assert!(matches!(
        http.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            5,
            3
        ),
        Err(Error::Limit)
    ));
    let job = http
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            1,
            4,
            3,
        )
        .unwrap();
    assert_eq!(job.capability(), IoCapability::HttpRequest);
    assert_eq!(job.bytes(), 4);
    drop(job);
    instance.close(&mut f.host).unwrap();
}
