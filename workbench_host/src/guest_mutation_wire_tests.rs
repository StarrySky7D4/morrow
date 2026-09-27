//! The trusted private guest mutation wire must reach the original owner.
use super::*;
use crate::{host_capnp as host_wire, protocol};
use capnp::{
    message::{Builder, Reader, ReaderOptions},
    serialize::{self, OwnedSegments},
};
use morrow_core::mutation as guest_wire;
use std::path::Path;

fn private_call(
    app: &mut Workbench,
    action: host_wire::Action,
    configure: impl FnOnce(host_wire::request::Builder<'_>),
) -> Reader<OwnedSegments> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<host_wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    configure(request);
    let frame = serialize::write_message_to_words(&message);
    let reply = protocol::respond(app, &frame).unwrap();
    assert!(
        reply.len() <= 128 * 1024,
        "private reply exceeded frame bound"
    );
    serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap()
}

fn row<'a>(message: &'a Reader<OwnedSegments>) -> host_wire::response::Reader<'a> {
    message.get_root::<host_wire::response::Reader>().unwrap()
}

fn assert_ok(message: &Reader<OwnedSegments>) {
    assert_eq!(row(message).get_error().unwrap().to_str().unwrap(), "");
}

fn export(name: &str, message: &Reader<OwnedSegments>) {
    if let Some(dir) = std::env::var_os("MORROW_GUEST_MUTATION_WIRE_FIXTURES") {
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            Path::new(&dir).join(format!("rust-guest-{name}.bin")),
            serialize::write_message_segments_to_words(message.get_segments()),
        )
        .unwrap();
    }
}

fn revision(setup: &Setup) -> u64 {
    setup
        .app
        .local_state()
        .unwrap()
        .manager
        .as_ref()
        .unwrap()
        .revision()
}

fn start(
    setup: &mut Setup,
    revision: u64,
    token: u8,
    disposition: Disposition,
    selected_path: &Path,
    relative: &str,
    budget: Option<MutationBudget>,
) -> Reader<OwnedSegments> {
    let digest = setup.digest;
    private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationStart,
        |mut request| {
            let mut start = request.reborrow().init_guest_mutation_start();
            let mut selection = start.reborrow().init_selection();
            selection.set_submission(&[token; 32]);
            selection.set_package_id(ID);
            selection.set_package_digest(&digest);
            selection.set_registry_revision(revision);
            selection.set_disposition(match disposition {
                Disposition::Create => 1,
                Disposition::Replace => 2,
                Disposition::Delete => 3,
            });
            selection.set_selected_path(selected_path.to_str().unwrap());
            selection.set_relative_path(relative);
            selection.set_subject(SUBJECT);
            selection.set_approval_sha256(&APPROVAL);
            selection.set_timeout_ms(30_000);
            if let Some(budget) = budget {
                let mut approved = start.init_approved_budget();
                approved.set_max_job_bytes(budget.max_job_bytes);
                approved.set_max_bytes(budget.max_bytes);
            }
        },
    )
}

fn submit(
    app: &mut Workbench,
    key: &[u8],
    token: u8,
    kind: u16,
    plan_sha256: &[u8],
    offset: u64,
    bytes: &[u8],
    operation_id: &str,
    content_length: u64,
    content_sha256: &[u8],
) -> Reader<OwnedSegments> {
    private_call(
        app,
        host_wire::Action::GuestMutationSubmit,
        |mut request| {
            request.set_io_key(key);
            let mut command = request.reborrow().init_guest_mutation_command();
            command.set_submission(&[token; 32]);
            command.set_kind(kind);
            command.set_plan_sha256(plan_sha256);
            command.set_offset(offset);
            command.set_bytes(bytes);
            command.set_operation_id(operation_id);
            command.set_content_length(content_length);
            command.set_content_sha256(content_sha256);
        },
    )
}

