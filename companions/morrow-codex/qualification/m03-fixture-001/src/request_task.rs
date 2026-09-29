//! Business ownership, independent of any UI. Uses the real frozen Core API and
//! preserves typed events including Completed(end_turn=false).
use crate::{
    driver::*,
    fixture::{CoreEventId, Fixture, Mode},
    limits,
    network::{Audit, HostModelNetworkBackend, Operation},
};
use codex_core::test_support::{TestCodexResponsesRequestKind, responses_metadata};
use codex_core::{ModelClient, Prompt, ResponseEvent};
use codex_http_client::{HttpClientFactory, NetworkPolicyController, OutboundProxyPolicy};
use codex_login::auth::AgentIdentityAuthPolicy;
use codex_model_provider::WorkspaceRoutingContext;
use codex_model_provider_info::{WireApi, create_oss_provider_with_base_url};
use codex_otel::SessionTelemetry;
use codex_protocol::{
    ThreadId, config_types::ReasoningSummary, openai_models::ModelInfo, protocol::SessionSource,
};
use codex_rollout_trace::InferenceTraceContext;
use futures::{FutureExt, StreamExt};
use serde::Serialize;
use serde_json::json;
use std::{panic::AssertUnwindSafe, sync::Arc};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

/// Controlled fixture settings, not production provider configuration/authority.
pub struct FixtureRequest {
    pub fixture: Option<Arc<Fixture>>,
    pub base_url: String,
    /// Must come from the validated native session; host enforces it independently.
    pub original_native_deadline: Instant,
    pub max_parser_chunk: usize,
    pub local_request_key: RequestKey,
}
#[derive(Debug, Serialize)]
pub enum CoreTerminal {
    Completed { end_turn: Option<bool> },
    Failed(String),
    Cancelled,
    Expired,
}
#[derive(Debug, Serialize)]
pub struct TaskReceipt {
    pub core_terminal: CoreTerminal,
    pub core_events: usize,
    pub transport_drain: Option<Result<(), BridgeError>>,
    pub cleanup: Result<CleanupReceipt, BridgeError>,
    pub audit: Audit,
    pub product_success_claimed: bool,
}

pub struct RequestTask {
    operation: Arc<Operation>,
    events: mpsc::Receiver<QueuedEvent>,
    fixture: Option<Arc<Fixture>>,
    completion: Option<oneshot::Receiver<TaskReceipt>>,
    finished: bool,
}
struct QueuedEvent {
    value: Result<ResponseEvent, String>,
    fixture_id: Option<CoreEventId>,
}
impl From<Result<ResponseEvent, String>> for QueuedEvent {
    fn from(value: Result<ResponseEvent, String>) -> Self {
        Self {
            value,
            fixture_id: None,
        }
    }
}
impl RequestTask {
    pub async fn next_event(&mut self) -> Option<Result<ResponseEvent, String>> {
        let token = self.operation.token();
        tokio::select! { biased;
            _ = token.cancelled() => None,
            _ = tokio::time::sleep_until(self.operation.deadline()) => {
                self.operation.cancel(CancelReason::Deadline); None
            },
            event = self.events.recv() => {
                let event=event?;
                let target=event.fixture_id.as_ref().filter(|id|id.delta_ordinal==1);
                if let (Some(fixture),Some(id))=(&self.fixture,target) {
                    if !fixture.hold_consumer(id,self.operation.deadline()).await {
                        self.operation.cancel(CancelReason::NativeFailure);
                        return None;
                    }
                }
                let id=event.fixture_id;
                // The original final delivery gate stays decisive after hold.
                let delivered=self.operation.deliver_if_live(||event.value);
                if let (Some(fixture),Some(id))=(&self.fixture,id.as_ref().filter(|id|id.delta_ordinal==1)) {
                    if delivered.is_none() {fixture.suppressed(id,false,token.is_cancelled());}
                    else {fixture.fail("held Core event delivered after fixture release");}
                }
                delivered
            },
        }
    }
    pub fn cancel(&self) {
        self.operation.cancel(CancelReason::User);
    }
    pub async fn wait_cleanup(&mut self) -> Result<TaskReceipt, BridgeError> {
        // Keep the receiver owned by RequestTask while pending: callers may
        // cancel this wait with select!/timeout and resume it later.
        let result = self
            .completion
            .as_mut()
            .ok_or(BridgeError::DuplicateRequest)?
            .await;
        self.completion.take();
        match result {
            Ok(result) => {
                self.finished = true;
                Ok(result)
            }
            Err(_) => {
                self.operation.cancel(CancelReason::OwnerDropped);
                Err(BridgeError::CleanupUnconfirmed)
            }
        }
    }
    pub async fn cancel_and_wait(&mut self) -> Result<TaskReceipt, BridgeError> {
        self.cancel();
        self.wait_cleanup().await
    }
}
impl Drop for RequestTask {
    fn drop(&mut self) {
        if !self.finished {
            self.operation.cancel(CancelReason::OwnerDropped);
        }
    }
}

