#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]

use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        MAX_MUTATION_BYTES, MAX_MUTATION_JOB_BYTES, MUTATION_BUDGET_FEATURE, MUTATION_FEATURE,
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        proto,
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::{IoBinding, MutationBudget},
    manager::{ManagedInstance, Manager},
};
use std::collections::BTreeSet;

const ID: &str = "org.example.mutation-budget";
const MIB: u64 = 1024 * 1024;

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileCreate, IoCapability::FileDelete])
}

struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    host: HostRuntime,
    digest: [u8; 32],
}
impl Fixture {
    fn new(extended: bool, approve: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
              (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
              (import "morrow_mutation_v1" "call" (func $call (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 6)
              (func (export "morrow_run") (result i32) (i32.const 0)))"#,
        )
        .unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features = vec![io::FEATURE.into(), MUTATION_FEATURE.into()];
        manifest.mutation_schema_sha256 = morrow_core::mutation::schema_digest().to_vec();
        manifest.io_declaration = Some(io::declaration(
            caps().into_iter().collect(),
            vec!["mutation.run".into()],
        ));
        if extended {
            manifest
                .required_features
                .push(MUTATION_BUDGET_FEATURE.into());
            manifest.mutation_budget = Some(proto::MutationBudget {
                max_job_bytes: MAX_MUTATION_JOB_BYTES,
                max_bytes: MAX_MUTATION_BYTES,
            });
        }
        let package = Package::build(manifest, &wasm).unwrap();
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
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        Self {
            _dir: dir,
            manager,
            host,
            digest: package.digest(),
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.manager.connect(ID, &mut self.host).unwrap()
    }
    fn bind(
        &self,
        instance: &ManagedInstance,
        approved: MutationBudget,
    ) -> morrow_plugin_runtime::manager::Result<IoBinding> {
        self.manager.bind_budgeted_mutation(
            &self.host,
            instance,
            self.digest,
            self.manager.revision(),
            &caps(),
            30_000,
            1,
            approved,
        )
    }
}

fn approved() -> MutationBudget {
    MutationBudget {
        max_job_bytes: 24 * MIB,
        max_bytes: 100 * MIB,
    }
}

#[test]
fn mutation_extension_requires_explicit_host_budget_and_preserves_legacy_bind() {
    let mut f = Fixture::new(true, true);
    let instance = f.connect();
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.digest,
                f.manager.revision(),
                &caps(),
                30_000,
                1
            )
            .is_err()
    );
    assert!(
        f.bind(
            &instance,
            MutationBudget {
                max_job_bytes: 33 * MIB,
                max_bytes: 100 * MIB
            }
        )
        .is_err()
    );
    assert!(
        f.bind(
            &instance,
            MutationBudget {
                max_job_bytes: 24 * MIB,
                max_bytes: 257 * MIB
            }
        )
        .is_err()
    );
    assert!(
        f.bind(
            &instance,
            MutationBudget {
                max_job_bytes: 24 * MIB,
                max_bytes: 20 * MIB
            }
        )
        .is_err()
    );
    let binding = f.bind(&instance, approved()).unwrap();
    assert_eq!(binding.mutation_budget(), Some(approved()));
    drop(binding);
    assert!(f.bind(&instance, approved()).is_err());
    assert!(
        f.manager
            .bind_io(
                &f.host,
                &instance,
                f.digest,
                f.manager.revision(),
                &caps(),
                30_000,
                2
            )
            .is_err()
    );
    instance.close(&mut f.host).unwrap();

    let mut legacy = Fixture::new(false, true);
    let old = legacy.connect();
    assert!(legacy.bind(&old, approved()).is_err());
    let binding = legacy
        .manager
        .bind_io(
            &legacy.host,
            &old,
            legacy.digest,
            legacy.manager.revision(),
            &caps(),
            30_000,
            1,
        )
        .unwrap();
    assert_eq!(binding.mutation_budget(), None);
    assert!(
        binding
            .admit(
                &legacy.manager,
                &legacy.host,
                &old,
                IoCapability::FileCreate,
                0,
                17 * MIB,
                2
            )
            .is_err()
    );
    old.close(&mut legacy.host).unwrap();
}

