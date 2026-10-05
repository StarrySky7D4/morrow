//! C08 private factory tests over ordinary keyless Store and bounded synthetic directories.
//! Private scoped qualification. No guest import, picker proof or full SDK freeze claim.
#![cfg(all(feature = "packages", windows))]

use super::{
    EntropyProbe, EntropyTestMode, FreshSecret, fresh_directory_secret, next_directory_serial,
};

use crate::{
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
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
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
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
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
        let revocation = self.owner.host.revocation(instance.connection()).unwrap();
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
            revocation,
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
    revocation: morrow_core::lifecycle::Revocation,
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
    request: crate::directory_io::PageRequest,
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

fn install(run: &Running, mode: EntropyTestMode) -> Arc<EntropyProbe> {
    let probe = Arc::new(EntropyProbe::new(mode));
    run.worker.install_entropy_probe(probe.clone());
    probe
}

fn fresh(run: &Running) -> (DirectorySession, SelectedDirectory) {
    let (session, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
        panic!("fresh capture returned another response")
    };
    (session, selected)
}

fn idle_empty(run: &Running) {
    wait_for(
        || {
            run.worker.directory_usage() == (0, 0)
                && run.worker.directory_binding_usage().unwrap().resources == 0
                && run.worker.directory_binding_usage().unwrap().jobs == 0
        },
        "original maintenance must actually release fresh directory resources",
    );
}

struct ReleaseEntropyOnDrop(Arc<Gate>);
impl Drop for ReleaseEntropyOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

fn gate_probe(probe: &Arc<EntropyProbe>, after: bool) -> (Arc<Gate>, mpsc::Receiver<()>) {
    let gate = Arc::new(Gate::default());
    let (sent, received) = mpsc::channel();
    let delayed = gate.clone();
    let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
        sent.send(()).unwrap();
        delayed.wait();
    });
    if after {
        *probe.after_fill.lock().unwrap() = Some(hook);
    } else {
        *probe.before_fill.lock().unwrap() = Some(hook);
    }
    (gate, received)
}

fn wait_entropy(entered: &mpsc::Receiver<()>, gate: &Gate) {
    let observed = entered.recv_timeout(WAIT);
    // Release before assertions on every failed entry observation.
    if observed.is_err() {
        gate.release();
    }
    observed.expect("entropy phase gate was not reached within bound");
}

#[test]
fn fresh_os_entropy_uses_original_worker_and_typed_directory_lifecycle() {
    for count in [0, 70] {
        let fixture = Fixture::new();
        let expected = fixture.populate(count);
        let mut run = fixture.start();
        let probe = install(&run, EntropyTestMode::OperatingSystem);
        let (session, selected) = fresh(&run);
        assert_eq!(selected.entries, count);
        assert!(selected.reference.iter().any(|byte| *byte != 0));
        assert!(selected.selection_epoch.iter().any(|byte| *byte != 0));
        assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
        let entropy_thread = probe
            .thread
            .lock()
            .unwrap()
            .expect("worker thread recorded");
        assert_ne!(entropy_thread, thread::current().id());
        let mut request = selected.first_page();
        let mut actual = BTreeSet::new();
        let mut page_count = 0;
        loop {
            let current = page(&run, session, request);
            page_count += 1;
            assert_eq!(
                FsDirectoryPage::decode(&current.encode().unwrap()).unwrap(),
                current
            );
            assert_eq!(current.selection_epoch, selected.selection_epoch);
            for entry in &current.entries {
                assert_eq!(entry.encoding, NameEncoding::Utf16Le);
                assert!(actual.insert(entry.name.clone()));
            }
            if current.terminal {
                break;
            }
            request.page_sequence += 1;
            request.after_entry_id = current.entries.last().map(|entry| entry.entry_id);
        }
        assert_eq!(actual, expected);
        assert_eq!(page_count, if count == 0 { 1 } else { 3 });
        idle_empty(&run);
        assert_closed(run.worker.finish_directory(session));
        let exit = run.reclaim(true);
        assert_eq!(exit.result, Ok(()));
        assert_eq!(exit.maintenance, Ok(()));
        assert_eq!(exit.disconnect, Ok(()));
        assert!(exit.instance.is_none());
        assert!(
            run.hooks
                .lock()
                .unwrap()
                .iter()
                .all(|(_, id)| *id == entropy_thread)
        );
        assert!(probe.wipe_observed.load(Ordering::SeqCst));
    }
}

