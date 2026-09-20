//! Ownership and failure qualification with a real Store and managed Wasm
//! instance. Routers here are scripted; audited/real effects are host tests.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::{ConnectionBinding, HostBinding, HostRuntime},
    io::{Request, Response, Status},
    outbound_authority::{self, Record, proto},
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
    io_binding::IoBinding,
    io_jobs::{
        HostOwner, IoWorker, JobError, JobHandle, JobLimits, Phase, Poll, Router, RouterFault,
        ServiceUpdate, WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.io.owner";
const WAIT: Duration = Duration::from_secs(10);

struct Owner {
    host: HostRuntime,
    connection: ConnectionBinding,
    identity: Arc<()>,
    drops: Arc<AtomicUsize>,
    hooks: Arc<Mutex<Vec<(&'static str, thread::ThreadId)>>>,
    prepares: usize,
    fail_prepare: Option<usize>,
    prepare_panics: bool,
    fail_finish: bool,
    finish_panics: bool,
    disconnected_at_finish: bool,
    runtime_mut_calls: usize,
    panic_runtime_mut: Option<usize>,
    swapped_host: Option<HostRuntime>,
    swap_on_prepare: bool,
    swap_on_finish: bool,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        self.runtime_mut_calls += 1;
        assert_ne!(
            self.panic_runtime_mut,
            Some(self.runtime_mut_calls),
            "injected runtime borrow panic"
        );
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("prepare", thread::current().id()));
        self.prepares += 1;
        if self.swap_on_prepare {
            std::mem::swap(&mut self.host, self.swapped_host.as_mut().unwrap());
            self.swap_on_prepare = false;
        }
        if self.fail_prepare == Some(self.prepares) {
            assert!(!self.prepare_panics, "injected preparation panic");
            return Err(JobError::Busy);
        }
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("finish", thread::current().id()));
        self.disconnected_at_finish = self.host.binding_phase(self.connection).is_err();
        if self.swap_on_finish {
            std::mem::swap(&mut self.host, self.swapped_host.as_mut().unwrap());
        }
        assert!(!self.finish_panics, "injected maintenance panic");
        if self.fail_finish {
            Err(JobError::Busy)
        } else {
            Ok(())
        }
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
struct Fixture {
    _dir: tempfile::TempDir,
    manager: Manager,
    owner: Owner,
    instance: ManagedInstance,
    binding: IoBinding,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wat::parse_str(
            r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) (local $n i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
            call $io call $done drop i32.const 0))"#,
        )
        .unwrap();
        let capabilities = BTreeSet::from([IoCapability::FileRead]);
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(io::declaration(
            capabilities.iter().copied().collect(),
            vec!["io.invoke".into()],
        ));
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                capabilities.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &capabilities,
                100,
                1,
            )
            .unwrap();
        let owner = Owner {
            host,
            connection: instance.connection().binding(),
            identity: Arc::new(()),
            drops: Arc::new(AtomicUsize::new(0)),
            hooks: Arc::new(Mutex::new(vec![])),
            prepares: 0,
            fail_prepare: None,
            prepare_panics: false,
            fail_finish: false,
            finish_panics: false,
            disconnected_at_finish: false,
            runtime_mut_calls: 0,
            panic_runtime_mut: None,
            swapped_host: None,
            swap_on_prepare: false,
            swap_on_finish: false,
        };
        Self {
            _dir: dir,
            manager,
            owner,
            instance,
            binding,
        }
    }
    fn start(self) -> (tempfile::TempDir, Manager, IoWorker<Owner>) {
        let worker = IoWorker::spawn_managed_owned(
            &self.manager,
            self.owner,
            self.instance,
            self.binding,
            || 2,
            2,
            JobLimits::default(),
        )
        .unwrap();
        (self._dir, self.manager, worker)
    }
}
struct Script(Arc<AtomicUsize>);
impl Router for Script {
    fn route(&mut self, _: u32, bytes: &[u8]) -> Result<Vec<u8>, RouterFault> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Response::encode(
            &Request::decode(bytes).unwrap(),
            Status::Completed,
            b"scripted",
            0,
            true,
        )
        .unwrap())
    }
}
fn input() -> Vec<u8> {
    Request::encode_read(1, &[5; 32], 0, 64)
        .unwrap()
        .bytes()
        .to_vec()
}
fn ready(handle: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match handle.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end);
                thread::sleep(Duration::from_millis(1));
            }
            other => panic!("unexpected job state {other:?}"),
        }
    }
}
fn reclaim(worker: &mut IoWorker<Owner>) -> WorkerExit<Owner> {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
}
fn record() -> Record {
    Record::encode(proto::Record {
        schema_version: 1,
        reference: vec![7; 32],
        revision: 1,
        created_ms: 10,
        expires_ms: 1010,
        disabled: false,
        kind: Some(proto::record::Kind::Credential(proto::Credential {
            provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
            ciphertext: vec![9; 32],
        })),
    })
    .unwrap()
}
fn update(worker: &IoWorker<Owner>) {
    let mut handle = worker
        .update_service(ServiceUpdate::Outbound {
            value: record(),
            expected_revision: 0,
        })
        .unwrap();
    let end = Instant::now() + WAIT;
    loop {
        if let Some(result) = handle.read().unwrap() {
            result.unwrap();
            return;
        }
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
}
fn check_owner(owner: &Owner, identity: &Arc<()>, host: HostBinding) {
    assert!(Arc::ptr_eq(&owner.identity, identity));
    assert_eq!(owner.host.binding(), host);
    assert_eq!(owner.drops.load(Ordering::SeqCst), 0);
    assert!(owner.disconnected_at_finish);
    let hooks = owner.hooks.lock().unwrap();
    assert_eq!(
        hooks.iter().filter(|(name, _)| *name == "finish").count(),
        1
    );
    assert!(hooks.iter().all(|(_, id)| *id != thread::current().id()));
}

#[test]
fn same_owner_returns_after_jobs_and_original_store_mutation() {
    let f = Fixture::new();
    let identity = f.owner.identity.clone();
    let original_host = f.owner.host.binding();
    let (_dir, _manager, mut worker) = f.start();
    update(&worker);
    let calls = Arc::new(AtomicUsize::new(0));
    let mut job = worker
        .submit(input(), Box::new(Script(calls.clone())), WAIT)
        .unwrap();
    ready(&mut job);
    assert_eq!(
        job.read(4096).unwrap().unwrap().response.unwrap().payload,
        b"scripted"
    );
    worker.stop();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    check_owner(&exit.owner, &identity, original_host);
    assert_eq!(exit.owner.prepares, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        exit.owner
            .host
            .store_local()
            .load_outbound_authority(&[7; 32])
            .unwrap()
            .unwrap()
            .container(),
        record().container()
    );
    assert!(matches!(worker.try_reclaim(), Err(JobError::Consumed)));
}

#[test]
fn failed_admission_and_panicking_clock_return_original_owner_and_instance() {
    for panic_clock in [false, true] {
        let f = Fixture::new();
        let identity = f.owner.identity.clone();
        let original_host = f.owner.host.binding();
        let connection = f.instance.connection().binding();
        let result = IoWorker::spawn_managed_owned(
            &f.manager,
            f.owner,
            f.instance,
            f.binding,
            move || {
                assert!(!panic_clock, "injected admission clock panic");
                2
            },
            if panic_clock { 2 } else { 0 },
            JobLimits::default(),
        );
        let mut failure = match result {
            Err(error) => error,
            Ok(_) => panic!("admitted invalid worker"),
        };
        assert_eq!(
            failure.error,
            if panic_clock {
                JobError::Unavailable
            } else {
                JobError::InvalidOptions
            }
        );
        assert!(Arc::ptr_eq(&failure.owner.identity, &identity));
        assert_eq!(failure.owner.host.binding(), original_host);
        assert_eq!(failure.owner.drops.load(Ordering::SeqCst), 0);
        assert!(failure.owner.hooks.lock().unwrap().is_empty());
        let instance = failure.instance.take().unwrap();
        assert_eq!(instance.connection().binding(), connection);
        instance.close(&mut failure.owner.host).unwrap();
    }
}

#[test]
fn preparation_failure_or_panic_revokes_ready_reports_and_preserves_owner() {
    for panic_hook in [false, true] {
        let mut f = Fixture::new();
        f.owner.fail_prepare = Some(2);
        f.owner.prepare_panics = panic_hook;
        let identity = f.owner.identity.clone();
        let original_host = f.owner.host.binding();
        let (_dir, _manager, mut worker) = f.start();
        let calls = Arc::new(AtomicUsize::new(0));
        let mut ready_job = worker
            .submit(input(), Box::new(Script(calls.clone())), WAIT)
            .unwrap();
        ready(&mut ready_job);
        let mut failed_job = worker
            .submit(input(), Box::new(Script(calls.clone())), WAIT)
            .unwrap();
        let exit = reclaim(&mut worker);
        assert_eq!(
            exit.result,
            Err(if panic_hook {
                JobError::Unavailable
            } else {
                JobError::Busy
            })
        );
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(worker.phase(), Phase::Failed);
        check_owner(&exit.owner, &identity, original_host);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let suppressed = ready_job.read(4096).unwrap().unwrap();
        assert!(suppressed.cancelled);
        assert!(suppressed.response.is_none());
        assert!(matches!(failed_job.read(4096), Err(JobError::Unavailable)));
    }
}

#[test]
fn preparation_failure_prevents_queued_store_mutation() {
    let mut f = Fixture::new();
    f.owner.fail_prepare = Some(1);
    let (_dir, _manager, mut worker) = f.start();
    let mut update = worker
        .update_service(ServiceUpdate::Outbound {
            value: record(),
            expected_revision: 0,
        })
        .unwrap();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Err(JobError::Busy));
    assert_eq!(exit.maintenance, Ok(()));
    assert!(
        exit.owner
            .host
            .store_local()
            .load_outbound_authority(&[7; 32])
            .unwrap()
            .is_none()
    );
    assert!(matches!(update.read(), Err(JobError::Unavailable)));
}