fn read_result(app: &mut Workbench, key: &[u8], command: u64) -> Reader<OwnedSegments> {
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        let reply = private_call(app, host_wire::Action::GuestMutationRead, |mut request| {
            request.set_io_key(key);
            request.set_mutation_command_id(command);
        });
        assert_ok(&reply);
        if row(&reply).get_guest_mutation_result().unwrap().get_kind() != 0 {
            return reply;
        }
        assert!(
            Instant::now() < until,
            "guest command {command} remained pending"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn command_id(message: &Reader<OwnedSegments>) -> u64 {
    assert_ok(message);
    row(message).get_mutation_command_id()
}

fn decode_task_key(message: &Reader<OwnedSegments>) -> Vec<u8> {
    assert_ok(message);
    row(message)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec()
}

struct FrameFacts {
    phase: guest_wire::Phase,
    effect: guest_wire::Effect,
    staged_bytes: u64,
    durable_content: bool,
    reference: [u8; 32],
}

fn frame(
    message: &Reader<OwnedSegments>,
    expected_call_id: u64,
    expected_token: u8,
    expected_operation_id: &str,
    expected_kind: guest_wire::Kind,
) -> FrameFacts {
    assert_ok(message);
    let result = row(message).get_guest_mutation_result().unwrap();
    assert_eq!(result.get_kind(), 2);
    let encoded = result.get_frame().unwrap();
    assert!(encoded.len() <= guest_wire::MAX_FRAME_BYTES);
    let mut input = encoded;
    let decoded = serialize::read_message(&mut input, ReaderOptions::new()).unwrap();
    assert!(input.is_empty(), "trailing guest response bytes");
    let response = decoded
        .get_root::<morrow_core::mutation_capnp::response::Reader>()
        .unwrap();
    assert_eq!(response.get_version(), guest_wire::VERSION);
    assert_eq!(
        response.get_schema_sha256().unwrap(),
        guest_wire::schema_digest()
    );
    assert_eq!(response.get_call_id(), expected_call_id);
    assert_eq!(response.get_submission().unwrap(), [expected_token; 32]);
    assert_eq!(
        response.get_operation_id().unwrap().to_str().unwrap(),
        expected_operation_id
    );
    assert_eq!(response.get_kind().unwrap(), expected_kind);
    assert_eq!(
        response.get_status().unwrap(),
        guest_wire::Status::Completed
    );
    let reference: [u8; 32] = response.get_reference().unwrap().try_into().unwrap();
    assert_ne!(reference, [0; 32]);
    FrameFacts {
        phase: response.get_phase().unwrap(),
        effect: response.get_effect().unwrap(),
        staged_bytes: response.get_staged_bytes(),
        durable_content: response.get_durable_content(),
        reference,
    }
}

fn checked_frame(
    app: &mut Workbench,
    key: &[u8],
    token: u8,
    kind: u16,
    plan_sha256: &[u8],
    offset: u64,
    bytes: &[u8],
    operation_id: &str,
    content_length: u64,
    content_sha256: &[u8],
    expected_kind: guest_wire::Kind,
) -> FrameFacts {
    let submitted = submit(
        app,
        key,
        token,
        kind,
        plan_sha256,
        offset,
        bytes,
        "",
        content_length,
        content_sha256,
    );
    let command = command_id(&submitted);
    let read = read_result(app, key, command);
    let facts = frame(&read, command, token, operation_id, expected_kind);
    export(&format!("kind-{kind}-{token}"), &read);
    facts
}

#[test]
fn private_guest_wire_real_sdk_original_owner_create_delete() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let revision = revision(&setup);
    let root = setup.dir.path().join("private-guest");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("content.bin");
    let content_length = 3 * guest_wire::MAX_CHUNK_BYTES + 37;
    let mut body = Vec::with_capacity(content_length);
    let mut counter = 0u64;
    while body.len() < content_length {
        let mut hash = Sha256::new();
        hash.update(b"morrow.private.guest.wire.body.v1");
        hash.update(counter.to_le_bytes());
        body.extend_from_slice(&hash.finalize());
        counter += 1;
    }
    body.truncate(content_length);
    let digest: [u8; 32] = Sha256::digest(&body).into();
    let started = start(
        &mut setup,
        revision,
        1,
        Disposition::Create,
        &root,
        "content.bin",
        Some(BUDGET),
    );
    let key = decode_task_key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    assert_eq!(command_id(&started), 1);
    assert_eq!(
        row(&started)
            .get_guest_mutation_state()
            .unwrap()
            .get_command(),
        1
    );
    export("started", &started);
    let wrong_read = private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationRead,
        |mut request| {
            request.set_io_key(&key);
            request.set_mutation_command_id(999);
        },
    );
    assert!(!row(&wrong_read).get_error().unwrap().is_empty());
    let wrong_cancel = private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationCancelCommand,
        |mut request| {
            request.set_io_key(&key);
            request.set_mutation_command_id(999);
        },
    );
    assert!(!row(&wrong_cancel).get_error().unwrap().is_empty());
    let selected = read_result(&mut setup.app, &key, 1);
    assert_ok(&selected);
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_kind(),
        1
    );
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        1
    );
    export("selected", &selected);
    assert!(!leaf.exists());

    let operation = "private-guest-create";
    let planned = submit(
        &mut setup.app,
        &key,
        2,
        1,
        &[],
        0,
        &[],
        operation,
        body.len() as u64,
        &digest,
    );
    let planned_id = command_id(&planned);
    let duplicate = submit(
        &mut setup.app,
        &key,
        2,
        1,
        &[],
        0,
        &[],
        operation,
        body.len() as u64,
        &digest,
    );
    assert_eq!(command_id(&duplicate), planned_id);
    let conflicting = submit(
        &mut setup.app,
        &key,
        2,
        1,
        &[],
        0,
        &[],
        "other-operation",
        body.len() as u64,
        &digest,
    );
    assert!(!row(&conflicting).get_error().unwrap().is_empty());
    let planned = read_result(&mut setup.app, &key, planned_id);
    assert_ok(&planned);
    let plan_result = row(&planned).get_guest_mutation_result().unwrap();
    assert_eq!(plan_result.get_kind(), 1);
    let plan_result = plan_result.get_owner().unwrap();
    assert_eq!(plan_result.get_kind(), 11);
    let plan = plan_result.get_plan().unwrap();
    let plan_sha256: [u8; 32] = Sha256::digest(plan).into();
    export("planned", &planned);
    let status = private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationStatus,
        |mut request| {
            request.set_io_key(&key);
        },
    );
    assert_ok(&status);
    assert_eq!(
        row(&status)
            .get_guest_mutation_state()
            .unwrap()
            .get_reviewed_plan_sha256()
            .unwrap(),
        plan_sha256
    );
    export("planned-status", &status);
    assert!(!leaf.exists());

    let prepared = checked_frame(
        &mut setup.app,
        &key,
        3,
        2,
        &plan_sha256,
        0,
        &[],
        operation,
        0,
        &[],
        guest_wire::Kind::PrepareCreate,
    );
    assert_eq!(prepared.phase, guest_wire::Phase::Prepared);
    assert_eq!(prepared.effect, guest_wire::Effect::Unspecified);
    assert!(!leaf.exists());
    let reference = prepared.reference;
    for (index, chunk) in body.chunks(guest_wire::MAX_CHUNK_BYTES).enumerate() {
        let staged = checked_frame(
            &mut setup.app,
            &key,
            4 + index as u8,
            3,
            &[],
            (index * guest_wire::MAX_CHUNK_BYTES) as u64,
            chunk,
            operation,
            0,
            &[],
            guest_wire::Kind::Chunk,
        );
        assert_eq!(staged.reference, reference);
        assert_eq!(staged.phase, guest_wire::Phase::Prepared);
        assert_eq!(
            staged.staged_bytes,
            (index * guest_wire::MAX_CHUNK_BYTES + chunk.len()) as u64
        );
        assert!(!staged.durable_content);
        assert!(!leaf.exists());
    }
    let committed = checked_frame(
        &mut setup.app,
        &key,
        8,
        4,
        &[],
        0,
        &[],
        operation,
        0,
        &[],
        guest_wire::Kind::Commit,
    );
    assert_eq!(committed.reference, reference);
    assert!(committed.durable_content);
    assert!(!leaf.exists());
    let wrong_plan = submit(&mut setup.app, &key, 9, 5, &[9; 32], 0, &[], "", 0, &[]);
    assert!(!row(&wrong_plan).get_error().unwrap().is_empty());
    export("invalid-plan", &wrong_plan);
    assert!(!leaf.exists());
    let executed = checked_frame(
        &mut setup.app,
        &key,
        9,
        5,
        &plan_sha256,
        0,
        &[],
        operation,
        0,
        &[],
        guest_wire::Kind::Execute,
    );
    assert_eq!(executed.reference, reference);
    assert_eq!(
        (executed.phase, executed.effect),
        (guest_wire::Phase::Observed, guest_wire::Effect::OsSucceeded)
    );
    assert_eq!(fs::read(&leaf).unwrap(), body);
    let repeated_effect = submit(
        &mut setup.app,
        &key,
        10,
        5,
        &plan_sha256,
        0,
        &[],
        "",
        0,
        &[],
    );
    assert!(!row(&repeated_effect).get_error().unwrap().is_empty());
    assert_eq!(fs::read(&leaf).unwrap(), body);
    let queried = checked_frame(
        &mut setup.app,
        &key,
        11,
        6,
        &[],
        0,
        &[],
        operation,
        0,
        &[],
        guest_wire::Kind::Query,
    );
    assert_eq!(queried.reference, reference);
    assert_eq!(
        (queried.phase, queried.effect),
        (guest_wire::Phase::Observed, guest_wire::Effect::OsSucceeded)
    );
    let released = checked_frame(
        &mut setup.app,
        &key,
        12,
        8,
        &[],
        0,
        &[],
        operation,
        0,
        &[],
        guest_wire::Kind::Release,
    );
    assert_eq!(released.reference, reference);
    stop(&mut setup.app, task);

    let delete_start = start(
        &mut setup,
        revision,
        13,
        Disposition::Delete,
        &leaf,
        "",
        Some(BUDGET),
    );
    let delete_key = decode_task_key(&delete_start);
    let delete_task = TaskKey::from_bytes(&delete_key).unwrap();
    let selected = read_result(&mut setup.app, &delete_key, 1);
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        1
    );
    let delete_operation = "private-guest-delete";
    let planned = submit(
        &mut setup.app,
        &delete_key,
        14,
        1,
        &[],
        0,
        &[],
        delete_operation,
        0,
        &[],
    );
    let planned = read_result(&mut setup.app, &delete_key, command_id(&planned));
    let owner = row(&planned)
        .get_guest_mutation_result()
        .unwrap()
        .get_owner()
        .unwrap();
    assert_eq!(owner.get_kind(), 11);
    let delete_sha256: [u8; 32] = Sha256::digest(owner.get_plan().unwrap()).into();
    let prepared = checked_frame(
        &mut setup.app,
        &delete_key,
        15,
        2,
        &delete_sha256,
        0,
        &[],
        delete_operation,
        0,
        &[],
        guest_wire::Kind::PrepareDelete,
    );
    assert_eq!(prepared.phase, guest_wire::Phase::Prepared);
    let deleted = checked_frame(
        &mut setup.app,
        &delete_key,
        16,
        5,
        &delete_sha256,
        0,
        &[],
        delete_operation,
        0,
        &[],
        guest_wire::Kind::Execute,
    );
    assert_eq!(
        (deleted.phase, deleted.effect),
        (guest_wire::Phase::Observed, guest_wire::Effect::OsSucceeded)
    );
    assert!(!leaf.exists());
    let released = checked_frame(
        &mut setup.app,
        &delete_key,
        17,
        8,
        &[],
        0,
        &[],
        delete_operation,
        0,
        &[],
        guest_wire::Kind::Release,
    );
    assert_eq!(released.reference, prepared.reference);
    stop(&mut setup.app, delete_task);
}

