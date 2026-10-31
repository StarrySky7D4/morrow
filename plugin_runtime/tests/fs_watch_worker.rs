//! Original managed worker and real synthetic local filesystem events only.
//! Negative delivery cases retain their whole original owner until this isolated
//! test process ends; every physically opened native handle is checked closed.
#![cfg(all(feature = "packages", windows))]
#![deny(warnings)]
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_fs_watch_v1::{Batch, End, Kind, wire_digest};
use morrow_plugin_runtime::{
    Fault, Limits,
    directory_io::DirectoryAbsolutePath,
    fs_watch_binding::{ApprovalRequest, WatchApproval, WatchRevocation},
    fs_watch_worker::{Error, Exit, Phase, WatchWorker},
    io_binding::IoBinding,
    io_jobs::{HostOwner, JobError, ManagedHostOwner},
    manager::Manager,
};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, ThreadId},
    time::{Duration, Instant},
};
const ID: &str = "org.example.watch.worker";
const ECHO: &str = r#"(module
 (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
 (import "morrow_task_v1" "complete" (func $complete (param i32 i32) (result i32)))
 (memory (export "memory") 2 256)
 (func (export "morrow_run") (result i32) (local $n i32) (local $k i32)
  i32.const 0 i32.const 131072 call $read local.set $n
  ;; work
  i32.const 0 local.get $n call $complete))"#;
type Log = Arc<Mutex<Vec<(&'static str, ThreadId)>>>;
struct Owner {
    host: HostRuntime,
    manager: Manager,
    log: Log,
    fail_prepare: bool,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.log
            .lock()
            .unwrap()
            .push(("prepare", thread::current().id()));
        if self.fail_prepare {
            Err(JobError::Unavailable)
        } else {
            Ok(())
        }
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.log
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
}
struct Fixture {
    owner: Owner,
    package: Package,
    observed: PathBuf,
    log: Log,
}
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::FileList])
}
impl Fixture {
    fn new(wasm: Vec<u8>, fuel: u64, calls: u32) -> Self {
        let root = tempfile::tempdir().unwrap().keep();
        assert!(root.starts_with(r"D:\build_cache"));
        use std::os::windows::fs::MetadataExt;
        for p in root
            .ancestors()
            .take_while(|p| *p != std::path::Path::new(r"D:\build_cache"))
        {
            assert_eq!(
                fs::symlink_metadata(p).unwrap().file_attributes() & 0x400,
                0
            );
            assert!(!p.join(".git").exists());
        }
        let mut manifest =
            Package::manifest_for_fs_watch(ID, "1.0.0", &wasm, vec!["observe".into()]);
        let budget = manifest.budget.as_mut().unwrap();
        budget.fuel = fuel;
        budget.host_calls = calls;
        let declaration = manifest.fs_watch_declaration.as_mut().unwrap();
        declaration.max_frames = 4;
        declaration.max_wire_bytes = 512 * 1024;
        declaration.max_window_ms = 25_000;
        let package = Package::build(manifest, &wasm).unwrap();
        let catalog = Catalog::open(&root.join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let mut manager = Manager::new(
            Registry::open(&root.join("registry"), catalog).unwrap(),
            Limits::default(),
        );
        manager.select(&package, 0).unwrap();
        manager
            .approve_io(ID, package.digest(), caps(), manager.revision())
            .unwrap();
        manager
            .approve_fs_watch(ID, package.digest(), true, manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let host =
            HostRuntime::new(Store::open(&root.join("store"), EventBudget::default()).unwrap())
                .unwrap();
        let observed = root.join("observed");
        fs::create_dir(&observed).unwrap();
        let log = Log::default();
        Self {
            owner: Owner {
                host,
                manager,
                log: log.clone(),
                fail_prepare: false,
            },
            package,
            observed,
            log,
        }
    }
    fn authority(
        &mut self,
        frames: u32,
        window: u64,
    ) -> (
        morrow_plugin_runtime::manager::ManagedInstance,
        IoBinding,
        WatchApproval,
    ) {
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
                self.package.digest(),
                self.owner.manager.revision(),
                &caps(),
                29_000,
                1,
            )
            .unwrap();
        let approval = WatchApproval::issue(
            &self.owner.manager,
            &self.owner.host,
            &instance,
            &binding,
            ApprovalRequest {
                expected_revision: self.owner.manager.revision(),
                handler: "observe".into(),
                nominated_path: DirectoryAbsolutePath::new(&self.observed).unwrap(),
                max_frames: frames,
                max_wire_bytes: 512 * 1024,
                window_ms: window,
            },
            1,
        )
        .unwrap();
        (instance, binding, approval)
    }
    fn spawn(
        mut self,
        frames: u32,
        window: u64,
    ) -> (
        WatchWorker<Owner>,
        IoBinding,
        WatchRevocation,
        PathBuf,
        Log,
        Arc<AtomicU64>,
    ) {
        let (instance, binding, approval) = self.authority(frames, window);
        let revocation = approval.revocation();
        let tick = Arc::new(AtomicU64::new(1));
        let clock = tick.clone();
        let worker = WatchWorker::spawn_owned(self.owner, instance, approval, move || {
            clock.load(Ordering::Acquire)
        })
        .unwrap();
        (worker, binding, revocation, self.observed, self.log, tick)
    }
}
fn record(label: &str, suffix: &str, bytes: &[u8]) {
    let root = PathBuf::from(
        std::env::var_os("MORROW_WATCH_WORKER_EVIDENCE").expect("qualification evidence directory"),
    );
    assert!(root.starts_with(r"D:\build_cache"));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(format!("{label}-{suffix}")))
        .unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}
