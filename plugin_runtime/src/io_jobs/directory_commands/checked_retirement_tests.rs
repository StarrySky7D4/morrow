//! Production owner-worker routes over real Windows directory objects.
//! Lost-result injection is synthetic; it follows one actual successful close.
//! No protected legacy account setup, native picker or release claim is made.
use super::*;
use crate::directory_io::{DirectoryRelativePath, DirectoryRetirementSnapshot};

fn exclusive_selection(run: &Running, name: &str) -> (File, PathBuf) {
    let path = run._temp.path().join(name);
    fs::create_dir(&path).unwrap();
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0220_0000)
        .open(&path)
        .unwrap();
    (file, path)
}

fn held(path: &Path) {
    assert_eq!(
        fs::rename(path, path.with_extension("moved"))
            .unwrap_err()
            .raw_os_error(),
        Some(32)
    );
}

fn released(path: &Path) {
    let moved = path.with_extension("moved");
    fs::rename(path, &moved).unwrap();
    fs::rename(&moved, path).unwrap();
}

fn closed(handle: &DirectoryCommandHandle, objects: usize) {
    assert_eq!(
        handle.retirement_snapshot(),
        DirectoryRetirementSnapshot {
            objects,
            close_calls: objects,
            confirmed_closed: objects,
            unknown: 0,
            worker_bound: true, picker_started: false, picker_closed: false,
        }
    );
    assert!(handle.retirement_snapshot().all_closed());
}

fn capture(
    run: &Running,
    name: &str,
) -> (
    DirectorySession,
    SelectedDirectory,
    DirectoryCommandHandle,
    PathBuf,
) {
    let (file, path) = exclusive_selection(run, name);
    held(&path);
    let (session, mut handle) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap();
    let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
        panic!("capture");
    };
    assert_eq!(handle.retirement_snapshot().objects, 1);
    assert_eq!(handle.retirement_snapshot().close_calls, 0);
    held(&path);
    (session, selected, handle, path)
}

#[test]
fn finish_and_terminal_page_observe_original_native_close_before_success() {
    for terminal_page in [false, true] {
        let mut run = Fixture::new().start();
        let (session, selected, capture, path) = capture(&run, "finish-or-terminal");
        let mut completion = if terminal_page {
            run.worker
                .next_directory_page(session, selected.first_page())
                .unwrap()
        } else {
            run.worker.finish_directory(session).unwrap()
        };
        let response = result(&mut completion).unwrap().unwrap();
        if terminal_page {
            assert!(matches!(response, DirectoryResponse::Page(page) if page.terminal));
        } else {
            assert!(matches!(response, DirectoryResponse::Finished));
        }
        closed(&completion, 1);
        closed(&capture, 1);
        released(&path);
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
        assert_eq!(run.worker.directory_usage(), (0, 0));
        assert_closed(run.worker.finish_directory(session));
        assert_eq!(run.reclaim(true).result, Ok(()));
        closed(&capture, 1);
    }
}

#[test]
fn queued_cancel_and_exit_close_input_on_original_worker_without_capture() {
    for exit in [false, true] {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let barrier = run.block();
        let (file, path) = exclusive_selection(&run, "queued");
        let (_, handle) = run
            .worker
            .capture_directory_fresh_retained(file, small_limits())
            .unwrap();
        assert_eq!(
            handle.retirement_snapshot(),
            DirectoryRetirementSnapshot::default()
        );
        held(&path);
        if exit {
            run.worker.stop();
        } else {
            handle.cancel();
        }
        run.gate.release();
        if exit {
            assert_eq!(run.reclaim(false).result, Ok(()));
        } else {
            wait_for(
                || handle.retirement_snapshot().all_closed(),
                "queued native retirement",
            );
            idle_empty(&run);
            assert_eq!(run.reclaim(true).result, Ok(()));
        }
        closed(&handle, 1);
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert!(!handle.is_started());
        released(&path);
        drop(barrier);
    }
}

