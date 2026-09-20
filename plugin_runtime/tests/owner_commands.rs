//! Trusted owner-command scheduling with a real Store and managed Wasm.
//! The business counter is synthetic; these tests do not qualify app UI routing.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]

use morrow_core::{
    dispatch::HostRuntime,
    io::{Request, Response, Status},
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
        CommandOwner, HostOwner, IoWorker, JobError, JobHandle, JobLimits, ManagedHostOwner,
        OwnerCommandError, OwnerCommandHandle, OwnerCommandPoll, Poll, Router, RouterFault,
        WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(10);
const ID: &str = "org.example.owner.commands";
const BLOCK: u8 = 0;
const WRITE: u8 = 1;
const ERROR: u8 = 2;
const OVERSIZE: u8 = 3;
const PANIC: u8 = 4;
const READ: u8 = 5;
const EMPTY: u8 = 6;
const SWAP: u8 = 7;

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    fn wait(&self) {
        let open = self.open.lock().unwrap();
        let (open, timeout) = self
            .changed
            .wait_timeout_while(open, WAIT, |v| !*v)
            .unwrap();
        assert!(
            *open && !timeout.timed_out(),
            "blocked test command was not released"
        );
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct Owner {
    host: HostRuntime,
    replacement: Option<HostRuntime>,
    identity: Arc<()>,
    gate: Arc<Gate>,
    events: mpsc::Sender<u8>,
    effects: usize,
    calls: Vec<u8>,
    expire_in_prepare: Option<Arc<AtomicU64>>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        if let Some(clock) = &self.expire_in_prepare {
            clock.store(100, Ordering::SeqCst);
        }
        Ok(())
    }
}
struct ManagedOwner {
    inner: Owner,
    manager: Option<Manager>,
}
impl HostOwner for ManagedOwner {
    fn runtime(&self) -> &HostRuntime {
        self.inner.runtime()
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        self.inner.runtime_mut()
    }
}
impl ManagedHostOwner for ManagedOwner {
    fn manager(&self) -> Option<&Manager> {
        self.manager.as_ref()
    }
}
impl CommandOwner for Owner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        let kind = input[0];
        self.calls.push(kind);
        self.events.send(kind).unwrap();
        if kind == BLOCK {
            self.gate.wait();
        }
        if kind != READ {
            self.effects += 1;
        }
        match kind {
            ERROR => Err(JobError::Unavailable),
            OVERSIZE => Ok(vec![0; 65]),
            PANIC => panic!("injected owner command panic after effect"),
            EMPTY => Ok(vec![]),
            SWAP => {
                std::mem::swap(&mut self.host, self.replacement.as_mut().unwrap());
                Ok(vec![])
            }
            _ => Ok(self.effects.to_le_bytes().to_vec()),
        }
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    owner: Owner,
    instance: ManagedInstance,
    binding: IoBinding,
    events: mpsc::Receiver<u8>,
}
struct Running {
    _dir: tempfile::TempDir,
    _manager: Manager,
    worker: IoWorker<Owner>,
    events: mpsc::Receiver<u8>,
    gate: Arc<Gate>,
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
        let (events, receiver) = mpsc::channel();
        Self {
            dir,
            manager,
            instance,
            binding,
            events: receiver,
            owner: Owner {
                host,
                replacement: None,
                identity: Arc::new(()),
                gate: Arc::new(Gate::default()),
                events,
                effects: 0,
                calls: vec![],
                expire_in_prepare: None,
            },
        }
    }
    fn start(self) -> Running {
        self.start_with_clock(|| 2)
    }
    fn start_with_clock(self, clock: impl FnMut() -> u64 + Send + 'static) -> Running {
        let gate = self.owner.gate.clone();
        let worker = IoWorker::spawn_managed_owned(
            &self.manager,
            self.owner,
            self.instance,
            self.binding,
            clock,
            2,
            JobLimits::default(),
        )
        .unwrap();
        Running {
            _dir: self.dir,
            _manager: self.manager,
            worker,
            events: self.events,
            gate,
        }
    }
}
impl Running {
    fn command(&self, kind: u8) -> OwnerCommandHandle {
        self.worker.submit_owner_command(vec![kind], 64).unwrap()
    }
    fn entered(&self, kind: u8) {
        assert_eq!(self.events.recv_timeout(WAIT).unwrap(), kind);
    }
    fn stop_reclaim(&mut self) -> WorkerExit<Owner> {
        self.gate.release();
        self.worker.stop();
        reclaim(&mut self.worker)
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.gate.release();
        self.worker.stop();
    }
}
fn wait_for(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !predicate() {
        assert!(Instant::now() < end, "bounded wait expired");
        thread::sleep(Duration::from_millis(1));
    }
}
fn ready(handle: &OwnerCommandHandle) {
    wait_for(|| matches!(handle.poll(), OwnerCommandPoll::Ready));
}
fn result(handle: &mut OwnerCommandHandle) -> Result<Vec<u8>, OwnerCommandError> {
    let end = Instant::now() + WAIT;
    loop {
        match handle.read() {
            Ok(None) => {
                assert!(Instant::now() < end, "command result wait expired");
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Some(value)) => return Ok(value),
            Err(error) => return Err(error),
        }
    }
}
fn reclaim(worker: &mut IoWorker<Owner>) -> WorkerExit<Owner> {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < end, "owner reclamation wait expired");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn original_owner_executes_writes_and_reads_and_is_recovered_once() {
    let fixture = Fixture::new();
    let identity = fixture.owner.identity.clone();
    let binding = fixture.owner.host.binding();
    let mut run = fixture.start();
    let mut write = run.command(WRITE);
    assert_eq!(result(&mut write).unwrap(), 1_usize.to_le_bytes());
    assert!(write.is_started());
    assert_eq!(write.read(), Err(OwnerCommandError::Consumed));
    let mut read = run.command(READ);
    assert_eq!(result(&mut read).unwrap(), 1_usize.to_le_bytes());
    let exit = run.stop_reclaim();
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert!(Arc::ptr_eq(&exit.owner.identity, &identity));
    assert_eq!(exit.owner.host.binding(), binding);
    assert_eq!(exit.owner.effects, 1);
    assert_eq!(exit.owner.calls, [WRITE, READ]);
    assert!(matches!(run.worker.try_reclaim(), Err(JobError::Consumed)));
}