#[test]
fn maintenance_error_or_panic_is_separate_from_successful_committed_update() {
    for panic_hook in [false, true] {
        let mut f = Fixture::new();
        f.owner.fail_finish = true;
        f.owner.finish_panics = panic_hook;
        let identity = f.owner.identity.clone();
        let original_host = f.owner.host.binding();
        let (_dir, _manager, mut worker) = f.start();
        update(&worker);
        worker.stop();
        let exit = reclaim(&mut worker);
        assert_eq!(exit.result, Ok(()));
        assert_eq!(
            exit.maintenance,
            Err(if panic_hook {
                JobError::Unavailable
            } else {
                JobError::Busy
            })
        );
        check_owner(&exit.owner, &identity, original_host);
        assert_eq!(
            exit.owner
                .host
                .store_local()
                .load_outbound_authority(&[7; 32])
                .unwrap()
                .unwrap()
                .container(),
            record().container()
        );
    }
}

struct Gate {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl Router for Gate {
    fn route(&mut self, _: u32, bytes: &[u8]) -> Result<Vec<u8>, RouterFault> {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(WAIT).unwrap();
        Ok(Response::encode(
            &Request::decode(bytes).unwrap(),
            Status::Completed,
            b"late",
            0,
            true,
        )
        .unwrap())
    }
}
#[test]
fn dropped_worker_retains_owner_until_blocking_router_returns() {
    let f = Fixture::new();
    let drops = f.owner.drops.clone();
    let hooks = f.owner.hooks.clone();
    let (_dir, _manager, worker) = f.start();
    let (entered, started) = mpsc::channel();
    let (release, finished) = mpsc::channel();
    let mut job = worker
        .submit(
            input(),
            Box::new(Gate {
                entered,
                release: finished,
            }),
            WAIT,
        )
        .unwrap();
    started.recv_timeout(WAIT).unwrap();
    let before = Instant::now();
    drop(worker);
    assert!(before.elapsed() < Duration::from_secs(1));
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    let end = Instant::now() + WAIT;
    while drops.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < end);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(
        hooks
            .lock()
            .unwrap()
            .iter()
            .filter(|(name, _)| *name == "finish")
            .count(),
        1
    );
    let report = job.read(4096).unwrap().unwrap();
    assert!(report.cancelled);
    assert!(report.response.is_none());
}