#[test]
fn private_guest_wire_rejects_missing_budget_and_conflicting_start_identity() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let revision = revision(&setup);
    let root = setup.dir.path().join("private-start-identity");
    fs::create_dir(&root).unwrap();
    let without_budget = start(
        &mut setup,
        revision,
        30,
        Disposition::Create,
        &root,
        "first.bin",
        None,
    );
    assert!(!row(&without_budget).get_error().unwrap().is_empty());
    let started = start(
        &mut setup,
        revision,
        31,
        Disposition::Create,
        &root,
        "first.bin",
        Some(BUDGET),
    );
    let key = decode_task_key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    let same = start(
        &mut setup,
        revision,
        31,
        Disposition::Create,
        &root,
        "first.bin",
        Some(BUDGET),
    );
    assert_eq!(decode_task_key(&same), key);
    assert_eq!(command_id(&same), 1);
    let wrong_target = start(
        &mut setup,
        revision,
        31,
        Disposition::Create,
        &root,
        "other.bin",
        Some(BUDGET),
    );
    assert!(!row(&wrong_target).get_error().unwrap().is_empty());
    let lower = MutationBudget {
        max_job_bytes: 16 * 1024 * 1024,
        max_bytes: 64 * 1024 * 1024,
    };
    let wrong_budget = start(
        &mut setup,
        revision,
        31,
        Disposition::Create,
        &root,
        "first.bin",
        Some(lower),
    );
    assert!(!row(&wrong_budget).get_error().unwrap().is_empty());
    let selected = read_result(&mut setup.app, &key, 1);
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        1
    );
    let release = submit(&mut setup.app, &key, 32, 11, &[], 0, &[], "", 0, &[]);
    let release = read_result(&mut setup.app, &key, command_id(&release));
    assert_eq!(
        row(&release)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        7
    );
    stop(&mut setup.app, task);
    assert!(!root.join("first.bin").exists());
}

