//! Bounded IO job executor tests with scripted hosts only. No real network,
//! file or credential effect is performed; a routed call is never retried.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::{Connection, HostRuntime},
    io::{Request, Response, Status},
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
    store::{EventBudget, Store},
};
use morrow_plugin_runtime::{
    Fault, Limits,
    io_jobs::{IoWorker, JobError, JobLimits, JobReport, Phase, Poll, Router, RouterFault},
    package::PreparedPackage,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.io.jobs";
const WAIT: Duration = Duration::from_secs(10);

fn request_frame() -> Vec<u8> {
    Request::encode_read(1, &[5; 32], 0, 64)
        .unwrap()
        .bytes()
        .to_vec()
}
/// The task reads its complete input frame, then routes it through IO `calls`
/// times and completes with the last brokered response.
fn module(calls: usize) -> Vec<u8> {
    let mut body = String::from("i32.const 0 i32.const 131072 call $read local.set $n");
    for _ in 1..calls {
        body.push_str(" i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $io drop");
    }
    body.push_str(
        " i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $io call $done drop i32.const 0",
    );
    wat::parse_str(format!(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32) (local $n i32) {body}))"#
    ))
    .unwrap()
}
fn package(calls: usize) -> Package {
    let wasm = module(calls);
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    manifest.io_declaration = Some(io::declaration(
        vec![IoCapability::HttpRequest],
        vec!["api.invoke".into()],
    ));
    Package::build(manifest, &wasm).unwrap()
}
fn setup(dir: &tempfile::TempDir, calls: usize) -> (PreparedPackage, HostRuntime, Connection) {
    let p = PreparedPackage::new(package(calls), Limits::default()).unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let c = p.connect(&mut host).unwrap();
    (p, host, c)
}
/// Replays scripted payloads as valid IO response frames, in call order.
struct Script {
    payloads: Vec<Vec<u8>>,
    calls: Arc<AtomicU32>,
}
impl Router for Script {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, self.calls.load(Ordering::SeqCst));
        self.calls.fetch_add(1, Ordering::SeqCst);
        let request = Request::decode(request).map_err(|_| RouterFault::Denied)?;
        let payload = self
            .payloads
            .get(call as usize)
            .ok_or(RouterFault::Limit)?
            .clone();
        Response::encode(&request, Status::Completed, &payload, 0, true)
            .map_err(|_| RouterFault::Limit)
    }
}
/// Blocks the executor inside its first routed call until released.
struct Gate {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    calls: u32,
}
impl Router for Gate {
    fn route(&mut self, call: u32, request: &[u8]) -> Result<Vec<u8>, RouterFault> {
        assert_eq!(call, 0);
        self.calls += 1;
        let request = Request::decode(request).map_err(|_| RouterFault::Denied)?;
        self.entered.send(()).unwrap();
        self.release
            .recv_timeout(WAIT)
            .map_err(|_| RouterFault::Unknown)?;
        Response::encode(&request, Status::Completed, b"gated", 0, true)
            .map_err(|_| RouterFault::Limit)
    }
}
fn wait_ready(handle: &mut morrow_plugin_runtime::io_jobs::JobHandle) -> JobReport {
    let until = Instant::now() + WAIT;
    loop {
        match handle.read(4096) {
            Ok(Some(report)) => return report,
            Ok(None) => {
                assert!(Instant::now() < until, "job never completed");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("read: {error:?}"),
        }
    }
}
fn finish(worker: &mut IoWorker) -> HostRuntime {
    let until = Instant::now() + WAIT;
    loop {
        match worker.try_finish() {
            Ok(Some(host)) => return host,
            Ok(None) => {
                assert!(Instant::now() < until, "worker never terminated");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("finish: {error:?}"),
        }
    }
}
fn simple(payloads: &[&[u8]]) -> (Arc<AtomicU32>, Box<Script>) {
    let calls = Arc::new(AtomicU32::new(0));
    let script = Box::new(Script {
        payloads: payloads.iter().map(|p| p.to_vec()).collect(),
        calls: Arc::clone(&calls),
    });
    (calls, script)
}

#[test]
fn multiple_calls_route_in_order_and_read_is_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 3);
    let (calls, script) = simple(&[b"one", b"two", b"three"]);
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = w.submit(request_frame(), script, WAIT).unwrap();
    let report = wait_ready(&mut handle);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(report.calls, 3);
    assert!(report.bytes > 0);
    let response = report.response.as_ref().unwrap();
    assert_eq!(response.payload, b"three");
    assert_eq!(response.status, Status::Completed);
    assert!(!report.cancelled && !report.unknown);
    assert!(report.task.execution.outcome.is_ok());
    assert!(matches!(handle.read(0), Err(JobError::Consumed)));
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn poll_is_nonblocking_and_read_honours_its_byte_bound() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (_calls, script) = simple(&[b"payload"]);
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = w.submit(request_frame(), script, WAIT).unwrap();
    loop {
        match handle.poll() {
            Poll::Pending => thread::sleep(Duration::from_millis(1)),
            Poll::Ready => break,
            other => panic!("poll: {other:?}"),
        }
    }
    // Poll is non-consuming; a too-small read leaves the result available.
    assert!(matches!(handle.read(0), Err(JobError::ReadBound)));
    assert_eq!(handle.poll(), Poll::Ready);
    let report = handle.read(4096).unwrap().unwrap();
    assert_eq!(report.response.unwrap().payload, b"payload");
    assert_eq!(handle.poll(), Poll::Consumed);
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn queue_capacity_and_byte_limits_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut first = w
        .submit(
            request_frame(),
            Box::new(Gate {
                entered,
                release: gate,
                calls: 0,
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let (_calls, second_script) = simple(&[b"second"]);
    let mut second = w.submit(request_frame(), second_script, WAIT).unwrap();
    let (_calls, third_script) = simple(&[b"third"]);
    assert!(matches!(
        w.submit(request_frame(), third_script, WAIT),
        Err(JobError::Busy)
    ));
    release.send(()).unwrap();
    assert!(wait_ready(&mut first).task.execution.outcome.is_ok());
    assert!(wait_ready(&mut second).task.execution.outcome.is_ok());
    // Cumulative bytes are never refunded by releasing jobs.
    assert!(w.bytes() > 0);
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();

    // A byte budget smaller than the input frame refuses the job up front.
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let limits = JobLimits::new(1, 64, 64).unwrap();
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, limits).unwrap();
    let (_calls, script) = simple(&[b"x"]);
    assert!(matches!(
        w.submit(request_frame(), script, WAIT),
        Err(JobError::Limit)
    ));
    // Malformed input is refused before any admission.
    let (_calls, script) = simple(&[b"x"]);
    assert!(matches!(
        w.submit(Vec::new(), script, WAIT),
        Err(JobError::InvalidOptions)
    ));
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn a_call_that_exceeds_the_job_budget_is_denied_before_any_effect() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 2);
    let calls = Arc::new(AtomicU32::new(0));
    let script = Box::new(Script {
        payloads: vec![b"a".to_vec()],
        calls: Arc::clone(&calls),
    });
    // The input frame fits the total budget, but the first call's request
    // charge does not fit the per-job byte allowance.
    let limits = JobLimits::new(4, request_frame().len() as u64 + 1, 1024 * 1024).unwrap();
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, limits).unwrap();
    let mut handle = w.submit(request_frame(), script, WAIT).unwrap();
    let report = wait_ready(&mut handle);
    // The request byte cap is checked before the router can produce an effect.
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(report.calls, 0);
    assert_eq!(report.task.execution.outcome, Err(Fault::Limits));
    assert!(report.response.is_none());
    assert!(!report.unknown && !report.cancelled);
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn deadline_suppresses_a_late_result_without_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = w
        .submit(
            request_frame(),
            Box::new(Gate {
                entered,
                release: gate,
                calls: 0,
            }),
            Duration::from_millis(50),
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    thread::sleep(Duration::from_millis(80));
    release.send(()).unwrap();
    let report = wait_ready(&mut handle);
    assert!(report.cancelled);
    assert_eq!(report.task.execution.outcome, Err(Fault::Deadline));
    assert!(report.response.is_none());
    w.stop();
    let host = finish(&mut w);
    host.store_local().integrity_check().unwrap();
}

#[test]
fn stop_revokes_recycles_and_never_delivers_a_late_success() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut w = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = w
        .submit(
            request_frame(),
            Box::new(Gate {
                entered,
                release: gate,
                calls: 0,
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    w.stop();
    assert_eq!(w.phase(), Phase::Stopping);
    release.send(()).unwrap();
    let report = wait_ready(&mut handle);
    assert!(report.cancelled);
    assert_eq!(report.task.execution.outcome, Err(Fault::Cancelled));
    assert!(report.response.is_none());
    let host = finish(&mut w);
    assert_eq!(w.phase(), Phase::Stopped);
    assert_eq!(w.pending(), 0);
    host.store_local().integrity_check().unwrap();
    let (_calls, script) = simple(&[b"x"]);
    assert!(matches!(
        w.submit(request_frame(), script, WAIT),
        Err(JobError::Closed)
    ));
}

fn await_ready(handle: &mut morrow_plugin_runtime::io_jobs::JobHandle) {
    let until = Instant::now() + WAIT;
    loop {
        if handle.poll() == Poll::Ready {
            return;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn ready_then_cancel_must_not_deliver_success() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (_, script) = simple(&[b"sensitive"]);
    let mut worker = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = worker.submit(request_frame(), script, WAIT).unwrap();
    await_ready(&mut handle);
    handle.cancel();
    let report = handle.read(4096).unwrap().unwrap();
    worker.stop();
    let _host = finish(&mut worker);
    assert!(
        report.task.execution.outcome.is_err() && report.response.is_none(),
        "success after explicit handle cancellation: {:?}",
        report
    );
}
#[test]
fn ready_then_worker_stop_must_not_deliver_success() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (_, script) = simple(&[b"sensitive"]);
    let mut worker = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut handle = worker.submit(request_frame(), script, WAIT).unwrap();
    await_ready(&mut handle);
    worker.stop();
    let _host = finish(&mut worker);
    let report = handle.read(4096).unwrap().unwrap();
    assert!(
        report.task.execution.outcome.is_err() && report.response.is_none(),
        "success after worker revocation: {:?}",
        report
    );
}
#[test]
fn external_connection_revocation_must_block_router() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let revocation = h.revocation(&c).unwrap();
    let (calls, script) = simple(&[b"sensitive"]);
    let mut worker = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    revocation.revoke();
    assert!(matches!(
        worker.submit(request_frame(), script, WAIT),
        Err(JobError::Closed)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let _host = finish(&mut worker);
}
#[test]
fn spawn_must_validate_public_limits() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let worker = IoWorker::spawn(
        p,
        h,
        c,
        || 1,
        2,
        JobLimits {
            max_calls: 0,
            max_job_bytes: u64::MAX,
            max_total_bytes: u64::MAX,
        },
    );
    assert!(
        worker.is_err(),
        "unchecked public JobLimits bypasses hard caps"
    );
}
#[test]
fn ready_reports_hold_capacity_through_read_bound_and_release_on_read_or_drop() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let mut worker = IoWorker::spawn(p, h, c, || 1, 1, JobLimits::default()).unwrap();
    let (_, script) = simple(&[b"retained"]);
    let mut first = worker.submit(request_frame(), script, WAIT).unwrap();
    await_ready(&mut first);
    for _ in 0..8 {
        assert_eq!(worker.pending(), 1);
        assert!(matches!(first.read(0), Err(JobError::ReadBound)));
        let (_, next) = simple(&[b"no"]);
        assert!(matches!(
            worker.submit(request_frame(), next, WAIT),
            Err(JobError::Busy)
        ));
    }
    assert!(
        first
            .read(4096)
            .unwrap()
            .unwrap()
            .task
            .execution
            .outcome
            .is_ok()
    );
    assert_eq!(worker.pending(), 0);
    let (_, script) = simple(&[b"drop"]);
    let mut second = worker.submit(request_frame(), script, WAIT).unwrap();
    await_ready(&mut second);
    drop(second);
    assert_eq!(worker.pending(), 0);
    let (_, script) = simple(&[b"reused"]);
    let mut third = worker.submit(request_frame(), script, WAIT).unwrap();
    assert_eq!(wait_ready(&mut third).response.unwrap().payload, b"reused");
    worker.stop();
    let _host = finish(&mut worker);
}

#[test]
fn dropping_running_handle_cannot_release_capacity_while_router_is_still_running() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut worker = IoWorker::spawn(p, h, c, || 1, 1, JobLimits::default()).unwrap();
    let handle = worker
        .submit(
            request_frame(),
            Box::new(Gate {
                entered,
                release: gate,
                calls: 0,
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    drop(handle);
    assert_eq!(worker.pending(), 1);
    let (_, next) = simple(&[b"no"]);
    assert!(matches!(
        worker.submit(request_frame(), next, WAIT),
        Err(JobError::Busy)
    ));
    release.send(()).unwrap();
    let until = Instant::now() + WAIT;
    while worker.pending() != 0 {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    let (_, next) = simple(&[b"yes"]);
    let mut handle = worker.submit(request_frame(), next, WAIT).unwrap();
    assert_eq!(wait_ready(&mut handle).response.unwrap().payload, b"yes");
    worker.stop();
    let _host = finish(&mut worker);
}

#[test]
fn expiry_and_external_revocation_after_ready_suppress_delivery() {
    for revoke in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (p, h, c) = setup(&dir, 1);
        let revocation = h.revocation(&c).unwrap();
        let mut worker = IoWorker::spawn(p, h, c, || 1, 1, JobLimits::default()).unwrap();
        let (_, script) = simple(&[b"private"]);
        let mut handle = worker
            .submit(
                request_frame(),
                script,
                if revoke {
                    WAIT
                } else {
                    Duration::from_millis(200)
                },
            )
            .unwrap();
        await_ready(&mut handle);
        if revoke {
            revocation.revoke();
        } else {
            thread::sleep(Duration::from_millis(220));
        }
        let report = handle.read(0).unwrap().unwrap();
        assert_eq!(
            report.task.execution.outcome,
            Err(if revoke {
                Fault::Cancelled
            } else {
                Fault::Deadline
            })
        );
        assert!(report.response.is_none());
        assert!(report.cancelled);
        worker.stop();
        let _host = finish(&mut worker);
    }
}

#[test]
fn external_revocation_during_route_prevents_further_calls_and_queued_effects() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 2);
    let revocation = h.revocation(&c).unwrap();
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut worker = IoWorker::spawn(p, h, c, || 1, 2, JobLimits::default()).unwrap();
    let mut running = worker
        .submit(
            request_frame(),
            Box::new(Gate {
                entered,
                release: gate,
                calls: 0,
            }),
            WAIT,
        )
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    let (calls, script) = simple(&[b"never", b"never"]);
    let mut queued = worker.submit(request_frame(), script, WAIT).unwrap();
    revocation.revoke();
    release.send(()).unwrap();
    let late = wait_ready(&mut running);
    assert!(late.task.execution.outcome.is_err());
    assert!(late.response.is_none());
    assert!(late.unknown);
    let rejected = wait_ready(&mut queued);
    assert!(rejected.task.execution.outcome.is_err());
    assert!(rejected.response.is_none());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let _host = finish(&mut worker);
}

fn limited_setup(dir: &tempfile::TempDir) -> (PreparedPackage, HostRuntime, Connection) {
    let wasm = module(1);
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    manifest.required_features.push(io::FEATURE.into());
    let mut declaration =
        io::declaration(vec![IoCapability::HttpRequest], vec!["api.invoke".into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 1;
    budget.max_job_bytes = 2048;
    budget.max_bytes = 4096;
    budget.max_duration_ms = 50;
    manifest.io_declaration = Some(declaration);
    let package = Package::build(manifest, &wasm).unwrap();
    let p = PreparedPackage::new(package, Limits::default()).unwrap();
    let mut h =
        HostRuntime::new(Store::open(&dir.path().join("db"), EventBudget::default()).unwrap())
            .unwrap();
    let c = p.connect(&mut h).unwrap();
    (p, h, c)
}

#[test]
fn manifest_limits_bound_slots_bytes_duration_and_runtime_calls() {
    for (capacity, limits) in [
        (2, JobLimits::new(1, 2048, 4096).unwrap()),
        (1, JobLimits::new(1, 2049, 4096).unwrap()),
        (1, JobLimits::new(1, 2048, 4097).unwrap()),
        (1, JobLimits::new(17, 2048, 4096).unwrap()),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let (p, h, c) = limited_setup(&dir);
        assert!(matches!(
            IoWorker::spawn(p, h, c, || 1, capacity, limits),
            Err(JobError::InvalidOptions)
        ));
    }
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = limited_setup(&dir);
    let mut worker =
        IoWorker::spawn(p, h, c, || 1, 1, JobLimits::new(1, 2048, 4096).unwrap()).unwrap();
    let (calls, script) = simple(&[b"never"]);
    assert!(matches!(
        worker.submit(request_frame(), script, Duration::from_millis(51)),
        Err(JobError::InvalidOptions)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(worker.bytes(), 0);
    assert_eq!(worker.pending(), 0);
    worker.stop();
    let _host = finish(&mut worker);
}

#[test]
fn drain_preserves_valid_ready_until_read_or_drop() {
    for abandon in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (p, h, c) = setup(&dir, 1);
        let mut worker = IoWorker::spawn(p, h, c, || 1, 1, JobLimits::default()).unwrap();
        let (_, script) = simple(&[b"drained"]);
        let mut handle = worker.submit(request_frame(), script, WAIT).unwrap();
        await_ready(&mut handle);
        worker.drain(WAIT).unwrap();
        thread::sleep(Duration::from_millis(30));
        assert_eq!(worker.phase(), Phase::Draining);
        assert!(worker.try_finish().unwrap().is_none());
        assert_eq!(worker.pending(), 1);
        let (_, script) = simple(&[b"closed"]);
        assert!(matches!(
            worker.submit(request_frame(), script, WAIT),
            Err(JobError::Closed)
        ));
        if !abandon {
            let report = handle.read(4096).unwrap().unwrap();
            assert!(report.task.execution.outcome.is_ok());
            assert_eq!(report.response.unwrap().payload, b"drained");
        }
        drop(handle);
        let _host = finish(&mut worker);
        assert_eq!(worker.pending(), 0);
    }
}

#[test]
fn expired_ready_metadata_does_not_prevent_drain_from_ending() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, 1);
    let mut worker = IoWorker::spawn(p, h, c, || 1, 1, JobLimits::default()).unwrap();
    let (_, script) = simple(&[b"expired"]);
    let mut handle = worker.submit(request_frame(), script, WAIT).unwrap();
    await_ready(&mut handle);
    worker.drain(Duration::from_millis(20)).unwrap();
    let _host = finish(&mut worker);
    assert_eq!(worker.phase(), Phase::Stopped);
    let report = handle.read(0).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Err(Fault::Deadline));
    assert!(report.cancelled);
    assert!(report.response.is_none());
    assert_eq!(worker.pending(), 0);
}
