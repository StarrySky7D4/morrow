//! New directory guest negative tests on original-worker ordinary synthetic fixtures.
//! V-only draft, NOT_RUN. Root must qualify final exact source/API separately.
//! No protected Session/DPAPI/GUI/real data or above-anchor picker proof.
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
use morrow_plugin_runtime::{
    Limits,
    directory_io::{CaptureLimits, SelectedDirectory},
    io_binding::{Error as AdmissionError, IoBinding},
    io_jobs::{
        CommandOwner, DirectoryCommandError, DirectoryCommandHandle, DirectoryGuestHandle, DirectoryGuestResult, DirectoryResponse,
        DirectorySession, HostOwner, IoWorker, JobError, JobLimits, ManagedHostOwner,
        OwnerCommandError, OwnerCommandHandle, OwnerCommandPoll, WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.directory-request-negative";
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
    fn guest(body: &str) -> Self {
        let capabilities = &[IoCapability::FileList];
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("selected-synthetic");
        fs::create_dir(&data).unwrap();
        let wasm = wat::parse_str(format!(r#"(module
            (import "morrow_task_v1" "read_input" (func $input (param i32 i32) (result i32)))
            (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
            (import "morrow_fs_directory_v1" "call" (func $directory (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) (local $input_length i32) (local $reply_length i32) (local $p i32) (local $root i32) (local $data i32)
              i32.const 0 i32.const 131072 call $input local.set $input_length
              {body}
              i32.const 0))"#)).unwrap();
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
        manifest.required_features.push(morrow_core::plugin_package::DIRECTORY_REQUEST_FEATURE.into());
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
const OPEN: &str = "i32.const 0 local.get $input_length i32.const 1024 i32.const 65536 call $directory local.set $reply_length";
fn normal_guest() -> String {
    format!("{OPEN} i32.const 1024 local.get $reply_length call $complete drop")
}
fn capture(run: &Running) -> (DirectorySession, SelectedDirectory) {
    let (session, mut reply) = run.worker.capture_directory_fresh(open_directory(&run.data), small_limits()).unwrap();
    let selected = match result(&mut reply).unwrap().unwrap() {
        DirectoryResponse::Captured(selected) => selected,
        _ => panic!("expected captured response kind"),
    };
    (session, selected)
}
fn guest_result(handle: &mut DirectoryGuestHandle)
    -> Result<Result<DirectoryGuestResult, DirectoryCommandError>, OwnerCommandError> {
    let until = Instant::now() + WAIT;
    loop {
        match handle.read() {
            Ok(None) => {
                assert!(Instant::now() < until, "bounded new-profile guest completion wait");
                thread::sleep(Duration::from_millis(1));
            }
            Ok(Some(value)) => return Ok(value),
            Err(error) => return Err(error),
        }
    }
}
fn guest_ready(handle: &DirectoryGuestHandle) {
    wait_for(|| handle.poll() == OwnerCommandPoll::Ready, "new-profile reply Ready");
}
fn retired(run: &Running) {
    wait_for(|| run.worker.directory_usage() == (0, 0)
        && run.worker.directory_binding_usage().unwrap().resources == 0,
        "original maintenance releases exact selection and original resource lease");
}

#[test]
fn guest_actual_open_wire_fees_charge_original_binding_and_unread_job_lifetime() {
    use morrow_fs_directory_request_v1::{Action, Opened, Reply, Request, Response};
    let mut run = Fixture::guest(&normal_guest()).start();
    let (session, selected) = capture(&run);
    let before = run.worker.directory_binding_usage().unwrap();
    assert_eq!(before.resources, 1);
    assert_eq!(before.jobs, 0);
    let shape = Request::new([1; 32], [2; 32], Action::Open).unwrap();
    let response_shape = Response::new(&shape, Reply::Opened(Opened {
        selection_epoch: selected.selection_epoch, page_sequence: 1, after_entry_id: None,
        entries: selected.entries as u32, metadata_bytes: selected.metadata_bytes,
    })).unwrap();
    let mut handle = run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)).unwrap();
    guest_ready(&handle);
    let unread = run.worker.directory_binding_usage().unwrap();
    assert_eq!(unread.jobs, 1);
    assert_eq!(unread.resources, before.resources);
    // Host task input, guest Open request, and its actual reply all charge the
    // original IoBinding. This profile's Open performs no new native query.
    assert_eq!(unread.bytes - before.bytes,
        2 * shape.wire().len() as u64 + response_shape.wire().len() as u64);
    let completed = guest_result(&mut handle).unwrap().unwrap();
    assert_eq!(completed.execution.outcome, Ok(0));
    assert!(!completed.unknown);
    assert_eq!(completed.response.as_ref().unwrap().len(), response_shape.wire().len());
    assert_eq!(run.worker.directory_binding_usage().unwrap().jobs, 0);
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, unread.bytes);
    {
        let hooks = run.hooks.lock().unwrap();
        let caller = thread::current().id();
        assert!(hooks.iter().any(|(name, tid)| *name == "prepare" && *tid != caller));
        assert!(hooks.iter().all(|(_, tid)| *tid != caller));
    }
    run.finish(session);
    run.reclaim(true);
}

#[test]
fn undeclared_handler_rejection_preserves_this_selection_for_valid_call() {
    let mut run = Fixture::guest(&normal_guest()).start();
    let (session, _) = capture(&run);
    let before = run.worker.directory_binding_usage().unwrap();
    let mut invalid = run.worker.submit_directory_guest_frame(session, "undeclared.handler".into(), Duration::from_secs(2)).unwrap();
    assert!(matches!(guest_result(&mut invalid),
        Ok(Err(DirectoryCommandError::Admission(AdmissionError::Denied)))));
    assert_eq!(run.worker.directory_usage().0, 1);
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, before.resources);
    let mut valid = run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)).unwrap();
    assert_eq!(guest_result(&mut valid).unwrap().unwrap().execution.outcome, Ok(0));
    run.finish(session);
    run.reclaim(true);
}