struct Panics;
impl Router for Panics {
    fn route(&mut self, _: u32, _: &[u8]) -> Result<Vec<u8>, RouterFault> {
        panic!("injected router panic")
    }
}
#[test]
fn router_panic_still_returns_original_owner_after_disconnect_and_maintenance() {
    let f = Fixture::new();
    let identity = f.owner.identity.clone();
    let original_host = f.owner.host.binding();
    let (_dir, _manager, mut worker) = f.start();
    let _job = worker.submit(input(), Box::new(Panics), WAIT).unwrap();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    assert_eq!(exit.maintenance, Ok(()));
    check_owner(&exit.owner, &identity, original_host);
}

#[test]
fn failed_disconnect_preserves_exact_instance_for_explicit_cleanup() {
    let mut f = Fixture::new();
    let original = f.owner.host.binding();
    let binding = f.instance.connection().binding();
    f.owner.panic_runtime_mut = Some(1);
    let (_dir, _manager, mut worker) = f.start();
    worker.stop();
    let mut exit = reclaim(&mut worker);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    assert_eq!(exit.disconnect, Err(JobError::Unavailable));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(exit.owner.host.binding(), original);
    let instance = exit.instance.take().unwrap();
    assert_eq!(instance.connection().binding(), binding);
    instance.close(exit.owner.runtime_mut()).unwrap();
    assert!(exit.owner.host.binding_phase(binding).is_err());
}