#[test]
fn private_guest_wire_exports_owner_issue_failure_without_approval() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    let revision = revision(&setup);
    let root = setup.dir.path().join("private-owner-failure");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("never-created.bin");
    // Admission can select and build a plan, but the IssueGuest preflight
    // cannot promise the remaining complete mutation recipe in this budget.
    let tight = MutationBudget {
        max_job_bytes: 1024 * 1024,
        max_bytes: 1024 * 1024,
    };
    let started = start(
        &mut setup,
        revision,
        71,
        Disposition::Create,
        &root,
        "never-created.bin",
        Some(tight),
    );
    let key = decode_task_key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    let selected = read_result(&mut setup.app, &key, 1);
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        1
    );
    let body = b"host budget preflight";
    let content_sha256: [u8; 32] = Sha256::digest(body).into();
    let planned = submit(
        &mut setup.app,
        &key,
        72,
        1,
        &[],
        0,
        &[],
        "private-owner-failure",
        body.len() as u64,
        &content_sha256,
    );
    let planned = read_result(&mut setup.app, &key, command_id(&planned));
    let owner = row(&planned)
        .get_guest_mutation_result()
        .unwrap()
        .get_owner()
        .unwrap();
    assert_eq!(owner.get_kind(), 11);
    let plan_sha256: [u8; 32] = Sha256::digest(owner.get_plan().unwrap()).into();
    let prepare = submit(
        &mut setup.app,
        &key,
        73,
        2,
        &plan_sha256,
        0,
        &[],
        "",
        0,
        &[],
    );
    let failed = read_result(&mut setup.app, &key, command_id(&prepare));
    assert_ok(&failed);
    let result = row(&failed).get_guest_mutation_result().unwrap();
    assert_eq!(
        result.get_kind(),
        1,
        "IssueGuest failed inside original owner"
    );
    let owner = result.get_owner().unwrap();
    assert_eq!(owner.get_kind(), 9);
    assert_eq!(owner.get_failure_layer(), 2);
    assert_eq!(owner.get_failure_code(), 15);
    let state = row(&failed).get_guest_mutation_state().unwrap();
    assert!(!state.get_approval_delivered());
    assert!(state.get_reconcile_required());
    assert!(!leaf.exists());
    export("owner-issue-failure", &failed);
    let release = submit(&mut setup.app, &key, 74, 11, &[], 0, &[], "", 0, &[]);
    let release = read_result(&mut setup.app, &key, command_id(&release));
    assert_eq!(
        row(&release)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        7
    );
    stop(&mut setup.app, task);
    assert!(!leaf.exists());
}

