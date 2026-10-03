//! Pull-driven body worker. This is trusted transport, not native grant authority.
use crate::{Error, RawHttpRequest, Result, client::Client};
use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use std::{future::Future, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    sync::{OwnedSemaphorePermit, mpsc, oneshot, watch},
    task::JoinHandle,
    time::Instant,
};
use tokio_util::sync::CancellationToken;
use url::Url;

pub const MAX_DELIVERY_CHUNK: usize = 8 * 1024;

/// A synchronous trusted-owner liveness check. Never performs network I/O or renews time.
/// Revocation notifications should additionally cancel the context token for prompt wakeup.
pub trait StreamGuard: Send + Sync {
    fn check(&self) -> Result<()>;
}

/// Not serializable and not a guest capability. A native adapter must supply its own guard.
pub struct SendContext {
    deadline: Instant,
    cancel: CancellationToken,
    guard: Option<Arc<dyn StreamGuard>>,
}
impl SendContext {
    pub fn trusted(deadline: Instant, cancel: CancellationToken) -> Self {
        Self {
            deadline,
            cancel: cancel.child_token(),
            guard: None,
        }
    }
    pub fn guarded(
        deadline: Instant,
        cancel: CancellationToken,
        guard: Arc<dyn StreamGuard>,
    ) -> Self {
        Self {
            deadline,
            cancel: cancel.child_token(),
            guard: Some(guard),
        }
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn cap_deadline(&mut self, cap: Instant) {
        self.deadline = self.deadline.min(cap);
    }
    pub(crate) fn check(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= self.deadline {
            return Err(Error::Timeout);
        }
        if let Some(guard) = &self.guard {
            guard.check()?;
        }
        Ok(())
    }
    /// One fixed absolute deadline across DNS, headers, demand waits, body and cleanup trigger.
    pub(crate) async fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        self.check()?;
        tokio::pin!(future);
        let mut guard_tick = tokio::time::interval(Duration::from_millis(5));
        guard_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return Err(Error::Cancelled),
                _ = tokio::time::sleep_until(self.deadline) => return Err(Error::Timeout),
                _ = guard_tick.tick() => self.check()?,
                result = &mut future => { self.check()?; return result; }
            }
        }
    }
}

// No Debug: headers can contain private data. Values remain raw bytes.
#[derive(Clone, PartialEq, Eq)]
pub struct ResponseHead {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub remote_addr: SocketAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub outcome: Result<()>,
    pub received_bytes: usize,
    pub delivered_bytes: usize,
    pub network_polls: usize,
    pub peak_upstream_chunk: usize,
    pub http_eof: bool,
    /// True only after this layer's worker JoinHandle resolved; no claim about remote effects.
    pub worker_joined: bool,
}
impl Default for Completion {
    fn default() -> Self {
        Self {
            outcome: Ok(()),
            received_bytes: 0,
            delivered_bytes: 0,
            network_polls: 0,
            peak_upstream_chunk: 0,
            http_eof: false,
            worker_joined: false,
        }
    }
}
/// Opening failed before a lease could be returned. A Worker receipt retains
/// the original join flag; it never promotes an unresolved/panicked worker to success.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpeningCleanup {
    NoWorker,
    Worker(Completion),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningError {
    pub cause: Error,
    pub cleanup: OpeningCleanup,
}
impl OpeningError {
    pub(crate) fn no_worker(cause: Error) -> Self {
        Self { cause, cleanup: OpeningCleanup::NoWorker }
    }
}
impl std::fmt::Display for OpeningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "stream opening: {}", self.cause)
    }
}
impl std::error::Error for OpeningError {}

type Chunk = Result<Option<Bytes>>;
type Demand = oneshot::Sender<Chunk>;
struct WorkerExit {
    completion: Completion,
    // Kept even after EOF until the lease joins/drops the completed worker output.
    _permit: OwnedSemaphorePermit,
}

#[cfg(test)]
#[path = "delivery_tests.rs"]
mod delivery_tests;

