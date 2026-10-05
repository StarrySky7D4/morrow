//! Private original-owner directory commands over real Windows synthetic directories.
//! No protected owner/session, guest directory import, entropy factory or picker proof.
#![cfg(all(feature = "packages", windows))]

use morrow_core::{
    dispatch::{HostBinding, HostRuntime},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::Store,
};
use morrow_fs_directory_v1::{FsDirectoryPage, NameEncoding};
use morrow_plugin_runtime::{
    Limits,
    directory_io::{CaptureLimits, SelectedDirectory},
    io_binding::{Error as AdmissionError, IoBinding},
    io_jobs::{
        CommandOwner, DIRECTORY_COMMAND_FIXED_BYTES, DirectoryCommandError, DirectoryCommandHandle,
        DirectoryResponse, DirectorySession, HostOwner, IoWorker, JobError, JobLimits,
        MAX_DIRECTORY_OBSERVATIONS, ManagedHostOwner, OwnerCommandError, OwnerCommandHandle,
        OwnerCommandPoll, WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.directory-owner";
const HANDLER: &str = "directory.immediate-selected";
const WAIT: Duration = Duration::from_secs(10);
const BLOCK: u8 = 1;
const REVOKE: u8 = 2;
const NOOP: u8 = 3;
const SMALL_JOB: u64 = 128 * 1024;

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}
impl Gate {
    fn wait(&self) {
        let guard = self.open.lock().unwrap();
        let (guard, timed) = self
            .changed
            .wait_timeout_while(guard, WAIT, |v| !*v)
            .unwrap();
        assert!(
            *guard && !timed.timed_out(),
            "synthetic owner barrier was not released within 10 seconds"
        );
    }
    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.changed.notify_all();
    }
}
// Test-only delay in an original immutable owner-context getter. It neither
// intercepts an OS call nor creates an authorization clock/runtime.
struct SlowDirectoryContext {
    requested: AtomicBool,
    active: AtomicBool,
    readonly_calls: AtomicUsize,
    entered: mpsc::Sender<usize>,
    gate: Gate,
}
impl SlowDirectoryContext {
    fn arm_next_prepare(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }
    fn prepare(&self) {
        if self.requested.swap(false, Ordering::SeqCst) {
            self.readonly_calls.store(0, Ordering::SeqCst);
            self.active.store(true, Ordering::SeqCst);
        }
    }
    fn readonly_context(&self) {
        if self.active.load(Ordering::SeqCst) {
            let ordinal = self.readonly_calls.fetch_add(1, Ordering::SeqCst) + 1;
            // checked_runtime uses runtime_mut, which does not call this getter.
            // After prepare: identity validation is1; broker argument is2.
            if ordinal == 2 && self.active.swap(false, Ordering::SeqCst) {
                self.entered.send(ordinal).unwrap();
                self.gate.wait();
            }
        }
    }
}
struct ReleaseContextOnDrop(Arc<SlowDirectoryContext>);
impl Drop for ReleaseContextOnDrop {
    fn drop(&mut self) {
        self.0.gate.release();
    }
}
struct Owner {
    host: HostRuntime,
    manager: Manager,
    package: Package,
    identity: Arc<()>,
    hooks: Arc<Mutex<Vec<(&'static str, thread::ThreadId)>>>,
    gate: Arc<Gate>,
    entered: mpsc::Sender<u8>,
    swap: Option<HostRuntime>,
    fail_finish: bool,
    slow_context: Option<Arc<SlowDirectoryContext>>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        if let Some(context) = &self.slow_context {
            context.readonly_context();
        }
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        if let Some(context) = &self.slow_context {
            context.prepare();
        }
        self.hooks
            .lock()
            .unwrap()
            .push(("prepare", thread::current().id()));
        if let Some(alternate) = self.swap.as_mut() {
            std::mem::swap(&mut self.host, alternate);
        }
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("finish", thread::current().id()));
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
    // Default mutable split-borrow stays unimplemented: FileList only needs
    // the authenticated original immutable manager/runtime borrow.
}
impl CommandOwner for Owner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("command", thread::current().id()));
        self.entered.send(input[0]).unwrap();
        match input[0] {
            BLOCK => self.gate.wait(),
            REVOKE => self
                .manager
                .set_enabled(ID, self.package.digest(), false, self.manager.revision())
                .unwrap(),
            NOOP => {}
            _ => panic!("unknown synthetic barrier command"),
        }
        Ok(Vec::new())
    }
}
struct NoOptIn(Owner);
impl HostOwner for NoOptIn {
    fn runtime(&self) -> &HostRuntime {
        self.0.runtime()
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        self.0.runtime_mut()
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.0.prepare_io()
    }
}
impl ManagedHostOwner for NoOptIn {
    fn manager(&self) -> Option<&Manager> {
        None
    }
}