#[test]
fn foreign_worker_and_oversized_handler_allocation_cannot_retire_other_selection() {
    let mut first = Fixture::guest(&normal_guest()).start();
    let mut second = Fixture::guest(&normal_guest()).start();
    let (session, _) = capture(&first);
    assert!(matches!(second.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)), Err(OwnerCommandError::Closed)));
    let mut allocated = String::with_capacity(4096);
    allocated.push_str(HANDLER);
    assert!(matches!(first.worker.submit_directory_guest_frame(session, allocated, Duration::from_secs(2)), Err(OwnerCommandError::Closed)));
    assert_eq!(first.worker.directory_usage().0, 1);
    first.finish(session);
    first.reclaim(true);
    second.reclaim(true);
}

#[test]
fn queued_cancel_retains_fees_and_retires_only_after_original_worker_maintenance() {
    let mut run = Fixture::guest(&normal_guest()).start();
    let (session, _) = capture(&run);
    let blocker = run.block();
    let before = run.worker.directory_binding_usage().unwrap();
    let mut handle = run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)).unwrap();
    assert!(!handle.is_started());
    handle.cancel();
    assert!(matches!(handle.read(), Err(OwnerCommandError::Cancelled)));
    assert_eq!(run.worker.directory_binding_usage().unwrap().resources, before.resources);
    run.gate.release();
    drop(blocker);
    retired(&run);
    assert!(run.worker.directory_binding_usage().unwrap().bytes >= before.bytes);
    assert!(matches!(run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)), Err(OwnerCommandError::Closed)));
    run.reclaim(true);
}

#[test]
fn cancelled_ready_reply_is_unknown_and_cannot_be_replayed_as_new_invocation() {
    let mut run = Fixture::guest(&normal_guest()).start();
    let (session, _) = capture(&run);
    let mut handle = run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)).unwrap();
    guest_ready(&handle);
    let charged = run.worker.directory_binding_usage().unwrap().bytes;
    handle.cancel();
    assert!(matches!(handle.read(), Err(OwnerCommandError::Unknown)));
    retired(&run);
    assert_eq!(run.worker.directory_binding_usage().unwrap().bytes, charged);
    assert!(matches!(run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)), Err(OwnerCommandError::Closed)));
    run.reclaim(true);
}

