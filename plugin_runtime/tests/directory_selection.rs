//! Original-worker relative directory selection on newly created ordinary Windows fixtures.
//! No protected Session/DPAPI/GUI, public guest import or above-anchor picker proof.
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
    directory_io::{CaptureLimits, DirectoryRelativePath, SelectedDirectory},
    io_binding::{Error as AdmissionError, IoBinding},
    io_jobs::{
        CommandOwner, DirectoryCommandError, DirectoryCommandHandle, DirectoryResponse,
        DirectorySession, HostOwner, IoWorker, JobError, JobLimits, ManagedHostOwner,
        OwnerCommandError, OwnerCommandHandle, OwnerCommandPoll, WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        process::CommandExt,
    },
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
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.hooks
            .lock()
            .unwrap()
            .push(("finish", thread::current().id()));
        Ok(())
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

fn raw_name(name: &std::ffi::OsStr) -> Vec<u8> {
    name.encode_wide().flat_map(u16::to_le_bytes).collect()
}
fn relative(parts: &[&str]) -> DirectoryRelativePath {
    let mut components = Vec::with_capacity(parts.len());
    for part in parts {
        let mut units = Vec::with_capacity(part.encode_utf16().count());
        units.extend(part.encode_utf16());
        components.push(units);
    }
    DirectoryRelativePath::new(components).unwrap()
}
fn make_chain(root: &Path, parts: &[&str]) -> PathBuf {
    let mut leaf = root.to_path_buf();
    for part in parts {
        leaf.push(part);
        fs::create_dir(&leaf).unwrap();
    }
    leaf
}
fn capture_under(
    run: &Running,
    path: DirectoryRelativePath,
) -> (DirectorySession, SelectedDirectory) {
    let (session, mut handle) = run
        .worker
        .capture_directory_under(open_directory(&run.data), path, small_limits())
        .unwrap();
    let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
        panic!("relative capture returned another response")
    };
    (session, selected)
}
fn collect_pages(
    run: &Running,
    session: DirectorySession,
    selected: SelectedDirectory,
) -> Vec<FsDirectoryPage> {
    let mut cursor = selected.first_page();
    let mut pages = Vec::new();
    loop {
        let current = page(run, session, cursor);
        assert_eq!(
            FsDirectoryPage::decode(&current.encode().unwrap()).unwrap(),
            current
        );
        assert_eq!(current.selection_epoch, selected.selection_epoch);
        assert_eq!(current.page_sequence, cursor.page_sequence);
        let terminal = current.terminal;
        if !terminal {
            cursor.page_sequence = cursor.page_sequence.checked_add(1).unwrap();
            cursor.after_entry_id = current.entries.last().map(|entry| entry.entry_id);
        }
        pages.push(current);
        if terminal {
            return pages;
        }
    }
}
fn assert_resource_limit(run: &Running, path: DirectoryRelativePath) {
    match run
        .worker
        .capture_directory_under(open_directory(&run.data), path, small_limits())
    {
        Err(OwnerCommandError::Busy | OwnerCommandError::Limit) => {}
        Ok((_, mut handle)) => assert!(matches!(
            result(&mut handle),
            Ok(Err(DirectoryCommandError::Admission(AdmissionError::Limit)
                | DirectoryCommandError::Limit))
        )),
        _ => panic!("expected original resource limit/busy refusal"),
    }
}

