//! Lost delivery receipts for nonempty, multichunk guest mutation calls.
//! Every effect is routed through the original managed owner and a real Wasm import.
use super::*;
use morrow_core::mutation::{self, Action, Effect, Phase as WirePhase, Request, Status};
use morrow_plugin_runtime::io_jobs::{JobHandle, MutationGuestJobMode, MutationGuestLease, Poll};

fn wasm() -> Vec<u8> {
    if let Some(path) = std::env::var_os("MORROW_MUTATION_DELIVERY_WASM_PATH") {
        let path = std::path::PathBuf::from(path);
        let bytes = fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            bytes.starts_with(b"\0asm"),
            "{} is not WebAssembly",
            path.display()
        );
        assert!(
            bytes.len() <= morrow_core::plugin_package::MAX_MODULE_BYTES,
            "{} exceeds the module limit",
            path.display()
        );
        return bytes;
    }
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

fn request(call_id: u64, lease: MutationGuestLease, action: Action) -> Request {
    let mut submission = [0u8; 32];
    submission[..8].copy_from_slice(&call_id.to_le_bytes());
    submission[8] = 0x73;
    Request {
        call_id,
        reference: lease.reference(),
        submission,
        operation_id: "guest-delivery-create".into(),
        deadline_ms: 10_000,
        action,
    }
}

fn content() -> Vec<u8> {
    let length = 3 * MAX_MUTATION_CHUNK + 37;
    let mut bytes = Vec::with_capacity(length);
    let mut counter = 0u64;
    while bytes.len() < length {
        let mut hash = Sha256::new();
        hash.update(b"morrow-guest-lost-receipt-content-v1");
        hash.update(counter.to_le_bytes());
        bytes.extend_from_slice(&hash.finalize());
        counter += 1;
    }
    bytes.truncate(length);
    bytes
}

