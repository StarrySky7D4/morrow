use super::*;

const GUEST: &str = include_str!("../../tests/fixtures/owned_runner.wat");

fn runner(wat: &str, limits: Limits) -> Runner {
    Runner::new_io_task(&wat::parse_str(wat).unwrap(), limits).unwrap()
}
fn start(cancel: Cancellation) -> Execution {
    let runner = runner(GUEST, Limits::default());
    let input = vec![11, 22, 33];
    // Neither of these local variables may be borrowed by the returned state.
    Execution::start(&runner, Some(&input), cancel).unwrap()
}
fn reply(execution: &mut Execution, value: i32) {
    let token = execution.pending().unwrap().token.clone();
    execution
        .resume(&token, Ok(value.to_le_bytes().to_vec()))
        .unwrap();
}

const MUTATION_GUEST: &str = r#"(module
  (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
  (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
  (import "morrow_mutation_v1" "call" (func $mutate (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 6)
  (data (i32.const 0) "wire")
  (func (export "morrow_run") (result i32)
    (drop (call $read (i32.const 131072) (i32.const 131072)))
    (drop (call $mutate (i32.const INPUT) (i32.const LENGTH) (i32.const OUTPUT) (i32.const CAPACITY)))
    (drop (call $complete (i32.const 0) (i32.const 4)))
    (i32.const 0)))"#;

fn mutation_wat(input: i32, length: i32, output: i32, capacity: i32) -> Vec<u8> {
    let wat = MUTATION_GUEST
        .replace("INPUT", &input.to_string())
        .replace("LENGTH", &length.to_string())
        .replace("OUTPUT", &output.to_string())
        .replace("CAPACITY", &capacity.to_string());
    wat::parse_str(wat).unwrap()
}

#[test]
fn independent_mutation_import_requires_explicit_runner_and_owned_continuation() {
    let guest = mutation_wat(0, 4, 262144, MAX_MUTATION_FRAME_BYTES as i32);
    for ordinary in [
        Runner::new(&guest, Limits::default()),
        Runner::new_task(&guest, Limits::default()),
        Runner::new_io_task(&guest, Limits::default()),
        Runner::new_dependency_task(&guest, Limits::default()),
    ] {
        assert!(matches!(ordinary, Err(Fault::UnsupportedAbi)));
    }
    let runner = Runner::new_mutation_task(&guest, Limits::default()).unwrap();
    assert_eq!(
        runner
            .run_task(&[1], &mut |_| Ok(vec![1]), Cancellation::default())
            .report
            .outcome,
        Err(Fault::UnsupportedAbi)
    );
    let mut execution = Execution::start(&runner, Some(&[1]), Cancellation::default()).unwrap();
    let pending = execution.pending().unwrap();
    assert_eq!(pending.kind, Kind::Mutation);
    assert_eq!(pending.bytes, b"wire");
    assert_eq!(pending.capacity, MAX_MUTATION_FRAME_BYTES);
    let token = pending.token.clone();
    execution.resume(&token, Ok(b"reply".to_vec())).unwrap();
    assert!(execution.pending().is_none());
    let completed = execution.finish();
    assert_eq!(completed.report.outcome, Ok(0));
    assert_eq!(completed.report.host_calls, 1);
    assert_eq!(completed.completion.as_deref(), Some(b"wire".as_slice()));
}

#[test]
fn mutation_import_checks_bounded_disjoint_memory_and_call_budget_before_yield() {
    for (input, length, output, capacity, expected) in [
        (0, 0, 262144, 131072, Fault::Trap),
        (0, 131073, 262144, 131072, Fault::Trap),
        (0, 4, 2, 131072, Fault::Trap),
        (0, 4, 262145, 131072, Fault::Trap),
        (0, 4, 262144, 131071, Fault::Trap),
        (-1, 4, 262144, 131072, Fault::Trap),
    ] {
        let runner = Runner::new_mutation_task(
            &mutation_wat(input, length, output, capacity),
            Limits::default(),
        )
        .unwrap();
        let mut execution = Execution::start(&runner, Some(&[1]), Cancellation::default()).unwrap();
        assert!(execution.pending().is_none());
        let finished = execution.finish();
        assert_eq!(finished.report.outcome, Err(expected));
        assert_eq!(finished.report.host_calls, 0);
    }
    let runner = Runner::new_mutation_task(
        &mutation_wat(0, 4, 262144, 131072),
        Limits {
            host_calls: 0,
            ..Limits::default()
        },
    )
    .unwrap();
    let mut execution = Execution::start(&runner, Some(&[1]), Cancellation::default()).unwrap();
    assert!(execution.pending().is_none());
    assert_eq!(execution.finish().report.outcome, Err(Fault::Limits));
}

#[test]
fn mutation_pending_call_rejects_cancelled_delivery() {
    let runner =
        Runner::new_mutation_task(&mutation_wat(0, 4, 262144, 131072), Limits::default()).unwrap();
    let cancel = Cancellation::default();
    let mut execution = Execution::start(&runner, Some(&[1]), cancel.clone()).unwrap();
    let pending = execution.pending().unwrap();
    let memory = pending.memory;
    let output = pending.output;
    let token = pending.token.clone();
    let before = memory.data(&execution.store)[output..output + 5].to_vec();
    cancel.cancel();
    execution.resume(&token, Ok(b"reply".to_vec())).unwrap();
    assert_eq!(&memory.data(&execution.store)[output..output + 5], before);
    assert_eq!(execution.finish().report.outcome, Err(Fault::Cancelled));
}

#[test]
fn mutation_frame_accepts_exact_ceiling_and_rejects_oversize_reply() {
    let runner = Runner::new_mutation_task(
        &mutation_wat(0, MAX_MUTATION_FRAME_BYTES as i32, 262144, 131072),
        Limits::default(),
    )
    .unwrap();
    let mut execution = Execution::start(&runner, Some(&[1]), Cancellation::default()).unwrap();
    let pending = execution.pending().unwrap();
    assert_eq!(pending.kind, Kind::Mutation);
    assert_eq!(pending.bytes.len(), MAX_MUTATION_FRAME_BYTES);
    let token = pending.token.clone();
    execution
        .resume(&token, Ok(vec![0; MAX_MUTATION_FRAME_BYTES + 1]))
        .unwrap();
    assert_eq!(execution.finish().report.outcome, Err(Fault::Trap));
}

#[test]
fn original_call_survives_dropped_runner_and_input_and_preserves_locals() {
    let mut execution = start(Cancellation::default());
    assert_eq!(
        execution.store.data().task.as_ref().unwrap().input,
        [11, 22, 33]
    );
    assert_eq!(execution.pending().unwrap().kind, Kind::Core);
    assert_eq!(execution.pending().unwrap().bytes, b"core");
    let fuel = execution.store.get_fuel().unwrap();
    reply(&mut execution, 7);
    assert_eq!(execution.pending().unwrap().kind, Kind::Io);
    assert_eq!(execution.pending().unwrap().bytes, b"io");
    assert!(execution.store.get_fuel().unwrap() <= fuel);
    reply(&mut execution, 13);
    assert!(execution.pending().is_none());
    let result = execution.finish();
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.report.host_calls, 2);
    assert_eq!(result.completion.as_deref(), Some(b"done".as_slice()));
    assert!(result.report.fuel_remaining <= fuel);
}

#[test]
fn foreign_duplicate_and_stale_tokens_cannot_consume_current_continuation() {
    let mut execution = start(Cancellation::default());
    let mut other = start(Cancellation::default());
    let foreign = other.pending().unwrap().token.clone();
    let first = execution.pending().unwrap().token.clone();
    let fuel = execution.store.get_fuel().unwrap();
    assert_eq!(
        execution.resume(&foreign, Ok(vec![0; 4])),
        Err(Fault::TaskProtocol)
    );
    assert_eq!(execution.store.get_fuel().unwrap(), fuel);
    assert_eq!(execution.store.data().calls, 1);
    reply(&mut execution, 7);
    assert_eq!(
        execution.resume(&first, Ok(vec![0; 4])),
        Err(Fault::TaskProtocol)
    );
    assert_eq!(execution.pending().unwrap().bytes, b"io");
    let second = execution.pending().unwrap().token.clone();
    reply(&mut execution, 13);
    assert_eq!(
        execution.resume(&second, Ok(vec![0; 4])),
        Err(Fault::TaskProtocol)
    );
    assert_eq!(execution.finish().report.outcome, Ok(0));
}

#[test]
fn cancellation_and_deadline_during_pause_reject_ready_bytes_without_writing() {
    for deadline in [false, true] {
        let cancel = Cancellation::default();
        let mut execution = start(cancel.clone());
        reply(&mut execution, 7);
        let pending = execution.pending().unwrap();
        let token = pending.token.clone();
        let memory = pending.memory;
        let output = pending.output;
        let original = memory.data(&execution.store)[output..output + 4].to_vec();
        if deadline {
            cancel.limit_deadline(Instant::now());
        } else {
            cancel.cancel();
        }
        execution
            .resume(&token, Ok(13i32.to_le_bytes().to_vec()))
            .unwrap();
        assert_eq!(&memory.data(&execution.store)[output..output + 4], original);
        assert!(execution.continuation.is_none());
        assert!(execution.store.data().pending.is_none());
        let result = execution.finish();
        assert_eq!(
            result.report.outcome,
            Err(if deadline {
                Fault::Deadline
            } else {
                Fault::Cancelled
            })
        );
        assert_eq!(result.report.host_calls, 2);
        assert!(result.completion.is_none());
    }
}

#[test]
fn cancellation_before_dispatch_and_unanswered_finish_never_complete_task() {
    let cancel = Cancellation::default();
    let mut execution = start(cancel.clone());
    cancel.cancel();
    assert!(execution.pending().is_none());
    assert!(execution.continuation.is_none());
    assert_eq!(execution.finish().report.outcome, Err(Fault::Cancelled));
    let result = start(Cancellation::default()).finish();
    assert_eq!(result.report.outcome, Err(Fault::TaskProtocol));
    assert!(result.completion.is_none());
}

#[test]
fn original_boundary_fault_wins_over_later_cancel() {
    for malformed in [true, false] {
        let cancel = Cancellation::default();
        let mut execution = start(cancel.clone());
        let token = execution.pending().unwrap().token.clone();
        if !malformed {
            cancel.limit_deadline(Instant::now());
        }
        execution.resume(&token, Ok(vec![])).unwrap();
        cancel.cancel();
        assert!(execution.pending().is_none());
        assert_eq!(
            execution.finish().report.outcome,
            Err(if malformed {
                Fault::Trap
            } else {
                Fault::Deadline
            })
        );
    }
}

#[test]
fn same_import_and_fuel_budgets_survive_all_resumptions() {
    let limited = runner(
        GUEST,
        Limits {
            host_calls: 1,
            ..Limits::default()
        },
    );
    let mut execution = Execution::start(&limited, Some(&[1]), Cancellation::default()).unwrap();
    reply(&mut execution, 7);
    assert!(execution.pending().is_none());
    let result = execution.finish();
    assert_eq!(result.report.host_calls, 1);
    assert_eq!(result.report.outcome, Err(Fault::Limits));

    let burning = GUEST.replace(
        "(local.set $ret (call $complete",
        "(loop $burn (br $burn)) (local.set $ret (call $complete",
    );
    assert_ne!(burning, GUEST);
    let limited = runner(
        &burning,
        Limits {
            fuel: 10_000,
            ..Limits::default()
        },
    );
    let mut execution = Execution::start(&limited, Some(&[1]), Cancellation::default()).unwrap();
    reply(&mut execution, 7);
    let token = execution.pending().unwrap().token.clone();
    reply(&mut execution, 13);
    assert!(execution.pending().is_none());
    assert_eq!(
        execution.resume(&token, Ok(vec![0; 4])),
        Err(Fault::TaskProtocol)
    );
    let result = execution.finish();
    assert_eq!(result.report.outcome, Err(Fault::Limits));
    assert_eq!(result.report.host_calls, 2);
    assert!(result.completion.is_none());
}

#[test]
fn synchronous_driver_uses_same_continuation_and_callback_order() {
    use std::cell::RefCell;
    let runner = runner(GUEST, Limits::default());
    let calls = RefCell::new(Vec::new());
    let result = runner.run_task_with_io(
        &[1],
        &mut |bytes| {
            calls.borrow_mut().push(bytes.to_vec());
            Ok(7i32.to_le_bytes().to_vec())
        },
        &mut |bytes| {
            calls.borrow_mut().push(bytes.to_vec());
            Ok(13i32.to_le_bytes().to_vec())
        },
        Cancellation::default(),
    );
    assert_eq!(result.report.outcome, Ok(0));
    assert_eq!(result.completion.as_deref(), Some(b"done".as_slice()));
    assert_eq!(calls.into_inner(), [b"core".to_vec(), b"io".to_vec()]);
    assert_eq!(result.report.host_calls, 2);
}

#[test]
fn paused_real_runner_leaves_original_host_free_for_durable_content_commands() {
    use morrow_core::{content::CardRecord, dispatch::HostRuntime, store::Store as ContentStore};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owner.db");
    let mut owner =
        HostRuntime::new(ContentStore::open(&path, Default::default()).unwrap()).unwrap();
    let identity = owner.binding();
    let mut execution = start(Cancellation::default());
    reply(&mut execution, 7);
    assert_eq!(execution.pending().unwrap().kind, Kind::Io);
    let card = CardRecord::new(
        "paused-runner",
        "note",
        1,
        "created while pending",
        vec![1, 2, 3],
    )
    .unwrap();
    owner
        .store_local_mut()
        .create_local("create-while-pending", &card)
        .unwrap();
    assert_eq!(
        owner
            .store_local()
            .card("paused-runner")
            .unwrap()
            .unwrap()
            .body(),
        [1, 2, 3]
    );
    assert_eq!(owner.binding(), identity);
    reply(&mut execution, 13);
    assert_eq!(execution.finish().report.outcome, Ok(0));
    drop(owner);
    let reopened = ContentStore::open_existing(&path, Default::default()).unwrap();
    assert_eq!(
        reopened.card("paused-runner").unwrap().unwrap().body(),
        [1, 2, 3]
    );
}