/// Owns the only body receiver, cancellation, original deadline, delivery guard,
/// worker and concurrency permit (in worker/output). Returning headers releases none.
pub struct StreamLease {
    head: Option<ResponseHead>,
    deadline: Instant,
    cancel: CancellationToken,
    guard: Option<Arc<dyn StreamGuard>>,
    demands: mpsc::Sender<Demand>,
    pending: Option<oneshot::Receiver<Chunk>>,
    terminal: watch::Receiver<Option<Completion>>,
    worker: Option<JoinHandle<WorkerExit>>,
    eof_seen: bool,
}
impl StreamLease {
    // Worker send and caller delivery are different boundaries. An already queued
    // oneshot payload must not survive cancellation, expiry or authority revocation.
    pub(crate) fn check_delivery(&mut self) -> Result<()> {
        let result = if self.cancel.is_cancelled() {
            Err(Error::Cancelled)
        } else if Instant::now() >= self.deadline {
            Err(Error::Timeout)
        } else if let Some(guard) = &self.guard {
            guard.check()
        } else {
            Ok(())
        };
        if result.is_err() {
            self.pending = None;
            self.cancel.cancel();
        }
        result
    }
    pub fn head(&self) -> &ResponseHead {
        self.head.as_ref().expect("head installed before return")
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn terminal(&self) -> Option<Completion> {
        self.terminal.borrow().clone()
    }
    pub fn cancel(&self) {
        self.cancel.cancel();
    }
    async fn closed_result(&mut self) -> Chunk {
        loop {
            if let Some(done) = self.terminal.borrow().as_ref() {
                return done.outcome.map(|()| None);
            }
            if self.terminal.changed().await.is_err() {
                return Err(Error::Closed);
            }
        }
    }

    /// No prefetch: one demand at a time, at most 8KiB delivered. Cancellation-safe:
    /// dropping this future preserves its pending receiver for the next call.
    pub async fn next_chunk(&mut self) -> Chunk {
        if self.eof_seen {
            return Ok(None);
        }
        self.check_delivery()?;
        if self.pending.is_none() {
            if let Some(done) = self.terminal.borrow().as_ref() {
                return done.outcome.map(|()| None);
            }
            let (tx, rx) = oneshot::channel();
            if self.demands.try_send(tx).is_err() {
                return self.closed_result().await;
            }
            self.pending = Some(rx);
        }
        let result = self.pending.as_mut().expect("pending demand").await;
        self.pending = None;
        let chunk = match result {
            Ok(chunk) => chunk,
            Err(_) => self.closed_result().await,
        };
        if chunk.is_ok() {
            self.check_delivery()?;
        }
        if matches!(chunk, Ok(None)) {
            self.eof_seen = true;
        }
        chunk
    }
    /// If EOF was not reached, finishing requests cancellation; it never drains silently.
    pub async fn finish(mut self) -> Completion {
        if self.terminal.borrow().is_none() {
            self.cancel.cancel();
        }
        match self.worker.take().expect("single worker").await {
            Ok(mut exit) => {
                exit.completion.worker_joined = true;
                exit.completion
            }
            Err(_) => Completion {
                outcome: Err(Error::Closed),
                ..Completion::default()
            },
        }
    }
    pub async fn cancel_and_wait(self) -> Completion {
        self.cancel.cancel();
        self.finish().await
    }
}
impl Drop for StreamLease {
    fn drop(&mut self) {
        // Async cleanup remains owned by the worker. Dropping a handle is not a join receipt.
        self.cancel.cancel();
    }
}

pub(crate) async fn start_with_receipt(
    client: Client,
    target: Url,
    request: RawHttpRequest,
    headers: HeaderMap,
    context: SendContext,
    permit: OwnedSemaphorePermit,
) -> std::result::Result<StreamLease, OpeningError> {
    let (head_tx, head_rx) = oneshot::channel();
    let (demand_tx, demand_rx) = mpsc::channel(1);
    let (terminal_tx, terminal_rx) = watch::channel(None);
    let cancel = context.cancel.clone();
    let deadline = context.deadline;
    let guard = context.guard.clone();
    let worker = tokio::spawn(async move {
        let mut completion = Completion::default();
        completion.outcome = run_worker(
            client,
            target,
            request,
            headers,
            &context,
            head_tx,
            demand_rx,
            &mut completion,
        )
        .await;
        // Send terminal before dropping demand receiver so waiting callers see the cause.
        terminal_tx.send_replace(Some(completion.clone()));
        WorkerExit {
            completion,
            _permit: permit,
        }
    });
    // Construct before waiting for headers: cancel/drop of send_stream cancels the worker.
    let mut lease = StreamLease {
        head: None,
        deadline,
        cancel,
        guard,
        demands: demand_tx,
        pending: None,
        terminal: terminal_rx,
        worker: Some(worker),
        eof_seen: false,
    };
    match head_rx.await.unwrap_or(Err(Error::Closed)) {
        Ok(head) => {
            if let Err(error) = lease.check_delivery() {
                let receipt = lease.cancel_and_wait().await;
                return Err(OpeningError { cause: error, cleanup: OpeningCleanup::Worker(receipt) });
            }
            lease.head = Some(head);
            Ok(lease)
        }
        Err(error) => {
            let receipt = lease.cancel_and_wait().await;
            Err(OpeningError { cause: error, cleanup: OpeningCleanup::Worker(receipt) })
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_worker(
    client: Client,
    target: Url,
    request: RawHttpRequest,
    headers: HeaderMap,
    context: &SendContext,
    head_tx: oneshot::Sender<Result<ResponseHead>>,
    mut demands: mpsc::Receiver<Demand>,
    completion: &mut Completion,
) -> Result<()> {
    let (head, response) = match context
        .run(client.execute_head(target, request, headers, context))
        .await
    {
        Ok(pair) => pair,
        Err(error) => {
            let _ = head_tx.send(Err(error));
            return Err(error);
        }
    };
    head_tx.send(Ok(head)).map_err(|_| Error::Cancelled)?;
    let mut body = Box::pin(response.bytes_stream());
    let mut pending = Bytes::new();
    loop {
        let demand = context
            .run(async { demands.recv().await.ok_or(Error::Cancelled) })
            .await?;
        let chunk = context
            .run(async {
                while pending.is_empty() {
                    completion.network_polls += 1;
                    match body.next().await {
                        None => {
                            completion.http_eof = true;
                            return Ok(None);
                        }
                        Some(Err(_)) => return Err(Error::Transport),
                        Some(Ok(bytes)) => {
                            completion.peak_upstream_chunk =
                                completion.peak_upstream_chunk.max(bytes.len());
                            let total = completion
                                .received_bytes
                                .checked_add(bytes.len())
                                .ok_or(Error::Limit)?;
                            if total > client.response_limit() {
                                return Err(Error::Limit);
                            }
                            completion.received_bytes = total;
                            pending = bytes;
                        }
                    }
                }
                context.check()?;
                let output = pending.split_to(pending.len().min(MAX_DELIVERY_CHUNK));
                Ok(Some(output))
            })
            .await;
        let terminal = match &chunk {
            Ok(Some(_)) => None,
            Ok(None) => Some(Ok(())),
            Err(e) => Some(Err(*e)),
        };
        let length = chunk
            .as_ref()
            .ok()
            .and_then(|c| c.as_ref())
            .map_or(0, Bytes::len);
        demand.send(chunk).map_err(|_| Error::Cancelled)?;
        completion.delivered_bytes += length;
        if let Some(result) = terminal {
            return result;
        }
    }
}
