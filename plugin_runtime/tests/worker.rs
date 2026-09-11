#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    content::CardRecord,
    dispatch::HostRuntime,
    lifecycle::GrantKind,
    plugin_package::Package,
    runtime::RenameRequest,
    store::{EventBudget, Store},
    transaction::Lookup,
};
use morrow_plugin_runtime::{
    Fault, Limits, Report,
    package::PreparedPackage,
    worker::{Phase, TaskHandle, Worker, WorkerError},
};
use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(10);
fn setup(
    dir: &tempfile::TempDir,
    twice: bool,
) -> (
    PreparedPackage,
    HostRuntime,
    morrow_core::dispatch::Connection,
) {
    let request = RenameRequest {
        operation_id: "op".into(),
        card_id: "card".into(),
        expected_revision: 1,
        title: "after".into(),
    }
    .encode()
    .unwrap();
    let data = request
        .iter()
        .map(|b| format!("{}{:02x}", char::from(92), b))
        .collect::<String>();
    let call = format!(
        "i32.const 0 i32.const {} i32.const 65536 i32.const 65536 call $e drop",
        request.len()
    );
    let wasm=wat::parse_str(format!(r#"(module (import "morrow_v1" "exchange" (func $e (param i32 i32 i32 i32) (result i32))) (memory (export "memory") 2) (data (i32.const 0) "{data}") (func (export "morrow_run") (result i32) {call} {} i32.const 7))"#,if twice{&call}else{""})).unwrap();
    let p = Package::build(
        Package::manifest_for(
            "example",
            "1.0.0",
            &wasm,
            vec![morrow_core::plugin_package::proto::Capability::RenameCard],
        ),
        &wasm,
    )
    .unwrap();
    let prepared = PreparedPackage::new(p, Limits::default()).unwrap();
    let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
    store
        .create_local(
            "seed",
            &CardRecord::new("card", "note", 1, "before", vec![]).unwrap(),
        )
        .unwrap();
    let mut host = HostRuntime::new(store).unwrap();
    let mut c = prepared.connect(&mut host).unwrap();
    host.grant(&mut c, GrantKind::Rename, "card", u64::MAX, 0)
        .unwrap();
    (prepared, host, c)
}
fn gated_clock() -> (
    impl FnMut() -> u64 + Send + 'static,
    mpsc::Receiver<()>,
    mpsc::Sender<()>,
) {
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut ticks = 0;
    (
        move || {
            ticks += 1;
            if ticks == 1 {
                entered.send(()).unwrap();
                gate.recv_timeout(WAIT).unwrap();
            }
            ticks
        },
        observed,
        release,
    )
}
fn result(task: &mut TaskHandle) -> Result<Report, WorkerError> {
    let until = Instant::now() + WAIT;
    loop {
        match task.try_result() {
            Ok(Some(r)) => return Ok(r),
            Err(e) => return Err(e),
            Ok(None) => {
                assert!(Instant::now() < until, "task never completed");
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
fn finish(worker: &mut Worker) -> Result<HostRuntime, WorkerError> {
    let until = Instant::now() + WAIT;
    loop {
        match worker.try_finish() {
            Ok(Some(h)) => return Ok(h),
            Err(e) => return Err(e),
            Ok(None) => {
                assert!(Instant::now() < until, "worker never terminated");
                thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
fn committed(host: &HostRuntime) {
    assert!(
        matches!(host.store_local().lookup_for_card("card","op").unwrap(),Lookup::Committed(r) if r.revision==2)
    );
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 2);
    host.store_local().integrity_check().unwrap();
}
#[test]
fn stop_revokes_before_commit_cancels_queue_and_remains_nonblocking() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, false);
    let (clock, entered, release) = gated_clock();
    let mut w = Worker::spawn(p, h, c, clock, 3).unwrap();
    let mut a = w.submit(WAIT).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    let mut b = w.submit(WAIT).unwrap();
    let mut c = w.submit(WAIT).unwrap();
    assert!(matches!(w.submit(WAIT), Err(WorkerError::Busy)));
    assert!(a.try_result().unwrap().is_none());
    assert_eq!(w.pending(), 3);
    w.stop();
    assert_eq!(w.phase(), Phase::Stopping);
    assert!(matches!(w.submit(WAIT), Err(WorkerError::Closed)));
    assert!(w.try_finish().unwrap().is_none());
    release.send(()).unwrap();
    let r = result(&mut a).unwrap();
    assert_eq!(r.outcome, Err(Fault::Cancelled));
    assert_eq!(r.host_calls, 1);
    for t in [&mut b, &mut c] {
        let r = result(t).unwrap();
        assert_eq!(r.outcome, Err(Fault::Cancelled));
        assert_eq!(r.host_calls, 0);
    }
    let host = finish(&mut w).unwrap();
    assert_eq!(w.phase(), Phase::Stopped);
    assert_eq!(w.pending(), 0);
    assert!(matches!(
        host.store_local().lookup_for_card("card", "op").unwrap(),
        Lookup::Absent
    ));
    assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
    drop(host);
    Store::open_existing(&dir.path().join("db"), EventBudget::default())
        .unwrap()
        .integrity_check()
        .unwrap();
}
#[test]
fn drain_finishes_admitted_jobs_and_retires_connection() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, false);
    let (clock, entered, release) = gated_clock();
    let mut w = Worker::spawn(p, h, c, clock, 3).unwrap();
    let mut a = w.submit(WAIT).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    let mut b = w.submit(WAIT).unwrap();
    w.drain(WAIT).unwrap();
    assert_eq!(w.phase(), Phase::Draining);
    assert!(matches!(w.submit(WAIT), Err(WorkerError::Closed)));
    assert!(w.try_finish().unwrap().is_none());
    release.send(()).unwrap();
    for t in [&mut a, &mut b] {
        let r = result(t).unwrap();
        assert_eq!(r.outcome, Ok(7));
        assert_eq!(r.host_calls, 1);
    }
    committed(&finish(&mut w).unwrap());
    assert_eq!(w.phase(), Phase::Stopped);
    assert!(matches!(w.try_finish(), Err(WorkerError::Consumed)));
}
#[test]
fn drain_deadline_suppresses_late_reply_and_never_starts_expired_queue() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, false);
    let (clock, entered, release) = gated_clock();
    let mut w = Worker::spawn(p, h, c, clock, 3).unwrap();
    let mut a = w.submit(WAIT).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    let mut b = w.submit(WAIT).unwrap();
    w.drain(Duration::from_millis(1)).unwrap();
    thread::sleep(Duration::from_millis(10));
    release.send(()).unwrap();
    let r = result(&mut a).unwrap();
    assert_eq!(r.outcome, Err(Fault::Deadline));
    assert_eq!(r.host_calls, 1);
    let r = result(&mut b).unwrap();
    assert_eq!(r.outcome, Err(Fault::Deadline));
    assert_eq!(r.host_calls, 0);
    committed(&finish(&mut w).unwrap());
}
#[test]
fn individual_and_dropped_handles_cancel_without_stopping_other_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, false);
    let (clock, entered, release) = gated_clock();
    let mut w = Worker::spawn(p, h, c, clock, 4).unwrap();
    let mut a = w.submit(WAIT).unwrap();
    entered.recv_timeout(WAIT).unwrap();
    let mut b = w.submit(WAIT).unwrap();
    let dropped = w.submit(WAIT).unwrap();
    let mut d = w.submit(WAIT).unwrap();
    assert_ne!(a.id(), b.id());
    b.cancel();
    drop(dropped);
    assert_eq!(w.phase(), Phase::Running);
    release.send(()).unwrap();
    assert_eq!(result(&mut a).unwrap().outcome, Ok(7));
    let cancelled = result(&mut b).unwrap();
    assert_eq!(cancelled.outcome, Err(Fault::Cancelled));
    assert_eq!(cancelled.host_calls, 0);
    assert_eq!(result(&mut d).unwrap().outcome, Ok(7));
    assert!(matches!(a.try_result(), Err(WorkerError::Consumed)));
    w.drain(WAIT).unwrap();
    committed(&finish(&mut w).unwrap());
}
#[test]
fn worker_panic_reports_unavailable_without_claiming_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, true);
    let observer = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    let mut calls = 0;
    let mut w = Worker::spawn(
        p,
        h,
        c,
        move || {
            calls += 1;
            assert!(
                !matches!(
                    observer.lookup_for_card("card", "op").unwrap(),
                    Lookup::Committed(_)
                ),
                "simulated host failure after independently observed commit"
            );
            calls
        },
        2,
    )
    .unwrap();
    let mut task = w.submit(WAIT).unwrap();
    assert!(matches!(result(&mut task), Err(WorkerError::Unavailable)));
    assert!(matches!(finish(&mut w), Err(WorkerError::Unavailable)));
    assert_eq!(w.phase(), Phase::Failed);
    let store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    assert!(
        matches!(store.lookup_for_card("card","op").unwrap(),Lookup::Committed(r) if r.revision==2)
    );
    store.integrity_check().unwrap();
}
#[test]
fn idle_stop_and_option_validation_do_not_start_tasks() {
    for drain in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (p, h, c) = setup(&dir, false);
        let mut w = Worker::spawn(p, h, c, || panic!("no job"), 1).unwrap();
        assert!(matches!(
            w.submit(Duration::ZERO),
            Err(WorkerError::InvalidOptions)
        ));
        assert!(matches!(
            w.submit(Duration::from_secs(3601)),
            Err(WorkerError::InvalidOptions)
        ));
        assert!(matches!(
            w.drain(Duration::ZERO),
            Err(WorkerError::InvalidOptions)
        ));
        assert_eq!(w.phase(), Phase::Running);
        if drain {
            w.drain(WAIT).unwrap();
        } else {
            w.stop();
        }
        let host = finish(&mut w).unwrap();
        assert_eq!(host.store_local().pending(0, 10).unwrap().len(), 1);
    }
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, false);
    assert!(matches!(
        Worker::spawn(p, h, c, || 0, 65),
        Err(WorkerError::InvalidOptions)
    ));
}

#[test]
fn stop_after_independently_observed_commit_preserves_result() {
    let dir = tempfile::tempdir().unwrap();
    let (p, h, c) = setup(&dir, true);
    let observer = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
    let (entered, observed) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let mut ticks = 0;
    let mut blocked = false;
    let mut w = Worker::spawn(
        p,
        h,
        c,
        move || {
            ticks += 1;
            if !blocked
                && matches!(
                    observer.lookup_for_card("card", "op").unwrap(),
                    Lookup::Committed(_)
                )
            {
                blocked = true;
                entered.send(()).unwrap();
                gate.recv_timeout(WAIT).unwrap();
            }
            ticks
        },
        2,
    )
    .unwrap();
    let mut a = w.submit(WAIT).unwrap();
    observed.recv_timeout(WAIT).unwrap();
    w.stop();
    assert!(w.try_finish().unwrap().is_none());
    release.send(()).unwrap();
    let r = result(&mut a).unwrap();
    assert_eq!(r.outcome, Err(Fault::Cancelled));
    assert_eq!(r.host_calls, 2);
    committed(&finish(&mut w).unwrap());
}
