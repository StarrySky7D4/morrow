//! Budget declarations do not become write authority during historical recovery.
use super::*;
use crate::io_tasks::mutation::{DiscoveryStart, ReconciliationStart};
use morrow_plugin_runtime::io_jobs::MutationOutcome;

fn revision(setup: &mut Setup) -> u64 {
    setup
        .app
        .local_state()
        .unwrap()
        .manager
        .as_ref()
        .unwrap()
        .revision()
}

fn discover(setup: &mut Setup, token: u8, subject: &str) -> DiscoveryStart {
    DiscoveryStart {
        submission: [token; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(setup),
        subject: subject.into(),
        disposition: Disposition::Create,
        scan_limit: morrow_core::store::MAX_FILE_MUTATION_PLAN_SCAN_LIMIT,
        timeout_ms: 30_000,
        checkpoint: None,
    }
}

fn native_read(app: &mut Workbench, key: TaskKey, command: u64) -> MutationResponse {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(result) = app.read_mutation_result(key, command).unwrap() {
            return result.unwrap();
        }
        assert!(Instant::now() < until, "history timeout");
        thread::sleep(Duration::from_millis(1));
    }
}

fn close_discovery(app: &mut Workbench, key: TaskKey) {
    let command = app.request_mutation(key, Action::Release).unwrap();
    assert!(matches!(
        native_read(app, key, command),
        MutationResponse::Released
    ));
    stop(app, key);
}

fn tight_total_setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let wasm = wasm();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let caps = vec![IoCapability::FileCreate, IoCapability::FileDelete];
    let mut declaration = io::declaration(caps.clone(), vec!["mutation.unused".into()]);
    let ordinary = declaration.budget.as_mut().unwrap();
    ordinary.max_jobs = 4;
    ordinary.max_resources = 8;
    ordinary.max_job_bytes = 16 * 1024 * 1024;
    ordinary.max_bytes = ordinary.max_job_bytes;
    ordinary.max_duration_ms = 30_000;
    manifest.io_declaration = Some(declaration);
    manifest.required_features.extend([
        io::FEATURE.into(),
        MUTATION_FEATURE.into(),
        MUTATION_BUDGET_FEATURE.into(),
    ]);
    manifest.mutation_schema_sha256 = wire::schema_digest().to_vec();
    manifest.mutation_budget = Some(DeclaredBudget {
        max_job_bytes: BUDGET.max_job_bytes,
        max_bytes: BUDGET.max_bytes,
    });
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
        HostRuntime::new(Store::open(&dir.path().join("db"), Default::default()).unwrap()).unwrap(),
    );
    #[cfg(target_os = "windows")]
    let storage = crate::storage::Storage::open(&dir.path().join("db")).unwrap();
    let owner = WorkbenchState::with_manager(storage, Ok(manager), None).unwrap();
    Setup {
        app: Workbench {
            http_tasks: Default::default(),
            state: StateSlot::new(owner),
        },
        dir,
        digest: package.digest(),
        approved_budget: Some(BUDGET),
    }
}