#[test]
fn missing_or_foreign_owned_manager_is_rejected_before_clock_and_returns_original_objects() {
    for foreign in [false, true] {
        let fixture = Fixture::new();
        let other = Fixture::new();
        let identity = fixture.owner.identity.clone();
        let host_binding = fixture.owner.host.binding();
        let connection_binding = fixture.instance.connection().binding();
        let samples = Arc::new(AtomicUsize::new(0));
        let sampled = samples.clone();
        let owned = ManagedOwner {
            inner: fixture.owner,
            manager: if foreign { Some(other.manager) } else { None },
        };
        let mut failure = match IoWorker::spawn_managed_owner(
            owned,
            fixture.instance,
            fixture.binding,
            move || {
                sampled.fetch_add(1, Ordering::SeqCst);
                2
            },
            2,
            JobLimits::default(),
        ) {
            Ok(_) => panic!("admitted missing or unrelated owner manager"),
            Err(failure) => failure,
        };
        assert_eq!(failure.error, JobError::InvalidOptions);
        assert_eq!(samples.load(Ordering::SeqCst), 0);
        assert!(Arc::ptr_eq(&failure.owner.inner.identity, &identity));
        assert_eq!(failure.owner.inner.host.binding(), host_binding);
        assert_eq!(failure.owner.inner.effects, 0);
        assert!(failure.owner.inner.calls.is_empty());
        assert_eq!(failure.owner.manager.is_some(), foreign);
        let instance = failure.instance.take().unwrap();
        assert_eq!(instance.connection().binding(), connection_binding);
        instance.close(&mut failure.owner.inner.host).unwrap();
        assert!(
            failure
                .owner
                .inner
                .host
                .binding_phase(connection_binding)
                .is_err()
        );
    }
}

#[test]
fn expiry_during_prepare_prevents_start_and_recovers_owner_without_business_effect() {
    let clock = Arc::new(AtomicU64::new(2));
    let mut fixture = Fixture::new();
    let identity = fixture.owner.identity.clone();
    let binding = fixture.owner.host.binding();
    fixture.owner.expire_in_prepare = Some(clock.clone());
    let sampled = clock.clone();
    let mut run = fixture.start_with_clock(move || sampled.load(Ordering::SeqCst));
    let mut command = run.command(WRITE);
    assert_eq!(result(&mut command), Err(OwnerCommandError::Closed));
    assert!(!command.is_started());
    assert_eq!(clock.load(Ordering::SeqCst), 100);
    let exit = reclaim(&mut run.worker);
    assert!(Arc::ptr_eq(&exit.owner.identity, &identity));
    assert_eq!(exit.owner.host.binding(), binding);
    assert!(exit.owner.calls.is_empty());
    assert_eq!(exit.owner.effects, 0);
    assert_eq!(run.worker.owner_command_usage(), (0, 0));
}

