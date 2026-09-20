//! Renewal commands execute against the original Manager moved with the owner.
//! These are native scheduler tests, not a qualification of app UI routing.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]

use morrow_core::{
    dispatch::HostRuntime,
    io as wire,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    service::{self, Invocation, Reply},
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Limits,
    io_binding::{Error, IoBinding, ServiceRunBudget, ServiceRunSnapshot},
    io_jobs::{
        BrokerRouter, CommandOwner, HostOwner, IoWorker, JobError, JobLimits, ManagedHostOwner,
        OwnerCommandError, OwnerCommandHandle, OwnerCommandPoll, Poll, RouteContext, RouterFault,
        ServiceRunRenewalHandle,
    },
    manager::{ManagedInstance, Manager},
    service_io::{ListenerGrant, ServiceGrant},
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
const ID: &str = "org.example.owned.service.renewal";
const RUN_MS: u64 = 60_000;
const INITIAL_EXPIRES: u64 = 20_001;
const NEXT_EXPIRES: u64 = 40_001;
const MAX_BYTES: u64 = 65_536;
const WAIT: Duration = Duration::from_secs(10);

fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([
        IoCapability::FileRead,
        IoCapability::HttpListen,
        IoCapability::HttpPublish,
        IoCapability::HttpRequest,
    ])
}
fn budget(jobs: u64, bytes: u64) -> ServiceRunBudget {
    ServiceRunBudget {
        max_jobs: jobs,
        max_bytes: bytes,
    }
}
fn initial_budget() -> ServiceRunBudget {
    budget(2, 8192)
}
fn request() -> service::Request {
    service::Request::encode(
        1,
        &Invocation {
            service: "echo".into(),
            handler: "echo.call".into(),
            principal: "local-user".into(),
            method: "POST".into(),
            target: "/echo".into(),
            headers: vec![],
            body: b"hello".to_vec(),
        },
    )
    .unwrap()
}
fn reply() -> Reply {
    Reply {
        status: 200,
        headers: vec![],
        body: b"same-original-run".to_vec(),
    }
}
fn package(budgeted: bool, run_ms: u64) -> Package {
    let response = service::Response::encode(&request(), &reply()).unwrap();
    let literal: String = response
        .iter()
        .map(|byte| format!("\\{byte:02x}"))
        .collect();
    let wasm = wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
        (memory (export "memory") 3)
        (data (i32.const 131072) "{literal}")
        (func (export "morrow_run") (result i32)
            i32.const 0 i32.const 131072 call $read drop
            i32.const 131072 i32.const {} call $complete drop i32.const 0))"#,
        response.len()
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec!["echo.call".into()]);
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let limits = declaration.budget.as_mut().unwrap();
    limits.max_jobs = 2;
    limits.max_resources = 4;
    limits.max_job_bytes = 8192;
    limits.max_bytes = MAX_BYTES;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: run_ms,
        budget: budgeted.then_some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: 64,
            max_bytes: MAX_BYTES,
        }),
    });
    manifest
        .required_features
        .extend([io::FEATURE.into(), io::SERVICE_RUN_FEATURE.into()]);
    if budgeted {
        manifest
            .required_features
            .push(io::SERVICE_RUN_BUDGET_FEATURE.into());
    }
    manifest.io_declaration = Some(declaration);
    Package::build(manifest, &wasm).unwrap()
}

