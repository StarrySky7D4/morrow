//! Actual Core ModelClientSession branches with local, always-refusing capabilities.
use codex_api::{ApiError, ResponsesWebsocketConnection};
use codex_core::morrow_network::{ModelNetworkBackend, NetworkFuture, WebsocketConnectRequest};
use codex_core::morrow_network_qualification::{self as core_probe, Scenario};
use codex_http_client::{Request, Response, StreamResponse, TransportError};
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
enum WebsocketFixture {
    Disconnected,
    UpgradeRequired,
}
#[derive(Debug)]
pub(crate) struct RefusingBackend {
    websocket: WebsocketFixture,
    calls: Mutex<Vec<Value>>,
}

pub(crate) fn context() -> Arc<RefusingBackend> {
    Arc::new(RefusingBackend {
        websocket: WebsocketFixture::Disconnected,
        calls: Mutex::new(vec![]),
    })
}
pub(crate) fn calls(backend: &RefusingBackend) -> Vec<Value> {
    backend.calls.lock().unwrap().clone()
}
fn headers(values: &HeaderMap) -> Vec<(String, String)> {
    for name in [
        "authorization",
        "proxy-authorization",
        "cookie",
        "x-api-key",
    ] {
        assert!(
            !values.contains_key(name),
            "refuse credential header before recording"
        );
    }
    let mut rows: Vec<_> = values
        .iter()
        .map(|(name, value)| {
            (
                name.to_string(),
                value.to_str().expect("fixture header UTF-8").to_owned(),
            )
        })
        .collect();
    rows.sort();
    rows
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl ModelNetworkBackend for RefusingBackend {
    fn execute(&self, _request: Request) -> NetworkFuture<'_, Result<Response, TransportError>> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"method":"http.execute_unexpected"}));
        Box::pin(async {
            Err(TransportError::Build(
                "qualification-unexpected-execute".into(),
            ))
        })
    }
    fn stream(
        &self,
        request: Request,
    ) -> NetworkFuture<'_, Result<StreamResponse, TransportError>> {
        Box::pin(async move {
            let prepared = request
                .prepare_body_for_send()
                .map_err(TransportError::Build)?;
            let safe_headers = headers(&prepared.headers);
            let body = prepared.body_bytes();
            self.calls.lock().unwrap().push(json!({"method":"http.stream","http_method":request.method.as_str(),"url":request.url,"headers":safe_headers,"body_utf8":std::str::from_utf8(&body).unwrap(),"body_bytes":body.len(),"body_sha256":digest(&body),"response":"local_refusal_before_successful_stream"}));
            Err(TransportError::Build(
                "qualification-disconnected-http".into(),
            ))
        })
    }
    fn connect_websocket(
        &self,
        request: WebsocketConnectRequest,
    ) -> NetworkFuture<'_, Result<ResponsesWebsocketConnection, ApiError>> {
        Box::pin(async move {
            let url = request
                .provider
                .websocket_url_for_path("/responses")
                .unwrap()
                .to_string();
            self.calls.lock().unwrap().push(json!({"method":"websocket.connect","url":url,"provider_headers":headers(&request.provider.headers),"extra_headers":headers(&request.extra_headers),"default_headers":headers(&request.default_headers),"boundary":"before_default_API_header_merge_and_handshake","response":match self.websocket {WebsocketFixture::Disconnected=>"local_disconnect",WebsocketFixture::UpgradeRequired=>"synthetic_426"}}));
            match self.websocket {
                WebsocketFixture::Disconnected => Err(ApiError::Transport(TransportError::Build(
                    "qualification-disconnected-websocket".into(),
                ))),
                WebsocketFixture::UpgradeRequired => {
                    Err(ApiError::Transport(TransportError::Http {
                        status: StatusCode::UPGRADE_REQUIRED,
                        url: Some(url),
                        headers: None,
                        body: Some("qualification synthetic handshake error only".into()),
                        retry_after: None,
                    }))
                }
            }
        })
    }
}
fn check(condition: bool, message: &str, count: &mut usize) {
    assert!(condition, "{message}");
    *count += 1;
}
pub(crate) async fn qualify() -> Vec<Value> {
    let scenarios = [
        (
            "http_direct",
            Scenario::Http,
            WebsocketFixture::Disconnected,
            vec!["http.stream"],
            json!([false, false]),
        ),
        (
            "websocket_disconnect",
            Scenario::Websocket,
            WebsocketFixture::Disconnected,
            vec!["websocket.connect"],
            json!([true, true]),
        ),
        (
            "prewarm_disconnect",
            Scenario::Prewarm,
            WebsocketFixture::Disconnected,
            vec!["websocket.connect"],
            json!([true, true]),
        ),
        (
            "stream_426_natural_fallback_and_next_session",
            Scenario::StreamFallback,
            WebsocketFixture::UpgradeRequired,
            vec!["websocket.connect", "http.stream", "http.stream"],
            json!([true, false, false]),
        ),
        (
            "prewarm_426_natural_fallback_then_stream",
            Scenario::PrewarmFallback,
            WebsocketFixture::UpgradeRequired,
            vec!["websocket.connect", "http.stream"],
            json!([true, false, false]),
        ),
        (
            "preconnect_426_natural_fallback_then_stream",
            Scenario::PreconnectFallback,
            WebsocketFixture::UpgradeRequired,
            vec!["websocket.connect", "http.stream"],
            json!([true, false, false]),
        ),
    ];
    let mut results = vec![];
    for (name, scenario, websocket, expected_calls, expected_states) in scenarios {
        let backend = Arc::new(RefusingBackend {
            websocket,
            calls: Mutex::new(vec![]),
        });
        let result = core_probe::run(backend.clone(), scenario).await;
        let calls = backend.calls.lock().unwrap();
        let actual: Vec<_> = calls
            .iter()
            .map(|call| call["method"].as_str().unwrap())
            .collect();
        let mut assertions = 0;
        check(
            actual == expected_calls,
            "complete actual Core backend call order",
            &mut assertions,
        );
        check(
            result["websocket_enabled_states"] == expected_states,
            "Core-owned fallback state and persistence",
            &mut assertions,
        );
        let outcomes = result["outcomes"].as_array().unwrap();
        check(
            outcomes.len()
                == if matches!(
                    scenario,
                    Scenario::StreamFallback
                        | Scenario::PrewarmFallback
                        | Scenario::PreconnectFallback
                ) {
                    2
                } else {
                    1
                },
            "actual entrypoint outcome count",
            &mut assertions,
        );
        for (index, outcome) in outcomes.iter().enumerate() {
            let text = outcome.as_str().unwrap();
            if matches!(
                scenario,
                Scenario::PrewarmFallback | Scenario::PreconnectFallback
            ) && index == 0
            {
                check(
                    text == if matches!(scenario, Scenario::PreconnectFallback) {
                        "preconnect_setup_ok_without_network_success"
                    } else {
                        "prewarm_setup_ok_without_network_success"
                    },
                    "426 setup means control flow only",
                    &mut assertions,
                );
            } else {
                let expected = if matches!(scenario, Scenario::Websocket | Scenario::Prewarm) {
                    "qualification-disconnected-websocket"
                } else {
                    "qualification-disconnected-http"
                };
                check(
                    text.contains(expected),
                    "original refusal reaches Core caller",
                    &mut assertions,
                );
            }
        }
        for call in calls.iter() {
            match call["method"].as_str().unwrap() {
                "http.stream" => {
                    let body = call["body_utf8"].as_str().unwrap();
                    let value: Value = serde_json::from_str(body).unwrap();
                    check(
                        call["http_method"] == "POST"
                            && call["url"] == "https://fixture.invalid/v1/responses",
                        "actual Responses HTTP target",
                        &mut assertions,
                    );
                    check(
                        value["model"] == "qualification-model" && value["stream"] == true,
                        "actual Core serialized model stream request",
                        &mut assertions,
                    );
                    check(
                        call["body_sha256"] == digest(body.as_bytes())
                            && call["body_bytes"] == body.len(),
                        "exact recorded request bytes and digest",
                        &mut assertions,
                    );
                }
                "websocket.connect" => {
                    check(
                        call["url"] == "wss://fixture.invalid/v1/responses",
                        "actual provider WebSocket target",
                        &mut assertions,
                    );
                }
                _ => panic!("unexpected backend operation"),
            }
        }
        results.push(json!({"case":name,"assertions":assertions,"core_result":result,"backend_calls":*calls}));
    }
    results
}