#[test]
fn expired_ready_payload_is_suppressed_while_blocked_handler_prevents_monitor_tick() {
    let clock = Arc::new(AtomicU64::new(2));
    let fixture = Fixture::new();
    let identity = fixture.owner.identity.clone();
    let binding = fixture.owner.host.binding();
    let sampled = clock.clone();
    let mut run = fixture.start_with_clock(move || sampled.load(Ordering::SeqCst));
    let mut first = run.command(WRITE);
    ready(&first);
    run.entered(WRITE);
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    assert!(blocker.is_started());
    assert_eq!(run.worker.owner_command_usage(), (2, 130));

    // The worker is inside the gated application command and cannot run its
    // monitor. Reading must sample the same authority clock independently.
    clock.store(100, Ordering::SeqCst);
    assert_eq!(first.read(), Err(OwnerCommandError::Unknown));
    assert_eq!(run.worker.owner_command_usage(), (1, 65));
    run.gate.release();
    assert_eq!(result(&mut blocker), Err(OwnerCommandError::Unknown));
    let exit = reclaim(&mut run.worker);
    assert!(Arc::ptr_eq(&exit.owner.identity, &identity));
    assert_eq!(exit.owner.host.binding(), binding);
    assert_eq!(exit.owner.calls, [WRITE, BLOCK]);
    assert_eq!(exit.owner.effects, 2);
    assert_eq!(run.worker.owner_command_usage(), (0, 0));
}

#[test]
fn eight_ready_unread_commands_retain_count_and_reserved_bytes_until_consumed() {
    let mut run = Fixture::new().start();
    let mut handles = Vec::new();
    for _ in 0..8 {
        let handle = run.command(WRITE);
        ready(&handle);
        handles.push(handle);
    }
    assert_eq!(run.worker.owner_command_usage(), (8, 8 * 65));
    assert!(matches!(
        run.worker.submit_owner_command(vec![WRITE], 64),
        Err(OwnerCommandError::Busy)
    ));
    assert_eq!(result(&mut handles[0]).unwrap(), 1_usize.to_le_bytes());
    wait_for(|| run.worker.owner_command_usage() == (7, 7 * 65));
    let mut ninth = run.command(WRITE);
    assert_eq!(result(&mut ninth).unwrap(), 9_usize.to_le_bytes());
    drop(handles);
    wait_for(|| run.worker.owner_command_usage() == (0, 0));
    assert_eq!(run.stop_reclaim().owner.effects, 9);
}

#[test]
fn invalid_bounds_have_no_effect_or_reservation_and_zero_reply_supports_empty_result() {
    let mut run = Fixture::new().start();
    for (input, reply) in [
        (vec![WRITE; 65_537], 64),
        (vec![WRITE], 1_048_577),
        (vec![WRITE], usize::MAX),
    ] {
        assert!(matches!(
            run.worker.submit_owner_command(input, reply),
            Err(OwnerCommandError::Limit)
        ));
        assert_eq!(run.worker.owner_command_usage(), (0, 0));
    }
    let mut empty = run.worker.submit_owner_command(vec![EMPTY], 0).unwrap();
    assert_eq!(result(&mut empty).unwrap(), Vec::<u8>::new());
    let exit = run.stop_reclaim();
    assert_eq!(exit.owner.calls, [EMPTY]);
    assert_eq!(exit.owner.effects, 1);
}

#[test]
fn exact_input_and_reply_limits_are_accepted_and_fully_reserved() {
    let mut run = Fixture::new().start();
    let mut input = vec![0; 65_536];
    input[0] = BLOCK;
    let mut command = run.worker.submit_owner_command(input, 1_048_576).unwrap();
    run.entered(BLOCK);
    assert_eq!(run.worker.owner_command_usage(), (1, 65_536 + 1_048_576));
    run.gate.release();
    ready(&command);
    assert_eq!(run.worker.owner_command_usage(), (1, 65_536 + 1_048_576));
    assert_eq!(result(&mut command).unwrap(), 1_usize.to_le_bytes());
    wait_for(|| run.worker.owner_command_usage() == (0, 0));
    assert_eq!(run.stop_reclaim().owner.calls, [BLOCK]);
}

