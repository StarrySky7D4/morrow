//! Fresh, read-only mutation history tasks on the original protected store.
use super::*;
use morrow_core::file_effect::CreateResult;
use morrow_plugin_runtime::io_jobs::MutationOutcome;
use std::path::PathBuf;

fn create_plan(
    setup: &mut Setup,
    operation: &str,
    bytes: &[u8],
) -> (RequestRecord, PathBuf, PathBuf, TaskKey) {
    let root = setup.dir.path().join("reconcile-root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("created.bin");
    let relative = RelativeFilePath::parse("created.bin").unwrap();
    let key = setup
        .app
        .start_mutation(
            setup.options(Disposition::Create),
            Selection::Create {
                root: root.clone(),
                relative: relative.clone(),
            },
            scope(Disposition::Create),
        )
        .unwrap();
    let MutationResponse::Selected {
        reference,
        expected_identity: None,
    } = read(&mut setup.app, key, 1).unwrap()
    else {
        panic!("create selection");
    };
    let request = plan(
        setup.digest,
        operation,
        Disposition::Create,
        reference,
        Some(relative),
        None,
        bytes,
    );
    let MutationResponse::Prepared(record) = call(
        &mut setup.app,
        key,
        Action::Prepare(Box::new(request.clone())),
    )
    .unwrap() else {
        panic!("prepared create");
    };
    assert_eq!(record.phase(), Phase::Prepared);
    (request, root, leaf, key)
}

fn finish_original(setup: &mut Setup, key: TaskKey, request: &RequestRecord) {
    setup.app.cancel_io(key).unwrap();
    reclaim(&mut setup.app, key);
    assert!(
        setup
            .app
            .start_mutation_reconciliation(
                setup.options(request.request().disposition),
                request.clone(),
            )
            .is_err()
    );
    assert_eq!(setup.app.io_status().key, Some(key));
    setup.app.acknowledge_io(key).unwrap();
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
}

fn reconcile(setup: &mut Setup, request: RequestRecord) -> MutationResponse {
    let key = setup
        .app
        .start_mutation_reconciliation(setup.options(request.request().disposition), request)
        .unwrap();
    let snapshot = setup.app.mutation_status(key).unwrap();
    assert_eq!(snapshot.command, 1);
    assert_eq!(snapshot.kind, CommandKind::Reconcile);
    assert!(!snapshot.selected);
    assert!(snapshot.selection.is_none());
    assert!(setup.app.local_state().is_err());
    let result = read(&mut setup.app, key, 1).unwrap();
    let MutationResponse::Reconciled { record, .. } = &result else {
        panic!("history-only task result");
    };
    let phase = record.as_ref().map(|value| value.phase());
    let snapshot = setup.app.mutation_status(key).unwrap();
    assert!(snapshot.selection.is_none());
    assert_eq!(
        snapshot.reconcile_required,
        phase == Some(Phase::OutcomeUnknown)
    );
    assert_eq!(
        snapshot.terminal,
        matches!(
            phase,
            Some(Phase::Observed | Phase::CancelledBeforeDispatch)
        )
    );
    // A history task has no retained target session. Even read-only mutation
    // actions cannot be used to renew its authority or replay an effect.
    for action in [
        Action::Prepare(Box::new(plan(
            setup.digest,
            "irrelevant-plan",
            Disposition::Create,
            [91; 32],
            Some(RelativeFilePath::parse("created.bin").unwrap()),
            None,
            b"x",
        ))),
        Action::Chunk {
            offset: 0,
            bytes: vec![1].into(),
        },
        Action::CommitContent,
        Action::Execute,
        Action::Query,
        Action::CancelPlan,
        Action::Release,
    ] {
        assert!(setup.app.request_mutation(key, action).is_err());
    }
    reclaim(&mut setup.app, key);
    setup.app.acknowledge_io(key).unwrap();
    result
}

#[test]
fn fresh_history_recovers_observed_create_after_target_disappears() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original_binding = setup.app.local_state().unwrap().host.binding();
    let bytes = b"one durable create, one historical answer";
    let (request, root, leaf, key) = create_plan(&mut setup, "fresh-observed", bytes);
    call(
        &mut setup.app,
        key,
        Action::Chunk {
            offset: 0,
            bytes: bytes.to_vec().into(),
        },
    )
    .unwrap();
    call(&mut setup.app, key, Action::CommitContent).unwrap();
    let MutationResponse::Created(created) = call(&mut setup.app, key, Action::Execute).unwrap()
    else {
        panic!("created effect");
    };
    assert_eq!(created.result(), CreateResult::Created);
    let MutationResponse::History {
        record: Some(original_record),
        ..
    } = call(&mut setup.app, key, Action::Query).unwrap()
    else {
        panic!("observed original history");
    };
    assert_eq!(original_record.phase(), Phase::Observed);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    finish_original(&mut setup, key, &request);
    assert_eq!(
        setup.app.local_state().unwrap().host.binding(),
        original_binding
    );

    fs::remove_file(&leaf).unwrap();
    fs::remove_dir(&root).unwrap();
    let MutationResponse::Reconciled {
        record: Some(recovered),
        outcome: Some(outcome),
    } = reconcile(&mut setup, request)
    else {
        panic!("observed create and its actual effect");
    };
    let MutationOutcome::Created(actual) = *outcome else {
        panic!("create outcome");
    };
    assert_eq!(recovered.phase(), Phase::Observed);
    assert_eq!(recovered.digest(), original_record.digest());
    assert_eq!(actual.result(), CreateResult::Created);
    assert!(!root.exists(), "history must not reopen or recreate target");
    assert_eq!(
        setup.app.local_state().unwrap().host.binding(),
        original_binding
    );
}

