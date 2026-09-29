//! A real upstream ResponsesClient call intercepted and deliberately refused.
//! This is not a Codex loop, HTTP backend, connected host, or production plugin.

use codex_api::{
    AuthProvider, Compression, Provider, ResponsesApiRequest, ResponsesClient, ResponsesOptions,
    RetryConfig,
};
use codex_client::{HttpTransport, Request, Response, StreamResponse, TransportError};
use http::{HeaderMap, Method};
use morrow_agent_host_contract::{self as kit, agent_host_capnp as wire};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const BASE_URL: &str = "https://morrow-offline-fixture.invalid/v1";
const EXPECTED_URL: &str = "https://morrow-offline-fixture.invalid/v1/responses";
const DISCONNECTED: &str = "MORROW_P02_HOST_DISCONNECTED_BEFORE_OPEN";
const DESTINATION_DENIED: &str = "MORROW_P02_DESTINATION_OR_METHOD_DENIED";
const SESSION: &str = "p02-disconnected-session";
const ATTEMPT: &str = "p02-disconnected-attempt";
const HOST_KIT_SHA256: &str = "5555751ab1cf590d68e9610dae396677b79422999ec61b7c3d3d7183e8ed4a01";

fn digest_hex(bytes: &[u8]) -> String {
    kit::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct NoAuthProvider;
impl AuthProvider for NoAuthProvider {
    fn add_auth_headers(&self, _: &mut HeaderMap) {}
}

#[derive(Clone, Debug, Serialize)]
struct ObservedRequest {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    prepared_body_length: usize,
    prepared_body_sha256: String,
    prepared_body_json: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
struct ObservedHostFrame {
    kind: &'static str,
    request_id: u64,
    session_id: String,
    instance_epoch: u64,
    attempt_id: String,
    request_body_length: u64,
    request_body_sha256: String,
    frame_sha256: String,
}

#[derive(Clone, Default, Debug, Serialize)]
struct Evidence {
    stream_calls: usize,
    unexpected_execute_calls: usize,
    requests: Vec<ObservedRequest>,
    host_exchange_calls: usize,
    host_frames: Vec<ObservedHostFrame>,
    open_attempts: usize,
    commit_attempts: usize,
}

// Deliberately disconnected fixture at the exact exported kit Transport boundary.
// Its narrow Error enum has no Disconnected variant; Invalid is fixture-only and
// the adapter maps this known local refusal to a separately named sentinel.
struct DisconnectedHost {
    evidence: Arc<Mutex<Evidence>>,
}

impl kit::Transport for DisconnectedHost {
    fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, kit::Error> {
        let mut evidence = self.evidence.lock().expect("evidence mutex poisoned");
        evidence.host_exchange_calls += 1;
        let message = kit::decode(request)?;
        let frame = message.get_root::<wire::frame::Reader>()?;
        let wire::frame::Which::Stream(stream) = frame.which()? else {
            return Err(kit::Error::WrongDirection);
        };
        let stream = stream?;
        match stream.which()? {
            wire::stream::Which::Open(open) => {
                evidence.open_attempts += 1;
                let open = open?;
                let request_digest = open.get_request_digest()?;
                evidence.host_frames.push(ObservedHostFrame {
                    kind: "Stream.Open",
                    request_id: frame.get_request_id(),
                    session_id: kit::text(frame.get_session_id())?,
                    instance_epoch: frame.get_instance_epoch(),
                    attempt_id: kit::text(stream.get_attempt_id())?,
                    request_body_length: open.get_request_bytes(),
                    request_body_sha256: request_digest
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                    frame_sha256: digest_hex(request),
                });
            }
            wire::stream::Which::CommitRequest(_) => evidence.commit_attempts += 1,
            _ => return Err(kit::Error::Invalid),
        }
        Err(kit::Error::Invalid)
    }
}

struct MorrowDisconnectedTransport {
    evidence: Arc<Mutex<Evidence>>,
}

impl HttpTransport for MorrowDisconnectedTransport {
    async fn execute(&self, _: Request) -> Result<Response, TransportError> {
        self.evidence
            .lock()
            .expect("evidence mutex poisoned")
            .unexpected_execute_calls += 1;
        Err(TransportError::Build(
            "MORROW_P02_UNEXPECTED_NONSTREAM_ENTRY".to_string(),
        ))
    }

    async fn stream(&self, request: Request) -> Result<StreamResponse, TransportError> {
        self.evidence
            .lock()
            .expect("evidence mutex poisoned")
            .stream_calls += 1;
        let prepared = request
            .prepare_body_for_send()
            .map_err(TransportError::Build)?;
        // All input is explicitly constructed below. Refuse sensitive headers
        // before recording values so this harness cannot turn into a token log.
        if ["authorization", "proxy-authorization", "cookie"]
            .iter()
            .any(|name| prepared.headers.contains_key(*name))
        {
            return Err(TransportError::Build(
                "MORROW_P02_CREDENTIAL_HEADER_DENIED".to_string(),
            ));
        }
        let body = prepared.body_bytes();
        let mut headers = Vec::new();
        for (name, value) in &prepared.headers {
            headers.push((
                name.to_string(),
                value
                    .to_str()
                    .map_err(|error| TransportError::Build(error.to_string()))?
                    .to_string(),
            ));
        }
        headers.sort();
        self.evidence
            .lock()
            .expect("evidence mutex poisoned")
            .requests
            .push(ObservedRequest {
                method: request.method.to_string(),
                url: request.url.clone(),
                headers,
                prepared_body_length: body.len(),
                prepared_body_sha256: digest_hex(&body),
                prepared_body_json: serde_json::from_slice(&body)
                    .map_err(|error| TransportError::Build(error.to_string()))?,
            });
        if request.method != Method::POST || request.url != EXPECTED_URL {
            return Err(TransportError::Build(DESTINATION_DENIED.to_string()));
        }
        if body.len() > kit::MAX_REQUEST_BYTES {
            return Err(TransportError::Build("MORROW_P02_BODY_LIMIT".to_string()));
        }

        let mut message = kit::frame(1, SESSION, 1);
        {
            let frame = message
                .get_root::<wire::frame::Builder>()
                .map_err(|error| TransportError::Build(error.to_string()))?;
            let mut stream = frame.init_stream();
            stream.set_attempt_id(ATTEMPT);
            let mut open = stream.init_open();
            open.set_request_bytes(body.len() as u64);
            open.set_request_digest(&kit::digest(&body));
            {
                let mut destination = open.reborrow().init_destination();
                destination.set_namespace("fixture");
                destination.set_id("destination");
                destination.set_revision(9_007_199_254_740_993);
                destination.set_byte_length(0);
                destination.set_sha256(&kit::digest(b"fixture-reference"));
            }
            let mut deadline = open.init_deadline();
            deadline.set_domain(kit::ClockDomain::MonotonicMillis);
            deadline.set_clock_id("fixture-clock");
            deadline.set_value(1000);
        }
        let bytes =
            kit::encode(&message).map_err(|error| TransportError::Build(error.to_string()))?;
        let mut host = DisconnectedHost {
            evidence: self.evidence.clone(),
        };
        match kit::exchange_checked(&mut host, &bytes) {
            Err(error) => Err(TransportError::Network(format!(
                "{DISCONNECTED}; fixture_transport_error={error}"
            ))),
            Ok(_) => Err(TransportError::Build(
                "MORROW_P02_UNEXPECTED_CONNECTED_HOST".to_string(),
            )),
        }
    }
}

fn provider(base_url: &str) -> Provider {
    Provider {
        name: "morrow-offline-qualification".to_string(),
        base_url: base_url.to_string(),
        query_params: None,
        headers: HeaderMap::new(),
        retry: RetryConfig {
            max_attempts: 1,
            base_delay: Duration::from_millis(1),
            retry_429: false,
            retry_5xx: false,
            retry_transport: false,
        },
        stream_idle_timeout: Duration::from_millis(100),
    }
}

fn fixture_request() -> ResponsesApiRequest {
    ResponsesApiRequest {
        model: "morrow-qualification-only".to_string(),
        instructions: "Offline callsite interception fixture. No model request may be sent."
            .to_string(),
        input: Vec::new(),
        tools: None,
        tool_choice: "none".to_string(),
        parallel_tool_calls: false,
        reasoning: None,
        store: false,
        stream: true,
        stream_options: None,
        include: Vec::new(),
        service_tier: None,
        prompt_cache_key: None,
        text: None,
        client_metadata: None,
        access_programs: None,
    }
}

#[derive(Serialize)]
struct CaseResult {
    name: &'static str,
    status: &'static str,
    upstream_returned_error: Option<String>,
    assertions: Vec<(&'static str, bool)>,
    evidence: Evidence,
}

async fn run_case(
    name: &'static str,
    base_url: &str,
    expected_sentinel: &str,
    expected_host_calls: usize,
) -> CaseResult {
    let evidence = Arc::new(Mutex::new(Evidence::default()));
    let client = ResponsesClient::new(
        MorrowDisconnectedTransport {
            evidence: evidence.clone(),
        },
        provider(base_url),
        Arc::new(NoAuthProvider),
    );
    let request = fixture_request();
    let expected_bytes = serde_json::to_vec(&request).expect("fixture request serializes");
    let mut options = ResponsesOptions::default();
    options.compression = Compression::None;
    // This must remain the actual upstream entry; directly calling our transport
    // would not prove the ResponsesClient / EndpointSession seam was reached.
    let returned = client.stream_request(request, options).await;
    let error = match returned {
        Err(error) => Some(error.to_string()),
        Ok(_) => None,
    };
    let observed = evidence.lock().expect("evidence mutex poisoned").clone();
    let request = observed.requests.first();
    let mut assertions = vec![
        (
            "upstream_error_preserves_named_sentinel",
            error
                .as_ref()
                .is_some_and(|value| value.contains(expected_sentinel)),
        ),
        (
            "one_original_stream_request_no_retry",
            observed.stream_calls == 1 && observed.requests.len() == 1,
        ),
        (
            "nonstream_entry_never_used",
            observed.unexpected_execute_calls == 0,
        ),
        (
            "actual_prepared_body_matches_fixed_request",
            request.is_some_and(|value| {
                value.prepared_body_sha256 == digest_hex(&expected_bytes)
                    && value.prepared_body_length == expected_bytes.len()
            }),
        ),
        (
            "endpoint_set_sse_and_json_headers",
            request.is_some_and(|value| {
                value
                    .headers
                    .contains(&("accept".to_string(), "text/event-stream".to_string()))
                    && value
                        .headers
                        .contains(&("content-type".to_string(), "application/json".to_string()))
            }),
        ),
        (
            "expected_host_exchange_count",
            observed.host_exchange_calls == expected_host_calls,
        ),
        ("no_commit_attempt", observed.commit_attempts == 0),
    ];
    if expected_host_calls == 1 {
        assertions.push((
            "actual_open_frame_binds_upstream_prepared_body",
            observed.host_frames.first().is_some_and(|frame| {
                frame.request_body_sha256 == digest_hex(&expected_bytes)
                    && frame.request_body_length == expected_bytes.len() as u64
                    && frame.session_id == SESSION
                    && frame.attempt_id == ATTEMPT
                    && frame.request_id == 1
                    && frame.instance_epoch == 1
            }) && observed.open_attempts == 1,
        ));
    }
    CaseResult {
        name,
        status: if assertions.iter().all(|(_, passed)| *passed) {
            "passed_callsite_refusal"
        } else {
            "failed"
        },
        upstream_returned_error: error,
        assertions,
        evidence: observed,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(args.next().ok_or("one new receipt file path is required")?);
    if args.next().is_some() {
        return Err("only one output argument is accepted".into());
    }
    let allowed = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("no qualification parent")?
        .parent()
        .ok_or("no repository parent")?
        .join("receipts/p02-native-probe-001")
        .canonicalize()?;
    if output
        .parent()
        .ok_or("receipt parent is required")?
        .canonicalize()?
        != allowed
    {
        return Err("receipt must be directly inside receipts/p02-native-probe-001".into());
    }
    let kit_manifest = include_bytes!("../../../sdk/host-kit-003-copy/manifest.json");
    if digest_hex(kit_manifest) != HOST_KIT_SHA256 {
        return Err("compiled host-kit manifest digest differs from the reviewed 003 kit".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    let cases = runtime.block_on(async {
        vec![
            run_case("host_disconnected_before_open", BASE_URL, DISCONNECTED, 1).await,
            run_case(
                "unapproved_destination_rejected",
                "https://unapproved-fixture.invalid/v1",
                DESTINATION_DENIED,
                0,
            )
            .await,
        ]
    });
    let passed = cases
        .iter()
        .all(|case| case.status == "passed_callsite_refusal");
    let receipt = serde_json::json!({
        "status": if passed { "passed_limited_net_callsite_probe" } else { "failed" },
        "upstream_commit": "44fe510ce3ee61c8ef623adcbf89b901c73ddd61",
        "host_kit_manifest_sha256": HOST_KIT_SHA256,
        "real_entry": "codex_api::ResponsesClient::stream_request -> EndpointSession::stream_encoded_json_with -> HttpTransport::stream",
        "cases": cases,
        "limits": ["Disconnected in-process host fixture only; no actual host connection", "No successful StreamResponse, HTTP metadata, SSE parser, login or model request", "No ModelClient/Core loop, exec/store replacement or complete network-bypass qualification", "No OS-level network monitor; this harness constructs no ReqwestTransport, socket or network client"],
        "P-02": "not_complete", "G0": "not_claimed"
    });
    let bytes = serde_json::to_vec_pretty(&receipt)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    if !passed {
        return Err("one or more real-callsite refusal assertions failed".into());
    }
    Ok(())
}
