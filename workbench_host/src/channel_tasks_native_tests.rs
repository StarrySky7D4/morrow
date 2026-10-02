//! Explicit synthetic owner tests on native non-Windows hosts. The original
//! HostRuntime, SQLite, Manager, wire controls, Wasmi and native threads are real;
//! protected storage remains unavailable and its maintenance failure is retained.
use super::*;
use crate::{
    channel_binding::{OwnerBinding, Prerequisite},
    host_capnp as wire, protocol,
};
use capnp::{message::Builder, message::ReaderOptions, serialize};
use morrow_core::{
    channel::{self, Directory, Frame, Request, Response},
    dispatch::HostRuntime,
    plugin_package::{Package, catalog::Catalog, proto::TransformHandler, registry::Registry},
    store::Store,
};
use morrow_plugin_runtime::{Fault, Limits, manager::Manager};

const WAIT: Duration = Duration::from_secs(10);

fn inert_package() -> Package {
    let wasm = wat::parse_str(
        r#"(module
            (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) i32.const 0))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.example.channel.native-owner-fixture",
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "channel.directory.consume".into(),
            input_type: DIRECTORY_INPUT_TYPE.into(),
            output_type: "bytes".into(),
            max_input_bytes: 65_536,
            max_output_bytes: 64,
        }],
    );
    manifest.required_features.push(channel::FEATURE.into());
    manifest.channel_declaration = Some(channel::declaration(
        vec!["channel.directory.consume".into()],
        vec![Kind::ByteStream, Kind::Events],
    ));
    Package::build(manifest, &wasm).unwrap()
}

fn trap_package() -> Package {
    let wasm = wat::parse_str(
        r#"(module
            (import "morrow_channel_v1" "call" (func (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 4)
            (func (export "morrow_run") (result i32) unreachable))"#,
    )
    .unwrap();
    let mut manifest = Package::manifest_for_transform(
        "org.example.channel.native-trap-fixture",
        "1.0.0",
        &wasm,
        vec![TransformHandler {
            handler: "channel.directory.consume".into(),
            input_type: DIRECTORY_INPUT_TYPE.into(),
            output_type: "bytes".into(),
            max_input_bytes: 65_536,
            max_output_bytes: 64,
        }],
    );
    manifest.required_features.push(channel::FEATURE.into());
    manifest.channel_declaration = Some(channel::declaration(
        vec!["channel.directory.consume".into()],
        vec![Kind::Events],
    ));
    Package::build(manifest, &wasm).unwrap()
}

struct Fixture {
    app: Workbench,
    package: Package,
    directory: tempfile::TempDir,
}
impl Fixture {
    fn new(package: Package) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let catalog = Catalog::open(&directory.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&directory.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, Limits::default());
        manager.select(&package, manager.revision()).unwrap();
        let runtime = HostRuntime::new(
            Store::open(&directory.path().join("store.sqlite3"), Default::default()).unwrap(),
        )
        .unwrap();
        let storage = crate::storage::Storage::test_from_runtime(runtime);
        let owner = WorkbenchState::with_manager(storage, Ok(manager), None).unwrap();
        Self {
            app: Workbench {
                channel_tasks: Default::default(),
                http_tasks: Default::default(),
                state: crate::io_tasks::StateSlot::new(owner),
            },
            package,
            directory,
        }
    }
    fn revision(&self) -> u64 {
        self.app
            .state
            .local()
            .unwrap()
            .manager
            .as_ref()
            .unwrap()
            .revision()
    }
    fn bind_synthetic(&mut self) {
        // Explicit test-only marker. Never call or emulate a production Linux
        // supervisor handshake, never replace Storage's failing maintenance.
        self.app.state.local_mut().unwrap().local_channel_owner =
            Some(OwnerBinding::SyntheticFixture);
        self.app.channel_tasks.owner = Some(OwnerBinding::SyntheticFixture);
    }
    fn enable(&mut self) {
        let revision = self.revision();
        self.app
            .configure_external(
                &self.package.manifest().package_id,
                &self.package.digest(),
                revision,
                &[],
                true,
            )
            .unwrap();
    }
    fn prepare(&self, submission: u8, kind: Kind, frames: u32, size: u32) -> Prepare {
        Prepare {
            submission: [submission; 32],
            package_id: self.package.manifest().package_id.clone(),
            package_digest: self.package.digest(),
            registry_revision: self.revision(),
            handler: "channel.directory.consume".into(),
            kind,
            duplex: false,
            budget: Budget {
                max_channels: 1,
                max_frame_bytes: size,
                max_bytes: u64::from(frames.max(1)) * u64::from(size),
                max_messages: u64::from(frames.max(1)),
                max_requests: 2 * u64::from(frames) + 1,
                max_duration_ms: 30_000,
            },
            lifetime_ms: 30_000,
            frame_count: frames,
            total_bytes: u64::from(frames) * u64::from(size),
        }
    }
}