#[test]
fn fixed_test_entropy_is_separated_by_worker_and_capture_serial() {
    let make = |worker, serial| {
        fresh_directory_secret(
            DirectorySession { worker, serial },
            FreshSecret {
                probe: Some(Arc::new(EntropyProbe::new(EntropyTestMode::Nonzero))),
            },
            || Ok(()),
        )
        .unwrap()
    };
    let first = make(7, 11);
    let repeated = make(7, 11);
    let next_capture = make(7, 12);
    let next_worker = make(8, 11);
    // Only fixed synthetic test entropy is examined here. Secret values never
    // appear in assert diagnostics, and no production secret getter exists.
    assert!(*first == *repeated);
    assert!(*first != *next_capture);
    assert!(*first != *next_worker);
    assert!(first.iter().any(|byte| *byte != 0));
    let blocked = Arc::new(EntropyProbe::new(EntropyTestMode::Nonzero));
    let calls = AtomicU64::new(0);
    let refused = fresh_directory_secret(
        DirectorySession {
            worker: 7,
            serial: 11,
        },
        FreshSecret {
            probe: Some(blocked.clone()),
        },
        || {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(DirectoryCommandError::Admission(AdmissionError::Denied))
        },
    );
    assert!(matches!(
        refused,
        Err(DirectoryCommandError::Admission(AdmissionError::Denied))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(blocked.wipe_observed.load(Ordering::SeqCst));
    let mut a = Fixture::new().start();
    let mut b = Fixture::new().start();
    let pa = install(&a, EntropyTestMode::Nonzero);
    let pb = install(&b, EntropyTestMode::Nonzero);
    let (a1, m1) = fresh(&a);
    let (a2, m2) = fresh(&a);
    let (b1, mut h) = b
        .worker
        .capture_directory_fresh(open_directory(&a.data), small_limits())
        .unwrap();
    let DirectoryResponse::Captured(m3) = result(&mut h).unwrap().unwrap() else {
        panic!()
    };
    assert!(m1.reference != m2.reference && m1.selection_epoch != m2.selection_epoch);
    assert!(m1.reference != m3.reference && m1.selection_epoch != m3.selection_epoch);
    assert!(a1 != a2 && a1 != b1);
    assert_eq!(pa.calls.load(Ordering::SeqCst), 2);
    assert_eq!(pb.calls.load(Ordering::SeqCst), 1);
    a.finish(a1);
    a.finish(a2);
    b.finish(b1);
    assert_eq!(a.reclaim(true).disconnect, Ok(()));
    assert_eq!(b.reclaim(true).disconnect, Ok(()));
}

#[test]
fn checked_local_directory_serial_exhaustion_never_wraps_or_reuses() {
    let ordinary = AtomicU64::new(1);
    assert_eq!(next_directory_serial(&ordinary), Ok(1));
    assert_eq!(next_directory_serial(&ordinary), Ok(2));
    let last = AtomicU64::new(u64::MAX - 1);
    assert_eq!(next_directory_serial(&last), Ok(u64::MAX - 1));
    assert_eq!(last.load(Ordering::Relaxed), u64::MAX);
    for _ in 0..2 {
        assert_eq!(next_directory_serial(&last), Err(OwnerCommandError::Limit));
        assert_eq!(last.load(Ordering::Relaxed), u64::MAX);
    }
    let zero = AtomicU64::new(0);
    assert_eq!(next_directory_serial(&zero), Err(OwnerCommandError::Limit));
    assert_eq!(zero.load(Ordering::Relaxed), 0);
    for id in [
        DirectorySession {
            worker: 0,
            serial: 1,
        },
        DirectorySession {
            worker: 1,
            serial: 0,
        },
    ] {
        let probe = Arc::new(EntropyProbe::new(EntropyTestMode::Nonzero));
        let key = fresh_directory_secret(
            id,
            FreshSecret {
                probe: Some(probe.clone()),
            },
            || Ok(()),
        );
        assert!(matches!(key, Err(DirectoryCommandError::Limit)));
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    }
    // The production global counter is neither overwritten nor rewound.
}

#[test]
fn partial_entropy_error_zeroizes_scratch_without_selection_fallback_or_refund() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::PartialError);
    let before = run.worker.bytes();
    let effective = run.worker.directory_capture_budget(small_limits()).unwrap();
    let (session, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    assert_eq!(
        result(&mut handle).unwrap().err(),
        Some(DirectoryCommandError::Entropy)
    );
    idle_empty(&run);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(probe.wipe_observed.load(Ordering::SeqCst));
    assert_eq!(
        run.worker.bytes(),
        before + effective.max_job_bytes + DIRECTORY_COMMAND_FIXED_BYTES
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, 0);
    assert_closed(run.worker.finish_directory(session));
    // A new explicitly submitted request is not an automatic retry.
    let successful = install(&run, EntropyTestMode::Nonzero);
    let (next, _) = fresh(&run);
    assert!(next != session);
    assert_eq!(successful.calls.load(Ordering::SeqCst), 1);
    run.finish(next);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn all_zero_entropy_is_refused_without_derive_or_native_publication() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::AllZero);
    let (_, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    let charged = run.worker.bytes();
    assert_eq!(
        result(&mut handle).unwrap().err(),
        Some(DirectoryCommandError::Entropy)
    );
    idle_empty(&run);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(probe.wipe_observed.load(Ordering::SeqCst));
    assert_eq!(run.worker.bytes(), charged);
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, 0);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn original_file_list_identity_and_time_gates_precede_entropy() {
    for capability in [IoCapability::FileRead, IoCapability::FileCreate] {
        let mut run = Fixture::capabilities(&[capability]).start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let (_, mut handle) = run
            .worker
            .capture_directory_fresh(open_directory(&run.data), small_limits())
            .unwrap();
        assert!(matches!(
            result(&mut handle),
            Ok(Err(DirectoryCommandError::Admission(
                AdmissionError::Denied
            )))
        ));
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        idle_empty(&run);
        assert_eq!(run.reclaim(true).disconnect, Ok(()));
    }
    let mut fixture = Fixture::new();
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
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (_, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Closed));
    assert!(!handle.is_started());
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    let mut exit = reclaim(&mut run.worker);
    assert_eq!(exit.result, Err(JobError::InvalidOptions));
    std::mem::swap(&mut exit.owner.host, exit.owner.swap.as_mut().unwrap());
    assert_eq!(exit.owner.host.binding(), original);
    exit.instance
        .take()
        .unwrap()
        .close(&mut exit.owner.host)
        .unwrap();
    for tick in [9000, 1] {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        run.clock.store(tick, Ordering::SeqCst);
        assert_closed(
            run.worker
                .capture_directory_fresh(open_directory(&run.data), small_limits()),
        );
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert_eq!(run.reclaim(false).disconnect, Ok(()));
    }
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let mut disable = run.worker.submit_owner_command(vec![REVOKE], 0).unwrap();
    assert_eq!(run.entered.recv_timeout(WAIT).unwrap(), REVOKE);
    wait_for(
        || disable.poll() == OwnerCommandPoll::Ready,
        "original registry disable completed",
    );
    assert_eq!(disable.read(), Err(OwnerCommandError::Unknown));
    assert_closed(
        run.worker
            .capture_directory_fresh(open_directory(&run.data), small_limits()),
    );
    assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
    assert_eq!(run.reclaim(false).disconnect, Ok(()));
}

