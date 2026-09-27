//! Private wire coverage for bounded, read-only original mutation plans.
use super::*;

fn discover_start(
    setup: &mut Setup,
    submission: &[u8; 32],
    digest: &[u8; 32],
    revision: u64,
    subject: &str,
    scan_limit: u16,
) -> Reader<OwnedSegments> {
    wire_call(
        &mut setup.app,
        wire::Action::MutationDiscover,
        |mut request| {
            let mut start = request.reborrow().init_mutation_discover();
            start.set_submission(submission);
            start.set_package_id(ID);
            start.set_package_digest(digest);
            start.set_registry_revision(revision);
            start.set_subject(subject);
            start.set_disposition(1);
            start.set_scan_limit(scan_limit);
            start.set_timeout_ms(10_000);
        },
    )
}

fn discover_next(
    app: &mut Workbench,
    key: &[u8],
    submission: &[u8; 32],
    scan_limit: u16,
) -> Reader<OwnedSegments> {
    wire_call(app, wire::Action::MutationSubmit, |mut request| {
        request.set_io_key(key);
        let mut command = request.reborrow().init_mutation_command();
        command.set_submission(submission);
        command.set_kind(10);
        command.set_scan_limit(scan_limit);
    })
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

fn durable_create_plan(
    setup: &mut Setup,
    root: &Path,
    name: &str,
    operation: &str,
    token: u8,
) -> RequestRecord {
    let revision = setup.options(Disposition::Create).revision;
    let started = start(
        setup,
        revision,
        &[token; 32],
        Disposition::Create,
        root,
        name,
    );
    let key = key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    let selected = read_result(&mut setup.app, &key, 1);
    let reference: [u8; 32] = row(&selected)
        .get_mutation_result()
        .unwrap()
        .get_reference()
        .unwrap()
        .try_into()
        .unwrap();
    let plan = plan(
        setup.digest,
        operation,
        Disposition::Create,
        reference,
        Some(RelativeFilePath::parse(name).unwrap()),
        None,
        b"",
    );
    let submitted = submit(
        &mut setup.app,
        &key,
        &[token + 1; 32],
        1,
        plan.container(),
        0,
        &[],
    );
    assert_eq!(
        row(&read_result(&mut setup.app, &key, command_id(&submitted)))
            .get_mutation_result()
            .unwrap()
            .get_kind(),
        2
    );
    close(setup, &key, task, &[token + 2; 32]);
    assert!(!root.join(name).exists());
    plan
}

#[test]
fn private_discovery_pages_return_original_plans_with_dedup_and_explicit_release() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("discovery-wire-root");
    fs::create_dir(&root).unwrap();
    let first_plan = durable_create_plan(&mut setup, &root, "a.bin", "wire-discover-a", 60);
    let second_plan = durable_create_plan(&mut setup, &root, "b.bin", "wire-discover-b", 64);
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let token = [70; 32];
    let started = discover_start(&mut setup, &token, &digest, revision, SUBJECT, 1);
    let discovery_key = key(&started);
    let task = TaskKey::from_bytes(&discovery_key).unwrap();
    assert_eq!(command_id(&started), 1);
    let state = row(&started).get_mutation_state().unwrap();
    assert_eq!(state.get_kind(), 10);
    assert!(!state.get_selected());
    assert!(state.get_reference().unwrap().is_empty());
    assert_eq!(
        key(&discover_start(
            &mut setup, &token, &digest, revision, SUBJECT, 1
        )),
        discovery_key
    );
    for (subject, scan_limit) in [("other.subject", 1), (SUBJECT, 2)] {
        let conflict = discover_start(&mut setup, &token, &digest, revision, subject, scan_limit);
        assert!(!row(&conflict).get_error().unwrap().is_empty());
    }
    let first = read_result(&mut setup.app, &discovery_key, 1);
    export_wire_fixture("plans", &first);
    let page = row(&first).get_mutation_result().unwrap();
    assert_eq!(page.get_kind(), 12);
    assert_eq!(page.get_scanned(), 1);
    assert!(!page.get_done());
    assert_eq!(page.get_plans().unwrap().len(), 1);
    assert_eq!(
        page.get_plans().unwrap().get(0).unwrap(),
        first_plan.container()
    );
    assert!(page.get_record().unwrap().is_empty());
    assert!(page.get_outcome().unwrap().is_empty());
    assert_eq!(
        key(&discover_start(
            &mut setup, &token, &digest, revision, SUBJECT, 1
        )),
        discovery_key
    );
    assert_eq!(
        row(&discover_start(
            &mut setup, &token, &digest, revision, SUBJECT, 1
        ))
        .get_mutation_state()
        .unwrap()
        .get_delivery(),
        2
    );

    for (token, kind, data) in [
        ([71; 32], 1, first_plan.container()),
        ([72; 32], 4, &[][..]),
        ([73; 32], 5, &[][..]),
    ] {
        let rejected = submit(&mut setup.app, &discovery_key, &token, kind, data, 0, &[]);
        assert!(!row(&rejected).get_error().unwrap().is_empty());
    }
    let mixed = submit(
        &mut setup.app,
        &discovery_key,
        &[74; 32],
        10,
        b"forged plan",
        0,
        &[],
    );
    assert!(!row(&mixed).get_error().unwrap().is_empty());
    let mixed_old_kind = wire_call(
        &mut setup.app,
        wire::Action::MutationSubmit,
        |mut request| {
            request.set_io_key(&discovery_key);
            let mut command = request.reborrow().init_mutation_command();
            command.set_submission(&[77; 32]);
            command.set_kind(5);
            command.set_scan_limit(1);
        },
    );
    assert!(!row(&mixed_old_kind).get_error().unwrap().is_empty());
    assert_eq!(setup.app.mutation_status(task).unwrap().command, 1);

    let next_token = [75; 32];
    let next = discover_next(&mut setup.app, &discovery_key, &next_token, 1);
    assert_eq!(command_id(&next), 2);
    assert_eq!(
        command_id(&discover_next(
            &mut setup.app,
            &discovery_key,
            &next_token,
            1
        )),
        2
    );
    assert!(
        !row(&discover_next(
            &mut setup.app,
            &discovery_key,
            &next_token,
            2
        ))
        .get_error()
        .unwrap()
        .is_empty()
    );
    let second = read_result(&mut setup.app, &discovery_key, 2);
    let page = row(&second).get_mutation_result().unwrap();
    assert_eq!(page.get_kind(), 12);
    assert_eq!(page.get_scanned(), 1);
    assert!(page.get_done());
    assert_eq!(page.get_plans().unwrap().len(), 1);
    assert_eq!(
        page.get_plans().unwrap().get(0).unwrap(),
        second_plan.container()
    );
    assert_eq!(
        command_id(&discover_next(
            &mut setup.app,
            &discovery_key,
            &next_token,
            1
        )),
        2
    );
    assert!(
        row(&discover_next(
            &mut setup.app,
            &discovery_key,
            &next_token,
            2
        ))
        .get_error()
        .unwrap()
        .len()
            > 0
    );
    assert!(setup.app.mutation_status(task).unwrap().terminal);
    close(&mut setup, &discovery_key, task, &[76; 32]);
    assert!(!root.join("a.bin").exists());
    assert!(!root.join("b.bin").exists());
}

