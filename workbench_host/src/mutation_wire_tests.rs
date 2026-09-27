//! Private Workbench mutation wire tests. These exercise the native outer scheduler.
use super::*;
use crate::{host_capnp as wire, protocol};
use capnp::{
    message::{Builder, Reader, ReaderOptions},
    serialize::{self, OwnedSegments},
};
use morrow_core::{
    file_effect::{CreateOutcome, CreateResult},
    io_intent::Record,
};
use morrow_plugin_runtime::io_jobs::MAX_MUTATION_CHUNK;
use std::path::Path;

fn wire_call(
    app: &mut Workbench,
    action: wire::Action,
    configure: impl FnOnce(wire::request::Builder<'_>),
) -> Reader<OwnedSegments> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    configure(request);
    let frame = serialize::write_message_to_words(&message);
    let reply = protocol::respond(app, &frame).unwrap();
    assert!(reply.len() <= 128 * 1024);
    serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap()
}

fn row<'a>(message: &'a Reader<OwnedSegments>) -> wire::response::Reader<'a> {
    message.get_root::<wire::response::Reader>().unwrap()
}

fn assert_ok(message: &Reader<OwnedSegments>) {
    assert_eq!(row(message).get_error().unwrap().to_str().unwrap(), "");
}

fn start(
    setup: &mut Setup,
    revision: u64,
    submission: &[u8; 32],
    disposition: Disposition,
    selected: &Path,
    relative: &str,
) -> Reader<OwnedSegments> {
    let digest = setup.digest;
    let action = match disposition {
        Disposition::Create => 1,
        Disposition::Replace => 2,
        Disposition::Delete => 3,
    };
    wire_call(
        &mut setup.app,
        wire::Action::MutationStart,
        |mut request| {
            let mut start = request.reborrow().init_mutation_start();
            start.set_submission(submission);
            start.set_package_id(ID);
            start.set_package_digest(&digest);
            start.set_registry_revision(revision);
            start.set_disposition(action);
            start.set_selected_path(selected.to_str().unwrap());
            start.set_relative_path(relative);
            start.set_subject(SUBJECT);
            start.set_approval_sha256(&APPROVAL);
            start.set_timeout_ms(10_000);
        },
    )
}

fn submit(
    app: &mut Workbench,
    key: &[u8],
    submission: &[u8; 32],
    kind: u16,
    plan: &[u8],
    offset: u64,
    bytes: &[u8],
) -> Reader<OwnedSegments> {
    wire_call(app, wire::Action::MutationSubmit, |mut request| {
        request.set_io_key(key);
        let mut command = request.reborrow().init_mutation_command();
        command.set_submission(submission);
        command.set_kind(kind);
        command.set_plan(plan);
        command.set_offset(offset);
        command.set_bytes(bytes);
    })
}