#[test]
fn queued_fresh_cancel_and_drop_never_generate_or_replay() {
    for dropped in [false, true] {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let mut blocker = run.block();
        let (session, mut handle) = run
            .worker
            .capture_directory_fresh(open_directory(&run.data), small_limits())
            .unwrap();
        let charged = run.worker.bytes();
        assert!(!handle.is_started());
        if dropped {
            drop(handle);
        } else {
            handle.cancel();
            assert_eq!(
                result(&mut handle).err(),
                Some(OwnerCommandError::Cancelled)
            );
        }
        run.gate.release();
        wait_for(
            || blocker.poll() == OwnerCommandPoll::Ready,
            "old owner barrier completed",
        );
        assert_eq!(blocker.read(), Ok(Some(Vec::new())));
        idle_empty(&run);
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert_eq!(run.worker.bytes(), charged);
        assert_closed(run.worker.finish_directory(session));
        assert_eq!(run.reclaim(true).disconnect, Ok(()));
    }
}

#[test]
fn delayed_entropy_does_not_hold_original_clock_control_or_ticket_locks() {
    for tick in [2, 9000, 1] {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let (gate, entered) = gate_probe(&probe, false);
        let _release = ReleaseEntropyOnDrop(gate.clone());
        let (first_session, mut first) = run
            .worker
            .capture_directory_fresh(open_directory(&run.data), small_limits())
            .unwrap();
        wait_entropy(&entered, &gate);
        assert!(first.is_started());
        let first_fee = run.worker.bytes();
        run.clock.store(tick, Ordering::SeqCst);
        let second_file = open_directory(&run.data);
        let (sent, received) = mpsc::channel();
        let (responsive, completed) = thread::scope(|scope| {
            let worker = &run.worker;
            let observer = scope.spawn(move || {
                let poll = first.poll();
                let read = first.read();
                let enqueue = worker.capture_directory_fresh(second_file, small_limits());
                let _ = sent.send(());
                (first, poll, read, enqueue)
            });
            let responsive = received.recv_timeout(Duration::from_secs(2)).is_ok();
            // Release before join/assert/cancel/stop, including timeout.
            gate.release();
            (responsive, observer.join())
        });
        let (mut first, poll, read, enqueue) =
            completed.expect("observer joins after gate release");
        assert!(
            responsive,
            "test entropy seam blocked original handles; tick={tick}"
        );
        if tick == 2 {
            assert_eq!(poll, OwnerCommandPoll::Pending);
            assert!(matches!(read, Ok(None)));
            let (second, mut second_handle) = enqueue.unwrap();
            let DirectoryResponse::Captured(_) = result(&mut first).unwrap().unwrap() else {
                panic!()
            };
            let DirectoryResponse::Captured(_) = result(&mut second_handle).unwrap().unwrap()
            else {
                panic!()
            };
            assert_eq!(probe.calls.load(Ordering::SeqCst), 2);
            run.finish(first_session);
            run.finish(second);
            assert_eq!(run.reclaim(true).disconnect, Ok(()));
        } else {
            assert_eq!(poll, OwnerCommandPoll::Ready);
            assert_eq!(read.err(), Some(OwnerCommandError::Unknown));
            assert_closed(enqueue);
            idle_empty(&run);
            assert_eq!(run.worker.bytes(), first_fee);
            assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
            assert_eq!(run.reclaim(false).disconnect, Ok(()));
        }
    }
}