#[test]
fn tight_ordinary_total_preserves_small_history_reads() {
    let _serial = serial_effects();
    let mut setup = tight_total_setup();
    let root = setup.dir.path().join("tight-total");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("still-absent.bin");
    let key = setup.start(
        160,
        Disposition::Create,
        root,
        Some(RelativeFilePath::parse("still-absent.bin").unwrap()),
    );
    selected(&mut setup.app, key);
    let body = b"small plan under tight ordinary total";
    let GuestMutationReply::Owner(MutationResponse::Planned(original)) = call(
        &mut setup.app,
        key,
        161,
        GuestAction::BuildPlan {
            operation_id: "tight-history".into(),
            content_length: body.len() as u64,
            content_sha256: Some(Sha256::digest(body).into()),
        },
    ) else {
        panic!("tight canonical plan");
    };
    let plan_sha256 = Sha256::digest(original.container()).into();
    assert!(matches!(
        call(
            &mut setup.app,
            key,
            162,
            GuestAction::Prepare { plan_sha256 }
        ),
        GuestMutationReply::Frame(_)
    ));
    assert!(matches!(
        call(&mut setup.app, key, 163, GuestAction::HostRelease),
        GuestMutationReply::Owner(MutationResponse::Released)
    ));
    stop(&mut setup.app, key);
    assert!(!leaf.exists());

    let request = ReconciliationStart {
        submission: [164; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(&mut setup),
        plan: original,
        timeout_ms: 30_000,
    };
    let key = setup
        .app
        .start_mutation_reconciliation_request(request)
        .unwrap();
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    let MutationResponse::Reconciled {
        record: Some(record),
        outcome: None,
    } = native_read(&mut setup.app, key, 1)
    else {
        panic!("tight ordinary budget history");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    stop(&mut setup.app, key);
    assert!(!leaf.exists());
}

#[test]
fn budgeted_guest_history_is_read_only_and_preserves_exact_original_plan() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("history-root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("must-not-exist.bin");
    let key = setup.start(
        101,
        Disposition::Create,
        root,
        Some(RelativeFilePath::parse("must-not-exist.bin").unwrap()),
    );
    selected(&mut setup.app, key);
    let body = b"prepared guest history";
    let GuestMutationReply::Owner(MutationResponse::Planned(original)) = call(
        &mut setup.app,
        key,
        102,
        GuestAction::BuildPlan {
            operation_id: "budget-history".into(),
            content_length: body.len() as u64,
            content_sha256: Some(Sha256::digest(body).into()),
        },
    ) else {
        panic!("canonical plan");
    };
    let plan_sha256 = Sha256::digest(original.container()).into();
    assert!(matches!(
        call(
            &mut setup.app,
            key,
            103,
            GuestAction::Prepare { plan_sha256 }
        ),
        GuestMutationReply::Frame(_)
    ));
    assert!(matches!(
        call(&mut setup.app, key, 104, GuestAction::HostRelease),
        GuestMutationReply::Owner(MutationResponse::Released)
    ));
    stop(&mut setup.app, key);
    assert!(!leaf.exists());

    let other_scope = discover(&mut setup, 108, "other-subject");
    let key = setup
        .app
        .start_mutation_discovery_request(other_scope)
        .unwrap();
    let MutationResponse::Plans { plans, .. } = native_read(&mut setup.app, key, 1) else {
        panic!("scoped plans");
    };
    assert!(
        plans.is_empty(),
        "another subject cannot see the saved guest plan"
    );
    close_discovery(&mut setup.app, key);

    let request = discover(&mut setup, 105, SUBJECT);
    let key = setup.app.start_mutation_discovery_request(request).unwrap();
    assert!(!setup.app.mutation_status(key).unwrap().selected);
    let MutationResponse::Plans { plans, .. } = native_read(&mut setup.app, key, 1) else {
        panic!("plans");
    };
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].container(), original.container());
    for action in [
        Action::Query,
        Action::Execute,
        Action::Prepare(Box::new(original.clone())),
        Action::CancelPlan,
    ] {
        assert!(setup.app.request_mutation(key, action).is_err());
    }
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [106; 32], GuestAction::HostQuery)
            .is_err()
    );
    close_discovery(&mut setup.app, key);

    let request = ReconciliationStart {
        submission: [107; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(&mut setup),
        plan: original.clone(),
        timeout_ms: 30_000,
    };
    let key = setup
        .app
        .start_mutation_reconciliation_request(request)
        .unwrap();
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    let MutationResponse::Reconciled {
        record: Some(record),
        outcome: None,
    } = native_read(&mut setup.app, key, 1)
    else {
        panic!("history");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    stop(&mut setup.app, key);
    assert!(!leaf.exists());

    let manager = setup
        .app
        .local_state_mut()
        .unwrap()
        .manager
        .as_mut()
        .unwrap();
    manager
        .approve_io(ID, setup.digest, BTreeSet::new(), manager.revision())
        .unwrap();
    let revoked = ReconciliationStart {
        submission: [109; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(&mut setup),
        plan: original,
        timeout_ms: 30_000,
    };
    assert!(
        setup
            .app
            .start_mutation_reconciliation_request(revoked)
            .is_err()
    );
    assert!(setup.app.io_status().key.is_none());
    assert!(!leaf.exists());
}

#[test]
fn budgeted_history_requires_current_authority_and_exact_scope() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let mut stale = discover(&mut setup, 111, SUBJECT);
    stale.revision = stale.revision.saturating_sub(1);
    assert!(setup.app.start_mutation_discovery_request(stale).is_err());
    assert!(setup.app.io_status().key.is_none());
    {
        let manager = setup
            .app
            .local_state_mut()
            .unwrap()
            .manager
            .as_mut()
            .unwrap();
        manager
            .approve_io(ID, setup.digest, BTreeSet::new(), manager.revision())
            .unwrap();
    }
    let denied = discover(&mut setup, 112, SUBJECT);
    assert!(setup.app.start_mutation_discovery_request(denied).is_err());
    assert!(setup.app.io_status().key.is_none());
    {
        let manager = setup
            .app
            .local_state_mut()
            .unwrap()
            .manager
            .as_mut()
            .unwrap();
        manager
            .approve_io(
                ID,
                setup.digest,
                BTreeSet::from([IoCapability::FileCreate]),
                manager.revision(),
            )
            .unwrap();
    }
    // A failed submission is burned, even after current approval changes.
    let reused = discover(&mut setup, 112, SUBJECT);
    assert!(setup.app.start_mutation_discovery_request(reused).is_err());
    let request = discover(&mut setup, 113, "other-subject");
    let key = setup.app.start_mutation_discovery_request(request).unwrap();
    let MutationResponse::Plans { plans, .. } = native_read(&mut setup.app, key, 1) else {
        panic!("plans");
    };
    assert!(plans.is_empty());
    close_discovery(&mut setup.app, key);
    // Read admission did not make the old direct native write route legal.
    let request = setup.request(
        114,
        Disposition::Create,
        setup.dir.path().to_owned(),
        Some(RelativeFilePath::parse("no-fallback.bin").unwrap()),
    );
    assert!(setup.app.start_selected_mutation(request).is_err());
    assert!(!setup.dir.path().join("no-fallback.bin").exists());
}

#[test]
fn maximum_guest_create_reconciles_with_read_only_history_budget() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let root = setup.dir.path().join("maximum-history");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("maximum.bin");
    let mut body = vec![0_u8; 16 * 1024 * 1024];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    for byte in &mut body {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    let body_sha256: [u8; 32] = Sha256::digest(&body).into();
    let key = setup.start(
        150,
        Disposition::Create,
        root,
        Some(RelativeFilePath::parse("maximum.bin").unwrap()),
    );
    selected(&mut setup.app, key);
    let GuestMutationReply::Owner(MutationResponse::Planned(original)) = call(
        &mut setup.app,
        key,
        151,
        GuestAction::BuildPlan {
            operation_id: "maximum-history".into(),
            content_length: body.len() as u64,
            content_sha256: Some(body_sha256),
        },
    ) else {
        panic!("canonical maximum plan");
    };
    let plan_sha256 = Sha256::digest(original.container()).into();
    let GuestMutationReply::Frame(prepared) = call(
        &mut setup.app,
        key,
        152,
        GuestAction::Prepare { plan_sha256 },
    ) else {
        panic!("maximum Prepare");
    };
    assert_eq!(prepared.phase, wire::Phase::Prepared);
    assert!(!leaf.exists());
    for (index, chunk) in body.chunks(wire::MAX_CHUNK_BYTES).enumerate() {
        let mut submission = [0xA5; 32];
        submission[..8].copy_from_slice(&(index as u64).to_le_bytes());
        let command = setup
            .app
            .submit_guest_mutation(
                key,
                submission,
                GuestAction::Chunk {
                    offset: (index * wire::MAX_CHUNK_BYTES) as u64,
                    bytes: Zeroizing::new(chunk.to_vec()),
                },
            )
            .unwrap();
        let GuestMutationReply::Frame(staged) = read(&mut setup.app, key, command).unwrap() else {
            panic!("maximum chunk {index}");
        };
        assert_eq!(
            staged.staged_bytes,
            (index * wire::MAX_CHUNK_BYTES + chunk.len()) as u64
        );
    }
    assert_eq!(body.chunks(wire::MAX_CHUNK_BYTES).count(), 274);
    let GuestMutationReply::Frame(committed) =
        call(&mut setup.app, key, 153, GuestAction::CommitContent)
    else {
        panic!("maximum Commit");
    };
    assert!(committed.durable_content);
    assert!(!leaf.exists());
    let GuestMutationReply::Frame(executed) = call(
        &mut setup.app,
        key,
        154,
        GuestAction::Execute { plan_sha256 },
    ) else {
        panic!("maximum Execute");
    };
    assert_eq!(
        (executed.phase, executed.effect),
        (wire::Phase::Observed, wire::Effect::OsSucceeded)
    );
    let actual_body_sha256: [u8; 32] = Sha256::digest(fs::read(&leaf).unwrap()).into();
    assert_eq!(actual_body_sha256, body_sha256);
    assert!(matches!(
        call(&mut setup.app, key, 155, GuestAction::Release),
        GuestMutationReply::Frame(_)
    ));
    stop(&mut setup.app, key);

    let request = ReconciliationStart {
        submission: [156; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(&mut setup),
        plan: original.clone(),
        timeout_ms: 30_000,
    };
    let key = setup
        .app
        .start_mutation_reconciliation_request(request)
        .unwrap();
    assert!(setup.app.request_mutation(key, Action::Execute).is_err());
    assert!(
        setup
            .app
            .submit_guest_mutation(key, [157; 32], GuestAction::HostQuery)
            .is_err()
    );
    let MutationResponse::Reconciled {
        record: Some(record),
        outcome: Some(outcome),
    } = native_read(&mut setup.app, key, 1)
    else {
        panic!("maximum observed history");
    };
    assert_eq!(record.phase(), Phase::Observed);
    let MutationOutcome::Created(created) = *outcome else {
        panic!("maximum Create outcome");
    };
    assert_eq!(
        created.result(),
        morrow_core::file_effect::CreateResult::Created
    );
    assert_eq!(created.content_sha256(), body_sha256);
    stop(&mut setup.app, key);
    let actual_body_sha256: [u8; 32] = Sha256::digest(fs::read(&leaf).unwrap()).into();
    assert_eq!(actual_body_sha256, body_sha256);

    let wrong_digest = ReconciliationStart {
        submission: [158; 32],
        package_id: ID.into(),
        digest: [0xDA; 32],
        revision: revision(&mut setup),
        plan: original.clone(),
        timeout_ms: 30_000,
    };
    assert!(
        setup
            .app
            .start_mutation_reconciliation_request(wrong_digest)
            .is_err()
    );
    assert!(setup.app.io_status().key.is_none());
    let manager = setup
        .app
        .local_state_mut()
        .unwrap()
        .manager
        .as_mut()
        .unwrap();
    manager
        .approve_io(ID, setup.digest, BTreeSet::new(), manager.revision())
        .unwrap();
    let revoked = ReconciliationStart {
        submission: [159; 32],
        package_id: ID.into(),
        digest: setup.digest,
        revision: revision(&mut setup),
        plan: original,
        timeout_ms: 30_000,
    };
    assert!(
        setup
            .app
            .start_mutation_reconciliation_request(revoked)
            .is_err()
    );
    assert!(setup.app.io_status().key.is_none());
    let actual_body_sha256: [u8; 32] = Sha256::digest(fs::read(&leaf).unwrap()).into();
    assert_eq!(actual_body_sha256, body_sha256);
}
