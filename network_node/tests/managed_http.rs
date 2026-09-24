//! End-to-end managed Wasm HTTP against loopback servers with synthetic data.
//! Network observations are durable facts, not proof of remote business success.
#![cfg(feature = "plugin-adapter")]
use morrow_core::{
    dispatch::HostRuntime,
    io::{Header, HttpSubmission, Request, Response, Status},
    io_evidence::Kind,
    io_intent::{Phase, Recovery},
    outbound_authority::{self, Record as StoredRecord, proto as outbound},
    plugin_package::{
        Package,
        catalog::Catalog,
        io::{self, IoCapability},
        registry::Registry,
    },
    store::{EventBudget, Store},
};
use morrow_network_node::{
    HttpResponse, Limits,
    managed_http::{Credential, EndpointApproval, HttpEndpoint, NetworkProfile},
    server::{Handler, Node, Route, TlsIdentity},
    stored_http::StoredHttpEndpoint,
};
use morrow_plugin_runtime::{
    Limits as RuntimeLimits,
    io_jobs::{
        CommandOwner, HostOwner, IoWorker, JobError, JobHandle, JobLimits, JobReport, Poll,
        ServiceUpdate,
    },
    manager::{ManagedInstance, Manager},
};
use std::{
    collections::BTreeSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
    task::JoinHandle,
};
const ID: &str = "org.example.managed.http.tests";
const WAIT: Duration = Duration::from_secs(10);
const TOKEN: &str = "synthetic-managed-http-bearer-0123456789";
fn limits() -> Limits {
    Limits {
        max_request_bytes: 65536,
        max_response_bytes: 65536,
        max_header_bytes: 16384,
        max_concurrent: 2,
        timeout: Duration::from_secs(2),
    }
}
fn approval(origin: &str) -> EndpointApproval {
    EndpointApproval {
        origin: origin.into(),
        methods: vec!["GET".into(), "POST".into(), "HEAD".into()],
        profile: NetworkProfile::LoopbackHttp,
        limits: limits(),
        response_frame_limit: 128 * 1024,
        credential: None,
        root_certificate: None,
    }
}
fn submission(endpoint: &HttpEndpoint, operation: &str) -> HttpSubmission {
    HttpSubmission {
        operation_id: operation.as_bytes().to_vec(),
        deadline_ms: 0,
        endpoint: endpoint.endpoint_reference().into_bytes(),
        method: "POST".into(),
        relative_target: "/api?q=one&q=two".into(),
        headers: vec![
            Header {
                name: "x-request".into(),
                value: b"one".to_vec(),
            },
            Header {
                name: "x-request".into(),
                value: b"two".to_vec(),
            },
        ],
        body: b"\0\xffinput".to_vec(),
        credential: vec![],
    }
}
const STORED_ENDPOINT: [u8; 32] = [31; 32];
const STORED_CREDENTIAL: [u8; 32] = [32; 32];
struct StoredSetup {
    credential: Option<StoredRecord>,
    wall: Arc<AtomicU64>,
    provider_calls: Arc<AtomicUsize>,
    wrong_reference: bool,
    foreign_instance: bool,
    windows_provider: bool,
    persistent: bool,
    alter: Option<fn(&mut outbound::Endpoint)>,
}
fn stored_setup(credentials: bool) -> StoredSetup {
    StoredSetup {
        credential: credentials.then(|| {
            StoredRecord::encode(outbound::Record {
                schema_version: outbound_authority::VERSION,
                reference: STORED_CREDENTIAL.to_vec(),
                revision: 1,
                created_ms: 1000,
                expires_ms: 61_000,
                disabled: false,
                kind: Some(outbound::record::Kind::Credential(outbound::Credential {
                    provider: outbound_authority::WINDOWS_DPAPI_PROVIDER.into(),
                    ciphertext: vec![1, 2, 3],
                })),
            })
            .unwrap()
        }),
        wall: Arc::new(AtomicU64::new(1000)),
        provider_calls: Arc::new(AtomicUsize::new(0)),
        wrong_reference: false,
        foreign_instance: false,
        windows_provider: false,
        persistent: false,
        alter: None,
    }
}
fn credential_wire_reference() -> Vec<u8> {
    STORED_CREDENTIAL
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>()
        .into_bytes()
}
struct HttpOwner {
    host: HostRuntime,
    original: morrow_core::dispatch::HostBinding,
}
impl HostOwner for HttpOwner {
    fn runtime(&self) -> &HostRuntime {
        &self.host
    }
    fn runtime_mut(&mut self) -> &mut HostRuntime {
        &mut self.host
    }
}
impl CommandOwner for HttpOwner {
    fn command(&mut self, input: Vec<u8>) -> Result<Vec<u8>, JobError> {
        assert_eq!(self.host.binding(), self.original);
        let card = morrow_core::content::CardRecord::new(
            "during-http",
            "note",
            1,
            "owner command",
            input.clone(),
        )
        .map_err(|_| JobError::Unavailable)?;
        self.host
            .store_local_mut()
            .create_local("create-during-http", &card)
            .map_err(|_| JobError::Unavailable)?;
        if input == b"panic" {
            panic!("synthetic owner panic after committed card");
        }
        Ok(input)
    }
}
struct Running {
    _dir: tempfile::TempDir,
    _manager: Manager,
    _original_instance: Option<ManagedInstance>,
    worker: IoWorker<HttpOwner>,
    endpoint: HttpEndpoint,
    secondary: Option<HttpEndpoint>,
}
impl Running {
    fn new(approval: EndpointApproval, credentials: bool) -> Self {
        Self::configured(approval, credentials, false)
    }
    fn configured(approval: EndpointApproval, credentials: bool, second_instance: bool) -> Self {
        Self::build(approval, credentials, second_instance, None, None).unwrap()
    }
    fn stored(
        approval: EndpointApproval,
        credentials: bool,
        setup: StoredSetup,
    ) -> morrow_network_node::Result<Self> {
        Self::build(approval, credentials, false, Some(setup), None)
    }
    fn build(
        approval: EndpointApproval,
        credentials: bool,
        second_instance: bool,
        stored: Option<StoredSetup>,
        extra: Option<EndpointApproval>,
    ) -> morrow_network_node::Result<Self> {
        Self::build_with_guest(approval, credentials, second_instance, stored, extra, None)
    }
    fn build_with_guest(
        approval: EndpointApproval,
        credentials: bool,
        second_instance: bool,
        stored: Option<StoredSetup>,
        extra: Option<EndpointApproval>,
        guest: Option<&[u8]>,
    ) -> morrow_network_node::Result<Self> {
        Self::build_with_package(
            approval,
            credentials,
            second_instance,
            stored,
            extra,
            guest,
            None,
        )
    }
    fn build_with_package(
        approval: EndpointApproval,
        credentials: bool,
        second_instance: bool,
        stored: Option<StoredSetup>,
        extra: Option<EndpointApproval>,
        guest: Option<&[u8]>,
        original_package: Option<&Package>,
    ) -> morrow_network_node::Result<Self> {
        let dir = tempfile::tempdir().unwrap();
        let wasm = guest.map(<[u8]>::to_vec).unwrap_or_else(|| {
            wat::parse_str(
                r#"(module
          (import "morrow_task_v1" "read_input" (func $read (param i32 i32) (result i32)))
          (import "morrow_task_v1" "complete" (func $done (param i32 i32) (result i32)))
          (import "morrow_io_v1" "call" (func $io (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "morrow_run") (result i32) (local $n i32)
            i32.const 0 i32.const 131072 call $read local.set $n
            i32.const 131072 i32.const 0 local.get $n i32.const 131072 i32.const 131072
            call $io call $done drop i32.const 0))"#,
            )
            .unwrap()
        });
        let mut caps = BTreeSet::from([IoCapability::HttpRequest]);
        if credentials {
            caps.insert(IoCapability::CredentialUse);
        }
        let mut manifest = Package::manifest_for_task(ID, "1.0.0", &wasm, vec![]);
        let mut declaration =
            io::declaration(caps.iter().copied().collect(), vec!["io.invoke".into()]);
        let budget = declaration.budget.as_mut().unwrap();
        budget.max_jobs = 1;
        budget.max_resources = if extra.is_some() { 3 } else { 2 };
        budget.max_job_bytes = 1024 * 1024;
        budget.max_bytes = 4 * 1024 * 1024;
        manifest.required_features.push(io::FEATURE.into());
        manifest.io_declaration = Some(declaration);
        let package = if let Some(original) = original_package {
            // Exercise the generated archive as-is, never repair its declaration/budget.
            assert_eq!(original.manifest().package_id, ID);
            assert_eq!(original.module(), wasm.as_slice());
            let decoded = Package::decode(original.archive()).unwrap();
            assert_eq!(decoded.digest(), original.digest());
            decoded
        } else {
            Package::build(manifest, &wasm).unwrap()
        };
        let declared_io_budget = package.io_declaration().unwrap().budget.as_ref().unwrap();
        let worker_limits = JobLimits::new(
            1,
            declared_io_budget.max_job_bytes.min(1024 * 1024),
            declared_io_budget.max_bytes.min(4 * 1024 * 1024),
        )
        .unwrap();
        let digest = package.digest();
        let catalog = Catalog::open(&dir.path().join("catalog")).unwrap();
        catalog.install(&package).unwrap();
        let registry = Registry::open(&dir.path().join("registry"), catalog).unwrap();
        let mut manager = Manager::new(registry, RuntimeLimits::default());
        manager.select(&package, manager.revision()).unwrap();
        manager
            .approve_io(ID, digest, caps.clone(), manager.revision())
            .unwrap();
        manager
            .set_enabled(ID, digest, true, manager.revision())
            .unwrap();
        let mut store = Store::open(&dir.path().join("db"), EventBudget::default()).unwrap();
        let restored = if let Some(settings) = &stored {
            if let Some(credential) = &settings.credential {
                store.save_outbound_authority_local(credential, 0).unwrap();
            }
            let mut endpoint = outbound::Endpoint {
                package_id: ID.into(),
                package_sha256: digest.to_vec(),
                origin: approval.origin.clone(),
                profile: match approval.profile {
                    NetworkProfile::PublicHttps => 1,
                    NetworkProfile::LoopbackHttp => 2,
                    NetworkProfile::LoopbackHttps => 3,
                },
                methods: vec!["GET".into(), "HEAD".into(), "POST".into()],
                credential_reference: settings
                    .credential
                    .as_ref()
                    .map_or_else(Vec::new, |_| STORED_CREDENTIAL.to_vec()),
                root_certificate: approval.root_certificate.clone().unwrap_or_default(),
                max_request_bytes: approval.limits.max_request_bytes as u64,
                max_response_bytes: approval.limits.max_response_bytes as u64,
                max_header_bytes: approval.limits.max_header_bytes as u64,
                max_concurrent: approval.limits.max_concurrent as u32,
                timeout_ms: approval.limits.timeout.as_millis() as u64,
                max_frame_bytes: approval.response_frame_limit,
            };
            if let Some(alter) = settings.alter {
                alter(&mut endpoint);
            }
            let endpoint = StoredRecord::encode(outbound::Record {
                schema_version: outbound_authority::VERSION,
                reference: STORED_ENDPOINT.to_vec(),
                revision: 1,
                created_ms: 1000,
                expires_ms: 61_000,
                disabled: false,
                kind: Some(outbound::record::Kind::Endpoint(endpoint)),
            })
            .unwrap();
            store.save_outbound_authority_local(&endpoint, 0).unwrap();
            // Close the writer completely: restored approval and ciphertext must
            // come from the reopened original database, not in-memory records.
            drop(store);
            store = Store::open_existing(&dir.path().join("db"), EventBudget::default()).unwrap();
            let wall = settings.wall.clone();
            Some(StoredHttpEndpoint::resolve(
                &mut store,
                &STORED_ENDPOINT,
                move || wall.load(Ordering::SeqCst),
            )?)
        } else {
            None
        };
        let mut host = HostRuntime::new(store).unwrap();
        let instance = manager.connect(ID, &mut host).unwrap();
        let binding = manager
            .bind_io(&host, &instance, digest, manager.revision(), &caps, 100, 1)
            .unwrap();
        let endpoint = if let Some(restored) = restored {
            let settings = stored.as_ref().unwrap();
            let foreign = settings
                .foreign_instance
                .then(|| manager.connect(ID, &mut host).unwrap());
            let approved_instance = foreign.as_ref().unwrap_or(&instance);
            if settings.windows_provider {
                #[cfg(target_os = "windows")]
                {
                    restored.approve_windows(
                        &manager,
                        &host,
                        approved_instance,
                        &binding,
                        [7; 32],
                        2,
                    )?
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return Err(morrow_network_node::Error::Denied);
                }
            } else {
                let approve = if settings.persistent {
                    StoredHttpEndpoint::approve_persistent
                } else {
                    StoredHttpEndpoint::approve
                };
                approve(
                    restored,
                    &manager,
                    &host,
                    approved_instance,
                    &binding,
                    [7; 32],
                    2,
                    |_: &StoredRecord| {
                        settings.provider_calls.fetch_add(1, Ordering::SeqCst);
                        let reference = if settings.wrong_reference {
                            b"wrong-reference".to_vec()
                        } else {
                            credential_wire_reference()
                        };
                        Credential::header(reference, "authorization", &format!("Bearer {TOKEN}"))
                    },
                )?
            }
        } else {
            HttpEndpoint::approve(&manager, &host, &instance, &binding, approval, [7; 32], 2)
                .unwrap()
        };
        let secondary = extra.map(|approval| {
            HttpEndpoint::approve(&manager, &host, &instance, &binding, approval, [8; 32], 2)
                .unwrap()
        });
        let (instance, binding, original) = if second_instance {
            let second = manager.connect(ID, &mut host).unwrap();
            let binding = manager
                .bind_io(&host, &second, digest, manager.revision(), &caps, 100, 2)
                .unwrap();
            (second, binding, Some(instance))
        } else {
            (instance, binding, None)
        };
        let owner = HttpOwner {
            original: host.binding(),
            host,
        };
        let worker = IoWorker::spawn_managed_owned(
            &manager,
            owner,
            instance,
            binding,
            || 2,
            1,
            worker_limits,
        )
        .unwrap();
        Ok(Self {
            _dir: dir,
            _manager: manager,
            _original_instance: original,
            worker,
            endpoint,
            secondary,
        })
    }
    fn submit(&mut self, submission: &HttpSubmission) -> (Request, JobHandle) {
        let request = Request::encode_http_submit(1, submission).unwrap();
        let router = self.endpoint.router(tokio::runtime::Handle::current());
        let job = self
            .worker
            .submit_brokered(request.bytes().to_vec(), Box::new(router), WAIT)
            .unwrap();
        (request, job)
    }
    async fn finish(&mut self) -> HostRuntime {
        self.worker.stop();
        let end = Instant::now() + WAIT;
        loop {
            if let Some(exit) = self.worker.try_reclaim().unwrap() {
                assert_eq!(exit.result, Ok(()));
                assert_eq!(exit.maintenance, Ok(()));
                assert_eq!(exit.disconnect, Ok(()));
                let host = exit.owner.host;
                host.store_local().integrity_check().unwrap();
                return host;
            }
            assert!(Instant::now() < end, "managed worker did not stop");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}
async fn ready(job: &mut JobHandle) {
    let end = Instant::now() + WAIT;
    loop {
        match job.poll() {
            Poll::Ready => return,
            Poll::Pending => {
                assert!(Instant::now() < end, "managed request did not finish");
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            other => panic!("unexpected phase {other:?}"),
        }
    }
}
async fn consume(job: &mut JobHandle) -> JobReport {
    ready(job).await;
    job.read(256 * 1024).unwrap().unwrap()
}
struct Server {
    origin: String,
    calls: Arc<AtomicUsize>,
    received: mpsc::UnboundedReceiver<Vec<u8>>,
    task: JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn new(response: Option<Vec<u8>>, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let (send, received) = mpsc::unbounded_channel();
        let task = tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                counter.fetch_add(1, Ordering::SeqCst);
                let mut raw = vec![];
                let mut buffer = [0; 1024];
                let header_end = loop {
                    let n = socket.read(&mut buffer).await.unwrap();
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&buffer[..n]);
                    assert!(raw.len() < 128 * 1024);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                let text = String::from_utf8_lossy(&raw[..header_end]);
                let length = text
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.trim().parse::<usize>().unwrap());
                assert!(header_end + length <= 128 * 1024);
                while raw.len() < header_end + length {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    raw.extend_from_slice(&buffer[..n]);
                }
                let _ = send.send(raw);
                tokio::time::sleep(delay).await;
                if let Some(response) = &response {
                    let _ = socket.write_all(response).await;
                }
                let _ = socket.shutdown().await;
            }
        });
        Self {
            origin,
            calls,
            received,
            task,
        }
    }
    async fn request(&mut self) -> Vec<u8> {
        tokio::time::timeout(WAIT, self.received.recv())
            .await
            .unwrap()
            .unwrap()
    }
}
fn raw_response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut raw=format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\nX-Repeat: one\r\nX-Repeat: two\r\n\r\n",body.len()).into_bytes();
    raw.extend_from_slice(body);
    raw
}
fn assert_observed(host: &HostRuntime, request: &Request, operation: &str, report: &JobReport) {
    let store = host.store_local();
    let record = store.lookup_io_intent(ID, operation).unwrap().unwrap();
    assert_eq!(record.phase(), Phase::Observed);
    assert_eq!(record.recovery(), Recovery::AlreadyObserved);
    assert_eq!(
        store
            .io_material(ID, operation, Kind::Request)
            .unwrap()
            .unwrap()
            .payload(),
        request.bytes()
    );
    let material = store
        .io_material(ID, operation, Kind::Response)
        .unwrap()
        .unwrap();
    let outcome = Response::decode_http(request, material.payload()).unwrap();
    assert_eq!(Some(&outcome), report.http_response.as_ref());
}
fn assert_unknown(host: &HostRuntime, operation: &str) {
    let record = host
        .store_local()
        .lookup_io_intent(ID, operation)
        .unwrap()
        .unwrap();
    assert_eq!(record.phase(), Phase::OutcomeUnknown);
    assert_eq!(record.recovery(), Recovery::ReconcileOnly);
}
#[tokio::test]
async fn real_post_preserves_binary_body_duplicate_headers_and_durable_frames() {
    let mut raw = raw_response("201 Created", b"\0\xffreply");
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    raw.splice(end..end, b"\r\nX-Label: \xe9".iter().copied());
    let mut server = Server::new(Some(raw), Duration::ZERO).await;
    let mut run = Running::new(approval(&server.origin), false);
    let input = submission(&run.endpoint, "real-post");
    let (request, mut job) = run.submit(&input);
    let report = consume(&mut job).await;
    assert!(report.task.execution.outcome.is_ok());
    let result = report.http_response.as_ref().unwrap();
    assert_eq!(result.status, Status::Completed);
    assert_eq!(result.http_status, 201);
    assert_eq!(result.body, b"\0\xffreply");
    assert_eq!(
        result
            .headers
            .iter()
            .find(|h| h.name == "x-label")
            .unwrap()
            .value,
        vec![0xe9]
    );
    for (name, value) in [
        ("content-length", b"7".as_slice()),
        ("connection", b"close".as_slice()),
    ] {
        assert!(
            result
                .headers
                .iter()
                .any(|h| h.name == name && h.value == value)
        );
    }
    assert_eq!(
        result
            .headers
            .iter()
            .filter(|h| h.name == "x-repeat")
            .map(|h| h.value.as_slice())
            .collect::<Vec<_>>(),
        vec![b"one".as_slice(), b"two".as_slice()]
    );
    let wire = server.request().await;
    assert!(wire.starts_with(b"POST /api?q=one&q=two HTTP/1.1\r\n"));
    assert!(wire.ends_with(b"\0\xffinput"));
    let text = String::from_utf8_lossy(&wire).to_ascii_lowercase();
    assert!(text.contains("x-request: one\r\n") && text.contains("x-request: two\r\n"));
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    assert_observed(&run.finish().await, &request, "real-post", &report);
}
#[tokio::test]
async fn error_http_status_is_observed_and_redirect_never_contacts_second_origin() {
    for status in [
        "404 Not Found",
        "429 Too Many Requests",
        "500 Internal Server Error",
        "302 Found",
    ] {
        let second = Server::new(Some(raw_response("200 OK", b"unwanted")), Duration::ZERO).await;
        let raw = if status.starts_with("302") {
            format!("HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\nLocation: {}/other\r\n\r\n",second.origin).into_bytes()
        } else {
            raw_response(status, b"error-body")
        };
        let mut server = Server::new(Some(raw), Duration::ZERO).await;
        let mut run = Running::new(approval(&server.origin), false);
        let (request, mut job) = run.submit(&submission(&run.endpoint, "status"));
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_ok());
        assert!(!report.unknown);
        assert_eq!(
            report.http_response.as_ref().unwrap().http_status,
            status[..3].parse::<u16>().unwrap()
        );
        let _ = server.request().await;
        assert_eq!(second.calls.load(Ordering::SeqCst), 0);
        assert_observed(&run.finish().await, &request, "status", &report);
    }
}
#[tokio::test]
async fn wrong_reference_method_or_credential_is_rejected_before_network_and_history() {
    for mismatch in 0..3 {
        let server = Server::new(Some(raw_response("200 OK", b"unwanted")), Duration::ZERO).await;
        // CredentialUse is approved so the third case reaches endpoint-reference validation.
        let mut run = Running::new(approval(&server.origin), true);
        let mut input = submission(&run.endpoint, "deny-before-send");
        match mismatch {
            0 => input.endpoint = b"unapproved-reference".to_vec(),
            1 => input.method = "DELETE".into(),
            2 => input.credential = b"unapproved-credential".to_vec(),
            _ => unreachable!(),
        }
        let (_, mut job) = run.submit(&input);
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_err());
        assert!(!report.unknown);
        assert!(report.http_response.is_none());
        assert_eq!(server.calls.load(Ordering::SeqCst), 0);
        assert!(
            run.finish()
                .await
                .store_local()
                .lookup_io_intent(ID, "deny-before-send")
                .unwrap()
                .is_none()
        );
    }
}
#[tokio::test]
async fn disconnect_after_request_is_unknown_and_same_operation_is_not_sent_again() {
    let mut server = Server::new(None, Duration::ZERO).await;
    let mut run = Running::new(approval(&server.origin), false);
    let input = submission(&run.endpoint, "disconnect-once");
    let (_, mut first) = run.submit(&input);
    let _ = server.request().await;
    let report = consume(&mut first).await;
    assert!(report.unknown && report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none());
    let (_, mut second) = run.submit(&input);
    let again = consume(&mut second).await;
    assert!(again.unknown && again.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    assert_unknown(&run.finish().await, "disconnect-once");
}
#[tokio::test]
async fn positive_guest_deadline_limits_network_wait_and_keeps_unknown_history() {
    let mut server = Server::new(
        Some(raw_response("200 OK", b"late")),
        Duration::from_secs(1),
    )
    .await;
    let mut run = Running::new(approval(&server.origin), false);
    let mut input = submission(&run.endpoint, "guest-timeout");
    input.deadline_ms = 50;
    let (_, mut job) = run.submit(&input);
    let _ = server.request().await;
    let report = consume(&mut job).await;
    assert!(report.unknown && report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none());
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    assert_unknown(&run.finish().await, "guest-timeout");
}
#[tokio::test]
async fn endpoint_revocation_after_request_prevents_delivery_without_claiming_rollback() {
    let mut server = Server::new(
        Some(raw_response("200 OK", b"late")),
        Duration::from_millis(150),
    )
    .await;
    let mut run = Running::new(approval(&server.origin), false);
    let (_, mut job) = run.submit(&submission(&run.endpoint, "revoke-after-send"));
    let _ = server.request().await;
    run.endpoint.revoke();
    let report = consume(&mut job).await;
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none() && report.response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    let host = run.finish().await;
    let record = host
        .store_local()
        .lookup_io_intent(ID, "revoke-after-send")
        .unwrap()
        .unwrap();
    // An interrupted transport is Unknown; a fully observed response may already
    // be durable. Neither state permits redelivery after endpoint revocation.
    assert!(matches!(
        record.phase(),
        Phase::OutcomeUnknown | Phase::Observed
    ));
}
#[tokio::test]
async fn oversized_or_unrepresentable_real_responses_remain_unknown() {
    let many_headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n{}\r\n",
        (0..65).map(|i| format!("x-{i}: v\r\n")).collect::<String>()
    )
    .into_bytes();
    let large_header = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\nX-Large: {}\r\n\r\n",
        "a".repeat(8193)
    )
    .into_bytes();
    let chunked=b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\n123\r\n2\r\n45\r\n0\r\n\r\n".to_vec();
    for (index, raw) in [
        raw_response("200 OK", b"12345"),
        chunked,
        many_headers,
        large_header,
        raw_response("200 OK", b"framed"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut server = Server::new(Some(raw), Duration::ZERO).await;
        let mut config = approval(&server.origin);
        if index < 2 {
            config.limits.max_response_bytes = 4;
        }
        if index == 4 {
            config.response_frame_limit = 1;
        }
        let mut run = Running::new(config, false);
        let (_, mut job) = run.submit(&submission(&run.endpoint, "response-limit"));
        let _ = server.request().await;
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_err(), "case {index}");
        assert!(report.unknown, "case {index}");
        assert!(report.http_response.is_none());
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        assert_unknown(&run.finish().await, "response-limit");
    }
}
#[tokio::test]
async fn head_and_bodyless_statuses_do_not_reject_advertised_large_content_length() {
    for (method, status) in [
        ("HEAD", "200 OK"),
        ("GET", "204 No Content"),
        ("GET", "304 Not Modified"),
    ] {
        let raw =
            format!("HTTP/1.1 {status}\r\nContent-Length: 9999999\r\nConnection: close\r\n\r\n")
                .into_bytes();
        let mut server = Server::new(Some(raw), Duration::ZERO).await;
        let mut run = Running::new(approval(&server.origin), false);
        let mut input = submission(&run.endpoint, "bodyless");
        input.method = method.into();
        input.body.clear();
        let (request, mut job) = run.submit(&input);
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_ok());
        assert!(report.http_response.as_ref().unwrap().body.is_empty());
        let _ = server.request().await;
        assert_observed(&run.finish().await, &request, "bodyless", &report);
    }
}
#[tokio::test]
async fn trusted_credential_is_injected_only_at_transport_not_into_protected_guest_frame() {
    let mut server = Server::new(Some(raw_response("200 OK", b"ok")), Duration::ZERO).await;
    let mut config = approval(&server.origin);
    config.credential = Some(
        Credential::header(
            b"secret-ref".to_vec(),
            "authorization",
            &format!("Bearer {TOKEN}"),
        )
        .unwrap(),
    );
    let mut run = Running::new(config, true);
    let mut input = submission(&run.endpoint, "credential");
    input.credential = b"secret-ref".to_vec();
    let (request, mut job) = run.submit(&input);
    let report = consume(&mut job).await;
    assert!(report.task.execution.outcome.is_ok());
    let raw = server.request().await;
    assert!(String::from_utf8_lossy(&raw).contains(&format!("Bearer {TOKEN}")));
    assert!(
        !request
            .bytes()
            .windows(TOKEN.len())
            .any(|w| w == TOKEN.as_bytes())
    );
    assert_observed(&run.finish().await, &request, "credential", &report);
}
#[tokio::test]
async fn real_tls_keeps_root_and_hostname_validation_in_managed_path() {
    for (trusted_root, matching_hostname) in [(true, true), (false, true), (true, false)] {
        let valid_root = trusted_root && matching_hostname;
        let key = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let wrong = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let handler: Handler = Arc::new(move |request, _| {
            counter.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(HttpResponse {
                    status: 201,
                    headers: vec![],
                    body: request.body,
                })
            })
        });
        let identity = TlsIdentity::from_pem(
            key.cert.pem().as_bytes(),
            key.key_pair.serialize_pem().as_bytes(),
        )
        .unwrap();
        let node = Node::bind_tls(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            TOKEN.into(),
            vec![Route::new("POST", "/api", handler).unwrap()],
            limits(),
            identity,
        )
        .await
        .unwrap();
        let hostname = if matching_hostname {
            "localhost"
        } else {
            "127.0.0.1"
        };
        let origin = format!("https://{hostname}:{}", node.local_addr().port());
        let mut config = approval(&origin);
        config.profile = NetworkProfile::LoopbackHttps;
        config.root_certificate = Some(if trusted_root {
            key.cert.der().to_vec()
        } else {
            wrong.cert.der().to_vec()
        });
        config.credential = Some(
            Credential::header(
                b"tls-ref".to_vec(),
                "authorization",
                &format!("Bearer {TOKEN}"),
            )
            .unwrap(),
        );
        let mut run = Running::new(config, true);
        let mut input = submission(&run.endpoint, "tls");
        input.credential = b"tls-ref".to_vec();
        let (request, mut job) = run.submit(&input);
        let report = consume(&mut job).await;
        let host = run.finish().await;
        if valid_root {
            assert!(report.task.execution.outcome.is_ok());
            assert_eq!(report.http_response.as_ref().unwrap().body, input.body);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_observed(&host, &request, "tls", &report);
        } else {
            assert!(report.task.execution.outcome.is_err() && report.unknown);
            assert!(report.http_response.is_none());
            assert_eq!(calls.load(Ordering::SeqCst), 0);
            assert_unknown(&host, "tls");
        }
        node.shutdown().await.unwrap();
    }
}