fn read_result(app: &mut Workbench, key: &[u8], command: u64) -> Reader<OwnedSegments> {
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        let reply = wire_call(app, wire::Action::MutationRead, |mut request| {
            request.set_io_key(key);
            request.set_mutation_command_id(command);
        });
        assert_ok(&reply);
        if row(&reply).get_mutation_result().unwrap().get_kind() != 0 {
            return reply;
        }
        assert!(
            Instant::now() < until,
            "mutation command {command} remained pending"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn export_wire_fixture(name: &str, response: &Reader<OwnedSegments>) {
    if let Ok(directory) = std::env::var("MORROW_MUTATION_WIRE_FIXTURES") {
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            Path::new(&directory).join(format!("rust-mutation-{name}.bin")),
            serialize::write_message_segments_to_words(response.get_segments()),
        )
        .unwrap();
    }
}

fn command_id(message: &Reader<OwnedSegments>) -> u64 {
    assert_ok(message);
    row(message).get_mutation_command_id()
}

fn close(setup: &mut Setup, key: &[u8], task: TaskKey, token: &[u8; 32]) {
    let release = submit(&mut setup.app, key, token, 7, &[], 0, &[]);
    let result = read_result(&mut setup.app, key, command_id(&release));
    assert_eq!(row(&result).get_mutation_result().unwrap().get_kind(), 7);
    reclaim(&mut setup.app, task);
    setup.app.acknowledge_io(task).unwrap();
}

#[test]
fn private_wire_create_is_idempotent_and_reports_authoritative_result() {
    let _serial = serial_effects();
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("root");
    fs::create_dir(&root).unwrap();
    let leaf = root.join("new.bin");
    let bytes = b"private mutation wire create";
    let start_token = [11; 32];
    let revision = setup.options(Disposition::Create).revision;
    let started = start(
        &mut setup,
        revision,
        &start_token,
        Disposition::Create,
        &root,
        "new.bin",
    );
    assert_ok(&started);
    let key = row(&started)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let task = TaskKey::from_bytes(&key).unwrap();
    assert_eq!(command_id(&started), 1);
    assert_eq!(row(&started).get_mutation_state().unwrap().get_command(), 1);

    let repeated = start(
        &mut setup,
        revision,
        &start_token,
        Disposition::Create,
        &root,
        "new.bin",
    );
    assert_ok(&repeated);
    assert_eq!(
        row(&repeated).get_io_state().unwrap().get_key().unwrap(),
        key
    );
    assert_eq!(command_id(&repeated), 1);
    let conflict = start(
        &mut setup,
        revision,
        &start_token,
        Disposition::Create,
        &root,
        "other.bin",
    );
    assert!(!row(&conflict).get_error().unwrap().is_empty());
    assert!(!leaf.exists());
    assert_eq!(setup.app.mutation_status(task).unwrap().command, 1);

    let selected = read_result(&mut setup.app, &key, 1);
    export_wire_fixture("selected", &selected);
    let selection = row(&selected).get_mutation_result().unwrap();
    assert_eq!(selection.get_kind(), 1);
    let reference: [u8; 32] = selection.get_reference().unwrap().try_into().unwrap();
    assert!(selection.get_expected_identity().unwrap().is_empty());
    let request = plan(
        setup.digest,
        "wire-create",
        Disposition::Create,
        reference,
        Some(RelativeFilePath::parse("new.bin").unwrap()),
        None,
        bytes,
    );

    let prepared = submit(
        &mut setup.app,
        &key,
        &[12; 32],
        1,
        request.container(),
        0,
        &[],
    );
    let prepare_id = command_id(&prepared);
    assert_eq!(prepare_id, 2);
    assert_eq!(
        command_id(&submit(
            &mut setup.app,
            &key,
            &[12; 32],
            1,
            request.container(),
            0,
            &[]
        )),
        prepare_id
    );
    let reused = submit(&mut setup.app, &key, &[12; 32], 5, &[], 0, &[]);
    assert!(!row(&reused).get_error().unwrap().is_empty());
    for action in [
        wire::Action::MutationRead,
        wire::Action::MutationCancelCommand,
    ] {
        let late = wire_call(&mut setup.app, action, |mut request| {
            request.set_io_key(&key);
            request.set_mutation_command_id(1);
        });
        assert_eq!(row(&late).get_ui_code(), 113);
        assert!(!row(&late).has_mutation_result());
    }
    let prepared_result = read_result(&mut setup.app, &key, prepare_id);
    export_wire_fixture("prepared", &prepared_result);
    let prepared_row = row(&prepared_result).get_mutation_result().unwrap();
    assert_eq!(prepared_row.get_kind(), 2);
    assert_eq!(prepared_row.get_phase(), 1);
    assert_eq!(
        prepared_row.get_operation_id().unwrap().to_str().unwrap(),
        "wire-create"
    );
    assert_eq!(
        Record::decode(prepared_row.get_record().unwrap())
            .unwrap()
            .phase(),
        Phase::Prepared
    );
    assert_eq!(
        command_id(&submit(
            &mut setup.app,
            &key,
            &[12; 32],
            1,
            request.container(),
            0,
            &[]
        )),
        prepare_id
    );

    let chunk = submit(&mut setup.app, &key, &[13; 32], 2, &[], 0, bytes);
    let chunk_id = command_id(&chunk);
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[13; 32], 2, &[], 0, bytes)),
        chunk_id
    );
    let staged = read_result(&mut setup.app, &key, chunk_id);
    assert_eq!(row(&staged).get_mutation_result().unwrap().get_kind(), 3);
    assert_eq!(
        row(&staged)
            .get_mutation_result()
            .unwrap()
            .get_staged_bytes(),
        bytes.len() as u64
    );
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[13; 32], 2, &[], 0, bytes)),
        chunk_id
    );

    let commit = submit(&mut setup.app, &key, &[14; 32], 3, &[], 0, &[]);
    let commit_id = command_id(&commit);
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[14; 32], 3, &[], 0, &[])),
        commit_id
    );
    let committed = read_result(&mut setup.app, &key, commit_id);
    assert_eq!(row(&committed).get_mutation_result().unwrap().get_kind(), 3);
    assert!(
        row(&committed)
            .get_mutation_result()
            .unwrap()
            .get_durable_content()
    );
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[14; 32], 3, &[], 0, &[])),
        commit_id
    );

    let execute = submit(&mut setup.app, &key, &[15; 32], 4, &[], 0, &[]);
    let execute_id = command_id(&execute);
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[15; 32], 4, &[], 0, &[])),
        execute_id
    );
    let executed = read_result(&mut setup.app, &key, execute_id);
    export_wire_fixture("created", &executed);
    let effect = row(&executed).get_mutation_result().unwrap();
    assert_eq!(effect.get_kind(), 4);
    assert_eq!(effect.get_effect(), 1);
    assert_eq!(effect.get_os_code(), 0);
    let outcome = CreateOutcome::decode(effect.get_outcome().unwrap()).unwrap();
    assert_eq!(outcome.operation_id(), "wire-create");
    assert!(matches!(outcome.result(), CreateResult::Created));
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
    assert_eq!(
        command_id(&submit(&mut setup.app, &key, &[15; 32], 4, &[], 0, &[])),
        execute_id
    );

    let query = submit(&mut setup.app, &key, &[16; 32], 5, &[], 0, &[]);
    let history = read_result(&mut setup.app, &key, command_id(&query));
    export_wire_fixture("query", &history);
    let history_row = row(&history).get_mutation_result().unwrap();
    assert_eq!(history_row.get_kind(), 6);
    assert_eq!(history_row.get_phase(), 3);
    assert_eq!(
        history_row.get_operation_id().unwrap().to_str().unwrap(),
        "wire-create"
    );
    assert_eq!(history_row.get_effect(), 0);
    assert_eq!(
        Record::decode(history_row.get_record().unwrap())
            .unwrap()
            .phase(),
        Phase::Observed
    );
    close(&mut setup, &key, task, &[17; 32]);
    let stale = start(
        &mut setup,
        revision,
        &start_token,
        Disposition::Create,
        &root,
        "new.bin",
    );
    assert_eq!(row(&stale).get_ui_code(), 113);
    assert!(!row(&stale).has_mutation_state());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert_eq!(fs::read(&leaf).unwrap(), bytes);
}

