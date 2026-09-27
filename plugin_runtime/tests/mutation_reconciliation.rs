//! Session-free mutation history reads use the current managed owner and budget.
#![cfg(all(windows, feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    file_mutation::{Disposition, MutationRequest, RequestRecord, Target},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Limits,
    file_target::Error as TargetError,
    io_jobs::{
        HostOwner, IoWorker, JobError, JobLimits, ManagedHostOwner, MutationHandle,
        MutationResponse, OwnerCommandError, OwnerCommandPoll,
    },
    manager::Manager,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.mutation-reconciliation";
const SUBJECT: &str = "mutation.reconciliation-test";

struct Owner {
    manager: Manager,
    host: HostRuntime,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
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
        Some(action(&self.manager, &mut self.host))
    }
}

fn worker() -> (tempfile::TempDir, [u8; 32], Arc<AtomicU64>, IoWorker<Owner>) {
    let dir = tempfile::tempdir().unwrap();
    let wasm = wat::parse_str(
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
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let caps = BTreeSet::from([IoCapability::FileCreate]);
    let mut declaration = io::declaration(vec![IoCapability::FileCreate], vec!["unused".into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 4;
    budget.max_resources = 2;
    budget.max_job_bytes = 1024 * 1024;
    budget.max_bytes = 4 * 1024 * 1024;
    budget.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    let package = Package::build(manifest, &wasm).unwrap();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve_io(ID, package.digest(), caps.clone(), manager.revision())
        .unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let instance = manager.connect(ID, &mut host).unwrap();
    let binding = manager
        .bind_io(
            &host,
            &instance,
            package.digest(),
            manager.revision(),
            &caps,
            30_000,
            1,
        )
        .unwrap();
    let clock = Arc::new(AtomicU64::new(2));
    let ticking = clock.clone();
    let worker = IoWorker::spawn_managed_owner(
        Owner { manager, host },
        instance,
        binding,
        move || ticking.fetch_add(1, Ordering::SeqCst),
        1,
        JobLimits::new(4, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    (dir, package.digest(), clock, worker)
}

fn request(package_sha256: [u8; 32], operation: &str, length: u64) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256,
        approval_sha256: [0x71; 32],
        target: Target {
            reference: [0x72; 32],
            relative_path: None,
        },
        disposition: Disposition::Create,
        expected_identity: None,
        content_length: length,
        content_sha256: Some(Sha256::digest([]).into()),
    })
    .unwrap()
}

fn read(handle: &mut MutationHandle) -> Result<MutationResponse, TargetError> {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < until, "reconciliation did not complete");
        thread::sleep(Duration::from_millis(1));
    }
    handle.read().unwrap().unwrap()
}

fn finish(worker: &mut IoWorker<Owner>) {
    worker.stop();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            assert_eq!(exit.result, Ok(()));
            return;
        }
        assert!(Instant::now() < until, "reconciliation worker did not stop");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn absent_exact_history_is_safe_none_without_a_target_session() {
    let (_dir, digest, _clock, mut worker) = worker();
    let mut handle = worker
        .reconcile_mutation(request(digest, "missing", 0))
        .unwrap();
    assert!(matches!(
        read(&mut handle),
        Ok(MutationResponse::Reconciled {
            record: None,
            outcome: None
        })
    ));
    let mut foreign = worker
        .reconcile_mutation(request([0x99; 32], "missing", 0))
        .unwrap();
    assert!(matches!(read(&mut foreign), Err(TargetError::Mismatch)));
    finish(&mut worker);
}

#[test]
fn oversized_history_read_is_rejected_before_store_lookup() {
    let (_dir, digest, _clock, mut worker) = worker();
    let mut handle = worker
        .reconcile_mutation(request(digest, "oversized", 1024 * 1024))
        .unwrap();
    assert!(matches!(read(&mut handle), Err(TargetError::Limit)));
    finish(&mut worker);
}

#[test]
fn expired_binding_refuses_to_enqueue_even_absent_history() {
    let (_dir, digest, clock, mut worker) = worker();
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(
        worker.reconcile_mutation(request(digest, "expired", 0)),
        Err(OwnerCommandError::Closed)
    ));
    finish(&mut worker);
}

#[test]
fn ready_history_reply_is_withheld_when_binding_expires_before_read() {
    let (_dir, digest, clock, mut worker) = worker();
    let mut handle = worker
        .reconcile_mutation(request(digest, "ready-then-expired", 0))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < until, "history reply did not become ready");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(handle.is_started());
    clock.store(30_001, Ordering::SeqCst);
    assert!(matches!(handle.read(), Err(OwnerCommandError::Unknown)));
    worker.stop();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if worker.try_reclaim().unwrap().is_some() {
            break;
        }
        assert!(Instant::now() < until, "expired worker did not stop");
        thread::sleep(Duration::from_millis(1));
    }
}