#[test]
fn cancellation_after_entropy_before_derive_is_unknown_and_never_replayed() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (gate, entered) = gate_probe(&probe, true);
    let _release = ReleaseEntropyOnDrop(gate.clone());
    let (session, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    wait_entropy(&entered, &gate);
    let fees = run.worker.bytes();
    // Original handle cancellation during the synthetic after-fill seam must
    // remain responsive; this is not interruption of Windows entropy.
    let (sent, received) = mpsc::channel();
    let (responsive, joined) = thread::scope(|scope| {
        let command = &handle;
        let cancel = scope.spawn(move || {
            command.cancel();
            let _ = sent.send(());
        });
        let responsive = received.recv_timeout(Duration::from_secs(2)).is_ok();
        gate.release();
        (responsive, cancel.join())
    });
    joined.unwrap();
    assert!(
        responsive,
        "original Ticket/Control lock held during entropy seam"
    );
    assert!(handle.is_started());
    assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Unknown));
    idle_empty(&run);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(probe.wipe_observed.load(Ordering::SeqCst));
    assert_eq!(run.worker.bytes(), fees);
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, 0);
    assert_closed(run.worker.finish_directory(session));
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn original_stop_revocation_expiry_and_backwards_time_after_fill_prevent_selection() {
    for cause in 0..4 {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let (gate, entered) = gate_probe(&probe, true);
        let _release = ReleaseEntropyOnDrop(gate.clone());
        let (_, mut handle) = run
            .worker
            .capture_directory_fresh(open_directory(&run.data), small_limits())
            .unwrap();
        wait_entropy(&entered, &gate);
        let charged = run.worker.bytes();
        let signal: Box<dyn FnOnce() + Send> = match cause {
            0 => {
                let stop = run.worker.stop_handle();
                Box::new(move || stop())
            }
            1 => {
                let revoke = run.revocation.clone();
                Box::new(move || revoke.revoke())
            }
            2 => {
                let clock = run.clock.clone();
                Box::new(move || clock.store(9000, Ordering::SeqCst))
            }
            3 => {
                let clock = run.clock.clone();
                Box::new(move || clock.store(1, Ordering::SeqCst))
            }
            _ => unreachable!(),
        };
        let (sent, received) = mpsc::channel();
        let (responsive, joined) = thread::scope(|scope| {
            let trigger = scope.spawn(move || {
                signal();
                let _ = sent.send(());
            });
            let responsive = received.recv_timeout(Duration::from_secs(2)).is_ok();
            gate.release();
            (responsive, trigger.join())
        });
        joined.unwrap();
        assert!(responsive, "original signal blocked during entropy seam");
        assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Unknown));
        idle_empty(&run);
        assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
        assert!(probe.wipe_observed.load(Ordering::SeqCst));
        assert_eq!(run.worker.bytes(), charged);
        assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, 0);
        let exit = run.reclaim(false);
        assert_eq!(exit.disconnect, Ok(()));
        assert!(exit.instance.is_none());
    }
}