#[test]
fn unread_delivery_cancellation_revocation_expiry_and_exit_retire_original_objects() {
    for route in 0..4 {
        let mut run = Fixture::new().start();
        let (file, path) = exclusive_selection(&run, "idle-retirement");
        let (_, mut handle) = run
            .worker
            .capture_directory_fresh_retained(file, small_limits())
            .unwrap();
        ready(&handle);
        held(&path);
        if route != 0 {
            assert!(matches!(
                result(&mut handle),
                Ok(Ok(DirectoryResponse::Captured(_)))
            ));
        }
        match route {
            0 => handle.cancel(),
            1 => run.revocation.revoke(),
            2 => run.clock.store(9000, Ordering::SeqCst),
            3 => run.worker.stop(),
            _ => unreachable!(),
        }
        wait_for(
            || handle.retirement_snapshot().all_closed(),
            "resident native retirement",
        );
        idle_empty(&run);
        closed(&handle, 1);
        released(&path);
        run.reclaim(true);
        closed(&handle, 1);
    }
}

#[test]
fn partial_descendant_failure_closes_opened_ancestors_and_refunds_only_capacity() {
    let mut run = Fixture::new().start();
    let (anchor, path) = exclusive_selection(&run, "partial");
    fs::create_dir(path.join("first")).unwrap();
    let relative = DirectoryRelativePath::new(vec![
        "first".encode_utf16().collect(),
        "missing".encode_utf16().collect(),
    ])
    .unwrap();
    let before = run.worker.directory_binding_usage().unwrap().bytes;
    let (_, mut handle) = run
        .worker
        .capture_directory_under(anchor, relative, small_limits())
        .unwrap();
    assert!(matches!(
        result(&mut handle),
        Ok(Err(DirectoryCommandError::Io(std::io::ErrorKind::NotFound)))
    ));
    closed(&handle, 2);
    idle_empty(&run);
    assert!(run.worker.directory_binding_usage().unwrap().bytes > before);
    released(&path);
    released(&path.join("first"));
    assert_eq!(run.reclaim(true).result, Ok(()));
}

#[test]
fn worker_unwind_retires_resident_and_accepted_queued_inputs_before_exit() {
    let mut run = Fixture::new().start();
    let (_, _, resident, resident_path) = capture(&run, "resident-before-unwind");
    let barrier = run.block();
    let panic_command = run.worker.submit_owner_command(vec![255], 0).unwrap();
    let (file, path) = exclusive_selection(&run, "queued-before-unwind");
    let (_, queued) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap();
    held(&path);
    run.gate.release();
    let exit = run.reclaim(false);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    closed(&resident, 1);
    closed(&queued, 1);
    assert!(!queued.is_started());
    released(&resident_path);
    released(&path);
    drop((barrier, panic_command));
}

#[test]
fn synthetic_lost_close_result_never_reports_terminal_success_or_releases_capacity() {
    for terminal_page in [false, true] {
        let mut run = Fixture::new().start();
        let (session, selected, capture, path) = capture(&run, "lost-close");
        // This private seam loses an actual successful CloseHandle observation.
        // It does not manufacture a Windows error or retry a native call.
        capture.ledger.lose_next_close_success();
        let mut completion = if terminal_page {
            run.worker
                .next_directory_page(session, selected.first_page())
                .unwrap()
        } else {
            run.worker.finish_directory(session).unwrap()
        };
        assert!(matches!(
            result(&mut completion),
            Err(OwnerCommandError::Unknown)
        ));
        let exit = reclaim(&mut run.worker);
        assert_eq!(exit.result, Err(JobError::Unavailable));
        assert!(!exit.native_retirement.is_complete());
        assert_eq!(
            exit.native_retirement.directory_snapshots(),
            vec![completion.retirement_snapshot()]
        );
        let snapshot = completion.retirement_snapshot();
        assert_eq!(
            snapshot,
            DirectoryRetirementSnapshot {
                objects: 1,
                close_calls: 1,
                confirmed_closed: 0,
                unknown: 1,
                worker_bound: true, picker_started: false, picker_closed: false,
            }
        );
        assert!(!snapshot.all_closed());
        assert_eq!(run.worker.directory_usage(), (1, 0));
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 1);
        assert_closed(run.worker.finish_directory(session));
        // The real OS close happened, but its intentionally lost receipt must
        // not be inferred from a later path operation or turn into a refund.
        released(&path);
        run.worker.stop();
        assert_eq!(completion.retirement_snapshot(), snapshot);
        assert_eq!(capture.retirement_snapshot(), snapshot);
    }
}

