//! Selected synthetic libraries only. Never run the broader host/native suite.
//! Visible cases require the one original native chooser; no injected selection.
#![cfg(windows)]
#![deny(warnings)]
use morrow_audit::session::{OpenMode, Session};
use morrow_core::plugin_package::Package;
use morrow_plugin_runtime::fs_watch_worker::Error;
use morrow_workbench_host::{
    Workbench,
    io_tasks::{StoragePhase, TaskKey, fs_watch::WatchStart},
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.protected.watch";
fn revision(w: &Workbench) -> u64 {
    w.catalog_page("", None).unwrap().revision
}
fn fixture(name: &str, language: &str) -> (Workbench, Package, PathBuf) {
    let base = PathBuf::from(std::env::var_os("MORROW_WATCH_PROTECTED_FIXTURES").unwrap());
    assert!(base.is_absolute() && base.starts_with(r"D:\build_cache"));
    assert!(
        !base
            .components()
            .any(|p| matches!(p, std::path::Component::ParentDir))
    );
    let root = base.join(name);
    fs::create_dir(&root).expect("new synthetic fixture; never replay");
    fs::create_dir(root.join("observed")).unwrap();
    let wasm_root = PathBuf::from(std::env::var_os("MORROW_WATCH_SDK_MODULES").unwrap());
    let bytes = fs::read(wasm_root.join(format!("watch-{language}.wasm"))).unwrap();
    let expected = match language {
        "rust" => "b47b4db04dfa1a7cc5a32764c6bd1b476ca8cba96fa8b8edb1e971b05801cf28",
        "c" => "e1797f06cdaa3444838e8153f408486d80723064cda5fbcd7397033cba2c0133",
        "cpp" => "508005f9ef91d5788a7d2cdd5f15581590051a829ecfd261c312c12f19572df6",
        _ => panic!("unqualified SDK language"),
    };
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected);
    let p = Package::build(
        Package::manifest_for_fs_watch(ID, "1.0.0", &bytes, vec!["observe".into()]),
        &bytes,
    )
    .unwrap();
    let incoming = root.join("incoming.mplugin");
    fs::write(&incoming, p.archive()).unwrap();
    let mut w = Workbench::open(&root.join("original.db"), None).unwrap();
    w.import_plugin(&incoming, &p.digest(), revision(&w))
        .unwrap();
    w.configure_external_io(ID, &p.digest(), revision(&w), &["file-list".into()])
        .unwrap();
    w.configure_external(ID, &p.digest(), revision(&w), &[], true)
        .unwrap();
    (w, p, root)
}
fn options(w: &Workbench, p: &Package, root: &Path, submission: u8) -> WatchStart {
    WatchStart {
        submission: [submission; 32],
        package_id: ID.into(),
        digest: p.digest(),
        revision: revision(w),
        handler: "observe".into(),
        nominated_path: root.join("observed"),
        max_frames: 2,
        max_wire_bytes: 512 * 1024,
        window_ms: 29_000,
    }
}
fn approve(w: &mut Workbench, p: &Package) {
    w.configure_external_fs_watch(ID, &p.digest(), revision(w), true)
        .unwrap();
}
fn verify_sealed(root: &Path) {
    // A new read is permitted only after the actual original owner finished.
    let s = Session::open(
        &root.join("original.db"),
        Default::default(),
        OpenMode::Existing,
    )
    .unwrap();
    s.runtime_ref().store_local().integrity_check().unwrap();
    drop(s);
}
fn finish(w: &mut Workbench) {
    w.finish().unwrap();
}

