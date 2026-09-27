//! Real Workbench admission and original-owner guest mutation tests.
use super::*;
use crate::io_tasks::{StateSlot, StoragePhase};
use morrow_core::{
    mutation as wire,
    plugin_package::{
        MAX_MUTATION_BYTES, MAX_MUTATION_JOB_BYTES, MUTATION_BUDGET_FEATURE, MUTATION_FEATURE,
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        proto::MutationBudget as DeclaredBudget,
        registry::Registry,
    },
};
use morrow_plugin_runtime::{Limits, manager::Manager};
use std::{fs, path::PathBuf, thread, time::Instant};

const ID: &str = "org.example.workbench.guest-mutation";
const SUBJECT: &str = "workbench.guest-mutation-test";
const APPROVAL: [u8; 32] = [83; 32];
const BUDGET: MutationBudget = MutationBudget {
    max_job_bytes: MAX_MUTATION_JOB_BYTES,
    max_bytes: MAX_MUTATION_BYTES,
};

fn serial_effects() -> std::sync::MutexGuard<'static, ()> {
    static EFFECTS: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    EFFECTS
        .get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

fn wasm() -> Vec<u8> {
    let bytes = if let Some(path) = std::env::var_os("MORROW_WORKBENCH_MUTATION_WASM_PATH") {
        fs::read(PathBuf::from(path)).expect("specified real mutation Wasm must load")
    } else {
        wat::parse_str(
            r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_mutation_v1" "call" (func $mutation (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32)
            (local $n i32) (local $m i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 0 local.get $n i32.const 131072 i32.const 131072 call $mutation local.set $m
            i32.const 131072 local.get $m call $done drop
            i32.const 0))"#,
        )
        .unwrap()
    };
    println!("WORKBENCH_GUEST_SHA256={:x}", Sha256::digest(&bytes));
    bytes
}

struct Setup {
    app: Workbench,
    dir: tempfile::TempDir,
    digest: [u8; 32],
    approved_budget: Option<MutationBudget>,
}
impl Setup {
    fn new() -> Self {
        Self::with_budget(true)
    }
    fn with_budget(extended: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let wasm = wasm();
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let caps = vec![IoCapability::FileCreate, IoCapability::FileDelete];
        let mut declaration = io::declaration(caps.clone(), vec!["mutation.unused".into()]);
        let old = declaration.budget.as_mut().unwrap();
        old.max_jobs = 4;
        old.max_resources = 8;
        old.max_job_bytes = 16 * 1024 * 1024;
        old.max_bytes = 64 * 1024 * 1024;
        old.max_duration_ms = 30_000;
        manifest.io_declaration = Some(declaration);
        manifest
            .required_features
            .extend([io::FEATURE.into(), MUTATION_FEATURE.into()]);
        manifest.mutation_schema_sha256 = wire::schema_digest().to_vec();
        if extended {
            manifest
                .required_features
                .push(MUTATION_BUDGET_FEATURE.into());
            manifest.mutation_budget = Some(DeclaredBudget {
                max_job_bytes: BUDGET.max_job_bytes,
                max_bytes: BUDGET.max_bytes,
            });
        }
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
                caps.into_iter().collect(),
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
        Self {
            app: Workbench {
                http_tasks: Default::default(),
                state: StateSlot::new(owner),
            },
            dir,
            digest: package.digest(),
            approved_budget: extended.then_some(BUDGET),
        }
    }
    fn start(
        &mut self,
        submission: u8,
        disposition: Disposition,
        selected_path: PathBuf,
        relative_path: Option<RelativeFilePath>,
    ) -> TaskKey {
        let request = self.request(submission, disposition, selected_path, relative_path);
        self.app
            .start_selected_guest_mutation(request, self.approved_budget)
            .unwrap()
    }
    fn request(
        &mut self,
        submission: u8,
        disposition: Disposition,
        selected_path: PathBuf,
        relative_path: Option<RelativeFilePath>,
    ) -> StartRequest {
        StartRequest {
            submission: [submission; 32],
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
            disposition,
            selected_path,
            relative_path,
            subject: SUBJECT.into(),
            approval: APPROVAL,
            timeout_ms: 30_000,
        }
    }
}

