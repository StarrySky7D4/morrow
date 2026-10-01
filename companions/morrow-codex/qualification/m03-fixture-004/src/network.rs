use crate::{driver::*, limits};
use codex_api::{ApiError, ResponsesWebsocketConnection};
use codex_core::morrow_network::{ModelNetworkBackend, NetworkFuture, WebsocketConnectRequest};
use codex_http_client::{Request, Response, StreamResponse, TransportError};
use futures::{Stream, StreamExt, stream::BoxStream};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    sync::watch,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct Observation {
    pub kind: &'static str,
    pub elapsed_us: u64,
    pub offset: u64,
    pub bytes: usize,
    pub sha256: Option<String>,
}
#[derive(Default, Clone, Debug, Serialize)]
pub struct Audit {
    pub observations: Vec<Observation>,
    pub overflow: bool,
    pub cancel_queue_error: Option<BridgeError>,
    pub received_bytes: u64,
    pub parser_yielded_bytes: u64,
    pub drain_discarded_bytes: u64,
    pub error_body_consumed_bytes: u64,
    pub ack_queued_offset: u64,
    pub first_cancel_reason: Option<CancelReason>,
    pub core_completion: Option<CoreCompletion>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CoreCompletion {
    pub end_turn: Option<bool>,
}

pub struct Operation {
    driver: Arc<dyn NativeHttpSession>,
    key: RequestKey,
    deadline: Instant,
    started: Instant,
    cancellation: CancellationSignal,
    entered: AtomicBool,
    received: AtomicU64,
    acknowledged: AtomicU64,
    parser_yielded: AtomicU64,
    drain_discarded: AtomicU64,
    error_consumed: AtomicU64,
    response_limit: AtomicUsize,
    eof: AtomicBool,
    parser_detached: watch::Sender<bool>,
    read_guard: tokio::sync::Mutex<()>,
    max_chunk: usize,
    expected_url: String,
    audit: Mutex<Audit>,
}
impl std::fmt::Debug for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Operation")
            .field("key", &self.key)
            .field("max_chunk", &self.max_chunk)
            .finish_non_exhaustive()
    }
}
impl Operation {
    pub fn new(
        driver: Arc<dyn NativeHttpSession>,
        key: RequestKey,
        base: &str,
        deadline: Instant,
        max_chunk: usize,
    ) -> Result<Arc<Self>, BridgeError> {
        limits::fixture_base(base).map_err(BridgeError::InvalidInput)?;
        if max_chunk == 0 || max_chunk > limits::MAX_CHUNK || deadline <= Instant::now() {
            return Err(BridgeError::InvalidInput(
                "chunk limit or expired original deadline",
            ));
        }
        let (detached, _) = watch::channel(true);
        let cancellation = CancellationSignal::default();
        driver.bind_cancellation(key, cancellation.clone())?;
        Ok(Arc::new(Self {
            driver,
            key,
            deadline,
            started: Instant::now(),
            cancellation,
            entered: AtomicBool::new(false),
            received: AtomicU64::new(0),
            acknowledged: AtomicU64::new(0),
            parser_yielded: AtomicU64::new(0),
            drain_discarded: AtomicU64::new(0),
            error_consumed: AtomicU64::new(0),
            response_limit: AtomicUsize::new(limits::RESPONSE_BODY),
            eof: AtomicBool::new(false),
            parser_detached: detached,
            read_guard: tokio::sync::Mutex::new(()),
            max_chunk,
            expected_url: format!("{base}/responses"),
            audit: Mutex::new(Audit::default()),
        }))
    }
    pub fn token(&self) -> CancellationToken {
        self.cancellation.token()
    }
    #[cfg(test)]
    pub(crate) fn cancellation_signal(&self) -> CancellationSignal {
        self.cancellation.clone()
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn first_cancel_reason(&self) -> Option<CancelReason> {
        self.cancellation.reason()
    }
    pub fn deliver_if_live<T>(&self, publish: impl FnOnce() -> T) -> Option<T> {
        if Instant::now() >= self.deadline {
            self.cancel(CancelReason::Deadline);
            return None;
        }
        self.cancellation.with_live(publish)
    }
    /// Retain the actual event/permit across a pause. No locks cross this await.
    pub async fn deliver_when_resolved<T>(&self, publish: impl FnOnce() -> T) -> Option<T> {
        if Instant::now() >= self.deadline {
            self.cancel(CancelReason::Deadline);
            return None;
        }
        match self.cancellation.try_delivery(publish) {
            Ok(value) => value,
            Err(held) => {
                self.resolve_delivery_pause().await;
                // The pause is terminal-only. Never execute held after it.
                drop(held);
                None
            }
        }
    }
    pub async fn resolve_delivery_pause(&self) {
        tokio::select! { biased;
            _ = self.cancellation.resolve_pause() => {},
            _ = tokio::time::sleep_until(self.deadline) => self.cancel(CancelReason::Deadline),
        }
    }
    pub fn observe_core_completion(&self, end_turn: Option<bool>) {
        self.audit.lock().unwrap().core_completion = Some(CoreCompletion { end_turn });
    }
    pub fn audit(&self) -> Audit {
        let mut audit = self.audit.lock().unwrap().clone();
        audit.received_bytes = self.received.load(Ordering::Acquire);
        audit.parser_yielded_bytes = self.parser_yielded.load(Ordering::Acquire);
        audit.drain_discarded_bytes = self.drain_discarded.load(Ordering::Acquire);
        audit.error_body_consumed_bytes = self.error_consumed.load(Ordering::Acquire);
        audit.ack_queued_offset = self.acknowledged.load(Ordering::Acquire);
        audit.first_cancel_reason = self.cancellation.reason();
        audit
    }
    pub fn note(&self, kind: &'static str, bytes: &[u8]) {
        let mut audit = self.audit.lock().unwrap();
        if audit.observations.len() >= 4096 {
            audit.overflow = true;
            return;
        }
        audit.observations.push(Observation {
            kind,
            elapsed_us: self.started.elapsed().as_micros().min(u64::MAX as u128) as u64,
            offset: self.received.load(Ordering::Acquire),
            bytes: bytes.len(),
            sha256: if bytes.is_empty() {
                None
            } else {
                Some(format!("{:x}", Sha256::digest(bytes)))
            },
        });
    }
    pub fn cancel(&self, reason: CancelReason) {
        self.cancellation.cancel(reason);
        let first_reason = self.cancellation.reason().unwrap();
        {
            let mut audit = self.audit.lock().unwrap();
            if audit.first_cancel_reason.is_some() {
                return;
            }
            audit.first_cancel_reason = Some(first_reason);
        }
        self.note("cancel_requested_not_release", &[]);
        if let Err(error) = self.driver.cancel(self.key, first_reason) {
            self.audit.lock().unwrap().cancel_queue_error = Some(error);
        }
    }
    async fn bounded<T>(&self, future: DriverFuture<'_, T>) -> Result<T, BridgeError> {
        tokio::select! { biased;
            _ = self.cancellation.cancelled() => Err(BridgeError::Cancelled),
            _ = tokio::time::sleep_until(self.deadline) => { self.cancel(CancelReason::Deadline); Err(BridgeError::Deadline) },
            result = future => result,
        }
    }
    fn prepared(&self, request: Request) -> Result<PreparedHttpRequest, BridgeError> {
        if request.method != http::Method::POST || request.url != self.expected_url {
            return Err(BridgeError::InvalidInput(
                "only the fixed fixture POST target",
            ));
        }
        let limit = request
            .response_body_limit_bytes
            .unwrap_or(limits::RESPONSE_BODY)
            .min(limits::RESPONSE_BODY);
        if limit == 0 {
            return Err(BridgeError::InvalidInput("zero response budget"));
        }
        self.response_limit.store(limit, Ordering::Release);
        let prepared = request
            .prepare_body_for_send()
            .map_err(|_| BridgeError::InvalidInput("prepared request body"))?;
        let body = prepared.body_bytes();
        if body.len() > limits::REQUEST_BODY {
            return Err(BridgeError::InvalidInput("prepared body exceeds32KiB"));
        }
        validate_headers(&prepared.headers)?;
        for name in prepared.headers.keys() {
            let name = name.as_str();
            if matches!(
                name,
                "authorization" | "cookie" | "host" | "content-length" | "transfer-encoding"
            ) || name.starts_with("proxy-")
            {
                return Err(BridgeError::InvalidInput(
                    "forbidden credential or framing header",
                ));
            }
        }
        self.note("core_prepared_request", &body);
        Ok(PreparedHttpRequest {
            method: request.method.to_string(),
            url: request.url,
            headers: prepared
                .headers
                .iter()
                .map(|(name, value)| (name.as_str().to_owned(), value.as_bytes().to_vec()))
                .collect(),
            body_sha256: Sha256::digest(&body).into(),
            body,
            response_limit: limit,
        })
    }
    async fn open(self: &Arc<Self>, request: Request) -> Result<StreamResponse, TransportError> {
        if self.entered.swap(true, Ordering::AcqRel) {
            return Err(transport(BridgeError::DuplicateRequest));
        }
        let prepared = self.prepared(request).map_err(transport)?;
        let proposal = self
            .bounded(self.driver.prepare(self.key, prepared))
            .await
            .map_err(transport)?;
        self.note("proposal_not_approval", &[]);
        let approved = self
            .bounded(self.driver.await_approval(self.key, proposal))
            .await
            .map_err(transport)?;
        if approved.response_limit == 0
            || approved.response_limit > self.response_limit.load(Ordering::Acquire)
        {
            return Err(transport(BridgeError::Protocol(
                "approval response cap must not expand proposal",
            )));
        }
        self.response_limit
            .store(approved.response_limit, Ordering::Release);
        self.note("host_explicit_approval_observed", &[]);
        let head = self
            .bounded(self.driver.commit(self.key, approved))
            .await
            .map_err(transport)?;
        self.note("response_head", &[]);
        if let Err(error) = validate_headers(&head.headers) {
            self.cancel(CancelReason::InvalidResponse);
            return Err(transport(error));
        }
        if !head.status.is_success() {
            let mut diagnostic = Vec::new();
            loop {
                let max = (limits::ERROR_BODY + 1 - diagnostic.len()).min(self.max_chunk);
                match self.read_next(max).await {
                    Ok(Some(bytes)) => {
                        self.error_consumed
                            .fetch_add(bytes.len() as u64, Ordering::AcqRel);
                        if diagnostic.len() + bytes.len() > limits::ERROR_BODY {
                            self.cancel(CancelReason::ErrorBodyLimit);
                            break;
                        }
                        diagnostic.extend_from_slice(&bytes);
                    }
                    Ok(None) => break,
                    Err(_) => {
                        self.cancel(CancelReason::CoreError);
                        break;
                    }
                }
            }
            return Err(TransportError::Http {
                status: head.status,
                url: Some(self.expected_url.clone()),
                headers: Some(head.headers),
                body: String::from_utf8(diagnostic).ok(),
                retry_after: None,
            });
        }
        if let Err(error) = validate_sse_headers(&head.headers) {
            self.cancel(CancelReason::InvalidResponse);
            return Err(transport(error));
        }
        self.parser_detached.send_replace(false);
        let stream = futures::stream::unfold((self.clone(), false), |(op, done)| async move {
            if done {
                return None;
            }
            match op.read_next(op.max_chunk).await {
                Ok(Some(bytes)) => {
                    let delivered = op
                        .deliver_when_resolved(|| {
                            op.parser_yielded
                                .fetch_add(bytes.len() as u64, Ordering::AcqRel);
                            op.note("actual_parser_input_chunk", &bytes);
                            bytes
                        })
                        .await;
                    delivered.map(|bytes| (Ok(bytes), (op, false)))
                }
                Ok(None) => None,
                Err(error) => {
                    op.cancel(CancelReason::InvalidResponse);
                    Some((Err(transport(error)), (op, true)))
                }
            }
        })
        .boxed();
        Ok(StreamResponse {
            status: head.status,
            headers: head.headers,
            bytes: Box::pin(BodyStream {
                inner: Some(stream),
                operation: self.clone(),
            }),
        })
    }
    async fn read_next(&self, max: usize) -> Result<Option<bytes::Bytes>, BridgeError> {
        let _guard = self
            .bounded(Box::pin(async { Ok(self.read_guard.lock().await) }))
            .await?;
        if self.cancellation.is_cancelled() {
            return Err(BridgeError::Cancelled);
        }
        if self.eof.load(Ordering::Acquire) {
            return Ok(None);
        }
        let offset = self.received.load(Ordering::Acquire);
        if offset > self.acknowledged.load(Ordering::Acquire) {
            self.driver.acknowledge(
                self.key,
                ConsumptionProgress {
                    consumed_offset: offset,
                    parser_yielded_bytes: self.parser_yielded.load(Ordering::Acquire),
                    drain_discarded_bytes: self.drain_discarded.load(Ordering::Acquire),
                    error_body_consumed_bytes: self.error_consumed.load(Ordering::Acquire),
                },
            )?;
            self.acknowledged.store(offset, Ordering::Release);
            self.note("absolute_ack_queued_not_server_confirmed", &[]);
        }
        match self
            .bounded(self.driver.read(self.key, offset, max))
            .await?
        {
            ReadResult::Eof { offset: end } if end == offset => {
                self.eof.store(true, Ordering::Release);
                self.note("http_eof", &[]);
                Ok(None)
            }
            ReadResult::Chunk(chunk)
                if chunk.offset == offset
                    && !chunk.bytes.is_empty()
                    && chunk.bytes.len() <= max =>
            {
                let end = offset
                    .checked_add(chunk.bytes.len() as u64)
                    .ok_or(BridgeError::Protocol("offset overflow"))?;
                if end > self.response_limit.load(Ordering::Acquire) as u64 {
                    return Err(BridgeError::Protocol("response byte quota"));
                }
                self.received.store(end, Ordering::Release);
                Ok(Some(chunk.bytes))
            }
            _ => Err(BridgeError::Protocol(
                "chunk/EOF offset, empty chunk or read bound",
            )),
        }
    }
    pub async fn drain_after_completed(&self) -> Result<(), BridgeError> {
        self.note("core_completed_before_transport_cleanup", &[]);
        let mut detached = self.parser_detached.subscribe();
        while !*detached.borrow_and_update() {
            self.bounded(Box::pin(async {
                detached.changed().await.map_err(|_| BridgeError::Unknown)
            }))
            .await?;
        }
        while let Some(bytes) = self.read_next(self.max_chunk).await? {
            self.drain_discarded
                .fetch_add(bytes.len() as u64, Ordering::AcqRel);
            self.note("bounded_post_completed_drain", &bytes);
        }
        Ok(())
    }
    pub async fn cleanup(&self) -> Result<CleanupReceipt, BridgeError> {
        // Cleanup observation may outlive authorization. It cannot authorize
        // fresh data or extend the original HTTP operation deadline.
        tokio::time::timeout(
            Duration::from_millis(limits::CLEANUP_GRACE_MS),
            self.driver.cleanup(self.key),
        )
        .await
        .map_err(|_| BridgeError::CleanupUnconfirmed)?
    }
}

fn validate_headers(headers: &http::HeaderMap) -> Result<(), BridgeError> {
    if headers.len() > limits::HEADER_COUNT
        || headers.keys().any(|n| n.as_str().len() > 128)
        || headers
            .iter()
            .map(|(n, v)| n.as_str().len() + v.as_bytes().len() + 4)
            .sum::<usize>()
            > limits::HEADER_BYTES
    {
        Err(BridgeError::InvalidInput("header byte/count limit"))
    } else {
        Ok(())
    }
}
fn transport(error: BridgeError) -> TransportError {
    TransportError::Build(format!("host-native:{error}"))
}

fn validate_sse_headers(headers: &http::HeaderMap) -> Result<(), BridgeError> {
    let mut values = headers.get_all(http::header::CONTENT_TYPE).iter();
    let valid = values
        .next()
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(';')
                .next()
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("text/event-stream"))
        });
    if !valid || values.next().is_some() {
        return Err(BridgeError::InvalidInput(
            "Responses requires one text/event-stream content type",
        ));
    }
    // No decompression layer exists in this adapter. Preserve approved raw bytes
    // and refuse encodings we cannot pass directly to the real SSE parser.
    if headers
        .get_all(http::header::CONTENT_ENCODING)
        .iter()
        .any(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"))
    {
        return Err(BridgeError::InvalidInput("encoded response unsupported"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn paused_gate_retains_received_event_and_reserved_permit_until_host_cancel() {
        let op = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(1),
            "http://127.0.0.1:12345/v1",
            Instant::now() + Duration::from_secs(1),
            1024,
        )
        .unwrap();
        let signal = op.cancellation_signal();
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tx.send("actual queued local event").await.unwrap();
        let event = rx.recv().await.unwrap();
        let permit = tx.reserve().await.unwrap();
        signal.pause_delivery();
        let mut consumer = Box::pin(op.deliver_when_resolved(|| event));
        let mut producer =
            Box::pin(op.deliver_when_resolved(|| permit.send("reserved local event")));
        assert!(futures::poll!(&mut consumer).is_pending());
        assert!(futures::poll!(&mut producer).is_pending());
        assert!(
            tx.try_reserve().is_err(),
            "original permit must remain held"
        );
        assert_eq!(signal.reason(), None);
        assert!(!signal.is_cancelled());
        let cause = signal.cancel_observed(CancelReason::HostCancelled);
        assert!(
            !cause.cancel_gate_before && cause.cancel_gate_after && cause.host_control_closed_gate
        );
        assert!(cause.delivery_paused_before);
        assert!(consumer.await.is_none());
        assert!(producer.await.is_none());
        assert!(rx.try_recv().is_err());
        assert!(tx.try_reserve().is_ok());
        assert_eq!(op.first_cancel_reason(), Some(CancelReason::HostCancelled));
    }
    #[tokio::test]
    async fn pause_between_future_creation_and_gate_poll_does_not_publish_or_set_reason() {
        let op = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(1),
            "http://127.0.0.1:12345/v1",
            Instant::now() + Duration::from_secs(1),
            1024,
        )
        .unwrap();
        let signal = op.cancellation_signal();
        let published = AtomicBool::new(false);
        let future = op.deliver_when_resolved(|| published.store(true, Ordering::SeqCst));
        signal.pause_delivery();
        let mut future = Box::pin(future);
        assert!(futures::poll!(&mut future).is_pending());
        assert_eq!(signal.reason(), None);
        signal.cancel(CancelReason::NativeFailure);
        let cause = signal.cancel_observed(CancelReason::HostCancelled);
        assert!(!cause.host_control_closed_gate);
        assert_eq!(
            cause.first_cancel_reason_after,
            Some(CancelReason::NativeFailure)
        );
        assert!(future.await.is_none());
        assert!(!published.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn unresolved_pause_cannot_extend_original_operation_deadline() {
        let op = Operation::new(
            Arc::new(WireNotReady),
            RequestKey(1),
            "http://127.0.0.1:12345/v1",
            Instant::now() + Duration::from_millis(20),
            1024,
        )
        .unwrap();
        op.cancellation_signal().pause_delivery();
        assert!(
            op.deliver_when_resolved(|| panic!("paused publish"))
                .await
                .is_none()
        );
        assert_eq!(op.first_cancel_reason(), Some(CancelReason::Deadline));
    }
    #[test]
    fn ambiguous_or_compressed_sse_head_is_rejected() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        assert!(validate_sse_headers(&headers).is_ok());
        headers.append(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        assert!(validate_sse_headers(&headers).is_err());
        headers.remove(http::header::CONTENT_TYPE);
        headers.insert(
            http::header::CONTENT_TYPE,
            http::HeaderValue::from_static("text/event-stream"),
        );
        headers.insert(
            http::header::CONTENT_ENCODING,
            http::HeaderValue::from_static("gzip"),
        );
        assert!(validate_sse_headers(&headers).is_err());
    }
}

struct BodyStream {
    inner: Option<BoxStream<'static, Result<bytes::Bytes, TransportError>>>,
    operation: Arc<Operation>,
}
impl Stream for BodyStream {
    type Item = Result<bytes::Bytes, TransportError>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().unwrap().as_mut().poll_next(cx)
    }
}
impl Drop for BodyStream {
    fn drop(&mut self) {
        drop(self.inner.take());
        self.operation.note("parser_detached_not_user_cancel", &[]);
        self.operation.parser_detached.send_replace(true);
    }
}

#[derive(Debug)]
pub struct HostModelNetworkBackend {
    pub operation: Arc<Operation>,
}
impl ModelNetworkBackend for HostModelNetworkBackend {
    fn execute(&self, _: Request) -> NetworkFuture<'_, Result<Response, TransportError>> {
        Box::pin(async { Err(transport(BridgeError::Unsupported)) })
    }
    fn stream(
        &self,
        request: Request,
    ) -> NetworkFuture<'_, Result<StreamResponse, TransportError>> {
        Box::pin(async move { self.operation.open(request).await })
    }
    fn connect_websocket(
        &self,
        _: WebsocketConnectRequest,
    ) -> NetworkFuture<'_, Result<ResponsesWebsocketConnection, ApiError>> {
        // Never manufacture426, which would activate Core's HTTP fallback.
        Box::pin(async { Err(ApiError::Transport(transport(BridgeError::Unsupported))) })
    }
}
