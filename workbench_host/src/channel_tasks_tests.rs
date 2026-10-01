//! Explicit production-candidate tests with a real caller-selected directory package.
//! These do not replace supervised process/UI qualification or claim producer joins.
use super::*;
use crate::{host_capnp as wire, protocol};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use morrow_core::{
    channel::{Action, Request},
    plugin_package::Package,
};

struct Setup {
    app: Workbench,
    package: Package,
    _directory: tempfile::TempDir,
}
impl Setup {
    fn new() -> Self {
        let package_path = std::env::var_os("MORROW_CHANNEL_DIRECTORY_PACKAGE")
            .expect("qualification must supply the actual directory .mplugin artifact");
        let package = Package::decode(&std::fs::read(package_path).unwrap()).unwrap();
        assert_eq!(
            package.manifest().transform_handlers[0].input_type,
            DIRECTORY_INPUT_TYPE
        );
        let directory = tempfile::tempdir().unwrap();
        let mut app = Workbench::open(&directory.path().join("store.sqlite3"), None).unwrap();
        // Trusted native unit entry; full supervisor proof is exercised separately in Dart.
        app.bind_supervised_channel_owner().unwrap();
        let revision = app
            .state
            .local()
            .unwrap()
            .manager
            .as_ref()
            .unwrap()
            .revision();
        app.import_plugin_bytes(package.archive(), &package.digest(), revision)
            .unwrap();
        let revision = app
            .state
            .local()
            .unwrap()
            .manager
            .as_ref()
            .unwrap()
            .revision();
        app.configure_external(
            &package.manifest().package_id,
            &package.digest(),
            revision,
            &[],
            true,
        )
        .unwrap();
        Self {
            app,
            package,
            _directory: directory,
        }
    }
    fn prepare(&self, submission: u8) -> Prepare {
        let budget = Budget::from_proto(
            self.package
                .channel_declaration()
                .unwrap()
                .budget
                .as_ref()
                .unwrap(),
        )
        .unwrap();
        Prepare {
            submission: [submission; 32],
            package_id: self.package.manifest().package_id.clone(),
            package_digest: self.package.digest(),
            registry_revision: self
                .app
                .state
                .local()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
            handler: self.package.channel_declaration().unwrap().handlers[0].clone(),
            kind: Kind::ByteStream,
            duplex: false,
            budget: Budget::from_proto(
                self.package
                    .channel_declaration()
                    .unwrap()
                    .budget
                    .as_ref()
                    .unwrap(),
            )
            .unwrap(),
            lifetime_ms: budget.max_duration_ms.min(30_000) as u32,
            frame_count: 1,
            total_bytes: 1,
        }
    }
}
fn wire_call(
    app: &mut Workbench,
    action: wire::Action,
    fill: impl FnOnce(wire::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    fill(request);
    protocol::respond(app, &serialize::write_message_to_words(&message)).unwrap()
}
fn error(bytes: &[u8]) -> String {
    let mut remaining = bytes;
    let message =
        serialize::read_message_from_flat_slice(&mut remaining, ReaderOptions::new()).unwrap();
    message
        .get_root::<wire::response::Reader>()
        .unwrap()
        .get_error()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
#[ignore = "requires the actual production directory package via MORROW_CHANNEL_DIRECTORY_PACKAGE"]
fn production_wire_rejects_positive_inbound_duplex_before_any_grant_or_thread() {
    let mut setup = Setup::new();
    let p = setup.prepare(71);
    let revision = p.registry_revision;
    let response = wire_call(&mut setup.app, wire::Action::ChannelPrepare, |request| {
        let mut out = request.init_channel_prepare();
        out.set_submission(&p.submission);
        out.set_package_id(&p.package_id);
        out.set_package_digest(&p.package_digest);
        out.set_registry_revision(p.registry_revision);
        out.set_handler(&p.handler);
        out.set_kind(1);
        out.set_duplex(true);
        out.set_lifetime_ms(p.lifetime_ms);
        out.set_frame_count(1);
        out.set_total_bytes(1);
        let mut b = out.init_budget();
        b.set_max_channels(p.budget.max_channels);
        b.set_max_frame_bytes(p.budget.max_frame_bytes);
        b.set_max_bytes(p.budget.max_bytes);
        b.set_max_messages(p.budget.max_messages);
        b.set_max_requests(p.budget.max_requests);
        b.set_max_duration_ms(p.budget.max_duration_ms);
    });
    assert!(error(&response).contains("invalid, repeated or exhausted local source preparation"));
    assert!(setup.app.channel_tasks.job.is_none());
    assert!(setup.app.channel_tasks.seen.is_empty());
    assert_eq!(setup.app.channel_tasks.reserved_bytes, 0);
    assert_eq!(
        setup
            .app
            .state
            .local()
            .unwrap()
            .manager
            .as_ref()
            .unwrap()
            .revision(),
        revision
    );
    setup.app.finish().unwrap();
}

#[test]
#[ignore = "requires the actual production directory package via MORROW_CHANNEL_DIRECTORY_PACKAGE"]
fn production_foreign_keys_after_real_request_exhaustion_cannot_stop_original_control() {
    let mut setup = Setup::new();
    let p = setup.prepare(72);
    let state = setup.app.prepare_channel(p).unwrap();
    let key = state.key;
    // Exhaust the original host budget through real serialized status calls;
    // no counter assignment or simulated callback outcome is used.
    while setup.app.channel_tasks.requests < MAX_HOST_REQUESTS {
        let response = wire_call(&mut setup.app, wire::Action::ChannelStatus, |mut r| {
            r.set_channel_key(&key)
        });
        assert!(error(&response).is_empty());
    }
    let mut foreign = key;
    foreign[0] ^= 1;
    for action in [wire::Action::ChannelStatus, wire::Action::ChannelClose] {
        let response = wire_call(&mut setup.app, action, |mut r| r.set_channel_key(&foreign));
        assert!(error(&response).contains("stale local channel job key"));
    }
    let job = setup.app.channel_tasks.job.as_ref().unwrap();
    assert!(!job.close_requested);
    let broker = job.broker.clone();
    let instance = job.instance.clone();
    let endpoint = broker.endpoint();
    let request = Request {
        call_id: [73; 32],
        reference: endpoint.reference,
        source_epoch: endpoint.source_epoch,
        action: Action::Receive {
            last_acked: 0,
            credit_bytes: 64 * 1024,
        },
    };
    let owner = setup.app.state.local_mut().unwrap();
    let response = broker
        .dispatch(
            owner.manager.as_ref().unwrap(),
            &mut owner.host,
            &instance,
            &request,
            now(owner.start),
        )
        .unwrap();
    assert_eq!(response.status, Status::Idle); // actual original gate remains active
    let closed = setup.app.close_channel(&key).unwrap();
    assert_eq!(closed.phase, 5);
    assert!(closed.resource_reclaimed);
    assert_eq!(closed.cleanup_proof, CleanupProof::NoProducer);
    assert!(!closed.worker_joined);
    setup.app.finish().unwrap();
}

#[test]
#[ignore = "requires the actual production directory package via MORROW_CHANNEL_DIRECTORY_PACKAGE"]
fn production_never_started_close_proves_no_producer_without_faking_executor_join() {
    let mut setup = Setup::new();
    let p = setup.prepare(74);
    let state = setup.app.prepare_channel(p).unwrap();
    assert_eq!(state.cleanup_proof, CleanupProof::Pending);
    assert!(!state.resource_reclaimed);
    let response = wire_call(&mut setup.app, wire::Action::ChannelClose, |mut r| {
        r.set_channel_key(&state.key)
    });
    assert!(error(&response).is_empty());
    let state = setup.app.channel_status(&state.key).unwrap();
    assert_eq!(state.phase, 5);
    assert_eq!(state.cleanup_proof, CleanupProof::NoProducer);
    assert!(state.resource_reclaimed);
    assert!(!state.worker_joined);
    assert!(setup.app.state.local().is_ok());
    setup.app.finish().unwrap();
}
