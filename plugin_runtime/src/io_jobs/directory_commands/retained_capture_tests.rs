//! Real Windows directory ownership checks; queue refusal seams are test-only.
//! This is not native picker integration or a full SDK acceptance receipt.
use super::*;
use std::os::windows::io::{AsRawHandle, IntoRawHandle, RawHandle};

fn selected_file(run: &Running) -> (File, PathBuf) {
    let path = run._temp.path().join("original-retained-selection");
    fs::create_dir(&path).unwrap();
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x0220_0000)
        .open(&path)
        .unwrap();
    (file, path)
}

#[allow(unsafe_code)]
fn identity(file: &File) -> [u64; 3] {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandleEx(
            handle: RawHandle,
            class: i32,
            buffer: *mut std::ffi::c_void,
            bytes: u32,
        ) -> i32;
    }
    let mut id = [0u64; 3];
    // SAFETY: original File is borrowed for this synchronous FILE_ID_INFO
    // query. The initialized buffer is aligned and exactly 24 writable bytes.
    assert_ne!(
        unsafe {
            GetFileInformationByHandleEx(file.as_raw_handle(), 18, id.as_mut_ptr().cast(), 24)
        },
        0
    );
    id
}

#[allow(unsafe_code)]
fn checked_close(file: File) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: RawHandle) -> i32;
    }
    let raw = file.into_raw_handle();
    // SAFETY: consuming File transfers its sole owned handle; this is the one
    // close attempt. Failure is not retried or declared successfully closed.
    assert_ne!(unsafe { CloseHandle(raw) }, 0);
}

fn verify_returned_file(file: File, path: &Path, raw: RawHandle, id: [u64; 3]) {
    assert_eq!(file.as_raw_handle(), raw);
    assert_eq!(identity(&file), id);
    assert!(file.metadata().unwrap().is_dir());
    let moved = path.with_extension("after-checked-close");
    assert_eq!(
        fs::rename(path, &moved).unwrap_err().raw_os_error(),
        Some(32)
    );
    checked_close(file);
    fs::rename(path, &moved).unwrap();
    fs::rename(&moved, path).unwrap();
}

#[test]
fn validation_refusal_returns_the_exact_open_selection() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    let limits = CaptureLimits {
        max_job_bytes: 0,
        ..small_limits()
    };
    let (error, returned) = run
        .worker
        .capture_directory_fresh_retained(file, limits)
        .unwrap_err();
    assert_eq!(error, OwnerCommandError::Limit);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(run.worker.owner_command_usage(), (0, 0));
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    verify_returned_file(returned, &path, raw, id);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn stopped_worker_returns_the_exact_open_selection() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    run.worker.stop();
    let (error, returned) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap_err();
    assert_eq!(error, OwnerCommandError::Closed);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    verify_returned_file(returned, &path, raw, id);
    assert_eq!(run.reclaim(false).disconnect, Ok(()));
}

#[test]
fn resident_selection_limit_does_not_discard_the_new_selection() {
    let mut run = Fixture::new().start();
    let sessions: Vec<_> = (1..=MAX_DIRECTORY_OBSERVATIONS)
        .map(|n| run.capture(n as u8).0)
        .collect();
    let before = run.worker.directory_usage();
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    let (error, returned) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap_err();
    assert_eq!(error, OwnerCommandError::Busy);
    assert_eq!(run.worker.directory_usage(), before);
    verify_returned_file(returned, &path, raw, id);
    for session in sessions {
        run.finish(session);
    }
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn owner_command_limit_removes_only_the_unsubmitted_directory_reservation() {
    let mut run = Fixture::new().start();
    let mut blocker = run.block();
    let queued: Vec<_> = (1..crate::io_jobs::MAX_OWNER_COMMANDS)
        .map(|_| run.worker.submit_owner_command(vec![NOOP], 0).unwrap())
        .collect();
    let before = run.worker.owner_command_usage();
    let charged = run.worker.bytes();
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    let (error, returned) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap_err();
    assert_eq!(error, OwnerCommandError::Busy);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(run.worker.owner_command_usage(), before);
    assert_eq!(run.worker.bytes(), charged);
    verify_returned_file(returned, &path, raw, id);
    drop(queued);
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "original barrier completed",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn exhausted_cumulative_budget_returns_the_new_file_without_refunding_prior_work() {
    let mut fixture = Fixture::new();
    let ceiling = SMALL_JOB + DIRECTORY_COMMAND_FIXED_BYTES;
    fixture.limits = JobLimits::new(JobLimits::default().max_calls, ceiling, ceiling).unwrap();
    let mut run = fixture.start();
    let _existing = run.capture(1);
    let before = run.worker.directory_usage();
    assert_eq!(run.worker.bytes(), ceiling);
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    let (error, returned) = run
        .worker
        .capture_directory_fresh_retained(file, small_limits())
        .unwrap_err();
    assert_eq!(error, OwnerCommandError::Limit);
    assert_eq!(run.worker.directory_usage(), before);
    assert_eq!(run.worker.bytes(), ceiling);
    assert_eq!(run.worker.owner_command_usage(), (0, 0));
    verify_returned_file(returned, &path, raw, id);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

fn failed_send_returns_original(disconnected: bool) {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let mut blocker = run.block();
    let before = run.worker.owner_command_usage();
    let charged = run.worker.bytes();
    let (file, path) = selected_file(&run);
    let (raw, id) = (file.as_raw_handle(), identity(&file));
    // The original worker/Control stays intact. A private test-only sender
    // replacement deterministically exercises both failed try_send results.
    let (sender, receiver) = mpsc::sync_channel(0);
    let receiver = if disconnected {
        drop(receiver);
        None
    } else {
        Some(receiver)
    };
    let original = std::mem::replace(&mut run.worker.owner_sender, sender);
    let attempted = run
        .worker
        .capture_directory_fresh_retained(file, small_limits());
    run.worker.owner_sender = original;
    drop(receiver);
    let (error, returned) = attempted.unwrap_err();
    assert_eq!(
        error,
        if disconnected {
            OwnerCommandError::Closed
        } else {
            OwnerCommandError::Busy
        }
    );
    assert_eq!(run.worker.owner_command_usage(), before);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert!(
        run.worker.bytes() > charged,
        "existing cumulative charge is never refunded"
    );
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    verify_returned_file(returned, &path, raw, id);
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "original barrier completed",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn full_send_returns_the_original_file_without_reopening() {
    failed_send_returns_original(false);
}

#[test]
fn disconnected_send_returns_the_original_file_without_reopening() {
    failed_send_returns_original(true);
}

#[test]
fn accepted_capture_stays_on_the_original_worker_and_is_consumed_once() {
    let fixture = Fixture::new();
    let names = fixture.populate(3);
    let mut run = fixture.start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (session, mut handle) = run
        .worker
        .capture_directory_fresh_retained(open_directory(&run.data), small_limits())
        .unwrap();
    let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
        panic!("capture");
    };
    assert_eq!(selected.entries, names.len());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(handle.read().unwrap_err(), OwnerCommandError::Consumed);
    run.finish(session);
    assert_closed(run.worker.finish_directory(session));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}