struct Fixture {
    temp: tempfile::TempDir,
    data: PathBuf,
    owner: Owner,
    clock: Arc<AtomicU64>,
    capabilities: BTreeSet<IoCapability>,
    limits: JobLimits,
}
impl Fixture {
    fn new() -> Self {
        Self::capabilities(&[IoCapability::FileList])
    }
    fn capabilities(capabilities: &[IoCapability]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("selected-synthetic");
        fs::create_dir(&data).unwrap();
        // No directory guest import exists. This ordinary old-profile managed
        // instance supplies the original identity/approval/lifecycle chain only.
        let wasm = wat::parse_str(
            r#"(module
            (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
            (import "morrow_io_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let capabilities: BTreeSet<_> = capabilities.iter().copied().collect();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration =
            io::declaration(capabilities.iter().copied().collect(), vec![HANDLER.into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_resources = 8;
        budget.max_jobs = io::MAX_JOBS;
        budget.max_bytes = io::MAX_BYTES;
        budget.max_job_bytes = io::MAX_JOB_BYTES;
        budget.max_duration_ms = 10_000;
        manifest.io_declaration = Some(declaration);
        manifest.required_features.push(io::FEATURE.into());
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&temp.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&temp.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                capabilities.clone(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host = HostRuntime::new(
            Store::open(
                &temp.path().join("ordinary-keyless-store"),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap();
        let (entered, _) = mpsc::channel();
        Self {
            temp,
            data,
            owner: Owner {
                host,
                manager,
                package,
                identity: Arc::new(()),
                hooks: Arc::new(Mutex::new(Vec::new())),
                gate: Arc::new(Gate::default()),
                entered,
                swap: None,
                fail_finish: false,
                slow_context: None,
            },
            clock: Arc::new(AtomicU64::new(2)),
            capabilities,
            limits: JobLimits::default(),
        }
    }
    fn connect(&mut self) -> (ManagedInstance, IoBinding) {
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
                self.owner.package.digest(),
                self.owner.manager.revision(),
                &self.capabilities,
                9000,
                1,
            )
            .unwrap();
        (instance, binding)
    }
    fn populate(&self, count: usize) -> BTreeSet<Vec<u8>> {
        let mut names = BTreeSet::new();
        for n in 0..count {
            let name = format!("文件-🙂-{n:03}");
            fs::write(self.data.join(&name), [n as u8]).unwrap();
            names.insert(
                std::ffi::OsStr::new(&name)
                    .encode_wide()
                    .flat_map(u16::to_le_bytes)
                    .collect(),
            );
        }
        names
    }
    fn start(mut self) -> Running {
        let (instance, binding) = self.connect();
        let (sender, entered) = mpsc::channel();
        self.owner.entered = sender;
        let gate = self.owner.gate.clone();
        let identity = self.owner.identity.clone();
        let host = self.owner.host.binding();
        let hooks = self.owner.hooks.clone();
        let clock = self.clock.clone();
        let sampled = clock.clone();
        let worker = IoWorker::spawn_managed_owner(
            self.owner,
            instance,
            binding,
            move || sampled.load(Ordering::SeqCst),
            io::MAX_JOBS as usize,
            self.limits,
        )
        .unwrap();
        Running {
            _temp: self.temp,
            data: self.data,
            worker,
            clock,
            entered,
            gate,
            identity,
            host,
            hooks,
        }
    }
}
struct Running {
    _temp: tempfile::TempDir,
    data: PathBuf,
    worker: IoWorker<Owner>,
    clock: Arc<AtomicU64>,
    entered: mpsc::Receiver<u8>,
    gate: Arc<Gate>,
    identity: Arc<()>,
    host: HostBinding,
    hooks: Arc<Mutex<Vec<(&'static str, thread::ThreadId)>>>,
}
impl Running {
    fn capture(&self, secret: u8) -> (DirectorySession, SelectedDirectory) {
        let (session, mut handle) = self
            .worker
            .capture_directory(open_directory(&self.data), small_limits(), [secret; 32])
            .unwrap();
        let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
            panic!("capture returned another response")
        };
        (session, selected)
    }
    fn block(&self) -> OwnerCommandHandle {
        let handle = self.worker.submit_owner_command(vec![BLOCK], 0).unwrap();
        assert_eq!(self.entered.recv_timeout(WAIT).unwrap(), BLOCK);
        assert!(handle.is_started());
        handle
    }
    fn finish(&self, session: DirectorySession) {
        let mut handle = self.worker.finish_directory(session).unwrap();
        assert!(matches!(
            result(&mut handle),
            Ok(Ok(DirectoryResponse::Finished))
        ));
    }
    fn reclaim(&mut self, stop: bool) -> WorkerExit<Owner> {
        self.gate.release();
        if stop {
            self.worker.stop();
        }
        let exit = reclaim(&mut self.worker);
        assert!(Arc::ptr_eq(&self.identity, &exit.owner.identity));
        assert_eq!(exit.owner.host.binding(), self.host);
        assert_eq!(self.worker.directory_usage(), (0, 0));
        exit
    }
}
impl Drop for Running {
    fn drop(&mut self) {
        self.gate.release();
        self.worker.stop();
    }
}
fn small_limits() -> CaptureLimits {
    CaptureLimits {
        max_job_bytes: SMALL_JOB,
        ..CaptureLimits::default()
    }
}
fn open_directory(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(7)
        .custom_flags(0x0220_0000)
        .open(path)
        .unwrap()
}
fn wait_for(mut predicate: impl FnMut() -> bool, reason: &str) {
    let until = Instant::now() + WAIT;
    while !predicate() {
        assert!(Instant::now() < until, "bounded wait: {reason}");
        thread::sleep(Duration::from_millis(1));
    }
}
fn ready(handle: &DirectoryCommandHandle) {
    wait_for(
        || handle.poll() == OwnerCommandPoll::Ready,
        "directory reply Ready",
    );
}
fn result(
    handle: &mut DirectoryCommandHandle,
) -> Result<Result<DirectoryResponse, DirectoryCommandError>, OwnerCommandError> {
    let until = Instant::now() + WAIT;
    loop {
        match handle.read() {
            Ok(None) => {
                assert!(
                    Instant::now() < until,
                    "directory reply never became terminal"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Some(response)) => return Ok(response),
            Err(error) => return Err(error),
        }
    }
}
fn reclaim<O: HostOwner>(worker: &mut IoWorker<O>) -> WorkerExit<O> {
    let until = Instant::now() + WAIT;
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(
            Instant::now() < until,
            "original owner did not join/reclaim within bound"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn page(
    run: &Running,
    session: DirectorySession,
    request: morrow_plugin_runtime::directory_io::PageRequest,
) -> FsDirectoryPage {
    let mut handle = run.worker.next_directory_page(session, request).unwrap();
    let DirectoryResponse::Page(page) = result(&mut handle).unwrap().unwrap() else {
        panic!("expected page")
    };
    page
}
fn assert_closed<T>(result: Result<T, OwnerCommandError>) {
    assert!(matches!(result, Err(OwnerCommandError::Closed)));
}
fn assert_busy<T>(result: Result<T, OwnerCommandError>) {
    assert!(matches!(result, Err(OwnerCommandError::Busy)));
}

#[test]
fn file_list_capture_pages_and_terminal_release_use_original_owner() {
    let fixture = Fixture::new();
    let expected = fixture.populate(70);
    let mut run = fixture.start();
    let (session, selected) = run.capture(11);
    assert_eq!(selected.entries, 70);
    assert_ne!(selected.selection_epoch, [0; 32]);
    assert_eq!(run.worker.directory_usage(), (1, selected.metadata_bytes));
    let mut request = selected.first_page();
    let mut actual = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut pages = 0;
    loop {
        let current = page(&run, session, request);
        pages += 1;
        let wire = current.encode().unwrap();
        let decoded = FsDirectoryPage::decode(&wire).unwrap();
        assert_eq!(current, decoded);
        assert!(wire.len() <= 65536);
        assert_eq!(current.page_sequence, request.page_sequence);
        assert_eq!(current.selection_epoch, selected.selection_epoch);
        for entry in &current.entries {
            assert_eq!(entry.encoding, NameEncoding::Utf16Le);
            assert!(actual.insert(entry.name.clone()));
            assert!(ids.insert(entry.entry_id));
        }
        if current.terminal {
            break;
        }
        request.page_sequence += 1;
        request.after_entry_id = current.entries.last().map(|e| e.entry_id);
    }
    assert_eq!(pages, 3);
    assert_eq!(actual, expected);
    assert_eq!(ids.len(), 70);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_closed(run.worker.finish_directory(session));
    let exit = run.reclaim(true);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.maintenance, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert!(exit.instance.is_none());
    let hooks = run.hooks.lock().unwrap();
    assert!(!hooks.is_empty());
    let worker_thread = hooks[0].1;
    assert_ne!(worker_thread, thread::current().id());
    assert!(hooks.iter().all(|(_, id)| *id == worker_thread));
    assert!(hooks.iter().any(|(name, _)| *name == "finish"));
}

#[test]
fn default_owner_opt_in_and_exact_file_list_capability_are_required() {
    let mut fixture = Fixture::new();
    let (instance, binding) = fixture.connect();
    let clock = fixture.clock.clone();
    let identity = fixture.owner.identity.clone();
    let host = fixture.owner.host.binding();
    let mut failure = IoWorker::spawn_managed_owner(
        NoOptIn(fixture.owner),
        instance,
        binding,
        move || clock.load(Ordering::SeqCst),
        io::MAX_JOBS as usize,
        JobLimits::default(),
    )
    .err()
    .expect("absent original manager must deny worker spawn");
    assert_eq!(failure.error, JobError::InvalidOptions);
    assert!(Arc::ptr_eq(&identity, &failure.owner.0.identity));
    assert_eq!(failure.owner.0.host.binding(), host);
    assert!(failure.owner.0.hooks.lock().unwrap().is_empty());
    failure
        .instance
        .take()
        .unwrap()
        .close(&mut failure.owner.0.host)
        .unwrap();
    drop(failure);
    drop(fixture.temp);
    for capability in [IoCapability::FileRead, IoCapability::FileCreate] {
        let mut run = Fixture::capabilities(&[capability]).start();
        let (_, mut handle) = run
            .worker
            .capture_directory(open_directory(&run.data), small_limits(), [13; 32])
            .unwrap();
        assert!(matches!(
            result(&mut handle),
            Ok(Err(DirectoryCommandError::Admission(
                AdmissionError::Denied
            )))
        ));
        assert_eq!(run.worker.directory_usage(), (0, 0));
        assert_eq!(run.reclaim(true).disconnect, Ok(()));
    }
}

#[test]
fn runtime_replacement_during_prepare_refuses_directory_dispatch() {
    let mut fixture = Fixture::new();
    let identity = fixture.owner.identity.clone();
    let original = fixture.owner.host.binding();
    fixture.owner.swap = Some(
        HostRuntime::new(
            Store::open(
                &fixture.temp.path().join("alternate-keyless"),
                Default::default(),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let mut run = fixture.start();
    let (_, mut handle) = run
        .worker
        .capture_directory(open_directory(&run.data), small_limits(), [14; 32])
        .unwrap();
    assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Closed));
    assert!(!handle.is_started());
    let mut exit = reclaim(&mut run.worker);
    assert!(Arc::ptr_eq(&identity, &exit.owner.identity));
    assert_eq!(exit.result, Err(JobError::InvalidOptions));
    assert_eq!(exit.disconnect, Err(JobError::InvalidOptions));
    assert_eq!(run.worker.directory_usage(), (0, 0));
    std::mem::swap(&mut exit.owner.host, exit.owner.swap.as_mut().unwrap());
    assert_eq!(exit.owner.host.binding(), original);
    exit.instance
        .take()
        .unwrap()
        .close(&mut exit.owner.host)
        .unwrap();
}

#[test]
fn foreign_worker_stale_session_epoch_and_cursor_preserve_both_observations() {
    let a = Fixture::new();
    a.populate(40);
    let b = Fixture::new();
    b.populate(40);
    let mut a = a.start();
    let mut b = b.start();
    let (sa, ma) = a.capture(15);
    let (sb, mb) = b.capture(16);
    let ua = a.worker.directory_usage();
    let ub = b.worker.directory_usage();
    let fees = (a.worker.bytes(), b.worker.bytes());
    assert_closed(b.worker.next_directory_page(sa, ma.first_page()));
    assert_closed(a.worker.finish_directory(sb));
    let mut bad = ma.first_page();
    bad.selection_epoch[0] ^= 1;
    assert_closed(a.worker.next_directory_page(sa, bad));
    bad = ma.first_page();
    bad.after_entry_id = Some([0x91; 32]);
    assert_closed(a.worker.next_directory_page(sa, bad));
    bad = ma.first_page();
    bad.page_sequence = 2;
    assert_closed(a.worker.next_directory_page(sa, bad));
    assert_eq!(a.worker.directory_usage(), ua);
    assert_eq!(b.worker.directory_usage(), ub);
    assert_eq!((a.worker.bytes(), b.worker.bytes()), fees);
    assert!(!page(&a, sa, ma.first_page()).terminal);
    assert!(!page(&b, sb, mb.first_page()).terminal);
    a.finish(sa);
    let (fresh, meta) = a.capture(17);
    assert_ne!(sa, fresh);
    assert_closed(a.worker.next_directory_page(sa, ma.first_page()));
    assert!(!page(&a, fresh, meta.first_page()).terminal);
    a.finish(fresh);
    b.finish(sb);
    assert!(a.worker.bytes() >= fees.0 && b.worker.bytes() >= fees.1);
    assert_eq!(a.reclaim(true).disconnect, Ok(()));
    assert_eq!(b.reclaim(true).disconnect, Ok(()));
}

#[test]
fn eight_global_live_and_queued_captures_reserve_capacity() {
    assert_eq!(MAX_DIRECTORY_OBSERVATIONS, 8);
    let mut run = Fixture::new().start();
    let mut sessions = Vec::new();
    for secret in 20..24 {
        sessions.push(run.capture(secret).0);
    }
    let mut blocker = run.block();
    let mut queued = Vec::new();
    for secret in 24..28 {
        queued.push(
            run.worker
                .capture_directory(open_directory(&run.data), small_limits(), [secret; 32])
                .unwrap(),
        );
    }
    assert_eq!(run.worker.directory_usage().0, 8);
    assert_eq!(run.worker.owner_command_usage().0, 5);
    assert_busy(
        run.worker
            .capture_directory(open_directory(&run.data), small_limits(), [28; 32]),
    );
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "barrier reply",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    for (session, mut h) in queued {
        assert!(matches!(
            result(&mut h),
            Ok(Ok(DirectoryResponse::Captured(_)))
        ));
        sessions.push(session);
    }
    assert_eq!(run.worker.directory_usage().0, 8);
    for session in sessions {
        run.finish(session);
    }
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));

    // Two subcases reproduce one admission-capacity defect. They are not two
    // extra test methods. Each original binding contains only directory leases.
    for use_page in [false, true] {
        let fixture = Fixture::new();
        fixture.populate(1);
        let mut quota = fixture.start();
        let mut captures = Vec::new();
        for secret in 50..58 {
            captures.push(quota.capture(secret));
        }
        let before = quota.worker.directory_usage();
        let original_usage = quota.worker.directory_binding_usage().unwrap();
        let fees = quota.worker.bytes();
        assert_eq!(before.0, 8);
        assert!(before.1 > 0);
        assert_eq!(original_usage.resources, 8);
        assert_eq!(original_usage.jobs, 0);

        // Keep one completed but unread original data Ticket, then block the
        // original owner and fill its remaining six command reservations.
        let completed = quota.worker.submit_owner_command(vec![NOOP], 0).unwrap();
        assert_eq!(quota.entered.recv_timeout(WAIT).unwrap(), NOOP);
        wait_for(
            || completed.poll() == OwnerCommandPoll::Ready,
            "completed unread data Ticket",
        );
        assert!(completed.is_started());
        let mut blocker = quota.block();
        let mut waiting = Vec::new();
        for _ in 0..6 {
            waiting.push(quota.worker.submit_owner_command(vec![NOOP], 0).unwrap());
        }
        assert_eq!(quota.worker.owner_command_usage().0, 8);
        let (retired, metadata) = captures[0];
        if use_page {
            assert_busy(
                quota
                    .worker
                    .next_directory_page(retired, metadata.first_page()),
            );
        } else {
            assert_busy(quota.worker.finish_directory(retired));
        }
        // Busy has admitted retirement, but the root is still on the blocked
        // worker. Tombstone capacity and metadata must survive until actual drop.
        assert_eq!(quota.worker.directory_usage(), before);
        assert_eq!(
            quota.worker.directory_binding_usage().unwrap(),
            original_usage
        );
        assert_eq!(quota.worker.bytes(), fees);
        drop(completed);
        assert_eq!(quota.worker.owner_command_usage().0, 7);
        assert_busy(quota.worker.capture_directory(
            open_directory(&quota.data),
            small_limits(),
            [60; 32],
        ));
        assert_eq!(quota.worker.directory_usage(), before);
        assert_eq!(
            quota.worker.directory_binding_usage().unwrap(),
            original_usage
        );
        assert_eq!(quota.worker.bytes(), fees);
        assert!(quota.worker.try_reclaim().unwrap().is_none());

        quota.gate.release();
        wait_for(
            || blocker.poll() == OwnerCommandPoll::Ready,
            "quota barrier completion",
        );
        assert_eq!(blocker.read(), Ok(Some(Vec::new())));
        for mut ticket in waiting {
            wait_for(
                || ticket.poll() == OwnerCommandPoll::Ready,
                "queued data completion",
            );
            assert_eq!(ticket.read(), Ok(Some(Vec::new())));
        }
        // No second directory request performs cleanup: original maintenance
        // must drop the exact retained broker/root before releasing its slot.
        wait_for(
            || quota.worker.directory_usage() == (7, before.1 - metadata.metadata_bytes),
            "retired root dropped before quota release",
        );
        let maintained = quota.worker.directory_binding_usage().unwrap();
        assert_eq!(maintained.resources, 7);
        assert_eq!(maintained.jobs, 0);
        assert_eq!(maintained.bytes, original_usage.bytes);
        assert_eq!(quota.worker.bytes(), fees);
        assert_closed(quota.worker.finish_directory(retired));
        let (fresh, _) = quota.capture(61);
        assert_ne!(fresh, retired);
        assert_eq!(quota.worker.directory_usage().0, 8);
        assert_eq!(quota.worker.directory_binding_usage().unwrap().resources, 8);
        quota.finish(fresh);
        for (session, _) in captures.into_iter().skip(1) {
            quota.finish(session);
        }
        assert_eq!(quota.worker.directory_usage(), (0, 0));
        assert_eq!(quota.worker.directory_binding_usage().unwrap().resources, 0);
        assert!(quota.worker.directory_binding_usage().unwrap().bytes >= original_usage.bytes);
        assert!(quota.worker.bytes() > fees);
        assert_eq!(quota.reclaim(true).disconnect, Ok(()));
    }
}

#[test]
fn one_pending_or_unread_page_blocks_second_page_until_consumed() {
    let fixture = Fixture::new();
    fixture.populate(70);
    let mut run = fixture.start();
    let (session, mut capture) = run
        .worker
        .capture_directory(open_directory(&run.data), small_limits(), [30; 32])
        .unwrap();
    ready(&capture);
    assert_busy(run.worker.finish_directory(session));
    let DirectoryResponse::Captured(meta) = result(&mut capture).unwrap().unwrap() else {
        panic!()
    };
    let mut blocker = run.block();
    let mut first = run
        .worker
        .next_directory_page(session, meta.first_page())
        .unwrap();
    assert_busy(run.worker.next_directory_page(session, meta.first_page()));
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "barrier reply",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    ready(&first);
    assert_busy(run.worker.finish_directory(session));
    // The old cursor is already advanced after native execution; it is Closed,
    // while the still-valid finish command is Busy until this page is consumed.
    assert_closed(run.worker.next_directory_page(session, meta.first_page()));
    let DirectoryResponse::Page(p) = result(&mut first).unwrap().unwrap() else {
        panic!()
    };
    assert!(!p.terminal);
    assert_eq!(run.worker.directory_usage(), (1, meta.metadata_bytes));
    let mut next = meta.first_page();
    next.page_sequence = 2;
    next.after_entry_id = p.entries.last().map(|e| e.entry_id);
    assert!(!page(&run, session, next).terminal);
    run.finish(session);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn queued_page_cancel_before_started_retires_without_replay() {
    let fixture = Fixture::new();
    fixture.populate(40);
    let mut run = fixture.start();
    let (session, meta) = run.capture(31);
    let mut blocker = run.block();
    let mut h = run
        .worker
        .next_directory_page(session, meta.first_page())
        .unwrap();
    let fee = run.worker.bytes();
    assert!(!h.is_started());
    h.cancel();
    assert_eq!(result(&mut h).err(), Some(OwnerCommandError::Cancelled));
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "barrier completed",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "cancelled page idle retirement",
    );
    assert_eq!(run.worker.bytes(), fee);
    assert_closed(run.worker.next_directory_page(session, meta.first_page()));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn ready_unread_capture_cancel_after_started_is_unknown_and_not_replayed() {
    let mut run = Fixture::new().start();
    let (session, mut h) = run
        .worker
        .capture_directory(open_directory(&run.data), small_limits(), [32; 32])
        .unwrap();
    ready(&h);
    assert!(h.is_started());
    let fee = run.worker.bytes();
    assert_eq!(run.worker.directory_usage().0, 1);
    h.cancel();
    // Ready proves execution completed, not interruption of a synchronous OS query.
    assert_eq!(result(&mut h).err(), Some(OwnerCommandError::Unknown));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "unread capture cancellation retirement",
    );
    assert_eq!(run.worker.bytes(), fee);
    assert_closed(run.worker.finish_directory(session));
    let (fresh, _) = run.capture(33);
    assert_ne!(session, fresh);
    run.finish(fresh);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn dropped_unread_page_retires_only_its_selection_and_other_instance_runs() {
    let fixture = Fixture::new();
    fixture.populate(40);
    let mut run = fixture.start();
    let mut other = Fixture::new().start();
    let (session, meta) = run.capture(34);
    let (other_session, other_meta) = other.capture(35);
    let h = run
        .worker
        .next_directory_page(session, meta.first_page())
        .unwrap();
    ready(&h);
    assert!(h.is_started());
    let fee = run.worker.bytes();
    drop(h);
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "dropped unread page idle retirement",
    );
    assert_eq!(run.worker.bytes(), fee);
    assert_closed(run.worker.next_directory_page(session, meta.first_page()));
    assert!(page(&other, other_session, other_meta.first_page()).terminal);
    assert_eq!(other.worker.directory_usage(), (0, 0));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
    assert_eq!(other.reclaim(true).disconnect, Ok(()));
}

#[test]
fn original_clock_expiry_and_backwards_time_release_idle_without_new_request() {
    for now in [9000, 1] {
        let mut run = Fixture::new().start();
        let (_session, meta) = run.capture(36);
        let fee = run.worker.bytes();
        assert_eq!(run.worker.directory_usage(), (1, meta.metadata_bytes));
        run.clock.store(now, Ordering::SeqCst);
        // Read-only counters and original try_reclaim do not submit a second directory command.
        wait_for(
            || run.worker.directory_usage() == (0, 0),
            "idle original-clock cleanup",
        );
        assert_eq!(run.worker.bytes(), fee);
        let exit = run.reclaim(false);
        assert_eq!(exit.disconnect, Ok(()));
        assert!(exit.instance.is_none());
        assert_eq!(exit.maintenance, Ok(()));
    }
}

#[test]
fn original_registry_revoke_suppresses_unread_reply_and_releases_idle() {
    let fixture = Fixture::new();
    fixture.populate(40);
    let mut run = fixture.start();
    let (session, meta) = run.capture(37);
    let mut h = run
        .worker
        .next_directory_page(session, meta.first_page())
        .unwrap();
    ready(&h);
    let fee = run.worker.bytes();
    let mut revoke = run.worker.submit_owner_command(vec![REVOKE], 0).unwrap();
    assert_eq!(run.entered.recv_timeout(WAIT).unwrap(), REVOKE);
    wait_for(
        || revoke.poll() == OwnerCommandPoll::Ready,
        "original revoke result",
    );
    assert_eq!(revoke.read(), Err(OwnerCommandError::Unknown));
    assert_eq!(result(&mut h).err(), Some(OwnerCommandError::Unknown));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "revoke idle resource cleanup",
    );
    assert_eq!(run.worker.bytes(), fee);
    let exit = run.reclaim(false);
    assert_eq!(exit.disconnect, Ok(()));
    assert!(exit.instance.is_none());
}

#[test]
fn stop_and_failed_maintenance_return_joined_original_owner_without_resources() {
    for fail_finish in [false, true] {
        let mut fixture = Fixture::new();
        fixture.owner.fail_finish = fail_finish;
        let mut run = fixture.start();
        let (session, _) = run.capture(38);
        let mut blocker = run.block();
        let mut pending = run.worker.finish_directory(session).unwrap();
        assert!(!pending.is_started());
        run.worker.stop();
        assert!(run.worker.try_reclaim().unwrap().is_none());
        assert_eq!(result(&mut pending).err(), Some(OwnerCommandError::Closed));
        run.gate.release();
        wait_for(
            || blocker.poll() == OwnerCommandPoll::Ready,
            "stopped barrier",
        );
        assert_eq!(blocker.read(), Err(OwnerCommandError::Unknown));
        let exit = run.reclaim(false);
        assert_eq!(exit.disconnect, Ok(()));
        assert!(exit.instance.is_none());
        assert_eq!(
            exit.maintenance,
            if fail_finish {
                Err(JobError::Busy)
            } else {
                Ok(())
            }
        );
        assert!(
            run.hooks
                .lock()
                .unwrap()
                .iter()
                .any(|(name, _)| *name == "finish")
        );
    }
}

#[test]
fn effective_capture_ceiling_and_cumulative_admission_are_not_refunded() {
    let mut fixture = Fixture::new();
    fixture.limits = JobLimits::new(16, io::MAX_JOB_BYTES, io::MAX_JOB_BYTES).unwrap();
    let mut run = fixture.start();
    let requested = CaptureLimits::default();
    let effective = run.worker.directory_capture_budget(requested).unwrap();
    assert!(effective.max_job_bytes <= requested.max_job_bytes);
    assert_eq!(
        effective.max_job_bytes + DIRECTORY_COMMAND_FIXED_BYTES,
        io::MAX_JOB_BYTES
    );
    assert_eq!(effective.max_entries, requested.max_entries);
    assert_eq!(effective.max_metadata_bytes, requested.max_metadata_bytes);
    let narrow = run.worker.directory_capture_budget(small_limits()).unwrap();
    assert_eq!(narrow.max_job_bytes, SMALL_JOB);
    let mut invalid = requested;
    invalid.max_job_bytes = 0;
    assert!(matches!(
        run.worker.directory_capture_budget(invalid),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(run.worker.bytes(), 0);
    let (_, mut h) = run
        .worker
        .capture_directory(open_directory(&run.data), requested, [39; 32])
        .unwrap();
    ready(&h);
    assert!(h.is_started());
    let charged = run.worker.bytes();
    assert_eq!(charged, io::MAX_JOB_BYTES);
    h.cancel();
    assert_eq!(result(&mut h).err(), Some(OwnerCommandError::Unknown));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "budget case cancellation cleanup",
    );
    assert_eq!(run.worker.bytes(), charged);
    assert!(matches!(
        run.worker
            .capture_directory(open_directory(&run.data), small_limits(), [40; 32]),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(run.worker.bytes(), charged);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn slow_original_directory_context_does_not_block_poll_read_or_admission() {
    // These three cases are one named method. The seam delays an original
    // owner getter, not real Windows IO or a GUI; static lock review is separate.
    for tick in [2, 9000, 1] {
        let (entered_tx, entered_rx) = mpsc::channel();
        let context = Arc::new(SlowDirectoryContext {
            requested: AtomicBool::new(false),
            active: AtomicBool::new(false),
            readonly_calls: AtomicUsize::new(0),
            entered: entered_tx,
            gate: Gate::default(),
        });
        let mut fixture = Fixture::new();
        fixture.populate(1);
        fixture.owner.slow_context = Some(context.clone());
        let mut run = fixture.start();
        // This guard drops before Running on any unwind. Spawn identity checks
        // have already completed and cannot activate the delay.
        let _release_on_drop = ReleaseContextOnDrop(context.clone());
        let second_file = open_directory(&run.data);
        context.arm_next_prepare();
        let (first_session, mut first) = run
            .worker
            .capture_directory(open_directory(&run.data), small_limits(), [71; 32])
            .unwrap();
        let entered = entered_rx.recv_timeout(WAIT);
        if entered.is_err() {
            context.gate.release();
        }
        assert_eq!(
            entered.unwrap(),
            2,
            "broker argument must be readonly getter2"
        );
        assert!(first.is_started());
        let first_fee = run.worker.bytes();
        run.clock.store(tick, Ordering::SeqCst);
        let (observed_tx, observed_rx) = mpsc::channel();
        let (responsive, completed) = thread::scope(|scope| {
            let worker = &run.worker;
            let probe = scope.spawn(move || {
                let poll = first.poll();
                let read = first.read();
                let enqueue = worker.capture_directory(second_file, small_limits(), [72; 32]);
                let _ = observed_tx.send(());
                (first, poll, read, enqueue)
            });
            let responsive = observed_rx.recv_timeout(Duration::from_secs(2)).is_ok();
            // Release BEFORE join/assert/cancel/stop, even on timeout: the old
            // worker may hold clock while probe holds Control waiting for it.
            context.gate.release();
            (responsive, probe.join())
        });
        let (mut first, poll, read, enqueue) =
            completed.expect("probe must join after gate release");
        assert!(
            responsive,
            "original directory context blocked poll/read/admission beyond2s; tick={tick}"
        );
        if tick == 2 {
            assert_eq!(poll, OwnerCommandPoll::Pending);
            assert!(matches!(read, Ok(None)));
            let (second_session, mut second) = enqueue.unwrap();
            assert!(matches!(
                result(&mut first),
                Ok(Ok(DirectoryResponse::Captured(_)))
            ));
            assert!(matches!(
                result(&mut second),
                Ok(Ok(DirectoryResponse::Captured(_)))
            ));
            assert_eq!(run.worker.directory_usage().0, 2);
            let admitted = run.worker.bytes();
            assert!(admitted > first_fee);
            run.finish(first_session);
            run.finish(second_session);
            assert!(run.worker.bytes() >= admitted);
            assert_eq!(run.reclaim(true).disconnect, Ok(()));
        } else {
            assert_eq!(poll, OwnerCommandPoll::Ready);
            assert!(matches!(read, Err(OwnerCommandError::Unknown)));
            assert_closed(enqueue);
            wait_for(
                || run.worker.directory_usage() == (0, 0),
                "late original clock cleanup without another request",
            );
            assert_eq!(run.worker.bytes(), first_fee);
            let exit = run.reclaim(false);
            assert_eq!(exit.disconnect, Ok(()));
            assert!(exit.instance.is_none());
        }
    }
}
