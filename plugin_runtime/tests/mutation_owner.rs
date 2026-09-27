//! Native mutation commands retain the original managed owner and use real Windows files.
#![cfg(all(windows, feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    file_effect::{CreateResult, DeleteResult},
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    file_path::RelativeFilePath,
    io_intent::Phase,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{FileMutationPlanCheckpoint, Store},
};
use morrow_plugin_runtime::{
    Limits,
    file_target::{Error as TargetError, SelectionScope},
    io_jobs::{
        HostOwner, IoWorker, JobError, JobLimits, MAX_MUTATION_CHUNK, ManagedHostOwner,
        MutationHandle, MutationResponse, OwnerCommandError, OwnerCommandPoll, WorkerExit,
    },
    manager::Manager,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.mutation-owner";
const SUBJECT: &str = "mutation.owner-test";
const APPROVAL: [u8; 32] = [0x93; 32];
fn serial_effects() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: OnceLock<Mutex<()>> = OnceLock::new();
    SERIAL
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
const HANDLER: &str = "file.owner-unused";

#[path = "support/mutation_guest_approval.rs"]
mod mutation_guest_approval;
#[path = "support/mutation_guest_budget.rs"]
mod mutation_guest_budget;
#[cfg(feature = "fault-injection")]
#[path = "support/mutation_guest_crash.rs"]
mod mutation_guest_crash;
#[path = "support/mutation_guest_delivery.rs"]
mod mutation_guest_delivery;
#[path = "support/mutation_guest_frame.rs"]
mod mutation_guest_frame;
#[path = "support/mutation_sdk_wasm.rs"]
mod mutation_sdk_wasm;

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::FileCreate,
        IoCapability::FileDelete,
        IoCapability::FileReplace,
    ])
}
fn guest() -> Vec<u8> {
    wat::parse_str(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32)
          (local $n i32)
          i32.const 0 i32.const 131072 call $read local.set $n
          i32.const 131072 local.get $n call $done drop
          i32.const 0))"#,
    )
    .unwrap()
}

struct Owner {
    host: HostRuntime,
    manager: Manager,
    identity: Arc<()>,
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    managed_gate: Arc<Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        if let Some((entered, release)) = self.gate.take() {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        Ok(())
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
        let gate = self
            .managed_gate
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some((entered, release)) = gate {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        Some(action(&self.manager, &mut self.host))
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    owner: Owner,
    clock: Arc<AtomicU64>,
    package_digest: [u8; 32],
    mutation: bool,
}
impl Fixture {
    fn new() -> Self {
        Self::with_wasm(guest(), false)
    }
    fn with_wasm(wasm: Vec<u8>, mutation: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(caps().into_iter().collect(), vec![HANDLER.into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 4;
        budget.max_resources = 8;
        budget.max_job_bytes = 4 * 1024 * 1024;
        budget.max_bytes = if mutation {
            32 * 1024 * 1024
        } else {
            8 * 1024 * 1024
        };
        budget.max_duration_ms = 30_000;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        if mutation {
            manifest
                .required_features
                .push(morrow_core::plugin_package::MUTATION_FEATURE.into());
            manifest.mutation_schema_sha256 = morrow_core::mutation::schema_digest().to_vec();
        }
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package.digest(), caps(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        Self {
            dir,
            owner: Owner {
                host,
                manager,
                identity: Arc::new(()),
                gate: None,
                managed_gate: Arc::new(Mutex::new(None)),
            },
            clock: Arc::new(AtomicU64::new(1)),
            package_digest: package.digest(),
            mutation,
        }
    }
    fn start(mut self) -> (tempfile::TempDir, [u8; 32], IoWorker<Owner>) {
        let instance = self
            .owner
            .manager
            .connect(ID, &mut self.owner.host)
            .unwrap();
        let binding = self
            .owner
            .manager
            .bind_io(
                &self.owner.host,
                &instance,
                instance.package().package().digest(),
                self.owner.manager.revision(),
                &caps(),
                30_000,
                1,
            )
            .unwrap();
        let clock = self.clock;
        let worker = IoWorker::spawn_managed_owner(
            self.owner,
            instance,
            binding,
            move || clock.fetch_add(1, Ordering::SeqCst),
            1,
            JobLimits::new(
                8,
                4 * 1024 * 1024,
                if self.mutation {
                    32 * 1024 * 1024
                } else {
                    8 * 1024 * 1024
                },
            )
            .unwrap(),
        )
        .unwrap();
        (self.dir, self.package_digest, worker)
    }
}
fn ready(handle: &MutationHandle) {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < until, "mutation command did not complete");
        thread::sleep(Duration::from_millis(1));
    }
}
fn read(handle: &mut MutationHandle) -> Result<MutationResponse, TargetError> {
    ready(handle);
    handle.read().unwrap().unwrap()
}
fn reclaim(worker: &mut IoWorker<Owner>) -> WorkerExit<Owner> {
    worker.stop();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < until, "mutation worker did not stop");
        thread::sleep(Duration::from_millis(1));
    }
}
fn restart(owner: Owner, clock: Arc<AtomicU64>) -> IoWorker<Owner> {
    restart_with_limits(
        owner,
        clock,
        JobLimits::new(8, 4 * 1024 * 1024, 8 * 1024 * 1024).unwrap(),
    )
}
fn restart_with_limits(
    mut owner: Owner,
    clock: Arc<AtomicU64>,
    limits: JobLimits,
) -> IoWorker<Owner> {
    let now = clock.load(Ordering::SeqCst);
    let instance = owner.manager.connect(ID, &mut owner.host).unwrap();
    let binding = owner
        .manager
        .bind_io(
            &owner.host,
            &instance,
            instance.package().package().digest(),
            owner.manager.revision(),
            &caps(),
            now + 30_000,
            now,
        )
        .unwrap();
    IoWorker::spawn_managed_owner(
        owner,
        instance,
        binding,
        move || clock.fetch_add(1, Ordering::SeqCst),
        1,
        limits,
    )
    .unwrap()
}
fn scope(disposition: Disposition) -> SelectionScope {
    SelectionScope {
        subject: SUBJECT.into(),
        approval_sha256: APPROVAL,
        disposition,
    }
}
fn selected(handle: &mut MutationHandle) -> ([u8; 32], Option<[u8; 32]>) {
    let MutationResponse::Selected {
        reference,
        expected_identity,
    } = read(handle).unwrap()
    else {
        panic!("expected selected target");
    };
    (reference, expected_identity)
}
fn request(
    package_sha256: [u8; 32],
    operation: &str,
    disposition: Disposition,
    reference: [u8; 32],
    relative_path: Option<RelativeFilePath>,
    expected_identity: Option<[u8; 32]>,
    bytes: &[u8],
) -> RequestRecord {
    let (length, digest) = match disposition {
        Disposition::Delete => (0, None),
        _ => (bytes.len() as u64, Some(Sha256::digest(bytes).into())),
    };
    RequestRecord::new(MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256,
        approval_sha256: APPROVAL,
        target: Target {
            reference,
            relative_path,
        },
        disposition,
        expected_identity,
        content_length: length,
        content_sha256: digest,
    })
    .unwrap()
}