fn ready_job(handle: &mut JobHandle, action: &str) {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == Poll::Pending {
        assert!(Instant::now() < until, "{action} guest job did not finish");
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(handle.poll(), Poll::Ready, "{action} guest job unavailable");
}

fn lost_reply(worker: &IoWorker<Owner>, mode: MutationGuestJobMode, request: &Request) {
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap();
    ready_job(&mut handle, &format!("{:?}", request.action.kind()));
    drop(handle); // Computed, but the caller deliberately never claims its receipt.
}

fn call(
    worker: &IoWorker<Owner>,
    mode: MutationGuestJobMode,
    request: &Request,
) -> mutation::Response {
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap();
    ready_job(&mut handle, &format!("{:?}", request.action.kind()));
    let report = handle.read(131_072).unwrap().unwrap();
    assert_eq!(
        report.task.execution.outcome,
        Ok(0),
        "call_id={} action={:?}: {report:?}",
        request.call_id,
        request.action.kind()
    );
    assert_eq!(report.calls, 1);
    let response = report
        .mutation_response
        .expect("correlated mutation response");
    assert_eq!(response.call_id, request.call_id);
    assert_eq!(response.reference, request.reference);
    assert_eq!(response.submission, request.submission);
    assert_eq!(response.kind, request.action.kind());
    response
}

fn rejected_conflicting_receipt(
    worker: &IoWorker<Owner>,
    mode: MutationGuestJobMode,
    request: &Request,
) {
    let mut handle = worker
        .submit_mutation_guest_frame(request.encode().unwrap(), mode, Duration::from_secs(10))
        .unwrap();
    ready_job(&mut handle, "conflicting cached chunk");
    let report = handle.read(131_072).unwrap().unwrap();
    assert_eq!(
        report.task.execution.outcome,
        Err(morrow_plugin_runtime::Fault::TaskProtocol),
        "conflicting submission must not be accepted: {report:?}"
    );
    assert!(report.mutation_response.is_none());
}

fn retry_with_call_id(original: &Request, call_id: u64) -> Request {
    let mut retry = original.clone();
    retry.call_id = call_id;
    retry
}

#[test]
fn multichunk_guest_recovers_lost_stage_commit_and_execute_receipts_without_replaying_effect() {
    let _serial = serial_effects();
    let (dir, _, mut worker) = Fixture::with_wasm(wasm(), true).start();
    let root = dir.path().join("lost-guest-delivery");
    fs::create_dir(&root).unwrap();
    let created = root.join("created.bin");
    let body = content();
    let digest: [u8; 32] = Sha256::digest(&body).into();
    let (session, mut selection) = worker
        .select_mutation_create(
            root,
            RelativeFilePath::parse("created.bin").unwrap(),
            scope(Disposition::Create),
            [161; 32],
        )
        .unwrap();
    selected(&mut selection);
    let plan = planned(
        &mut worker
            .build_mutation_plan(
                session,
                "guest-delivery-create".into(),
                body.len() as u64,
                Some(digest),
            )
            .unwrap(),
    );
    let lease = match read(
        &mut worker
            .issue_mutation_guest(session, plan, [162; 32])
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestApproved(lease) => lease,
        other => panic!("expected guest approval: {other:?}"),
    };
    let stage = MutationGuestJobMode::Stage(lease);
    let prepare = request(
        1,
        lease,
        Action::Create {
            content_length: body.len() as u64,
            content_sha256: digest,
        },
    );
    assert_eq!(
        (
            call(&worker, stage, &prepare).status,
            call(&worker, stage, &retry_with_call_id(&prepare, 101)).phase
        ),
        (Status::Completed, WirePhase::Prepared)
    );

    let chunks = body.chunks(MAX_MUTATION_CHUNK).count();
    assert_eq!(chunks, 4);
    for (index, chunk) in body.chunks(MAX_MUTATION_CHUNK).enumerate() {
        let offset = index * MAX_MUTATION_CHUNK;
        let staged = request(
            index as u64 + 2,
            lease,
            Action::Chunk {
                offset: offset as u64,
                bytes: chunk.to_vec(),
            },
        );
        lost_reply(&worker, stage, &staged);
        let history = call(
            &worker,
            stage,
            &request(20 + index as u64, lease, Action::Query),
        );
        assert_eq!(
            (
                history.status,
                history.phase,
                history.staged_bytes,
                history.durable_content
            ),
            (
                Status::Completed,
                WirePhase::Prepared,
                (offset + chunk.len()) as u64,
                false
            )
        );
        if index == 1 {
            let mut forged = retry_with_call_id(&staged, 201);
            let Action::Chunk { bytes, .. } = &mut forged.action else {
                unreachable!()
            };
            bytes[0] ^= 0x80;
            rejected_conflicting_receipt(&worker, stage, &forged);
            let after_rejection = call(&worker, stage, &request(40, lease, Action::Query));
            assert_eq!(
                (
                    after_rejection.status,
                    after_rejection.phase,
                    after_rejection.staged_bytes,
                    after_rejection.durable_content
                ),
                (
                    Status::Completed,
                    WirePhase::Prepared,
                    (offset + chunk.len()) as u64,
                    false
                )
            );
        }
        let recovered = call(
            &worker,
            stage,
            &retry_with_call_id(&staged, 102 + index as u64),
        );
        assert_eq!(
            (
                recovered.status,
                recovered.phase,
                recovered.staged_bytes,
                recovered.durable_content
            ),
            (
                Status::Completed,
                WirePhase::Prepared,
                (offset + chunk.len()) as u64,
                false
            )
        );
    }

    let commit = request(6, lease, Action::Commit);
    lost_reply(&worker, stage, &commit);
    let history = call(&worker, stage, &request(30, lease, Action::Query));
    assert_eq!(
        (
            history.status,
            history.phase,
            history.staged_bytes,
            history.durable_content
        ),
        (
            Status::Completed,
            WirePhase::Prepared,
            body.len() as u64,
            true
        )
    );
    let recovered = call(&worker, stage, &retry_with_call_id(&commit, 106));
    assert_eq!(
        (
            recovered.status,
            recovered.phase,
            recovered.staged_bytes,
            recovered.durable_content
        ),
        (
            Status::Completed,
            WirePhase::Prepared,
            body.len() as u64,
            true
        )
    );
    assert!(!created.exists(), "commit is not the OS effect");

    let permit = match read(
        &mut worker
            .authorize_mutation_guest_execution(session, lease.plan_sha256())
            .unwrap(),
    )
    .unwrap()
    {
        MutationResponse::GuestExecutionAuthorized(permit) => permit,
        other => panic!("expected execution permit: {other:?}"),
    };
    let execute = request(7, lease, Action::Execute);
    lost_reply(&worker, MutationGuestJobMode::Execute(permit), &execute);
    // Ready means the original owner finished this job; the actual file now
    // proves the effect point before any retry or changed-path experiment.
    assert_eq!(fs::read(&created).unwrap(), body);
    fs::remove_file(&created).unwrap();
    let unrelated = b"unrelated replacement after the original effect";
    fs::write(&created, unrelated).unwrap();

    let observed = call(&worker, stage, &request(31, lease, Action::Query));
    assert_eq!(
        (observed.status, observed.phase, observed.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&created).unwrap(), unrelated);
    let recovered = call(
        &worker,
        MutationGuestJobMode::Execute(permit),
        &retry_with_call_id(&execute, 107),
    );
    assert_eq!(
        (recovered.status, recovered.phase, recovered.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&created).unwrap(), unrelated);
    let fresh = call(
        &worker,
        MutationGuestJobMode::Execute(permit),
        &request(8, lease, Action::Execute),
    );
    assert_eq!(
        (fresh.status, fresh.phase, fresh.effect),
        (
            Status::OutcomeUnknown,
            WirePhase::OutcomeUnknown,
            Effect::Unspecified
        )
    );
    assert_eq!(fs::read(&created).unwrap(), unrelated);
    let again = call(&worker, stage, &request(32, lease, Action::Query));
    assert_eq!(
        (again.status, again.phase, again.effect),
        (Status::Completed, WirePhase::Observed, Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&created).unwrap(), unrelated);
    assert_eq!(
        call(&worker, stage, &request(9, lease, Action::Release)).status,
        Status::Completed
    );
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}
