//! Real application service admission, original-owner commands and reclamation on Windows.
use super::*;
use crate::{Workbench, host_capnp as wire, protocol, test_common as common};
use morrow_core::{
    io::Header,
    plugin_package::{
        Package,
        io::{self, IoCapability},
        proto::TransformHandler,
    },
    service::{self, Invocation, Reply, Request, Response},
    service_authority::{self, Record as Authority, proto as authority},
    service_config::{self, Config, proto as config},
    service_record::{self, Policy},
    ui::{Document, Event, EventKind},
};
use morrow_plugin_runtime::{
    io_binding::ServiceRunBudget,
    io_jobs::{JobLimits, OwnerCommandPoll},
    service_content::scope_digest,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ID: &str = "org.example.workbench.application.service";
const SERVICE: &str = "application.service";
const HANDLER: &str = "application.serve";
const CONFIG: &str = "application-service";
const AUTH: [u8; 32] = [42; 32];
const PUBLICATION: [u8; 32] = [43; 32];
const TOKEN: &str = "synthetic-application-service-bearer-123456789";
const FIRST_KEY: &str = "application-service-before";
const SECOND_KEY: &str = "application-service-after";
const WAIT: Duration = Duration::from_secs(15);

fn policy() -> Policy {
    Policy {
        namespace: [19; 32],
        retention_ms: 30_000,
    }
}
fn expected(key: &str, body: &[u8]) -> Request {
    let invocation = Invocation {
        service: SERVICE.into(),
        handler: HANDLER.into(),
        principal: "alice".into(),
        method: "POST".into(),
        target: "/api".into(),
        headers: vec![Header {
            name: "morrow-content-scope".into(),
            value: scope_digest(&[])
                .unwrap()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
        }],
        body: body.to_vec(),
    };
    Request::encode(
        service_record::call_id(&policy(), key, &invocation).unwrap(),
        &invocation,
    )
    .unwrap()
}
fn service_package() -> Package {
    let first = expected(FIRST_KEY, b"before");
    let second = expected(SECOND_KEY, b"after");
    let response = |request: &Request, body: &[u8]| {
        Response::encode(
            request,
            &Reply {
                status: 202,
                headers: vec![],
                body: body.to_vec(),
            },
        )
        .unwrap()
    };
    let first_reply = response(&first, b"executed-before");
    let second_reply = response(&second, b"executed-after");
    let quoted = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("\\{byte:02x}"))
            .collect::<String>()
    };
    // Match complete authenticated request frames, including distinct durable call IDs.
    let wasm = wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (memory (export "memory") 5)
      (data (i32.const 131072) "{first}") (data (i32.const 163840) "{second}")
      (data (i32.const 196608) "{first_reply}") (data (i32.const 229376) "{second_reply}")
      (func $matches (param $base i32) (param $size i32) (param $actual i32) (result i32)
        (local $i i32)
        local.get $actual local.get $size i32.ne if i32.const 0 return end
        (loop $check
          local.get $i i32.load8_u local.get $base local.get $i i32.add i32.load8_u
          i32.ne if i32.const 0 return end
          local.get $i i32.const 1 i32.add local.tee $i local.get $size i32.lt_u br_if $check)
        i32.const 1)
      (func (export "morrow_run") (result i32) (local $size i32)
        i32.const 0 i32.const 131072 call $read local.set $size
        i32.const 131072 i32.const {first_size} local.get $size call $matches
        if i32.const 196608 i32.const {first_reply_size} call $done drop
        else
          i32.const 163840 i32.const {second_size} local.get $size call $matches
          i32.eqz if unreachable end
          i32.const 229376 i32.const {second_reply_size} call $done drop
        end i32.const 0))"#,
        first = quoted(first.bytes()),
        second = quoted(second.bytes()),
        first_reply = quoted(&first_reply),
        second_reply = quoted(&second_reply),
        first_size = first.bytes().len(),
        second_size = second.bytes().len(),
        first_reply_size = first_reply.len(),
        second_reply_size = second_reply.len(),
    ))
    .unwrap();
    let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    let mut declaration = io::declaration(caps().into_iter().collect(), vec![HANDLER.into()]);
    declaration.service_schema_sha256 = service::schema_digest().to_vec();
    let limits = declaration.budget.as_mut().unwrap();
    limits.max_resources = 8;
    limits.max_jobs = 4;
    limits.max_job_bytes = 1024 * 1024;
    limits.max_bytes = 4 * 1024 * 1024;
    declaration.service_run = Some(io::proto::ServiceRunProfile {
        schema_version: io::SERVICE_RUN_VERSION,
        max_duration_ms: 120_000,
        budget: Some(io::proto::ServiceRunBudget {
            schema_version: io::SERVICE_RUN_BUDGET_VERSION,
            max_jobs: 64,
            max_bytes: 4 * 1024 * 1024,
        }),
    });
    manifest.required_features.extend([
        io::FEATURE.into(),
        io::SERVICE_RUN_FEATURE.into(),
        io::SERVICE_RUN_BUDGET_FEATURE.into(),
    ]);
    manifest.io_declaration = Some(declaration);
    Package::build(manifest, &wasm).unwrap()
}
fn caps() -> BTreeSet<IoCapability> {
    BTreeSet::from([IoCapability::HttpListen, IoCapability::HttpPublish])
}
fn workbench_package() -> Package {
    let original = common::package();
    let mut manifest = original.manifest().clone();
    manifest.transform_handlers.extend([
        TransformHandler {
            handler: "ui.form".into(),
            input_type: "text.utf8".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 32,
            max_output_bytes: 65536,
        },
        TransformHandler {
            handler: "ui.edit".into(),
            input_type: "morrow.ui.event.v1".into(),
            output_type: "morrow.ui.document.v1".into(),
            max_input_bytes: 65536,
            max_output_bytes: 65536,
        },
    ]);
    Package::build(manifest, original.module()).unwrap()
}
struct Fixture {
    app: Workbench,
    dir: tempfile::TempDir,
    package_digest: [u8; 32],
    config_digest: [u8; 32],
    registry_revision: u64,
}
impl Fixture {
    fn new(address: SocketAddr) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut app = Workbench::open_managed(dir.path(), Some(workbench_package())).unwrap();
        let package = service_package();
        let package_digest = package.digest();
        let state = app.local_state_mut().unwrap();
        state.catalog.as_ref().unwrap().install(&package).unwrap();
        let manager = state.manager.as_mut().unwrap();
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, package_digest, caps(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, package_digest, true, manager.revision())
            .unwrap();
        let registry_revision = manager.revision();
        let configuration = Config::encode(config::Configuration {
            schema_version: service_config::VERSION,
            id: CONFIG.into(),
            revision: 1,
            namespace: policy().namespace.to_vec(),
            retention_ms: policy().retention_ms,
            service: SERVICE.into(),
            handler: HANDLER.into(),
            package_sha256: package_digest.to_vec(),
            disabled: false,
            principals: vec![config::Principal {
                id: "alice".into(),
                authentication_reference: AUTH.to_vec(),
                content_scopes: vec![],
            }],
            approval_references: vec![PUBLICATION.to_vec()],
        })
        .unwrap();
        let config_digest = configuration.digest();
        let utc = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let authentication = Authority::encode(authority::Record {
            schema_version: service_authority::VERSION,
            reference: AUTH.to_vec(),
            revision: 1,
            created_ms: utc - 1000,
            expires_ms: utc + 600_000,
            disabled: false,
            kind: Some(authority::record::Kind::Authentication(
                authority::Authentication {
                    principal_id: "alice".into(),
                    token_sha256: Sha256::digest(TOKEN.as_bytes()).to_vec(),
                },
            )),
        })
        .unwrap();
        let publication = Authority::encode(authority::Record {
            schema_version: service_authority::VERSION,
            reference: PUBLICATION.to_vec(),
            revision: 1,
            created_ms: utc - 1000,
            expires_ms: utc + 600_000,
            disabled: false,
            kind: Some(authority::record::Kind::Publication(
                authority::Publication {
                    config_id: CONFIG.into(),
                    config_sha256: config_digest.to_vec(),
                    listen_address: address.to_string(),
                    tls_required: false,
                    method: "POST".into(),
                    path: "/api".into(),
                    query_path: "/history".into(),
                },
            )),
        })
        .unwrap();
        state.host.prepare_write().unwrap();
        let store = state.host.store_local_mut();
        store.save_service_config_local(&configuration, 0).unwrap();
        store
            .save_service_authority_local(&authentication, 0)
            .unwrap();
        store.save_service_authority_local(&publication, 0).unwrap();
        state.host.flush_pending().unwrap();
        Self {
            app,
            dir,
            package_digest,
            config_digest,
            registry_revision,
        }
    }
    fn options(&self, submission: u8) -> ServiceStart {
        ServiceStart {
            submission: [submission; 32],
            config_id: CONFIG.into(),
            config_digest: self.config_digest,
            config_revision: 1,
            publication: PUBLICATION,
            publication_revision: 1,
            package_id: ID.into(),
            package_digest: self.package_digest,
            registry_revision: self.registry_revision,
            lifetime: Duration::from_secs(120),
            budget: ServiceRunBudget {
                max_jobs: 8,
                max_bytes: 4 * 1024 * 1024,
            },
            limits: JobLimits::new(4, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
            network_limits: morrow_network_node::Limits {
                max_request_bytes: 65536,
                max_response_bytes: 65536,
                max_header_bytes: 16384,
                max_concurrent: 2,
                timeout: Duration::from_secs(2),
            },
        }
    }
    fn start(&mut self) -> TaskKey {
        let options = self.options(1);
        self.app.start_service(options).unwrap()
    }
}
fn running(app: &mut Workbench, key: TaskKey) -> SocketAddr {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = app.service_status(key).unwrap();
        match snapshot.phase {
            ServicePhase::Running => {
                assert_eq!(snapshot.bind, Some(Ok(())));
                return snapshot.address.unwrap();
            }
            ServicePhase::Starting => {}
            _ => panic!("service failed to start: {:?}", snapshot.bind),
        }
        assert!(Instant::now() < deadline, "service did not bind");
        thread::sleep(Duration::from_millis(2));
    }
}
fn exited(app: &mut Workbench, key: TaskKey) -> ServiceSnapshot {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = app.service_status(key).unwrap();
        if matches!(snapshot.phase, ServicePhase::Exited) {
            assert!(
                snapshot.task.exit.is_some(),
                "Exited requires actual owner recovery diagnostics"
            );
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "service did not return original owner"
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn post(address: SocketAddr, key: &str, body: &[u8]) -> Vec<u8> {
    let mut socket = TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap();
    socket.set_read_timeout(Some(WAIT)).unwrap();
    socket.set_write_timeout(Some(WAIT)).unwrap();
    write!(socket, "POST /api HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {key}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    socket.write_all(body).unwrap();
    let mut output = Vec::new();
    socket.read_to_end(&mut output).unwrap();
    assert!(
        output.starts_with(b"HTTP/1.1 202 "),
        "{}",
        String::from_utf8_lossy(&output)
    );
    output
}
fn frame(action: wire::Action, configure: impl FnOnce(wire::request::Builder<'_>)) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut request = message.init_root::<wire::request::Builder>();
    request.set_version(1);
    request.set_digest(&protocol::digest());
    request.set_action(action);
    configure(request);
    capnp::serialize::write_message_to_words(&message)
}
struct CommandReply {
    revision: u64,
    generation: u64,
    serial: u64,
    payload: Vec<u8>,
}
fn command(app: &mut Workbench, key: TaskKey, input: Vec<u8>) -> CommandReply {
    let mut handle = app.submit_service_command(key, input).unwrap();
    let deadline = Instant::now() + WAIT;
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(Instant::now() < deadline, "service owner command timed out");
        thread::sleep(Duration::from_millis(2));
    }
    let bytes = zeroize::Zeroizing::new(handle.read().unwrap().unwrap());
    let mut slice = bytes.as_slice();
    let message =
        capnp::serialize::read_message_from_flat_slice(&mut slice, Default::default()).unwrap();
    assert!(slice.is_empty());
    let response = message.get_root::<wire::response::Reader>().unwrap();
    assert_eq!(response.get_version(), 1);
    assert_eq!(response.get_digest().unwrap(), protocol::digest());
    assert!(
        response.get_error().unwrap().is_empty(),
        "{}",
        response.get_error().unwrap().to_str().unwrap()
    );
    assert_eq!(response.get_ui_code(), 0);
    CommandReply {
        revision: response.get_revision(),
        generation: response.get_ui_generation(),
        serial: response.get_ui_serial(),
        payload: response.get_payload().unwrap().to_vec(),
    }
}
fn mutation(operation: &str, revision: u64, idea: morrow_workbench_plugin::Idea) -> Vec<u8> {
    let mut request = crate::command(if revision == 0 {
        morrow_workbench_plugin::Action::Create
    } else {
        morrow_workbench_plugin::Action::Edit
    });
    let id = idea.id.clone();
    request.proposed = idea;
    let payload = morrow_workbench_plugin::codec::encode_request(&request).unwrap();
    frame(wire::Action::Mutate, |mut r| {
        r.set_id(&id);
        r.set_operation(operation);
        r.set_revision(revision);
        r.set_payload(&payload);
    })
}
fn guards(root: &Path) {
    use morrow_audit::{
        identity::LeaseError,
        library,
        sealer::Sealer,
        session::{self, OpenMode, SessionError},
    };
    let database = root.join("workbench.db");
    assert!(matches!(
        library::Registry::open(root),
        Err(library::Error::Busy)
    ));
    assert!(matches!(
        session::Session::open(&database, Default::default(), OpenMode::Existing),
        Err(SessionError::Busy)
    ));
    let key = morrow_audit::keys::Key::load(&session::key_path(&database).unwrap()).unwrap();
    assert!(matches!(Sealer::new(key), Err(LeaseError::Busy)));
    let catalog =
        morrow_core::plugin_package::catalog::Catalog::open(&root.join("plugin-manager/packages"))
            .unwrap();
    assert!(matches!(
        morrow_core::plugin_package::registry::Registry::open(
            &root.join("plugin-manager/state"),
            catalog
        ),
        Err(morrow_core::Error::StorageBusy)
    ));
}

#[test]
fn persistent_application_service_interleaves_real_http_content_and_original_ui() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let (identity, root) = {
        let state = fixture.app.local_state().unwrap();
        (
            state.host.binding(),
            state
                .pool
                .root(state.plugin.as_ref().unwrap())
                .unwrap()
                .connection()
                .binding(),
        )
    };
    let opened = fixture.app.ui_open("original").unwrap();
    assert!(opened.failure.is_none());
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    assert_eq!(fixture.app.service_status(key).unwrap().submission, [1; 32]);
    assert_eq!(fixture.app.http_submission(), None);
    guards(fixture.dir.path());
    assert!(fixture.app.local_state().is_err());
    assert!(post(address, FIRST_KEY, b"before").ends_with(b"executed-before"));
    let created = command(
        &mut fixture.app,
        key,
        mutation("service-create", 0, common::idea("service-card")),
    );
    assert_eq!(created.revision, 1);
    let mut idea = morrow_workbench_plugin::codec::decode_response(&created.payload)
        .unwrap()
        .idea;
    idea.title = "Edited during persistent service".into();
    let edited = command(&mut fixture.app, key, mutation("service-edit", 1, idea));
    assert_eq!(edited.revision, 2);
    let read = command(
        &mut fixture.app,
        key,
        frame(wire::Action::Read, |mut r| r.set_id("service-card")),
    );
    assert_eq!(read.revision, 2);
    assert_eq!(
        morrow_workbench_plugin::codec::decode_response(&read.payload)
            .unwrap()
            .idea
            .title,
        "Edited during persistent service"
    );
    let event = Event {
        view: "workbench-tools".into(),
        generation: opened.generation,
        revision: opened.revision,
        serial: 1,
        node: "text".into(),
        action: "text.edit".into(),
        kind: EventKind::EditText,
        text: "same live view".into(),
        checked: false,
    }
    .encode()
    .unwrap();
    let ui = command(
        &mut fixture.app,
        key,
        frame(wire::Action::UiEvent, |mut r| {
            r.set_offset(opened.generation);
            r.set_payload(&event);
        }),
    );
    assert_eq!(
        (ui.generation, ui.revision, ui.serial),
        (opened.generation, opened.revision + 1, 1)
    );
    let document = Document::decode(&ui.payload).unwrap();
    assert_eq!(
        document
            .nodes()
            .iter()
            .find(|node| node.id == "preview")
            .unwrap()
            .text,
        "SAME LIVE VIEW"
    );
    assert!(post(address, SECOND_KEY, b"after").ends_with(b"executed-after"));
    assert!(matches!(
        fixture.app.service_status(key).unwrap().phase,
        ServicePhase::Running
    ));
    fixture.app.cancel_io(key).unwrap();
    let stopped = exited(&mut fixture.app, key);
    assert_eq!(stopped.listener, Some(Ok(())));
    let exit = stopped.task.exit.unwrap();
    assert!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    let state = fixture.app.local_state().unwrap();
    assert_eq!(state.host.binding(), identity);
    assert_eq!(
        state
            .pool
            .root(state.plugin.as_ref().unwrap())
            .unwrap()
            .connection()
            .binding(),
        root
    );
    assert_eq!(fixture.app.read("service-card").unwrap().revision, 2);
    guards(fixture.dir.path());
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.ui_close(opened.generation).unwrap();
    fixture.app.finish().unwrap();
}

#[test]
fn stale_service_admission_does_not_move_owner_or_consume_valid_attempt() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let identity = fixture.app.local_state().unwrap().host.binding();
    for kind in 0..6 {
        let mut options = fixture.options(1);
        match kind {
            0 => options.registry_revision -= 1,
            1 => options.config_revision += 1,
            2 => options.publication_revision += 1,
            3 => options.package_digest = [8; 32],
            4 => options.config_digest = [8; 32],
            _ => options.publication = [8; 32],
        }
        assert!(fixture.app.start_service(options).is_err());
        assert_eq!(fixture.app.local_state().unwrap().host.binding(), identity);
        assert!(fixture.app.io_status().key.is_none());
        assert_eq!(
            fixture
                .app
                .local_state()
                .unwrap()
                .manager
                .as_ref()
                .unwrap()
                .revision(),
            fixture.registry_revision
        );
        guards(fixture.dir.path());
    }
    let key = fixture.start();
    running(&mut fixture.app, key);
    fixture.app.cancel_io(key).unwrap();
    exited(&mut fixture.app, key);
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
}

#[test]
fn occupied_port_returns_original_owner_and_requires_explicit_acknowledgement() {
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap();
    let mut fixture = Fixture::new(address);
    let identity = fixture.app.local_state().unwrap().host.binding();
    let key = fixture.start();
    let failed = exited(&mut fixture.app, key);
    assert!(failed.bind.unwrap().is_err());
    assert!(failed.address.is_none());
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), identity);
    let options = fixture.options(2);
    let error = fixture.app.start_service(options).unwrap_err();
    assert_eq!(
        error.downcast_ref::<crate::io_tasks::AccessError>(),
        Some(&crate::io_tasks::AccessError::UnacknowledgedTask)
    );
    guards(fixture.dir.path());
    fixture.app.acknowledge_io(key).unwrap();
    let replay = fixture.options(1);
    assert!(fixture.app.start_service(replay).is_err());
    assert!(fixture.app.io_status().key.is_none());
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), identity);
    drop(occupied);
    let options = fixture.options(2);
    let current = fixture.app.start_service(options).unwrap();
    assert_ne!(current, key);
    assert_eq!(running(&mut fixture.app, current), address);
    assert!(fixture.app.cancel_io(key).is_err());
    let mut foreign = Fixture::new("127.0.0.1:0".parse().unwrap());
    let foreign_key = foreign.start();
    running(&mut foreign.app, foreign_key);
    for wrong in [key, foreign_key] {
        for error in [
            fixture.app.cancel_io(wrong).unwrap_err(),
            fixture.app.repair_io(wrong).unwrap_err(),
            fixture.app.acknowledge_io(wrong).unwrap_err(),
            fixture.app.service_status(wrong).unwrap_err(),
        ] {
            assert_eq!(
                error.downcast_ref::<crate::io_tasks::AccessError>(),
                Some(&crate::io_tasks::AccessError::StaleTask)
            );
        }
    }
    assert!(
        fixture
            .app
            .submit_service_command(
                foreign_key,
                frame(wire::Action::Page, |mut r| r.set_limit(1))
            )
            .is_err()
    );
    assert!(matches!(
        fixture.app.service_status(current).unwrap().phase,
        ServicePhase::Running
    ));
    fixture.app.cancel_io(current).unwrap();
    exited(&mut fixture.app, current);
    fixture.app.acknowledge_io(current).unwrap();
    fixture.app.finish().unwrap();
    foreign.app.cancel_io(foreign_key).unwrap();
    exited(&mut foreign.app, foreign_key);
    foreign.app.acknowledge_io(foreign_key).unwrap();
    foreign.app.finish().unwrap();
}