#[test]
fn private_wire_cancel_plan_keeps_file_and_exposes_cancelled_history() {
    let mut setup = Setup::new("rust");
    let leaf = setup.dir.path().join("keep.bin");
    fs::write(&leaf, b"keep").unwrap();
    let revision = setup.options(Disposition::Delete).revision;
    let started = start(
        &mut setup,
        revision,
        &[21; 32],
        Disposition::Delete,
        &leaf,
        "",
    );
    let key = row(&started)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let task = TaskKey::from_bytes(&key).unwrap();
    let selected = read_result(&mut setup.app, &key, 1);
    let selection = row(&selected).get_mutation_result().unwrap();
    let reference = selection.get_reference().unwrap().try_into().unwrap();
    let expected = Some(
        selection
            .get_expected_identity()
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let request = plan(
        setup.digest,
        "wire-cancel",
        Disposition::Delete,
        reference,
        None,
        expected,
        b"",
    );
    let prepare = submit(
        &mut setup.app,
        &key,
        &[22; 32],
        1,
        request.container(),
        0,
        &[],
    );
    assert_eq!(
        row(&read_result(&mut setup.app, &key, command_id(&prepare)))
            .get_mutation_result()
            .unwrap()
            .get_kind(),
        2
    );
    let cancel = submit(&mut setup.app, &key, &[23; 32], 6, &[], 0, &[]);
    let cancelled = read_result(&mut setup.app, &key, command_id(&cancel));
    assert_eq!(row(&cancelled).get_mutation_result().unwrap().get_kind(), 8);
    assert_eq!(
        row(&cancelled).get_mutation_result().unwrap().get_phase(),
        4
    );
    let rejected = submit(&mut setup.app, &key, &[24; 32], 4, &[], 0, &[]);
    assert!(!row(&rejected).get_error().unwrap().is_empty());
    let query = submit(&mut setup.app, &key, &[25; 32], 5, &[], 0, &[]);
    let history = read_result(&mut setup.app, &key, command_id(&query));
    let result = row(&history).get_mutation_result().unwrap();
    assert_eq!(result.get_kind(), 6);
    assert_eq!(result.get_phase(), 4);
    assert_eq!(
        result.get_operation_id().unwrap().to_str().unwrap(),
        "wire-cancel"
    );
    assert_eq!(result.get_effect(), 0);
    assert_eq!(
        Record::decode(result.get_record().unwrap())
            .unwrap()
            .phase(),
        Phase::CancelledBeforeDispatch
    );
    close(&mut setup, &key, task, &[26; 32]);
    assert_eq!(fs::read(&leaf).unwrap(), b"keep");
}

#[test]
fn private_wire_mixed_payload_and_frame_limit_have_no_side_effect() {
    let mut setup = Setup::new("rust");
    let root = setup.dir.path().join("root");
    fs::create_dir(&root).unwrap();
    let revision = setup.options(Disposition::Create).revision;
    let digest = setup.digest;
    let mixed_start = wire_call(
        &mut setup.app,
        wire::Action::MutationStart,
        |mut request| {
            request.set_io_key(&[8; 32]);
            let mut start = request.reborrow().init_mutation_start();
            start.set_submission(&[31; 32]);
            start.set_package_id(ID);
            start.set_package_digest(&digest);
            start.set_registry_revision(revision);
            start.set_disposition(1);
            start.set_selected_path(root.to_str().unwrap());
            start.set_relative_path("blocked.bin");
            start.set_subject(SUBJECT);
            start.set_approval_sha256(&APPROVAL);
            start.set_timeout_ms(10_000);
        },
    );
    assert!(!row(&mixed_start).get_error().unwrap().is_empty());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    assert!(!root.join("blocked.bin").exists());

    let oversized = wire_call(
        &mut setup.app,
        wire::Action::MutationStart,
        |mut request| {
            request.set_payload(&vec![0x5a; 128 * 1024]);
        },
    );
    assert!(!row(&oversized).get_error().unwrap().is_empty());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);

    let started = start(
        &mut setup,
        revision,
        &[32; 32],
        Disposition::Create,
        &root,
        "blocked.bin",
    );
    assert_ok(&started);
    let key = row(&started)
        .get_io_state()
        .unwrap()
        .get_key()
        .unwrap()
        .to_vec();
    let task = TaskKey::from_bytes(&key).unwrap();
    read_result(&mut setup.app, &key, 1);
    let mixed = submit(
        &mut setup.app,
        &key,
        &[33; 32],
        3,
        b"extraneous plan",
        0,
        &[],
    );
    assert!(!row(&mixed).get_error().unwrap().is_empty());
    let too_large = submit(
        &mut setup.app,
        &key,
        &[34; 32],
        2,
        &[],
        0,
        &vec![0x5a; MAX_MUTATION_CHUNK + 1],
    );
    assert!(!row(&too_large).get_error().unwrap().is_empty());
    assert_eq!(setup.app.mutation_status(task).unwrap().command, 1);
    assert!(!root.join("blocked.bin").exists());
    close(&mut setup, &key, task, &[35; 32]);
}

#[test]
fn private_mutation_actions_cannot_run_inside_business_state() {
    let mut setup = Setup::new("rust");
    for action in [
        wire::Action::MutationStart,
        wire::Action::MutationSubmit,
        wire::Action::MutationStatus,
        wire::Action::MutationRead,
        wire::Action::MutationCancelCommand,
    ] {
        let mut request = Builder::new_default();
        let mut root = request.init_root::<wire::request::Builder>();
        root.set_version(1);
        root.set_digest(&protocol::digest());
        root.set_action(action);
        let frame = serialize::write_message_to_words(&request);
        let reply = protocol::respond_state(setup.app.local_state_mut().unwrap(), &frame).unwrap();
        let response =
            serialize::read_message(&mut reply.as_slice(), ReaderOptions::new()).unwrap();
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
    }
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
}

#[path = "mutation_reconciliation_wire_tests.rs"]
mod reconciliation_wire_tests;

#[path = "mutation_plan_wire_tests.rs"]
mod plan_wire_tests;

#[path = "mutation_discovery_wire_tests.rs"]
mod discovery_wire_tests;