#[test]
fn private_guest_wire_rejects_cross_domain_and_irrelevant_payloads() {
    let _serial = serial_effects();
    let mut setup = Setup::new();
    // Exercise the worker's actual business dispatcher, independent of whether
    // the outer app currently has a service task accepting CommandSubmit.
    for action in [
        host_wire::Action::GuestMutationStart,
        host_wire::Action::GuestMutationSubmit,
        host_wire::Action::GuestMutationStatus,
        host_wire::Action::GuestMutationRead,
        host_wire::Action::GuestMutationCancelCommand,
    ] {
        let mut message = Builder::new_default();
        let mut request = message.init_root::<host_wire::request::Builder>();
        request.set_version(1);
        request.set_digest(&protocol::digest());
        request.set_action(action);
        let bytes = serialize::write_message_to_words(&message);
        let reply = protocol::respond_state(setup.app.local_state_mut().unwrap(), &bytes).unwrap();
        let decoded = serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap();
        assert_eq!(
            row(&decoded).get_error().unwrap().to_str().unwrap(),
            "scheduler actions require the outer application"
        );
        assert!(!row(&decoded).has_guest_mutation_state());
    }
    let root = setup.dir.path().join("private-domain");
    fs::create_dir(&root).unwrap();
    let digest = setup.digest;
    let revision = setup
        .app
        .local_state()
        .unwrap()
        .manager
        .as_ref()
        .unwrap()
        .revision();
    let mixed_start = private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationStart,
        |mut request| {
            request.reborrow().init_mutation_start();
            let mut guest = request.reborrow().init_guest_mutation_start();
            let mut selection = guest.reborrow().init_selection();
            selection.set_submission(&[40; 32]);
            selection.set_package_id(ID);
            selection.set_package_digest(&digest);
            selection.set_registry_revision(revision);
            selection.set_disposition(1);
            selection.set_selected_path(root.to_str().unwrap());
            selection.set_relative_path("never.bin");
            selection.set_subject(SUBJECT);
            selection.set_approval_sha256(&APPROVAL);
            selection.set_timeout_ms(30_000);
            let mut budget = guest.init_approved_budget();
            budget.set_max_job_bytes(BUDGET.max_job_bytes);
            budget.set_max_bytes(BUDGET.max_bytes);
        },
    );
    assert!(!row(&mixed_start).get_error().unwrap().is_empty());
    let started = start(
        &mut setup,
        revision,
        41,
        Disposition::Create,
        &root,
        "never.bin",
        Some(BUDGET),
    );
    let key = decode_task_key(&started);
    let task = TaskKey::from_bytes(&key).unwrap();
    let selected = read_result(&mut setup.app, &key, 1);
    assert_eq!(
        row(&selected)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        1
    );

    let nested_business = private_call(
        &mut setup.app,
        host_wire::Action::CommandSubmit,
        |mut request| {
            request.set_io_key(&key);
            let mut command = request.reborrow().init_guest_mutation_command();
            command.set_submission(&[42; 32]);
            command.set_kind(1);
            command.set_operation_id("cross-domain");
        },
    );
    assert!(!row(&nested_business).get_error().unwrap().is_empty());
    assert!(
        !nested_business
            .get_root::<host_wire::response::Reader>()
            .unwrap()
            .has_guest_mutation_state()
    );

    let mixed_command = private_call(
        &mut setup.app,
        host_wire::Action::GuestMutationSubmit,
        |mut request| {
            request.set_io_key(&key);
            request.reborrow().init_mutation_command();
            let mut guest = request.reborrow().init_guest_mutation_command();
            guest.set_submission(&[43; 32]);
            guest.set_kind(1);
            guest.set_operation_id("cross-domain");
            guest.set_content_sha256(&Sha256::digest([]));
        },
    );
    assert!(!row(&mixed_command).get_error().unwrap().is_empty());

    let empty_hash: [u8; 32] = Sha256::digest([]).into();
    let irrelevant = submit(
        &mut setup.app,
        &key,
        44,
        1,
        &[],
        0,
        b"unexpected",
        "irrelevant",
        0,
        &empty_hash,
    );
    assert!(!row(&irrelevant).get_error().unwrap().is_empty());
    let incomplete = submit(&mut setup.app, &key, 45, 1, &[], 0, &[], "", 0, &[]);
    assert!(!row(&incomplete).get_error().unwrap().is_empty());
    assert_eq!(setup.app.guest_mutation_status(task).unwrap().command, 1);
    assert!(!root.join("never.bin").exists());
    let release = submit(&mut setup.app, &key, 46, 11, &[], 0, &[], "", 0, &[]);
    let release = read_result(&mut setup.app, &key, command_id(&release));
    assert_eq!(
        row(&release)
            .get_guest_mutation_result()
            .unwrap()
            .get_owner()
            .unwrap()
            .get_kind(),
        7
    );
    stop(&mut setup.app, task);
}
