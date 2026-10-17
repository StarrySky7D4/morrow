use super::*;
use crate::winutil::to_wide;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::io::FromRawHandle;
use std::ptr;
use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Pipes::{
    CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId};

#[test]
fn a0_strict_release_requires_same_reserved_id_and_retains_on_late_busy_or_poison() {
    use crate::station_broker::{CommitClock,StationBrokerConnectRelease as R};
    let owner=Owner::new();let early=Instant::now();let deadline=early+Duration::from_secs(1);
    let mut clock=CommitClock::scripted([early;8],None);
    assert_eq!(owner.release_startup_reservation_strict(StartupId(7),deadline,&mut clock),R::Missing);
    owner.reserve_for_startup(StartupId(7)).unwrap();
    assert_eq!(owner.release_startup_reservation_strict(StartupId(8),deadline,&mut clock),R::Mismatch);
    assert_eq!(strict_release_ready(Some(StartupId(7)),true,StartupId(7)),Err(R::Active));
    {
        let held=owner.slot.lock().unwrap();
        assert_eq!(owner.release_startup_reservation_strict(StartupId(7),deadline,&mut clock),R::Busy);
        assert_eq!(held.reserved_startup,Some(StartupId(7)));
    }
    assert_eq!(owner.release_startup_reservation_strict(StartupId(7),deadline,
        &mut CommitClock::scripted([early,deadline,deadline,deadline,deadline,deadline,deadline,deadline],None)),R::LateRetained);
    assert_eq!(owner.slot.lock().unwrap().reserved_startup,Some(StartupId(7)));
    assert_eq!(owner.release_startup_reservation_strict(StartupId(7),deadline,&mut clock),R::ReleasedSameStartupId);
    assert_eq!(owner.release_startup_reservation_strict(StartupId(7),deadline,&mut clock),R::Missing);
    let poisoned=Owner::new();poisoned.reserve_for_startup(StartupId(9)).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _held=poisoned.slot.lock().unwrap();panic!("pure injected connect metadata poison");
    })).is_err());
    assert_eq!(poisoned.release_startup_reservation_strict(StartupId(9),deadline,&mut clock),R::Poisoned);
    assert_eq!(poisoned.slot.lock().err().expect("original metadata mutex poisoned")
        .into_inner().reserved_startup,Some(StartupId(9)));
}

