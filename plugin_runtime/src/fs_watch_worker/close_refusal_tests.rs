//! One selected unit-only native fault fixture, never a production control API.
use super::*;
use crate::{
    Limits,
    directory_io::{DirectoryAbsolutePath, watch::protect_next_original_leaf_for_close_fixture},
    fs_watch_binding::ApprovalRequest,
    io_jobs::HostOwner,
    manager::Manager,
};
use morrow_core::{
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, io::IoCapability, registry::Registry},
    store::{EventBudget, Store},
};
use morrow_fs_watch_native_v1::windows::{Call, Fault as NativeFault};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Owner {
    manager: Manager,
    host: HostRuntime,
    prepare: Arc<AtomicUsize>,
    finish: Arc<AtomicUsize>,
}
impl HostOwner for Owner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
    fn prepare_io(&mut self) -> Result<(), JobError> {
        self.prepare.fetch_add(1, Ordering::AcqRel);
        // Runs only on the actual managed owner thread. The unit-only flag is
        // consumed by its next unique relative File before native ownership.
        protect_next_original_leaf_for_close_fixture();
        Ok(())
    }
    fn finish_io(&mut self) -> Result<(), JobError> {
        self.finish.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }
}
impl ManagedHostOwner for Owner {
    fn manager(&self) -> Option<&Manager> {
        Some(&self.manager)
    }
}
#[test]
fn actual_close_refusal_retains_original_whole_owner_and_shared_capacity() {
    let root = tempfile::tempdir().unwrap().keep();
    assert!(root.starts_with(r"D:\build_cache"));
    use std::os::windows::fs::MetadataExt;
    for path in root
        .ancestors()
        .take_while(|p| *p != std::path::Path::new(r"D:\build_cache"))
    {
        assert_eq!(
            fs::symlink_metadata(path).unwrap().file_attributes() & 0x400,
            0
        );
        assert!(!path.join(".git").exists());
    }
    let wasm = wat::parse_str(
        r#"(module
        (import "morrow_task_v1" "read_input" (func (param i32 i32) (result i32)))
        (import "morrow_task_v1" "complete" (func (param i32 i32) (result i32)))
        (memory (export "memory") 2 256)
        (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let package = Package::build(
        Package::manifest_for_fs_watch(
            "org.example.watch.close",
            "1.0.0",
            &wasm,
            vec!["observe".into()],
        ),
        &wasm,
    )
    .unwrap();
    let catalog = Catalog::open(&root.join("catalog")).unwrap();
    catalog.install(&package).unwrap();
    let mut manager = Manager::new(
        Registry::open(&root.join("registry"), catalog).unwrap(),
        Limits::default(),
    );
    let id = package.manifest().package_id.as_str();
    let caps = BTreeSet::from([IoCapability::FileList]);
    manager.select(&package, 0).unwrap();
    manager
        .approve_io(id, package.digest(), caps.clone(), manager.revision())
        .unwrap();
    manager
        .approve_fs_watch(id, package.digest(), true, manager.revision())
        .unwrap();
    manager
        .set_enabled(id, package.digest(), true, manager.revision())
        .unwrap();
    let mut host =
        HostRuntime::new(Store::open(&root.join("store"), EventBudget::default()).unwrap())
            .unwrap();
    let instance = manager.connect(id, &mut host).unwrap();
    let binding = manager
        .bind_io(
            &host,
            &instance,
            package.digest(),
            manager.revision(),
            &caps,
            29_000,
            1,
        )
        .unwrap();
    let path = root.join("observed");
    fs::create_dir(&path).unwrap();
    let approval = WatchApproval::issue(
        &manager,
        &host,
        &instance,
        &binding,
        ApprovalRequest {
            expected_revision: manager.revision(),
            handler: "observe".into(),
            nominated_path: DirectoryAbsolutePath::new(&path).unwrap(),
            max_frames: 2,
            max_wire_bytes: 512 * 1024,
            window_ms: 20_000,
        },
        1,
    )
    .unwrap();
    let prepare = Arc::new(AtomicUsize::new(0));
    let finish = Arc::new(AtomicUsize::new(0));
    let owner = Owner {
        manager,
        host,
        prepare: prepare.clone(),
        finish: finish.clone(),
    };
    let mut worker = WatchWorker::spawn_owned(owner, instance, approval, || 1).unwrap();
    let ready_until = Instant::now() + Duration::from_secs(5);
    let mut observed_read = false;
    while Instant::now() < ready_until {
        let s = worker.snapshot();
        if s.native
            .is_some_and(|n| n.native.is_some_and(|n| n.read_in_flight))
        {
            observed_read = true;
            break;
        }
        if s.phase == Phase::Finished {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    worker.cancel();
    let join_until = Instant::now() + Duration::from_secs(10);
    let exit = loop {
        if let Some(exit) = worker.try_join().unwrap() {
            break exit;
        }
        assert!(
            Instant::now() < join_until,
            "original managed close-refusal join unresolved"
        );
        thread::sleep(Duration::from_millis(2));
    };
    let evidence = PathBuf::from(
        std::env::var_os("MORROW_WATCH_WORKER_EVIDENCE").expect("explicit qualification output"),
    );
    assert!(evidence.starts_with(r"D:\build_cache"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(evidence.join("native-close-refusal-exit.txt"))
        .unwrap();
    file.write_all(format!("original_join_returned=true\nresult={:?}\nunknown={}\nowner_returned={}\nnative_complete={}\ndisconnect={:?}\nmaintenance={:?}\nsnapshot={:?}\nusage={:?}\nlast_execution={:?}\noriginal_prepare_calls={}\noriginal_finish_calls={}\nprocess_exit_is_not_native_retirement=true\n",exit.result,exit.unknown,exit.owner.is_some(),exit.native_retirement.is_complete(),exit.disconnect,exit.maintenance,exit.snapshot,exit.usage,exit.last_execution,prepare.load(Ordering::Acquire),finish.load(Ordering::Acquire)).as_bytes()).unwrap();
    file.sync_all().unwrap();
    assert!(observed_read);
    assert!(matches!(worker.try_join(), Err(Error::Consumed)));
    assert_eq!(exit.result, Err(Error::Cancelled));
    assert!(exit.unknown && exit.owner.is_none() && exit.instance.is_none());
    assert!(!exit.native_retirement.is_complete());
    assert!(exit.disconnect.is_none() && exit.maintenance.is_none());
    assert!(exit.deliveries.is_empty() && exit.last_execution.is_none());
    assert_eq!(prepare.load(Ordering::Acquire), 1);
    assert_eq!(finish.load(Ordering::Acquire), 0);
    let snapshot = exit.snapshot.native.unwrap();
    assert_eq!(snapshot.ancestors.objects, 4);
    assert_eq!(snapshot.ancestors.close_calls, 0);
    assert_eq!(snapshot.ancestors.confirmed_closed, 0);
    assert!(snapshot.unknown && !snapshot.retired && snapshot.job_held);
    assert_eq!(snapshot.held_resources, 6);
    let native = snapshot.native.unwrap();
    assert!(native.unknown && !native.retired && !native.read_in_flight);
    assert_eq!(
        native.first_error,
        Some(NativeFault::Native {
            call: Call::CloseDirectory,
            code: 6
        })
    );
    assert_eq!(
        (
            native.owned_handles,
            native.close_calls,
            native.confirmed_closed
        ),
        (2, 1, 0)
    );
    assert_eq!((native.read_calls, native.completions), (1, 1));
    assert!(native.cancel_called);
    assert_eq!(binding.usage().resources, 6);
    assert_eq!(binding.usage().jobs, 1);
    // No further call, flag removal, native cleanup or maintenance occurs.
    // The original owner allocation remains until this isolated test process exits.
}