const BLOCK: u8 = 1;
const HIDE_MANAGER: u8 = 2;
const NOOP: u8 = 3;

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    fn wait(&self) {
        let guard = self.open.lock().unwrap();
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, WAIT, |v| !*v)
            .unwrap();
        assert!(*guard, "blocked owner handler was not released");
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
struct Owner {
    host: HostRuntime,
    manager: Manager,
    manager_visible: bool,
    gate: Arc<Gate>,
    events: mpsc::Sender<u8>,
    identity: Arc<()>,
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
        self.manager_visible.then_some(&self.manager)
    }
}
impl CommandOwner for Owner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        self.events.send(input[0]).unwrap();
        match input[0] {
            BLOCK => self.gate.wait(),
            HIDE_MANAGER => self.manager_visible = false,
            NOOP => {}
            _ => panic!("unknown fixture command"),
        }
        Ok(vec![])
    }
}
// This wrapper deliberately has no CommandOwner implementation: typed management
// must not require an application byte-command parser or an arbitrary callback API.
struct ManagementOnly(Owner);
impl HostOwner for ManagementOnly {
    fn runtime(&self) -> &HostRuntime {
        self.0.runtime()
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        self.0.runtime_mut()
    }
}
impl ManagedHostOwner for ManagementOnly {
    fn manager(&self) -> Option<&Manager> {
        self.0.manager()
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    owner: Owner,
    instance: ManagedInstance,
    binding: IoBinding,
    events: mpsc::Receiver<u8>,
}
impl Fixture {
    fn new(budgeted: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let package = package(budgeted, RUN_MS);
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
        let mut host =
            HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
                .unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = if budgeted {
            manager.bind_budgeted_service_run(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &caps(),
                INITIAL_EXPIRES,
                1,
                initial_budget(),
            )
        } else {
            manager.bind_service_run(
                &host,
                &instance,
                package.digest(),
                manager.revision(),
                &caps(),
                INITIAL_EXPIRES,
                1,
            )
        }
        .unwrap();
        let (events, receiver) = mpsc::channel();
        Self {
            dir,
            instance,
            binding,
            events: receiver,
            owner: Owner {
                host,
                manager,
                manager_visible: true,
                events,
                gate: Arc::new(Gate::default()),
                identity: Arc::new(()),
            },
        }
    }
    fn start<O: ManagedHostOwner>(self, wrap: impl FnOnce(Owner) -> O) -> Running<O> {
        let owner = self.owner;
        let revision = owner.manager.revision();
        let listener = ListenerGrant::issue(
            &owner.manager,
            &owner.host,
            &self.instance,
            &self.binding,
            1,
        )
        .unwrap();
        listener.activate().unwrap();
        let grant = ServiceGrant::issue(
            &owner.manager,
            &owner.host,
            &self.instance,
            &self.binding,
            "echo",
            "echo.call",
            1,
        )
        .unwrap()
        .bound_to_listener(&listener)
        .unwrap();
        let gate = owner.gate.clone();
        let identity = owner.identity.clone();
        let tick = Arc::new(AtomicU64::new(1));
        let caller_samples = Arc::new(AtomicUsize::new(0));
        let clock_tick = tick.clone();
        let clock_samples = caller_samples.clone();
        let caller = thread::current().id();
        let worker = IoWorker::spawn_managed_owner(
            wrap(owner),
            self.instance,
            self.binding,
            move || {
                if thread::current().id() == caller {
                    clock_samples.fetch_add(1, Ordering::SeqCst);
                }
                clock_tick.load(Ordering::SeqCst)
            },
            2,
            JobLimits::new(2, 8192, MAX_BYTES).unwrap(),
        )
        .unwrap();
        Running {
            _dir: self.dir,
            worker,
            grant,
            listener,
            revision,
            tick,
            caller_samples,
            gate,
            events: self.events,
            identity,
        }
    }
}
struct Running<O: ManagedHostOwner> {
    _dir: tempfile::TempDir,
    worker: IoWorker<O>,
    grant: ServiceGrant,
    listener: ListenerGrant,
    revision: u64,
    tick: Arc<AtomicU64>,
    caller_samples: Arc<AtomicUsize>,
    gate: Arc<Gate>,
    events: mpsc::Receiver<u8>,
    identity: Arc<()>,
}
impl<O: ManagedHostOwner> Running<O> {
    fn queue(
        &self,
        run_revision: u64,
        expires: u64,
        budget: ServiceRunBudget,
    ) -> ServiceRunRenewalHandle {
        self.worker
            .queue_service_run_renewal(
                self.grant.clone(),
                self.revision,
                run_revision,
                expires,
                budget,
            )
            .unwrap()
    }
    fn snapshot(&self) -> ServiceRunSnapshot {
        self.worker.service_run_snapshot().unwrap()
    }
    fn reclaim(&mut self) -> O {
        self.worker.stop();
        let end = Instant::now() + WAIT;
        loop {
            if let Some(exit) = self.worker.try_reclaim().unwrap() {
                exit.owner
                    .runtime()
                    .store_local()
                    .integrity_check()
                    .unwrap();
                return exit.owner;
            }
            assert!(Instant::now() < end, "original owner not reclaimed");
            thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Running<Owner> {
    fn new() -> Self {
        Fixture::new(true).start(|owner| owner)
    }
    fn command(&self, kind: u8) -> OwnerCommandHandle {
        self.worker.submit_owner_command(vec![kind], 0).unwrap()
    }
    fn block(&self) -> OwnerCommandHandle {
        let handle = self.command(BLOCK);
        // Wait for this particular handler; previous commands may have sent events.
        loop {
            if self.events.recv_timeout(WAIT).unwrap() == BLOCK {
                break;
            }
        }
        assert!(handle.is_started());
        handle
    }
}
fn wait_for(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < end, "bounded wait expired");
        thread::sleep(Duration::from_millis(1));
    }
}
fn renewal(
    handle: &mut ServiceRunRenewalHandle,
) -> Result<Result<ServiceRunSnapshot, Error>, OwnerCommandError> {
    wait_for(|| matches!(handle.poll(), OwnerCommandPoll::Ready));
    let result = handle
        .read()
        .map(|result| result.expect("ready renewal has result"));
    assert_eq!(handle.poll(), OwnerCommandPoll::Consumed);
    assert_eq!(handle.read(), Err(OwnerCommandError::Consumed));
    result
}
fn command(handle: &mut OwnerCommandHandle) -> Result<Vec<u8>, OwnerCommandError> {
    wait_for(|| matches!(handle.poll(), OwnerCommandPoll::Ready));
    handle
        .read()
        .map(|reply| reply.expect("ready command has result"))
}
struct NoIo;
impl BrokerRouter for NoIo {
    fn route(
        &mut self,
        _: &mut RouteContext<'_>,
        _: u32,
        _: &wire::Request,
    ) -> Result<Vec<u8>, RouterFault> {
        panic!("zero-IO guest dispatched an unexpected host call")
    }
}
fn invoke<O: ManagedHostOwner>(run: &Running<O>) {
    let mut job = run
        .worker
        .submit_service(request(), run.grant.clone(), Box::new(NoIo), WAIT)
        .unwrap();
    wait_for(|| matches!(job.poll(), Poll::Ready));
    let report = job.read(8192).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Ok(0));
    assert!(report.service_response.unwrap() == reply());
}

#[test]
fn original_manager_moves_with_owner_and_renewal_keeps_grants_and_usage() {
    let mut run = Running::new();
    let old_grant = run.grant.clone();
    let old_listener = run.listener.clone();
    invoke(&run);
    let before = run.snapshot();
    assert_eq!(before.usage.jobs, 1);
    assert!(before.usage.bytes > 0);
    let result = renewal(&mut run.queue(1, NEXT_EXPIRES, budget(4, 16_384)))
        .unwrap()
        .unwrap();
    assert_eq!(result.revision, 2);
    assert_eq!(result.expires, NEXT_EXPIRES);
    assert_eq!(result.usage, before.usage);
    run.tick.store(INITIAL_EXPIRES + 1, Ordering::SeqCst);
    run.worker.check_service(&old_grant).unwrap();
    run.worker.check_listener(&old_listener).unwrap();
    invoke(&run);
    assert_eq!(run.snapshot().usage.jobs, 2);
    assert_eq!(run.snapshot().usage.bytes, before.usage.bytes * 2);
    let owner = run.reclaim();
    assert!(Arc::ptr_eq(&owner.identity, &run.identity));
    assert_eq!(owner.manager.revision(), run.revision);
}

#[test]
fn typed_renewal_does_not_require_generic_command_owner() {
    let mut run = Fixture::new(true).start(ManagementOnly);
    let renewed = renewal(&mut run.queue(1, NEXT_EXPIRES, initial_budget()))
        .unwrap()
        .unwrap();
    assert_eq!(renewed.revision, 2);
    run.tick.store(INITIAL_EXPIRES + 1, Ordering::SeqCst);
    invoke(&run);
    let owner = run.reclaim();
    assert!(Arc::ptr_eq(&owner.0.identity, &run.identity));
}

#[test]
fn semantic_rejections_are_definitive_results_and_do_not_change_the_run() {
    let mut run = Running::new();
    let before = run.snapshot();
    for (revision, expires, budget) in [
        (0, NEXT_EXPIRES, initial_budget()),
        (2, NEXT_EXPIRES, initial_budget()),
        (1, INITIAL_EXPIRES, initial_budget()),
        (1, INITIAL_EXPIRES - 1, budget(3, 8192)),
        (1, NEXT_EXPIRES, budget(1, 8192)),
        (1, NEXT_EXPIRES, budget(2, 8191)),
        (1, RUN_MS + 2, initial_budget()),
        (1, NEXT_EXPIRES, budget(65, 8192)),
        (1, NEXT_EXPIRES, budget(2, MAX_BYTES + 1)),
    ] {
        let error = renewal(&mut run.queue(revision, expires, budget))
            .unwrap()
            .unwrap_err();
        assert!(matches!(error, Error::Denied | Error::Limit));
        assert_eq!(run.snapshot(), before);
    }
    let mut stale_registry = run
        .worker
        .queue_service_run_renewal(
            run.grant.clone(),
            run.revision - 1,
            1,
            NEXT_EXPIRES,
            initial_budget(),
        )
        .unwrap();
    assert_eq!(renewal(&mut stale_registry), Ok(Err(Error::Denied)));
    assert_eq!(run.snapshot(), before);
    run.reclaim();
}

#[test]
fn foreign_grant_is_rejected_before_sampling_the_callers_clock() {
    let mut run = Running::new();
    let mut foreign = Running::new();
    let before = run.snapshot();
    let samples = run.caller_samples.load(Ordering::SeqCst);
    assert!(matches!(
        run.worker.queue_service_run_renewal(
            foreign.grant.clone(),
            run.revision,
            1,
            NEXT_EXPIRES,
            initial_budget(),
        ),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(run.caller_samples.load(Ordering::SeqCst), samples);
    assert_eq!(run.snapshot(), before);
    run.reclaim();
    foreign.reclaim();
}

#[test]
fn cancelled_queued_renewal_never_executes_or_changes_the_snapshot() {
    let mut run = Running::new();
    let mut blocker = run.block();
    let before = run.snapshot();
    let mut queued = run.queue(1, NEXT_EXPIRES, initial_budget());
    assert!(!queued.is_started());
    queued.cancel();
    assert_eq!(renewal(&mut queued), Err(OwnerCommandError::Cancelled));
    run.gate.release();
    command(&mut blocker).unwrap();
    // The FIFO barrier proves the cancelled envelope has actually been dequeued.
    command(&mut run.command(NOOP)).unwrap();
    assert_eq!(run.snapshot(), before);
    run.reclaim();
}

#[test]
fn queued_renewal_cannot_restore_expired_or_revoked_authority() {
    for revoke in [false, true] {
        let mut run = Running::new();
        let mut blocker = run.block();
        let before = run.snapshot();
        let mut queued = run.queue(1, NEXT_EXPIRES, initial_budget());
        if revoke {
            run.grant.revoke();
        } else {
            run.tick.store(INITIAL_EXPIRES, Ordering::SeqCst);
        }
        run.gate.release();
        let result = renewal(&mut queued);
        assert!(
            matches!(
                result,
                Ok(Err(Error::Denied | Error::Expired)) | Err(OwnerCommandError::Closed)
            ),
            "expired/revoked renewal unexpectedly succeeded: {result:?}"
        );
        let _ = command(&mut blocker);
        assert_eq!(run.snapshot(), before);
        assert!(run.worker.check_service(&run.grant).is_err());
        run.reclaim();
    }
}

#[test]
fn ready_generic_commands_and_renewals_share_the_same_eight_reservations() {
    let mut run = Running::new();
    let mut held: Vec<_> = (0..8)
        .map(|_| {
            let handle = run.command(NOOP);
            wait_for(|| matches!(handle.poll(), OwnerCommandPoll::Ready));
            handle
        })
        .collect();
    assert!(matches!(
        run.worker.queue_service_run_renewal(
            run.grant.clone(),
            run.revision,
            1,
            NEXT_EXPIRES,
            initial_budget(),
        ),
        Err(OwnerCommandError::Busy)
    ));
    command(&mut held.pop().unwrap()).unwrap();
    let mut renewal_handle = run.queue(1, NEXT_EXPIRES, initial_budget());
    wait_for(|| matches!(renewal_handle.poll(), OwnerCommandPoll::Ready));
    assert!(matches!(
        run.worker.submit_owner_command(vec![NOOP], 0),
        Err(OwnerCommandError::Busy)
    ));
    renewal(&mut renewal_handle).unwrap().unwrap();
    for mut handle in held {
        command(&mut handle).unwrap();
    }
    run.reclaim();
}

#[test]
fn missing_internal_manager_returns_denied_without_reconstructing_authority() {
    let mut run = Running::new();
    command(&mut run.command(HIDE_MANAGER)).unwrap();
    let before = run.snapshot();
    assert_eq!(
        renewal(&mut run.queue(1, NEXT_EXPIRES, initial_budget())),
        Ok(Err(Error::Denied))
    );
    assert_eq!(run.snapshot(), before);
    // Retained Manager is still alive: hiding access did not manufacture expiry.
    run.worker.check_service(&run.grant).unwrap();
    let owner = run.reclaim();
    assert!(!owner.manager_visible);
    assert_eq!(owner.manager.revision(), run.revision);
}

#[test]
fn ready_renewal_suppressed_by_cancel_or_stop_reports_unknown_without_rollback() {
    for stop in [false, true] {
        let mut run = Running::new();
        let mut handle = run.queue(1, NEXT_EXPIRES, initial_budget());
        wait_for(|| matches!(handle.poll(), OwnerCommandPoll::Ready));
        assert!(handle.is_started());
        assert_eq!(run.snapshot().revision, 2);
        if stop {
            run.worker.stop();
        } else {
            handle.cancel();
        }
        assert_eq!(renewal(&mut handle), Err(OwnerCommandError::Unknown));
        assert_eq!(run.snapshot().revision, 2);
        assert_eq!(run.snapshot().expires, NEXT_EXPIRES);
        run.reclaim();
    }
}

#[test]
fn two_queued_compare_and_swap_requests_have_exactly_one_success() {
    let mut run = Running::new();
    let mut blocker = run.block();
    let mut first = run.queue(1, NEXT_EXPIRES, initial_budget());
    let mut second = run.queue(1, NEXT_EXPIRES, budget(3, 8192));
    assert!(!first.is_started() && !second.is_started());
    run.gate.release();
    command(&mut blocker).unwrap();
    assert_eq!(renewal(&mut first).unwrap().unwrap().revision, 2);
    assert_eq!(renewal(&mut second), Ok(Err(Error::Denied)));
    assert_eq!(run.snapshot().revision, 2);
    assert_eq!(run.snapshot().budget, initial_budget());
    run.reclaim();
}

#[test]
fn budgetless_profile_cannot_be_promoted_through_the_management_queue() {
    let mut run = Fixture::new(false).start(|owner| owner);
    assert!(run.worker.service_run_snapshot().is_none());
    assert_eq!(
        renewal(&mut run.queue(1, NEXT_EXPIRES, initial_budget())),
        Ok(Err(Error::Denied))
    );
    assert!(run.worker.service_run_snapshot().is_none());
    invoke(&run);
    run.reclaim();
}