#[test]
fn fresh_history_reports_prepared_without_dispatch_or_target_selection() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original_binding = setup.app.local_state().unwrap().host.binding();
    let (request, root, leaf, key) = create_plan(&mut setup, "fresh-prepared", b"pending");
    let MutationResponse::History {
        record: Some(original_record),
        ..
    } = call(&mut setup.app, key, Action::Query).unwrap()
    else {
        panic!("prepared original history");
    };
    assert_eq!(original_record.phase(), Phase::Prepared);
    assert!(!leaf.exists());
    finish_original(&mut setup, key, &request);
    fs::remove_dir(&root).unwrap();

    let MutationResponse::Reconciled {
        record: Some(recovered),
        outcome: None,
    } = reconcile(&mut setup, request)
    else {
        panic!("prepared plan without effect");
    };
    assert_eq!(recovered.phase(), Phase::Prepared);
    assert_eq!(recovered.digest(), original_record.digest());
    assert!(!root.exists());
    assert!(!leaf.exists());
    assert_eq!(
        setup.app.local_state().unwrap().host.binding(),
        original_binding
    );
}

#[test]
fn fresh_history_returns_absent_for_a_never_selected_plan() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original_binding = setup.app.local_state().unwrap().host.binding();
    let absent_root = setup.dir.path().join("never-selected-root");
    let request = plan(
        setup.digest,
        "fresh-absent",
        Disposition::Create,
        [93; 32],
        Some(RelativeFilePath::parse("created.bin").unwrap()),
        None,
        b"absent",
    );
    assert!(!absent_root.exists());
    let MutationResponse::Reconciled {
        record: None,
        outcome: None,
    } = reconcile(&mut setup, request)
    else {
        panic!("absent history must be explicit");
    };
    assert!(!absent_root.exists());
    assert_eq!(
        setup.app.local_state().unwrap().host.binding(),
        original_binding
    );
}

#[test]
fn fresh_history_reports_cancelled_plan_without_a_target() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let (request, root, leaf, key) = create_plan(&mut setup, "fresh-cancelled", b"cancelled");
    let MutationResponse::PlanCancelled(original_record) =
        call(&mut setup.app, key, Action::CancelPlan).unwrap()
    else {
        panic!("cancelled plan");
    };
    assert_eq!(original_record.phase(), Phase::CancelledBeforeDispatch);
    finish_original(&mut setup, key, &request);
    fs::remove_dir(&root).unwrap();

    let MutationResponse::Reconciled {
        record: Some(recovered),
        outcome: None,
    } = reconcile(&mut setup, request)
    else {
        panic!("cancelled historical plan without effect");
    };
    assert_eq!(recovered.phase(), Phase::CancelledBeforeDispatch);
    assert_eq!(recovered.digest(), original_record.digest());
    assert!(!root.exists());
    assert!(!leaf.exists());
}