#[test]
fn file_list_is_not_watch_approval_and_stale_decisions_do_not_dispatch() {
    let (mut w, p, root) = fixture("category", "rust");
    let page = w.catalog_page("", None).unwrap();
    let entry = page.entries.iter().find(|e| e.id == ID).unwrap();
    assert!(entry.fs_watch_supported && !entry.fs_watch_approved);
    assert_eq!(entry.fs_watch_handlers, ["observe"]);
    assert_eq!(entry.approved_io, ["file-list"]);
    let denied = options(&w, &p, &root, 1);
    assert!(w.start_fs_watch(denied).is_err());
    assert!(w.io_status().key.is_none());
    let rev = revision(&w);
    assert!(
        w.configure_external_fs_watch(ID, &[77; 32], rev, true)
            .is_err()
    );
    assert_eq!(revision(&w), rev);
    approve(&mut w, &p);
    assert!(
        w.configure_external_fs_watch(ID, &p.digest(), rev, false)
            .is_err()
    );
    assert!(
        w.catalog_page("", None)
            .unwrap()
            .entries
            .iter()
            .find(|e| e.id == ID)
            .unwrap()
            .fs_watch_approved
    );
    w.configure_external_fs_watch(ID, &p.digest(), revision(&w), false)
        .unwrap();
    let denied = options(&w, &p, &root, 2);
    assert!(w.start_fs_watch(denied).is_err());
    assert!(w.io_status().key.is_none());
    finish(&mut w);
    drop(w);
    verify_sealed(&root);
    eprintln!("PROTECTED_WATCH_PREFLIGHT category native_windows=0 workers=0 sealed=true");
}
#[test]
fn exact_bounds_handler_revision_and_host_nomination_preflight() {
    let (mut w, p, root) = fixture("preflight", "rust");
    approve(&mut w, &p);
    for n in 0..9 {
        let mut value = options(&w, &p, &root, n + 1);
        match n {
            0 => value.submission = [0; 32],
            1 => value.window_ms = 0,
            2 => value.window_ms = 30_001,
            3 => value.max_frames = 65,
            4 => value.max_wire_bytes = u64::MAX,
            5 => value.handler = "different".into(),
            6 => value.revision -= 1,
            7 => value.digest = [99; 32],
            8 => value.nominated_path = PathBuf::from("relative"),
            _ => unreachable!(),
        }
        assert!(w.start_fs_watch(value).is_err());
        assert!(w.io_status().key.is_none());
    }
    let gate = w.product_gate();
    let value = options(&w, &p, &root, 20);
    gate.revoke();
    assert!(w.start_fs_watch(value).is_err());
    assert!(w.io_status().key.is_none());
    finish(&mut w);
    drop(w);
    verify_sealed(&root);
    eprintln!(
        "PROTECTED_WATCH_PREFLIGHT bounds=9 lost_gate=1 native_windows=0 workers=0 sealed=true"
    );
}