#[test]
fn queued_cancellation_never_enters_handler() {
    let mut run = Fixture::new().start();
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    let mut cancelled = run.command(WRITE);
    assert!(!cancelled.is_started());
    cancelled.cancel();
    assert_eq!(result(&mut cancelled), Err(OwnerCommandError::Cancelled));
    run.gate.release();
    assert!(result(&mut blocker).is_ok());
    let mut barrier = run.command(READ);
    assert_eq!(result(&mut barrier).unwrap(), 1_usize.to_le_bytes());
    assert_eq!(run.stop_reclaim().owner.calls, [BLOCK, READ]);
}

#[test]
fn running_cancellation_reports_unknown_and_retains_reservation_until_handler_returns() {
    let mut run = Fixture::new().start();
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    assert!(blocker.is_started());
    blocker.cancel();
    assert_eq!(result(&mut blocker), Err(OwnerCommandError::Unknown));
    assert_eq!(run.worker.owner_command_usage(), (1, 65));
    run.gate.release();
    wait_for(|| run.worker.owner_command_usage() == (0, 0));
    let exit = run.stop_reclaim();
    assert_eq!(exit.owner.effects, 1);
    assert_eq!(exit.owner.calls, [BLOCK]);
}

#[test]
fn dropping_running_handle_cannot_refund_its_reservation_while_effect_is_running() {
    let mut run = Fixture::new().start();
    let blocker = run.command(BLOCK);
    run.entered(BLOCK);
    drop(blocker);
    assert_eq!(run.worker.owner_command_usage(), (1, 65));
    let queued: Vec<_> = (0..7).map(|_| run.command(WRITE)).collect();
    assert_eq!(run.worker.owner_command_usage(), (8, 8 * 65));
    assert!(matches!(
        run.worker.submit_owner_command(vec![WRITE], 64),
        Err(OwnerCommandError::Busy)
    ));
    for command in &queued {
        command.cancel();
    }
    drop(queued);
    run.gate.release();
    wait_for(|| run.worker.owner_command_usage() == (0, 0));
    assert_eq!(run.stop_reclaim().owner.calls, [BLOCK]);
}

