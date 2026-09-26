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