#[tokio::test]
async fn ready_result_is_suppressed_if_endpoint_is_revoked_before_read() {
    let mut server = Server::new(
        Some(raw_response("200 OK", b"ready-secret")),
        Duration::ZERO,
    )
    .await;
    let mut run = Running::new(approval(&server.origin), false);
    let (request, mut job) = run.submit(&submission(&run.endpoint, "revoke-ready"));
    ready(&mut job).await;
    let _ = server.request().await;
    run.endpoint.revoke();
    let report = job.read(0).unwrap().unwrap();
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none() && report.response.is_none());
    assert_eq!(report.payload_bytes(), 0);
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    let host = run.finish().await;
    assert_eq!(
        host.store_local()
            .lookup_io_intent(ID, "revoke-ready")
            .unwrap()
            .unwrap()
            .phase(),
        Phase::Observed
    );
    let original = host
        .store_local()
        .io_material(ID, "revoke-ready", Kind::Response)
        .unwrap()
        .unwrap();
    assert_eq!(
        Response::decode_http(&request, original.payload())
            .unwrap()
            .body,
        b"ready-secret"
    );
}
#[tokio::test]
async fn original_endpoint_cannot_authorize_second_live_instance_of_same_package() {
    let server = Server::new(Some(raw_response("200 OK", b"unwanted")), Duration::ZERO).await;
    // Both instances remain live in the same Host/Manager; only their connection
    // identity differs. Keeping the first instance alive avoids a vacuous revoked test.
    let mut run = Running::configured(approval(&server.origin), false, true);
    let (_, mut job) = run.submit(&submission(&run.endpoint, "other-instance"));
    let report = consume(&mut job).await;
    assert!(report.task.execution.outcome.is_err());
    assert!(report.http_response.is_none() && report.response.is_none());
    assert!(!report.unknown);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    assert!(
        run.finish()
            .await
            .store_local()
            .lookup_io_intent(ID, "other-instance")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn all_seven_approved_methods_reach_server_through_managed_wasm_and_are_observed() {
    const METHODS: [&str; 7] = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
    for method in METHODS {
        let response_body: &[u8] = if method == "HEAD" {
            b""
        } else {
            b"\0\xffmethod-reply"
        };
        let mut server =
            Server::new(Some(raw_response("200 OK", response_body)), Duration::ZERO).await;
        let mut config = approval(&server.origin);
        config.methods = METHODS.iter().map(|method| (*method).to_owned()).collect();
        let mut run = Running::new(config, false);
        let operation = format!("method-{}", method.to_ascii_lowercase());
        let mut input = submission(&run.endpoint, &operation);
        input.method = method.into();
        // Including HEAD: request bytes must not be dropped merely because the
        // corresponding response is bodyless.
        input.body = b"\0\xffmethod-input".to_vec();
        let (request, mut job) = run.submit(&input);
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_ok(), "method {method}");
        assert!(!report.unknown, "method {method}");
        let response = report.http_response.as_ref().unwrap();
        assert_eq!(response.status, Status::Completed, "method {method}");
        assert_eq!(response.http_status, 200, "method {method}");
        assert_eq!(response.body, response_body, "method {method}");
        let wire = server.request().await;
        assert!(
            wire.starts_with(format!("{method} /api?q=one&q=two HTTP/1.1\r\n").as_bytes()),
            "method {method}"
        );
        let body_start = wire
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .unwrap()
            + 4;
        assert_eq!(
            &wire[body_start..],
            input.body.as_slice(),
            "method {method}"
        );
        assert_eq!(server.calls.load(Ordering::SeqCst), 1, "method {method}");
        assert_observed(&run.finish().await, &request, &operation, &report);
    }
}

#[tokio::test]
async fn stored_endpoint_without_credential_never_calls_provider_and_uses_real_transport() {
    let mut server = Server::new(
        Some(raw_response("200 OK", b"stored response")),
        Duration::ZERO,
    )
    .await;
    let setup = stored_setup(false);
    let calls = setup.provider_calls.clone();
    let mut run = Running::stored(approval(&server.origin), false, setup).unwrap();
    let (request, mut job) = run.submit(&submission(&run.endpoint, "stored-no-credential"));
    let report = consume(&mut job).await;
    assert_eq!(
        report.http_response.as_ref().unwrap().body,
        b"stored response"
    );
    let wire = server.request().await;
    assert!(
        !String::from_utf8_lossy(&wire)
            .to_ascii_lowercase()
            .contains("authorization:")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_observed(
        &run.finish().await,
        &request,
        "stored-no-credential",
        &report,
    );
}
#[tokio::test]
async fn stored_endpoint_rejects_wrong_package_capability_policy_and_alias_before_secret_callback()
{
    for mode in 0..7 {
        let mut setup = stored_setup(true);
        setup.foreign_instance = mode == 6;
        let calls = setup.provider_calls.clone();
        setup.alter = match mode {
            0 => Some(|endpoint| endpoint.package_sha256 = vec![99; 32]),
            1 => None,
            2 => Some(|endpoint| endpoint.root_certificate = b"not a valid certificate".to_vec()),
            3 => Some(|endpoint| endpoint.origin = "http://127.1".into()),
            4 => Some(|endpoint| endpoint.origin = "http://0x7f000001".into()),
            5 => Some(|endpoint| endpoint.package_id = "org.example.foreign".into()),
            _ => None,
        };
        assert!(
            Running::stored(approval("http://127.0.0.1:12345"), mode != 1, setup).is_err(),
            "mode {mode}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0, "mode {mode}");
    }
    for disabled in [false, true] {
        let mut setup = stored_setup(true);
        let calls = setup.provider_calls.clone();
        let mut credential = setup.credential.take().unwrap().value().clone();
        if disabled {
            credential.disabled = true;
        } else {
            credential.created_ms = 1;
            credential.expires_ms = 1000;
        }
        setup.credential = Some(StoredRecord::encode(credential).unwrap());
        assert!(Running::stored(approval("http://127.0.0.1:12345"), true, setup).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    let mut setup = stored_setup(true);
    setup.wrong_reference = true;
    let calls = setup.provider_calls.clone();
    assert!(Running::stored(approval("http://127.0.0.1:12345"), true, setup).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
#[tokio::test]
async fn stored_endpoint_owner_queue_disable_suppresses_inflight_and_ready_responses() {
    for ready_first in [false, true] {
        let mut server = Server::new(
            Some(raw_response("200 OK", b"must not be delivered")),
            if ready_first {
                Duration::ZERO
            } else {
                Duration::from_millis(150)
            },
        )
        .await;
        let mut run = Running::stored(approval(&server.origin), true, stored_setup(true)).unwrap();
        let mut input = submission(&run.endpoint, "stored-revoke");
        input.credential = credential_wire_reference();
        let (_, mut job) = run.submit(&input);
        let _ = server.request().await;
        if ready_first {
            ready(&mut job).await;
        }
        let mut observer =
            Store::open_existing(&run._dir.path().join("db"), EventBudget::default()).unwrap();
        let mut value = observer
            .load_outbound_authority(&STORED_CREDENTIAL)
            .unwrap()
            .unwrap()
            .value()
            .clone();
        value.revision += 1;
        value.disabled = true;
        let value = StoredRecord::encode(value).unwrap();
        assert_eq!(
            observer.save_outbound_authority_local(&value, 1),
            Err(morrow_core::Error::StorageBusy)
        );
        let mut update = run
            .worker
            .update_service(ServiceUpdate::Outbound {
                value,
                expected_revision: 1,
            })
            .unwrap();
        let report = consume(&mut job).await;
        assert!(report.task.execution.outcome.is_err());
        assert!(report.http_response.is_none() && report.response.is_none());
        assert_eq!(report.payload_bytes(), 0);
        tokio::time::timeout(WAIT, async {
            loop {
                if let Some(result) = update.read().unwrap() {
                    result.unwrap();
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        let host = run.finish().await;
        assert!(matches!(
            host.store_local()
                .lookup_io_intent(ID, "stored-revoke")
                .unwrap()
                .unwrap()
                .phase(),
            Phase::OutcomeUnknown | Phase::Observed
        ));
    }
}
#[tokio::test]
async fn stored_endpoint_utc_regression_and_expiry_are_sticky_at_ready_delivery() {
    for invalid_time in [999, 61_000] {
        let mut server = Server::new(
            Some(raw_response("200 OK", b"undeliverable")),
            Duration::ZERO,
        )
        .await;
        let setup = stored_setup(false);
        let wall = setup.wall.clone();
        let mut run = Running::stored(approval(&server.origin), false, setup).unwrap();
        let (_, mut job) = run.submit(&submission(&run.endpoint, "stored-time"));
        let _ = server.request().await;
        ready(&mut job).await;
        wall.store(invalid_time, Ordering::SeqCst);
        let report = job.read(256 * 1024).unwrap().unwrap();
        assert!(report.http_response.is_none() && report.response.is_none());
        assert_eq!(report.payload_bytes(), 0);
        wall.store(1000, Ordering::SeqCst);
        let (_, mut again) = run.submit(&submission(&run.endpoint, "stored-time-again"));
        let report = consume(&mut again).await;
        assert!(report.task.execution.outcome.is_err());
        assert!(report.http_response.is_none());
        assert_eq!(server.calls.load(Ordering::SeqCst), 1);
        run.finish().await;
    }
}
#[cfg(target_os = "windows")]
#[tokio::test]
async fn stored_windows_dpapi_credential_injects_real_http_without_plaintext_in_material() {
    let mut server = Server::new(
        Some(raw_response("200 OK", b"provider response")),
        Duration::ZERO,
    )
    .await;
    let mut setup = stored_setup(true);
    let credential = morrow_audit::credentials::seal(
        STORED_CREDENTIAL,
        1,
        1000,
        61_000,
        "authorization",
        &format!("Bearer {TOKEN}"),
    )
    .unwrap();
    assert!(
        !credential
            .container()
            .windows(TOKEN.len())
            .any(|part| part == TOKEN.as_bytes())
    );
    setup.credential = Some(credential);
    setup.windows_provider = true;
    let calls = setup.provider_calls.clone();
    let mut run = Running::stored(approval(&server.origin), true, setup).unwrap();
    let mut input = submission(&run.endpoint, "stored-dpapi");
    input.credential = credential_wire_reference();
    let (request, mut job) = run.submit(&input);
    let report = consume(&mut job).await;
    assert_eq!(
        report.http_response.as_ref().unwrap().body,
        b"provider response"
    );
    let wire = server.request().await;
    assert!(String::from_utf8_lossy(&wire).contains(&format!("authorization: Bearer {TOKEN}\r\n")));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let host = run.finish().await;
    assert_observed(&host, &request, "stored-dpapi", &report);
    for kind in [Kind::Request, Kind::Response] {
        let material = host
            .store_local()
            .io_material(ID, "stored-dpapi", kind)
            .unwrap()
            .unwrap();
        assert!(
            !material
                .payload()
                .windows(TOKEN.len())
                .any(|part| part == TOKEN.as_bytes())
        );
    }
}

#[path = "support/deferred_http_owner.rs"]
mod deferred_http_owner;

#[path = "support/http_route_set.rs"]
mod http_route_set;

#[path = "support/gated_http.rs"]
mod gated_http;

#[path = "support/stored_http_identity.rs"]
mod stored_http_identity;

// Explicit compiled SDK qualification; ordinary tests continue using the WAT fixture.
#[path = "support/sdk_http_guest.rs"]
mod sdk_http_guest;