struct Script;
impl Router for Script {
    fn route(&mut self, _: u32, bytes: &[u8]) -> Result<Vec<u8>, RouterFault> {
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
struct EventRouter(mpsc::Sender<u8>);
impl Router for EventRouter {
    fn route(&mut self, call: u32, bytes: &[u8]) -> Result<Vec<u8>, RouterFault> {
        self.0.send(8).unwrap();
        Script.route(call, bytes)
    }
}
fn io_ready(handle: &mut JobHandle) {
    wait_for(|| match handle.poll() {
        Poll::Ready => true,
        Poll::Pending => false,
        other => panic!("unexpected IO job state {other:?}"),
    });
}
#[test]
fn io_ready_queue_saturation_does_not_consume_reserved_owner_command_capacity() {
    let mut run = Fixture::new().start();
    let input = || {
        Request::encode_read(1, &[5; 32], 0, 64)
            .unwrap()
            .bytes()
            .to_vec()
    };
    let mut jobs = Vec::new();
    for _ in 0..2 {
        let mut job = run.worker.submit(input(), Box::new(Script), WAIT).unwrap();
        io_ready(&mut job);
        jobs.push(job);
    }
    assert!(matches!(
        run.worker.submit(input(), Box::new(Script), WAIT),
        Err(JobError::Busy)
    ));
    let mut command = run.command(WRITE);
    assert_eq!(result(&mut command).unwrap(), 1_usize.to_le_bytes());
    for mut job in jobs {
        assert_eq!(
            job.read(4096).unwrap().unwrap().response.unwrap().payload,
            b"scripted"
        );
    }
    assert_eq!(run.stop_reclaim().owner.effects, 1);
}

#[test]
fn runnable_guest_gets_a_turn_before_owner_queue_is_exhausted() {
    let fixture = Fixture::new();
    let events = fixture.owner.events.clone();
    let mut run = fixture.start();
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    let mut queued: Vec<_> = (0..3).map(|_| run.command(WRITE)).collect();
    let input = Request::encode_read(1, &[5; 32], 0, 64)
        .unwrap()
        .bytes()
        .to_vec();
    let mut job = run
        .worker
        .submit(input, Box::new(EventRouter(events)), WAIT)
        .unwrap();

    // Both lanes are now runnable behind the first owner command. The shared
    // event stream proves the next guest turn precedes the next owner effect.
    run.gate.release();
    run.entered(8);
    for _ in 0..3 {
        run.entered(WRITE);
    }
    assert_eq!(result(&mut blocker).unwrap(), 1_usize.to_le_bytes());
    for (index, command) in queued.iter_mut().enumerate() {
        assert_eq!(result(command).unwrap(), (index + 2).to_le_bytes());
    }
    io_ready(&mut job);
    assert_eq!(
        job.read(4096).unwrap().unwrap().response.unwrap().payload,
        b"scripted"
    );
    let exit = run.stop_reclaim();
    assert_eq!(exit.owner.calls, [BLOCK, WRITE, WRITE, WRITE]);
    assert_eq!(exit.owner.effects, 4);
}

#[test]
fn stop_or_drain_rejects_queued_commands_and_suppresses_running_output() {
    for drain in [false, true] {
        let mut run = Fixture::new().start();
        let mut blocker = run.command(BLOCK);
        run.entered(BLOCK);
        let mut queued = run.command(WRITE);
        if drain {
            run.worker.drain(WAIT).unwrap();
        } else {
            run.worker.stop();
        }
        assert!(matches!(
            run.worker.submit_owner_command(vec![WRITE], 64),
            Err(OwnerCommandError::Closed)
        ));
        assert_eq!(result(&mut queued), Err(OwnerCommandError::Closed));
        run.gate.release();
        assert_eq!(result(&mut blocker), Err(OwnerCommandError::Unknown));
        let exit = reclaim(&mut run.worker);
        assert_eq!(exit.owner.calls, [BLOCK]);
        assert_eq!(exit.owner.effects, 1);
    }
}

#[test]
fn handler_error_or_oversize_after_effect_reports_unknown_without_automatic_replay() {
    for kind in [ERROR, OVERSIZE] {
        let mut run = Fixture::new().start();
        let mut handle = run.command(kind);
        assert_eq!(result(&mut handle), Err(OwnerCommandError::Unknown));
        assert!(handle.is_started());
        assert_eq!(handle.read(), Err(OwnerCommandError::Consumed));
        let mut read = run.command(READ);
        assert_eq!(result(&mut read).unwrap(), 1_usize.to_le_bytes());
        assert_eq!(run.stop_reclaim().owner.calls, [kind, READ]);
    }
}

#[test]
fn panicking_command_recovers_original_owner_and_never_runs_queued_command() {
    let fixture = Fixture::new();
    let identity = fixture.owner.identity.clone();
    let binding = fixture.owner.host.binding();
    let mut run = fixture.start();
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    let mut panic = run.command(PANIC);
    let mut queued = run.command(WRITE);
    run.gate.release();
    ready(&blocker);
    let exit = reclaim(&mut run.worker);
    assert!(exit.result.is_err());
    assert!(Arc::ptr_eq(&exit.owner.identity, &identity));
    assert_eq!(exit.owner.host.binding(), binding);
    assert_eq!(exit.owner.calls, [BLOCK, PANIC]);
    assert_eq!(exit.owner.effects, 2);
    assert_eq!(result(&mut panic), Err(OwnerCommandError::Unknown));
    assert_eq!(result(&mut queued), Err(OwnerCommandError::Closed));
    assert_eq!(result(&mut blocker), Err(OwnerCommandError::Unknown));
}

#[test]
fn substituted_runtime_is_detected_and_queue_does_not_continue() {
    let mut fixture = Fixture::new();
    let original = fixture.owner.host.binding();
    fixture.owner.replacement = Some(
        HostRuntime::new(
            Store::open(&fixture.dir.path().join("replacement"), Default::default()).unwrap(),
        )
        .unwrap(),
    );
    let mut run = fixture.start();
    let mut blocker = run.command(BLOCK);
    run.entered(BLOCK);
    let mut swap = run.command(SWAP);
    let mut queued = run.command(WRITE);
    run.gate.release();
    ready(&blocker);
    let exit = reclaim(&mut run.worker);
    assert!(exit.result.is_err());
    assert_eq!(exit.owner.replacement.as_ref().unwrap().binding(), original);
    assert_eq!(exit.owner.calls, [BLOCK, SWAP]);
    assert_eq!(result(&mut swap), Err(OwnerCommandError::Unknown));
    assert_eq!(result(&mut queued), Err(OwnerCommandError::Closed));
    assert_eq!(result(&mut blocker), Err(OwnerCommandError::Unknown));
}
