//! Original-owner opt-in is required before a queued mutation may select a file.
#![cfg(all(windows, feature = "packages", not(target_arch = "wasm32")))]
use morrow_core::{
    dispatch::HostRuntime,
    file_mutation::Disposition,
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
    file_target::{Error as TargetError, SelectionScope},
    io_binding::{Error as AdmissionError, IoBinding},
    io_jobs::{
        HostOwner, IoWorker, JobLimits, ManagedHostOwner, MutationHandle, OwnerCommandPoll,
        WorkerExit,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

const ID: &str = "org.example.mutation-opt-in";
const SUBJECT: &str = "mutation.opt-in";
const APPROVAL: [u8; 32] = [0x81; 32];

fn fixture() -> (
    tempfile::TempDir,
    Manager,
    HostRuntime,
    ManagedInstance,
    IoBinding,
) {
    let dir = tempfile::tempdir().unwrap();
    let wasm = wat::parse_str(
        r#"(module
        (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
        (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 4)
        (func (export "morrow_run") (result i32)
          (local $n i32)
          i32.const 0 i32.const 131072 call $read local.set $n
          i32.const 131072 local.get $n call $done drop
          i32.const 0))"#,
    )
    .unwrap();
    let caps = BTreeSet::from([IoCapability::FileDelete]);
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(vec![IoCapability::FileDelete], vec!["unused".into()]);
    let budget = declaration.budget.as_mut().unwrap();
    budget.max_jobs = 1;
    budget.max_resources = 2;
    budget.max_job_bytes = 1024 * 1024;
    budget.max_bytes = 4 * 1024 * 1024;
    budget.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.push(io::FEATURE.into());
    let package = Package::build(manifest, &wasm).unwrap();
    let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
    let mut manager = Manager::new(registry, Limits::default());
    manager.select(&package, manager.revision()).unwrap();
    manager
        .approve_io(ID, package.digest(), caps.clone(), manager.revision())
        .unwrap();
    manager
        .set_enabled(ID, package.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap();
    let instance = manager.connect(ID, &mut host).unwrap();
    let binding = manager
        .bind_io(
            &host,
            &instance,
            package.digest(),
            manager.revision(),
            &caps,
            30_000,
            1,
        )
        .unwrap();
    (dir, manager, host, instance, binding)
}

struct Original {
    manager: Manager,
    host: HostRuntime,
}
struct NoOptIn(Original);
impl HostOwner for NoOptIn {
    fn runtime(&self) -> &HostRuntime {
        &self.0.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.0.host
    }
}
impl ManagedHostOwner for NoOptIn {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.0.manager)
    }
}

struct WrongRuntime {
    original: Original,
    alternate: HostRuntime,
}
impl HostOwner for WrongRuntime {
    fn runtime(&self) -> &HostRuntime {
        &self.original.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.original.host
    }
}
impl ManagedHostOwner for WrongRuntime {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.original.manager)
    }
    fn with_managed_runtime<T>(
        &mut self,
        action: impl FnOnce(&Manager, &mut HostRuntime) -> T,
    ) -> Option<T> {
        Some(action(&self.original.manager, &mut self.alternate))
    }
}

fn spawn<O: ManagedHostOwner>(
    owner: O,
    instance: ManagedInstance,
    binding: IoBinding,
) -> IoWorker<O> {
    IoWorker::spawn_managed_owner(
        owner,
        instance,
        binding,
        || 5,
        1,
        JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap()
}
fn response(
    handle: &mut MutationHandle,
) -> Result<morrow_plugin_runtime::io_jobs::MutationResponse, TargetError> {
    let until = Instant::now() + Duration::from_secs(10);
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < until, "mutation command did not complete");
        thread::sleep(Duration::from_millis(1));
    }
    handle.read().unwrap().unwrap()
}
fn reclaim<O: HostOwner>(worker: &mut IoWorker<O>) -> WorkerExit<O> {
    worker.stop();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(exit) = worker.try_reclaim().unwrap() {
            return exit;
        }
        assert!(Instant::now() < until, "mutation worker did not stop");
        thread::sleep(Duration::from_millis(1));
    }
}
fn queue_selection<O: ManagedHostOwner>(worker: &IoWorker<O>, path: &Path) -> MutationHandle {
    worker
        .select_mutation_existing(
            path.to_path_buf(),
            SelectionScope {
                subject: SUBJECT.into(),
                approval_sha256: APPROVAL,
                disposition: Disposition::Delete,
            },
            [0x82; 32],
        )
        .unwrap()
        .1
}

#[test]
fn default_managed_owner_cannot_enter_mutation_file_selection() {
    let (dir, manager, host, instance, binding) = fixture();
    let leaf = dir.path().join("untouched.bin");
    fs::write(&leaf, b"sentinel").unwrap();
    let original_binding = host.binding();
    let pending = host.store_local().pending(0, 10).unwrap();
    let owner = NoOptIn(Original { manager, host });
    let mut worker = spawn(owner, instance, binding);

    let mut selected = queue_selection(&worker, &leaf);
    assert!(matches!(
        response(&mut selected),
        Err(TargetError::Admission(AdmissionError::Denied))
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"sentinel");
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.owner.0.host.binding(), original_binding);
    assert_eq!(
        exit.owner.0.host.store_local().pending(0, 10).unwrap(),
        pending
    );
    exit.owner.0.host.store_local().integrity_check().unwrap();
}

#[test]
fn split_borrow_substituting_another_runtime_is_rejected_before_file_selection() {
    let (dir, manager, host, instance, binding) = fixture();
    let leaf = dir.path().join("untouched.bin");
    fs::write(&leaf, b"sentinel").unwrap();
    let original_binding = host.binding();
    let original_pending = host.store_local().pending(0, 10).unwrap();
    let alternate = HostRuntime::new(
        Store::open(&dir.path().join("alternate.db"), Default::default()).unwrap(),
    )
    .unwrap();
    let alternate_binding = alternate.binding();
    let alternate_pending = alternate.store_local().pending(0, 10).unwrap();
    let owner = WrongRuntime {
        original: Original { manager, host },
        alternate,
    };
    let mut worker = spawn(owner, instance, binding);

    let mut selected = queue_selection(&worker, &leaf);
    assert!(matches!(
        response(&mut selected),
        Err(TargetError::Mismatch)
    ));
    assert_eq!(fs::read(&leaf).unwrap(), b"sentinel");
    let exit = reclaim(&mut worker);
    assert_eq!(exit.result, Ok(()));
    assert_eq!(exit.disconnect, Ok(()));
    assert_eq!(exit.owner.original.host.binding(), original_binding);
    assert_eq!(exit.owner.alternate.binding(), alternate_binding);
    assert_eq!(
        exit.owner
            .original
            .host
            .store_local()
            .pending(0, 10)
            .unwrap(),
        original_pending
    );
    assert_eq!(
        exit.owner.alternate.store_local().pending(0, 10).unwrap(),
        alternate_pending
    );
    exit.owner
        .original
        .host
        .store_local()
        .integrity_check()
        .unwrap();
    exit.owner
        .alternate
        .store_local()
        .integrity_check()
        .unwrap();
}