#[test]
fn ready_unread_fresh_cancel_or_drop_retires_without_regeneration() {
    for dropped in [false, true] {
        let mut run = Fixture::new().start();
        let probe = install(&run, EntropyTestMode::Nonzero);
        let (session, mut handle) = run
            .worker
            .capture_directory_fresh(open_directory(&run.data), small_limits())
            .unwrap();
        ready(&handle);
        assert!(handle.is_started());
        assert_eq!(run.worker.directory_usage().0, 1);
        let charged = run.worker.bytes();
        let binding_bytes = run.worker.directory_binding_usage().unwrap().bytes;
        if dropped {
            drop(handle);
        } else {
            handle.cancel();
            assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Unknown));
        }
        idle_empty(&run);
        assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
        assert_eq!(run.worker.bytes(), charged);
        assert_eq!(
            run.worker.directory_binding_usage().unwrap().bytes,
            binding_bytes
        );
        assert_closed(run.worker.finish_directory(session));
        assert_eq!(run.reclaim(true).disconnect, Ok(()));
    }
}

#[test]
fn fresh_global_quota_includes_live_queued_and_in_generation_reservations() {
    let mut run = Fixture::new().start();
    install(&run, EntropyTestMode::Nonzero);
    let mut live = Vec::new();
    for _ in 0..7 {
        live.push(fresh(&run).0);
    }
    let probe = install(&run, EntropyTestMode::Nonzero);
    let (gate, entered) = gate_probe(&probe, false);
    let _release = ReleaseEntropyOnDrop(gate.clone());
    let (eighth, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    wait_entropy(&entered, &gate);
    let charged = run.worker.bytes();
    let usage = run.worker.directory_usage();
    assert_eq!(usage.0, MAX_DIRECTORY_OBSERVATIONS);
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 7);
    assert_busy(
        run.worker
            .capture_directory_fresh(open_directory(&run.data), small_limits()),
    );
    assert_eq!(run.worker.directory_usage(), usage);
    assert_eq!(run.worker.bytes(), charged);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    gate.release();
    assert!(matches!(
        result(&mut handle),
        Ok(Ok(DirectoryResponse::Captured(_)))
    ));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 8);
    live.push(eighth);
    for session in live {
        run.finish(session);
    }
    idle_empty(&run);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn fresh_effective_capture_ceiling_and_cumulative_charges_never_refund() {
    let mut fixture = Fixture::new();
    fixture.limits = JobLimits::new(16, io::MAX_JOB_BYTES, io::MAX_JOB_BYTES).unwrap();
    let mut run = fixture.start();
    let probe = install(&run, EntropyTestMode::PartialError);
    let effective = run
        .worker
        .directory_capture_budget(CaptureLimits::default())
        .unwrap();
    assert!(effective.max_job_bytes <= CaptureLimits::default().max_job_bytes);
    assert_eq!(
        effective.max_job_bytes + DIRECTORY_COMMAND_FIXED_BYTES,
        io::MAX_JOB_BYTES
    );
    let (_, mut handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), CaptureLimits::default())
        .unwrap();
    assert_eq!(
        result(&mut handle).unwrap().err(),
        Some(DirectoryCommandError::Entropy)
    );
    idle_empty(&run);
    assert_eq!(run.worker.bytes(), io::MAX_JOB_BYTES);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        run.worker
            .capture_directory_fresh(open_directory(&run.data), small_limits()),
        Err(OwnerCommandError::Limit)
    ));
    assert_eq!(run.worker.bytes(), io::MAX_JOB_BYTES);
    assert_eq!(probe.calls.load(Ordering::SeqCst), 1);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn legacy_explicit_secret_bypasses_factory_and_entropy_failures_are_redacted() {
    let mut run = Fixture::new().start();
    let probe = install(&run, EntropyTestMode::PartialError);
    // Legacy callers own their seed's freshness. The new fresh entropy zero
    // refusal must not silently impose a new policy on this existing API.
    for seed in [0, 0x42] {
        let (session, mut handle) = run
            .worker
            .capture_directory(open_directory(&run.data), small_limits(), [seed; 32])
            .unwrap();
        let handle_debug = format!("{handle:?}");
        assert_eq!(handle_debug, "DirectoryCommandHandle { .. }");
        let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
            panic!("legacy explicit seed returned another response")
        };
        assert_eq!(probe.calls.load(Ordering::SeqCst), 0);
        assert_eq!(selected.entries, 0);
        // Compare synthetic opaque IDs privately; neither arrays nor secrets
        // are passed to failure diagnostics. The exact handle format above
        // excludes every field, including all seed-derived observation IDs.
        assert!(!handle_debug.contains(&format!("{:?}", selected.reference)));
        assert!(!handle_debug.contains(&format!("{:?}", selected.selection_epoch)));
        assert!(page(&run, session, selected.first_page()).terminal);
        idle_empty(&run);
    }
    let (_, mut fresh_handle) = run
        .worker
        .capture_directory_fresh(open_directory(&run.data), small_limits())
        .unwrap();
    assert_eq!(format!("{fresh_handle:?}"), "DirectoryCommandHandle { .. }");
    let Err(failure) = result(&mut fresh_handle).unwrap() else {
        // Avoid unwrap_err: an unexpected Captured response's Debug could print
        // opaque observation IDs. This fixed diagnostic never formats response.
        panic!("entropy failure unexpectedly delivered a response")
    };
    assert_eq!(failure, DirectoryCommandError::Entropy);
    assert_eq!(format!("{failure:?}"), "Entropy");
    assert_eq!(failure.to_string(), "selected directory: Entropy");
    assert!(probe.wipe_observed.load(Ordering::SeqCst));
    idle_empty(&run);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}
