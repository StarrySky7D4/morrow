//! Real ModelClientSession entrypoints with credential-free, local qualification inputs.
use crate::client::ModelClient;
use crate::client_common::Prompt;
use crate::morrow_network::ModelNetworkBackend;
use crate::test_support::{TestCodexResponsesRequestKind, responses_metadata};
use codex_http_client::{HttpClientFactory, NetworkPolicyController, OutboundProxyPolicy};
use codex_login::auth::AgentIdentityAuthPolicy;
use codex_model_provider::WorkspaceRoutingContext;
use codex_model_provider_info::{WireApi, create_oss_provider_with_base_url};
use codex_otel::SessionTelemetry;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ReasoningSummary;
use codex_protocol::openai_models::ModelInfo;
use codex_protocol::protocol::SessionSource;
use codex_rollout_trace::InferenceTraceContext;
use serde_json::{Value, json};
use std::sync::Arc;

/// Each scenario enters an existing Core branch, without changing fallback flags directly.
#[derive(Clone, Copy)]
pub enum Scenario {
    Http,
    Websocket,
    Prewarm,
    StreamFallback,
    PrewarmFallback,
    PreconnectFallback,
}

pub async fn run(backend: Arc<dyn ModelNetworkBackend>, scenario: Scenario) -> Value {
    run_with_backend(Some(backend), scenario).await
}

/// Deliberately omit injection to reach the actual default-selection guards.
pub async fn run_without_backend(scenario: Scenario) -> Value {
    run_with_backend(None, scenario).await
}