fn retain(w: Workbench, reason: &str) -> ! {
    let _ = Box::leak(Box::new(w));
    eprintln!("PROTECTED_WATCH_UNRESOLVED_RETAINED {reason}");
    loop {
        thread::park_timeout(Duration::from_secs(30));
    }
}
fn visible(language: &str, cancel: bool, expire: bool) {
    let name = format!(
        "visible-{language}-{}",
        if expire {
            "expire"
        } else if cancel {
            "cancel"
        } else {
            "complete"
        }
    );
    let (mut w, p, root) = fixture(&name, language);
    approve(&mut w, &p);
    let original_options = options(&w, &p, &root, 40);
    let duplicate_revision = original_options.revision;
    let key = w.start_fs_watch(original_options).unwrap();
    let until = Instant::now() + Duration::from_secs(40);
    let mut made = 0;
    let mut stopped = false;
    loop {
        let status = match w.poll_fs_watch(key) {
            Ok(status) => status,
            Err(_) => retain(w, "original poll failed"),
        };
        if status.ready {
            if status.task.storage != StoragePhase::Reclaimed
                || !status
                    .task
                    .exit
                    .is_some_and(|e| e.disconnect.is_ok() && e.maintenance.is_ok())
            {
                retain(w, "owner or cleanup unresolved");
            }
            break;
        }
        if let Some(progress) = status.progress {
            let reads = progress
                .native
                .and_then(|s| s.native)
                .map_or(0, |s| s.read_calls);
            if !expire && made == 0 && reads == 1 {
                if fs::create_dir(root.join("observed").join("first 空白😀")).is_err() {
                    retain(w, "first fixture change failed");
                }
                made = 1;
            }
            if made == 1 && progress.delivered == 1 && reads == 2 {
                if cancel {
                    if !stopped {
                        if w.cancel_io(key).is_err() {
                            retain(w, "stop request failed");
                        }
                        stopped = true;
                    }
                } else {
                    if fs::create_dir(root.join("observed").join("second")).is_err() {
                        retain(w, "second fixture change failed");
                    }
                    made = 2;
                }
            }
        }
        if Instant::now() >= until {
            retain(w, "original join not established");
        }
        thread::sleep(Duration::from_millis(2));
    }
    // Native ownership is known closed before fallible assertions can unwind.
    assert!(
        w.acknowledge_io(key).is_err(),
        "result must be reviewed first"
    );
    assert!(w.finish().is_err(), "final close cannot skip result review");
    assert!(
        w.configure_external_fs_watch(ID, &p.digest(), duplicate_revision, false)
            .is_err()
    );
    let foreign = TaskKey::from_bytes(&[97; 32]).unwrap();
    assert!(w.read_fs_watch(foreign).is_err());
    let result = w.read_fs_watch(key).unwrap().unwrap();
    // Preserve the actual original result before assertions about the expected
    // scenario. A chooser deadline must not leave only an unwrap panic.
    fs::write(
        root.join("original-exit.txt"),
        format!("language={language}\ncancel={cancel}\nexpire={expire}\nresult={result:?}\n"),
    )
    .unwrap();
    assert!(result.original_worker_joined && result.native_retirement_complete && !result.unknown);
    assert_eq!(result.disconnect, Some(Ok(())));
    assert_eq!(result.maintenance, Some(Ok(())));
    let native = result.snapshot.unwrap().native.unwrap();
    assert_eq!(native.ancestors.objects, native.ancestors.confirmed_closed);
    assert!(native.ancestors.picker_started && native.ancestors.picker_closed);
    assert_eq!(result.usage.resources, 0);
    assert_eq!(result.usage.jobs, 0);
    assert!(w.read_fs_watch(key).is_err(), "one receipt delivery");
    w.acknowledge_io(key).unwrap();
    assert!(w.poll_fs_watch(key).is_err());
    assert!(w.io_status().key.is_none());
    let replay = options(&w, &p, &root, 40);
    assert!(
        w.start_fs_watch(replay).is_err(),
        "used submission cannot redispatch"
    );
    finish(&mut w);
    drop(w);
    verify_sealed(&root);
    fs::write(root.join("original-owner-finished.txt"),
        "original_worker_joined=true\nnative_retirement_complete=true\nexplicit_result_acknowledged=true\noriginal_owner_finished=true\nknown_closed_existing_session_integrity=true\n").unwrap();
    if expire {
        use morrow_plugin_runtime::{directory_io::watch, fs_watch_binding, io_binding};
        assert!(matches!(
            result.outcome,
            Err(Error::Expired)
                | Err(Error::Native(watch::Error::Approval(
                    fs_watch_binding::Error::Expired
                )))
                | Err(Error::Native(watch::Error::Approval(
                    fs_watch_binding::Error::Binding(io_binding::Error::Expired)
                )))
        ));
        assert!(native.native.is_none() && native.ancestors.objects == 0);
        assert!(result.deliveries.is_empty() && made == 0 && !stopped);
    } else {
        let handles = native
            .native
            .expect("expected original native capture; actual exit and owner finish retained");
        assert_eq!(handles.owned_handles, 2);
        assert_eq!(handles.close_calls, 2);
        assert_eq!(handles.confirmed_closed, 2);
        if cancel {
            assert!(stopped && made == 1);
            assert_eq!(result.outcome, Err(Error::Cancelled));
            assert_eq!(result.deliveries.len(), 1);
        } else {
            assert_eq!(made, 2);
            assert_eq!(result.outcome, Ok(()));
            assert_eq!(result.deliveries.len(), 2);
        }
    }
    eprintln!(
        "PROTECTED_WATCH_ACCEPTED {name} frames={} actual_native_closes={} sealed=true",
        result.deliveries.len(),
        native.ancestors.confirmed_closed
            + native.native.map_or(0, |h| h.confirmed_closed as usize)
    );
}
#[test]
fn original_protected_owner_visible_rust_guest() {
    visible("rust", false, false);
}
#[test]
fn original_protected_owner_visible_c_guest() {
    visible("c", false, false);
}
#[test]
fn original_protected_owner_visible_cpp_guest() {
    visible("cpp", false, false);
}
#[test]
fn original_protected_owner_cancel_after_guest_delivery() {
    visible("rust", true, false);
}