fn fixture() -> (File, OwnedHandle) {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(b"owned-pipe-object").unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut raw = 0;
    let process = unsafe { GetCurrentProcess() };
    assert_ne!(
        unsafe {
            DuplicateHandle(
                process,
                process,
                process,
                &mut raw,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        },
        0
    );
    (file, unsafe { OwnedHandle::from_raw_handle(raw as _) })
}

fn create_local_server_pipe(name: &str) -> File {
    let wide = to_wide(name);
    let handle = unsafe {
        CreateNamedPipeW(
            wide.as_ptr(),
            crate::runner_pipe::PIPE_ACCESS_INBOUND,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            ptr::null_mut(),
        )
    };
    assert!(handle != 0 && handle != INVALID_HANDLE_VALUE);
    unsafe { File::from_raw_handle(handle as _) }
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(2)
}

#[test]
fn owned_clone_survives_original_file_close_and_unknown_is_retained() {
    let owner = Owner::new();
    let (original, process) = fixture();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let id = owner
        .begin_with(&original, &process, soon(), move |file| {
            entered_tx.send(()).map_err(io::Error::other)?;
            release_rx.recv().map_err(io::Error::other)?;
            let mut borrowed = file;
            let mut bytes = Vec::new();
            borrowed.read_to_end(&mut bytes)?;
            if bytes != b"owned-pipe-object" {
                return Err(io::Error::other("wrong owned object"));
            }
            Ok(())
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    drop(original);
    let unknown = owner.observe(id, Instant::now()).unwrap();
    // The injected operation is waiting on a channel, not synchronous I/O.
    // ERROR_NOT_FOUND must leave the exact same occupied lease in place.
    assert!(matches!(
        unknown,
        Observation::RetainedUnknown {
            cancel: CancelObservation::Failed(windows_sys::Win32::Foundation::ERROR_NOT_FOUND),
            ..
        }
    ));
    assert_eq!(
        owner.acknowledge_joined(id).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    let (another_file, another_process) = fixture();
    assert_eq!(
        owner
            .begin_with(&another_file, &another_process, soon(), |_| Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    release_tx.send(()).unwrap();
    let joined = owner.observe(id, soon()).unwrap();
    assert!(matches!(
        joined,
        Observation::Joined {
            result: Ok(()),
            within_deadline: false,
            prior_unknown: Some(_)
        }
    ));
    owner.acknowledge_joined(id).unwrap();
}

#[test]
fn timely_join_requires_explicit_ack_and_old_id_cannot_access_new_attempt() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let id = owner.begin_with(&file, &process, soon(), |_| Ok(())).unwrap();
    assert!(matches!(
        owner.observe(id, soon()).unwrap(),
        Observation::Joined {
            result: Ok(()),
            within_deadline: true,
            prior_unknown: None
        }
    ));
    assert_eq!(
        owner
            .begin_with(&file, &process, soon(), |_| Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    owner.acknowledge_joined(id).unwrap();
    let next = owner.begin_with(&file, &process, soon(), |_| Ok(())).unwrap();
    assert_ne!(id, next);
    assert_eq!(
        owner.observe(id, soon()).unwrap_err().kind(),
        io::ErrorKind::NotFound
    );
    assert!(matches!(
        owner.observe(next, soon()).unwrap(),
        Observation::Joined { .. }
    ));
    owner.acknowledge_joined(next).unwrap();
}

#[test]
fn helper_panic_is_joined_error_and_never_connect_success() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let id = owner
        .begin_with(&file, &process, soon(), |_| panic!("injected helper panic"))
        .unwrap();
    assert!(matches!(
        owner.observe(id, soon()).unwrap(),
        Observation::Joined {
            result: Err(_),
            ..
        }
    ));
    owner.acknowledge_joined(id).unwrap();
}

#[test]
fn preexpired_deadline_starts_no_worker_and_preserves_free_slot() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let worker_calls = Arc::clone(&calls);
    assert_eq!(
        owner
            .begin_with(&file, &process, Instant::now(), move |_| {
                worker_calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(owner.slot.try_lock().unwrap().active.is_none());
}

#[test]
fn wait_conversion_never_requests_infinite() {
    assert_eq!(finite_wait_ms(Duration::ZERO), 0);
    assert_eq!(finite_wait_ms(Duration::from_nanos(1)), 1);
    assert_eq!(finite_wait_ms(Duration::from_millis(15_000)), 15_000);
    assert_eq!(finite_wait_ms(Duration::MAX), u32::MAX - 1);
}

#[test]
fn busy_owner_fails_without_waiting_or_spawning() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let _guard = owner.slot.try_lock().unwrap();
    assert!(owner.begin_with(&file, &process, soon(), |_| Ok(())).is_err());
}

struct ExitBlocker {
    entered: mpsc::SyncSender<()>,
    release: mpsc::Receiver<()>,
}

impl Drop for ExitBlocker {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        let _ = self.release.recv();
    }
}

thread_local! {
    static EXIT_BLOCKER: std::cell::RefCell<Option<ExitBlocker>> = const { std::cell::RefCell::new(None) };
}

#[test]
fn result_before_tls_exit_does_not_join_or_release_lease() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let id = owner
        .begin_with(&file, &process, soon(), move |_| {
            EXIT_BLOCKER.with(|blocker| {
                *blocker.borrow_mut() = Some(ExitBlocker {
                    entered: entered_tx,
                    release: release_rx,
                })
            });
            Ok(())
        })
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(matches!(
        owner.observe(id, Instant::now()).unwrap(),
        Observation::RetainedUnknown { .. }
    ));
    assert_eq!(
        owner.acknowledge_joined(id).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    release_tx.send(()).unwrap();
    assert!(matches!(
        owner.observe(id, soon()).unwrap(),
        Observation::Joined {
            result: Ok(()),
            within_deadline: false,
            prior_unknown: Some(_),
        }
    ));
    owner.acknowledge_joined(id).unwrap();
}

#[test]
fn real_pipe_connect_verifies_client_pid_and_handles_already_connected_and_mismatch() {
    let owner = Owner::new();
    let (_, process) = fixture();
    let self_pid = unsafe { GetCurrentProcessId() };

    let (pipe_ok_name, pipe_bad_name) = crate::runner_pipe::pipe_pair();
    let server_ok = create_local_server_pipe(&pipe_ok_name);
    // Open client before ConnectNamedPipe to exercise ERROR_PIPE_CONNECTED + PID check.
    let _client_ok = OpenOptions::new().write(true).open(&pipe_ok_name).unwrap();
    let id_ok = owner
        .begin_with(&server_ok, &process, soon(), move |owned_pipe| {
            crate::runner_pipe::connect_pipe(owned_pipe.as_raw_handle() as HANDLE, self_pid)
        })
        .unwrap();
    assert!(matches!(
        owner.observe(id_ok, soon()).unwrap(),
        Observation::Joined {
            result: Ok(()),
            within_deadline: true,
            prior_unknown: None,
        }
    ));
    owner.acknowledge_joined(id_ok).unwrap();

    // Wrong client PID must fail with PermissionDenied and preserve JoinedErrorMeta.
    let server_bad = create_local_server_pipe(&pipe_bad_name);
    let _client_bad = OpenOptions::new().write(true).open(&pipe_bad_name).unwrap();
    let id_bad = owner
        .begin_with(&server_bad, &process, soon(), move |owned_pipe| {
            crate::runner_pipe::connect_pipe(
                owned_pipe.as_raw_handle() as HANDLE,
                self_pid.wrapping_add(1),
            )
        })
        .unwrap();
    let (obs, meta) = owner.observe_detailed(id_bad, soon()).unwrap();
    assert!(matches!(
        obs,
        Observation::Joined {
            result: Err(_),
            within_deadline: true,
            prior_unknown: None,
        }
    ));
    assert_eq!(meta.unwrap().kind, io::ErrorKind::PermissionDenied);
    owner.acknowledge_joined(id_bad).unwrap();
}

#[test]
fn startup_lane_reservation_rejects_other_startups_until_explicit_release() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let s1 = StartupId(10);
    let s2 = StartupId(20);
    owner.reserve_for_startup(s1).unwrap();
    assert_eq!(
        owner.reserve_for_startup(s2).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        owner
            .begin_with_for_startup(Some(s2), &file, &process, soon(), |_| Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    let id = owner
        .begin_with_for_startup(Some(s1), &file, &process, soon(), |_| Ok(()))
        .unwrap();
    assert_eq!(
        owner.release_startup_reservation(s1).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert!(matches!(
        owner.observe(id, soon()).unwrap(),
        Observation::Joined {
            result: Ok(()),
            within_deadline: true,
            prior_unknown: None,
        }
    ));
    owner.acknowledge_joined(id).unwrap();
    owner.release_startup_reservation(s1).unwrap();
    owner.reserve_for_startup(s2).unwrap();
    owner.release_startup_reservation(s2).unwrap();
}

#[test]
fn expired_observation_deadline_latches_unknown_even_when_worker_already_signaled() {
    let owner = Owner::new();
    let (file, process) = fixture();
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let id = owner
        .begin_with(&file, &process, soon(), move |_| {
            let _ = done_tx.send(());
            Ok(())
        })
        .unwrap();
    done_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    thread::sleep(Duration::from_millis(20));
    let obs = owner.observe(id, Instant::now()).unwrap();
    assert!(matches!(
        obs,
        Observation::Joined {
            within_deadline: false,
            prior_unknown: Some((UnknownReason::Deadline, _)),
            ..
        }
    ));
    owner.acknowledge_joined(id).unwrap();
}