fn read(
    app: &mut Workbench,
    key: TaskKey,
    command: u64,
) -> std::result::Result<GuestMutationReply, GuestMutationFailure> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(reply) = app.read_guest_mutation_result(key, command).unwrap() {
            return reply;
        }
        assert!(
            Instant::now() < until,
            "guest reply timeout for command {command}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}
fn call(app: &mut Workbench, key: TaskKey, token: u8, action: GuestAction) -> GuestMutationReply {
    let command = app.submit_guest_mutation(key, [token; 32], action).unwrap();
    read(app, key, command).unwrap()
}
fn selected(app: &mut Workbench, key: TaskKey) {
    assert!(matches!(
        read(app, key, 1).unwrap(),
        GuestMutationReply::Owner(MutationResponse::Selected { .. })
    ));
    assert!(app.guest_mutation_status(key).unwrap().selected);
}
fn plan(
    app: &mut Workbench,
    key: TaskKey,
    token: u8,
    operation: &str,
    bytes: &[u8],
    delete: bool,
) -> [u8; 32] {
    let action = GuestAction::BuildPlan {
        operation_id: operation.into(),
        content_length: if delete { 0 } else { bytes.len() as u64 },
        content_sha256: (!delete).then(|| Sha256::digest(bytes).into()),
    };
    let GuestMutationReply::Owner(MutationResponse::Planned(record)) =
        call(app, key, token, action)
    else {
        panic!("host canonical plan");
    };
    assert_eq!(record.request().operation_id, operation);
    assert_eq!(record.request().subject, SUBJECT);
    assert_eq!(record.request().approval_sha256, APPROVAL);
    Sha256::digest(record.container()).into()
}
fn stop(app: &mut Workbench, key: TaskKey) {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(exit) = app.poll_io(key).unwrap().exit {
            assert_eq!(exit.execution, Ok(()));
            assert_eq!(exit.disconnect, Ok(()));
            assert_eq!(exit.maintenance, Ok(()));
            break;
        }
        assert!(Instant::now() < until, "guest worker did not reclaim");
        thread::sleep(Duration::from_millis(1));
    }
    app.acknowledge_io(key).unwrap();
    assert_eq!(app.io_status().storage, StoragePhase::Local);
}

fn repeated_create_request(
    digest: [u8; 32],
    revision: u64,
    path: PathBuf,
    token: u8,
) -> StartRequest {
    StartRequest {
        submission: [token; 32],
        package_id: ID.into(),
        digest,
        revision,
        disposition: Disposition::Create,
        selected_path: path,
        relative_path: Some(RelativeFilePath::parse("pending.bin").unwrap()),
        subject: SUBJECT.into(),
        approval: APPROVAL,
        timeout_ms: 30_000,
    }
}

