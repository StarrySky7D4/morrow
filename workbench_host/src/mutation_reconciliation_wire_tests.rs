//! Private wire admission for read-only mutation history tasks.
use super::*;
use morrow_core::file_effect::CreateOutcome;

fn reconcile_start(
    setup: &mut Setup,
    revision: u64,
    submission: &[u8; 32],
    digest: &[u8; 32],
    request: &RequestRecord,
) -> Reader<OwnedSegments> {
    wire_call(
        &mut setup.app,
        wire::Action::MutationReconcile,
        |mut request_wire| {
            let mut start = request_wire.reborrow().init_mutation_reconcile();
            start.set_submission(submission);
            start.set_package_id(ID);
            start.set_package_digest(digest);
            start.set_registry_revision(revision);
            start.set_plan(request.container());
            start.set_timeout_ms(10_000);
        },
    )
}

fn key(message: &Reader<OwnedSegments>) -> Vec<u8> {
    assert_ok(message);
    row(message)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec()
}

fn stop_and_ack(setup: &mut Setup, bytes: &[u8]) {
    let task = TaskKey::from_bytes(bytes).unwrap();
    setup.app.cancel_io(task).unwrap();
    reclaim(&mut setup.app, task);
    setup.app.acknowledge_io(task).unwrap();
}

