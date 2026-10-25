// Included by directory_protected_sdk: use its actual protected owner fixtures.
use morrow_workbench_host::{host_capnp as private, protocol as private_protocol};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

#[derive(Debug)]
struct PrivateDirectoryReply {
    error: String,
    key: Option<TaskKey>,
    submission: Vec<u8>,
    storage: u16,
    has_exit: bool,
    exit: (u16, u16, u16),
    kind: u16,
    entries: u32,
    metadata: u64,
    frame: Vec<u8>,
    unknown: bool,
    fault: u16,
    code: i32,
    calls: u32,
}
fn private_frame(
    action: private::Action,
    fill: impl FnOnce(private::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut r = message.init_root::<private::request::Builder>();
    r.set_version(1);
    r.set_digest(&private_protocol::digest());
    r.set_action(action);
    fill(r);
    capnp::serialize::write_message_to_words(&message)
}
fn private_exchange(w: &mut Workbench, frame: &[u8]) -> PrivateDirectoryReply {
    let bytes = private_protocol::respond(w, frame).unwrap();
    assert!(bytes.len() <= 128 * 1024);
    let mut remaining = bytes.as_slice();
    let message = capnp::serialize::read_message_from_flat_slice(
        &mut remaining,
        capnp::message::ReaderOptions {
            traversal_limit_in_words: Some(32768),
            nesting_limit: 20,
        },
    )
    .unwrap();
    assert!(remaining.is_empty());
    let r = message.get_root::<private::response::Reader>().unwrap();
    assert_eq!(r.get_version(), 1);
    assert_eq!(r.get_digest().unwrap(), private_protocol::digest());
    let io = r.get_io_state().unwrap();
    let key = io.get_key().unwrap();
    let directory = r.get_directory_result().unwrap();
    PrivateDirectoryReply {
        error: r.get_error().unwrap().to_str().unwrap().into(),
        key: if key.is_empty() {
            None
        } else {
            Some(TaskKey::from_bytes(key).unwrap())
        },
        submission: io.get_submission().unwrap().to_vec(),
        storage: io.get_storage(),
        has_exit: io.get_has_exit(),
        exit: (
            io.get_execution(),
            io.get_disconnect(),
            io.get_maintenance(),
        ),
        kind: directory.get_kind(),
        entries: directory.get_entries(),
        metadata: directory.get_metadata_bytes(),
        frame: directory.get_frame().unwrap().to_vec(),
        unknown: directory.get_unknown(),
        fault: directory.get_execution_fault(),
        code: directory.get_exit_code(),
        calls: directory.get_host_calls(),
    }
}
fn private_key(w: &mut Workbench, action: private::Action, key: TaskKey) -> PrivateDirectoryReply {
    private_exchange(
        w,
        &private_frame(action, |r| {
            let mut r = r;
            r.set_io_key(key.as_bytes());
            r.set_directory_timeout_ms(5000);
        }),
    )
}
fn private_start_frame(f: &LifecycleFixture, submission: u8, path: &Path) -> Vec<u8> {
    private_frame(private::Action::DirectoryStart, |r| {
        let mut start = r.init_directory_start();
        start.set_submission(&[submission; 32]);
        start.set_package_id(ID);
        start.set_package_digest(&f.package.digest());
        start.set_registry_revision(f.revision);
        start.set_handler(HANDLER);
        start.set_max_entries(1024);
        start.set_max_metadata_bytes(1024 * 1024);
        start.set_timeout_ms(10_000);
        let units: Vec<_> = path.as_os_str().encode_wide().collect();
        let mut out = start.init_selected_path_utf16(units.len() as u32);
        for (i, unit) in units.into_iter().enumerate() {
            out.set(i as u32, unit);
        }
    })
}
fn private_read(w: &mut Workbench, key: TaskKey, guest: bool) -> PrivateDirectoryReply {
    let deadline = Instant::now() + WAIT;
    loop {
        let r = private_key(
            w,
            if guest {
                private::Action::DirectoryReadGuest
            } else {
                private::Action::DirectoryRead
            },
            key,
        );
        assert!(r.error.is_empty(), "{r:?}");
        assert_eq!(r.key, Some(key));
        if r.kind != 0 {
            return r;
        }
        assert!(
            Instant::now() < deadline,
            "original private result retained; no retry"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn private_reclaim(f: &mut LifecycleFixture, key: TaskKey) {
    let deadline = Instant::now() + WAIT;
    loop {
        let r = private_key(&mut f.workbench, private::Action::IoPoll, key);
        assert!(r.error.is_empty(), "{r:?}");
        assert_eq!(r.key, Some(key));
        if r.storage == 3 {
            assert!(r.has_exit);
            assert_eq!(r.exit, (0, 0, 0));
            break;
        }
        assert!(r.storage == 1 || r.storage == 2, "{r:?}");
        assert!(Instant::now() < deadline, "original owner not reclaimed");
        thread::sleep(Duration::from_millis(1));
    }
    drop(directory(&f.temp, 0).unwrap());
    drop(directory(&f.root, 0).unwrap());
    let r = private_key(&mut f.workbench, private::Action::IoAcknowledge, key);
    assert!(r.error.is_empty(), "{r:?}");
    assert_eq!(r.storage, 0);
    assert_eq!(r.key, None);
    assert!(
        !private_key(&mut f.workbench, private::Action::IoPoll, key)
            .error
            .is_empty()
    );
}
fn private_close(mut f: LifecycleFixture, key: TaskKey, label: &str) {
    assert_eq!(f.workbench.io_status().storage, StoragePhase::Local);
    assert!(Workbench::open_managed(&f.library, None).is_err());
    assert_eq!(revision(&f.workbench), f.revision);
    f.workbench.finish().unwrap();
    drop(f.workbench);
    let mut reopened = Workbench::open_managed(&f.library, None).unwrap();
    assert!(entry(&reopened).directory_supported);
    assert_eq!(entry(&reopened).approved_io, ["file-list"]);
    assert_eq!(entry(&reopened).digest, f.package.digest());
    assert_eq!(revision(&reopened), f.revision);
    assert!(reopened.poll_io(key).is_err());
    assert_eq!(
        fs::read(f.root.join("outside-selection.txt")).unwrap(),
        b"original outside bytes"
    );
    reopened.finish().unwrap();
    drop(reopened);
    eprintln!(
        "PRIVATE_DIRECTORY_PASS case={label} capnp=true original_owner_reclaimed=true independent_reopen=true"
    );
}
fn private_guest_roundtrip(language: &str) {
    let mut f = LifecycleFixture::new_language(language);
    let start = private_start_frame(&f, 91, &f.root.join("child"));
    let r = private_exchange(&mut f.workbench, &start);
    assert!(r.error.is_empty(), "{r:?}");
    let key = r.key.unwrap();
    assert_eq!(r.submission, [91; 32]);
    assert!(!private_exchange(&mut f.workbench, &start).error.is_empty());
    let captured = private_read(&mut f.workbench, key, false);
    assert_eq!(captured.kind, 1);
    assert_eq!(captured.entries, 70);
    assert!(captured.metadata > 0);
    assert!(captured.frame.is_empty());
    assert!(directory(&f.temp, 0).is_err());
    assert!(
        !private_key(&mut f.workbench, private::Action::DirectoryRead, key)
            .error
            .is_empty()
    );
    let wrong = TaskKey::from_bytes(&[99; 32]).unwrap();
    assert!(
        !private_key(&mut f.workbench, private::Action::DirectoryRunGuest, wrong)
            .error
            .is_empty()
    );
    assert!(
        private_key(&mut f.workbench, private::Action::DirectoryRunGuest, key)
            .error
            .is_empty()
    );
    assert!(
        !private_key(&mut f.workbench, private::Action::DirectoryRunGuest, key)
            .error
            .is_empty()
    );
    let guest = private_read(&mut f.workbench, key, true);
    assert_eq!(guest.kind, 2);
    assert!(!guest.unknown);
    assert_eq!((guest.fault, guest.code, guest.calls), (0, 0, 4));
    let mut raw = guest.frame.as_slice();
    let msg =
        capnp::serialize::read_message(&mut raw, capnp::message::ReaderOptions::new()).unwrap();
    let reply = msg.get_root::<RawResponse<'_>>().unwrap().0;
    let epoch = reply
        .get_pointer_field(5)
        .get_data(None)
        .unwrap()
        .try_into()
        .unwrap();
    // The private channel intentionally does not export a cursor/grant. Exact
    // page shape/names are independently inspected; original binding is checked
    // by production dispatch and the separate original-owner tests.
    final_page(
        &guest.frame,
        SelectedDirectory {
            reference: [0; 32],
            selection_epoch: epoch,
            entries: 70,
            metadata_bytes: captured.metadata,
        },
        70,
    );
    assert!(
        !private_key(&mut f.workbench, private::Action::DirectoryReadGuest, key)
            .error
            .is_empty()
    );
    private_reclaim(&mut f, key);
    private_close(f, key, language);
}
#[test]
#[ignore = "explicit real Windows Cap'n Proto directory qualification"]
fn private_directory_rust_guest_roundtrip() {
    private_guest_roundtrip("RUST");
}
#[test]
#[ignore = "explicit real Windows Cap'n Proto directory qualification"]
fn private_directory_c_guest_roundtrip() {
    private_guest_roundtrip("C");
}
#[test]
#[ignore = "explicit real Windows Cap'n Proto directory qualification"]
fn private_directory_cpp_guest_roundtrip() {
    private_guest_roundtrip("CPP");
}

#[test]
#[ignore = "explicit real Windows Cap'n Proto directory qualification"]
fn private_directory_lost_start_is_observed_and_consumed_capture_is_not_replayed() {
    let mut f = LifecycleFixture::new();
    let start = private_start_frame(&f, 92, &f.root.join("child"));
    // Only discard our own successful start reply. Status inspects the retained
    // original task; no START is sent again to recover it.
    let discarded = private_protocol::respond(&mut f.workbench, &start).unwrap();
    drop(discarded);
    let r = private_exchange(
        &mut f.workbench,
        &private_frame(private::Action::IoStatus, |_| {}),
    );
    assert!(r.error.is_empty(), "{r:?}");
    assert_eq!(r.submission, [92; 32]);
    let key = r.key.unwrap();
    let capture = private_read(&mut f.workbench, key, false);
    assert_eq!(capture.kind, 1);
    drop(capture);
    assert!(
        !private_key(&mut f.workbench, private::Action::DirectoryRead, key)
            .error
            .is_empty()
    );
    assert!(
        private_key(&mut f.workbench, private::Action::IoCancel, key)
            .error
            .is_empty()
    );
    private_reclaim(&mut f, key);
    assert!(!private_exchange(&mut f.workbench, &start).error.is_empty());
    private_close(f, key, "lost-start-consumed-capture");
}

#[test]
#[ignore = "explicit raw Windows UTF-16 native directory qualification"]
fn private_directory_preserves_unpaired_utf16_selection_and_explicit_finish() {
    let mut f = LifecycleFixture::new();
    let spelling = std::ffi::OsString::from_wide(&[0x7528, 0x6237, 0xd800, 0x76ee, 0x5f55]);
    let selected = f.root.join(spelling);
    fs::rename(f.root.join("child"), &selected).unwrap();
    let frame = private_start_frame(&f, 93, &selected);
    let r = private_exchange(&mut f.workbench, &frame);
    assert!(r.error.is_empty(), "{r:?}");
    let key = r.key.unwrap();
    let captured = private_read(&mut f.workbench, key, false);
    assert_eq!(captured.entries, 70);
    assert!(directory(&selected, 0).is_err());
    assert!(
        private_key(&mut f.workbench, private::Action::DirectoryFinish, key)
            .error
            .is_empty()
    );
    assert_eq!(private_read(&mut f.workbench, key, false).kind, 3);
    private_reclaim(&mut f, key);
    drop(directory(&selected, 0).unwrap());
    fs::rename(selected, f.root.join("child")).unwrap();
    private_close(f, key, "raw-utf16-explicit-finish");
}