#[test]
fn guest_sdk_original_owner_create_delete() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("payload.bin");
    let relative = RelativeFilePath::parse("payload.bin").unwrap();
    let body: Vec<u8> = (0..(3 * wire::MAX_CHUNK_BYTES + 37))
        .map(|n| (n % 251) as u8)
        .collect();
    let key = setup.start(1, Disposition::Create, root, Some(relative));
    assert!(setup.app.request_mutation(key, Action::Query).is_err());
    assert!(setup.app.mutation_status(key).is_err());
    selected(&mut setup.app, key);
    let sha = plan(&mut setup.app, key, 2, "guest-create-1", &body, false);
    assert!(!leaf.exists());
    assert!(
        setup
            .app
            .submit_guest_mutation(
                key,
                [3; 32],
                GuestAction::Prepare {
                    plan_sha256: [9; 32]
                }
            )
            .is_err()
    );
    let GuestMutationReply::Frame(prepared) = call(
        &mut setup.app,
        key,
        3,
        GuestAction::Prepare { plan_sha256: sha },
    ) else {
        panic!("guest Create prepare");
    };
    assert_eq!(
        (prepared.status, prepared.phase),
        (wire::Status::Completed, wire::Phase::Prepared)
    );
    assert!(!leaf.exists());
    for (index, chunk) in body.chunks(wire::MAX_CHUNK_BYTES).enumerate() {
        let GuestMutationReply::Frame(staged) = call(
            &mut setup.app,
            key,
            4 + index as u8,
            GuestAction::Chunk {
                offset: (index * wire::MAX_CHUNK_BYTES) as u64,
                bytes: Zeroizing::new(chunk.to_vec()),
            },
        ) else {
            panic!("guest chunk");
        };
        assert_eq!(
            staged.staged_bytes,
            (index * wire::MAX_CHUNK_BYTES + chunk.len()) as u64
        );
        assert!(!staged.durable_content);
        assert!(!leaf.exists());
    }
    let GuestMutationReply::Frame(committed) =
        call(&mut setup.app, key, 9, GuestAction::CommitContent)
    else {
        panic!("guest Commit");
    };
    assert!(committed.durable_content);
    assert!(!leaf.exists());
    assert!(
        setup
            .app
            .submit_guest_mutation(
                key,
                [10; 32],
                GuestAction::Execute {
                    plan_sha256: [8; 32]
                }
            )
            .is_err()
    );
    let GuestMutationReply::Frame(executed) = call(
        &mut setup.app,
        key,
        10,
        GuestAction::Execute { plan_sha256: sha },
    ) else {
        panic!("guest Execute");
    };
    assert_eq!(
        (executed.phase, executed.effect),
        (wire::Phase::Observed, wire::Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&leaf).unwrap(), body);
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [11; 32], GuestAction::Execute { plan_sha256: sha })
            .is_err()
    );
    let GuestMutationReply::Owner(MutationResponse::History {
        record: Some(record),
        ..
    }) = call(&mut setup.app, key, 11, GuestAction::HostQuery)
    else {
        panic!("host observed history");
    };
    assert_eq!(record.phase(), Phase::Observed);
    assert!(
        !setup
            .app
            .guest_mutation_status(key)
            .unwrap()
            .reconcile_required
    );
    let GuestMutationReply::Frame(again) = call(&mut setup.app, key, 12, GuestAction::Query) else {
        panic!("guest Query");
    };
    assert_eq!(again.phase, wire::Phase::Observed);
    assert!(matches!(
        call(&mut setup.app, key, 18, GuestAction::Release),
        GuestMutationReply::Frame(_)
    ));
    stop(&mut setup.app, key);

    let original = fs::read(&leaf).unwrap();
    let delete = setup.start(13, Disposition::Delete, leaf.clone(), None);
    selected(&mut setup.app, delete);
    let sha = plan(&mut setup.app, delete, 14, "guest-delete-2", b"", true);
    let GuestMutationReply::Frame(prepared) = call(
        &mut setup.app,
        delete,
        15,
        GuestAction::Prepare { plan_sha256: sha },
    ) else {
        panic!("guest Delete prepare");
    };
    assert_eq!(prepared.phase, wire::Phase::Prepared);
    // The selected Windows file is locked; durable content is checked from the Create above.
    assert_eq!(original.len(), 3 * wire::MAX_CHUNK_BYTES + 37);
    let GuestMutationReply::Frame(deleted) = call(
        &mut setup.app,
        delete,
        16,
        GuestAction::Execute { plan_sha256: sha },
    ) else {
        panic!("guest Delete execute");
    };
    assert_eq!(
        (deleted.phase, deleted.effect),
        (wire::Phase::Observed, wire::Effect::OsSucceeded)
    );
    assert!(!leaf.exists());
    assert!(matches!(
        call(&mut setup.app, delete, 17, GuestAction::Release),
        GuestMutationReply::Frame(_)
    ));
    stop(&mut setup.app, delete);
}

#[test]
fn guest_start_budget_is_part_of_exact_submission_identity() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("budget");
    fs::create_dir(&root).unwrap();
    let relative = RelativeFilePath::parse("pending.bin").unwrap();
    let request = setup.request(21, Disposition::Create, root, Some(relative));
    let revision = request.revision;
    let key = setup
        .app
        .start_selected_guest_mutation(request, Some(BUDGET))
        .unwrap();
    let mut changed = BUDGET;
    changed.max_bytes -= 1;
    let retry = setup.app.start_selected_guest_mutation(
        repeated_create_request(setup.digest, revision, setup.dir.path().join("budget"), 21),
        Some(changed),
    );
    assert!(retry.unwrap_err().to_string().contains("conflicts"));
    let mut changed = BUDGET;
    changed.max_job_bytes -= 1;
    assert!(
        setup
            .app
            .start_selected_guest_mutation(
                repeated_create_request(
                    setup.digest,
                    revision,
                    setup.dir.path().join("budget"),
                    21
                ),
                Some(changed),
            )
            .unwrap_err()
            .to_string()
            .contains("conflicts")
    );
    assert!(
        setup
            .app
            .start_selected_guest_mutation(
                repeated_create_request(
                    setup.digest,
                    revision,
                    setup.dir.path().join("budget"),
                    21
                ),
                None,
            )
            .unwrap_err()
            .to_string()
            .contains("conflicts")
    );
    selected(&mut setup.app, key);
    assert!(matches!(
        call(&mut setup.app, key, 22, GuestAction::HostRelease),
        GuestMutationReply::Owner(MutationResponse::Released)
    ));
    stop(&mut setup.app, key);
}