#[test]
fn private_wire_reconciles_actual_create_without_reopening_target() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("wire-reconcile-root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("created.bin");
    let bytes = b"one durable effect, one historical answer";
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let original = start(
        &mut setup,
        revision,
        &[41; 32],
        Disposition::Create,
        &root,
        "created.bin",
    );
    let original_key = key(&original);
    let selected = read_result(&mut setup.app, &original_key, 1);
    let reference: [u8; 32] = row(&selected)
        .get_mutation_result()
        .unwrap()
        .get_reference()
        .unwrap()
        .try_into()
        .unwrap();
    let request = plan(
        digest,
        "wire-reconciled-create",
        Disposition::Create,
        reference,
        Some(RelativeFilePath::parse("created.bin").unwrap()),
        None,
        bytes,
    );
    let prepare = submit(
        &mut setup.app,
        &original_key,
        &[42; 32],
        1,
        request.container(),
        0,
        &[],
    );
    assert_eq!(
        row(&read_result(
            &mut setup.app,
            &original_key,
            command_id(&prepare)
        ))
        .get_mutation_result()
        .unwrap()
        .get_kind(),
        2
    );
    for (token, kind, chunk) in [
        ([43; 32], 2, &bytes[..]),
        ([44; 32], 3, &[][..]),
        ([45; 32], 4, &[][..]),
    ] {
        let submitted = submit(&mut setup.app, &original_key, &token, kind, &[], 0, chunk);
        let result = read_result(&mut setup.app, &original_key, command_id(&submitted));
        assert_ne!(row(&result).get_mutation_result().unwrap().get_kind(), 9);
    }
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    let query = submit(&mut setup.app, &original_key, &[46; 32], 5, &[], 0, &[]);
    let observed = read_result(&mut setup.app, &original_key, command_id(&query));
    assert_eq!(
        Record::decode(
            row(&observed)
                .get_mutation_result()
                .unwrap()
                .get_record()
                .unwrap()
        )
        .unwrap()
        .phase(),
        Phase::Observed
    );

    // A joined original still owns the scheduler slot until its acknowledgement.
    let original_task = TaskKey::from_bytes(&original_key).unwrap();
    setup.app.cancel_io(original_task).unwrap();
    reclaim(&mut setup.app, original_task);
    let premature = reconcile_start(&mut setup, revision, &[47; 32], &digest, &request);
    assert!(!row(&premature).get_error().unwrap().is_empty());
    assert_eq!(setup.app.io_status().key, Some(original_task));
    setup.app.acknowledge_io(original_task).unwrap();
    fs::remove_file(&leaf).unwrap();
    fs::remove_dir(&root).unwrap();

    let started = reconcile_start(&mut setup, revision, &[48; 32], &digest, &request);
    let history_key = key(&started);
    assert_eq!(command_id(&started), 1);
    let state = row(&started).get_mutation_state().unwrap();
    assert_eq!(state.get_kind(), 8);
    assert!(!state.get_selected());
    assert!(state.get_reference().unwrap().is_empty());
    assert!(state.get_expected_identity().unwrap().is_empty());
    assert!(!root.exists());

    let duplicate = reconcile_start(&mut setup, revision, &[48; 32], &digest, &request);
    assert_eq!(key(&duplicate), history_key);
    assert_eq!(command_id(&duplicate), 1);
    let altered = plan(
        digest,
        "wire-reconciled-create",
        Disposition::Create,
        reference,
        Some(RelativeFilePath::parse("created.bin").unwrap()),
        None,
        b"different content",
    );
    let conflicting = reconcile_start(&mut setup, revision, &[48; 32], &digest, &altered);
    assert!(!row(&conflicting).get_error().unwrap().is_empty());
    assert_eq!(
        setup
            .app
            .mutation_status(TaskKey::from_bytes(&history_key).unwrap())
            .unwrap()
            .command,
        1
    );

    let answer = read_result(&mut setup.app, &history_key, 1);
    export_wire_fixture("reconciled", &answer);
    let result = row(&answer).get_mutation_result().unwrap();
    assert_eq!(result.get_kind(), 10);
    assert_eq!(result.get_phase(), 3);
    assert_eq!(result.get_effect(), 1);
    assert_eq!(result.get_os_code(), 0);
    assert_eq!(
        result.get_operation_id().unwrap().to_str().unwrap(),
        "wire-reconciled-create"
    );
    assert_eq!(
        Record::decode(result.get_record().unwrap())
            .unwrap()
            .phase(),
        Phase::Observed
    );
    let outcome = CreateOutcome::decode(result.get_outcome().unwrap()).unwrap();
    assert_eq!(outcome.operation_id(), "wire-reconciled-create");
    assert!(matches!(outcome.result(), CreateResult::Created));
    assert!(!root.exists(), "history must not reopen or recreate target");

    let after_read = reconcile_start(&mut setup, revision, &[48; 32], &digest, &request);
    assert_eq!(key(&after_read), history_key);
    assert_eq!(command_id(&after_read), 1);
    assert_eq!(
        row(&after_read)
            .get_mutation_state()
            .unwrap()
            .get_delivery(),
        2
    );
    let forbidden = submit(&mut setup.app, &history_key, &[49; 32], 5, &[], 0, &[]);
    assert!(!row(&forbidden).get_error().unwrap().is_empty());
    assert_eq!(
        setup
            .app
            .mutation_status(TaskKey::from_bytes(&history_key).unwrap())
            .unwrap()
            .command,
        1
    );
    stop_and_ack(&mut setup, &history_key);
    let wrong_plan = reconcile_start(&mut setup, revision, &[50; 32], &digest, &altered);
    let wrong_key = key(&wrong_plan);
    let wrong_result = read_result(&mut setup.app, &wrong_key, 1);
    assert_eq!(
        row(&wrong_result).get_mutation_result().unwrap().get_kind(),
        9
    );
    assert!(!root.exists());
    stop_and_ack(&mut setup, &wrong_key);
    let stale = reconcile_start(&mut setup, revision, &[48; 32], &digest, &request);
    assert_eq!(row(&stale).get_ui_code(), 113);
    assert!(!row(&stale).has_mutation_state());
    assert!(!root.exists());
}