fn ready(worker: &WatchWorker<Owner>, read: u32) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let s = worker.snapshot();
        if s.phase == Phase::Observing
            && s.native.is_some_and(|n| {
                n.native
                    .is_some_and(|n| n.read_in_flight && n.read_calls >= read)
            })
        {
            return;
        }
        assert!(
            s.phase != Phase::Finished && Instant::now() < until,
            "not ready: {s:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn drive(
    label: &str,
    mut worker: WatchWorker<Owner>,
    action: impl FnOnce(&WatchWorker<Owner>),
) -> Exit<Owner> {
    let action = catch_unwind(AssertUnwindSafe(|| action(&worker)));
    if action.is_err() {
        worker.cancel();
    }
    let until = Instant::now() + Duration::from_secs(35);
    let exit = loop {
        if let Some(exit) = worker.try_join().unwrap() {
            break exit;
        }
        assert!(Instant::now() < until, "original join unresolved");
        thread::sleep(Duration::from_millis(2));
    };
    assert!(matches!(worker.try_join(), Err(Error::Consumed)));
    record(label,"exit.txt",format!("original_join_returned=true\nresult={:?}\nunknown={}\nowner_returned={}\nnative_complete={}\ndisconnect={:?}\nmaintenance={:?}\nsnapshot={:?}\nusage={:?}\nlast_execution={:?}\n",exit.result,exit.unknown,exit.owner.is_some(),exit.native_retirement.is_complete(),exit.disconnect,exit.maintenance,exit.snapshot,exit.usage,exit.last_execution).as_bytes());
    if let Err(error) = action {
        resume_unwind(error);
    }
    exit
}
fn closed(exit: &Exit<Owner>, binding: &IoBinding, log: &Log) {
    assert!(!exit.unknown && exit.owner.is_some() && exit.instance.is_none());
    assert!(exit.native_retirement.is_complete());
    assert_eq!(exit.disconnect, Some(Ok(())));
    assert_eq!(exit.maintenance, Some(Ok(())));
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    let entries = log.lock().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].0, "prepare");
    assert_eq!(entries[1].0, "finish");
    assert_eq!(entries[0].1, entries[1].1);
    assert_ne!(entries[0].1, thread::current().id());
    if let Some(proof) = exit.native_retirement.closed() {
        assert!(proof.ancestor_snapshot().all_closed());
        let n = proof.native_snapshot().unwrap();
        assert_eq!(n.owned_handles, n.close_calls);
        assert_eq!(n.close_calls, n.confirmed_closed);
        assert_eq!(n.read_calls, n.completions);
        assert!(n.retired && !n.unknown && !n.read_in_flight);
    }
}
fn echo(label: &str, wasm: Vec<u8>) {
    let (worker, binding, _, path, log, _) = Fixture::new(wasm, 20_000_000, 16).spawn(2, 20_000);
    let exit = drive(label, worker, |worker| {
        ready(worker, 1);
        fs::create_dir(path.join("first 空白😀")).unwrap();
        ready(worker, 2);
        fs::create_dir(path.join("second")).unwrap();
    });
    closed(&exit, &binding, &log);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.deliveries.len(), 2);
    let mut fuel = 20_000_000;
    let mut epoch = None;
    for (index, delivery) in exit.deliveries.iter().enumerate() {
        assert_eq!(delivery.sequence, index as u64 + 1);
        assert_eq!(delivery.digest, wire_digest(delivery.bytes()));
        let batch = Batch::decode(delivery.bytes()).unwrap();
        assert_eq!(batch.sequence, delivery.sequence);
        assert!(
            batch
                .changes
                .iter()
                .any(|change| change.kind == Kind::Added)
        );
        assert_eq!(*epoch.get_or_insert(batch.watch_epoch), batch.watch_epoch);
        assert!(delivery.execution.fuel_remaining < fuel);
        fuel = delivery.execution.fuel_remaining;
        assert_eq!(delivery.execution.host_calls, 2);
        record(
            label,
            &format!("frame{}.capnp", index + 1),
            delivery.bytes(),
        );
        record(
            label,
            &format!("frame{}.txt", index + 1),
            format!(
                "sequence={}\nexecution={:?}\n",
                delivery.sequence, delivery.execution
            )
            .as_bytes(),
        );
    }
    assert_eq!(exit.snapshot.fuel_remaining, fuel);
    assert_eq!(exit.snapshot.task_calls_remaining, 12);
}
#[test]
fn raw_echo_has_cumulative_budget_and_original_join() {
    echo("echo", wat::parse_str(ECHO).unwrap());
}
fn sdk(language: &str) -> Vec<u8> {
    let dir =
        PathBuf::from(std::env::var_os("MORROW_WATCH_SDK_MODULES").expect("pinned SDK modules"));
    assert!(dir.starts_with(r"D:\build_cache"));
    fs::read(dir.join(format!("watch-{language}.wasm"))).unwrap()
}
#[test]
fn rust_sdk_receives_actual_original_frames() {
    echo("rust", sdk("rust"));
}
#[test]
fn c_sdk_receives_actual_original_frames() {
    echo("c", sdk("c"));
}
#[test]
fn cpp_sdk_receives_actual_original_frames() {
    echo("cpp", sdk("cpp"));
}
#[test]
fn cancellation_closes_pending_read_on_original_worker() {
    let (worker, binding, _, _, log, _) =
        Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16).spawn(2, 20_000);
    let exit = drive("cancel", worker, |worker| {
        ready(worker, 1);
        worker.cancel();
    });
    closed(&exit, &binding, &log);
    assert_eq!(exit.result, Err(Error::Cancelled));
    assert!(exit.last_execution.is_none());
}
#[test]
fn live_revocation_closes_without_dispatching_a_guest() {
    let (worker, binding, revoke, _, log, _) =
        Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16).spawn(2, 20_000);
    let exit = drive("revoke", worker, |worker| {
        ready(worker, 1);
        revoke.revoke();
    });
    closed(&exit, &binding, &log);
    assert!(exit.result.is_err());
    assert!(exit.last_execution.is_none());
}
#[test]
fn real_deadline_has_no_budget_or_authority_refresh() {
    let (worker, binding, _, _, log, _) =
        Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16).spawn(2, 800);
    let exit = drive("expiry", worker, |worker| ready(worker, 1));
    closed(&exit, &binding, &log);
    assert!(exit.result.is_err());
    assert!(exit.last_execution.is_none());
}
#[test]
fn original_clock_rollback_closes_without_guest_dispatch() {
    let (worker, binding, _, _, log, tick) =
        Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16).spawn(2, 20_000);
    let exit = drive("clock", worker, |worker| {
        ready(worker, 1);
        tick.store(0, Ordering::Release);
    });
    closed(&exit, &binding, &log);
    assert!(exit.result.is_err());
    assert!(exit.last_execution.is_none());
}
#[test]
fn failed_prepare_returns_original_owner_without_native_capture() {
    let mut f = Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16);
    f.owner.fail_prepare = true;
    let (worker, binding, _, _, log, _) = f.spawn(2, 20_000);
    let exit = drive("prepare", worker, |_| {});
    closed(&exit, &binding, &log);
    assert_eq!(exit.result, Err(Error::Owner));
    assert!(exit.snapshot.native.is_none());
}
fn unknown(label: &str, wasm: Vec<u8>, fuel: u64, calls: u32, frames: u32) -> Exit<Owner> {
    let (worker, binding, _, path, log, _) = Fixture::new(wasm, fuel, calls).spawn(frames, 20_000);
    let exit = drive(label, worker, |worker| {
        ready(worker, 1);
        fs::create_dir(path.join("first")).unwrap();
        if frames == 2 {
            ready(worker, 2);
            fs::create_dir(path.join("second")).unwrap();
        }
    });
    assert!(exit.unknown && exit.owner.is_none() && exit.instance.is_none());
    assert!(!exit.native_retirement.is_complete());
    assert!(exit.disconnect.is_none() && exit.maintenance.is_none());
    assert!(exit.deliveries.is_empty());
    assert_eq!(log.lock().unwrap().len(), 1);
    let n = exit.snapshot.native.unwrap();
    assert!(n.unknown && n.job_held && n.held_resources > 0);
    assert!(n.ancestors.all_closed());
    let native = n.native.unwrap();
    assert_eq!(native.owned_handles, native.confirmed_closed);
    assert_eq!(native.close_calls, native.confirmed_closed);
    assert!(!native.read_in_flight);
    assert!(binding.usage().resources > 0 && binding.usage().jobs > 0);
    exit
}
#[test]
fn mismatched_completion_retains_original_owner_without_retry() {
    let wasm =
        wat::parse_str(ECHO.replace(";; work", "i32.const 0 i32.const 255 i32.store8")).unwrap();
    let exit = unknown("bad-echo", wasm, 20_000_000, 16, 1);
    assert_eq!(exit.result, Err(Error::Protocol));
    assert_eq!(exit.snapshot.delivered, 0);
}
#[test]
fn task_call_budget_is_shared_across_actual_frames() {
    let exit = unknown("calls", wat::parse_str(ECHO).unwrap(), 20_000_000, 3, 2);
    assert_eq!(exit.result, Err(Error::Guest(Fault::Limits)));
    assert_eq!(exit.snapshot.delivered, 1);
    assert_eq!(exit.snapshot.task_calls_remaining, 0);
    assert_eq!(exit.last_execution.unwrap().host_calls, 1);
}
#[test]
fn fuel_budget_is_shared_across_actual_frames() {
    let wasm=wat::parse_str(ECHO.replace(";; work","i32.const 1000 local.set $k (loop $burn local.get $k i32.const 1 i32.sub local.tee $k br_if $burn)")).unwrap();
    let exit = unknown("fuel", wasm, 4000, 16, 2);
    assert_eq!(exit.result, Err(Error::Guest(Fault::Limits)));
    assert_eq!(exit.snapshot.delivered, 1);
    assert!(exit.snapshot.fuel_remaining < 4_000);
    assert_eq!(exit.last_execution.as_ref().unwrap().host_calls, 1);
}
#[test]
fn foreign_owner_is_refused_before_clock_or_worker() {
    let mut original = Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16);
    let (instance, binding, approval) = original.authority(2, 20_000);
    let other = Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16);
    let log = other.log.clone();
    let samples = Arc::new(AtomicU64::new(0));
    let count = samples.clone();
    let failure = match WatchWorker::spawn_owned(other.owner, instance, approval, move || {
        count.fetch_add(1, Ordering::AcqRel);
        1
    }) {
        Ok(_) => panic!("foreign admission"),
        Err(failure) => failure,
    };
    assert_eq!(samples.load(Ordering::Acquire), 0);
    assert!(log.lock().unwrap().is_empty());
    assert_eq!(binding.usage().resources, 0);
    assert_eq!(binding.usage().jobs, 0);
    failure.instance.close(&mut original.owner.host).unwrap();
    record(
        "foreign",
        "refusal.txt",
        format!(
            "error={:?}\nclock_samples=0\nworker_started=false\noriginal_disconnect=true\n",
            failure.error
        )
        .as_bytes(),
    );
}

