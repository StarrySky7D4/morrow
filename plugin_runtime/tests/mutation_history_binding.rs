//! A budget-declaring guest may be re-opened for original history only without
//! granting its published extended execution budget or a new selected target.
#![cfg(all(windows, feature = "packages", not(target_arch = "wasm32")))]

use morrow_core::{
    dispatch::HostRuntime,
    file_content::FileContent,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_evidence::{Kind, Material},
    io_intent::{Phase, Record},
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
    file_target::{Error as TargetError, SelectionScope, TargetBroker},
    io_binding::{IoBinding, MutationBudget},
    io_jobs::{
        CommandOwner, HostOwner, IoWorker, JobError, JobLimits, MAX_JOB_BYTES,
        MAX_MUTATION_HISTORY_METADATA_BYTES, MAX_TOTAL_BYTES, ManagedHostOwner, MutationResponse,
        OwnerCommandError, OwnerCommandPoll, Router, RouterFault,
    },
    manager::{ManagedInstance, Manager},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.mutation-history";
const SUBJECT: &str = "mutation.history-test";

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileCreate, IoCapability::FileDelete])
}
fn one(capability: IoCapability) -> BTreeSet<IoCapability> {
    BTreeSet::from([capability])
}
fn scope(disposition: Disposition) -> SelectionScope {
    SelectionScope {
        subject: SUBJECT.into(),
        approval_sha256: [7; 32],
        disposition,
    }
}

struct Owner {
    host: HostRuntime,
    manager: Manager,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
    fn with_managed_runtime<T>(
        &mut self,
        action: impl FnOnce(&Manager, &mut HostRuntime) -> T,
    ) -> Option<T> {
        Some(action(&self.manager, &mut self.host))
    }
}
impl CommandOwner for Owner {
    fn command(&mut self, _input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        panic!("history binding dispatched a raw owner command")
    }
}
struct PanicRouter;
impl Router for PanicRouter {
    fn route(&mut self, _call: u32, _request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        panic!("history binding dispatched a guest job")
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    owner: Owner,
    digest: [u8; 32],
}
impl Fixture {
    fn new() -> Self {
        Self::new_with(true, true)
    }
    fn new_with(extended: bool, approved: bool) -> Self {
        Self::new_with_job(extended, approved, MAX_JOB_BYTES)
    }
    fn new_with_job(extended: bool, approved: bool, declared_job_bytes: u64) -> Self {
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
        if extended {
            manifest
                .required_features
                .push(MUTATION_BUDGET_FEATURE.into());
            manifest.mutation_budget = Some(proto::MutationBudget {
                max_job_bytes: MAX_MUTATION_JOB_BYTES,
                max_bytes: MAX_MUTATION_BYTES,
            });
        }
        let mut declaration = io::declaration(
            caps().into_iter().collect(),
            vec!["mutation.history".into()],
        );
        declaration.budget.as_mut().unwrap().max_job_bytes = declared_job_bytes;
        manifest.io_declaration = Some(declaration);
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        if approved {
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
            dir,
            owner: Owner { host, manager },
            digest: package.digest(),
        }
    }
    fn connect(&mut self) -> ManagedInstance {
        self.owner
            .manager
            .connect(ID, &mut self.owner.host)
            .unwrap()
    }
    fn history(&self, instance: &ManagedInstance) -> IoBinding {
        self.owner
            .manager
            .bind_mutation_history(
                &self.owner.host,
                instance,
                self.digest,
                self.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                1,
            )
            .unwrap()
    }
}

#[test]
fn history_binding_is_ordinary_and_cannot_select_even_via_direct_broker() {
    let mut f = Fixture::new();
    let instance = f.connect();
    assert!(
        f.owner
            .manager
            .bind_io(
                &f.owner.host,
                &instance,
                f.digest,
                f.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                1,
            )
            .is_err()
    );
    assert!(
        f.owner
            .manager
            .bind_mutation_history(
                &f.owner.host,
                &instance,
                f.digest,
                f.owner.manager.revision(),
                &caps(),
                30_000,
                1,
            )
            .is_err()
    );
    let binding = f.history(&instance);
    assert!(binding.is_mutation_history());
    assert_eq!(binding.mutation_budget(), None);
    // The public admission path used by io_execution::Broker::begin must
    // deny even a tiny request within the ordinary quota.
    for resources in [0, 1] {
        assert!(
            binding
                .admit(
                    &f.owner.manager,
                    &f.owner.host,
                    &instance,
                    IoCapability::FileCreate,
                    resources,
                    1,
                    2,
                )
                .is_err()
        );
    }
    // A direct trusted broker call must fail before opening either a directory
    // or an existing file, despite the binding's approved file capability.
    let mut broker = TargetBroker::new([8; 32]).unwrap();
    let root = f.dir.path();
    let result = broker.select_create(
        &f.owner.manager,
        &f.owner.host,
        &instance,
        &binding,
        root,
        &RelativeFilePath::parse("never.bin").unwrap(),
        scope(Disposition::Create),
        || 3,
    );
    assert!(matches!(result, Err(TargetError::Admission(_))));
    let existing = root.join("existing.bin");
    std::fs::write(&existing, b"preserved").unwrap();
    let result = broker.select_existing(
        &f.owner.manager,
        &f.owner.host,
        &instance,
        &binding,
        &existing,
        scope(Disposition::Delete),
        || 4,
    );
    assert!(matches!(result, Err(TargetError::Admission(_))));
    assert_eq!(broker.len(), 0);
    assert_eq!(std::fs::read(existing).unwrap(), b"preserved");
    assert!(!root.join("never.bin").exists());
    assert!(
        f.owner
            .manager
            .bind_budgeted_mutation(
                &f.owner.host,
                &instance,
                f.digest,
                f.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                5,
                MutationBudget {
                    max_job_bytes: MAX_MUTATION_JOB_BYTES,
                    max_bytes: MAX_MUTATION_BYTES
                },
            )
            .is_err()
    );
    instance.close(&mut f.owner.host).unwrap();
}

fn read(handle: morrow_plugin_runtime::io_jobs::MutationHandle) -> MutationResponse {
    read_result(handle).unwrap()
}

fn read_result(
    mut handle: morrow_plugin_runtime::io_jobs::MutationHandle,
) -> Result<MutationResponse, TargetError> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < deadline, "history reply timed out");
        thread::sleep(Duration::from_millis(1));
    }
    handle.read().unwrap().unwrap()
}

