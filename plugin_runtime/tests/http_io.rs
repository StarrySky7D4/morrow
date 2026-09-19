//! Live resource admission only; actual HTTP and Ready-delivery tests live in
//! the network adapter suite. These tests use the real registry and manager.
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
    http_io::HttpGrant,
    io_binding::{Error, IoBinding, Usage},
    manager::{ManagedInstance, Manager},
};
use std::collections::BTreeSet;
const ID: &str = "org.example.http.grant";
fn all_caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::FileRead,
        IoCapability::HttpRequest,
        IoCapability::CredentialUse,
    ])
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    instance: ManagedInstance,
    binding: IoBinding,
}
impl Fixture {
    fn new(approved: BTreeSet<IoCapability>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module (memory (export "memory") 1)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration =
            io::declaration(all_caps().into_iter().collect(), vec!["api.invoke".into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_resources = 1;
        budget.max_jobs = 1;
        budget.max_bytes = 10;
        budget.max_job_bytes = 6;
        budget.max_duration_ms = 50;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), approved.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &approved,
                40,
                1,
            )
            .unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            instance,
            binding,
        }
    }
    fn issue(&self, credential: bool, now: u64) -> Result<HttpGrant, Error> {
        HttpGrant::issue(
            &self.manager,
            &self.host,
            &self.instance,
            &self.binding,
            [11; 32],
            [0xab; 32],
            credential,
            now,
        )
    }
}
#[test]
fn resource_only_admission_works_with_full_job_capacity_and_clones_hold_until_last_drop() {
    let f = Fixture::new(all_caps());
    let job = f
        .binding
        .admit(
            &f.manager,
            &f.host,
            &f.instance,
            IoCapability::FileRead,
            0,
            3,
            1,
        )
        .unwrap();
    assert_eq!(
        f.binding.usage(),
        Usage {
            jobs: 1,
            resources: 0,
            bytes: 3
        }
    );
    let grant = f.issue(true, 2).unwrap();
    assert_eq!(grant.endpoint_reference(), "ab".repeat(32));
    assert_eq!(grant.policy_sha256(), [11; 32]);
    let occupied = Usage {
        jobs: 1,
        resources: 1,
        bytes: 3,
    };
    assert_eq!(f.binding.usage(), occupied);
    assert_eq!(f.issue(false, 3).err(), Some(Error::Limit));
    assert_eq!(f.binding.usage(), occupied);
    let retained = grant.clone();
    grant.revoke();
    assert_eq!(f.binding.usage(), occupied);
    drop(grant);
    assert_eq!(f.binding.usage(), occupied);
    // Revocation withdraws authority, but it does not pretend held resources
    // disappeared while another owner may still be using their guard.
    assert_eq!(f.issue(false, 3).err(), Some(Error::Limit));
    drop(retained);
    assert_eq!(
        f.binding.usage(),
        Usage {
            jobs: 1,
            resources: 0,
            bytes: 3
        }
    );
    let replacement = f.issue(false, 3).unwrap();
    drop(replacement);
    drop(job);
    assert_eq!(
        f.binding.usage(),
        Usage {
            jobs: 0,
            resources: 0,
            bytes: 3
        }
    );
}
#[test]
fn declared_http_or_credentials_do_not_substitute_for_actual_io_approval() {
    for (approved, credential, allowed) in [
        (BTreeSet::from([IoCapability::FileRead]), false, false),
        (BTreeSet::from([IoCapability::HttpRequest]), true, false),
        (BTreeSet::from([IoCapability::HttpRequest]), false, true),
        (
            BTreeSet::from([IoCapability::HttpRequest, IoCapability::CredentialUse]),
            true,
            true,
        ),
    ] {
        let f = Fixture::new(approved);
        let result = f.issue(credential, 10);
        if allowed {
            let grant = result.unwrap();
            assert_eq!(
                f.binding.usage(),
                Usage {
                    jobs: 0,
                    resources: 1,
                    bytes: 0
                }
            );
            drop(grant);
        } else {
            assert_eq!(result.err(), Some(Error::Denied));
            // Failed capability validation did not advance the valid owner's clock.
            f.binding
                .check(&f.manager, &f.host, &f.instance, 2)
                .unwrap();
        }
        assert_eq!(f.binding.usage(), Usage::default());
    }
}
#[test]
fn wrong_host_manager_same_package_instance_and_expiry_never_reserve_resources() {
    let mut f = Fixture::new(all_caps());
    let other = Fixture::new(all_caps());
    let second = f.manager.connect(ID, &mut f.host).unwrap();
    for (manager, host, instance) in [
        (&f.manager, &other.host, &f.instance),
        (&other.manager, &f.host, &f.instance),
        (&f.manager, &f.host, &second),
    ] {
        let denied = HttpGrant::issue(
            manager, host, instance, &f.binding, [11; 32], [0xab; 32], false, 39,
        );
        assert_eq!(denied.err(), Some(Error::Denied));
        assert_eq!(f.binding.usage(), Usage::default());
        assert_eq!(other.binding.usage(), Usage::default());
        f.binding
            .check(&f.manager, &f.host, &f.instance, 2)
            .unwrap();
    }
    assert_eq!(f.issue(false, 40).err(), Some(Error::Expired));
    assert_eq!(f.binding.usage(), Usage::default());
    let grant = f.issue(false, 2).unwrap();
    assert_eq!(
        f.binding.usage(),
        Usage {
            jobs: 0,
            resources: 1,
            bytes: 0
        }
    );
    drop(grant);
    assert_eq!(f.binding.usage(), Usage::default());
    second.close(&mut f.host).unwrap();
}