#[test]
fn actual_gap_is_delivered_once_and_never_rearmed() {
    let mut f = Fixture::new(wat::parse_str(ECHO).unwrap(), 20_000_000, 16);
    let (instance, binding, approval) = f.authority(4, 20_000);
    let block = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let clock_flags = (block.clone(), entered.clone(), release.clone());
    let worker = WatchWorker::spawn_owned(f.owner, instance, approval, move || {
        if clock_flags.0.swap(false, Ordering::AcqRel) {
            clock_flags.1.store(true, Ordering::Release);
            // Bounded fixture coordination only. This pauses the original
            // clock before polling, while actual Windows notifications arrive.
            let until = Instant::now() + Duration::from_secs(5);
            while !clock_flags.2.load(Ordering::Acquire) && Instant::now() < until {
                thread::sleep(Duration::from_millis(1));
            }
        }
        1
    })
    .unwrap();
    struct Release(Arc<AtomicBool>);
    impl Drop for Release {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let exit = drive("gap", worker, |worker| {
        ready(worker, 1);
        block.store(true, Ordering::Release);
        let until = Instant::now() + Duration::from_secs(2);
        while !entered.load(Ordering::Acquire) {
            assert!(
                Instant::now() < until,
                "original clock did not enter bounded fixture step"
            );
            thread::sleep(Duration::from_millis(1));
        }
        let release = Release(release);
        for index in 0..4096 {
            fs::create_dir(f.observed.join(format!(
                "gap-{index:04}-abcdefghijklmnopqrstuvwxyz0123456789"
            )))
            .unwrap();
        }
        drop(release);
    });
    closed(&exit, &binding, &f.log);
    assert_eq!(exit.result, Err(Error::ObservationEnded(End::Gap)));
    let native = exit.snapshot.native.unwrap().native.unwrap();
    assert_eq!(native.read_calls, 2);
    assert_eq!(native.completions, 2);
    assert_eq!(exit.deliveries.len(), 2);
    for (index, delivery) in exit.deliveries.iter().enumerate() {
        let batch = Batch::decode(delivery.bytes()).unwrap();
        assert_eq!(batch.sequence, index as u64 + 1);
        assert_eq!(batch.end, if index == 1 { End::Gap } else { End::Continue });
        if index == 1 {
            assert!(batch.changes.is_empty());
        }
        record(
            "gap",
            &format!("frame{}.capnp", index + 1),
            delivery.bytes(),
        );
    }
    assert_eq!(exit.snapshot.task_calls_remaining, 12);
}
