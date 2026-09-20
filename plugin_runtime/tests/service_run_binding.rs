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
    io_binding::{Error, IoBinding, Usage},
    manager::{ManagedInstance, Manager},
};
use std::{collections::BTreeSet, time::Duration};

const ID: &str = "org.example.service.run.binding";
const RUN_MS: u64 = 60_000;

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::HttpListen,
        IoCapability::HttpPublish,
        IoCapability::HttpRequest,
    ])
}

fn package(service_run: bool) -> Package {
    let wasm = wat::parse_str(
        r#"(module (memory (export "memory") 1) (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["api.invoke".into()]);
    declaration.service_schema_sha256 = morrow_core::service::schema_digest().to_vec();
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_resources = 2;
    budget.max_jobs = 2;
    budget.max_bytes = 10;
    budget.max_job_bytes = 6;
    if service_run {
        declaration.service_run = Some(io::proto::ServiceRunProfile {
            schema_version: io::SERVICE_RUN_VERSION,
            max_duration_ms: RUN_MS,
        });
        manifest
            .required_features
            .push(io::SERVICE_RUN_FEATURE.into());
    }
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
    fn new(service_run: bool, approved: BTreeSet<IoCapability>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package(service_run);
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        if !approved.is_empty() {
            manager
                .approve_io(ID, package.digest(), approved, manager.revision())
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

    fn bind(&self, instance: &ManagedInstance, expires: u64, now: u64) -> IoBinding {
        self.manager
            .bind_service_run(
                &self.host,
                instance,
                self.package.digest(),
                self.manager.revision(),
                &caps(),
                expires,
                now,
            )
            .unwrap()
    }

    fn cannot_rebind(&self, instance: &ManagedInstance, expires: u64, now: u64) {
        assert!(
            self.manager
                .bind_service_run(
                    &self.host,
                    instance,
                    self.package.digest(),
                    self.manager.revision(),
                    &caps(),
                    expires,
                    now,
                )
                .is_err()
        );
    }
}

#[test]
fn service_profile_requires_explicit_entry_and_legacy_remains_thirty_seconds() {
    for service_run in [false, true] {
        let mut f = Fixture::new(service_run, caps());
        let instance = f.connect();
        if service_run {
            assert!(
                f.manager
                    .bind_io(
                        &f.host,
                        &instance,
                        f.package.digest(),
                        f.manager.revision(),
                        &caps(),
                        100,
                        1,
                    )
                    .is_err()
            );
            let binding = f.bind(&instance, RUN_MS + 1, 1);
            // Logical-clock coverage; the real >30-second worker test lives separately.
            binding
                .check(&f.manager, &f.host, &instance, 30_002)
                .unwrap();
        } else {
            f.cannot_rebind(&instance, RUN_MS + 1, 1);
            assert!(
                f.manager
                    .bind_io(
                        &f.host,
                        &instance,
                        f.package.digest(),
                        f.manager.revision(),
                        &caps(),
                        30_002,
                        1,
                    )
                    .is_err()
            );
            let binding = f
                .manager
                .bind_io(
                    &f.host,
                    &instance,
                    f.package.digest(),
                    f.manager.revision(),
                    &caps(),
                    30_001,
                    1,
                )
                .unwrap();
            binding
                .check(&f.manager, &f.host, &instance, 30_000)
                .unwrap();
            assert_eq!(
                binding.check(&f.manager, &f.host, &instance, 30_001),
                Err(Error::Expired)
            );
        }
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn rejected_issuance_does_not_consume_first_valid_run() {
    let approved = BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish]);
    let mut f = Fixture::new(true, approved.clone());
    let instance = f.connect();
    let other_host = HostRuntime::new(
        Store::open(&f.dir.path().join("other-db"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    let revision = f.manager.revision();
    let digest = f.package.digest();
    // Wrong identity, stale registry, undeclared/unapproved capability, missing service
    // capability and invalid lifetime must all leave issuance available.
    let undeclared = BTreeSet::from([
        IoCapability::HttpListen,
        IoCapability::HttpPublish,
        IoCapability::FileDelete,
    ]);
    let incomplete = BTreeSet::from([IoCapability::HttpListen]);
    let requested = caps();
    for (host, expected, rev, capabilities, expires, now) in [
        (&other_host, digest, revision, &approved, RUN_MS + 1, 1),
        (&f.host, [9; 32], revision, &approved, RUN_MS + 1, 1),
        (&f.host, digest, revision - 1, &approved, RUN_MS + 1, 1),
        (&f.host, digest, revision, &undeclared, RUN_MS + 1, 1),
        (&f.host, digest, revision, &requested, RUN_MS + 1, 1),
        (&f.host, digest, revision, &incomplete, RUN_MS + 1, 1),
        (&f.host, digest, revision, &approved, RUN_MS + 2, 1),
        (&f.host, digest, revision, &approved, 1, 1),
    ] {
        assert!(
            f.manager
                .bind_service_run(host, &instance, expected, rev, capabilities, expires, now,)
                .is_err()
        );
    }
    let binding = f
        .manager
        .bind_service_run(
            &f.host,
            &instance,
            digest,
            revision,
            &approved,
            RUN_MS + 1,
            1,
        )
        .unwrap();
    binding.check(&f.manager, &f.host, &instance, 2).unwrap();
    assert_eq!(binding.usage(), Usage::default());
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            0,
            3,
        ),
        Err(Error::Denied)
    ));
    instance.close(&mut f.host).unwrap();
}

#[test]
fn declaration_is_not_approval_and_revocation_invalidates_original_run() {
    let mut f = Fixture::new(true, BTreeSet::new());
    let unapproved = f.connect();
    f.cannot_rebind(&unapproved, RUN_MS + 1, 1);
    f.manager
        .approve_io(ID, f.package.digest(), caps(), f.manager.revision())
        .unwrap();
    // Approval changes invalidate old instances, so they cannot inherit the new grant.
    f.cannot_rebind(&unapproved, RUN_MS + 1, 1);
    unapproved.close(&mut f.host).unwrap();
    let current = f.connect();
    let binding = f.bind(&current, RUN_MS + 1, 1);
    let lease = binding
        .admit(
            &f.manager,
            &f.host,
            &current,
            IoCapability::HttpRequest,
            1,
            4,
            2,
        )
        .unwrap();
    f.manager
        .approve_io(
            ID,
            f.package.digest(),
            BTreeSet::new(),
            f.manager.revision(),
        )
        .unwrap();
    assert_eq!(
        binding.check(&f.manager, &f.host, &current, 3),
        Err(Error::Denied)
    );
    assert_eq!(
        lease.check(&f.manager, &f.host, &current, 3),
        Err(Error::Denied)
    );
    f.cannot_rebind(&current, RUN_MS + 3, 3);
    drop(lease);
    assert_eq!(
        binding.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 4
        }
    );
    current.close(&mut f.host).unwrap();
}

#[test]
fn one_shot_issuance_survives_expiry_and_dropping_every_binding() {
    for expire in [false, true] {
        let mut f = Fixture::new(true, caps());
        let instance = f.connect();
        let binding = f.bind(&instance, RUN_MS + 1, 1);
        f.cannot_rebind(&instance, RUN_MS + 2, 2);
        if expire {
            assert_eq!(
                binding.check(&f.manager, &f.host, &instance, RUN_MS + 1),
                Err(Error::Expired)
            );
        }
        drop(binding);
        f.cannot_rebind(&instance, RUN_MS + 3, 3);
        f.cannot_rebind(&instance, RUN_MS * 2 + 1, RUN_MS + 1);
        instance.close(&mut f.host).unwrap();
    }
}

#[test]
fn clock_regression_permanently_invalidates_the_service_run_and_held_lease() {
    let mut f = Fixture::new(true, caps());
    let instance = f.connect();
    let binding = f.bind(&instance, RUN_MS + 1, 1);
    let lease = binding
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            1,
            4,
            10,
        )
        .unwrap();
    assert_eq!(
        lease.check(&f.manager, &f.host, &instance, 9),
        Err(Error::Clock)
    );
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 11),
        Err(Error::Clock)
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            0,
            12,
        ),
        Err(Error::Clock)
    ));
    drop(lease);
    assert_eq!(
        binding.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 4
        }
    );
    f.cannot_rebind(&instance, RUN_MS + 12, 12);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn clock_rollback_after_expiry_cannot_revive_a_service_run() {
    let mut f = Fixture::new(true, caps());
    let instance = f.connect();
    let binding = f.bind(&instance, RUN_MS + 1, 1);
    binding.check(&f.manager, &f.host, &instance, 10).unwrap();
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, RUN_MS + 1),
        Err(Error::Expired)
    );
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 11),
        Err(Error::Expired)
    );
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 1),
        Err(Error::Expired)
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn frozen_host_clock_does_not_extend_real_service_deadline() {
    let mut f = Fixture::new(true, caps());
    let instance = f.connect();
    let binding = f.bind(&instance, 41, 1);
    std::thread::sleep(Duration::from_millis(65));
    assert_eq!(
        binding.check(&f.manager, &f.host, &instance, 1),
        Err(Error::Expired)
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            0,
            1,
        ),
        Err(Error::Expired)
    ));
    drop(binding);
    f.cannot_rebind(&instance, 101, 1);
    instance.close(&mut f.host).unwrap();
}

#[test]
fn sequential_requests_release_capacity_without_refunding_run_bytes() {
    let mut f = Fixture::new(true, caps());
    let instance = f.connect();
    let binding = f.bind(&instance, RUN_MS + 1, 1);
    let first = binding
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            2,
            6,
            2,
        )
        .unwrap();
    drop(first);
    assert_eq!(
        binding.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 6
        }
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            1,
            5,
            3,
        ),
        Err(Error::Limit)
    ));
    let second = binding
        .admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            2,
            4,
            3,
        )
        .unwrap();
    assert_eq!(
        binding.usage(),
        Usage {
            resources: 2,
            jobs: 1,
            bytes: 10
        }
    );
    drop(second);
    assert_eq!(
        binding.usage(),
        Usage {
            resources: 0,
            jobs: 0,
            bytes: 10
        }
    );
    assert!(matches!(
        binding.admit(
            &f.manager,
            &f.host,
            &instance,
            IoCapability::HttpRequest,
            0,
            1,
            4,
        ),
        Err(Error::Limit)
    ));
    drop(binding);
    f.cannot_rebind(&instance, RUN_MS + 4, 4);
    instance.close(&mut f.host).unwrap();
}
