//! Real paused Wasm mutation imports on the selected original managed owner.
//! The same helper qualifies WAT and freshly compiled Rust/C/C++ SDK guests.
use super::*;
use morrow_core::mutation::{self, Action, Effect, Phase as WirePhase, Request, Status};
use morrow_plugin_runtime::io_jobs::{JobHandle, MutationGuestJobMode, MutationGuestLease, Poll};

fn wat_module() -> Vec<u8> {
    wat::parse_str(
        r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_mutation_v1" "call" (func $mutation (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32)
            (local $n i32) (local $m i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $mutation local.set $m
            i32.const 131072 local.get $m call $done drop
            i32.const 0))"#,
    )
    .unwrap()
}

fn job(mut handle: JobHandle) -> morrow_plugin_runtime::io_jobs::JobReport {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == Poll::Pending {
        assert!(
            Instant::now() < until,
            "mutation guest job did not complete"
        );
        thread::sleep(Duration::from_millis(1));
    }
    handle.read(131072).unwrap().unwrap()
}

fn call(
    worker: &IoWorker<Owner>,
    mode: MutationGuestJobMode,
    request: &Request,
) -> morrow_core::mutation::Response {
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == Poll::Pending {
        assert!(
            Instant::now() < until,
            "mutation guest job did not complete"
        );
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(handle.read(0).unwrap_err(), JobError::ReadBound);
    let report = handle.read(131072).unwrap().unwrap();
    assert_eq!(report.task.execution.outcome, Ok(0), "{report:?}");
    assert_eq!(report.calls, 1);
    let raw = report
        .mutation_frame
        .as_ref()
        .expect("original completion frame");
    assert_eq!(
        report.payload_bytes(),
        raw.len() + request.operation_id.len()
    );
    assert_eq!(
        mutation::Response::decode(request, raw).unwrap(),
        *report.mutation_response.as_ref().unwrap()
    );
    let response = report.mutation_response.expect("correlated mutation reply");
    assert_eq!(response.call_id, request.call_id);
    assert_eq!(response.reference, request.reference);
    assert_eq!(response.submission, request.submission);
    response
}

fn request(
    call_id: u64,
    lease: MutationGuestLease,
    submission: u8,
    operation_id: &str,
    action: Action,
) -> Request {
    Request {
        call_id,
        reference: lease.reference(),
        submission: [submission; 32],
        operation_id: operation_id.into(),
        deadline_ms: 10_000,
        action,
    }
}

/// Called by WAT and the three independent SDK Wasm qualification tests.
pub(super) fn exercise_module(wasm: Vec<u8>) {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::with_wasm(wasm, true).start();
    let root = dir.path().join("mutation-guest-frame");
    fs::create_dir(&root).unwrap();
    let created = root.join("created.bin");
    let body = b"original owner imported guest content";
    let operation = "guest-frame-create";
    let (session, mut selection) = worker
        .select_mutation_create(
            root,
            RelativeFilePath::parse("created.bin").unwrap(),
            scope(Disposition::Create),
            [101; 32],
        )
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(
                session,
                operation.into(),
                body.len() as u64,
                Some(Sha256::digest(body).into()),
            )
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan.clone(), [102; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected approved lease: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    let prepare = request(
        1,
        lease,
        111,
        operation,
        Action::Create {
            content_length: body.len() as u64,
            content_sha256: Sha256::digest(body).into(),
        },
    );
    let response = call(&worker, stage, &prepare);
    assert_eq!(
        (response.status, response.phase),
        (Status::Completed, WirePhase::Prepared)
    );
    let mut replay = prepare.clone();
    replay.call_id = 91;
    assert_eq!(call(&worker, stage, &replay).call_id, 91);
    let mut forged = prepare.clone();
    forged.action = Action::Delete;
    assert_eq!(
        job(worker
            .submit_mutation_guest_frame(forged.encode().unwrap(), stage, Duration::from_secs(10))
            .unwrap())
        .task
        .execution
        .outcome,
        Err(morrow_plugin_runtime::Fault::TaskProtocol),
    );
    let chunk = request(
        2,
        lease,
        112,
        operation,
        Action::Chunk {
            offset: 0,
            bytes: body.to_vec(),
        },
    );
    assert_eq!(call(&worker, stage, &chunk).staged_bytes, body.len() as u64);
    let commit = request(3, lease, 113, operation, Action::Commit);
    assert!(call(&worker, stage, &commit).durable_content);
    assert!(!created.exists());
    let permit = match read(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected delivered permit: {other:?}"),
    };
    let execute = request(4, lease, 114, operation, Action::Execute);
    assert!(
        worker
            .submit_mutation_guest_frame(execute.encode().unwrap(), stage, Duration::from_secs(10))
            .is_err()
    );
    let effect = call(&worker, MutationGuestJobMode::Execute(permit), &execute);
    assert_eq!(
        (effect.status, effect.phase, effect.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&created).unwrap(), body);
    let mut execute_replay = execute.clone();
    execute_replay.call_id = 93;
    assert_eq!(
        call(
            &worker,
            MutationGuestJobMode::Execute(permit),
            &execute_replay
        )
        .effect,
        Effect::OsSucceeded
    );
    let attempted_replay = request(94, lease, 121, operation, Action::Execute);
    assert_eq!(
        call(
            &worker,
            MutationGuestJobMode::Execute(permit),
            &attempted_replay
        )
        .status,
        Status::OutcomeUnknown
    );
    assert_eq!(fs::read(&created).unwrap(), body);
    let query = request(5, lease, 115, operation, Action::Query);
    let history = call(&worker, stage, &query);
    assert_eq!(
        (history.phase, history.effect),
        (WirePhase::Observed, Effect::OsSucceeded)
    );
    let release = request(6, lease, 116, operation, Action::Release);
    assert_eq!(call(&worker, stage, &release).kind, mutation::Kind::Release);
    let mut release_replay = release.clone();
    release_replay.call_id = 92;
    assert_eq!(call(&worker, stage, &release_replay).call_id, 92);

    let deleted = dir.path().join("deleted.bin");
    fs::write(&deleted, b"delete original").unwrap();
    let operation = "guest-frame-delete";
    let (session, mut selection) = worker
        .select_mutation_existing(deleted.clone(), scope(Disposition::Delete), [103; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, operation.into(), 0, None)
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [104; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected approved delete lease: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    let prepare = request(7, lease, 117, operation, Action::Delete);
    assert_eq!(call(&worker, stage, &prepare).phase, WirePhase::Prepared);
    assert!(deleted.exists());
    let permit = match read(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected delete permit: {other:?}"),
    };
    let execute = request(8, lease, 118, operation, Action::Execute);
    assert_eq!(
        call(&worker, MutationGuestJobMode::Execute(permit), &execute).effect,
        Effect::OsSucceeded
    );
    assert!(!deleted.exists());
    let query = request(9, lease, 119, operation, Action::Query);
    let history = call(&worker, stage, &query);
    assert_eq!(
        (history.phase, history.effect),
        (WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(
        call(
            &worker,
            stage,
            &request(10, lease, 120, operation, Action::Release)
        )
        .status,
        Status::Completed
    );
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn wat_guest_runs_original_owner_create_delete() {
    exercise_module(wat_module());
}

#[test]
fn wat_guest_refuses_foreign_reference_and_cancelled_plan() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::with_wasm(wat_module(), true).start();
    let leaf = dir.path().join("cancelled.bin");
    fs::write(&leaf, b"still here").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [130; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-cancel".into(), 0, None)
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [131; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected approved lease: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    let mut wrong = request(1, lease, 132, "guest-cancel", Action::Delete);
    wrong.reference = [200; 32];
    assert_eq!(
        worker
            .submit_mutation_guest_frame(wrong.encode().unwrap(), stage, Duration::from_secs(10))
            .err(),
        Some(JobError::InvalidOptions),
    );
    assert_eq!(
        call(
            &worker,
            stage,
            &request(2, lease, 133, "guest-cancel", Action::Delete)
        )
        .phase,
        WirePhase::Prepared
    );
    assert!(matches!(
        read(&mut worker.cancel_mutation_plan(session).unwrap()),
        Ok(MutationResponse::PlanCancelled(_))
    ));
    let query = call(
        &worker,
        stage,
        &request(3, lease, 134, "guest-cancel", Action::Query),
    );
    assert_eq!(
        (query.phase, query.effect),
        (WirePhase::CancelledBeforeDispatch, Effect::Unspecified)
    );
    assert_eq!(
        call(
            &worker,
            stage,
            &request(4, lease, 135, "guest-cancel", Action::Release)
        )
        .status,
        Status::Completed
    );
    assert_eq!(fs::read(&leaf).unwrap(), b"still here");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));

    let (_, _, mut ordinary) = Fixture::new().start();
    assert_eq!(
        ordinary
            .submit_mutation_guest_frame(
                request(5, lease, 136, "guest-cancel", Action::Query)
                    .encode()
                    .unwrap(),
                stage,
                Duration::from_secs(10),
            )
            .err(),
        Some(JobError::InvalidOptions),
    );
    assert_eq!(reclaim(&mut ordinary).disconnect, Ok(()));
}

#[test]
fn submission_receipt_cannot_extend_its_original_deadline() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::with_wasm(wat_module(), true).start();
    let leaf = dir.path().join("deadline.bin");
    fs::write(&leaf, b"unchanged").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [140; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-deadline".into(), 0, None)
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [141; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected approval: {other:?}"),
    };
    let mode = MutationGuestJobMode::Stage(lease);
    let mut prepare = request(1, lease, 142, "guest-deadline", Action::Delete);
    prepare.deadline_ms = 1000;
    assert_eq!(call(&worker, mode, &prepare).phase, WirePhase::Prepared);
    thread::sleep(Duration::from_millis(1200));
    let mut retry = prepare.clone();
    retry.call_id = 2;
    let report = job(worker
        .submit_mutation_guest_frame(retry.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap());
    assert!(report.task.execution.outcome.is_err());
    assert!(report.mutation_response.is_none());
    assert!(report.mutation_frame.is_none());
    let fresh = call(
        &worker,
        mode,
        &request(3, lease, 143, "guest-deadline", Action::Query),
    );
    assert_eq!(
        (fresh.phase, fresh.effect),
        (WirePhase::Prepared, Effect::Unspecified)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"unchanged");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn call_deadline_is_rechecked_before_effect_claim() {
    let _serial = serial_effects();
    let fixture = Fixture::with_wasm(wat_module(), true);
    let managed_gate = Arc::clone(&fixture.owner.managed_gate);
    let (dir, _, mut worker) = fixture.start();
    let leaf = dir.path().join("gate-deadline.bin");
    fs::write(&leaf, b"no effect").unwrap();
    let (session, mut selection) = worker
        .select_mutation_existing(leaf.clone(), scope(Disposition::Delete), [150; 32])
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(session, "guest-gate-deadline".into(), 0, None)
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [151; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected approval: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    assert_eq!(
        call(
            &worker,
            stage,
            &request(1, lease, 152, "guest-gate-deadline", Action::Delete)
        )
        .phase,
        WirePhase::Prepared
    );
    let permit = match read(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected permit: {other:?}"),
    };
    let (entered_sender, entered_receiver) = mpsc::channel();
    let (release_sender, release_receiver) = mpsc::channel();
    *managed_gate.lock().unwrap_or_else(|e| e.into_inner()) =
        Some((entered_sender, release_receiver));
    let mut execute = request(2, lease, 153, "guest-gate-deadline", Action::Execute);
    execute.deadline_ms = 1000;
    let handle = worker
        .submit_mutation_guest_frame(
            execute.encode().unwrap(),
            MutationGuestJobMode::Execute(permit),
            Duration::from_secs(10),
        )
        .unwrap();
    entered_receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    thread::sleep(Duration::from_millis(1200));
    release_sender.send(()).unwrap();
    let report = job(handle);
    assert!(report.task.execution.outcome.is_err());
    assert!(report.mutation_response.is_none());
    assert!(report.mutation_frame.is_none());
    assert!(
        matches!(read(&mut worker.query_mutation(session).unwrap()), Ok(MutationResponse::History { record: Some(record), .. }) if record.phase() == Phase::Prepared)
    );
    assert!(matches!(
        read(&mut worker.release_mutation(session).unwrap()),
        Ok(MutationResponse::Released)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"no effect");
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}