#[test]
fn absent_extended_budget_burns_failed_start_token() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("missing-budget");
    fs::create_dir(&root).unwrap();
    let request = setup.request(
        24,
        Disposition::Create,
        root.clone(),
        Some(RelativeFilePath::parse("pending.bin").unwrap()),
    );
    let revision = request.revision;
    assert!(
        setup
            .app
            .start_selected_guest_mutation(request, None)
            .is_err()
    );
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert!(
        setup
            .app
            .start_selected_guest_mutation(
                repeated_create_request(setup.digest, revision, root, 24),
                Some(BUDGET),
            )
            .unwrap_err()
            .to_string()
            .contains("conflicts")
    );
}

#[test]
fn ordinary_mutation_v1_uses_declared_legacy_budget_without_extension() {
    let _serial = serial_effects();
    let mut setup = Setup::with_budget(false);
    let leaf = setup.dir.path().join("legacy-delete.bin");
    fs::write(&leaf, b"legacy target").unwrap();
    let key = setup.start(25, Disposition::Delete, leaf.clone(), None);
    selected(&mut setup.app, key);
    let sha = plan(&mut setup.app, key, 26, "legacy-delete", b"", true);
    let GuestMutationReply::Frame(prepared) = call(
        &mut setup.app,
        key,
        27,
        GuestAction::Prepare { plan_sha256: sha },
    ) else {
        panic!("legacy guest prepare");
    };
    assert_eq!(prepared.phase, wire::Phase::Prepared);
    let GuestMutationReply::Frame(deleted) = call(
        &mut setup.app,
        key,
        28,
        GuestAction::Execute { plan_sha256: sha },
    ) else {
        panic!("legacy guest Execute");
    };
    assert_eq!(
        (deleted.phase, deleted.effect),
        (wire::Phase::Observed, wire::Effect::OsSucceeded)
    );
    assert!(!leaf.exists());
    assert!(matches!(
        call(&mut setup.app, key, 29, GuestAction::Release),
        GuestMutationReply::Frame(_)
    ));
    stop(&mut setup.app, key);
}

#[test]
fn unread_guest_command_and_explicit_cancel_never_start_effect() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("unread");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("absent.bin");
    let key = setup.start(
        31,
        Disposition::Create,
        root,
        Some(RelativeFilePath::parse("absent.bin").unwrap()),
    );
    selected(&mut setup.app, key);
    let sha = plan(&mut setup.app, key, 32, "guest-unread", b"", false);
    let command = setup
        .app
        .submit_guest_mutation(key, [33; 32], GuestAction::Prepare { plan_sha256: sha })
        .unwrap();
    assert_eq!(
        setup
            .app
            .submit_guest_mutation(key, [33; 32], GuestAction::Prepare { plan_sha256: sha })
            .unwrap(),
        command
    );
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [34; 32], GuestAction::Execute { plan_sha256: sha })
            .is_err()
    );
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    // Reading the internal Issue receipt advances to a single guest frame;
    // this status observation never delivers the frame or grants Execute.
    let until = Instant::now() + Duration::from_secs(30);
    while !setup
        .app
        .guest_mutation_status(key)
        .unwrap()
        .approval_delivered
    {
        assert!(Instant::now() < until, "guest Issue did not advance");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [34; 32], GuestAction::Execute { plan_sha256: sha })
            .is_err()
    );
    setup
        .app
        .cancel_guest_mutation_command(key, command)
        .unwrap();
    let _ = read(&mut setup.app, key, command);
    assert!(!leaf.exists());
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [34; 32], GuestAction::Execute { plan_sha256: sha })
            .is_err()
    );
    assert!(matches!(
        call(&mut setup.app, key, 35, GuestAction::HostRelease),
        GuestMutationReply::Owner(MutationResponse::Released)
    ));
    stop(&mut setup.app, key);
}