#[test]
fn actual_service_deadline_closes_listener_and_returns_original_state() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let identity = fixture.app.local_state().unwrap().host.binding();
    let mut options = fixture.options(1);
    options.lifetime = Duration::from_millis(1500);
    options.network_limits.timeout = Duration::from_millis(200);
    let started = Instant::now();
    let key = fixture.app.start_service(options).unwrap();
    let address = running(&mut fixture.app, key);
    assert!(post(address, FIRST_KEY, b"before").ends_with(b"executed-before"));
    let expired = exited(&mut fixture.app, key);
    assert!(
        started.elapsed() >= Duration::from_millis(1000),
        "the service must not be immediately drained like short IO"
    );
    assert_eq!(expired.listener, Some(Ok(())));
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), identity);
    assert!(
        fixture
            .app
            .submit_service_command(key, frame(wire::Action::Page, |mut r| r.set_limit(1)))
            .is_err()
    );
    guards(fixture.dir.path());
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
}

fn observed_history(store: &morrow_core::store::Store, digest: [u8; 32]) -> Vec<u8> {
    // The lookup identity depends on namespace/key/request, not this reconstruction's timestamp.
    let identity = service_record::RequestRecord::encode(
        &policy(),
        FIRST_KEY,
        &expected(FIRST_KEY, b"before"),
        1,
    )
    .unwrap()
    .command(digest)
    .unwrap();
    let record = store
        .lookup_io_intent(&identity.subject, &identity.operation_id)
        .unwrap()
        .unwrap();
    assert_eq!(record.phase(), morrow_core::io_intent::Phase::Observed);
    record.container().to_vec()
}