#[test]
fn original_protected_owner_picker_expiry_has_no_native_capture() {
    visible("rust", false, true);
}

struct WatchWireReply {
    bytes: Vec<u8>,
    message: capnp::message::Reader<capnp::serialize::OwnedSegments>,
}
impl WatchWireReply {
    fn root(&self) -> morrow_workbench_host::host_capnp::response::Reader<'_> {
        self.message.get_root().unwrap()
    }
}
fn watch_wire(
    host: &mut Workbench,
    action: morrow_workbench_host::host_capnp::Action,
    fill: impl FnOnce(morrow_workbench_host::host_capnp::request::Builder<'_>),
) -> Result<WatchWireReply, String> {
    use morrow_workbench_host::{host_capnp as wire, protocol};
    let mut request = capnp::message::Builder::new_default();
    let mut root = request.init_root::<wire::request::Builder>();
    root.set_version(1);
    root.set_digest(&protocol::digest());
    root.set_action(action);
    fill(root);
    let bytes = protocol::respond(host, &capnp::serialize::write_message_to_words(&request))
        .map_err(|e| e.to_string())?;
    let message = capnp::serialize::read_message(
        &mut std::io::Cursor::new(&bytes),
        capnp::message::ReaderOptions::new(),
    )
    .map_err(|e| e.to_string())?;
    let response = message
        .get_root::<wire::response::Reader>()
        .map_err(|e| e.to_string())?;
    if response.get_version() != 1
        || response.get_digest().map_err(|e| e.to_string())? != protocol::digest()
    {
        return Err("original protocol identity changed".into());
    }
    Ok(WatchWireReply { bytes, message })
}
fn watch_wire_start(
    w: &mut Workbench,
    p: &Package,
    path: &[u8],
    submission: &[u8],
) -> Result<WatchWireReply, String> {
    use morrow_workbench_host::host_capnp::Action;
    let current = revision(w);
    watch_wire(w, Action::FsWatchStart, |mut request| {
        let mut value = request.reborrow().init_fs_watch_start();
        value.set_submission(submission);
        value.set_package_id(ID);
        value.set_package_digest(&p.digest());
        value.set_registry_revision(current);
        value.set_handler("observe");
        value.set_nominated_path_utf16_le(path);
        value.set_max_frames(2);
        value.set_max_wire_bytes(512 * 1024);
        value.set_window_ms(29_000);
    })
}

#[test]
fn original_protected_owner_capnp_expiry_and_one_receipt() {
    use morrow_workbench_host::host_capnp::Action;
    use std::os::windows::ffi::OsStrExt;
    let (mut w, p, root) = fixture("wire-expiry", "rust");
    let raw: Vec<u8> = root
        .join("observed")
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect();
    let denied = watch_wire_start(&mut w, &p, &raw, &[61; 32]).unwrap();
    if w.io_status().key.is_some() {
        retain(w, "wire start bypassed independent watch approval");
    }
    assert!(!denied.root().get_error().unwrap().is_empty());
    let current = revision(&w);
    let approved = watch_wire(&mut w, Action::PluginApproveFsWatch, |mut r| {
        r.set_id(ID);
        r.set_sha256(&p.digest());
        r.set_revision(current);
        r.set_fs_watch_approved(true);
    })
    .unwrap();
    assert!(approved.root().get_error().unwrap().is_empty());
    for path in [vec![68, 0, 58], vec![68, 0, 58, 0, 0, 0]] {
        let rejected = watch_wire_start(&mut w, &p, &path, &[62; 32]).unwrap();
        if w.io_status().key.is_some() {
            retain(w, "invalid wire UTF16 unexpectedly started an owner");
        }
        assert!(!rejected.root().get_error().unwrap().is_empty());
    }
    let start = match watch_wire_start(&mut w, &p, &raw, &[63; 32]) {
        Ok(value) => value,
        Err(_) => retain(w, "original wire start reply unavailable"),
    };
    let key = match start.root().get_io_state().and_then(|s| s.get_key()) {
        Ok(value) if value.len() == 32 => value.to_vec(),
        _ => retain(w, "original wire start identity unavailable"),
    };
    let until = Instant::now() + Duration::from_secs(40);
    let ready = loop {
        let reply = match watch_wire(&mut w, Action::FsWatchStatus, |mut r| r.set_io_key(&key)) {
            Ok(value) => value,
            Err(_) => retain(w, "original wire status unavailable"),
        };
        let value = reply.root();
        if !value.get_error().is_ok_and(|e| e.is_empty()) {
            retain(w, "original wire status refused");
        }
        if value.get_fs_watch_state().is_ok_and(|s| s.get_ready()) {
            break reply;
        }
        if Instant::now() >= until {
            retain(w, "original wire join not established");
        }
        thread::sleep(Duration::from_millis(5));
    };
    let read = match watch_wire(&mut w, Action::FsWatchRead, |mut r| r.set_io_key(&key)) {
        Ok(value) => value,
        Err(_) => retain(w, "original wire result unavailable"),
    };
    let result = match read.root().get_fs_watch_result() {
        Ok(value)
            if value.get_present()
                && value.get_original_worker_joined()
                && value.get_native_retirement_complete()
                && !value.get_unknown() =>
        {
            value
        }
        _ => retain(w, "original wire retirement unresolved"),
    };
    // The actual native owner is closed before expected-outcome assertions.
    for (name, value) in [("start", &start), ("ready", &ready), ("read", &read)] {
        fs::write(root.join(format!("{name}.capnp")), &value.bytes).unwrap();
    }
    let again = watch_wire(&mut w, Action::FsWatchRead, |mut r| r.set_io_key(&key)).unwrap();
    assert!(!again.root().get_error().unwrap().is_empty());
    let ack = watch_wire(&mut w, Action::IoAcknowledge, |mut r| r.set_io_key(&key)).unwrap();
    assert!(ack.root().get_error().unwrap().is_empty());
    assert!(w.io_status().key.is_none());
    let replay = watch_wire_start(&mut w, &p, &raw, &[63; 32]).unwrap();
    if w.io_status().key.is_some() {
        retain(w, "used wire submission unexpectedly redispatched");
    }
    assert!(!replay.root().get_error().unwrap().is_empty());
    finish(&mut w);
    drop(w);
    verify_sealed(&root);
    fs::write(root.join("original-owner-finished.txt"),
        "original_capnp_dispatch=true\noriginal_worker_joined=true\nnative_retirement_complete=true\nexplicit_result_acknowledged=true\noriginal_owner_finished=true\nknown_closed_existing_session_integrity=true\n").unwrap();
    assert!(!result.get_success());
    assert_eq!(result.get_error(), 8);
    assert!(result.get_has_snapshot() && result.get_picker_started() && result.get_picker_closed());
    assert!(!result.get_has_native());
    assert_eq!(result.get_ancestor_objects(), 0);
    assert_eq!(result.get_ancestor_closed(), 0);
    assert_eq!(result.get_deliveries().unwrap().len(), 0);
    assert_eq!(result.get_resources(), 0);
    assert_eq!(result.get_jobs(), 0);
    assert!(result.get_has_disconnect() && result.get_has_maintenance());
    assert_eq!(result.get_disconnect(), 0);
    assert_eq!(result.get_maintenance(), 0);
    let state = read.root().get_fs_watch_state().unwrap();
    assert!(state.get_delivered() && !state.get_ready());
    assert_eq!(state.get_frames(), 0);
    assert_eq!(state.get_phase(), 4);
    assert_eq!(read.root().get_io_state().unwrap().get_key().unwrap(), key);
    assert_eq!(
        read.root()
            .get_io_state()
            .unwrap()
            .get_submission()
            .unwrap(),
        [63; 32]
    );
    eprintln!(
        "PROTECTED_WATCH_CAPNP original_expiry=8 read_once=true replay_refused=true sealed=true"
    );
}