#[test]
fn repeated_original_open_nonce_fails_closed_and_never_leaves_resumable_selection() {
    let duplicate = format!("{OPEN} {OPEN} i32.const 1024 local.get $reply_length call $complete drop");
    let mut run = Fixture::guest(&duplicate).start();
    let (session, _) = capture(&run);
    let before = run.worker.directory_binding_usage().unwrap().bytes;
    let mut handle = run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)).unwrap();
    let failed = guest_result(&mut handle).unwrap().unwrap();
    assert_eq!(failed.execution.outcome, Err(morrow_plugin_runtime::Fault::TaskProtocol));
    assert!(failed.response.is_none());
    // No native page was generated; caller uncertainty and lifecycle still
    // force retirement, rather than granting another invocation replay.
    assert!(!failed.unknown);
    retired(&run);
    assert!(run.worker.directory_binding_usage().unwrap().bytes > before);
    assert!(matches!(run.worker.submit_directory_guest_frame(session, HANDLER.into(), Duration::from_secs(2)), Err(OwnerCommandError::Closed)));
    run.reclaim(true);
}


/// Both host Open input and Opened reply are small default-allocator messages;
/// this fixture only handles those single-segment canonical forms. Subsequent
/// real Page decoding remains entirely in the production codecs/owner.
fn copy_small_data_pointer(base: usize, ordinal: usize, destination: usize) -> String {
    format!(" i32.const {} i32.load local.set $root
        i32.const {} local.get $root i32.const 2 i32.shr_s i32.const 3 i32.shl i32.add
        i32.const {} i32.load i32.const 65535 i32.and i32.const 3 i32.shl i32.add
        i32.const {} i32.add local.tee $p
        i32.const 8 i32.add local.get $p i32.load i32.const 2 i32.shr_s i32.const 3 i32.shl i32.add local.set $data
        i32.const {} local.get $data i32.const 32 memory.copy",
        base+8, base+16, base+12, ordinal*8, destination)
}
fn first_page_guest(bad_complete: bool) -> String {
    use morrow_fs_directory_request_v1::{Action, Request};
    let wire = Request::new([0x11;32], [0x22;32], Action::Next {
        selection_epoch:[0x33;32], page_sequence:1, after_entry_id:None,
    }).unwrap();
    let bytes = wire.wire();
    // Do not infer a native frame layout by guessed absolute payload offsets.
    // Find the controlled constructor data, then copy via actual Capnp pointers.
    let locate = |marker:u8| {
        let matches:Vec<_> = bytes.windows(32).enumerate()
            .filter_map(|(offset, value)| (value == [marker;32]).then_some(offset)).collect();
        assert_eq!(matches.len(),1, "unique synthetic marker");
        matches[0]
    };
    let mut body = OPEN.to_owned();
    for (offset, byte) in bytes.iter().enumerate() {
        body.push_str(&format!(" i32.const {} i32.const {} i32.store8",4096+offset,byte));
    }
    body.push_str(&copy_small_data_pointer(0,1,4096+locate(0x11)));
    body.push_str(&copy_small_data_pointer(1024,5,4096+locate(0x33)));
    // Deterministically distinct nonce: change the first synthetic byte only
    // when the actual initial nonce begins with that same byte.
    body.push_str(&copy_small_data_pointer(0,2,6000));
    body.push_str(&format!(" i32.const 6000 i32.load8_u i32.const 34 i32.eq if i32.const {} i32.const 35 i32.store8 end",4096+locate(0x22)));
    body.push_str(&format!(" i32.const 4096 i32.const {} i32.const 16384 i32.const 65536 call $directory local.set $reply_length", bytes.len()));
    if bad_complete {
        // A Page has actually been delivered through the import, but final
        // task completion deliberately loses the exact final response.
        body.push_str(" i32.const 16384 i32.const 1 call $complete drop");
    } else {
        body.push_str(" i32.const 16384 local.get $reply_length call $complete drop");
    }
    body
}

