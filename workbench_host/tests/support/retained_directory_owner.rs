//! Original native File retention through actual protected Workbench admission.
//! This does not attest to a native picker or checked worker-side close receipts.
use super::*;
use morrow_workbench_host::io_tasks::directory::DirectoryObjectStart;
use std::os::windows::io::{AsRawHandle, IntoRawHandle, RawHandle};

fn request(f: &LifecycleFixture, submission: u8) -> DirectoryObjectStart {
    DirectoryObjectStart {
        submission: [submission; 32],
        package_id: ID.into(),
        digest: f.package.digest(),
        revision: f.revision,
        handler: HANDLER.into(),
        max_entries: 1024,
        max_metadata_bytes: 1024 * 1024,
        timeout_ms: 10_000,
    }
}

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
    // SAFETY: the original live File is borrowed synchronously; FILE_ID_INFO
    // has exactly 24 writable, correctly aligned bytes in this local array.
    assert_ne!(
        unsafe {
            GetFileInformationByHandleEx(file.as_raw_handle(), 18, id.as_mut_ptr().cast(), 24)
        },
        0
    );
    id
}

fn retained(slot: &Option<File>, path: &Path, raw: RawHandle, id: [u64; 3]) {
    let file = slot.as_ref().expect("original owner retains selection");
    assert_eq!(file.as_raw_handle(), raw);
    assert_eq!(identity(file), id);
    assert_eq!(
        fs::rename(path, path.with_extension("held"))
            .unwrap_err()
            .raw_os_error(),
        Some(32)
    );
}

fn close_original(slot: &mut Option<File>, path: &Path) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: RawHandle) -> i32;
    }
    let raw = slot.take().unwrap().into_raw_handle();
    // SAFETY: the original owner consumes its sole File and attempts one close.
    // A failure is not retried or reinterpreted as retirement.
    assert_ne!(unsafe { CloseHandle(raw) }, 0);
    let moved = path.with_extension("closed");
    fs::rename(path, &moved).unwrap();
    fs::rename(moved, path).unwrap();
}

