//! Host policy only: an intentionally incomplete receipt cannot become a clean
//! owner through ordinary maintenance. Native close evidence is tested on the
//! actual original worker in plugin_runtime's checked_retirement_tests.
use super::*;
use morrow_plugin_runtime::io_jobs::NativeRetirement;

fn recovery<T>(result: Result<T>) {
    match result {
        Err(error) => assert_eq!(
            error.downcast_ref::<AccessError>(),
            Some(&AccessError::RecoveryRequired)
        ),
        Ok(_) => panic!("native ambiguity was cleared by ordinary storage recovery"),
    }
}

#[test]
fn incomplete_native_retirement_retains_owner_and_cannot_be_repaired_or_acknowledged() {
    assert!(std::env::var_os("MORROW_DISPOSABLE_WINDOWS_TEST").is_none());
    let directory = tempfile::tempdir().unwrap();
    let mut app = Workbench::open_managed(directory.path(), None).unwrap();
    let original = app.state.local().unwrap().host.binding();
    let key = TaskKey::from_bytes(&[99; 32]).unwrap();
    app.state.task = Some(Task {
        key,
        commands: Default::default(),
        worker: None,
        handle: None,
        stopping: true,
        exit: None,
        service: None,
        file: None,
        directory: None,
        mutation: None,
    });
    // This default is a failure-only policy fixture, not an actual OS outcome.
    let incomplete = NativeRetirement::default();
    assert!(!incomplete.is_complete());
    app.state.native_retirement = Some(incomplete);
    for _ in 0..2 {
        assert_eq!(app.io_status().storage, StoragePhase::Unavailable);
        recovery(app.state.require_writable());
        recovery(app.state.finish_maintenance());
        recovery(app.acknowledge_io(key));
        recovery(app.repair_io(key));
        recovery(app.state.take_channel_owner());
        assert!(app.state.native_retirement.is_some());
        assert_eq!(app.state.owner.as_ref().unwrap().host.binding(), original);
        assert_eq!(app.state.task.as_ref().unwrap().key, key);
    }
    assert!(app.state.warning().unwrap().contains("关闭结果无法确认"));
}