fn finish(mut worker: IoWorker<Owner>) {
    worker.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while worker.try_reclaim().unwrap().is_none() {
        assert!(Instant::now() < deadline, "history worker did not exit");
        thread::sleep(Duration::from_millis(1));
    }
}

fn original_request(digest: [u8; 32], operation: &str, bytes: &[u8]) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256: digest,
        approval_sha256: [7; 32],
        target: Target {
            reference: [3; 32],
            relative_path: Some(RelativeFilePath::parse("leaf.bin").unwrap()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: bytes.len() as u64,
        content_sha256: Some(Sha256::digest(bytes).into()),
    })
    .unwrap()
}

fn retain_prepared_original(f: &mut Fixture, plan: &RequestRecord, bytes: &[u8]) {
    let record = Record::prepared(plan.command().unwrap()).unwrap();
    let command = record.command();
    let material = Material::encode(
        Kind::Request,
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        plan.container(),
    )
    .unwrap();
    let content = FileContent::new(
        &command.operation_id,
        &command.subject,
        command.request_sha256,
        bytes,
    )
    .unwrap();
    f.owner
        .host
        .store_local_mut()
        .prepare_file_mutation_local_authorized(&record, &material, || Ok(()))
        .unwrap();
    f.owner
        .host
        .store_local_mut()
        .stage_file_mutation_content_local_authorized(&content, || Ok(()))
        .unwrap();
}