#[test]
fn fresh_history_keeps_claimed_but_unobserved_create_unknown() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let (request, root, leaf, key) = create_plan(&mut setup, "fresh-unknown", b"staged");
    call(
        &mut setup.app,
        key,
        Action::Chunk {
            offset: 0,
            bytes: b"staged".to_vec().into(),
        },
    )
    .unwrap();
    call(&mut setup.app, key, Action::CommitContent).unwrap();
    let MutationResponse::History {
        record: Some(prepared),
        durable_content: true,
        ..
    } = call(&mut setup.app, key, Action::Query).unwrap()
    else {
        panic!("durable prepared history");
    };
    assert_eq!(prepared.phase(), Phase::Prepared);
    finish_original(&mut setup, key, &request);

    // Simulate the durable pre-effect claim from the original operation. No
    // target is opened or changed, so the historical answer stays uncertain.
    let unknown = setup
        .app
        .local_state_mut()
        .unwrap()
        .host
        .store_local_mut()
        .claim_file_create_local_authorized(&prepared.propose_dispatch_boundary().unwrap(), || {
            Ok(())
        })
        .unwrap();
    assert_eq!(unknown.phase(), Phase::OutcomeUnknown);
    fs::remove_dir(&root).unwrap();
    let MutationResponse::Reconciled {
        record: Some(recovered),
        outcome: None,
    } = reconcile(&mut setup, request)
    else {
        panic!("unknown effect must not be invented or replayed");
    };
    assert_eq!(recovered.phase(), Phase::OutcomeUnknown);
    assert_eq!(recovered.digest(), unknown.digest());
    assert!(!root.exists());
    assert!(!leaf.exists());
}

#[test]
fn fresh_history_rejects_wrong_plan_digest_revision_and_revoked_permission() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let original_binding = setup.app.local_state().unwrap().host.binding();
    let (request, root, leaf, key) = create_plan(&mut setup, "fresh-exact-plan", b"original");
    finish_original(&mut setup, key, &request);
    fs::remove_dir(&root).unwrap();

    let wrong_plan = plan(
        setup.digest,
        "fresh-exact-plan",
        Disposition::Create,
        request.request().target.reference,
        request.request().target.relative_path.clone(),
        None,
        b"different",
    );
    let wrong_key = setup
        .app
        .start_mutation_reconciliation(setup.options(Disposition::Create), wrong_plan)
        .unwrap();
    assert!(
        setup
            .app
            .mutation_status(wrong_key)
            .unwrap()
            .selection
            .is_none()
    );
    assert!(read(&mut setup.app, wrong_key, 1).is_err());
    reclaim(&mut setup.app, wrong_key);
    setup.app.acknowledge_io(wrong_key).unwrap();

    let mut wrong_digest_request = request.request().clone();
    wrong_digest_request.package_sha256 = [92; 32];
    let wrong_digest = RequestRecord::new(wrong_digest_request).unwrap();
    assert!(
        setup
            .app
            .start_mutation_reconciliation(setup.options(Disposition::Create), wrong_digest)
            .is_err()
    );
    let mut stale = setup.options(Disposition::Create);
    stale.revision += 1;
    assert!(
        setup
            .app
            .start_mutation_reconciliation(stale, request.clone())
            .is_err()
    );

    let manager = setup
        .app
        .local_state_mut()
        .unwrap()
        .manager
        .as_mut()
        .unwrap();
    manager
        .set_enabled(ID, setup.digest, false, manager.revision())
        .unwrap();
    assert!(
        setup
            .app
            .start_mutation_reconciliation(setup.options(Disposition::Create), request)
            .is_err()
    );
    assert_eq!(
        setup.app.local_state().unwrap().host.binding(),
        original_binding
    );
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert!(!root.exists());
    assert!(!leaf.exists());
}
