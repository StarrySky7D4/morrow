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