#[test]
fn private_discovery_rejects_mixed_frames_and_burns_failed_admission_tokens() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let mixed = wire_call(
        &mut setup.app,
        wire::Action::MutationDiscover,
        |mut request| {
            request.set_io_key(&[9; 32]);
            let mut start = request.reborrow().init_mutation_discover();
            start.set_submission(&[80; 32]);
            start.set_package_id(ID);
            start.set_package_digest(&digest);
            start.set_registry_revision(revision);
            start.set_subject(SUBJECT);
            start.set_disposition(1);
            start.set_scan_limit(1);
            start.set_timeout_ms(10_000);
        },
    );
    assert!(!row(&mixed).get_error().unwrap().is_empty());
    let wrong_digest = [0xee; 32];
    let rejected = discover_start(&mut setup, &[81; 32], &wrong_digest, revision, SUBJECT, 1);
    assert!(!row(&rejected).get_error().unwrap().is_empty());
    let burned = discover_start(&mut setup, &[81; 32], &digest, revision, SUBJECT, 1);
    assert!(!row(&burned).get_error().unwrap().is_empty());
    let rejected = discover_start(&mut setup, &[82; 32], &digest, revision + 1, SUBJECT, 1);
    assert!(!row(&rejected).get_error().unwrap().is_empty());
    let burned = discover_start(&mut setup, &[82; 32], &digest, revision, SUBJECT, 1);
    assert!(!row(&burned).get_error().unwrap().is_empty());
    let valid = discover_start(&mut setup, &[83; 32], &digest, revision, SUBJECT, 1);
    let key = key(&valid);
    let task = TaskKey::from_bytes(&key).unwrap();
    let empty = read_result(&mut setup.app, &key, 1);
    assert_eq!(row(&empty).get_mutation_result().unwrap().get_kind(), 12);
    assert!(row(&empty).get_mutation_result().unwrap().get_done());
    close(&mut setup, &key, task, &[84; 32]);
}

#[test]
fn private_discovery_action_is_rejected_by_nested_business_state() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(wire::Action::MutationDiscover);
    let frame = serialize::write_message_to_words(&message);
    let reply = protocol::respond_state(setup.app.local_state_mut().unwrap(), &frame).unwrap();
    let reply = serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap();
    assert!(
        row(&reply)
            .get_error()
            .unwrap()
            .to_str()
            .unwrap()
            .contains("scheduler actions require the outer application")
    );
    assert!(!row(&reply).has_mutation_state());
    assert!(!row(&reply).has_mutation_result());
}

#[test]
fn private_next_plans_cannot_run_on_a_selected_mutation_task() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("selected-not-discovery");
    fs::create_dir(&root).unwrap();
    let revision = setup.options(Disposition::Create).revision;
    let started = start(
        &mut setup,
        revision,
        &[86; 32],
        Disposition::Create,
        &root,
        "not-created.bin",
    );
    let key = key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    read_result(&mut setup.app, &key, 1);
    let rejected = discover_next(&mut setup.app, &key, &[87; 32], 1);
    assert!(!row(&rejected).get_error().unwrap().is_empty());
    assert_eq!(setup.app.mutation_status(task).unwrap().command, 1);
    close(&mut setup, &key, task, &[88; 32]);
    assert!(!root.join("not-created.bin").exists());
}

