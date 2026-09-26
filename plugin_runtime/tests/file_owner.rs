//! Native file commands retain the original owner and execute unchanged SDK modules.
#![cfg(all(feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::Store,
};
use morrow_plugin_runtime::{
    Limits,
    io_jobs::{
        FileCommandError, FileCommandHandle, FileResponse, HostOwner, IoWorker, JobError,
        JobLimits, ManagedHostOwner, OwnerCommandError, OwnerCommandPoll, WorkerExit,
    },
    manager::Manager,
};
use sha2::Digest;
use std::{
    collections::BTreeSet,
    fs::File,
    io::{Seek, SeekFrom, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
const ID: &str = "org.example.selected-owner";
const HANDLER: &str = "file.read-selected";
struct Owner {
    host: HostRuntime,
    manager: Manager,
    identity: Arc<()>,
    hooks: Arc<Mutex<Vec<thread::ThreadId>>>,
    gate: Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>,
    panic_prepare: bool,
    fail_finish: bool,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.hooks.lock().unwrap().push(thread::current().id());
        if let Some((entered, release)) = self.gate.take() {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        assert!(!self.panic_prepare, "injected original-owner prepare panic");
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.hooks.lock().unwrap().push(thread::current().id());
        if self.fail_finish {
            Err(JobError::Busy)
        } else {
            Ok(())
        }
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    owner: Owner,
    clock: Arc<AtomicU64>,
    budget: u64,
}
impl Fixture {
    fn new(language: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../sdk/compat/transport-v1-rc1/{language}-io.wasm")),
        )
        .unwrap();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration = io::declaration(vec![IoCapability::FileRead], vec![HANDLER.into()]);
        let b = declaration.budget.as_mut().unwrap();
        b.max_jobs = 1;
        b.max_resources = 2;
        b.max_job_bytes = 1024 * 1024;
        b.max_bytes = 4 * 1024 * 1024;
        b.max_duration_ms = 30_000;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
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
        let host =
            HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap())
                .unwrap();
        Self {
            dir,
            owner: Owner {
                host,
                manager,
                identity: Arc::new(()),
                hooks: Arc::new(Mutex::new(vec![])),
                gate: None,
                panic_prepare: false,
                fail_finish: false,
            },
            clock: Arc::new(AtomicU64::new(1)),
            budget: 4 * 1024 * 1024,
        }
    }
    fn start(mut self) -> (tempfile::TempDir, IoWorker<Owner>) {
        let instance = self
            .owner
            .manager
            .connect(ID, &mut self.owner.host)
            .unwrap();
        let binding = self
            .owner
            .manager
            .bind_io(
                &self.owner.host,
                &instance,
                instance.package().package().digest(),
                self.owner.manager.revision(),
                &BTreeSet::from([IoCapability::FileRead]),
                30_000,
                1,
            )
            .unwrap();
        let clock = self.clock;
        let worker = IoWorker::spawn_managed_owner(
            self.owner,
            instance,
            binding,
            move || clock.fetch_add(1, Ordering::SeqCst),
            1,
            JobLimits::new(1, 1024 * 1024, self.budget).unwrap(),
        )
        .unwrap();
        (self.dir, worker)
    }
}
fn file(bytes: &[u8]) -> File {
    let mut f = tempfile::tempfile().unwrap();
    f.write_all(bytes).unwrap();
    f
}
fn ready(handle: &FileCommandHandle) {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
fn read(handle: &mut FileCommandHandle) -> Result<FileResponse, FileCommandError> {
    ready(handle);
    handle.read().unwrap().unwrap()
}
fn reclaim(worker: &mut IoWorker<Owner>) -> WorkerExit<Owner> {
    worker.stop();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn three_original_modules_capture_read_finish_on_original_owner_thread() {
    for lang in ["rust", "c", "cpp"] {
        let fixture = Fixture::new(lang);
        let identity = fixture.owner.identity.clone();
        let host = fixture.owner.host.binding();
        let hooks = fixture.owner.hooks.clone();
        let (dir, mut worker) = fixture.start();
        let bytes = (0..90_007).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let path = dir.path().join("source");
        std::fs::write(&path, &bytes).unwrap();
        let (session, mut capture) = worker
            .capture_file(
                File::open(&path).unwrap(),
                HANDLER.into(),
                bytes.len() as u64,
                [7; 32],
            )
            .unwrap();
        let FileResponse::Captured(meta) = read(&mut capture).unwrap() else {
            panic!()
        };
        assert_eq!(meta.sha256, <[u8; 32]>::from(sha2::Sha256::digest(&bytes)));
        std::fs::remove_file(&path).unwrap();
        let mut all = Vec::new();
        for offset in [0, 65_536] {
            let mut h = worker.read_file_chunk(session, offset, 0).unwrap();
            let FileResponse::Chunk {
                offset: actual,
                bytes: chunk,
                eof,
            } = read(&mut h).unwrap()
            else {
                panic!()
            };
            assert_eq!(actual, offset);
            assert_eq!(eof, offset != 0);
            all.extend_from_slice(&chunk);
        }
        assert_eq!(all, bytes);
        let mut end = worker.finish_file(session).unwrap();
        assert!(matches!(read(&mut end), Ok(FileResponse::Finished)));
        let mut stale = worker.read_file_chunk(session, 0, 1).unwrap();
        assert!(matches!(read(&mut stale), Err(FileCommandError::Missing)));
        let exit = reclaim(&mut worker);
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(exit.disconnect, Ok(()));
        assert!(exit.instance.is_none());
        assert_eq!(exit.owner.host.binding(), host);
        assert!(Arc::ptr_eq(&identity, &exit.owner.identity));
        assert!(
            hooks
                .lock()
                .unwrap()
                .iter()
                .all(|id| *id != thread::current().id())
        );
        drop(exit);
        let reopened = Store::open(&dir.path().join("db"), Default::default()).unwrap();
        drop(reopened);
    }
}
#[test]
fn admission_is_nonblocking_and_queued_cancel_never_reads_the_file() {
    let mut fixture = Fixture::new("rust");
    let (entered_tx, entered) = mpsc::channel();
    let (release, release_rx) = mpsc::channel();
    fixture.owner.gate = Some((entered_tx, release_rx));
    let (_dir, mut worker) = fixture.start();
    let mut source = file(b"abcdefghij");
    source.seek(SeekFrom::Start(3)).unwrap();
    let mut cursor = source.try_clone().unwrap();
    let (_, mut handle) = worker
        .capture_file(source, HANDLER.into(), 10, [8; 32])
        .unwrap();
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(handle.poll(), OwnerCommandPoll::Pending);
    assert!(worker.try_reclaim().unwrap().is_none());
    handle.cancel();
    assert!(matches!(handle.read(), Err(OwnerCommandError::Cancelled)));
    assert_eq!(worker.owner_command_usage().0, 1);
    release.send(()).unwrap();
    let exit = reclaim(&mut worker);
    assert_eq!(cursor.stream_position().unwrap(), 3);
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(worker.owner_command_usage().0, 0);
}
#[test]
fn unread_cancelled_capture_releases_resource_before_next_capture() {
    let (_dir, mut worker) = Fixture::new("rust").start();
    for n in 0..6 {
        let (_, mut h) = worker
            .capture_file(file(b"data"), HANDLER.into(), 4, [n + 1; 32])
            .unwrap();
        ready(&h);
        h.cancel();
        assert!(h.read().is_err());
        // FIFO next command plus maintenance must reclaim the abandoned capture.
        thread::sleep(Duration::from_millis(12));
    }
    let (_, mut h) = worker
        .capture_file(file(b"good"), HANDLER.into(), 4, [12; 32])
        .unwrap();
    assert!(matches!(read(&mut h), Ok(FileResponse::Captured(_))));
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}
#[test]
fn ready_chunk_is_suppressed_on_stop_and_expiry() {
    for expire in [false, true] {
        let fixture = Fixture::new("cpp");
        let clock = fixture.clock.clone();
        let (_dir, mut worker) = fixture.start();
        let (session, mut h) = worker
            .capture_file(file(b"private"), HANDLER.into(), 7, [3; 32])
            .unwrap();
        read(&mut h).unwrap();
        let mut chunk = worker.read_file_chunk(session, 0, 0).unwrap();
        ready(&chunk);
        if expire {
            clock.store(30_001, Ordering::SeqCst)
        } else {
            worker.stop()
        };
        assert!(matches!(chunk.read(), Err(OwnerCommandError::Unknown)));
        let exit = reclaim(&mut worker);
        assert_eq!(exit.disconnect, Ok(()));
    }
}
#[test]
fn foreign_worker_token_cannot_consume_capacity() {
    let (_d1, mut a) = Fixture::new("rust").start();
    let f = Fixture::new("c");
    let (_d2, mut b) = f.start();
    let (session, mut h) = a
        .capture_file(file(b"x"), HANDLER.into(), 1, [4; 32])
        .unwrap();
    read(&mut h).unwrap();
    let usage = b.owner_command_usage();
    assert!(matches!(
        b.read_file_chunk(session, 0, 1),
        Err(OwnerCommandError::Closed)
    ));
    assert_eq!(b.owner_command_usage(), usage);
    reclaim(&mut a);
    reclaim(&mut b);
}
#[test]
fn bounded_queue_and_invalid_requests_do_not_replace_original_owner() {
    let mut fixture = Fixture::new("rust");
    let (tx, entered) = mpsc::channel();
    let (release, rx) = mpsc::channel();
    fixture.owner.gate = Some((tx, rx));
    let (_dir, mut worker) = fixture.start();
    let mut handles = Vec::new();
    let (session, h) = worker
        .capture_file(file(b"x"), HANDLER.into(), 1, [5; 32])
        .unwrap();
    handles.push(h);
    entered.recv_timeout(Duration::from_secs(5)).unwrap();
    for _ in 1..8 {
        handles.push(worker.read_file_chunk(session, 0, 1).unwrap());
    }
    assert_eq!(worker.owner_command_usage().0, 8);
    assert!(matches!(
        worker.finish_file(session),
        Err(OwnerCommandError::Busy)
    ));
    assert!(matches!(
        worker.read_file_chunk(session, 0, 65_537),
        Err(OwnerCommandError::Limit)
    ));
    worker.stop();
    release.send(()).unwrap();
    let exit = reclaim(&mut worker);
    assert_eq!(exit.disconnect, Ok(()));
    drop(handles);
    assert_eq!(worker.owner_command_usage().0, 0);
}
#[test]
fn preparation_panic_and_maintenance_failure_still_return_exact_owner() {
    for panic_prepare in [true, false] {
        let mut fixture = Fixture::new("rust");
        let identity = fixture.owner.identity.clone();
        fixture.owner.panic_prepare = panic_prepare;
        fixture.owner.fail_finish = !panic_prepare;
        let (_dir, mut worker) = fixture.start();
        let (_, mut h) = worker
            .capture_file(file(b"x"), HANDLER.into(), 1, [6; 32])
            .unwrap();
        ready(&h);
        if panic_prepare {
            assert!(h.read().is_err())
        } else {
            read(&mut h).unwrap();
        }
        let exit = reclaim(&mut worker);
        assert!(Arc::ptr_eq(&identity, &exit.owner.identity));
        assert_eq!(exit.disconnect, Ok(()));
        if panic_prepare {
            assert_eq!(exit.result, Err(JobError::Unavailable))
        } else {
            assert_eq!(exit.maintenance, Err(JobError::Busy))
        };
    }
}

#[test]
fn host_worker_ceiling_is_reserved_before_capture_and_never_refunded() {
    let mut fixture = Fixture::new("rust");
    fixture.budget = 1024 * 1024;
    let (_dir, mut worker) = fixture.start();
    let too_large = worker.capture_file(file(b"tiny"), HANDLER.into(), 1024 * 1024, [21; 32]);
    assert!(matches!(too_large, Err(OwnerCommandError::Limit)));
    assert_eq!(worker.bytes(), 0);
    let (_, mut first) = worker
        .capture_file(file(b"tiny"), HANDLER.into(), 512 * 1024, [22; 32])
        .unwrap();
    ready(&first);
    first.cancel();
    assert!(first.read().is_err());
    assert_eq!(worker.bytes(), 512 * 1024 + 1);
    let mut source = file(b"untouched");
    source.seek(SeekFrom::Start(2)).unwrap();
    let mut observer = source.try_clone().unwrap();
    assert!(matches!(
        worker.capture_file(source, HANDLER.into(), 512 * 1024, [23; 32]),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(observer.stream_position().unwrap(), 2);
    assert_eq!(worker.bytes(), 512 * 1024 + 1);
    assert_eq!(reclaim(&mut worker).disconnect, Ok(()));
}

#[test]
fn selected_path_is_opened_on_original_worker_after_admission() {
    let mut fixture = Fixture::new("rust");
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    fixture.owner.gate = Some((entered_tx, release_rx));
    let (dir, mut worker) = fixture.start();
    let path = dir.path().join("created-after-admission");
    let (_, mut capture) = worker
        .capture_selected_path(path.clone(), HANDLER.into(), 3, [9; 32])
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(capture.poll(), OwnerCommandPoll::Pending);
    // The source did not exist when admission returned; no caller-side open.
    std::fs::write(path, b"abc").unwrap();
    release_tx.send(()).unwrap();
    let FileResponse::Captured(meta) = read(&mut capture).unwrap() else {
        panic!()
    };
    assert_eq!(meta.length, 3);
    reclaim(&mut worker);
}
#[test]
fn selected_path_invalid_inputs_and_preopen_expiry_fail_closed() {
    let mut fixture = Fixture::new("c");
    let clock = fixture.clock.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    fixture.owner.gate = Some((entered_tx, release_rx));
    let (dir, mut worker) = fixture.start();
    for path in [
        "relative".into(),
        std::path::PathBuf::from("/bad\0path"),
        format!("/{}", "x".repeat(4096)).into(),
    ] {
        assert!(
            worker
                .capture_selected_path(path, HANDLER.into(), 1, [9; 32])
                .is_err()
        );
    }
    let (_, mut capture) = worker
        .capture_selected_path(
            dir.path().join("SECRET-does-not-exist"),
            HANDLER.into(),
            1,
            [9; 32],
        )
        .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    clock.store(40_000, Ordering::SeqCst);
    release_tx.send(()).unwrap();
    ready(&capture);
    // Expired authority must suppress both opening and delivery. No path-bearing OS error.
    assert!(capture.read().is_err());
    reclaim(&mut worker);
}