fn planned(handle: &mut MutationHandle) -> RequestRecord {
    let MutationResponse::Planned(plan) = read(handle).unwrap() else {
        panic!("expected canonical draft plan");
    };
    plan
}

fn plans(handle: &mut MutationHandle) -> (Vec<RequestRecord>, u32, bool) {
    let (plans, scanned, done, _) = plans_with_checkpoint(handle);
    (plans, scanned, done)
}
fn plans_with_checkpoint(
    handle: &mut MutationHandle,
) -> (
    Vec<RequestRecord>,
    u32,
    bool,
    Option<FileMutationPlanCheckpoint>,
) {
    let MutationResponse::Plans {
        plans,
        scanned,
        done,
        checkpoint,
    } = read(handle).unwrap()
    else {
        panic!("expected discovery page");
    };
    (plans, scanned, done, checkpoint)
}

#[test]
fn original_owner_discovers_prepared_plans_in_bounded_pages_without_file_effects() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, _, mut worker) = fixture.start();
    let root = dir.path().join("discovery-root");
    fs::create_dir(&root).unwrap();
    let digest = Some(Sha256::digest([]).into());
    let mut originals = Vec::new();
    for (index, operation) in ["discover-create-a", "discover-create-b"]
        .iter()
        .enumerate()
    {
        let relative = RelativeFilePath::parse(&format!("leaf-{index}.bin")).unwrap();
        let (session, mut select) = worker
            .select_mutation_create(
                root.clone(),
                relative,
                scope(Disposition::Create),
                [90 + index as u8; 32],
            )
            .unwrap();
        selected(&mut select);
        let plan = planned(
            &mut worker
                .build_mutation_plan(session, (*operation).into(), 0, digest)
                .unwrap(),
        );
        assert!(matches!(
            read(&mut worker.prepare_mutation(session, plan.clone()).unwrap()),
            Ok(MutationResponse::Prepared(_))
        ));
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
        originals.push(plan);
    }
    let path = dir.path().join("discovery-delete.bin");
    fs::write(&path, b"still here").unwrap();
    let (delete, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [92; 32])
        .unwrap();
    selected(&mut select);
    let delete_plan = planned(
        &mut worker
            .build_mutation_plan(delete, "discover-delete".into(), 0, None)
            .unwrap(),
    );
    assert!(matches!(
        read(
            &mut worker
                .prepare_mutation(delete, delete_plan.clone())
                .unwrap()
        ),
        Ok(MutationResponse::Prepared(_))
    ));
    assert!(matches!(
        read(&mut worker.release_mutation(delete).unwrap()),
        Ok(MutationResponse::Released)
    ));

    let (cursor, mut first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Create, 1)
        .unwrap();
    let (found, scanned, done) = plans(&mut first);
    assert_eq!(scanned, 1);
    assert!(!done);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].container(), originals[0].container());
    assert!(matches!(
        read(
            &mut worker
                .open_mutation_discovery(SUBJECT.into(), Disposition::Delete, 8)
                .unwrap()
                .1
        ),
        Err(TargetError::Limit)
    ));
    let (found, scanned, done) = plans(&mut worker.next_mutation_plans(cursor, 1).unwrap());
    assert_eq!(scanned, 1);
    assert!(!done); // the global scan still has the delete candidate
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].container(), originals[1].container());
    let (found, scanned, done) = plans(&mut worker.next_mutation_plans(cursor, 1).unwrap());
    assert_eq!(scanned, 1);
    assert!(done);
    assert!(found.is_empty());
    assert!(matches!(
        read(&mut worker.close_mutation_discovery(cursor).unwrap()),
        Ok(MutationResponse::Released)
    ));

    let (cursor, mut first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Delete, 8)
        .unwrap();
    let (found, scanned, done) = plans(&mut first);
    assert_eq!(scanned, 3);
    assert!(done);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].container(), delete_plan.container());
    assert!(matches!(
        read(&mut worker.close_mutation_discovery(cursor).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert!(!root.join("leaf-0.bin").exists());
    assert!(!root.join("leaf-1.bin").exists());
    assert_eq!(fs::read(path).unwrap(), b"still here");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn discovery_checkpoint_continues_on_new_worker_after_lease_exhaustion() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let clock = Arc::clone(&fixture.clock);
    let (dir, _, mut worker) = fixture.start();
    let root = dir.path().join("checkpoint-root");
    fs::create_dir(&root).unwrap();
    for index in 0..3 {
        let (session, mut select) = worker
            .select_mutation_create(
                root.clone(),
                RelativeFilePath::parse(&format!("{index}.bin")).unwrap(),
                scope(Disposition::Create),
                [100 + index; 32],
            )
            .unwrap();
        selected(&mut select);
        let plan = planned(
            &mut worker
                .build_mutation_plan(
                    session,
                    format!("checkpoint-plan-{index}"),
                    0,
                    Some(Sha256::digest([]).into()),
                )
                .unwrap(),
        );
        assert!(matches!(
            read(&mut worker.prepare_mutation(session, plan).unwrap()),
            Ok(MutationResponse::Prepared(_))
        ));
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
    }
    let (cursor, mut first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Create, 1)
        .unwrap();
    let (found, scanned, done, checkpoint) = plans_with_checkpoint(&mut first);
    assert_eq!(scanned, 1);
    assert!(!done);
    assert_eq!(found[0].request().operation_id, "checkpoint-plan-0");
    let checkpoint = checkpoint.unwrap();
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(
        worker.next_mutation_plans(cursor, 1),
        Err(OwnerCommandError::Closed)
    ));
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    let mut worker = restart_with_limits(
        exit.owner,
        Arc::clone(&clock),
        JobLimits::new(8, 400 * 1024, 400 * 1024).unwrap(),
    );
    let (cursor, mut second) = worker
        .open_mutation_discovery_from(SUBJECT.into(), Disposition::Create, 1, Some(checkpoint))
        .unwrap();
    let (found, scanned, done, checkpoint) = plans_with_checkpoint(&mut second);
    assert_eq!(scanned, 1);
    assert!(!done);
    assert_eq!(found[0].request().operation_id, "checkpoint-plan-1");
    let checkpoint = checkpoint.unwrap();
    assert!(matches!(
        worker.next_mutation_plans(cursor, 1),
        Err(OwnerCommandError::Limit)
    ));
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    let mut worker = restart(exit.owner, clock);
    let (cursor, mut last) = worker
        .open_mutation_discovery_from(SUBJECT.into(), Disposition::Create, 1, Some(checkpoint))
        .unwrap();
    let (found, scanned, done, checkpoint) = plans_with_checkpoint(&mut last);
    assert_eq!(scanned, 1);
    assert!(done);
    assert!(checkpoint.is_none());
    assert_eq!(found[0].request().operation_id, "checkpoint-plan-2");
    assert!(matches!(
        read(&mut worker.close_mutation_discovery(cursor).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert!(!root.join("0.bin").exists());
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn discovery_rejects_invalid_foreign_and_lost_pages_without_skipping_originals() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let clock = Arc::clone(&fixture.clock);
    let (dir, _, mut worker) = fixture.start();
    let root = dir.path().join("discovery-retry");
    fs::create_dir(&root).unwrap();
    for index in 0..2 {
        let (session, mut select) = worker
            .select_mutation_create(
                root.clone(),
                RelativeFilePath::parse(&format!("{index}.bin")).unwrap(),
                scope(Disposition::Create),
                [94 + index as u8; 32],
            )
            .unwrap();
        selected(&mut select);
        let plan = planned(
            &mut worker
                .build_mutation_plan(
                    session,
                    format!("retry-plan-{index}"),
                    0,
                    Some(Sha256::digest([]).into()),
                )
                .unwrap(),
        );
        assert!(matches!(
            read(&mut worker.prepare_mutation(session, plan).unwrap()),
            Ok(MutationResponse::Prepared(_))
        ));
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
    }
    let used = worker.owner_command_usage();
    let bytes = worker.bytes();
    assert!(matches!(
        worker.open_mutation_discovery("bad/subject".into(), Disposition::Create, 1),
        Err(OwnerCommandError::Limit)
    ));
    assert!(matches!(
        worker.open_mutation_discovery(SUBJECT.into(), Disposition::Create, 0),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(worker.owner_command_usage(), used);
    assert_eq!(worker.bytes(), bytes);

    let (cursor, mut first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Create, 1)
        .unwrap();
    let (found, _, done, checkpoint) = plans_with_checkpoint(&mut first);
    assert_eq!(found[0].request().operation_id, "retry-plan-0");
    assert!(!done);
    let checkpoint = checkpoint.unwrap();
    let other = Fixture::new();
    let (_other_dir, _, mut foreign) = other.start();
    let used = foreign.owner_command_usage();
    assert!(matches!(
        foreign.next_mutation_plans(cursor, 1),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(foreign.owner_command_usage(), used);
    let lost = worker.next_mutation_plans(cursor, 1).unwrap();
    ready(&lost);
    drop(lost); // the advanced page was never delivered to the caller
    assert!(matches!(
        read(&mut worker.next_mutation_plans(cursor, 1).unwrap()),
        Err(TargetError::Missing)
    ));
    let (resume, mut second) = worker
        .open_mutation_discovery_from(SUBJECT.into(), Disposition::Create, 1, Some(checkpoint))
        .unwrap();
    let (found, _, done, checkpoint) = plans_with_checkpoint(&mut second);
    assert_eq!(found[0].request().operation_id, "retry-plan-1");
    assert!(done);
    assert!(checkpoint.is_none());
    assert!(matches!(
        read(&mut worker.close_mutation_discovery(resume).unwrap()),
        Ok(MutationResponse::Released)
    ));
    let (retry, mut first) = worker
        .open_mutation_discovery(SUBJECT.into(), Disposition::Create, 1)
        .unwrap();
    let (found, _, _) = plans(&mut first);
    assert_eq!(found[0].request().operation_id, "retry-plan-0");
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(
        worker.next_mutation_plans(retry, 1),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
    assert_eq!(reclaim(&mut foreign).disconnect, Ok(()));
}

#[test]
fn draft_plans_use_retained_create_and_delete_authority_without_persistence() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let root = dir.path().join("draft-root");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse("new.bin").unwrap();
    let body = b"draft payload";
    let digest = Some(Sha256::digest(body).into());
    let (create, mut select) = worker
        .select_mutation_create(
            root.clone(),
            relative.clone(),
            scope(Disposition::Create),
            [81; 32],
        )
        .unwrap();
    let (reference, expected) = selected(&mut select);
    assert_eq!(expected, None);
    let plan = planned(
        &mut worker
            .build_mutation_plan(create, "draft-create".into(), body.len() as u64, digest)
            .unwrap(),
    );
    let canonical = request(
        package,
        "draft-create",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        body,
    );
    assert_eq!(plan.request(), canonical.request());
    assert_eq!(plan.container(), canonical.container());
    let repeated = planned(
        &mut worker
            .build_mutation_plan(create, "draft-create".into(), body.len() as u64, digest)
            .unwrap(),
    );
    assert_eq!(repeated.container(), plan.container());
    let revised = planned(
        &mut worker
            .build_mutation_plan(create, "revised-create".into(), body.len() as u64, digest)
            .unwrap(),
    );
    assert_eq!(revised.request().operation_id, "revised-create");
    let path = dir.path().join("draft-delete.bin");
    fs::write(&path, b"keep this file").unwrap();
    let (delete, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [82; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    let plan = planned(
        &mut worker
            .build_mutation_plan(delete, "draft-delete".into(), 0, None)
            .unwrap(),
    );
    let canonical = request(
        package,
        "draft-delete",
        Disposition::Delete,
        reference,
        None,
        expected,
        b"",
    );
    assert_eq!(plan.request(), canonical.request());
    assert_eq!(plan.container(), canonical.container());
    for session in [create, delete] {
        let mut history = worker.query_mutation(session).unwrap();
        assert!(matches!(
            read(&mut history),
            Ok(MutationResponse::History {
                record: None,
                staged_bytes: 0,
                durable_content: false
            })
        ));
        assert!(matches!(
            read(&mut worker.release_mutation(session).unwrap()),
            Ok(MutationResponse::Released)
        ));
    }
    assert!(!root.join("new.bin").exists());
    assert_eq!(fs::read(path).unwrap(), b"keep this file");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
    let store = Store::open(&dir.path().join("db"), Default::default()).unwrap();
    assert!(
        store
            .lookup_io_intent(SUBJECT, "draft-create")
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .lookup_io_intent(SUBJECT, "draft-delete")
            .unwrap()
            .is_none()
    );
}

#[test]
fn draft_plan_rejects_foreign_invalid_expired_released_and_prepared_changes() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let clock = Arc::clone(&fixture.clock);
    let (dir, _, mut worker) = fixture.start();
    let other = Fixture::new();
    let (_other_dir, _, mut foreign) = other.start();
    let path = dir.path().join("draft-guard.bin");
    fs::write(&path, b"unchanged").unwrap();
    let (session, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [83; 32])
        .unwrap();
    selected(&mut select);
    let used = foreign.owner_command_usage();
    let bytes = foreign.bytes();
    assert!(matches!(
        foreign.build_mutation_plan(session, "foreign".into(), 0, None),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(foreign.owner_command_usage(), used);
    assert_eq!(foreign.bytes(), bytes);
    let used = worker.owner_command_usage();
    let bytes = worker.bytes();
    assert!(matches!(
        worker.build_mutation_plan(session, "bad/id".into(), 0, None),
        Err(OwnerCommandError::Limit)
    ));
    assert!(matches!(
        worker.build_mutation_plan(session, "oversized".into(), 16 * 1024 * 1024 + 1, None),
        Err(OwnerCommandError::Limit)
    ));
    assert!(matches!(
        worker.build_mutation_plan(session, "missing-digest".into(), 1, None),
        Err(OwnerCommandError::Limit)
    ));
    assert!(matches!(
        worker.build_mutation_plan(session, "zero-digest".into(), 1, Some([0; 32])),
        Err(OwnerCommandError::Limit)
    ));
    assert!(matches!(
        worker.build_mutation_plan(session, "bad-empty".into(), 0, Some([3; 32])),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(worker.owner_command_usage(), used);
    assert_eq!(worker.bytes(), bytes);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "original-draft".into(), 0, None)
            .unwrap(),
    );
    let prior = plan.container().to_vec();
    assert!(matches!(
        read(&mut worker.prepare_mutation(session, plan).unwrap()),
        Ok(MutationResponse::Prepared(_))
    ));
    assert_eq!(
        planned(
            &mut worker
                .build_mutation_plan(session, "original-draft".into(), 0, None)
                .unwrap()
        )
        .container(),
        prior,
    );
    assert!(matches!(
        read(
            &mut worker
                .build_mutation_plan(session, "different-draft".into(), 0, None)
                .unwrap()
        ),
        Err(TargetError::Mismatch)
    ));
    let (released, mut selected_handle) = worker
        .select_mutation_create(
            dir.path().to_path_buf(),
            RelativeFilePath::parse("released.bin").unwrap(),
            scope(Disposition::Create),
            [84; 32],
        )
        .unwrap();
    selected(&mut selected_handle);
    assert!(matches!(
        read(&mut worker.release_mutation(released).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert!(matches!(
        read(
            &mut worker
                .build_mutation_plan(
                    released,
                    "released-draft".into(),
                    0,
                    Some(Sha256::digest([]).into())
                )
                .unwrap()
        ),
        Err(TargetError::Missing)
    ));
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(
        worker.build_mutation_plan(session, "original-draft".into(), 0, None),
        Err(OwnerCommandError::Closed)
    ));
    // Once the lease is stale, the owner stops accepting even the prior draft.
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
    assert_eq!(reclaim(&mut foreign).disconnect, Ok(()));
    assert_eq!(fs::read(path).unwrap(), b"unchanged");
}

#[test]
fn cancelled_or_stopped_draft_command_cannot_bind_a_plan() {
    let _serial = serial_effects();
    let mut fixture = Fixture::new();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    fixture.owner.gate = Some((entered_tx, release_rx));
    let (dir, _, mut worker) = fixture.start();
    let root = dir.path().join("cancel-draft");
    fs::create_dir(&root).unwrap();
    let (session, mut select) = worker
        .select_mutation_create(
            root.clone(),
            RelativeFilePath::parse("empty.bin").unwrap(),
            scope(Disposition::Create),
            [85; 32],
        )
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut draft = worker
        .build_mutation_plan(
            session,
            "cancelled-draft".into(),
            0,
            Some(Sha256::digest([]).into()),
        )
        .unwrap();
    draft.cancel();
    assert!(matches!(draft.read(), Err(OwnerCommandError::Cancelled)));
    release_tx.send(()).unwrap();
    selected(&mut select);
    let mut history = worker.query_mutation(session).unwrap();
    assert!(matches!(
        read(&mut history),
        Ok(MutationResponse::History {
            record: None,
            staged_bytes: 0,
            durable_content: false
        })
    ));
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert!(!root.join("empty.bin").exists());
    worker.stop();
    assert!(matches!(
        worker.build_mutation_plan(
            session,
            "revoked-draft".into(),
            0,
            Some(Sha256::digest([]).into())
        ),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn chunked_create_queries_durable_history_then_deletes_on_the_original_owner() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let identity = Arc::clone(&fixture.owner.identity);
    let (dir, package, mut worker) = fixture.start();
    let root = dir.path().join("root");
    let parent = root.join("子目录");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&parent).unwrap();
    let relative = RelativeFilePath::parse("子目录/新建.bin").unwrap();
    let leaf = parent.join("新建.bin");
    let bytes = (0..(MAX_MUTATION_CHUNK * 2 + 47))
        .map(|n| (n % 251) as u8)
        .collect::<Vec<_>>();
    assert!(bytes.len() > 64 * 1024);
    let (create, mut select) = worker
        .select_mutation_create(root, relative.clone(), scope(Disposition::Create), [1; 32])
        .unwrap();
    let (reference, identity_at_selection) = selected(&mut select);
    assert_eq!(identity_at_selection, None);
    assert!(!leaf.exists());
    let plan = request(
        package,
        "owner-create",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        &bytes,
    );
    let mut prepare = worker.prepare_mutation(create, plan.clone()).unwrap();
    let MutationResponse::Prepared(record) = read(&mut prepare).unwrap() else {
        panic!("expected Prepared");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    let mut offset = 0;
    for chunk in bytes.chunks(MAX_MUTATION_CHUNK) {
        let mut stage = worker
            .stage_mutation_chunk(create, offset as u64, chunk.to_vec())
            .unwrap();
        let MutationResponse::Staged {
            bytes: staged,
            durable,
        } = read(&mut stage).unwrap()
        else {
            panic!("expected staged progress");
        };
        offset += chunk.len();
        assert_eq!(staged, offset as u64);
        assert!(!durable);
    }
    let mut commit = worker.commit_mutation_content(create).unwrap();
    let MutationResponse::Staged {
        bytes: committed,
        durable,
    } = read(&mut commit).unwrap()
    else {
        panic!("expected durable staged response");
    };
    assert_eq!(committed, bytes.len() as u64);
    assert!(durable);
    let mut history = worker.query_mutation(create).unwrap();
    let MutationResponse::History {
        record: Some(record),
        staged_bytes,
        durable_content,
    } = read(&mut history).unwrap()
    else {
        panic!("expected prepared history");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    assert_eq!(staged_bytes, bytes.len() as u64);
    assert!(durable_content);

    let mut effect = worker.execute_mutation(create).unwrap();
    let MutationResponse::Created(outcome) = read(&mut effect).unwrap() else {
        panic!("expected Create outcome");
    };
    assert_eq!(outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    assert!(matches!(
        read(
            &mut worker
                .build_mutation_plan(
                    create,
                    "owner-create".into(),
                    bytes.len() as u64,
                    Some(Sha256::digest(&bytes).into())
                )
                .unwrap()
        ),
        Err(TargetError::Missing)
    ));
    let mut history = worker.query_mutation(create).unwrap();
    let MutationResponse::History {
        record: Some(record),
        durable_content: true,
        ..
    } = read(&mut history).unwrap()
    else {
        panic!("expected observed create history");
    };
    assert_eq!(record.phase(), Phase::Observed);

    let (delete, mut select) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [2; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    let plan = request(
        package,
        "owner-delete",
        Disposition::Delete,
        reference,
        None,
        expected,
        b"",
    );
    let mut prepare = worker.prepare_mutation(delete, plan).unwrap();
    assert!(matches!(
        read(&mut prepare),
        Ok(MutationResponse::Prepared(_))
    ));
    let mut effect = worker.execute_mutation(delete).unwrap();
    let MutationResponse::Deleted(outcome) = read(&mut effect).unwrap() else {
        panic!("expected Delete outcome");
    };
    assert_eq!(outcome.result(), DeleteResult::Deleted);
    assert!(!leaf.exists());
    assert!(matches!(
        read(
            &mut worker
                .build_mutation_plan(delete, "owner-delete".into(), 0, None)
                .unwrap()
        ),
        Err(TargetError::Missing)
    ));
    let mut history = worker.query_mutation(delete).unwrap();
    let MutationResponse::History {
        record: Some(record),
        ..
    } = read(&mut history).unwrap()
    else {
        panic!("expected observed delete history");
    };
    assert_eq!(record.phase(), Phase::Observed);

    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert!(Arc::ptr_eq(&identity, &exit.owner.identity));
    drop(exit);
    let reopened = Store::open(&dir.path().join("db"), Default::default()).unwrap();
    assert_eq!(
        reopened
            .lookup_io_intent(SUBJECT, "owner-create")
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
    assert_eq!(
        reopened
            .lookup_io_intent(SUBJECT, "owner-delete")
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
}

#[test]
fn replace_stays_prepared_and_unsupported_without_changing_selected_file() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let path = dir.path().join("replace.bin");
    fs::write(&path, b"original bytes").unwrap();
    let (session, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Replace), [3; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    let replacement = b"replacement not allowed";
    let plan = request(
        package,
        "owner-replace",
        Disposition::Replace,
        reference,
        None,
        expected,
        replacement,
    );
    let mut prepare = worker.prepare_mutation(session, plan).unwrap();
    assert!(matches!(
        read(&mut prepare),
        Ok(MutationResponse::Prepared(_))
    ));
    let mut effect = worker.execute_mutation(session).unwrap();
    assert!(matches!(
        read(&mut effect),
        Err(TargetError::UnsupportedConditionalReplacement)
    ));
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(record),
        durable_content: false,
        ..
    } = read(&mut history).unwrap()
    else {
        panic!("expected Prepared history");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    let mut release = worker.release_mutation(session).unwrap();
    assert!(matches!(read(&mut release), Ok(MutationResponse::Released)));
    assert_eq!(fs::read(path).unwrap(), b"original bytes");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn foreign_worker_session_is_rejected_without_its_budget_charge() {
    let _serial = serial_effects();
    let first = Fixture::new();
    let second = Fixture::new();
    let (dir, _, mut a) = first.start();
    let (_other_dir, _, mut b) = second.start();
    let path = dir.path().join("foreign.bin");
    fs::write(&path, b"do not touch").unwrap();
    let (session, mut selected_handle) = a
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [4; 32])
        .unwrap();
    selected(&mut selected_handle);
    let queued = b.owner_command_usage();
    let bytes = b.bytes();
    assert!(matches!(
        b.execute_mutation(session),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(b.owner_command_usage(), queued);
    assert_eq!(b.bytes(), bytes);
    let mut release = a.release_mutation(session).unwrap();
    assert!(matches!(read(&mut release), Ok(MutationResponse::Released)));
    assert_eq!(fs::read(path).unwrap(), b"do not touch");
    assert_eq!(reclaim(&mut a).disconnect, Ok(()));
    assert_eq!(reclaim(&mut b).disconnect, Ok(()));
}

#[test]
fn release_and_worker_stop_return_exclusive_file_handle_to_the_original_owner() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let identity = Arc::clone(&fixture.owner.identity);
    let (dir, _, mut worker) = fixture.start();
    let path = dir.path().join("held.bin");
    fs::write(&path, b"exclusive until release").unwrap();
    let (session, mut handle) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [5; 32])
        .unwrap();
    selected(&mut handle);
    assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
    let mut release = worker.release_mutation(session).unwrap();
    assert!(matches!(read(&mut release), Ok(MutationResponse::Released)));
    assert_eq!(fs::read(&path).unwrap(), b"exclusive until release");

    let (_session, mut handle) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [6; 32])
        .unwrap();
    selected(&mut handle);
    assert_eq!(fs::read(&path).unwrap_err().raw_os_error(), Some(32));
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    assert!(Arc::ptr_eq(&identity, &exit.owner.identity));
    assert_eq!(fs::read(&path).unwrap(), b"exclusive until release");
}

#[test]
fn bounded_queue_keeps_reservations_and_cancelled_prepare_gate_never_opens_path() {
    let _serial = serial_effects();
    let mut fixture = Fixture::new();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    fixture.owner.gate = Some((entered_tx, release_rx));
    let (dir, _, mut worker) = fixture.start();
    let path = dir.path().join("never-opened.bin");
    fs::write(&path, b"queued cancellation").unwrap();
    let (_, mut first) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [7; 32])
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(first.poll(), OwnerCommandPoll::Pending);
    let mut queued = Vec::new();
    for n in 1..8 {
        let (_, handle) = worker
            .select_mutation_existing(path.clone(), scope(Disposition::Delete), [n as u8 + 7; 32])
            .unwrap();
        queued.push(handle);
    }
    let admitted = worker.owner_command_usage();
    assert_eq!(admitted.0, 8);
    assert!(admitted.1 > 0);
    assert!(matches!(
        worker.select_mutation_existing(path.clone(), scope(Disposition::Delete), [20; 32]),
        Err(OwnerCommandError::Busy)
    ));
    assert_eq!(worker.owner_command_usage(), admitted);
    first.cancel();
    assert!(matches!(first.read(), Err(OwnerCommandError::Cancelled)));
    assert_eq!(worker.owner_command_usage(), admitted);
    // The first command is blocked in prepare_io, before dispatch/open.
    fs::remove_file(&path).unwrap();
    assert!(!path.exists());
    worker.stop();
    release_tx.send(()).unwrap();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    drop(queued);
    assert_eq!(worker.owner_command_usage().0, 0);
}

#[test]
fn lost_execute_reply_is_reconciled_by_readonly_history_without_replaying_create() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let root = dir.path().join("reply-root");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse("only-once.bin").unwrap();
    let leaf = root.join("only-once.bin");
    let bytes = b"effect survives a dropped reply";
    let (session, mut select) = worker
        .select_mutation_create(root, relative.clone(), scope(Disposition::Create), [21; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    assert_eq!(expected, None);
    let plan = request(
        package,
        "lost-create-reply",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        bytes,
    );
    let mut prepare = worker.prepare_mutation(session, plan).unwrap();
    assert!(matches!(
        read(&mut prepare),
        Ok(MutationResponse::Prepared(_))
    ));
    let mut chunk = worker
        .stage_mutation_chunk(session, 0, bytes.to_vec())
        .unwrap();
    assert!(matches!(
        read(&mut chunk),
        Ok(MutationResponse::Staged { .. })
    ));
    let mut commit = worker.commit_mutation_content(session).unwrap();
    assert!(matches!(
        read(&mut commit),
        Ok(MutationResponse::Staged { durable: true, .. })
    ));
    let effect = worker.execute_mutation(session).unwrap();
    ready(&effect);
    drop(effect); // single-use reply is intentionally lost after the real effect
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(record),
        durable_content: true,
        ..
    } = read(&mut history).unwrap()
    else {
        panic!("expected original observed history");
    };
    assert_eq!(record.phase(), Phase::Observed);
    fs::remove_file(&leaf).unwrap();
    let mut repeat = worker.execute_mutation(session).unwrap();
    assert!(matches!(
        read(&mut repeat),
        Err(TargetError::AlreadyDispatched)
    ));
    assert!(!leaf.exists());
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn oversized_chunk_allocation_is_rejected_before_queue_or_budget_admission() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, _, mut worker) = fixture.start();
    let path = dir.path().join("capacity.bin");
    fs::write(&path, b"selected only").unwrap();
    let (session, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [31; 32])
        .unwrap();
    selected(&mut select);
    let queued = worker.owner_command_usage();
    let bytes = worker.bytes();
    let mut oversized = Vec::with_capacity(MAX_MUTATION_CHUNK * 2);
    oversized.push(0x5a);
    assert_eq!(oversized.len(), 1);
    assert!(oversized.capacity() > MAX_MUTATION_CHUNK);
    assert!(matches!(
        worker.stage_mutation_chunk(session, 0, oversized),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(worker.owner_command_usage(), queued);
    assert_eq!(worker.bytes(), bytes);
    let mut release = worker.release_mutation(session).unwrap();
    assert!(matches!(read(&mut release), Ok(MutationResponse::Released)));
    assert_eq!(fs::read(path).unwrap(), b"selected only");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn failed_prepare_package_or_expected_identity_does_not_bind_the_session() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let path = dir.path().join("prepare-target.bin");
    fs::write(&path, b"unchanged").unwrap();
    let (session, mut select) = worker
        .select_mutation_existing(path.clone(), scope(Disposition::Delete), [32; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    let correct = request(
        package,
        "prepare-after-bad-plans",
        Disposition::Delete,
        reference,
        None,
        expected,
        b"",
    );
    let mut bad_package = correct.request().clone();
    bad_package.package_sha256 = [0xee; 32];
    let mut result = worker
        .prepare_mutation(session, RequestRecord::new(bad_package).unwrap())
        .unwrap();
    assert!(matches!(read(&mut result), Err(TargetError::Mismatch)));
    let mut bad_identity = correct.request().clone();
    bad_identity.expected_identity = Some([0x99; 32]);
    let mut result = worker
        .prepare_mutation(session, RequestRecord::new(bad_identity).unwrap())
        .unwrap();
    assert!(matches!(read(&mut result), Err(TargetError::Mismatch)));
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: None,
        staged_bytes: 0,
        durable_content: false,
    } = read(&mut history).unwrap()
    else {
        panic!("failed plans must not bind or persist");
    };
    let mut result = worker.prepare_mutation(session, correct).unwrap();
    let MutationResponse::Prepared(record) = read(&mut result).unwrap() else {
        panic!("correct plan must still prepare");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    let mut release = worker.release_mutation(session).unwrap();
    assert!(matches!(read(&mut release), Ok(MutationResponse::Released)));
    assert_eq!(fs::read(path).unwrap(), b"unchanged");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn invalid_chunk_offsets_preserve_progress_and_lost_commit_reply_is_queried() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let root = dir.path().join("chunk-root");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse("chunked.bin").unwrap();
    let leaf = root.join("chunked.bin");
    let bytes = (0..MAX_MUTATION_CHUNK + 11)
        .map(|n| (n % 239) as u8)
        .collect::<Vec<_>>();
    let (session, mut select) = worker
        .select_mutation_create(root, relative.clone(), scope(Disposition::Create), [33; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    assert_eq!(expected, None);
    let plan = request(
        package,
        "chunk-progress",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        &bytes,
    );
    let mut prepare = worker.prepare_mutation(session, plan).unwrap();
    assert!(matches!(
        read(&mut prepare),
        Ok(MutationResponse::Prepared(_))
    ));
    let mut first = worker
        .stage_mutation_chunk(session, 0, bytes[..MAX_MUTATION_CHUNK].to_vec())
        .unwrap();
    assert!(matches!(
        read(&mut first),
        Ok(MutationResponse::Staged {
            bytes: n,
            durable: false
        }) if n == MAX_MUTATION_CHUNK as u64
    ));
    for (offset, chunk) in [
        (0, vec![0x31]),
        (MAX_MUTATION_CHUNK as u64 + 1, vec![0x32]),
        (MAX_MUTATION_CHUNK as u64, vec![0x33; 12]),
    ] {
        let mut invalid = worker.stage_mutation_chunk(session, offset, chunk).unwrap();
        assert!(matches!(read(&mut invalid), Err(TargetError::Mismatch)));
    }
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(record),
        staged_bytes,
        durable_content,
    } = read(&mut history).unwrap()
    else {
        panic!("expected in-memory progress");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    assert_eq!(staged_bytes, MAX_MUTATION_CHUNK as u64);
    assert!(!durable_content);
    assert!(!leaf.exists());

    let mut second = worker
        .stage_mutation_chunk(
            session,
            MAX_MUTATION_CHUNK as u64,
            bytes[MAX_MUTATION_CHUNK..].to_vec(),
        )
        .unwrap();
    assert!(matches!(
        read(&mut second),
        Ok(MutationResponse::Staged {
            bytes: n,
            durable: false
        }) if n == bytes.len() as u64
    ));
    let commit = worker.commit_mutation_content(session).unwrap();
    ready(&commit);
    commit.cancel();
    let mut commit = commit;
    assert!(matches!(commit.read(), Err(OwnerCommandError::Unknown)));
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(record),
        staged_bytes,
        durable_content: true,
    } = read(&mut history).unwrap()
    else {
        panic!("lost commit response must be recovered read-only");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    assert_eq!(staged_bytes, bytes.len() as u64);
    let mut effect = worker.execute_mutation(session).unwrap();
    let MutationResponse::Created(outcome) = read(&mut effect).unwrap() else {
        panic!("expected created result");
    };
    assert_eq!(outcome.result(), CreateResult::Created);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    let mut duplicate = worker.execute_mutation(session).unwrap();
    assert!(matches!(
        read(&mut duplicate),
        Err(TargetError::AlreadyDispatched)
    ));
    let mut history = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(record),
        ..
    } = read(&mut history).unwrap()
    else {
        panic!("expected observed result");
    };
    assert_eq!(record.phase(), Phase::Observed);
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn empty_create_commits_without_chunks_and_repeated_queries_do_not_append_history() {
    let _serial = serial_effects();
    let fixture = Fixture::new();
    let (dir, package, mut worker) = fixture.start();
    let root = dir.path().join("empty-root");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse("empty.bin").unwrap();
    let leaf = root.join("empty.bin");
    let (session, mut select) = worker
        .select_mutation_create(root, relative.clone(), scope(Disposition::Create), [41; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    assert_eq!(expected, None);
    let plan = request(
        package,
        "owner-empty-create",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        b"",
    );
    let mut prepare = worker.prepare_mutation(session, plan).unwrap();
    let MutationResponse::Prepared(prepared) = read(&mut prepare).unwrap() else {
        panic!("expected empty Create plan");
    };
    assert_eq!(prepared.phase(), Phase::Prepared);

    let mut first = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(before),
        staged_bytes: 0,
        durable_content: false,
    } = read(&mut first).unwrap()
    else {
        panic!("expected unstaged zero-byte history");
    };
    assert_eq!(before.digest(), prepared.digest());
    let mut second = worker.query_mutation(session).unwrap();
    let MutationResponse::History {
        record: Some(repeated),
        staged_bytes: 0,
        durable_content: false,
    } = read(&mut second).unwrap()
    else {
        panic!("expected identical precommit query");
    };
    assert_eq!(repeated.digest(), before.digest());
    assert_eq!(repeated.event_id(), before.event_id());

    // Zero bytes still require the durable content receipt, but no chunk command.
    let mut commit = worker.commit_mutation_content(session).unwrap();
    assert!(matches!(
        read(&mut commit),
        Ok(MutationResponse::Staged {
            bytes: 0,
            durable: true
        })
    ));
    for _ in 0..2 {
        let mut history = worker.query_mutation(session).unwrap();
        let MutationResponse::History {
            record: Some(record),
            staged_bytes: 0,
            durable_content: true,
        } = read(&mut history).unwrap()
        else {
            panic!("expected durable zero-byte content");
        };
        assert_eq!(record.phase(), Phase::Prepared);
        assert_eq!(record.digest(), prepared.digest());
        assert_eq!(record.event_id(), prepared.event_id());
    }

    let mut effect = worker.execute_mutation(session).unwrap();
    let MutationResponse::Created(outcome) = read(&mut effect).unwrap() else {
        panic!("expected empty Create effect");
    };
    assert_eq!(outcome.result(), CreateResult::Created);
    assert_eq!(fs::metadata(&leaf).unwrap().len(), 0);
    let mut observed_digest = None;
    for _ in 0..2 {
        let mut history = worker.query_mutation(session).unwrap();
        let MutationResponse::History {
            record: Some(record),
            staged_bytes: 0,
            durable_content: true,
        } = read(&mut history).unwrap()
        else {
            panic!("expected observed empty Create");
        };
        assert_eq!(record.phase(), Phase::Observed);
        if let Some(previous) = observed_digest {
            assert_eq!(record.digest(), previous);
        }
        observed_digest = Some(record.digest());
    }

    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    let store = exit.owner.host.store_local();
    let pending = store.pending(0, 10).unwrap();
    // Prepared, content receipt, Unknown and Observed: queries add no events.
    assert_eq!(pending.len(), 4);
    assert_eq!(
        store
            .lookup_io_intent(SUBJECT, "owner-empty-create")
            .unwrap()
            .unwrap()
            .digest(),
        observed_digest.unwrap()
    );
    store.integrity_check().unwrap();
}

#[test]
fn durable_plan_cancel_preserves_evidence_and_lost_reply_is_queryable() {
    let _serial = serial_effects();
    for durable in [false, true] {
        let (dir, package, mut worker) = Fixture::new().start();
        let root = dir.path().join("cancel-root");
        fs::create_dir(&root).unwrap();
        let relative = RelativeFilePath::parse("never.bin").unwrap();
        let leaf = root.join("never.bin");
        let (session, mut select) = worker
            .select_mutation_create(root, relative.clone(), scope(Disposition::Create), [51; 32])
            .unwrap();
        let (reference, _) = selected(&mut select);
        let plan = request(
            package,
            "owner-cancel",
            Disposition::Create,
            reference,
            Some(relative),
            None,
            b"retained evidence",
        );
        assert!(matches!(
            read(&mut worker.cancel_mutation_plan(session).unwrap()),
            Err(TargetError::Missing)
        ));
        read(&mut worker.prepare_mutation(session, plan).unwrap()).unwrap();
        read(
            &mut worker
                .stage_mutation_chunk(session, 0, b"retained evidence".to_vec())
                .unwrap(),
        )
        .unwrap();
        if durable {
            read(&mut worker.commit_mutation_content(session).unwrap()).unwrap();
        }
        let mut cancel = worker.cancel_mutation_plan(session).unwrap();
        ready(&cancel);
        cancel.cancel();
        assert!(matches!(cancel.read(), Err(OwnerCommandError::Unknown)));
        let MutationResponse::History {
            record: Some(record),
            staged_bytes,
            durable_content,
        } = read(&mut worker.query_mutation(session).unwrap()).unwrap()
        else {
            panic!("history");
        };
        assert_eq!(record.phase(), Phase::CancelledBeforeDispatch);
        assert_eq!(durable_content, durable);
        assert_eq!(staged_bytes, if durable { 17 } else { 0 });
        let MutationResponse::PlanCancelled(repeated) =
            read(&mut worker.cancel_mutation_plan(session).unwrap()).unwrap()
        else {
            panic!("idempotent cancellation");
        };
        assert_eq!(record.digest(), repeated.digest());
        assert!(read(&mut worker.stage_mutation_chunk(session, 0, vec![1]).unwrap()).is_err());
        assert!(read(&mut worker.commit_mutation_content(session).unwrap()).is_err());
        assert!(read(&mut worker.execute_mutation(session).unwrap()).is_err());
        assert!(!leaf.exists());
        read(&mut worker.release_mutation(session).unwrap()).unwrap();
        let exit = reclaim(&mut worker);
        assert_eq!(exit.result, Ok(()));
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .io_intent_reservation_usage()
                .unwrap(),
            (0, 0)
        );
        assert_eq!(
            exit.owner.host.store_local().pending(0, 10).unwrap().len(),
            if durable { 3 } else { 2 }
        );
        drop(exit);
        let store = Store::open_existing(&dir.path().join("db"), Default::default()).unwrap();
        assert_eq!(
            store
                .lookup_io_intent(SUBJECT, "owner-cancel")
                .unwrap()
                .unwrap()
                .digest(),
            record.digest()
        );
        store.integrity_check().unwrap();
    }
}

#[test]
fn plan_cancel_cannot_relabel_an_observed_delete() {
    let _serial = serial_effects();
    let (dir, package, mut worker) = Fixture::new().start();
    let leaf = dir.path().join("delete.bin");
    fs::write(&leaf, b"once").unwrap();
    let (session, mut select) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [52; 32])
        .unwrap();
    let (reference, expected) = selected(&mut select);
    let plan = request(
        package,
        "cancel-after-delete",
        Disposition::Delete,
        reference,
        None,
        expected,
        b"",
    );
    read(&mut worker.prepare_mutation(session, plan).unwrap()).unwrap();
    read(&mut worker.execute_mutation(session).unwrap()).unwrap();
    assert!(matches!(
        read(&mut worker.cancel_mutation_plan(session).unwrap()),
        Err(TargetError::AlreadyDispatched)
    ));
    let MutationResponse::History {
        record: Some(record),
        ..
    } = read(&mut worker.query_mutation(session).unwrap()).unwrap()
    else {
        panic!("observed history");
    };
    assert_eq!(record.phase(), Phase::Observed);
    assert!(!leaf.exists());
    let exit = reclaim(&mut worker);
    assert_eq!(
        exit.owner.host.store_local().pending(0, 10).unwrap().len(),
        3
    );
    exit.owner.host.store_local().integrity_check().unwrap();
}

#[test]
fn delete_and_unsupported_replace_plans_cancel_without_touching_selected_file() {
    let _serial = serial_effects();
    for disposition in [Disposition::Delete, Disposition::Replace] {
        let (dir, package, mut worker) = Fixture::new().start();
        let leaf = dir.path().join("preserved.bin");
        fs::write(&leaf, b"original").unwrap();
        let (session, mut selection) = worker
            .select_mutation_existing(leaf.clone(), scope(disposition), [53; 32])
            .unwrap();
        let (reference, expected) = selected(&mut selection);
        let bytes: &[u8] = if disposition == Disposition::Replace {
            b"replacement"
        } else {
            b""
        };
        let plan = request(
            package,
            "cancel-existing",
            disposition,
            reference,
            None,
            expected,
            bytes,
        );
        read(&mut worker.prepare_mutation(session, plan).unwrap()).unwrap();
        if disposition == Disposition::Replace {
            assert!(matches!(
                read(&mut worker.execute_mutation(session).unwrap()),
                Err(TargetError::UnsupportedConditionalReplacement)
            ));
        }
        let MutationResponse::PlanCancelled(record) =
            read(&mut worker.cancel_mutation_plan(session).unwrap()).unwrap()
        else {
            panic!("cancel result");
        };
        assert_eq!(record.phase(), Phase::CancelledBeforeDispatch);
        assert!(read(&mut worker.execute_mutation(session).unwrap()).is_err());
        read(&mut worker.release_mutation(session).unwrap()).unwrap();
        assert_eq!(fs::read(&leaf).unwrap(), b"original");
        let exit = reclaim(&mut worker);
        assert_eq!(
            exit.owner.host.store_local().pending(0, 10).unwrap().len(),
            2
        );
        exit.owner.host.store_local().integrity_check().unwrap();
    }
}

#[test]
fn foreign_plan_cancel_rejects_before_queue_or_budget_charge() {
    let (dir, _, mut owner) = Fixture::new().start();
    let (_, _, mut foreign) = Fixture::new().start();
    let leaf = dir.path().join("foreign-cancel.bin");
    fs::write(&leaf, b"original").unwrap();
    let (session, mut selection) = owner
        .select_mutation_existing(leaf, scope(Disposition::Delete), [54; 32])
        .unwrap();
    selected(&mut selection);
    let bytes = foreign.bytes();
    assert!(matches!(
        foreign.cancel_mutation_plan(session),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(foreign.bytes(), bytes);
    reclaim(&mut foreign);
    reclaim(&mut owner);
}
