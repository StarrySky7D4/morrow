//! Real Rust guest, original audited Store, explicit saved endpoint, real loopback HTTP.
use super::*;
use crate::{
    endpoint_control::{EndpointInfo, EndpointUpdate},
    http_tasks::{HTTP_FORWARD_PROFILE, HttpStart},
    io_tasks::{AccessError, StoragePhase, TaskKey},
};
use morrow_core::{
    io::{Request, Response, Status},
    io_evidence::Kind,
    io_intent::Phase,
    outbound_authority::proto,
    plugin_package::{
        Package,
        io::{self, IoCapability},
    },
};
use morrow_plugin_runtime::io_jobs::Poll;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const ID: &str = "test.real-http-forward";
const WAIT: Duration = Duration::from_secs(10);
struct Setup {
    app: Workbench,
    package: Package,
    endpoint: EndpointInfo,
    registry_revision: u64,
    dir: tempfile::TempDir,
}
impl Setup {
    fn new(origin: &str, profile: bool) -> Self {
        let wasm =
            std::fs::read(std::env::var("MORROW_HTTP_FORWARD_WASM").expect(
                "set MORROW_HTTP_FORWARD_WASM to the actual compiled Rust HTTP-forward guest",
            ))
            .unwrap();
        Self::from_wasm(origin, profile, &wasm)
    }
    fn from_wasm(origin: &str, profile: bool, wasm: &[u8]) -> Self {
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", wasm, vec![]);
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(io::declaration(
            vec![IoCapability::HttpRequest],
            vec![
                if profile {
                    HTTP_FORWARD_PROFILE
                } else {
                    "test.unrelated.profile"
                }
                .into(),
            ],
        ));
        let package = Package::build(manifest, wasm).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let mut app = Workbench::open_managed(dir.path(), None).unwrap();
        app.local_state()
            .unwrap()
            .catalog
            .as_ref()
            .unwrap()
            .install(&package)
            .unwrap();
        let manager = app.local_state_mut().unwrap().manager.as_mut().unwrap();
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(
                ID,
                package.digest(),
                [IoCapability::HttpRequest].into(),
                manager.revision(),
            )
            .unwrap();
        manager
            .set_enabled(ID, package.digest(), true, manager.revision())
            .unwrap();
        let endpoint = app
            .save_endpoint(EndpointUpdate {
                reference: vec![],
                expected_revision: 0,
                registry_revision: app
                    .local_state()
                    .unwrap()
                    .manager
                    .as_ref()
                    .unwrap()
                    .revision(),
                lifetime_days: 1,
                policy: proto::Endpoint {
                    package_id: ID.into(),
                    package_sha256: package.digest().to_vec(),
                    origin: origin.into(),
                    profile: 2,
                    methods: vec!["GET".into()],
                    credential_reference: vec![],
                    root_certificate: vec![],
                    max_request_bytes: 65536,
                    max_response_bytes: 65536,
                    max_header_bytes: 16384,
                    max_concurrent: 1,
                    timeout_ms: 5000,
                    max_frame_bytes: 128 * 1024,
                },
            })
            .unwrap();
        let registry_revision = app
            .local_state()
            .unwrap()
            .manager
            .as_ref()
            .unwrap()
            .revision();
        Self {
            registry_revision,
            app,
            package,
            endpoint,
            dir,
        }
    }
    fn request(&self, byte: u8) -> HttpStart {
        HttpStart {
            submission: [byte; 32],
            endpoint: self.endpoint.reference,
            endpoint_revision: self.endpoint.revision,
            package_digest: self.package.digest(),
            registry_revision: self.registry_revision,
            method: "GET".into(),
            target: "/forward".into(),
            headers: vec![],
            body: vec![],
            timeout_ms: 5000,
        }
    }
}
struct Server {
    origin: String,
    calls: Arc<AtomicUsize>,
    connections: Arc<AtomicUsize>,
    release: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(body: Vec<u8>, held: bool) -> Self {
        Self::with_header(body, held, b"real-rust-guest".to_vec())
    }
    fn with_header(body: Vec<u8>, held: bool, header_value: Vec<u8>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let connections = Arc::new(AtomicUsize::new(0));
        let connected = connections.clone();
        let release = Arc::new(AtomicBool::new(!held));
        let released = release.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let join = thread::spawn(move || {
            while !stopping.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        connected.fetch_add(1, Ordering::SeqCst);
                        socket.set_nonblocking(false).unwrap();
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        socket
                            .set_write_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        let mut next = [0; 1024];
                        while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                            let n = socket.read(&mut next).unwrap();
                            assert!(n > 0 && bytes.len() + n < 65536);
                            bytes.extend_from_slice(&next[..n]);
                        }
                        assert!(bytes.starts_with(b"GET /forward HTTP/1.1\r\n"));
                        count.fetch_add(1, Ordering::SeqCst);
                        while !released.load(Ordering::SeqCst) && !stopping.load(Ordering::SeqCst) {
                            thread::sleep(Duration::from_millis(2));
                        }
                        if stopping.load(Ordering::SeqCst) {
                            break;
                        }
                        // A cancelled client may have closed its socket already.
                        let test_headers = if header_value.len() > 8192 {
                            let (left, right) = header_value.split_at(header_value.len() / 2);
                            format!(
                                "X-Test-A: {}\r\nX-Test-B: {}\r\n",
                                std::str::from_utf8(left).unwrap(),
                                std::str::from_utf8(right).unwrap()
                            )
                        } else {
                            format!(
                                "X-Test: {}\r\n",
                                std::str::from_utf8(&header_value).unwrap()
                            )
                        };
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n{test_headers}Connection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = socket
                            .write_all(header.as_bytes())
                            .and_then(|_| socket.write_all(&body));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("listener: {e}"),
                }
            }
        });
        Self {
            origin,
            calls,
            connections,
            release,
            stop,
            join: Some(join),
        }
    }
    fn received(&self) {
        let deadline = Instant::now() + WAIT;
        while self.calls.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Err(error) = self.join.take().unwrap().join()
            && !thread::panicking()
        {
            std::panic::resume_unwind(error);
        }
    }
}
fn ready(app: &mut Workbench, key: TaskKey) {
    let deadline = Instant::now() + WAIT;
    loop {
        match app.poll_io(key).unwrap().delivery {
            Some(Poll::Ready) => return,
            Some(Poll::Pending) => {}
            other => panic!("unexpected delivery {other:?}"),
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
}
fn reclaimed(app: &mut Workbench, key: TaskKey) {
    let deadline = Instant::now() + WAIT;
    loop {
        let snapshot = app.poll_io(key).unwrap();
        if let Some(exit) = snapshot.exit {
            assert_eq!(snapshot.storage, StoragePhase::Reclaimed);
            assert!(exit.disconnect.is_ok() && exit.maintenance.is_ok());
            return;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
}
fn operation(byte: u8) -> String {
    [byte; 32].iter().map(|v| format!("{v:02x}")).collect()
}

#[test]
fn saved_endpoint_real_rust_guest_http_result_and_evidence_survive_restart_without_replay() {
    let server = Server::new(b"rust-forwarded".to_vec(), false);
    let mut setup = Setup::new(&server.origin, true);
    let binding = setup.app.local_state().unwrap().host.binding();
    let request = setup.request(1);
    let key = setup.app.start_http(request).unwrap();
    ready(&mut setup.app, key);
    assert_eq!(setup.app.http_submission(), Some([1; 32]));
    assert_eq!(setup.app.io_status().storage, StoragePhase::Running);
    assert_eq!(
        setup
            .app
            .page("", 1)
            .err()
            .unwrap()
            .downcast_ref::<AccessError>(),
        Some(&AccessError::Busy)
    );
    assert!(setup.app.start_http(setup.request(1)).is_err());
    assert!(setup.app.read_io(key, 1).is_err());
    assert_eq!(setup.app.poll_io(key).unwrap().delivery, Some(Poll::Ready));
    let report = setup.app.read_io(key, 512 * 1024).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_ok());
    assert_eq!(report.calls, 1);
    assert!(!report.cancelled && !report.unknown);
    let response = report.http_response.unwrap();
    assert_eq!(response.status, Status::Completed);
    assert_eq!(response.http_status, 200);
    assert_eq!(response.body, b"rust-forwarded");
    reclaimed(&mut setup.app, key);
    assert_eq!(setup.app.local_state().unwrap().host.binding(), binding);
    let store = setup.app.local_state().unwrap().host.store_local();
    assert_eq!(
        store
            .lookup_io_intent(ID, &operation(1))
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
    let request = store
        .io_material(ID, &operation(1), Kind::Request)
        .unwrap()
        .unwrap();
    let response = store
        .io_material(ID, &operation(1), Kind::Response)
        .unwrap()
        .unwrap();
    let evidence = (request.digest(), response.digest());
    assert_eq!(
        Response::decode_http(
            &Request::decode(request.payload()).unwrap(),
            response.payload()
        )
        .unwrap()
        .body,
        b"rust-forwarded"
    );
    assert_eq!(store.pending_usage().unwrap(), (0, 0));
    store.integrity_check().unwrap();
    setup.app.acknowledge_io(key).unwrap();
    assert!(setup.app.start_http(setup.request(1)).is_err());
    setup.app.finish().unwrap();
    drop(setup.app);
    let mut app = Workbench::open_managed(setup.dir.path(), None).unwrap();
    let store = app.local_state().unwrap().host.store_local();
    assert_eq!(
        store
            .lookup_io_intent(ID, &operation(1))
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
    assert_eq!(
        store
            .io_material(ID, &operation(1), Kind::Request)
            .unwrap()
            .unwrap()
            .digest(),
        evidence.0
    );
    assert_eq!(
        store
            .io_material(ID, &operation(1), Kind::Response)
            .unwrap()
            .unwrap()
            .digest(),
        evidence.1
    );
    assert_eq!(app.io_status().storage, StoragePhase::Local);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    app.finish().unwrap();
}

#[test]
fn stale_metadata_disabled_endpoint_and_missing_forward_profile_send_nothing() {
    let server = Server::new(b"unreached".to_vec(), false);
    let mut setup = Setup::new(&server.origin, true);
    let mut request = setup.request(2);
    request.endpoint_revision += 1;
    assert!(setup.app.start_http(request).is_err());
    let mut request = setup.request(3);
    request.package_digest = [7; 32];
    assert!(setup.app.start_http(request).is_err());
    let mut request = setup.request(4);
    request.registry_revision -= 1;
    assert!(setup.app.start_http(request).is_err());
    assert_eq!(setup.app.io_status().storage, StoragePhase::Local);
    setup
        .app
        .disable_endpoint(&setup.endpoint.reference, 1)
        .unwrap();
    let mut request = setup.request(5);
    request.endpoint_revision = 2;
    assert!(setup.app.start_http(request).is_err());
    let mut missing = Setup::new(&server.origin, false);
    assert!(missing.app.start_http(missing.request(6)).is_err());
    assert_eq!(missing.app.io_status().storage, StoragePhase::Local);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
    setup.app.finish().unwrap();
    missing.app.finish().unwrap();
}

#[test]
fn cancelling_real_http_keeps_busy_until_actual_join_and_never_replays_submission() {
    let server = Server::new(b"late".to_vec(), true);
    let mut setup = Setup::new(&server.origin, true);
    let key = setup.app.start_http(setup.request(7)).unwrap();
    server.received();
    assert_eq!(
        setup
            .app
            .page("", 1)
            .err()
            .unwrap()
            .downcast_ref::<AccessError>(),
        Some(&AccessError::Busy)
    );
    let status = setup.app.cancel_io(key).unwrap();
    assert!(matches!(
        status.storage,
        StoragePhase::Stopping | StoragePhase::Reclaimed
    ));
    if status.exit.is_none() {
        assert!(setup.app.local_state().is_err());
    }
    ready(&mut setup.app, key);
    let report = setup.app.read_io(key, 512 * 1024).unwrap().unwrap();
    assert!(report.cancelled);
    assert!(report.http_response.is_none());
    reclaimed(&mut setup.app, key);
    setup.app.acknowledge_io(key).unwrap();
    assert!(setup.app.start_http(setup.request(7)).is_err());
    server.release.store(true, Ordering::SeqCst);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    setup.app.finish().unwrap();
}

fn wire_request(
    action: host_capnp::Action,
    fill: impl FnOnce(host_capnp::request::Builder<'_>),
) -> Vec<u8> {
    let mut message = capnp::message::Builder::new_default();
    let mut r = message.init_root::<host_capnp::request::Builder>();
    r.set_version(1);
    r.set_digest(&protocol::digest());
    r.set_action(action);
    fill(r);
    capnp::serialize::write_message_to_words(&message)
}
fn wire_start(request: &HttpStart) -> Vec<u8> {
    wire_request(host_capnp::Action::HttpStart, |r| {
        let mut h = r.init_http_start();
        h.set_submission(&request.submission);
        h.set_endpoint(&request.endpoint);
        h.set_endpoint_revision(request.endpoint_revision);
        h.set_package_digest(&request.package_digest);
        h.set_registry_revision(request.registry_revision);
        h.set_method(&request.method);
        h.set_target(&request.target);
        h.set_body(&request.body);
        h.set_timeout_ms(request.timeout_ms.try_into().unwrap());
        let mut headers = h.init_headers(request.headers.len().try_into().unwrap());
        for (i, header) in request.headers.iter().enumerate() {
            let mut value = headers.reborrow().get(i as u32);
            value.set_name(&header.name);
            value.set_value(&header.value);
        }
    })
}
fn parsed(bytes: &[u8]) -> capnp::message::Reader<capnp::serialize::OwnedSegments> {
    capnp::serialize::read_message(&mut &bytes[..], Default::default()).unwrap()
}
#[test]
fn private_protocol_preserves_submission_correlation_and_maximum_http_body_without_duplicate_guest_output()
 {
    let body: Vec<u8> = (0..morrow_core::io::MAX_PAYLOAD_BYTES)
        .map(|n| (n % 251) as u8)
        .collect();
    let header = vec![b'a'; 15 * 1024];
    let server = Server::with_header(body.clone(), false, header.clone());
    let mut setup = Setup::new(&server.origin, true);
    let start = wire_start(&setup.request(11));
    let output = protocol::respond(&mut setup.app, &start).unwrap();
    let message = parsed(&output);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    let state = response.get_io_state().unwrap();
    let key = state.get_key().unwrap().to_vec();
    assert_eq!(state.get_submission().unwrap(), [11; 32]);
    assert_eq!(key.len(), 32);
    let duplicate = protocol::respond(&mut setup.app, &start).unwrap();
    let message = parsed(&duplicate);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert!(!response.get_error().unwrap().is_empty());
    let status = protocol::respond(
        &mut setup.app,
        &wire_request(host_capnp::Action::IoStatus, |_| {}),
    )
    .unwrap();
    let message = parsed(&status);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert_eq!(
        response.get_io_state().unwrap().get_submission().unwrap(),
        [11; 32]
    );
    assert_eq!(response.get_io_state().unwrap().get_key().unwrap(), key);
    let deadline = Instant::now() + WAIT;
    loop {
        let output = protocol::respond(
            &mut setup.app,
            &wire_request(host_capnp::Action::IoPoll, |mut r| r.set_io_key(&key)),
        )
        .unwrap();
        let message = parsed(&output);
        let response = message.get_root::<host_capnp::response::Reader>().unwrap();
        assert!(response.get_error().unwrap().is_empty());
        let state = response.get_io_state().unwrap();
        assert_eq!(state.get_submission().unwrap(), [11; 32]);
        if state.get_delivery() == 2 {
            break;
        }
        assert_eq!(state.get_delivery(), 1);
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let output = protocol::respond(
        &mut setup.app,
        &wire_request(host_capnp::Action::IoRead, |mut r| r.set_io_key(&key)),
    )
    .unwrap();
    assert!(output.len() <= 128 * 1024);
    let message = parsed(&output);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    assert!(response.get_payload().unwrap().is_empty());
    let result = response.get_io_result().unwrap();
    assert!(
        result.get_present() && result.get_has_http(),
        "present={} http={} fault={} exit={} cancelled={} unknown={} calls={} charged={}",
        result.get_present(),
        result.get_has_http(),
        result.get_execution_fault(),
        result.get_exit_code(),
        result.get_cancelled(),
        result.get_unknown(),
        result.get_calls(),
        result.get_charged_bytes()
    );
    assert!(!result.get_cancelled() && !result.get_unknown());
    assert_eq!(result.get_execution_fault(), 0);
    assert_eq!(result.get_http_status(), 200);
    assert_eq!(result.get_calls(), 1);
    assert_eq!(result.get_body().unwrap(), body);
    let mut restored_header = Vec::new();
    for name in ["x-test-a", "x-test-b"] {
        let response_header = result
            .get_headers()
            .unwrap()
            .iter()
            .find(|v| v.get_name().unwrap().to_str().unwrap() == name)
            .unwrap();
        assert!(response_header.get_value().unwrap().len() <= 8192);
        restored_header.extend_from_slice(response_header.get_value().unwrap());
    }
    assert_eq!(restored_header, header);
    reclaimed(&mut setup.app, TaskKey::from_bytes(&key).unwrap());
    let output = protocol::respond(
        &mut setup.app,
        &wire_request(host_capnp::Action::IoAcknowledge, |mut r| {
            r.set_io_key(&key)
        }),
    )
    .unwrap();
    let message = parsed(&output);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    assert_eq!(response.get_io_state().unwrap().get_storage(), 0);
    assert!(
        response
            .get_io_state()
            .unwrap()
            .get_key()
            .unwrap()
            .is_empty()
    );
    let output = protocol::respond(&mut setup.app, &start).unwrap();
    let message = parsed(&output);
    assert!(
        !message
            .get_root::<host_capnp::response::Reader>()
            .unwrap()
            .get_error()
            .unwrap()
            .is_empty()
    );
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    setup.app.finish().unwrap();
}

#[test]
fn private_protocol_failed_admission_is_correlated_and_cannot_reuse_attempt_token() {
    let server = Server::new(b"must-not-send".to_vec(), false);
    let mut setup = Setup::new(&server.origin, true);
    let mut request = setup.request(12);
    request.registry_revision -= 1;
    for request in [request, setup.request(12)] {
        let output = protocol::respond(&mut setup.app, &wire_start(&request)).unwrap();
        let message = parsed(&output);
        let response = message.get_root::<host_capnp::response::Reader>().unwrap();
        assert!(!response.get_error().unwrap().is_empty());
    }
    let output = protocol::respond(
        &mut setup.app,
        &wire_request(host_capnp::Action::IoStatus, |_| {}),
    )
    .unwrap();
    let message = parsed(&output);
    let response = message.get_root::<host_capnp::response::Reader>().unwrap();
    assert!(response.get_error().unwrap().is_empty());
    let state = response.get_io_state().unwrap();
    assert_eq!(state.get_submission().unwrap(), [12; 32]);
    assert!(state.get_key().unwrap().is_empty());
    assert_eq!(state.get_storage(), 0);
    for action in [
        host_capnp::Action::IoPoll,
        host_capnp::Action::IoRead,
        host_capnp::Action::IoCancel,
        host_capnp::Action::IoRepair,
        host_capnp::Action::IoAcknowledge,
    ] {
        let output = protocol::respond(
            &mut setup.app,
            &wire_request(action, |mut r| r.set_io_key(&[99; 32])),
        )
        .unwrap();
        let message = parsed(&output);
        let response = message.get_root::<host_capnp::response::Reader>().unwrap();
        assert!(!response.get_error().unwrap().is_empty());
        assert_eq!(response.get_ui_code(), 113);
    }
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    assert_eq!(server.connections.load(Ordering::SeqCst), 0);
    setup.app.finish().unwrap();
}

/// Attack-only WAT fixture: preserve a valid core frame and its endpoint reference
/// while changing one user-selected field before the actual IO import.
fn tampering_guest(mode: &str) -> Vec<u8> {
    let mutate = match mode {
        "operation" => "i32.const 1680892976 local.get $n call $find i32.const 57 i32.store8",
        "path" => {
            "i32.const 1919903279 local.get $n call $find i32.const 1 i32.add i32.const 99 i32.store8"
        }
        "method" => {
            r#"
          i32.const 5522759 local.get $n call $find local.set $pos
          local.get $pos i64.const 76228242195780 i64.store
          i32.const 8 local.set $i
          block $changed
            loop $pointer
              local.get $i local.get $n i32.ge_u if unreachable end
              local.get $i i32.load i32.const 3 i32.and i32.const 1 i32.eq
              local.get $i i32.load offset=4 i32.const 34 i32.eq i32.and
              local.get $i i32.const 8 i32.add
              local.get $i i32.load i32.const 2 i32.shr_s i32.const 8 i32.mul i32.add
              local.get $pos i32.eq i32.and
              if local.get $i i32.const 58 i32.store offset=4 br $changed end
              local.get $i i32.const 8 i32.add local.set $i br $pointer
            end
          end"#
        }
        _ => unreachable!(),
    };
    wat::parse_str(format!(
        r#"(module
      (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
      (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
      (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 4)
      (func $find (param $word i32) (param $n i32) (result i32) (local $i i32)
        loop $scan
          local.get $i local.get $n i32.const 8 i32.sub i32.ge_u if unreachable end
          local.get $i i32.load local.get $word i32.eq if local.get $i return end
          local.get $i i32.const 1 i32.add local.set $i br $scan
        end unreachable)
      (func (export "morrow_run") (result i32) (local $n i32) (local $i i32) (local $pos i32)
        i32.const 0 i32.const 131072 call $read local.set $n
        {mutate}
        i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
        call $io call $done drop i32.const 0))"#
    ))
    .unwrap()
}
#[test]
fn valid_guest_frame_cannot_substitute_operation_path_or_another_allowed_http_method() {
    let server = Server::new(b"must-not-send".to_vec(), false);
    for mode in ["operation", "path", "method"] {
        let mut setup = Setup::from_wasm(&server.origin, true, &tampering_guest(mode));
        let mut policy = setup.endpoint.policy.clone();
        policy.methods = vec!["DELETE".into(), "GET".into()];
        setup.endpoint = setup
            .app
            .save_endpoint(EndpointUpdate {
                reference: setup.endpoint.reference.to_vec(),
                expected_revision: 1,
                registry_revision: setup
                    .app
                    .local_state()
                    .unwrap()
                    .manager
                    .as_ref()
                    .unwrap()
                    .revision(),
                lifetime_days: 1,
                policy,
            })
            .unwrap();
        let key = setup.app.start_http(setup.request(13)).unwrap();
        ready(&mut setup.app, key);
        let report = setup.app.read_io(key, 512 * 1024).unwrap().unwrap();
        assert_eq!(
            report.calls, 1,
            "{mode}: attack must reach a decoded, charged IO import"
        );
        assert!(report.http_response.is_none(), "{mode}");
        assert!(report.task.execution.outcome.is_err(), "{mode}");
        reclaimed(&mut setup.app, key);
        assert!(
            setup
                .app
                .local_state()
                .unwrap()
                .host
                .store_local()
                .lookup_io_intent(ID, &operation(13))
                .unwrap()
                .is_none()
        );
        assert_eq!(server.connections.load(Ordering::SeqCst), 0, "{mode}");
        setup.app.finish().unwrap();
    }
}