fn selection(f: &LifecycleFixture) -> (Option<File>, PathBuf, RawHandle, [u64; 3]) {
    let path = f.root.join("child");
    let file = directory(&path, 3).unwrap();
    let raw = file.as_raw_handle();
    let id = identity(&file);
    (Some(file), path, raw, id)
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn invalid_metadata_and_preparation_keep_original_native_selection() {
    let mut f = LifecycleFixture::new();
    let (mut slot, path, raw, id) = selection(&f);
    for case in 0..7 {
        let mut r = request(&f, 101 + case);
        match case {
            0 => r.timeout_ms = 0,
            1 => r.max_entries = 1025,
            2 => r.max_metadata_bytes = 1024 * 1024 + 1,
            3 => r.handler.clear(),
            4 => r.handler = "not.declared".into(),
            5 => r.digest = [0; 32],
            6 => r.package_id = "org.example.not-selected".into(),
            _ => unreachable!(),
        }
        assert!(
            f.workbench
                .start_open_directory_retained(r, &mut slot)
                .is_err()
        );
        retained(&slot, &path, raw, id);
        assert!(f.workbench.io_status().key.is_none());
        assert_eq!(f.workbench.io_status().storage, StoragePhase::Local);
    }
    close_original(&mut slot, &path);
    f.workbench.finish().unwrap();
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn stale_revision_burns_submission_without_taking_original_selection() {
    let mut f = LifecycleFixture::new();
    let (mut slot, path, raw, id) = selection(&f);
    let mut r = request(&f, 111);
    r.revision -= 1;
    assert!(
        f.workbench
            .start_open_directory_retained(r, &mut slot)
            .is_err()
    );
    retained(&slot, &path, raw, id);
    assert!(
        f.workbench
            .start_open_directory_retained(request(&f, 111), &mut slot)
            .is_err()
    );
    retained(&slot, &path, raw, id);
    close_original(&mut slot, &path);
    f.workbench.finish().unwrap();
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn revoked_file_list_approval_keeps_original_selection() {
    let mut f = LifecycleFixture::new();
    let (mut slot, path, raw, id) = selection(&f);
    f.workbench
        .configure_external_io(ID, &f.package.digest(), f.revision, &[])
        .unwrap();
    f.revision = revision(&f.workbench);
    assert!(
        f.workbench
            .start_open_directory_retained(request(&f, 121), &mut slot)
            .is_err()
    );
    retained(&slot, &path, raw, id);
    assert!(f.workbench.io_status().key.is_none());
    close_original(&mut slot, &path);
    f.workbench.finish().unwrap();
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn existing_task_cannot_take_another_original_selection() {
    let mut f = LifecycleFixture::new();
    let r = request(&f, 131);
    let (mut slot, path, raw, id) = selection(&f);
    let key = f.start("child", CaptureLimits::default());
    capture(&mut f.workbench, key);
    assert!(
        f.workbench
            .start_open_directory_retained(r, &mut slot)
            .is_err()
    );
    retained(&slot, &path, raw, id);
    // Retain our original File through the other task's actual worker exit.
    f.workbench.cancel_io(key).unwrap();
    let deadline = Instant::now() + WAIT;
    while f.workbench.poll_io(key).unwrap().storage != StoragePhase::Reclaimed {
        assert!(Instant::now() < deadline, "original owner remains retained");
        thread::sleep(Duration::from_millis(1));
    }
    retained(&slot, &path, raw, id);
    close_original(&mut slot, &path);
    f.finish(key, "other-task-retained-original");
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn product_gate_refusal_after_worker_creation_keeps_original_selection() {
    let mut f = LifecycleFixture::new();
    let (mut slot, path, raw, id) = selection(&f);
    let r = request(&f, 141);
    f.workbench.product_gate().revoke();
    assert!(
        f.workbench
            .start_open_directory_retained(r, &mut slot)
            .is_err()
    );
    retained(&slot, &path, raw, id);
    let key = f
        .workbench
        .io_status()
        .key
        .expect("worker was created before gate refusal");
    let deadline = Instant::now() + WAIT;
    let exit = loop {
        if let Some(exit) = f.workbench.poll_io(key).unwrap().exit {
            break exit;
        }
        assert!(
            Instant::now() < deadline,
            "original worker remains retained"
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    retained(&slot, &path, raw, id);
    close_original(&mut slot, &path);
    f.workbench.acknowledge_io(key).unwrap();
    f.workbench.finish().unwrap();
}

#[test]
#[ignore = "explicit protected Windows qualification with pinned real guest"]
fn accepted_original_selection_transfers_once_and_runs_real_guest() {
    let mut f = LifecycleFixture::new();
    let (mut slot, path, _, _) = selection(&f);
    let key = f
        .workbench
        .start_open_directory_retained(request(&f, 151), &mut slot)
        .unwrap();
    assert!(slot.is_none());
    let selected = capture(&mut f.workbench, key);
    assert_eq!(selected.entries, 70);
    assert_eq!(
        fs::rename(&path, path.with_extension("held"))
            .unwrap_err()
            .raw_os_error(),
        Some(32)
    );
    assert!(
        f.workbench
            .start_open_directory_retained(request(&f, 152), &mut slot)
            .is_err()
    );
    f.workbench
        .request_directory_guest_frame(key, Duration::from_secs(5))
        .unwrap();
    let result = completed(&mut f.workbench, key);
    assert!(!result.unknown);
    assert_eq!(result.execution.outcome, Ok(0));
    assert_eq!(result.execution.host_calls, 4);
    final_page(result.response.as_ref().unwrap(), selected, 70);
    assert!(slot.is_none());
    f.finish(key, "original-file-transferred-once");
}
