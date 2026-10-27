//! Real Windows refuses CloseHandle after protection of a newly owned fixture.
//! No synthetic lost-observation hook, legacy setup, picker or release claim.
use super::*;
use crate::directory_io::DirectoryRetirementSnapshot;

fn protected_file(run: &Running, name: &str) -> File {
    assert!(run._temp.path().starts_with(Path::new("D:/build_cache")));
    let path = run._temp.path().join(name);
    fs::create_dir(&path).unwrap();
    let file = OpenOptions::new().read(true).share_mode(3).custom_flags(0x0220_0000).open(&path).unwrap();
    assert_eq!(fs::rename(&path, path.with_extension("moved")).unwrap_err().raw_os_error(), Some(32));
    crate::directory_io::protect_close_fixture_for_test(&file).unwrap();
    file
}

fn refused(handle: &DirectoryCommandHandle) -> DirectoryRetirementSnapshot {
    let snapshot = handle.retirement_snapshot();
    assert_eq!(snapshot, DirectoryRetirementSnapshot {
        objects: 1, close_calls: 1, confirmed_closed: 0, unknown: 1, worker_bound: true, picker_started: false, picker_closed: false,
    });
    assert!(!snapshot.all_closed());
    snapshot
}

#[test]
fn actual_close_refusal_finish_and_terminal_page_keep_original_charge() {
    for terminal_page in [false, true] {
        let mut run = Fixture::new().start();
        let original = protected_file(&run, "retained-original");
        let (session, mut capture) = run.worker.capture_directory_fresh_retained(original, small_limits()).unwrap();
        let DirectoryResponse::Captured(selected) = result(&mut capture).unwrap().unwrap() else { panic!("capture"); };
        assert_eq!(capture.retirement_snapshot().close_calls, 0);
        let mut completion = if terminal_page {
            run.worker.next_directory_page(session, selected.first_page()).unwrap()
        } else { run.worker.finish_directory(session).unwrap() };
        assert!(matches!(result(&mut completion), Err(OwnerCommandError::Unknown)));
        let exit = reclaim(&mut run.worker);
        assert_eq!(exit.result, Err(JobError::Unavailable));
        assert!(!exit.native_retirement.is_complete());
        let snapshot = refused(&completion);
        assert_eq!(exit.native_retirement.directory_snapshots(), vec![snapshot]);
        assert_eq!(capture.retirement_snapshot(), snapshot);
        assert_eq!(run.worker.directory_usage(), (1, 0));
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 1);
        assert_closed(run.worker.finish_directory(session));
        assert_eq!(completion.retirement_snapshot(), snapshot);
        // Deliberately retain this entire failed allocation. Harness exit later
        // is not a checked close, successful owner completion or quota refund.
        std::mem::forget((run, exit, capture, completion));
    }
}

#[test]
fn actual_close_refusal_of_queued_input_blocks_worker_retirement_before_capture() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let barrier = run.block();
    let original = protected_file(&run, "queued-original");
    let (_, queued) = run.worker.capture_directory_fresh_retained(original, small_limits()).unwrap();
    assert_eq!(queued.retirement_snapshot(), DirectoryRetirementSnapshot::default());
    run.worker.stop();
    run.gate.release();
    let exit = reclaim(&mut run.worker);
    assert_eq!(exit.result, Err(JobError::Unavailable));
    assert!(!exit.native_retirement.is_complete());
    let snapshot = refused(&queued);
    assert_eq!(exit.native_retirement.directory_snapshots(), vec![snapshot]);
    assert!(!queued.is_started());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.worker.directory_usage(), (1, 0));
    std::mem::forget((run, exit, queued, barrier));
}