#[cfg(feature = "fault-injection")]
#[test]
fn service_seal_failure_requires_explicit_repair_without_replaying_http_or_content() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    fixture
        .app
        .create("before-seal-service", common::idea("seal-service-card"))
        .unwrap();
    let identity = {
        let state = fixture.app.local_state_mut().unwrap();
        state.host.flush_pending().unwrap();
        state.host.fail_next_seal_for_test();
        state.host.binding()
    };
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    assert!(post(address, FIRST_KEY, b"before").ends_with(b"executed-before"));
    fixture.app.cancel_io(key).unwrap();
    let failed = exited(&mut fixture.app, key);
    assert_eq!(
        failed.task.storage,
        crate::io_tasks::StoragePhase::RecoveryRequired
    );
    assert!(failed.task.exit.unwrap().maintenance.is_err());
    assert_eq!(failed.listener, Some(Ok(())));
    let (history, pending, card) = {
        let state = fixture.app.local_state().unwrap();
        assert_eq!(state.host.binding(), identity);
        let store = state.host.store_local();
        (
            observed_history(store, fixture.package_digest),
            store.pending_usage().unwrap(),
            store.card("seal-service-card").unwrap().unwrap().encode(),
        )
    };
    assert!(pending.0 > 0);
    assert_eq!(fixture.app.read("seal-service-card").unwrap().revision, 1);
    let options = fixture.options(2);
    let mutation_error = match fixture.app.local_state_mut() {
        Err(error) => error,
        Ok(_) => panic!("mutable state must require explicit repair"),
    };
    for error in [
        fixture.app.acknowledge_io(key).unwrap_err(),
        fixture.app.start_service(options).unwrap_err(),
        mutation_error,
    ] {
        assert_eq!(
            error.downcast_ref::<crate::io_tasks::AccessError>(),
            Some(&crate::io_tasks::AccessError::RecoveryRequired)
        );
    }
    assert_eq!(
        fixture
            .app
            .local_state()
            .unwrap()
            .host
            .store_local()
            .pending_usage()
            .unwrap(),
        pending
    );
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    let repaired = fixture.app.repair_io(key).unwrap();
    assert_eq!(repaired.storage, crate::io_tasks::StoragePhase::Reclaimed);
    assert_eq!(
        repaired.exit, failed.task.exit,
        "historical failure remains visible after repair"
    );
    let state = fixture.app.local_state().unwrap();
    assert_eq!(state.host.binding(), identity);
    let store = state.host.store_local();
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    assert_eq!(observed_history(store, fixture.package_digest), history);
    assert_eq!(
        store.card("seal-service-card").unwrap().unwrap().encode(),
        card
    );
    store.integrity_check().unwrap();
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    guards(fixture.dir.path());
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
}