async fn run_with_backend(
    backend: Option<Arc<dyn ModelNetworkBackend>>,
    scenario: Scenario,
) -> Value {
    // The runner launches an allowlisted, isolated environment. Refuse before
    // ModelClient's unconditional auth-env telemetry if a credential key survives.
    for key in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
    ] {
        assert!(
            std::env::var_os(key).is_none(),
            "credential environment must be absent"
        );
    }
    let thread_id = ThreadId::from_string("00000000-0000-4000-8000-000000000003").unwrap();
    let mut provider =
        create_oss_provider_with_base_url("https://fixture.invalid/v1", WireApi::Responses);
    provider.supports_websockets = !matches!(scenario, Scenario::Http);
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
    provider.websocket_connect_timeout_ms = Some(1000);
    assert!(
        provider.auth.is_none()
            && provider.gateway_oauth.is_none()
            && provider.aws.is_none()
            && provider.experimental_bearer_token.is_none()
            && provider.env_key.is_none()
            && provider.env_http_headers.is_none()
            && !provider.requires_openai_auth
    );
    let policy = NetworkPolicyController::default();
    let client = ModelClient::new(
        /*auth_manager*/ None,
        AgentIdentityAuthPolicy::JwtOnly,
        thread_id,
        provider,
        SessionSource::Exec,
        "qualification-local-fixture".into(),
        /*model_verbosity*/ None,
        /*content_item_kinds_enabled*/ false,
        /*reasoning_effort_override_enabled*/ false,
        /*enable_request_compression*/ false,
        /*include_timing_metrics*/ false,
        /*beta_features_header*/ None,
        /*concurrent_reasoning_summaries_enabled*/ false,
        /*attestation_provider*/ None,
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
            .with_network_policy(policy.policy()),
        WorkspaceRoutingContext::new("https://fixture.invalid/backend-api".into()),
        Vec::new(),
    );
    let client = match backend {
        Some(backend) => client.with_network_backend(backend),
        None => client,
    };
    let model: ModelInfo = serde_json::from_value(json!({
        "slug":"qualification-model", "display_name":"qualification-model", "description":"local fixture",
        "default_reasoning_level":"medium", "supported_reasoning_levels":[{"effort":"medium","description":"fixture"}],
        "shell_type":"shell_command", "visibility":"list", "supported_in_api":true,"priority":1,
        "upgrade":null,"model_messages":null,"support_verbosity":false,"default_verbosity":null,
        "apply_patch_tool_type":null,"truncation_policy":{"mode":"bytes","limit":10000},
        "supports_image_detail_original":false,"context_window":272000,"auto_compact_token_limit":null,"experimental_supported_tools":[]
    })).unwrap();
    let telemetry = SessionTelemetry::new(
        thread_id,
        "qualification-model",
        "qualification-model",
        None,
        None,
        None,
        "qualification-local-fixture".into(),
        false,
        "fixture".into(),
        SessionSource::Exec,
    );
    let prompt = Prompt::default();
    let metadata = responses_metadata(
        "11111111-1111-4111-8111-111111111111",
        &thread_id.to_string(),
        &thread_id.to_string(),
        Some("fixture-turn"),
        "fixture-window".into(),
        &SessionSource::Exec,
        None,
        TestCodexResponsesRequestKind::Turn,
    );
    let prewarm_metadata = responses_metadata(
        "11111111-1111-4111-8111-111111111111",
        &thread_id.to_string(),
        &thread_id.to_string(),
        Some("fixture-turn"),
        "fixture-window".into(),
        &SessionSource::Exec,
        None,
        TestCodexResponsesRequestKind::Prewarm,
    );
    let trace = InferenceTraceContext::disabled();
    let mut outcomes = vec![];
    let mut states = vec![client.responses_websocket_enabled()];
    let mut session = client.new_session();
    if matches!(scenario, Scenario::PreconnectFallback) {
        let connection_metadata = responses_metadata(
            "11111111-1111-4111-8111-111111111111",
            &thread_id.to_string(),
            &thread_id.to_string(),
            None,
            "fixture-window".into(),
            &SessionSource::Exec,
            None,
            TestCodexResponsesRequestKind::WebsocketConnection,
        );
        let outcome = session
            .preconnect_websocket(&model, None, &telemetry, &connection_metadata)
            .await;
        outcomes.push(match outcome {
            Ok(()) => "preconnect_setup_ok_without_network_success".into(),
            Err(error) => format!("preconnect_error:{error}"),
        });
        states.push(client.responses_websocket_enabled());
    }
    if matches!(scenario, Scenario::Prewarm | Scenario::PrewarmFallback) {
        let outcome = session
            .prewarm_websocket(
                &prompt,
                &model,
                &telemetry,
                None,
                ReasoningSummary::None,
                None,
                &prewarm_metadata,
            )
            .await;
        outcomes.push(match outcome {
            Ok(()) => "prewarm_setup_ok_without_network_success".into(),
            Err(error) => format!("prewarm_error:{error}"),
        });
        states.push(client.responses_websocket_enabled());
    }
    if !matches!(scenario, Scenario::Prewarm) {
        let result = session
            .stream(
                &prompt,
                &model,
                &telemetry,
                None,
                ReasoningSummary::None,
                None,
                &metadata,
                &trace,
            )
            .await;
        outcomes.push(match result {
            Ok(_) => "unexpected_successful_stream".into(),
            Err(error) => format!("stream_error:{error}"),
        });
        states.push(client.responses_websocket_enabled());
    }
    if matches!(scenario, Scenario::StreamFallback) {
        drop(session);
        let mut next_session = client.new_session();
        let result = next_session
            .stream(
                &prompt,
                &model,
                &telemetry,
                None,
                ReasoningSummary::None,
                None,
                &metadata,
                &trace,
            )
            .await;
        outcomes.push(match result {
            Ok(_) => "unexpected_successful_stream".into(),
            Err(error) => format!("next_session_error:{error}"),
        });
        states.push(client.responses_websocket_enabled());
    }
    json!({"outcomes":outcomes,"websocket_enabled_states":states,"factory_policy":"unavailable","personal_auth_manager":false,"credential_environment_absent":true,"provider_auth_fields_absent":true})
}