#[test]
fn original_worker_two_level_unicode_leaf_lists_paginated_metadata_without_read_grant() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.capabilities,
        BTreeSet::from([IoCapability::FileList])
    );
    let leaf = make_chain(&fixture.data, &["一级", "leaf"]);
    let mut expected = BTreeSet::new();
    for n in 0..65 {
        let name = format!("文件-🙂-{n:03}");
        fs::write(leaf.join(&name), [n as u8]).unwrap();
        expected.insert(raw_name(std::ffi::OsStr::new(&name)));
    }
    // Sentinel content is synthetic setup, never read by the listing consumer.
    fs::write(leaf.join("marker"), b"not a FileRead grant").unwrap();
    expected.insert(raw_name(std::ffi::OsStr::new("marker")));
    let mut run = fixture.start();
    let caller = thread::current().id();
    let (session, selected) = capture_under(&run, relative(&["一级", "leaf"]));
    assert_eq!(selected.entries, expected.len());
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 3);
    let pages = collect_pages(&run, session, selected);
    assert!(pages.len() >= 3);
    assert_eq!(
        pages.iter().map(|page| page.entries.len()).sum::<usize>(),
        expected.len()
    );
    let names: BTreeSet<_> = pages
        .iter()
        .flat_map(|page| page.entries.iter())
        .map(|entry| {
            assert_eq!(entry.encoding, NameEncoding::Utf16Le);
            assert_eq!(entry.kind, morrow_fs_directory_v1::EntryKind::File);
            assert!(entry.logical_length.is_some());
            entry.name.clone()
        })
        .collect();
    assert_eq!(names, expected);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    let hooks = run.hooks.lock().unwrap();
    assert!(hooks.iter().any(|(name, _)| *name == "prepare"));
    assert!(hooks.iter().all(|(_, id)| *id != caller));
    drop(hooks);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn retained_intermediate_and_leaf_block_rename_until_finish_then_release_every_lease() {
    let fixture = Fixture::new();
    let leaf = make_chain(&fixture.data, &["parent", "leaf"]);
    fs::write(leaf.join("entry"), [1]).unwrap();
    let mut run = fixture.start();
    let (session, _) = capture_under(&run, relative(&["parent", "leaf"]));
    assert!(fs::rename(&leaf, run.data.join("parent/moved-leaf")).is_err());
    assert!(fs::rename(run.data.join("parent"), run.data.join("moved-parent")).is_err());
    let fee = run.worker.bytes();
    run.finish(session);
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    fs::rename(&leaf, run.data.join("parent/moved-leaf")).unwrap();
    fs::rename(run.data.join("parent"), run.data.join("moved-parent")).unwrap();
    assert!(run.worker.bytes() >= fee); // finish charges; capture fee never refunds
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn loose_share_anchor_rename_replacement_still_uses_held_original_anchor_not_path() {
    let fixture = Fixture::new();
    let leaf = make_chain(&fixture.data, &["leaf"]);
    fs::write(leaf.join("original-only"), [1]).unwrap();
    let anchor = open_directory(&fixture.data); // deliberate share7, not root pinning
    let moved = fixture.temp.path().join("moved-anchor");
    fs::rename(&fixture.data, &moved).unwrap();
    fs::create_dir(&fixture.data).unwrap();
    fs::create_dir(fixture.data.join("leaf")).unwrap();
    fs::write(fixture.data.join("leaf/replacement-only"), [2]).unwrap();
    let mut run = fixture.start();
    let (session, mut handle) = run
        .worker
        .capture_directory_under(anchor, relative(&["leaf"]), small_limits())
        .unwrap();
    let DirectoryResponse::Captured(selected) = result(&mut handle).unwrap().unwrap() else {
        panic!("capture");
    };
    let pages = collect_pages(&run, session, selected);
    let names: Vec<_> = pages
        .iter()
        .flat_map(|page| page.entries.iter())
        .map(|entry| entry.name.clone())
        .collect();
    assert_eq!(names, vec![raw_name(std::ffi::OsStr::new("original-only"))]);
    assert!(run.data.join("leaf/replacement-only").exists());
    assert!(moved.join("leaf/original-only").exists());
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn anchor_plus_seven_components_exactly_uses_eight_resources_and_deeper_chain_fails_closed() {
    let fixture = Fixture::new();
    let parts = ["a", "b", "c", "d", "e", "f", "g"];
    let leaf = make_chain(&fixture.data, &parts);
    fs::create_dir(leaf.join("h")).unwrap();
    let mut run = fixture.start();
    let (session, _) = capture_under(&run, relative(&parts));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 8);
    assert_resource_limit(&run, relative(&["a"]));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 8);
    run.finish(session);
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    let fee = run.worker.bytes();
    assert_resource_limit(&run, relative(&["a", "b", "c", "d", "e", "f", "g", "h"]));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    assert_eq!(run.worker.directory_usage(), (0, 0));
    assert!(run.worker.bytes() >= fee);
    fs::rename(run.data.join("a"), run.data.join("released-a")).unwrap();
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn file_read_only_refuses_file_list_before_child_open_and_releases_admission() {
    let fixture = Fixture::capabilities(&[IoCapability::FileRead]);
    let leaf = make_chain(&fixture.data, &["leaf"]);
    // Native READ-only sharing would conflict with this pre-existing writer;
    // FileList denial must win before any child native open is attempted.
    let writer = OpenOptions::new()
        .access_mode(0x4000_0000)
        .share_mode(7)
        .custom_flags(0x0220_0000)
        .open(&leaf)
        .unwrap();
    let mut run = fixture.start();
    let (_, mut handle) = run
        .worker
        .capture_directory_under(
            open_directory(&run.data),
            relative(&["leaf"]),
            small_limits(),
        )
        .unwrap();
    assert!(matches!(
        result(&mut handle),
        Ok(Err(DirectoryCommandError::Admission(
            AdmissionError::Denied
        )))
    ));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "denied selection retired",
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    drop(writer);
    fs::rename(&leaf, run.data.join("released-leaf")).unwrap();
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn missing_or_ordinary_file_component_returns_io_without_creating_or_modifying_any_child() {
    let fixture = Fixture::new();
    fs::write(
        fixture.data.join("ordinary-file"),
        b"unchanged synthetic bytes",
    )
    .unwrap();
    let mut run = fixture.start();
    for parts in [
        &["missing"][..],
        &["ordinary-file"][..],
        &["missing", "nested"][..],
    ] {
        let (_, mut handle) = run
            .worker
            .capture_directory_under(open_directory(&run.data), relative(parts), small_limits())
            .unwrap();
        assert!(matches!(
            result(&mut handle),
            Ok(Err(DirectoryCommandError::Io(_)))
        ));
        wait_for(
            || run.worker.directory_usage() == (0, 0),
            "failed path retired",
        );
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    }
    assert!(!run.data.join("missing").exists());
    assert_eq!(
        fs::read(run.data.join("ordinary-file")).unwrap(),
        b"unchanged synthetic bytes"
    );
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn actual_isolated_surrogate_directory_component_is_opened_without_lossy_conversion() {
    let fixture = Fixture::new();
    let name = std::ffi::OsString::from_wide(&[97, 0xd800, 98, 0xdc00]);
    let leaf = fixture.data.join(&name);
    // Unsupported OS behavior is a real failure, never skipped into PASS.
    fs::create_dir(&leaf).unwrap();
    fs::write(leaf.join("marker"), [3]).unwrap();
    let mut run = fixture.start();
    let path = DirectoryRelativePath::new(vec![vec![97, 0xd800, 98, 0xdc00]]).unwrap();
    let (session, selected) = capture_under(&run, path);
    let pages = collect_pages(&run, session, selected);
    assert_eq!(pages.len(), 1);
    assert_eq!(
        pages[0].entries[0].name,
        raw_name(std::ffi::OsStr::new("marker"))
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn queued_cancel_never_opens_child_and_cumulative_fees_are_not_refunded_or_replayed() {
    let fixture = Fixture::new();
    let leaf = make_chain(&fixture.data, &["leaf"]);
    let mut run = fixture.start();
    let mut blocker = run.block();
    let (session, mut handle) = run
        .worker
        .capture_directory_under(
            open_directory(&run.data),
            relative(&["leaf"]),
            small_limits(),
        )
        .unwrap();
    assert!(!handle.is_started());
    // Original worker is gated; no retained child handle may block this rename.
    fs::rename(&leaf, run.data.join("moved-leaf")).unwrap();
    fs::rename(run.data.join("moved-leaf"), &leaf).unwrap();
    let fee = run.worker.bytes();
    handle.cancel();
    assert_eq!(
        result(&mut handle).err(),
        Some(OwnerCommandError::Cancelled)
    );
    run.gate.release();
    wait_for(
        || blocker.poll() == OwnerCommandPoll::Ready,
        "blocker released",
    );
    assert_eq!(blocker.read(), Ok(Some(Vec::new())));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "queued selection retired",
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    assert_eq!(run.worker.bytes(), fee);
    assert_closed(run.worker.finish_directory(session));
    fs::rename(&leaf, run.data.join("after-cancel")).unwrap();
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn ready_unread_capture_cancel_is_unknown_no_replay_and_releases_pinned_leaf() {
    let fixture = Fixture::new();
    let leaf = make_chain(&fixture.data, &["leaf"]);
    fs::write(leaf.join("marker"), [4]).unwrap();
    let mut run = fixture.start();
    let (session, mut handle) = run
        .worker
        .capture_directory_under(
            open_directory(&run.data),
            relative(&["leaf"]),
            small_limits(),
        )
        .unwrap();
    ready(&handle);
    assert!(handle.is_started());
    assert!(fs::rename(&leaf, run.data.join("moved-leaf")).is_err());
    let fee = run.worker.bytes();
    handle.cancel();
    assert_eq!(result(&mut handle).err(), Some(OwnerCommandError::Unknown));
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "started cancelled selection retired",
    );
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    assert_eq!(run.worker.bytes(), fee);
    assert_closed(run.worker.finish_directory(session));
    fs::rename(&leaf, run.data.join("moved-leaf")).unwrap();
    fs::rename(run.data.join("moved-leaf"), &leaf).unwrap();
    let (fresh, _) = capture_under(&run, relative(&["leaf"]));
    assert_ne!(session, fresh);
    run.finish(fresh);
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
}

#[test]
fn dropped_unread_page_releases_its_chain_and_other_original_instance_remains_live() {
    let fixture = Fixture::new();
    let leaf = make_chain(&fixture.data, &["leaf"]);
    for n in 0..40 {
        fs::write(leaf.join(format!("entry-{n:02}")), [n as u8]).unwrap();
    }
    let other_fixture = Fixture::new();
    let other_leaf = make_chain(&other_fixture.data, &["leaf"]);
    fs::write(other_leaf.join("other-marker"), [1]).unwrap();
    let mut run = fixture.start();
    let mut other = other_fixture.start();
    let (session, selected) = capture_under(&run, relative(&["leaf"]));
    let (other_session, other_selected) = capture_under(&other, relative(&["leaf"]));
    let handle = run
        .worker
        .next_directory_page(session, selected.first_page())
        .unwrap();
    ready(&handle);
    assert_eq!(
        run.worker.directory_usage().0,
        1,
        "first 32-entry page must remain nonterminal"
    );
    let fee = run.worker.bytes();
    drop(handle);
    wait_for(
        || run.worker.directory_usage() == (0, 0),
        "unread page dropped chain",
    );
    assert_eq!(run.worker.bytes(), fee);
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
    fs::rename(&leaf, run.data.join("released-leaf")).unwrap();
    assert_closed(
        run.worker
            .next_directory_page(session, selected.first_page()),
    );
    let pages = collect_pages(&other, other_session, other_selected);
    assert_eq!(
        pages[0].entries[0].name,
        raw_name(std::ffi::OsStr::new("other-marker"))
    );
    assert_eq!(run.reclaim(true).disconnect, Ok(()));
    assert_eq!(other.reclaim(true).disconnect, Ok(()));
}

#[test]
fn expired_or_backwards_original_clock_before_selection_refuses_without_native_child_open() {
    for tick in [9000, 1] {
        let fixture = Fixture::new();
        let leaf = make_chain(&fixture.data, &["leaf"]);
        let mut run = fixture.start();
        let _blocker = run.block();
        let fee = run.worker.bytes();
        run.clock.store(tick, Ordering::SeqCst);
        assert_closed(run.worker.capture_directory_under(
            open_directory(&run.data),
            relative(&["leaf"]),
            small_limits(),
        ));
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
        assert_eq!(run.worker.bytes(), fee);
        fs::rename(&leaf, run.data.join("unopened-leaf")).unwrap();
        run.gate.release();
        assert_eq!(run.reclaim(false).disconnect, Ok(()));
    }
}

fn synthetic_junction(temp: &Path, link: &Path, target: &Path) {
    // Exact system command, no PATH lookup, inherited environment or symlink
    // privilege. All paths are this test's newly created ordinary TempDir.
    const CMD: &str = r"C:\Windows\System32\cmd.exe";
    assert!(link.starts_with(temp) && target.starts_with(temp));
    assert!(!link.exists());
    assert!(target.is_dir());
    let result = std::process::Command::new(CMD)
        .args(["/D", "/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .current_dir(temp)
        .env_clear()
        .env("SYSTEMROOT", r"C:\Windows")
        .env("WINDIR", r"C:\Windows")
        .env("COMSPEC", CMD)
        .env("PATH", r"C:\Windows\System32")
        .env("TEMP", temp)
        .env("TMP", temp)
        .creation_flags(0x0800_0000)
        .output()
        .expect("trusted system cmd synthetic junction helper could not start");
    assert_eq!(
        result.status.code(),
        Some(0),
        "synthetic unprivileged junction creation refused; no skip/privilege change; stdout={}; stderr={}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn actual_intermediate_and_leaf_junctions_refuse_without_selection_leak_or_restart_poison() {
    for intermediate in [true, false] {
        let fixture = Fixture::new();
        let target = fixture.temp.path().join("synthetic-junction-target");
        fs::create_dir(&target).unwrap();
        let bait_leaf = if intermediate {
            let leaf = target.join("leaf");
            fs::create_dir(&leaf).unwrap();
            leaf
        } else {
            target.clone()
        };
        fs::write(
            bait_leaf.join("bait-never-delivered"),
            b"synthetic target bytes never read by selection",
        )
        .unwrap();
        let link = fixture.data.join("junction-component");
        synthetic_junction(fixture.temp.path(), &link, &target);
        let ordinary = make_chain(&fixture.data, &["ordinary-leaf"]);
        fs::write(ordinary.join("ordinary-ok"), [7]).unwrap();
        let mut run = fixture.start();
        let parts: &[&str] = if intermediate {
            &["junction-component", "leaf"]
        } else {
            &["junction-component"]
        };
        let (_, mut handle) = run
            .worker
            .capture_directory_under(open_directory(&run.data), relative(parts), small_limits())
            .unwrap();
        let admitted_fee = run.worker.bytes();
        assert!(admitted_fee > 0);
        let refusal = result(&mut handle);
        assert!(matches!(
            refusal,
            Ok(Err(
                DirectoryCommandError::Io(_) | DirectoryCommandError::ReparsePoint
            ))
        ));
        // No selected reference/page/target name or content may cross delivery.
        let diagnostic = format!("{refusal:?}");
        assert!(!diagnostic.contains("bait-never-delivered"));
        assert!(!diagnostic.contains("synthetic target bytes"));
        wait_for(
            || run.worker.directory_usage() == (0, 0),
            "junction refusal retired without publication",
        );
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
        assert_eq!(run.worker.directory_binding_usage().unwrap().jobs, 0);
        let charged_binding_bytes = run.worker.directory_binding_usage().unwrap().bytes;
        assert!(
            charged_binding_bytes > 0,
            "native selection attempt was charged before OS work"
        );
        assert_eq!(run.worker.bytes(), admitted_fee);
        // This failure is a read-only refusal, not a poisoned mutation effect.
        // An ordinary unlinked leaf remains usable on this same original worker.
        let (session, selected) = capture_under(&run, relative(&["ordinary-leaf"]));
        let pages = collect_pages(&run, session, selected);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].entries.len(), 1);
        assert_eq!(
            pages[0].entries[0].name,
            raw_name(std::ffi::OsStr::new("ordinary-ok"))
        );
        assert_eq!(run.worker.directory_binding_usage().unwrap().resources, 0);
        assert!(run.worker.directory_binding_usage().unwrap().bytes >= charged_binding_bytes);
        assert!(run.worker.bytes() >= admitted_fee);
        assert!(bait_leaf.join("bait-never-delivered").exists());
        assert_eq!(run.reclaim(true).disconnect, Ok(()));
    }
}