#[test]
fn history_worker_only_accepts_discovery_and_reconciliation_commands() {
    let mut f = Fixture::new();
    let instance = f.connect();
    let binding = f.history(&instance);
    let worker = IoWorker::spawn_managed_owner(
        f.owner,
        instance,
        binding,
        || 2,
        1,
        JobLimits::new(1, 4 * 1024 * 1024, 16 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let (cursor, first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Create, 1)
        .unwrap();
    assert!(matches!(read(first), MutationResponse::Plans { .. }));
    assert!(matches!(
        read(worker.close_mutation_discovery(cursor).unwrap()),
        MutationResponse::Released
    ));
    let plan = RequestRecord::new(MutationRequest {
        operation_id: "original-create".into(),
        subject: SUBJECT.into(),
        package_sha256: f.digest,
        approval_sha256: [7; 32],
        target: Target {
            reference: [3; 32],
            relative_path: Some(RelativeFilePath::parse("leaf.bin").unwrap()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: 0,
        content_sha256: Some(Sha256::digest([]).into()),
    })
    .unwrap();
    assert!(matches!(
        read(worker.reconcile_mutation(plan).unwrap()),
        MutationResponse::Reconciled { .. }
    ));
    assert!(matches!(
        worker.select_mutation_create(
            PathBuf::from("C:\\never"),
            RelativeFilePath::parse("leaf.bin").unwrap(),
            scope(Disposition::Create),
            [9; 32],
        ),
        Err(OwnerCommandError::Closed)
    ));
    assert!(matches!(
        worker.submit_owner_command(vec![1], 1),
        Err(OwnerCommandError::Closed)
    ));
    assert!(matches!(
        worker.submit(vec![1], Box::new(PanicRouter), Duration::from_secs(1)),
        Err(JobError::InvalidOptions)
    ));
    assert_eq!(worker.owner_command_usage().0, 0);
    let mut worker = worker;
    worker.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while worker.try_reclaim().unwrap().is_none() {
        assert!(Instant::now() < deadline, "history worker did not exit");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn history_binding_rejects_legacy_foreign_stale_unapproved_and_revoked_scope() {
    let mut legacy = Fixture::new_with(false, true);
    let old = legacy.connect();
    assert!(
        legacy
            .owner
            .manager
            .bind_mutation_history(
                &legacy.owner.host,
                &old,
                legacy.digest,
                legacy.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                1,
            )
            .is_err()
    );
    old.close(&mut legacy.owner.host).unwrap();

    let mut unapproved = Fixture::new_with(true, false);
    let unapproved_instance = unapproved.connect();
    assert!(
        unapproved
            .owner
            .manager
            .bind_mutation_history(
                &unapproved.owner.host,
                &unapproved_instance,
                unapproved.digest,
                unapproved.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                1,
            )
            .is_err()
    );
    unapproved_instance
        .close(&mut unapproved.owner.host)
        .unwrap();

    let mut f = Fixture::new();
    let instance = f.connect();
    for (digest, revision, capabilities) in [
        (
            [9; 32],
            f.owner.manager.revision(),
            one(IoCapability::FileCreate),
        ),
        (
            f.digest,
            f.owner.manager.revision() + 1,
            one(IoCapability::FileCreate),
        ),
        (f.digest, f.owner.manager.revision(), caps()),
        (
            f.digest,
            f.owner.manager.revision(),
            one(IoCapability::HttpRequest),
        ),
    ] {
        assert!(
            f.owner
                .manager
                .bind_mutation_history(
                    &f.owner.host,
                    &instance,
                    digest,
                    revision,
                    &capabilities,
                    30_000,
                    1,
                )
                .is_err()
        );
    }
    f.owner
        .manager
        .set_enabled(ID, f.digest, false, f.owner.manager.revision())
        .unwrap();
    assert!(
        f.owner
            .manager
            .bind_mutation_history(
                &f.owner.host,
                &instance,
                f.digest,
                f.owner.manager.revision(),
                &one(IoCapability::FileCreate),
                30_000,
                1,
            )
            .is_err()
    );
    instance.close(&mut f.owner.host).unwrap();
}

#[test]
fn history_worker_cannot_borrow_declared_extended_job_or_total_budget() {
    for (job, total) in [
        (17 * 1024 * 1024, 64 * 1024 * 1024),
        (16 * 1024 * 1024, 65 * 1024 * 1024),
    ] {
        let mut f = Fixture::new();
        let instance = f.connect();
        let binding = f.history(&instance);
        let failure = match IoWorker::spawn_managed_owner(
            f.owner,
            instance,
            binding,
            || 2,
            1,
            JobLimits::mutation(1, job, total).unwrap(),
        ) {
            Ok(_) => panic!("history worker borrowed an extended mutation budget"),
            Err(failure) => failure,
        };
        assert_eq!(failure.error, JobError::InvalidOptions);
        let mut owner = failure.owner;
        failure.instance.unwrap().close(&mut owner.host).unwrap();
    }
}

#[test]
fn history_metadata_allows_exact_original_body_but_never_extends_the_total_ledger() {
    let mut f = Fixture::new();
    let bytes = vec![0x5a; MAX_JOB_BYTES as usize];
    let original = original_request(f.digest, "max-protected-original", &bytes);
    retain_prepared_original(&mut f, &original, &bytes);
    let instance = f.connect();
    let binding = f.history(&instance);
    let limits = JobLimits::mutation_history(4, MAX_JOB_BYTES, MAX_TOTAL_BYTES).unwrap();
    assert_eq!(
        limits.max_job_bytes,
        MAX_JOB_BYTES + MAX_MUTATION_HISTORY_METADATA_BYTES
    );
    assert_eq!(limits.max_total_bytes, MAX_TOTAL_BYTES);
    let worker =
        IoWorker::spawn_managed_owner(f.owner, instance, binding, || 2, 1, limits).unwrap();
    assert!(matches!(
        worker.submit(vec![1], Box::new(PanicRouter), Duration::from_secs(1)),
        Err(JobError::InvalidOptions)
    ));
    for index in 0..4 {
        let result = read_result(worker.reconcile_mutation(original.clone()).unwrap());
        if index < 3 {
            match result.unwrap() {
                MutationResponse::Reconciled {
                    record: Some(record),
                    outcome: None,
                } => {
                    assert_eq!(record.phase(), Phase::Prepared);
                    assert_eq!(record.command().operation_id, "max-protected-original");
                }
                _ => panic!("exact original protected history was not returned"),
            }
        } else {
            assert!(matches!(result, Err(TargetError::Limit)));
        }
    }
    finish(worker);
}

#[test]
fn history_metadata_cannot_be_reassigned_to_content_or_enlarged_at_spawn() {
    const SMALL: u64 = 4 * 1024 * 1024;
    assert!(JobLimits::mutation_history(1, MAX_JOB_BYTES + 1, MAX_TOTAL_BYTES).is_err());
    for (job_extra, total_extra) in [(1, 0), (0, 1)] {
        let mut f = Fixture::new();
        let instance = f.connect();
        let binding = f.history(&instance);
        let mut limits = JobLimits::mutation_history(1, MAX_JOB_BYTES, MAX_TOTAL_BYTES).unwrap();
        limits.max_job_bytes += job_extra;
        limits.max_total_bytes += total_extra;
        let failure = IoWorker::spawn_managed_owner(f.owner, instance, binding, || 2, 1, limits)
            .err()
            .expect("history profile cannot exceed its metadata or total ceiling");
        assert_eq!(failure.error, JobError::InvalidOptions);
        let mut owner = failure.owner;
        failure.instance.unwrap().close(&mut owner.host).unwrap();
    }

    let mut f = Fixture::new_with_job(true, true, SMALL);
    let instance = f.connect();
    let binding = f.history(&instance);
    let limits = JobLimits::mutation_history(2, SMALL, MAX_TOTAL_BYTES).unwrap();
    let worker =
        IoWorker::spawn_managed_owner(f.owner, instance, binding, || 2, 1, limits).unwrap();
    let over = RequestRecord::new(MutationRequest {
        operation_id: "above-declared-content".into(),
        subject: SUBJECT.into(),
        package_sha256: f.digest,
        approval_sha256: [7; 32],
        target: Target {
            reference: [3; 32],
            relative_path: Some(RelativeFilePath::parse("leaf.bin").unwrap()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: SMALL + 1,
        content_sha256: Some(Sha256::digest([]).into()),
    })
    .unwrap();
    assert!(matches!(
        read_result(worker.reconcile_mutation(over).unwrap()),
        Err(TargetError::Limit)
    ));
    let exact = RequestRecord::new(MutationRequest {
        operation_id: "at-declared-content".into(),
        subject: SUBJECT.into(),
        package_sha256: f.digest,
        approval_sha256: [7; 32],
        target: Target {
            reference: [3; 32],
            relative_path: Some(RelativeFilePath::parse("leaf.bin").unwrap()),
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: SMALL,
        content_sha256: Some(Sha256::digest([]).into()),
    })
    .unwrap();
    assert!(matches!(
        read_result(worker.reconcile_mutation(exact).unwrap()),
        Ok(MutationResponse::Reconciled {
            record: None,
            outcome: None
        })
    ));
    finish(worker);
}