#[test]
fn actual_terminal_next_uses_direct_worker_and_eof_still_requires_owner_join() {
    let mut run = Fixture::guest(&first_page_guest(false)).start();
    let (session, _) = capture(&run);
    let mut handle = run.worker.submit_directory_guest_frame(session,HANDLER.into(),Duration::from_secs(2)).unwrap();
    guest_ready(&handle); // bounded completion would expose public self-queue deadlock
    let ready = run.worker.directory_binding_usage().unwrap();
    assert_eq!(ready.resources,0); // empty synthetic directory's terminal page released native root
    assert_eq!(ready.jobs,1); // unread invocation still owns its original job lease
    let completed = guest_result(&mut handle).unwrap().unwrap();
    assert_eq!(completed.execution.outcome,Ok(0));
    assert!(!completed.unknown);
    assert!(completed.response.is_some());
    assert!(matches!(run.worker.next_directory_page(session, morrow_plugin_runtime::directory_io::PageRequest {
        reference:[1;32], selection_epoch:[2;32], page_sequence:1, after_entry_id:None,
    }),Err(OwnerCommandError::Closed)));
    assert_eq!(run.worker.directory_binding_usage().unwrap().jobs,0);
    // EOF proves no thread completion by itself. Actual original worker exit
    // and returned owner identity are separately observed here.
    run.reclaim(true);
}

#[test]
fn page_generated_then_wrong_final_completion_is_unknown_without_cursor_replay() {
    let mut run = Fixture::guest(&first_page_guest(true)).start();
    let (session, _) = capture(&run);
    let before = run.worker.directory_binding_usage().unwrap().bytes;
    let mut handle = run.worker.submit_directory_guest_frame(session,HANDLER.into(),Duration::from_secs(2)).unwrap();
    let unknown = guest_result(&mut handle).unwrap().unwrap();
    assert_eq!(unknown.execution.outcome,Err(morrow_plugin_runtime::Fault::TaskProtocol));
    assert!(unknown.unknown);
    assert!(unknown.response.is_none());
    retired(&run);
    assert!(run.worker.directory_binding_usage().unwrap().bytes>before);
    assert!(matches!(run.worker.submit_directory_guest_frame(session,HANDLER.into(),Duration::from_secs(2)),Err(OwnerCommandError::Closed)));
    run.reclaim(true);
}

#[test]
fn unread_nonterminal_page_drop_retires_this_selection_and_preserves_other_worker() {
    let mut first = Fixture::guest(&first_page_guest(false)).start();
    for number in 0..33 { fs::write(first.data.join(format!("entry-{number:02}")),b"synthetic").unwrap(); }
    let mut other = Fixture::guest(&normal_guest()).start();
    let (session, selected) = capture(&first);
    assert_eq!(selected.entries,33);
    let (other_session, _) = capture(&other);
    let handle = first.worker.submit_directory_guest_frame(session,HANDLER.into(),Duration::from_secs(2)).unwrap();
    guest_ready(&handle);
    assert_eq!(first.worker.directory_binding_usage().unwrap().resources,1);
    assert_eq!(first.worker.directory_binding_usage().unwrap().jobs,1);
    let charged = first.worker.directory_binding_usage().unwrap().bytes;
    drop(handle);
    retired(&first);
    assert_eq!(first.worker.directory_binding_usage().unwrap().bytes,charged);
    assert_eq!(other.worker.directory_binding_usage().unwrap().resources,1);
    assert!(matches!(first.worker.submit_directory_guest_frame(session,HANDLER.into(),Duration::from_secs(2)),Err(OwnerCommandError::Closed)));
    other.finish(other_session);
    first.reclaim(true);
    other.reclaim(true);
}