#[test]
fn preparation_cannot_replace_host_for_store_mutation_or_disconnection() {
    let mut f = Fixture::new();
    let original = f.owner.host.binding();
    let mut alternate = HostRuntime::new(
        Store::open(&f._dir.path().join("alternate"), Default::default()).unwrap(),
    )
    .unwrap();
    let alternate_connection = alternate.connect().unwrap();
    let alternate_identity = alternate.binding();
    f.owner.swapped_host = Some(alternate);
    f.owner.swap_on_prepare = true;
    let (_dir, _manager, mut worker) = f.start();
    let mut update = worker
        .update_service(ServiceUpdate::Outbound {
            value: record(),
            expected_revision: 0,
        })
        .unwrap();
    let mut exit = reclaim(&mut worker);
    assert_eq!(exit.result, Err(JobError::InvalidOptions));
    assert_eq!(exit.disconnect, Err(JobError::InvalidOptions));
    assert_eq!(exit.maintenance, Err(JobError::InvalidOptions));
    assert!(
        !exit
            .owner
            .hooks
            .lock()
            .unwrap()
            .iter()
            .any(|(name, _)| *name == "finish")
    );
    assert!(matches!(update.read(), Err(JobError::Unavailable)));
    assert_eq!(exit.owner.host.binding(), alternate_identity);
    assert_eq!(
        exit.owner.host.connection_phase(&alternate_connection),
        Ok(morrow_core::lifecycle::InstancePhase::Ready)
    );
    assert!(
        exit.owner
            .host
            .store_local()
            .load_outbound_authority(&[7; 32])
            .unwrap()
            .is_none()
    );
    assert!(
        exit.owner
            .swapped_host
            .as_ref()
            .unwrap()
            .store_local()
            .load_outbound_authority(&[7; 32])
            .unwrap()
            .is_none()
    );
    std::mem::swap(
        &mut exit.owner.host,
        exit.owner.swapped_host.as_mut().unwrap(),
    );
    assert_eq!(exit.owner.host.binding(), original);
    let instance = exit.instance.take().unwrap();
    instance.close(&mut exit.owner.host).unwrap();
}

#[test]
fn maintenance_cannot_replace_owner_runtime_and_report_success() {
    let mut f = Fixture::new();
    let original = f.owner.host.binding();
    f.owner.swapped_host = Some(
        HostRuntime::new(
            Store::open(&f._dir.path().join("alternate"), Default::default()).unwrap(),
        )
        .unwrap(),
    );
    f.owner.swap_on_finish = true;
    let (_dir, _manager, mut worker) = f.start();
    worker.stop();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert!(exit.instance.is_none());
    assert_eq!(exit.maintenance, Err(JobError::InvalidOptions));
    assert_eq!(worker.phase(), Phase::Failed);
    assert_eq!(
        exit.owner.swapped_host.as_ref().unwrap().binding(),
        original
    );
    assert!(exit.owner.disconnected_at_finish);
}