fn resume_start(
    setup: &mut Setup,
    submission: u8,
    checkpoint: &[u8],
    subject: &str,
) -> Reader<OwnedSegments> {
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    wire_call(
        &mut setup.app,
        wire::Action::MutationDiscover,
        |mut request| {
            let mut start = request.reborrow().init_mutation_discover();
            start.set_submission(&[submission; 32]);
            start.set_package_id(ID);
            start.set_package_digest(&digest);
            start.set_registry_revision(revision);
            start.set_subject(subject);
            start.set_disposition(1);
            start.set_scan_limit(1);
            start.set_timeout_ms(10_000);
            start.set_checkpoint(checkpoint);
        },
    )
}

#[test]
fn private_checkpoint_continues_after_join_ack_and_rejects_other_scopes_or_tokens() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("checkpoint-root");
    fs::create_dir(&root).unwrap();
    let first = durable_create_plan(&mut setup, &root, "first", "a-checkpoint", 101);
    let second = durable_create_plan(&mut setup, &root, "second", "b-checkpoint", 108);
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let started = discover_start(&mut setup, &[120; 32], &digest, revision, SUBJECT, 1);
    let discovery_key = key(&started);
    let task = TaskKey::from_bytes(&discovery_key).unwrap();
    assert!(setup.app.state.mutation_checkpoints.is_empty());
    let page = read_result(&mut setup.app, &discovery_key, 1);
    let result = row(&page).get_mutation_result().unwrap();
    assert_eq!(
        result.get_plans().unwrap().get(0).unwrap(),
        first.container()
    );
    assert!(!result.get_done());
    let checkpoint = result.get_checkpoint().unwrap().to_vec();
    assert_eq!(checkpoint.len(), 32);
    assert_eq!(setup.app.state.mutation_checkpoints.len(), 1);
    close(&mut setup, &discovery_key, task, &[121; 32]);

    let mut tampered = checkpoint.clone();
    tampered[0] ^= 1;
    assert!(
        !row(&resume_start(&mut setup, 122, &tampered, SUBJECT))
            .get_error()
            .unwrap()
            .is_empty()
    );
    // A failed admission's token cannot be changed into a successful resume.
    assert!(
        !row(&resume_start(&mut setup, 122, &checkpoint, SUBJECT))
            .get_error()
            .unwrap()
            .is_empty()
    );
    let wrong_scope = resume_start(&mut setup, 123, &checkpoint, "other.subject");
    let wrong_key = key(&wrong_scope);
    let wrong_task = TaskKey::from_bytes(&wrong_key).unwrap();
    let rejected = read_result(&mut setup.app, &wrong_key, 1);
    assert_eq!(row(&rejected).get_mutation_result().unwrap().get_kind(), 9);
    setup.app.cancel_io(wrong_task).unwrap();
    reclaim(&mut setup.app, wrong_task);
    setup.app.acknowledge_io(wrong_task).unwrap();

    let resumed = resume_start(&mut setup, 124, &checkpoint, SUBJECT);
    let resumed_key = key(&resumed);
    let resumed_task = TaskKey::from_bytes(&resumed_key).unwrap();
    let page = read_result(&mut setup.app, &resumed_key, 1);
    let result = row(&page).get_mutation_result().unwrap();
    assert_eq!(
        result.get_plans().unwrap().get(0).unwrap(),
        second.container()
    );
    assert!(result.get_done());
    assert!(result.get_checkpoint().unwrap().is_empty());
    close(&mut setup, &resumed_key, resumed_task, &[125; 32]);
    // The cache is bounded; expired positions fail instead of silently restarting.
    let saved = setup.app.state.mutation_checkpoints[0].1.clone();
    setup.app.state.mutation_checkpoints.clear();
    for value in 1u8..=64 {
        setup
            .app
            .state
            .mutation_checkpoints
            .push_back(([value; 32], saved.clone()));
    }
    let fresh = discover_start(&mut setup, &[127; 32], &digest, revision, SUBJECT, 1);
    let fresh_key = key(&fresh);
    let fresh_task = TaskKey::from_bytes(&fresh_key).unwrap();
    read_result(&mut setup.app, &fresh_key, 1);
    assert_eq!(setup.app.state.mutation_checkpoints.len(), 64);
    close(&mut setup, &fresh_key, fresh_task, &[128; 32]);
    assert!(
        !row(&resume_start(&mut setup, 129, &[1; 32], SUBJECT))
            .get_error()
            .unwrap()
            .is_empty()
    );
    let mut foreign = Setup::new("rust");
    assert!(
        !row(&resume_start(&mut foreign, 126, &checkpoint, SUBJECT))
            .get_error()
            .unwrap()
            .is_empty()
    );
    assert!(!root.join("first").exists());
    assert!(!root.join("second").exists());
}