#[derive(Debug)]
struct View {
    key: Vec<u8>,
    directory: Vec<u8>,
    phase: u16,
    task_state: u16,
    task_error: String,
    output: Vec<u8>,
    last_acked: u64,
    source_frames: u32,
    source_bytes: u64,
    cleanup_proof: u16,
    resource_reclaimed: bool,
    worker_joined: bool,
}
struct Reply {
    error: String,
    ui_code: u16,
    state: Option<View>,
}
fn call(
    app: &mut Workbench,
    action: wire::Action,
    fill: impl FnOnce(wire::request::Builder<'_>),
) -> Reply {
    let mut request = Builder::new_default();
    let mut root = request.init_root::<wire::request::Builder>();
    root.set_version(1);
    root.set_digest(&protocol::digest());
    root.set_action(action);
    fill(root);
    let bytes = protocol::respond(app, &serialize::write_message_to_words(&request)).unwrap();
    let mut remaining = bytes.as_slice();
    let message =
        serialize::read_message_from_flat_slice(&mut remaining, ReaderOptions::new()).unwrap();
    assert!(remaining.is_empty());
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(response.get_version(), 1);
    assert_eq!(response.get_digest().unwrap(), protocol::digest());
    let state = response.has_channel_state().then(|| {
        let state = response.get_channel_state().unwrap();
        View {
            key: state.get_key().unwrap().to_vec(),
            directory: state.get_directory().unwrap().to_vec(),
            phase: state.get_phase(),
            task_state: state.get_task_state(),
            task_error: state.get_task_error().unwrap().to_str().unwrap().into(),
            output: state.get_output().unwrap().to_vec(),
            last_acked: state.get_last_acked(),
            source_frames: state.get_source_frames(),
            source_bytes: state.get_source_bytes(),
            cleanup_proof: state.get_cleanup_proof(),
            resource_reclaimed: state.get_resource_reclaimed(),
            worker_joined: state.get_worker_joined(),
        }
    });
    Reply {
        error: response.get_error().unwrap().to_str().unwrap().into(),
        ui_code: response.get_ui_code(),
        state,
    }
}
fn ok(reply: Reply) -> View {
    assert!(reply.error.is_empty(), "{}", reply.error);
    assert_eq!(reply.ui_code, 0);
    reply.state.expect("channel state")
}
fn prepare(app: &mut Workbench, p: &Prepare) -> Reply {
    call(app, wire::Action::ChannelPrepare, |r| {
        let mut out = r.init_channel_prepare();
        out.set_submission(&p.submission);
        out.set_package_id(&p.package_id);
        out.set_package_digest(&p.package_digest);
        out.set_registry_revision(p.registry_revision);
        out.set_handler(&p.handler);
        out.set_kind(if p.kind == Kind::ByteStream { 1 } else { 2 });
        out.set_duplex(p.duplex);
        out.set_lifetime_ms(p.lifetime_ms);
        out.set_frame_count(p.frame_count);
        out.set_total_bytes(p.total_bytes);
        let mut b = out.init_budget();
        b.set_max_channels(p.budget.max_channels);
        b.set_max_frame_bytes(p.budget.max_frame_bytes);
        b.set_max_bytes(p.budget.max_bytes);
        b.set_max_messages(p.budget.max_messages);
        b.set_max_requests(p.budget.max_requests);
        b.set_max_duration_ms(p.budget.max_duration_ms);
    })
}
fn status(app: &mut Workbench, key: &[u8]) -> View {
    ok(call(app, wire::Action::ChannelStatus, |mut r| {
        r.set_channel_key(key)
    }))
}
fn close(app: &mut Workbench, key: &[u8]) -> View {
    ok(call(app, wire::Action::ChannelClose, |mut r| {
        r.set_channel_key(key)
    }))
}
fn run(app: &mut Workbench, key: &[u8], input: &[u8]) -> Reply {
    call(app, wire::Action::ChannelRun, |r| {
        let mut out = r.init_channel_run();
        out.set_key(key);
        out.set_input(input);
    })
}
fn append(app: &mut Workbench, key: &[u8], sequence: u64, bytes: &[u8], cursor: &[u8]) -> Reply {
    call(app, wire::Action::ChannelAppend, |r| {
        let mut out = r.init_channel_append();
        out.set_key(key);
        out.set_sequence(sequence);
        out.set_bytes(bytes);
        out.set_cursor(cursor);
    })
}
fn original_control(app: &Workbench) -> morrow_core::lifecycle::Revocation {
    let instance = &app.channel_tasks.job.as_ref().unwrap().instance;
    app.state
        .local()
        .unwrap()
        .host
        .revocation(instance.connection())
        .unwrap()
}

fn wait_native_joins(app: &mut Workbench, key: &[u8]) -> View {
    let deadline = Instant::now() + WAIT;
    loop {
        let state = status(app, key);
        if state.worker_joined && state.resource_reclaimed {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "original native executor/source joins pending: {state:?}"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn native_compilation_cannot_enable_a_package_or_bind_a_production_owner() {
    let mut f = Fixture::new(inert_package());
    let revision = f.revision();
    let entry = f
        .app
        .catalog_page("", Some(revision))
        .unwrap()
        .entries
        .remove(0);
    assert!(entry.channel_supported);
    assert!(!entry.available);
    assert!(!entry.enabled);
    assert!(!entry.issue.is_empty());
    assert!(
        f.app
            .configure_external(
                &f.package.manifest().package_id,
                &f.package.digest(),
                revision,
                &[],
                true
            )
            .is_err()
    );
    assert_eq!(f.revision(), revision);
    let error = f.app.bind_supervised_channel_owner().unwrap_err();
    assert_eq!(
        error.downcast_ref::<Prerequisite>(),
        Some(&Prerequisite::ProtectedStorageOwnerUnavailable)
    );
    assert!(f.app.channel_tasks.owner.is_none());
    assert!(f.app.state.local().unwrap().local_channel_owner.is_none());
    let request = f.prepare(10, Kind::ByteStream, 1, 31);
    let reply = prepare(&mut f.app, &request);
    assert_eq!(reply.ui_code, 130);
    assert!(reply.error.contains("protected original storage owner"));
    assert!(
        reply.state.is_none(),
        "no abandoned success/grant is serialized"
    );
    assert!(f.app.channel_tasks.job.is_none());
    assert!(f.app.channel_tasks.seen.is_empty());
    assert_eq!(f.app.channel_tasks.reserved_bytes, 0);
    assert_eq!(f.revision(), revision);
}

#[test]
fn public_linux_open_remains_unsupported_and_creates_no_replacement_owner() {
    let dir = tempfile::tempdir().unwrap();
    let database = dir.path().join("must-not-exist.sqlite3");
    assert!(Workbench::open(&database, None).is_err());
    assert!(!database.exists());
    assert!(Workbench::open_managed(&dir.path().join("managed"), None).is_err());
    assert!(!dir.path().join("managed").exists());
}

#[test]
fn empty_unbound_finish_preserves_the_existing_storage_failure() {
    let mut f = Fixture::new(inert_package());
    let revision = f.revision();
    assert!(f.app.finish_channels().is_ok());
    assert!(f.app.channel_tasks.job.is_none());
    assert!(f.app.channel_tasks.owner.is_none());
    assert_eq!(f.app.channel_tasks.requests, 0);
    assert_eq!(f.revision(), revision);
    assert!(f.app.finish().is_err());
    assert!(
        f.app.state.local().is_ok(),
        "the same original owner remains local"
    );
    assert!(f.app.maintenance_warning().is_some());
    assert!(f.app.channel_tasks.job.is_none());
    assert_eq!(f.revision(), revision);
}

#[test]
fn synthetic_prepare_close_retains_no_producer_and_no_executor_join() {
    let mut f = Fixture::new(inert_package());
    f.bind_synthetic();
    f.enable();
    let request = f.prepare(11, Kind::Events, 1, 31);
    let prepared = ok(prepare(&mut f.app, &request));
    assert_eq!(prepared.phase, 0);
    assert_eq!(prepared.cleanup_proof, CleanupProof::Pending as u16);
    let directory = Directory::decode(&prepared.directory).unwrap();
    assert_eq!(directory.channels.len(), 1);
    assert_eq!(directory.channels[0].kind, Kind::Events);
    assert_eq!(directory.encode().unwrap(), prepared.directory);
    let closed = close(&mut f.app, &prepared.key);
    assert_eq!(closed.phase, 5);
    assert_eq!(closed.cleanup_proof, CleanupProof::NoProducer as u16);
    assert!(closed.resource_reclaimed);
    assert!(!closed.worker_joined);
    assert_eq!(closed.task_state, 0);
    assert!(closed.output.is_empty());
    assert!(f.app.state.local().is_ok());
    assert!(
        f.app.finish().is_err(),
        "synthetic storage maintenance remains unsupported"
    );
}

#[test]
fn malformed_append_incomplete_run_and_foreign_directory_start_no_threads() {
    let mut f = Fixture::new(inert_package());
    f.bind_synthetic();
    f.enable();
    let request = f.prepare(12, Kind::ByteStream, 1, 31);
    let prepared = ok(prepare(&mut f.app, &request));
    assert!(
        !run(&mut f.app, &prepared.key, &prepared.directory)
            .error
            .is_empty()
    );
    for (sequence, bytes, cursor) in [
        (2, vec![1; 31], vec![]),
        (1, vec![1; 32], vec![]),
        (1, vec![1; 31], vec![1]),
    ] {
        assert!(
            !append(&mut f.app, &prepared.key, sequence, &bytes, &cursor)
                .error
                .is_empty()
        );
    }
    ok(append(&mut f.app, &prepared.key, 1, &[7; 31], &[]));
    let mut foreign = Directory::decode(&prepared.directory).unwrap();
    foreign.channels[0].reference[0] ^= 1;
    let rejected = run(&mut f.app, &prepared.key, &foreign.encode().unwrap());
    assert!(rejected.error.contains("belongs to another grant"));
    assert!(rejected.state.is_none());
    let job = f.app.channel_tasks.job.as_ref().unwrap();
    assert!(!job.run_admitted);
    assert!(!job.worker_started);
    assert!(job.worker.is_none());
    assert_eq!(job.broker.snapshot().cleanup_proof, CleanupProof::Pending);
    assert!(f.app.state.local().is_ok());
    assert_eq!(close(&mut f.app, &prepared.key).phase, 5);
}

#[test]
fn native_catalog_stop_and_controller_loss_close_only_the_original_grant() {
    let mut f = Fixture::new(inert_package());
    f.bind_synthetic();
    f.enable();
    let request = f.prepare(13, Kind::ByteStream, 1, 31);
    let prepared = ok(prepare(&mut f.app, &request));
    let instance = original_control(&f.app);
    let mut foreign = prepared.key.clone();
    foreign[0] ^= 1;
    let rejected = call(&mut f.app, wire::Action::ChannelClose, |mut r| {
        r.set_channel_key(&foreign)
    });
    assert!(rejected.error.contains("stale local channel job key"));
    assert!(!f.app.channel_tasks.job.as_ref().unwrap().close_requested);
    let revision = f.revision();
    let reply = call(&mut f.app, wire::Action::PluginApprove, |mut r| {
        r.set_id(&f.package.manifest().package_id);
        r.set_sha256(&f.package.digest());
        r.set_revision(revision);
        r.set_limit(0);
        r.init_approved_capabilities(0);
    });
    assert!(reply.error.is_empty(), "{}", reply.error);
    assert!(instance.is_revoked());
    assert_eq!(status(&mut f.app, &prepared.key).phase, 5);
    f.enable();
    let request = f.prepare(14, Kind::ByteStream, 1, 31);
    let next = ok(prepare(&mut f.app, &request));
    let original = original_control(&f.app);
    f.app.product_gate().revoke();
    assert!(original.is_revoked());
    assert_eq!(status(&mut f.app, &next.key).phase, 5);
    assert!(f.app.bind_supervised_channel_owner().is_err());
    let request = f.prepare(15, Kind::ByteStream, 1, 31);
    let rejected = prepare(&mut f.app, &request);
    assert!(rejected.error.contains("controller lost"));
    assert!(rejected.state.is_none());
}

#[test]
fn native_foreign_cleanup_keys_cannot_revoke_after_real_request_exhaustion() {
    let mut f = Fixture::new(inert_package());
    f.bind_synthetic();
    f.enable();
    let request = f.prepare(16, Kind::ByteStream, 1, 31);
    let prepared = ok(prepare(&mut f.app, &request));
    let original = original_control(&f.app);
    while f.app.channel_tasks.requests < MAX_HOST_REQUESTS {
        assert_eq!(status(&mut f.app, &prepared.key).phase, 0);
    }
    let mut foreign = prepared.key.clone();
    foreign[0] ^= 1;
    for action in [wire::Action::ChannelStatus, wire::Action::ChannelClose] {
        let reply = call(&mut f.app, action, |mut r| r.set_channel_key(&foreign));
        assert!(reply.error.contains("stale local channel job key"));
        assert!(reply.state.is_none());
    }
    assert!(!original.is_revoked());
    assert!(!f.app.channel_tasks.job.as_ref().unwrap().close_requested);
    let closed = close(&mut f.app, &prepared.key);
    assert_eq!(closed.phase, 5);
    assert_eq!(closed.cleanup_proof, CleanupProof::NoProducer as u16);
    assert!(!closed.worker_joined);
}

#[test]
fn native_wire_failed_reports_keep_primary_fault_original_owner_and_explicit_close() {
    for (case, (package, expected)) in [
        (inert_package(), Fault::TaskProtocol),
        (trap_package(), Fault::Trap),
    ]
    .into_iter()
    .enumerate()
    {
        let mut f = Fixture::new(package);
        f.bind_synthetic();
        f.enable();
        let foreign = {
            let owner = f.app.state.local_mut().unwrap();
            owner
                .manager
                .as_mut()
                .unwrap()
                .connect(&f.package.manifest().package_id, &mut owner.host)
                .unwrap()
        };
        let foreign_control = f
            .app
            .state
            .local()
            .unwrap()
            .host
            .revocation(foreign.connection())
            .unwrap();
        let request = f.prepare(30 + case as u8, Kind::Events, 1, 31);
        let prepared = ok(prepare(&mut f.app, &request));
        let original = original_control(&f.app);
        let original_binding = f.app.state.local().unwrap().host.binding();
        let original_instance = f.app.channel_tasks.job.as_ref().unwrap().instance.clone();
        ok(append(
            &mut f.app,
            &prepared.key,
            1,
            &[0x53; 31],
            b"original-cursor",
        ));
        ok(run(&mut f.app, &prepared.key, &prepared.directory));
        let completed = wait_native_joins(&mut f.app, &prepared.key);
        assert_eq!(completed.phase, 3);
        assert_eq!(completed.task_state, 3);
        assert!(completed.task_error.contains("runtime:"));
        assert!(!completed.task_error.contains("maintenance"));
        assert!(completed.output.is_empty());
        assert_eq!(completed.last_acked, 0);
        assert_eq!(completed.cleanup_proof, CleanupProof::Joined as u16);
        assert!(original.is_revoked());
        assert!(!foreign_control.is_revoked());
        assert_eq!(
            f.app.state.local().unwrap().host.binding(),
            original_binding
        );
        assert_eq!(
            f.app
                .state
                .local()
                .unwrap()
                .host
                .connection_phase(original_instance.connection()),
            Ok(morrow_core::lifecycle::InstancePhase::Revoked)
        );
        assert!(f.app.local_state_mut().is_err());
        let job = f.app.channel_tasks.job.as_ref().unwrap();
        assert!(job.run_admitted && job.worker_started && job.worker_joined);
        assert!(!job.close_requested && !job.disconnected);
        assert!(
            job.error.is_empty(),
            "the failed TaskReport remains primary"
        );
        let report = job.report.as_ref().unwrap();
        assert_eq!(report.execution.outcome, Err(expected.clone()));
        assert!(report.output.is_none());
        assert!(report.failure.is_none());
        let recorded = format!("{report:?}");
        let cause = job.broker.snapshot().terminal_cause;
        assert!(cause.is_some());
        let endpoint = job.broker.endpoint();
        let scope: [u8; 32] = prepared.key.as_slice().try_into().unwrap();
        let reopened = Store::open_existing(
            &f.directory.path().join("store.sqlite3"),
            Default::default(),
        )
        .unwrap();
        assert!(
            reopened
                .channel_checkpoint(&scope, &endpoint.source_epoch)
                .unwrap()
                .is_none()
        );
        assert!(
            reopened
                .channel_ack_receipt(&scope, &endpoint.source_epoch, 1)
                .unwrap()
                .is_none()
        );
        assert!(
            run(&mut f.app, &prepared.key, &prepared.directory)
                .error
                .contains("already admitted")
        );
        let closed = close(&mut f.app, &prepared.key);
        assert_eq!(closed.phase, 5);
        assert_eq!(closed.task_state, 4);
        assert!(closed.output.is_empty());
        assert!(f.app.channel_tasks.job.as_ref().unwrap().disconnected);
        assert_eq!(
            f.app
                .channel_tasks
                .job
                .as_ref()
                .unwrap()
                .broker
                .snapshot()
                .terminal_cause,
            cause
        );
        assert_eq!(
            format!(
                "{:?}",
                f.app
                    .channel_tasks
                    .job
                    .as_ref()
                    .unwrap()
                    .report
                    .as_ref()
                    .unwrap()
            ),
            recorded
        );
        assert!(
            f.app
                .state
                .local()
                .unwrap()
                .host
                .connection_phase(original_instance.connection())
                .is_err()
        );
        assert!(!foreign_control.is_revoked());
        f.app
            .state
            .local_mut()
            .unwrap()
            .host
            .disconnect(foreign.connection())
            .unwrap();
        eprintln!(
            "native wire executor failure={expected:?} original_owner_returned=true original_control_revoked=true foreign_control_live=true phase_before_explicit_close=3 ack=0 producer_joined=true executor_joined=true maintenance_repair=true primary_report_retained=true no_replay=true production_binding=false"
        );
    }
}

fn actual_package(control: bool) -> Package {
    let (key, pins) = if control {
        (
            "MORROW_CHANNEL_NATIVE_ONESHOT_PACKAGE",
            include_str!(
                "../../sdk/fixtures/rust-channel-directory-oneshot-003/ARTIFACT_SHA256SUMS"
            ),
        )
    } else {
        (
            "MORROW_CHANNEL_NATIVE_REUSABLE_PACKAGE",
            include_str!(
                "../../sdk/fixtures/rust-channel-directory-reusable-003/ARTIFACT_SHA256SUMS"
            ),
        )
    };
    let path =
        std::env::var_os(key).unwrap_or_else(|| panic!("actual SDK artifact required: {key}"));
    let archive = std::fs::read(path).unwrap();
    let package = Package::decode(&archive).unwrap();
    for (expected, actual) in pins.lines().map(|line| {
        let mut fields = line.split_whitespace();
        let expected = fields.next().unwrap();
        let file = fields.next().unwrap();
        let bytes = if file == "build/plugin.wasm" {
            package.module()
        } else {
            package.archive()
        };
        (expected, format!("{:x}", Sha256::digest(bytes)))
    }) {
        assert_eq!(expected, actual, "existing immutable artifact pin");
    }
    let budget = package.manifest().budget.as_ref().unwrap();
    assert_eq!(
        (budget.fuel, budget.memory_bytes, budget.host_calls),
        (20_000_000, 16_777_216, 16)
    );
    package
}
fn payload(sequence: u64) -> Vec<u8> {
    (0..32_768)
        .map(|offset| ((offset + sequence as usize * 17) % 251) as u8)
        .collect()
}

#[test]
#[ignore = "requires the actual pinned reusable and matched one-shot Directory003 SDK packages"]
fn real_wasm_wire_runs_preserve_original_owner_acks_and_unknown_maintenance() {
    for control in [false, true] {
        for kind in [Kind::ByteStream, Kind::Events] {
            let mut f = Fixture::new(actual_package(control));
            f.bind_synthetic();
            f.enable();
            let request = f.prepare(21, kind, 5, 32_768);
            let prepared = ok(prepare(&mut f.app, &request));
            let original = original_control(&f.app);
            let original_binding = f.app.state.local().unwrap().host.binding();
            let original_instance = f.app.channel_tasks.job.as_ref().unwrap().instance.clone();
            for sequence in 1u64..=5 {
                let cursor = if kind == Kind::Events {
                    sequence.to_le_bytes().to_vec()
                } else {
                    vec![]
                };
                ok(append(
                    &mut f.app,
                    &prepared.key,
                    sequence,
                    &payload(sequence),
                    &cursor,
                ));
            }
            ok(run(&mut f.app, &prepared.key, &prepared.directory));
            // Admission is never replayable, even if the caller missed the reply.
            assert!(
                run(&mut f.app, &prepared.key, &prepared.directory)
                    .error
                    .contains("already admitted")
            );
            let deadline = Instant::now() + WAIT;
            let completed = loop {
                let state = status(&mut f.app, &prepared.key);
                if state.worker_joined && state.resource_reclaimed {
                    break state;
                }
                assert!(
                    Instant::now() < deadline,
                    "original native joins missing: {state:?}"
                );
                thread::sleep(Duration::from_millis(1));
            };
            assert_eq!(completed.phase, 3);
            assert_eq!(completed.task_state, 4);
            assert!(
                original.is_revoked(),
                "maintenance failure stops original Control"
            );
            assert_eq!(
                f.app.state.local().unwrap().host.binding(),
                original_binding
            );
            assert_eq!(
                f.app
                    .state
                    .local()
                    .unwrap()
                    .host
                    .connection_phase(original_instance.connection()),
                Ok(morrow_core::lifecycle::InstancePhase::Revoked)
            );
            assert!(!f.app.channel_tasks.job.as_ref().unwrap().close_requested);
            assert!(!f.app.channel_tasks.job.as_ref().unwrap().disconnected);
            assert!(
                completed
                    .task_error
                    .contains("original storage maintenance failed; outcome Unknown")
            );
            assert!(
                completed.output.is_empty(),
                "unsupported protected maintenance cannot deliver business success"
            );
            assert_eq!(completed.last_acked, 5);
            assert_eq!(completed.source_frames, 5);
            assert_eq!(completed.source_bytes, 163_840);
            assert_eq!(completed.cleanup_proof, CleanupProof::Joined as u16);
            let job = f.app.channel_tasks.job.as_ref().unwrap();
            let report = job
                .report
                .as_ref()
                .expect("original actual Wasmi TaskReport retained");
            assert_eq!(report.execution.outcome, Ok(0));
            assert_eq!(report.execution.host_calls, 11);
            assert!(report.failure.is_none());
            let output = &report.output.as_ref().unwrap().bytes;
            assert_eq!(&output[..4], b"CHV1");
            assert_eq!(u64::from_le_bytes(output[16..24].try_into().unwrap()), 5);
            assert_eq!(
                u64::from_le_bytes(output[24..32].try_into().unwrap()),
                163_840
            );
            let mut digest = Sha256::new();
            for sequence in 1..=5 {
                digest.update(payload(sequence));
            }
            assert_eq!(&output[32..], digest.finalize().as_slice());
            let report_fuel = report.execution.fuel_remaining;
            let directory = Directory::decode(&prepared.directory).unwrap();
            let endpoint = &directory.channels[0];
            let scope: [u8; 32] = prepared.key.as_slice().try_into().unwrap();
            let reopened = Store::open_existing(
                &f.directory.path().join("store.sqlite3"),
                Default::default(),
            )
            .unwrap();
            for sequence in 1..=5 {
                if kind == Kind::Events {
                    let receipt = reopened
                        .channel_ack_receipt(&scope, &endpoint.source_epoch, sequence)
                        .unwrap()
                        .unwrap();
                    let frame = Frame::decode(&receipt.frame_wire).unwrap();
                    let request = Request::decode(&receipt.request_wire).unwrap();
                    let response = Response::decode(&receipt.response_wire).unwrap();
                    assert_eq!(frame.sequence, sequence);
                    assert_eq!(frame.bytes, payload(sequence));
                    assert_eq!(frame.cursor, sequence.to_le_bytes());
                    assert_eq!(request.reference, endpoint.reference);
                    assert_eq!(request.source_epoch, endpoint.source_epoch);
                    assert_eq!(
                        request.action,
                        channel::Action::Ack {
                            sequence,
                            frame_sha256: frame.digest().unwrap(),
                            cursor: frame.cursor.clone(),
                        }
                    );
                    response.validate_for(&request).unwrap();
                    assert_eq!(response.status, Status::Acked);
                    assert_eq!(response.last_acked, sequence);
                }
            }
            if kind == Kind::ByteStream {
                assert!(
                    reopened
                        .channel_checkpoint(&scope, &endpoint.source_epoch)
                        .unwrap()
                        .is_none()
                );
            }
            reopened.integrity_check().unwrap();
            assert!(
                f.app.local_state_mut().is_err(),
                "original repair requirement remains set"
            );
            let closed = close(&mut f.app, &prepared.key);
            assert_eq!(closed.phase, 5);
            assert!(closed.worker_joined);
            assert_eq!(closed.cleanup_proof, CleanupProof::Joined as u16);
            assert!(closed.output.is_empty());
            assert!(f.app.channel_tasks.job.as_ref().unwrap().disconnected);
            assert!(
                f.app
                    .state
                    .local()
                    .unwrap()
                    .host
                    .connection_phase(original_instance.connection())
                    .is_err()
            );
            assert_eq!(
                f.app
                    .channel_tasks
                    .job
                    .as_ref()
                    .unwrap()
                    .report
                    .as_ref()
                    .unwrap()
                    .execution
                    .fuel_remaining,
                report_fuel
            );
            eprintln!(
                "native channel fixture control={control} kind={kind:?} actual_wasmi=Ok(0) calls=11 fuel_remaining={report_fuel} source_bytes=163840 ack=5 producer_joined=true executor_joined=true protected_maintenance=unsupported wire_task_state=Unknown production_binding=false"
            );
            assert!(f.app.finish().is_err());
        }
    }
}

#[test]
#[ignore = "requires the actual pinned reusable Directory003 SDK package"]
fn pinned_private_output_bound_keeps_primary_error_owner_and_ack_without_replay() {
    let mut f = Fixture::new(actual_package(false));
    f.bind_synthetic();
    f.enable();
    let request = f.prepare(23, Kind::Events, 5, 32_768);
    let prepared = ok(prepare(&mut f.app, &request));
    let original = original_control(&f.app);
    let original_binding = f.app.state.local().unwrap().host.binding();
    // An explicitly stricter host-private output ceiling. The original package,
    // Wasm, task envelope, input frames, and guest/runtime budgets are unchanged.
    f.app.channel_tasks.job.as_mut().unwrap().max_output_bytes = 4;
    for sequence in 1u64..=5 {
        ok(append(
            &mut f.app,
            &prepared.key,
            sequence,
            &payload(sequence),
            &sequence.to_le_bytes(),
        ));
    }
    ok(run(&mut f.app, &prepared.key, &prepared.directory));
    let completed = wait_native_joins(&mut f.app, &prepared.key);
    assert_eq!(completed.phase, 3);
    assert_eq!(completed.task_state, 4);
    assert_eq!(
        completed.task_error,
        "channel output exceeds the private result bound"
    );
    assert!(completed.output.is_empty());
    assert_eq!(completed.last_acked, 5);
    assert_eq!(completed.source_frames, 5);
    assert_eq!(completed.source_bytes, 163_840);
    assert!(original.is_revoked());
    assert_eq!(
        f.app.state.local().unwrap().host.binding(),
        original_binding
    );
    assert!(f.app.local_state_mut().is_err());
    let job = f.app.channel_tasks.job.as_ref().unwrap();
    assert!(
        job.report.is_none(),
        "rejected private output is never delivered"
    );
    assert!(!job.close_requested && !job.disconnected);
    let endpoint = job.broker.endpoint();
    let scope: [u8; 32] = prepared.key.as_slice().try_into().unwrap();
    let reopened = Store::open_existing(
        &f.directory.path().join("store.sqlite3"),
        Default::default(),
    )
    .unwrap();
    for sequence in 1u64..=5 {
        let receipt = reopened
            .channel_ack_receipt(&scope, &endpoint.source_epoch, sequence)
            .unwrap()
            .unwrap();
        let frame = Frame::decode(&receipt.frame_wire).unwrap();
        assert_eq!(frame.sequence, sequence);
        assert_eq!(frame.bytes, payload(sequence));
        assert_eq!(frame.cursor, sequence.to_le_bytes());
    }
    assert!(
        run(&mut f.app, &prepared.key, &prepared.directory)
            .error
            .contains("already admitted")
    );
    assert_eq!(close(&mut f.app, &prepared.key).phase, 5);
    assert!(f.app.channel_tasks.job.as_ref().unwrap().report.is_none());
    assert!(f.app.channel_tasks.job.as_ref().unwrap().disconnected);
    eprintln!(
        "native pinned wire private_output_bound=4 primary_error=private_result_bound original_owner_returned=true original_control_revoked=true phase_before_explicit_close=3 ack=5 source_bytes=163840 producer_joined=true executor_joined=true maintenance_repair=true no_replay=true production_binding=false"
    );
}

#[test]
#[ignore = "requires the actual pinned reusable Directory003 SDK package"]
fn pinned_original_catalog_stop_never_claims_suspended_import_proof() {
    let mut f = Fixture::new(actual_package(false));
    f.bind_synthetic();
    f.enable();
    let mut request = f.prepare(22, Kind::Events, 0, 32_768);
    request.duplex = true;
    request.budget.max_messages = 5;
    request.budget.max_requests = 11;
    let prepared = ok(prepare(&mut f.app, &request));
    let revision = f.revision();
    let original = original_control(&f.app);
    let started = ok(run(&mut f.app, &prepared.key, &prepared.directory));
    assert_eq!(started.phase, 2);
    assert!(!started.worker_joined);
    assert!(
        f.app.state.local().is_err(),
        "actual executor owns the original state"
    );
    // Stop immediately after the actual native ownership handoff. Scheduling
    // may stop the instance before Wasmi admission. No sleep or test latch
    // manufactures an import observation, and this is not suspended-import
    // qualification. The recorded original TaskReport is the exact outcome.
    let reply = call(&mut f.app, wire::Action::PluginApprove, |mut r| {
        r.set_id(&f.package.manifest().package_id);
        r.set_sha256(&f.package.digest());
        r.set_revision(revision);
        r.set_limit(0);
        r.init_approved_capabilities(0);
    });
    assert_eq!(
        reply.ui_code, 110,
        "catalog write waits for the same owner to return"
    );
    assert!(!reply.error.is_empty());
    assert!(reply.state.is_none());
    assert!(original.is_revoked());
    assert!(f.app.channel_tasks.job.as_ref().unwrap().close_requested);
    let deadline = Instant::now() + WAIT;
    let stopped = loop {
        let state = status(&mut f.app, &prepared.key);
        if state.phase == 5 {
            break state;
        }
        assert!(
            Instant::now() < deadline,
            "original stop/join pending: {state:?}"
        );
        thread::sleep(Duration::from_millis(1));
    };
    assert!(stopped.worker_joined);
    assert!(stopped.resource_reclaimed);
    assert_eq!(stopped.cleanup_proof, CleanupProof::Joined as u16);
    assert_eq!(stopped.task_state, 4);
    assert!(stopped.output.is_empty());
    assert_eq!(stopped.last_acked, 0);
    assert_eq!(stopped.source_bytes, 0);
    assert!(
        f.app.state.local().is_ok(),
        "only the actual joined original executor restored its owner"
    );
    assert!(
        f.app.local_state_mut().is_err(),
        "unsupported protected maintenance still requires repair"
    );
    let job = f.app.channel_tasks.job.as_ref().unwrap();
    let report = &job.report.as_ref().unwrap().execution;
    assert!(matches!(
        report.outcome,
        Err(morrow_plugin_runtime::Fault::PackageBinding
            | morrow_plugin_runtime::Fault::InactiveConnection
            | morrow_plugin_runtime::Fault::Cancelled)
    ));
    assert!(report.host_calls <= 1);
    assert!(job.report.as_ref().unwrap().output.is_none());
    assert_eq!(
        job.broker.snapshot().producer_outcome,
        ProducerOutcome::Unknown
    );
    eprintln!(
        "native channel fixture original_catalog_stop=true execution={:?} calls={} actual_owner_handoff=true ack=0 producer_joined=true executor_joined=true protected_maintenance=unsupported wire_task_state=Unknown no_replay=true suspended_import_qualification=false",
        report.outcome, report.host_calls
    );
}