#[test]
fn lower_host_approval_limits_single_target_admission_and_cumulative_bytes() {
    let mut f = Fixture::new(true, true);
    let instance = f.connect();
    let lower = MutationBudget {
        max_job_bytes: 8 * MIB,
        max_bytes: 12 * MIB,
    };
    let binding = f.bind(&instance, lower).unwrap();
    assert_eq!(binding.mutation_budget(), Some(lower));
    assert!(
        binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                9 * MIB,
                2
            )
            .is_err()
    );
    for tick in [3, 4] {
        let lease = binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                6 * MIB,
                tick,
            )
            .unwrap();
        drop(lease);
    }
    assert_eq!(binding.usage().bytes, 12 * MIB);
    assert!(
        binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                1,
                5
            )
            .is_err()
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn approved_mutation_budget_is_shared_nonrefundable_and_narrower_than_declaration() {
    let mut f = Fixture::new(true, true);
    let instance = f.connect();
    let binding = f.bind(&instance, approved()).unwrap();
    for tick in 2..=6 {
        let job = binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                20 * MIB,
                tick,
            )
            .unwrap();
        drop(job);
    }
    assert_eq!(binding.usage().bytes, 100 * MIB);
    assert!(
        binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                1,
                7
            )
            .is_err()
    );
    assert!(
        binding
            .admit(
                &f.manager,
                &f.host,
                &instance,
                IoCapability::FileCreate,
                0,
                25 * MIB,
                7
            )
            .is_err()
    );
    instance.close(&mut f.host).unwrap();
}

#[test]
fn foreign_identity_unapproved_capability_and_revocation_cannot_rebind() {
    let mut unapproved = Fixture::new(true, false);
    let no_approval = unapproved.connect();
    assert!(unapproved.bind(&no_approval, approved()).is_err());
    no_approval.close(&mut unapproved.host).unwrap();

    let mut f = Fixture::new(true, true);
    let instance = f.connect();
    let other = f.connect();
    let mut foreign = Fixture::new(false, true);
    let foreign_instance = foreign.connect();
    assert!(
        f.manager
            .bind_budgeted_mutation(
                &foreign.host,
                &foreign_instance,
                foreign.digest,
                f.manager.revision(),
                &caps(),
                30_000,
                1,
                approved()
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_budgeted_mutation(
                &f.host,
                &instance,
                [9; 32],
                f.manager.revision(),
                &caps(),
                30_000,
                1,
                approved()
            )
            .is_err()
    );
    assert!(
        f.manager
            .bind_budgeted_mutation(
                &f.host,
                &instance,
                f.digest,
                f.manager.revision() + 1,
                &caps(),
                30_000,
                1,
                approved()
            )
            .is_err()
    );
    let wrong_cap = BTreeSet::from([IoCapability::HttpRequest]);
    assert!(
        f.manager
            .bind_budgeted_mutation(
                &f.host,
                &instance,
                f.digest,
                f.manager.revision(),
                &wrong_cap,
                30_000,
                1,
                approved()
            )
            .is_err()
    );
    let binding = f.bind(&instance, approved()).unwrap();
    assert!(binding.check(&f.manager, &f.host, &other, 2).is_err());
    f.manager
        .set_enabled(ID, f.digest, false, f.manager.revision())
        .unwrap();
    assert!(binding.check(&f.manager, &f.host, &instance, 3).is_err());
    assert!(f.bind(&instance, approved()).is_err());
    instance.close(&mut f.host).unwrap();
    other.close(&mut f.host).unwrap();
    foreign_instance.close(&mut foreign.host).unwrap();
}
