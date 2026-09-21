//! Native application selection -> original managed guest -> real HTTP.
//! Actual Rust guest reuses core codecs; this does not qualify the public IO SDK.
use super::*;
use morrow_core::{
    io::{HttpSubmission, Request as IoRequest},
    outbound_authority::{self, Record, proto as outbound},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
const ENDPOINT: [u8; 32] = [61; 32];
const OPERATION: &str = "application-outbound-once";

fn input() -> IoRequest {
    IoRequest::encode_http_submit(
        1,
        &HttpSubmission {
            operation_id: OPERATION.as_bytes().to_vec(),
            deadline_ms: 0,
            endpoint: ENDPOINT
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_bytes(),
            method: "GET".into(),
            relative_target: "/value".into(),
            headers: vec![],
            body: vec![],
            credential: vec![],
        },
    )
    .unwrap()
}
fn package() -> Package {
    let mut manifest = service_package().manifest().clone();
    manifest
        .io_declaration
        .as_mut()
        .unwrap()
        .requested_capabilities
        .push(IoCapability::HttpRequest.number());
    let wasm = std::fs::read(
        std::env::var("MORROW_SERVICE_OUTBOUND_WASM")
            .expect("set MORROW_SERVICE_OUTBOUND_WASM to the actual compiled test guest"),
    )
    .unwrap();
    // Recompute the module metadata instead of reusing the old module digest.
    let base = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
    manifest.module_sha256 = base.module_sha256;
    Package::build(manifest, &wasm).unwrap()
}
struct Server {
    address: SocketAddr,
    calls: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let count = calls.clone();
        let stopped = stop.clone();
        let join = thread::spawn(move || {
            while !stopped.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        count.fetch_add(1, Ordering::SeqCst);
                        socket
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut request = Vec::new();
                        while !request.ends_with(b"\r\n\r\n") {
                            let mut byte = [0];
                            if socket.read_exact(&mut byte).is_err() {
                                break;
                            }
                            request.push(byte[0]);
                            assert!(request.len() < 16384);
                        }
                        assert!(
                            request.starts_with(b"GET /value HTTP/1.1\r\n"),
                            "synthetic server request: {:?}",
                            String::from_utf8_lossy(&request)
                        );
                        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nlive-native").unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        Self {
            address,
            calls,
            stop,
            join: Some(join),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let joined = self.join.take().unwrap().join();
        if !thread::panicking() {
            joined.unwrap();
        }
    }
}
fn save_endpoint(fixture: &mut Fixture, server: &Server, revision: u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let record = Record::encode(outbound::Record {
        schema_version: outbound_authority::VERSION,
        reference: ENDPOINT.to_vec(),
        revision,
        created_ms: now - 1000,
        expires_ms: now + 600_000,
        disabled: false,
        kind: Some(outbound::record::Kind::Endpoint(outbound::Endpoint {
            package_id: ID.into(),
            package_sha256: fixture.package_digest.to_vec(),
            origin: format!("http://{}", server.address),
            methods: vec!["GET".into()],
            profile: 2,
            credential_reference: vec![],
            root_certificate: vec![],
            max_request_bytes: 65536,
            max_response_bytes: 65536,
            max_header_bytes: 16384,
            max_concurrent: 2,
            timeout_ms: 2000,
            max_frame_bytes: 131072,
        })),
    })
    .unwrap();
    let state = fixture.app.local_state_mut().unwrap();
    state.host.prepare_write().unwrap();
    state
        .host
        .store_local_mut()
        .save_outbound_authority_local(&record, revision - 1)
        .unwrap();
    state.host.flush_pending().unwrap();
}
#[track_caller]
fn request(address: SocketAddr, path: &str, status: u16) -> Vec<u8> {
    let mut socket = TcpStream::connect(address).unwrap();
    socket.set_read_timeout(Some(WAIT)).unwrap();
    let body = input();
    write!(socket,"POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nIdempotency-Key: {FIRST_KEY}\r\nMorrow-Outbound-Scope: forged\r\nConnection: close, morrow-outbound-scope\r\nContent-Length: {}\r\n\r\n", body.bytes().len()).unwrap();
    socket.write_all(body.bytes()).unwrap();
    let mut result = Vec::new();
    socket.read_to_end(&mut result).unwrap();
    assert!(
        result.starts_with(format!("HTTP/1.1 {status} ").as_bytes()),
        "{}",
        String::from_utf8_lossy(&result)
    );
    result
}
fn stop(fixture: &mut Fixture, key: TaskKey) {
    fixture.app.cancel_io(key).unwrap();
    let exit = exited(&mut fixture.app, key).task.exit.unwrap();
    assert!(exit.execution.is_ok() && exit.disconnect.is_ok() && exit.maintenance.is_ok());
    fixture.app.acknowledge_io(key).unwrap();
}
#[test]
fn selected_endpoint_executes_real_http_and_scope_controls_replay_after_reapproval() {
    let server = Server::new();
    let mut allowed = caps();
    allowed.insert(IoCapability::HttpRequest);
    let mut fixture = Fixture::with_package("127.0.0.1:0".parse().unwrap(), package(), allowed);
    let identity = fixture.app.local_state().unwrap().host.binding();
    save_endpoint(&mut fixture, &server, 1);
    let selection = [ServiceEndpointSelection {
        reference: ENDPOINT,
        revision: 1,
    }];
    let key = fixture
        .app
        .start_service_with_outbound(fixture.options(1), &selection)
        .unwrap();
    let address = running(&mut fixture.app, key);
    let result = request(address, "/api", 202);
    let body = &result[result.windows(4).position(|b| b == b"\r\n\r\n").unwrap() + 4..];
    let output = morrow_core::io::Response::decode_http(&input(), body).unwrap();
    assert_eq!(output.http_status, 200);
    assert_eq!(output.body, b"live-native");
    request(address, "/api", 202);
    request(address, "/history", 202);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    stop(&mut fixture, key);
    assert_eq!(fixture.app.local_state().unwrap().host.binding(), identity);
    // Fresh live approval + unchanged persistent records can replay without a resend.
    let key = fixture
        .app
        .start_service_with_outbound(fixture.options(2), &selection)
        .unwrap();
    let address = running(&mut fixture.app, key);
    request(address, "/api", 202);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    stop(&mut fixture, key);
    save_endpoint(&mut fixture, &server, 2);
    assert!(
        fixture
            .app
            .start_service_with_outbound(fixture.options(3), &selection)
            .is_err()
    );
    let key = fixture
        .app
        .start_service_with_outbound(
            fixture.options(3),
            &[ServiceEndpointSelection {
                reference: ENDPOINT,
                revision: 2,
            }],
        )
        .unwrap();
    let address = running(&mut fixture.app, key);
    request(address, "/api", 409);
    request(address, "/history", 409);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    stop(&mut fixture, key);
    let state = fixture.app.local_state().unwrap();
    assert_eq!(
        state
            .host
            .store_local()
            .lookup_io_intent(ID, OPERATION)
            .unwrap()
            .unwrap()
            .phase(),
        morrow_core::io_intent::Phase::Observed
    );
    fixture.app.finish().unwrap();
}
#[test]
fn invalid_selection_and_unapproved_outbound_capability_fail_before_listener() {
    let server = Server::new();
    let mut fixture = Fixture::new("127.0.0.1:0".parse().unwrap());
    save_endpoint(&mut fixture, &server, 1);
    let valid = ServiceEndpointSelection {
        reference: ENDPOINT,
        revision: 1,
    };
    for selection in [
        vec![valid; 9],
        vec![valid; 2],
        vec![ServiceEndpointSelection {
            reference: [0; 32],
            revision: 1,
        }],
        vec![ServiceEndpointSelection {
            reference: ENDPOINT,
            revision: 0,
        }],
        vec![ServiceEndpointSelection {
            reference: [99; 32],
            revision: 1,
        }],
        vec![ServiceEndpointSelection {
            reference: ENDPOINT,
            revision: 2,
        }],
        vec![valid],
    ] {
        assert!(
            fixture
                .app
                .start_service_with_outbound(fixture.options(1), &selection)
                .is_err()
        );
        assert!(fixture.app.state.task.is_none());
        assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    }
    // The failed selections did not reserve the submission or leave a live owner.
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    post(address, FIRST_KEY, b"before");
    stop(&mut fixture, key);
    fixture.app.finish().unwrap();
}

#[test]
fn saved_endpoint_and_registry_approval_do_not_replace_explicit_service_selection() {
    let server = Server::new();
    let mut allowed = caps();
    allowed.insert(IoCapability::HttpRequest);
    let mut fixture = Fixture::with_package("127.0.0.1:0".parse().unwrap(), package(), allowed);
    save_endpoint(&mut fixture, &server, 1);
    let key = fixture.start();
    let address = running(&mut fixture.app, key);
    request(address, "/api", 409);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    stop(&mut fixture, key);
    assert!(
        fixture
            .app
            .local_state()
            .unwrap()
            .host
            .store_local()
            .lookup_io_intent(ID, OPERATION)
            .unwrap()
            .is_none()
    );
    fixture.app.finish().unwrap();
}