#[test]
fn original_owner_config_disable_revokes_listener_and_commits_once_even_if_reply_is_unknown() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let identity = fixture.app.local_state().unwrap().host.binding();
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    assert!(post(address, FIRST_KEY, b"before").ends_with(b"executed-before"));
    let input = frame(wire::Action::ServiceConfigDisable, |mut request| {
        request.set_id(CONFIG);
        request.set_revision(1);
    });
    let mut handle = fixture.app.submit_service_command(key, input).unwrap();
    let deadline = Instant::now() + WAIT;
    while handle.poll() == OwnerCommandPoll::Pending {
        assert!(
            Instant::now() < deadline,
            "configuration update never reached a terminal delivery state"
        );
        thread::sleep(Duration::from_millis(2));
    }
    match handle.read() {
        Ok(Some(bytes)) => {
            let bytes = zeroize::Zeroizing::new(bytes);
            let mut slice = bytes.as_slice();
            let message =
                capnp::serialize::read_message_from_flat_slice(&mut slice, Default::default())
                    .unwrap();
            let response = message.get_root::<wire::response::Reader>().unwrap();
            assert!(response.get_error().unwrap().is_empty());
            let configs = response.get_service_configs().unwrap();
            assert_eq!(configs.len(), 1);
            assert_eq!(configs.get(0).get_revision(), 2);
            assert!(configs.get(0).get_disabled());
        }
        Err(morrow_plugin_runtime::io_jobs::OwnerCommandError::Unknown) => {
            assert!(
                handle.is_started(),
                "only an actually started command may have an unknown outcome"
            );
        }
        Ok(None) => panic!("configuration delivery was still pending"),
        Err(error) => panic!("unexpected configuration delivery: {error}"),
    }
    // No explicit cancellation: the stored configuration invalidates the original live grants.
    let stopped = exited(&mut fixture.app, key);
    assert_eq!(stopped.listener, Some(Ok(())));
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    let state = fixture.app.local_state().unwrap();
    assert_eq!(state.host.binding(), identity);
    let config = state
        .host
        .store_local()
        .load_service_config(CONFIG)
        .unwrap()
        .unwrap();
    assert_eq!(config.value().revision, 2);
    assert!(config.value().disabled);
    let history = observed_history(state.host.store_local(), fixture.package_digest);
    for _ in 0..3 {
        fixture.app.service_status(key).unwrap();
    }
    assert_eq!(
        fixture
            .app
            .local_state()
            .unwrap()
            .host
            .store_local()
            .load_service_config(CONFIG)
            .unwrap()
            .unwrap()
            .container(),
        config.container()
    );
    assert_eq!(
        observed_history(
            fixture.app.local_state().unwrap().host.store_local(),
            fixture.package_digest
        ),
        history
    );
    fixture.app.acknowledge_io(key).unwrap();
    fixture.app.finish().unwrap();
}