#[test]
fn synthetic_lost_partial_close_keeps_every_original_resource_lease() {
    let mut run = Fixture::new().start();
    let barrier = run.block();
    let (anchor, path) = exclusive_selection(&run, "lost-partial");
    fs::create_dir(path.join("first")).unwrap();
    let relative = DirectoryRelativePath::new(vec![
        "first".encode_utf16().collect(),
        "missing".encode_utf16().collect(),
    ])
    .unwrap();
    let (_, mut handle) = run
        .worker
        .capture_directory_under(anchor, relative, small_limits())
        .unwrap();
    handle.ledger.lose_next_close_success();
    run.gate.release();
    assert!(matches!(
        result(&mut handle),
        Err(OwnerCommandError::Unknown)
    ));
    let exit = reclaim(&mut run.worker);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    assert!(!exit.native_retirement.is_complete());
    assert_eq!(
        exit.native_retirement.directory_snapshots(),
        vec![handle.retirement_snapshot()]
    );
    assert_eq!(
        handle.retirement_snapshot(),
        DirectoryRetirementSnapshot {
            objects: 2,
            close_calls: 2,
            confirmed_closed: 1,
            unknown: 1,
            worker_bound: true, picker_started: false, picker_closed: false,
        }
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 3);
    assert_eq!(run.worker.directory_usage(), (1, 0));
    released(&path);
    released(&path.join("first"));
    drop(barrier);
}

#[test]
fn synthetic_lost_queued_close_fails_worker_even_without_a_native_capture() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let barrier = run.block();
    let (file, path) = exclusive_selection(&run, "lost-queued");
    let (_, handle) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap();
    handle.ledger.lose_next_close_success();
    run.worker.stop();
    run.gate.release();
    let exit = reclaim(&mut run.worker);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    assert!(!exit.native_retirement.is_complete());
    assert_eq!(
        exit.native_retirement.directory_snapshots(),
        vec![handle.retirement_snapshot()]
    );
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.worker.directory_usage(), (1, 0));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    assert_eq!(handle.retirement_snapshot().close_calls, 1);
    assert_eq!(handle.retirement_snapshot().unknown, 1);
    released(&path);
    drop(barrier);
}

#[test]
fn native_retirement_does_not_lock_ui_status_admission_or_original_clock() {
    let mut run = Fixture::new().start();
    let (session, _, capture, path) = capture(&run, "slow-close");
    let gate = Arc::new(Gate::default());
    let release = ReleaseEntropyOnDrop(Arc::clone(&gate));
    let (entered, observed) = mpsc::channel();
    let blocked = Arc::clone(&gate);
    capture.ledger.before_close(Box::new(move || {
        entered.send(()).unwrap();
        blocked.wait();
    }));
    let mut finish = run.worker.finish_directory(session).unwrap();
    observed.recv_timeout(WAIT).unwrap();
    // This seam holds the actual original native-record lock just before the
    // actual close. UI-facing reads and original guards must remain available.
    let (responsive, joined) = thread::scope(|scope| {
        let (done, received) = mpsc::channel();
        let worker = &run.worker;
        let pending = &mut finish;
        let read = scope.spawn(move || {
            assert!(!pending.retirement_snapshot().all_closed());
            assert!(matches!(pending.read(), Ok(None)));
            assert_eq!(worker.directory_usage(), (1, 0));
            let control_guard = worker
                .control
                .state
                .try_lock()
                .expect("Control lock held by native close");
            drop(control_guard);
            let clock_guard = worker
                .control
                .authority
                .as_ref()
                .unwrap()
                .clock
                .try_lock()
                .expect("clock held by native close");
            drop(clock_guard);
            done.send(()).unwrap();
        });
        let responsive = received.recv_timeout(Duration::from_secs(1));
        gate.release();
        (responsive, read.join())
    });
    responsive.unwrap();
    joined.unwrap();
    drop(release);
    assert!(matches!(
        result(&mut finish),
        Ok(Ok(DirectoryResponse::Finished))
    ));
    closed(&capture, 1);
    released(&path);
    assert_eq!(run.reclaim(true).result, Ok(()));
}
