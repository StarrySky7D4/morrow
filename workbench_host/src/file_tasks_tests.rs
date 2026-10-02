//! Native task API only; Linux intentionally retains unsupported protected maintenance.
use super::*;
use crate::io_tasks::{StateSlot, StoragePhase};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, io, registry::Registry},
    store::Store,
};
use morrow_plugin_runtime::{Limits, io_jobs::JobLimits, manager::Manager};
use std::{
    io::{Seek, SeekFrom, Write},
    thread,
    time::Instant,
};
const ID: &str = "org.example.workbench.file";
const HANDLER: &str = "file.read-selected";
struct Setup {
    app: Workbench,
    dir: tempfile::TempDir,
    digest: [u8; 32],
}
impl Setup {
    fn new(language: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../sdk/compat/transport-v1-rc1/{language}-io.wasm")),
        )
        .unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut decl = io::declaration(vec![IoCapability::FileRead], vec![HANDLER.into()]);
        let b = decl.budget.as_mut().unwrap();
        b.max_jobs = 1;
        b.max_resources = 2;
        b.max_job_bytes = 1024 * 1024;
        b.max_bytes = 4 * 1024 * 1024;
        b.max_duration_ms = 30_000;
        manifest.io_declaration = Some(decl);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&dir.path().join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                BTreeSet::from([IoCapability::FileRead]),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        #[cfg(not(target_os = "windows"))]
        let storage = crate::storage::Storage::test_from_runtime(
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap(),
        );
        #[cfg(target_os = "windows")]
        let storage = crate::storage::Storage::open(&dir.path().join("db")).unwrap();
        let owner = WorkbenchState::with_manager(storage, Ok(manager), None).unwrap();
        let app = Workbench {
            channel_tasks: Default::default(),
            http_tasks: Default::default(),
            state: StateSlot::new(owner),
        };
        Self {
            app,
            dir,
            digest: package.digest(),
        }
    }
    fn options(&self) -> StartOptions {
        StartOptions {
            package_id: ID.into(),
            digest: self.digest,
            revision: self
                .app
                .local_state()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
            capabilities: BTreeSet::from([IoCapability::FileRead]),
            lifetime: Duration::from_secs(10),
            limits: JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        }
    }
}
fn file(bytes: &[u8]) -> File {
    let mut file = tempfile::tempfile().unwrap();
    file.write_all(bytes).unwrap();
    file
}
fn read(app: &mut Workbench, key: TaskKey) -> Result<FileResponse> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(r) = app.read_file_result(key)? {
            return Ok(r);
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
fn reclaim(app: &mut Workbench, key: TaskKey) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let s = app.poll_io(key).unwrap();
        if s.exit.is_some() {
            assert_eq!(s.exit.unwrap().disconnect, Ok(()));
            break;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn three_language_file_task_keeps_owner_busy_until_finished_and_joined() {
    for language in ["rust", "c", "cpp"] {
        let mut setup = Setup::new(language);
        let host = setup.app.local_state().unwrap().host.binding();
        let options = setup.options();
        let bytes = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let path = setup.dir.path().join("selected");
        std::fs::write(&path, &bytes).unwrap();
        let key = setup
            .app
            .start_file(
                options,
                File::open(&path).unwrap(),
                HANDLER.into(),
                bytes.len() as u64,
            )
            .unwrap();
        assert!(setup.app.local_state().is_err());
        assert!(setup.app.request_file_chunk(key, 0, 1).is_err());
        assert!(setup.app.acknowledge_io(key).is_err());
        let FileResponse::Captured(meta) = read(&mut setup.app, key).unwrap() else {
            panic!()
        };
        assert_eq!(meta.length, bytes.len() as u64);
        std::fs::remove_file(&path).unwrap();
        let mut actual = Vec::new();
        for offset in [0, 65_536] {
            setup.app.request_file_chunk(key, offset, 0).unwrap();
            assert!(setup.app.request_file_chunk(key, offset, 1).is_err());
            let FileResponse::Chunk { bytes: part, .. } = read(&mut setup.app, key).unwrap() else {
                panic!()
            };
            actual.extend_from_slice(&part);
        }
        assert_eq!(actual, bytes);
        setup.app.finish_file(key).unwrap();
        assert!(matches!(
            read(&mut setup.app, key),
            Ok(FileResponse::Finished)
        ));
        reclaim(&mut setup.app, key);
        assert_eq!(setup.app.local_state().unwrap().host.binding(), host);
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(
                setup.app.io_status().storage,
                StoragePhase::RecoveryRequired
            );
            assert!(setup.app.acknowledge_io(key).is_err());
            assert!(setup.app.repair_io(key).is_err());
        }
        #[cfg(target_os = "windows")]
        {
            setup.app.acknowledge_io(key).unwrap();
            assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
        }
    }
}
#[test]
fn invalid_handler_revision_or_capability_never_reads_or_moves_owner() {
    for case in ["handler", "revision", "capability", "budget"] {
        let mut setup = Setup::new("rust");
        let original = setup.app.local_state().unwrap().host.binding();
        let mut options = setup.options();
        if case == "revision" {
            options.revision += 1
        }
        if case == "budget" {
            options.limits.max_job_bytes = 3;
        }
        if case == "capability" {
            options.capabilities.clear()
        }
        let mut source = file(b"abc");
        source.seek(SeekFrom::Start(2)).unwrap();
        let mut observer = source.try_clone().unwrap();
        assert!(
            setup
                .app
                .start_file(
                    options,
                    source,
                    if case == "handler" {
                        "undeclared"
                    } else {
                        HANDLER
                    }
                    .into(),
                    3
                )
                .is_err()
        );
        assert_eq!(observer.stream_position().unwrap(), 2);
        assert_eq!(setup.app.local_state().unwrap().host.binding(), original);
        assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    }
}
#[test]
fn wrong_task_and_unread_cancel_do_not_deliver_ready_file_bytes() {
    let mut setup = Setup::new("cpp");
    let options = setup.options();
    let key = setup
        .app
        .start_file(options, file(b"private"), HANDLER.into(), 7)
        .unwrap();
    read(&mut setup.app, key).unwrap();
    setup.app.request_file_chunk(key, 0, 0).unwrap();
    assert!(
        setup
            .app
            .read_file_result(TaskKey::from_bytes(&[0xee; 32]).unwrap())
            .is_err()
    );
    let until = Instant::now() + Duration::from_secs(5);
    while setup.app.poll_io(key).unwrap().delivery != Some(Poll::Ready) {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    setup.app.cancel_io(key).unwrap();
    assert!(setup.app.read_file_result(key).is_err());
    reclaim(&mut setup.app, key);
}

use crate::{host_capnp as wire, protocol};
use capnp::{
    message::{Builder, Reader, ReaderOptions},
    serialize::{self, OwnedSegments},
};
fn wire_call(
    app: &mut Workbench,
    action: wire::Action,
    configure: impl FnOnce(wire::request::Builder<'_>),
) -> Reader<OwnedSegments> {
    let mut message = Builder::new_default();
    let mut r = message.init_root::<wire::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    configure(r);
    let bytes = protocol::respond(app, &serialize::write_message_to_words(&message)).unwrap();
    assert!(bytes.len() <= 128 * 1024);
    serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap()
}
fn wire_read(app: &mut Workbench, key: &[u8]) -> Reader<OwnedSegments> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let response = wire_call(app, wire::Action::FileRead, |mut r| r.set_io_key(key));
        let row = response.get_root::<wire::response::Reader>().unwrap();
        assert!(row.get_error().unwrap().is_empty(), "{:?}", row.get_error());
        assert_eq!(row.get_io_state().unwrap().get_key().unwrap(), key);
        if row.get_file_result().unwrap().get_kind() != 0 {
            return response;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}

fn export_wire_fixture(language: &str, name: &str, response: &Reader<OwnedSegments>) {
    if let Ok(dir) = std::env::var("MORROW_FILE_WIRE_FIXTURES") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{language}-{name}.bin")),
            serialize::write_message_segments_to_words(response.get_segments()),
        )
        .unwrap();
    }
}
#[test]
fn private_file_wire_three_language_capture_chunks_finish_and_no_replay() {
    use sha2::{Digest, Sha256};
    for language in ["rust", "c", "cpp"] {
        let mut setup = Setup::new(language);
        let options = setup.options();
        let bytes = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let path = setup.dir.path().join("private-source");
        std::fs::write(&path, &bytes).unwrap();
        let start = |mut r: wire::request::Builder<'_>| {
            let mut s = r.reborrow().init_file_start();
            s.set_submission(&[41; 32]);
            s.set_package_id(ID);
            s.set_package_digest(&options.digest);
            s.set_registry_revision(options.revision);
            s.set_handler(HANDLER);
            s.set_selected_path(path.to_str().unwrap());
            s.set_max_bytes(bytes.len() as u64);
            s.set_timeout_ms(10_000);
        };
        let response = wire_call(&mut setup.app, wire::Action::FileStart, start);
        let row = response.get_root::<wire::response::Reader>().unwrap();
        assert!(row.get_error().unwrap().is_empty(), "{:?}", row.get_error());
        let state = row.get_io_state().unwrap();
        assert_eq!(state.get_submission().unwrap(), &[41; 32]);
        let key = state.get_key().unwrap().to_vec();
        let repeated = wire_call(&mut setup.app, wire::Action::FileStart, start);
        assert!(
            !repeated
                .get_root::<wire::response::Reader>()
                .unwrap()
                .get_error()
                .unwrap()
                .is_empty()
        );
        let result = wire_read(&mut setup.app, &key);
        export_wire_fixture(language, "captured", &result);
        let row = result
            .get_root::<wire::response::Reader>()
            .unwrap()
            .get_file_result()
            .unwrap();
        assert_eq!(row.get_kind(), 1);
        assert_eq!(row.get_length(), bytes.len() as u64);
        assert_eq!(row.get_sha256().unwrap(), &Sha256::digest(&bytes)[..]);
        std::fs::remove_file(&path).unwrap();
        let mut actual = Vec::new();
        for offset in [0, 65_536] {
            let result = wire_call(&mut setup.app, wire::Action::FileChunk, |mut r| {
                r.set_io_key(&key);
                r.set_offset(offset);
                r.set_limit(0);
            });
            assert!(
                result
                    .get_root::<wire::response::Reader>()
                    .unwrap()
                    .get_error()
                    .unwrap()
                    .is_empty()
            );
            let result = wire_read(&mut setup.app, &key);
            export_wire_fixture(language, &format!("chunk-{offset}"), &result);
            let row = result
                .get_root::<wire::response::Reader>()
                .unwrap()
                .get_file_result()
                .unwrap();
            assert_eq!(row.get_kind(), 2);
            assert_eq!(row.get_offset(), offset);
            assert_eq!(row.get_eof(), offset == 65_536);
            actual.extend_from_slice(row.get_bytes().unwrap());
        }
        assert_eq!(actual, bytes);
        wire_call(&mut setup.app, wire::Action::FileFinish, |mut r| {
            r.set_io_key(&key)
        });
        assert_eq!(
            wire_read(&mut setup.app, &key)
                .get_root::<wire::response::Reader>()
                .unwrap()
                .get_file_result()
                .unwrap()
                .get_kind(),
            3
        );
        reclaim(&mut setup.app, TaskKey::from_bytes(&key).unwrap());
        assert_eq!(setup.app.http_submission(), Some([41; 32]));
    }
}
#[test]
fn private_file_wire_rejects_wrong_task_and_clears_failed_result() {
    let mut setup = Setup::new("rust");
    let o = setup.options();
    let path = setup.dir.path().join("SECRET-MISSING-FILE");
    let response = wire_call(&mut setup.app, wire::Action::FileStart, |mut r| {
        let mut s = r.reborrow().init_file_start();
        s.set_submission(&[42; 32]);
        s.set_package_id(ID);
        s.set_package_digest(&o.digest);
        s.set_registry_revision(o.revision);
        s.set_handler(HANDLER);
        s.set_selected_path(path.to_str().unwrap());
        s.set_max_bytes(20);
        s.set_timeout_ms(10_000);
    });
    let key = response
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let result = wire_call(&mut setup.app, wire::Action::FileRead, |mut r| {
        r.set_io_key(&[99; 32])
    });
    let row = result.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(row.get_ui_code(), 113);
    assert!(!row.has_file_result());
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let result = wire_call(&mut setup.app, wire::Action::FileRead, |mut r| {
            r.set_io_key(&key)
        });
        let row = result.get_root::<wire::response::Reader>().unwrap();
        let error = row.get_error().unwrap().to_str().unwrap();
        assert!(!error.contains("SECRET-MISSING-FILE"));
        if !error.is_empty() {
            assert!(!row.has_file_result());
            break;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    reclaim(&mut setup.app, TaskKey::from_bytes(&key).unwrap());
}
#[test]
fn file_submission_attempts_are_bounded_and_never_rebound_to_http() {
    let mut setup = Setup::new("rust");
    let request = |submission| FileStart {
        submission,
        package_id: ID.into(),
        digest: setup.digest,
        revision: u64::MAX,
        handler: HANDLER.into(),
        selected_path: setup.dir.path().join("never-opened"),
        max_bytes: 1,
        timeout_ms: 100,
    };
    let first = request([8; 32]);
    let repeated = request([8; 32]);
    let exhausted = request([9; 32]);
    assert!(setup.app.start_selected_file(first).is_err()); // stale revision burns the explicit attempt
    assert_eq!(setup.app.http_submission(), Some([8; 32]));
    assert!(
        setup
            .app
            .start_selected_file(repeated)
            .unwrap_err()
            .to_string()
            .contains("repeated")
    );
    setup.app.state.file_submissions = (0u32..512)
        .map(|i| {
            let mut id = [1; 32];
            id[..4].copy_from_slice(&i.to_le_bytes());
            id
        })
        .collect();
    assert!(setup.app.start_selected_file(exhausted).is_err());
    assert!(setup.app.io_status().key.is_none());
}

#[test]
fn private_file_wire_cancel_suppresses_ready_chunk_and_nested_routes_are_denied() {
    let mut setup = Setup::new("cpp");
    // A service command's business-only dispatcher must not admit file schedulers.
    for action in [
        wire::Action::FileStart,
        wire::Action::FileChunk,
        wire::Action::FileRead,
        wire::Action::FileFinish,
    ] {
        let mut message = Builder::new_default();
        let mut r = message.init_root::<wire::request::Builder>();
        r.set_version(1);
        r.set_digest(&protocol::digest());
        r.set_action(action);
        let bytes = protocol::respond_state(
            setup.app.local_state_mut().unwrap(),
            &serialize::write_message_to_words(&message),
        )
        .unwrap();
        let reply = serialize::read_message(&mut bytes.as_slice(), ReaderOptions::new()).unwrap();
        let row = reply.get_root::<wire::response::Reader>().unwrap();
        assert!(!row.get_error().unwrap().is_empty());
        assert!(!row.has_file_result());
    }
    let options = setup.options();
    let key = setup
        .app
        .start_file(options, file(b"private file bytes"), HANDLER.into(), 18)
        .unwrap();
    wire_read(&mut setup.app, key.as_bytes());
    wire_call(&mut setup.app, wire::Action::FileChunk, |mut r| {
        r.set_io_key(key.as_bytes())
    });
    let until = Instant::now() + Duration::from_secs(5);
    while setup.app.poll_io(key).unwrap().delivery != Some(Poll::Ready) {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    wire_call(&mut setup.app, wire::Action::IoCancel, |mut r| {
        r.set_io_key(key.as_bytes())
    });
    let result = wire_call(&mut setup.app, wire::Action::FileRead, |mut r| {
        r.set_io_key(key.as_bytes())
    });
    let row = result.get_root::<wire::response::Reader>().unwrap();
    assert!(!row.get_error().unwrap().is_empty());
    assert!(!row.has_file_result());
    reclaim(&mut setup.app, key);
}