#[test]
fn dropping_running_service_eventually_closes_socket_and_reopens_original_library_and_registry() {
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    assert!(post(address, FIRST_KEY, b"before").ends_with(b"executed-before"));
    let created = command(
        &mut fixture.app,
        key,
        mutation(
            "before-service-drop",
            0,
            common::idea("retained-after-service-drop"),
        ),
    );
    assert_eq!(created.revision, 1);
    let Fixture {
        app,
        dir,
        package_digest,
        registry_revision,
        config_digest,
    } = fixture;
    drop(app);
    // Observe eventual cleanup; a race-prone immediate lock assertion would prove nothing.
    let deadline = Instant::now() + WAIT;
    let (mut library, registry) = loop {
        match morrow_audit::library::Registry::open(dir.path()) {
            Ok(library) => {
                let catalog = morrow_core::plugin_package::catalog::Catalog::open(
                    &dir.path().join("plugin-manager/packages"),
                )
                .unwrap();
                match morrow_core::plugin_package::registry::Registry::open(
                    &dir.path().join("plugin-manager/state"),
                    catalog,
                ) {
                    Ok(registry) => break (library, registry),
                    Err(morrow_core::Error::StorageBusy) => {}
                    Err(error) => panic!("original plugin registry could not reopen: {error}"),
                }
            }
            Err(morrow_audit::library::Error::Busy) => {}
            Err(error) => panic!("original library could not reopen: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "dropped service did not release original owners"
        );
        thread::sleep(Duration::from_millis(2));
    };
    assert!(TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err());
    assert_eq!(registry.revision(), registry_revision);
    assert_eq!(registry.selection(ID).unwrap().digest, package_digest);
    let session = library.open_session(Default::default()).unwrap();
    let store = session.runtime_ref().store_local();
    let card = store.card("retained-after-service-drop").unwrap().unwrap();
    assert_eq!(card.summary().revision, 1);
    assert_eq!(card.summary().title, "记录 retained-after-service-drop");
    assert_eq!(
        store.load_service_config(CONFIG).unwrap().unwrap().digest(),
        config_digest
    );
    observed_history(store, package_digest);
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
}