pub fn start(
    driver: Arc<dyn NativeHttpSession>,
    config: FixtureRequest,
) -> Result<RequestTask, BridgeError> {
    // Inspect only presence, never values. No account/config discovery is used.
    for key in [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_REFRESH_TOKEN_URL_OVERRIDE",
    ] {
        if std::env::var_os(key).is_some() {
            return Err(BridgeError::InvalidInput(
                "credential environment must be absent",
            ));
        }
    }
    let operation = Operation::new(
        driver,
        config.local_request_key,
        &config.base_url,
        config.original_native_deadline,
        config.max_parser_chunk,
    )?;
    let (events_tx, events) = mpsc::channel(limits::CONSUMER_EVENTS);
    let (completion_tx, completion) = oneshot::channel();
    let op = operation.clone();
    let fixture = config.fixture.clone();
    tokio::spawn(async move {
        let outcome = AssertUnwindSafe(consume_core(op.clone(), config, events_tx))
            .catch_unwind()
            .await;
        let (terminal, count, drain) = match outcome {
            Ok(result) => result,
            Err(_) => {
                op.cancel(CancelReason::CoreError);
                (CoreTerminal::Failed("Core task panicked".into()), 0, None)
            }
        };
        let cleanup = op.cleanup().await;
        let _ = completion_tx.send(TaskReceipt {
            core_terminal: terminal,
            core_events: count,
            transport_drain: drain,
            cleanup,
            audit: op.audit(),
            product_success_claimed: false,
        });
    });
    Ok(RequestTask {
        operation,
        events,
        fixture,
        completion: Some(completion),
        finished: false,
    })
}