#[test]
fn lost_commit_receipt_requires_history_and_never_executes_implicitly() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("lost-commit");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("still-absent.bin");
    let body = b"durable but not executed";
    let key = setup.start(
        41,
        Disposition::Create,
        root,
        Some(RelativeFilePath::parse("still-absent.bin").unwrap()),
    );
    selected(&mut setup.app, key);
    let sha = plan(&mut setup.app, key, 42, "guest-lost-commit", body, false);
    assert!(matches!(
        call(
            &mut setup.app,
            key,
            43,
            GuestAction::Prepare { plan_sha256: sha }
        ),
        GuestMutationReply::Frame(_)
    ));
    assert!(matches!(
        call(
            &mut setup.app,
            key,
            44,
            GuestAction::Chunk {
                offset: 0,
                bytes: Zeroizing::new(body.to_vec())
            }
        ),
        GuestMutationReply::Frame(_)
    ));
    let command = setup
        .app
        .submit_guest_mutation(key, [45; 32], GuestAction::CommitContent)
        .unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    while setup.app.guest_mutation_status(key).unwrap().delivery != Poll::Ready {
        assert!(Instant::now() < until, "Commit did not complete");
        thread::sleep(Duration::from_millis(1));
    }
    // The public cancel path drops the completed but unread frame; it cannot
    // undo Core's durable Commit or turn an unknown receipt into success.
    setup
        .app
        .cancel_guest_mutation_command(key, command)
        .unwrap();
    assert!(matches!(
        read(&mut setup.app, key, command),
        Err(GuestMutationFailure::Cancelled)
    ));
    assert!(
        setup
            .app
            .guest_mutation_status(key)
            .unwrap()
            .reconcile_required
    );
    assert!(
        !setup
            .app
            .guest_mutation_status(key)
            .unwrap()
            .durable_content
    );
    assert!(!leaf.exists());
    let GuestMutationReply::Owner(MutationResponse::History {
        record: Some(record),
        staged_bytes,
        durable_content,
    }) = call(&mut setup.app, key, 46, GuestAction::HostQuery)
    else {
        panic!("host Commit history");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    assert_eq!(staged_bytes, body.len() as u64);
    assert!(durable_content);
    let status = setup.app.guest_mutation_status(key).unwrap();
    assert_eq!(status.staged_bytes, body.len() as u64);
    assert!(status.durable_content);
    assert!(!status.effect_attempted);
    assert!(!leaf.exists());
    assert!(matches!(
        call(&mut setup.app, key, 47, GuestAction::HostRelease),
        GuestMutationReply::Owner(MutationResponse::Released)
    ));
    stop(&mut setup.app, key);
}

#[test]
fn attempted_effect_absent_or_prepared_history_cannot_reenable_execute() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    for (index, prepare) in [(0u8, false), (1, true)] {
        let root = setup.dir.path().join(format!("uncertain-{index}"));
        fs::create_dir(&root).unwrap();
        let leaf = root.join("absent.bin");
        let key = setup.start(
            51 + index * 10,
            Disposition::Create,
            root,
            Some(RelativeFilePath::parse("absent.bin").unwrap()),
        );
        selected(&mut setup.app, key);
        let sha = plan(
            &mut setup.app,
            key,
            52 + index * 10,
            &format!("uncertain-{index}"),
            b"",
            false,
        );
        if prepare {
            assert!(matches!(
                call(
                    &mut setup.app,
                    key,
                    53 + index * 10,
                    GuestAction::Prepare { plan_sha256: sha }
                ),
                GuestMutationReply::Frame(_)
            ));
            assert!(matches!(
                call(
                    &mut setup.app,
                    key,
                    54 + index * 10,
                    GuestAction::CommitContent
                ),
                GuestMutationReply::Frame(_)
            ));
        }
        // Model a lost Execute receipt before an independent history lookup.
        // The retained attempt latch is never reset by a merely Absent or
        // Prepared record, even if the host query itself succeeds.
        {
            let mutation = guest_mutation(setup.app.state.checked_task(key).unwrap()).unwrap();
            mutation.guest.as_mut().unwrap().effect_attempted = true;
            mutation.reconcile_required = true;
        }
        let GuestMutationReply::Owner(MutationResponse::History { record, .. }) =
            call(&mut setup.app, key, 55 + index * 10, GuestAction::HostQuery)
        else {
            panic!("host history");
        };
        assert_eq!(
            record.as_ref().map(|r| r.phase()),
            prepare.then_some(Phase::Prepared)
        );
        let status = setup.app.guest_mutation_status(key).unwrap();
        assert!(status.effect_attempted);
        assert!(status.reconcile_required);
        assert!(
            setup
                .app
                .submit_guest_mutation(
                    key,
                    [56 + index * 10; 32],
                    GuestAction::Execute { plan_sha256: sha }
                )
                .is_err()
        );
        assert!(!leaf.exists());
        assert!(matches!(
            call(
                &mut setup.app,
                key,
                57 + index * 10,
                GuestAction::HostRelease
            ),
            GuestMutationReply::Owner(MutationResponse::Released)
        ));
        stop(&mut setup.app, key);
    }
}

#[path = "guest_mutation_wire_tests.rs"]
mod guest_wire_tests;

#[path = "mutation_guest_history_tests.rs"]
mod guest_history_tests;