#[test]
fn private_wire_reconcile_rejects_mixed_fields_and_bad_authority() {
    let mut setup = Setup::new("rust");
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let request = plan(
        digest,
        "wire-reconcile-invalid",
        Disposition::Create,
        [91; 32],
        Some(RelativeFilePath::parse("missing.bin").unwrap()),
        None,
        b"missing",
    );
    let mixed = wire_call(
        &mut setup.app,
        wire::Action::MutationReconcile,
        |mut outer| {
            outer.set_io_key(&[1; 32]);
            let mut start = outer.reborrow().init_mutation_reconcile();
            start.set_submission(&[51; 32]);
            start.set_package_id(ID);
            start.set_package_digest(&digest);
            start.set_registry_revision(revision);
            start.set_plan(request.container());
            start.set_timeout_ms(10_000);
        },
    );
    assert!(!row(&mixed).get_error().unwrap().is_empty());
    assert!(!row(&mixed).has_mutation_state());
    for extra in 0..3 {
        let mixed = wire_call(
            &mut setup.app,
            wire::Action::MutationReconcile,
            |mut outer| {
                match extra {
                    0 => outer.set_mutation_command_id(1),
                    1 => outer.reborrow().init_mutation_start().set_disposition(1),
                    _ => outer.reborrow().init_mutation_command().set_kind(5),
                }
                let mut start = outer.reborrow().init_mutation_reconcile();
                start.set_submission(&[51; 32]);
                start.set_package_id(ID);
                start.set_package_digest(&digest);
                start.set_registry_revision(revision);
                start.set_plan(request.container());
                start.set_timeout_ms(10_000);
            },
        );
        assert!(!row(&mixed).get_error().unwrap().is_empty());
        assert!(!row(&mixed).has_mutation_state());
    }
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);

    let wrong_digest = reconcile_start(&mut setup, revision, &[52; 32], &[92; 32], &request);
    assert!(!row(&wrong_digest).get_error().unwrap().is_empty());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    let burned_digest = reconcile_start(&mut setup, revision, &[52; 32], &digest, &request);
    assert!(!row(&burned_digest).get_error().unwrap().is_empty());
    assert!(!row(&burned_digest).has_mutation_state());
    let wrong_revision = reconcile_start(&mut setup, revision + 1, &[53; 32], &digest, &request);
    assert!(!row(&wrong_revision).get_error().unwrap().is_empty());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    let burned_revision = reconcile_start(&mut setup, revision, &[53; 32], &digest, &request);
    assert!(!row(&burned_revision).get_error().unwrap().is_empty());
    assert!(!row(&burned_revision).has_mutation_state());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
}

#[test]
fn private_wire_reconcile_absent_history_has_no_target_selection() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let absent_root = setup.dir.path().join("never-selected");
    let request = plan(
        digest,
        "wire-reconcile-absent",
        Disposition::Create,
        [93; 32],
        Some(RelativeFilePath::parse("missing.bin").unwrap()),
        None,
        b"absent",
    );
    let started = reconcile_start(&mut setup, revision, &[54; 32], &digest, &request);
    let history_key = key(&started);
    assert!(!row(&started).get_mutation_state().unwrap().get_selected());
    let absent = read_result(&mut setup.app, &history_key, 1);
    assert_eq!(row(&absent).get_mutation_result().unwrap().get_kind(), 10);
    assert_eq!(row(&absent).get_mutation_result().unwrap().get_phase(), 0);
    assert!(
        row(&absent)
            .get_mutation_result()
            .unwrap()
            .get_record()
            .unwrap()
            .is_empty()
    );
    assert!(
        row(&absent)
            .get_mutation_result()
            .unwrap()
            .get_outcome()
            .unwrap()
            .is_empty()
    );
    assert!(!absent_root.exists());
    stop_and_ack(&mut setup, &history_key);
}

#[test]
fn private_wire_reconcile_requires_outer_scheduler() {
    let mut setup = Setup::new("rust");
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::MutationReconcile);
    let frame = serialize::write_message_to_words(&message);
    let reply = protocol::respond_state(setup.app.local_state_mut().unwrap(), &frame).unwrap();
    let response = serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap();
    let response = row(&response);
    assert!(
        response
            .get_error()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("scheduler actions require the outer application")
    );
    assert!(!response.has_mutation_state());
    assert!(!response.has_mutation_result());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
}
