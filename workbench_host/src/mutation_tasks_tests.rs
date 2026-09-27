//! Windows native application task tests with real protected storage.
use super::*;
use crate::io_tasks::{StateSlot, StoragePhase};
use morrow_core::{
    file_mutation::{MutationRequest, Target},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
};
use morrow_plugin_runtime::{Limits, io_jobs::JobLimits, manager::Manager};
use sha2::{Digest, Sha256};
use std::{fs, thread, time::Instant};
fn serial_effects() -> std::sync::MutexGuard<'static, ()> {
    static EFFECTS: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    EFFECTS
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
const ID: &str = "org.example.workbench.mutation";
const HANDLER: &str = "mutation.unused";
const SUBJECT: &str = "workbench.mutation-test";
const APPROVAL: [u8; 32] = [83; 32];
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
        let mut decl = io::declaration(
            vec![
                IoCapability::FileCreate,
                IoCapability::FileDelete,
                IoCapability::FileReplace,
            ],
            vec![HANDLER.into()],
        );
        let b = decl.budget.as_mut().unwrap();
        b.max_jobs = 4;
        b.max_resources = 8;
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
                BTreeSet::from([
                    IoCapability::FileCreate,
                    IoCapability::FileDelete,
                    IoCapability::FileReplace,
                ]),
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
    fn options(&self, disposition: Disposition) -> StartOptions {
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
            capabilities: BTreeSet::from([disposition.capability()]),
            lifetime: Duration::from_secs(10),
            limits: JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
        }
    }
}
fn scope(disposition: Disposition) -> SelectionScope {
    SelectionScope {
        subject: SUBJECT.into(),
        approval_sha256: APPROVAL,
        disposition,
    }
}
fn read(
    app: &mut Workbench,
    key: TaskKey,
    command: u64,
) -> std::result::Result<MutationResponse, Failure> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(result) = app.read_mutation_result(key, command).unwrap() {
            return result;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
fn call(
    app: &mut Workbench,
    key: TaskKey,
    action: Action,
) -> std::result::Result<MutationResponse, Failure> {
    let command = app.request_mutation(key, action).unwrap();
    read(app, key, command)
}
fn reclaim(app: &mut Workbench, key: TaskKey) {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let status = app.poll_io(key).unwrap();
        if let Some(exit) = status.exit {
            assert_eq!(exit.execution, Ok(()));
            assert_eq!(exit.disconnect, Ok(()));
            assert_eq!(exit.maintenance, Ok(()));
            break;
        }
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
}
fn plan(
    digest: [u8; 32],
    operation: &str,
    disposition: Disposition,
    reference: [u8; 32],
    path: Option<RelativeFilePath>,
    expected: Option<[u8; 32]>,
    bytes: &[u8],
) -> RequestRecord {
    RequestRecord::new(MutationRequest {
        operation_id: operation.into(),
        subject: SUBJECT.into(),
        package_sha256: digest,
        approval_sha256: APPROVAL,
        target: Target {
            reference,
            relative_path: path,
        },
        disposition,
        expected_identity: expected,
        content_length: bytes.len() as u64,
        content_sha256: (disposition != Disposition::Delete).then(|| Sha256::digest(bytes).into()),
    })
    .unwrap()
}
#[test]
fn create_commands_keep_original_owner_until_release_and_real_join() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original = setup.app.local_state().unwrap().host.binding();
    let root = setup.dir.path().join("root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("new.bin");
    let relative = RelativeFilePath::parse("new.bin").unwrap();
    let key = setup
        .app
        .start_mutation(
            setup.options(Disposition::Create),
            Selection::Create {
                root,
                relative: relative.clone(),
            },
            scope(Disposition::Create),
        )
        .unwrap();
    assert!(setup.app.local_state().is_err());
    assert!(setup.app.acknowledge_io(key).is_err());
    assert!(setup.app.request_mutation(key, Action::Query).is_err());
    let MutationResponse::Selected {
        reference,
        expected_identity: None,
    } = read(&mut setup.app, key, 1).unwrap()
    else {
        panic!("selection");
    };
    assert_eq!(
        setup
            .app
            .mutation_status(key)
            .unwrap()
            .selection
            .unwrap()
            .reference,
        reference
    );
    let bytes = vec![0xa5; 70_001];
    let request = plan(
        setup.digest,
        "workbench-create",
        Disposition::Create,
        reference,
        Some(relative),
        None,
        &bytes,
    );
    let prepare = setup
        .app
        .request_mutation(key, Action::Prepare(Box::new(request)))
        .unwrap();
    assert!(setup.app.read_mutation_result(key, 1).is_err());
    assert!(setup.app.cancel_mutation_command(key, 1).is_err());
    assert!(setup.app.request_mutation(key, Action::Query).is_err());
    assert!(matches!(
        read(&mut setup.app, key, prepare),
        Ok(MutationResponse::Prepared(_))
    ));
    assert!(setup.app.read_mutation_result(key, prepare).is_err());
    for (n, chunk) in bytes.chunks(50_000).enumerate() {
        call(
            &mut setup.app,
            key,
            Action::Chunk {
                offset: (n * 50_000) as u64,
                bytes: chunk.to_vec().into(),
            },
        )
        .unwrap();
    }
    call(&mut setup.app, key, Action::CommitContent).unwrap();
    let execute = setup.app.request_mutation(key, Action::Execute).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while setup.app.mutation_status(key).unwrap().delivery != OwnerCommandPoll::Ready {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(setup.app.poll_io(key).unwrap().delivery, Some(Poll::Ready));
    setup.app.cancel_mutation_command(key, execute).unwrap();
    assert!(matches!(
        read(&mut setup.app, key, execute),
        Err(Failure::Delivery(OwnerCommandError::Unknown))
    ));
    assert!(setup.app.mutation_status(key).unwrap().reconcile_required);
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    let MutationResponse::History {
        record: Some(record),
        ..
    } = call(&mut setup.app, key, Action::Query).unwrap()
    else {
        panic!("history");
    };
    assert_eq!(record.phase(), Phase::Observed);
    assert!(setup.app.mutation_status(key).unwrap().terminal);
    assert!(!setup.app.mutation_status(key).unwrap().reconcile_required);
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    assert_eq!(fs::read(leaf).unwrap(), bytes);
    assert!(setup.app.local_state().is_err());
    call(&mut setup.app, key, Action::Release).unwrap();
    reclaim(&mut setup.app, key);
    assert_eq!(setup.app.local_state().unwrap().host.binding(), original);
    setup.app.acknowledge_io(key).unwrap();
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
}
#[test]
fn task_plan_cancel_retains_history_without_deleting_the_selected_file() {
    let mut setup = Setup::new("rust");
    let leaf = setup.dir.path().join("keep.bin");
    fs::write(&leaf, b"keep").unwrap();
    let key = setup
        .app
        .start_mutation(
            setup.options(Disposition::Delete),
            Selection::Existing(leaf.clone()),
            scope(Disposition::Delete),
        )
        .unwrap();
    let MutationResponse::Selected {
        reference,
        expected_identity,
    } = read(&mut setup.app, key, 1).unwrap()
    else {
        panic!("selection");
    };
    let request = plan(
        setup.digest,
        "workbench-cancel",
        Disposition::Delete,
        reference,
        None,
        expected_identity,
        b"",
    );
    call(&mut setup.app, key, Action::Prepare(Box::new(request))).unwrap();
    assert!(matches!(
        call(&mut setup.app, key, Action::CancelPlan),
        Ok(MutationResponse::PlanCancelled(_))
    ));
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    let MutationResponse::History {
        record: Some(record),
        ..
    } = call(&mut setup.app, key, Action::Query).unwrap()
    else {
        panic!("cancel history");
    };
    assert_eq!(record.phase(), Phase::CancelledBeforeDispatch);
    call(&mut setup.app, key, Action::Release).unwrap();
    reclaim(&mut setup.app, key);
    assert_eq!(fs::read(&leaf).unwrap(), b"keep");
    setup.app.acknowledge_io(key).unwrap();
}
#[test]
fn mismatched_admission_does_not_move_owner_or_open_target() {
    let mut setup = Setup::new("rust");
    let original = setup.app.local_state().unwrap().host.binding();
    let leaf = setup.dir.path().join("missing");
    assert!(
        setup
            .app
            .start_mutation(
                setup.options(Disposition::Delete),
                Selection::Existing(leaf.clone()),
                scope(Disposition::Create)
            )
            .is_err()
    );
    assert_eq!(setup.app.local_state().unwrap().host.binding(), original);
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    let mut options = setup.options(Disposition::Delete);
    options.revision += 1;
    assert!(
        setup
            .app
            .start_mutation(
                options,
                Selection::Existing(leaf),
                scope(Disposition::Delete)
            )
            .is_err()
    );
    assert_eq!(setup.app.local_state().unwrap().host.binding(), original);
}
#[test]
fn lost_selection_reply_preserves_query_and_explicit_stop_paths() {
    let mut setup = Setup::new("rust");
    let leaf = setup.dir.path().join("selected.bin");
    fs::write(&leaf, b"untouched").unwrap();
    let key = setup
        .app
        .start_mutation(
            setup.options(Disposition::Delete),
            Selection::Existing(leaf.clone()),
            scope(Disposition::Delete),
        )
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while setup.app.mutation_status(key).unwrap().delivery != OwnerCommandPoll::Ready {
        assert!(Instant::now() < until);
        thread::sleep(Duration::from_millis(1));
    }
    setup.app.cancel_mutation_command(key, 1).unwrap();
    assert!(read(&mut setup.app, key, 1).is_err());
    assert!(!setup.app.mutation_status(key).unwrap().selected);
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    // The runtime may already have reaped the undelivered selection. Either
    // Missing or empty history is explicit; the application must not auto-select.
    let _ = call(&mut setup.app, key, Action::Query);
    assert!(setup.app.local_state().is_err());
    setup.app.cancel_io(key).unwrap();
    reclaim(&mut setup.app, key);
    assert_eq!(fs::read(&leaf).unwrap(), b"untouched");
    setup.app.acknowledge_io(key).unwrap();
}

#[path = "mutation_wire_tests.rs"]
mod wire_tests;

#[test]
fn failed_catalog_admission_burns_start_submission_before_any_selection() {
    let mut setup = Setup::new("rust");
    let revision = setup.options(Disposition::Delete).revision;
    let leaf = setup.dir.path().join("not-opened.bin");
    fs::write(&leaf, b"unchanged").unwrap();
    let make = || StartRequest {
        submission: [201; 32],
        package_id: ID.into(),
        digest: [202; 32],
        revision,
        disposition: Disposition::Delete,
        selected_path: leaf.clone(),
        relative_path: None,
        subject: SUBJECT.into(),
        approval: APPROVAL,
        timeout_ms: 10_000,
    };
    assert!(setup.app.start_selected_mutation(make()).is_err());
    let retry = setup.app.start_selected_mutation(make()).unwrap_err();
    assert!(
        retry
            .to_string()
            .contains("original mutation admission failed")
    );
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert_eq!(setup.app.http_submission(), Some([201; 32]));
    assert_eq!(fs::read(&leaf).unwrap(), b"unchanged");
}

#[path = "mutation_reconciliation_tests.rs"]
mod reconciliation_tests;

#[path = "mutation_discovery_tests.rs"]
mod discovery_tests;