type CoreOutcome = (CoreTerminal, usize, Option<Result<(), BridgeError>>);
async fn consume_core(
    op: Arc<Operation>,
    config: FixtureRequest,
    events: mpsc::Sender<QueuedEvent>,
) -> CoreOutcome {
    let thread_id = ThreadId::from_string("00000000-0000-4000-8000-000000000003").unwrap();
    let mut provider = create_oss_provider_with_base_url(&config.base_url, WireApi::Responses);
    provider.supports_websockets = false;
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
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
        None,
        AgentIdentityAuthPolicy::JwtOnly,
        thread_id,
        provider,
        SessionSource::Exec,
        "m03-loopback-fixture".into(),
        None,
        false,
        false,
        false,
        false,
        None,
        false,
        None,
        HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
            .with_network_policy(policy.policy()),
        WorkspaceRoutingContext::new(format!("{}/backend-api", config.base_url)),
        Vec::new(),
    )
    .with_network_backend(Arc::new(HostModelNetworkBackend {
        operation: op.clone(),
    }));
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
        "m03-loopback-fixture".into(),
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
    let trace = InferenceTraceContext::disabled();
    let mut session = client.new_session();
    let token = op.token();
    let result = tokio::select! { biased;
        _ = token.cancelled() => return cancelled(&op, 0),
        _ = tokio::time::sleep_until(op.deadline()) => return expired(&op, 0),
        result = session.stream(&prompt, &model, &telemetry, None, ReasoningSummary::None, None, &metadata, &trace) => result,
    };
    let mut stream = match result {
        Ok(stream) => stream,
        Err(error) => {
            op.cancel(CancelReason::CoreError);
            return (
                CoreTerminal::Failed(bounded_error(error.to_string())),
                0,
                None,
            );
        }
    };
    let mut count = 0;
    let mut delta_ordinal = 0u64;
    loop {
        let next = tokio::select! { biased;
            _ = token.cancelled() => return cancelled(&op, count),
            _ = tokio::time::sleep_until(op.deadline()) => return expired(&op, count),
            value = stream.next() => value,
        };
        let event = match next {
            Some(Ok(event)) => event,
            Some(Err(error)) => {
                op.cancel(CancelReason::CoreError);
                return (
                    CoreTerminal::Failed(bounded_error(error.to_string())),
                    count,
                    None,
                );
            }
            None => {
                op.cancel(CancelReason::CoreError);
                return (
                    CoreTerminal::Failed("Core EOF without Completed".into()),
                    count,
                    None,
                );
            }
        };
        count += 1;
        let fixture_id = if config
            .fixture
            .as_ref()
            .is_some_and(|f| f.mode() == Mode::CoreRevoke)
        {
            if let ResponseEvent::OutputTextDelta(text) = &event {
                delta_ordinal += 1;
                Some(CoreEventId::delta(count as u64, delta_ordinal, text))
            } else {
                None
            }
        } else {
            None
        };
        let completed = match &event {
            ResponseEvent::Completed { end_turn, .. } => Some(*end_turn),
            _ => None,
        };
        if let Some(end_turn) = completed {
            op.observe_core_completion(end_turn);
        }
        op.note(
            if completed.is_some() {
                "core_typed_completed"
            } else {
                "core_typed_event"
            },
            &[],
        );
        // Sending to a slow consumer must never hide cancellation/deadline.
        let permit = tokio::select! { biased;
            _ = token.cancelled() => return cancelled(&op, count),
            _ = tokio::time::sleep_until(op.deadline()) => return expired(&op, count),
            result = events.reserve() => result,
        };
        let Ok(permit) = permit else {
            op.cancel(CancelReason::OwnerDropped);
            return (CoreTerminal::Cancelled, count, None);
        };
        if let (Some(fixture), Some(id)) = (
            &config.fixture,
            fixture_id.as_ref().filter(|id| id.delta_ordinal == 2),
        ) {
            if !fixture
                .hold_reserved(id, token.clone(), op.deadline())
                .await
            {
                op.cancel(CancelReason::NativeFailure);
            }
        }
        let sent = op.deliver_if_live(|| {
            permit.send(QueuedEvent {
                value: Ok(event),
                fixture_id: fixture_id.clone(),
            })
        });
        if sent.is_none() {
            if let (Some(fixture), Some(id)) = (
                &config.fixture,
                fixture_id.as_ref().filter(|id| id.delta_ordinal == 2),
            ) {
                fixture.suppressed(id, true, token.is_cancelled());
            }
            return cancelled(&op, count);
        }
        if let (Some(fixture), Some(id)) = (&config.fixture, &fixture_id) {
            fixture.queued(id);
        }
        if let Some(end_turn) = completed {
            // API SSE parser may already have dropped its ByteStream, or may do
            // so just after this event. Operation waits for that handoff safely.
            drop(stream);
            let drain = op.drain_after_completed().await;
            if drain.is_err() {
                op.cancel(CancelReason::InvalidResponse);
            }
            return (CoreTerminal::Completed { end_turn }, count, Some(drain));
        }
    }
}
fn bounded_error(error: String) -> String {
    error.chars().take(1024).collect()
}
fn cancelled(op: &Operation, count: usize) -> CoreOutcome {
    op.cancel(CancelReason::User);
    let terminal = if matches!(op.first_cancel_reason(), Some(CancelReason::Deadline)) {
        CoreTerminal::Expired
    } else {
        CoreTerminal::Cancelled
    };
    (terminal, count, None)
}
fn expired(op: &Operation, count: usize) -> CoreOutcome {
    op.cancel(CancelReason::Deadline);
    (CoreTerminal::Expired, count, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn held_received_event_and_reserved_event_use_original_gates_after_cancel() {
        // Synthetic local regression only, not actual ModelClient provenance.
        let (fixture, _) = crate::fixture::tests::fixture();
        let deadline = Instant::now() + std::time::Duration::from_secs(2);
        let operation = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(10),
            "http://127.0.0.1:12345/v1",
            deadline,
            1024,
        )
        .unwrap();
        let (sender, events) = mpsc::channel(8);
        let a = CoreEventId::delta(1, 1, "local A");
        let b = CoreEventId::delta(2, 2, "local B");
        sender
            .send(QueuedEvent {
                value: Ok(ResponseEvent::OutputTextDelta("local A".into())),
                fixture_id: Some(a.clone()),
            })
            .await
            .unwrap();
        fixture.queued(&a);
        let (completion_tx, completion) = oneshot::channel();
        let mut task = RequestTask {
            operation: operation.clone(),
            events,
            fixture: Some(fixture.clone()),
            completion: Some(completion),
            finished: true,
        };
        let mut consumer = Box::pin(task.next_event());
        assert!(futures::poll!(consumer.as_mut()).is_pending());
        let permit = sender.reserve().await.unwrap();
        let mut reserved = Box::pin(fixture.hold_reserved(&b, operation.token(), deadline));
        assert!(futures::poll!(reserved.as_mut()).is_pending());
        operation
            .cancellation_signal()
            .cancel(CancelReason::HostCancelled);
        fixture.observe_revoke(serde_json::json!({"local_test":true}), true);
        assert!(reserved.await);
        assert!(
            operation
                .deliver_if_live(|| permit.send(QueuedEvent {
                    value: Ok(ResponseEvent::OutputTextDelta("local B".into())),
                    fixture_id: Some(b.clone())
                }))
                .is_none()
        );
        fixture.suppressed(&b, true, operation.token().is_cancelled());
        crate::fixture::tests::release(&fixture, &a).unwrap();
        assert!(consumer.await.is_none());
        assert_eq!(
            sender.capacity(),
            8,
            "permit and held A released without publication"
        );
        assert_eq!(
            operation.first_cancel_reason(),
            Some(CancelReason::HostCancelled)
        );
        fixture.request_stop();
        assert!(fixture.join_until(deadline).await);
        assert_eq!(fixture.snapshot()["stages_complete"], true);
        assert_eq!(fixture.snapshot()["evidence_complete"], true);
        drop(completion_tx);
    }
    #[tokio::test]
    async fn host_latch_suppresses_queued_and_reserved_late_events_without_network_read() {
        let operation = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(9),
            "http://127.0.0.1:12345/v1",
            Instant::now() + std::time::Duration::from_secs(30),
            1024,
        )
        .unwrap();
        let (sender, events) = mpsc::channel(2);
        let (_completion_tx, completion) = oneshot::channel();
        sender
            .send(QueuedEvent::from(Ok(ResponseEvent::OutputTextDelta(
                "already queued".into(),
            ))))
            .await
            .unwrap();
        let permit = sender.reserve().await.unwrap();
        operation.observe_core_completion(Some(false));
        operation
            .cancellation_signal()
            .cancel(CancelReason::HostCancelled);
        assert!(
            operation
                .deliver_if_live(|| permit.send(QueuedEvent::from(Ok(
                    ResponseEvent::OutputTextDelta("late reserved".into())
                ))))
                .is_none()
        );
        let mut task = RequestTask {
            operation,
            events,
            fixture: None,
            completion: Some(completion),
            finished: false,
        };
        assert!(task.next_event().await.is_none());
        assert_eq!(
            task.events.len(),
            1,
            "queued event stays suppressed, late reserve never published"
        );
        assert_eq!(
            task.operation.audit().first_cancel_reason,
            Some(CancelReason::HostCancelled)
        );
        assert_eq!(
            task.operation.audit().core_completion.unwrap().end_turn,
            Some(false)
        );
    }
    #[tokio::test]
    async fn user_cancel_suppresses_queued_events_and_preserves_observed_completion() {
        let operation = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(8),
            "http://127.0.0.1:12345/v1",
            Instant::now() + std::time::Duration::from_secs(30),
            1024,
        )
        .unwrap();
        let (sender, events) = mpsc::channel(1);
        let (_completion_tx, completion) = oneshot::channel();
        sender
            .send(QueuedEvent::from(Ok(ResponseEvent::OutputTextDelta(
                "queued fixture".into(),
            ))))
            .await
            .unwrap();
        operation.observe_core_completion(Some(false));
        let mut task = RequestTask {
            operation,
            events,
            fixture: None,
            completion: Some(completion),
            finished: false,
        };
        task.cancel();
        task.operation.cancel(CancelReason::CoreError);
        assert!(task.next_event().await.is_none());
        let audit = task.operation.audit();
        assert_eq!(audit.first_cancel_reason, Some(CancelReason::User));
        assert_eq!(audit.core_completion.unwrap().end_turn, Some(false));
        assert!(matches!(
            cancelled(&task.operation, 1).0,
            CoreTerminal::Cancelled
        ));
    }

    #[tokio::test]
    async fn cancelled_cleanup_wait_can_resume_same_receipt() {
        // Local receipt ownership only. No Core, HTTP, or successful driver.
        let operation = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(7),
            "http://127.0.0.1:12345/v1",
            Instant::now() + std::time::Duration::from_secs(30),
            1024,
        )
        .unwrap();
        let (_events_tx, events) = mpsc::channel(1);
        let (sender, receiver) = oneshot::channel();
        let mut task = RequestTask {
            operation,
            events,
            fixture: None,
            completion: Some(receiver),
            finished: false,
        };
        {
            let wait = task.wait_cleanup();
            tokio::pin!(wait);
            assert!(futures::poll!(wait.as_mut()).is_pending());
        }
        sender
            .send(TaskReceipt {
                core_terminal: CoreTerminal::Completed {
                    end_turn: Some(false),
                },
                core_events: 17,
                transport_drain: Some(Err(BridgeError::Cancelled)),
                cleanup: Err(BridgeError::CleanupUnconfirmed),
                audit: Audit::default(),
                product_success_claimed: false,
            })
            .unwrap();
        let receipt = task.wait_cleanup().await.unwrap();
        assert_eq!(receipt.core_events, 17);
        assert!(matches!(
            receipt.core_terminal,
            CoreTerminal::Completed {
                end_turn: Some(false)
            }
        ));
        assert!(matches!(
            receipt.cleanup,
            Err(BridgeError::CleanupUnconfirmed)
        ));
        assert!(task.finished);
        assert!(matches!(
            task.wait_cleanup().await,
            Err(BridgeError::DuplicateRequest)
        ));
    }
}
