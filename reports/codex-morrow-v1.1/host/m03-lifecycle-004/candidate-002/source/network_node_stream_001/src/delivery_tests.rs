//! Deterministic delivery-boundary tests. The queued bytes represent an already
//! completed worker send; these are not HTTP or OS-pipe qualification claims.
use super::*;

fn queued(deadline: Instant) -> StreamLease {
    let (tx, rx) = oneshot::channel();
    tx.send(Ok(Some(Bytes::from_static(b"queued-before-revoke"))))
        .unwrap();
    let (demands, _) = mpsc::channel(1);
    let (_, terminal) = watch::channel(None);
    StreamLease {
        head: None,
        deadline,
        cancel: CancellationToken::new(),
        guard: None,
        demands,
        pending: Some(rx),
        terminal,
        worker: None,
        eof_seen: false,
    }
}

#[tokio::test]
async fn queued_chunk_is_not_delivered_after_cancel() {
    let mut lease = queued(Instant::now() + Duration::from_secs(1));
    lease.cancel();
    assert_eq!(lease.next_chunk().await, Err(Error::Cancelled));
    assert!(lease.pending.is_none(), "queued payload must be released");
}

#[tokio::test]
async fn queued_chunk_is_not_delivered_after_original_deadline() {
    let mut lease = queued(Instant::now() - Duration::from_millis(1));
    assert_eq!(lease.next_chunk().await, Err(Error::Timeout));
    assert!(lease.pending.is_none(), "expired payload must be released");
}

struct LiveGuard(std::sync::atomic::AtomicBool);
impl StreamGuard for LiveGuard {
    fn check(&self) -> Result<()> {
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            Ok(())
        } else {
            Err(Error::Cancelled)
        }
    }
}

#[tokio::test]
async fn queued_chunk_rechecks_guard_and_revocation_is_sticky() {
    let guard = Arc::new(LiveGuard(std::sync::atomic::AtomicBool::new(false)));
    let mut lease = queued(Instant::now() + Duration::from_secs(1));
    lease.guard = Some(guard.clone());
    assert_eq!(lease.next_chunk().await, Err(Error::Cancelled));
    guard.0.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(lease.next_chunk().await, Err(Error::Cancelled));
    assert!(lease.pending.is_none());
}

#[tokio::test]
async fn cancellation_during_pending_wait_is_checked_before_delivery() {
    let mut lease = queued(Instant::now() + Duration::from_secs(1));
    let (tx, rx) = oneshot::channel();
    lease.pending = Some(rx);
    let cancel = lease.cancel.clone();
    let waiting = lease.next_chunk();
    tokio::pin!(waiting);
    assert!(
        std::future::poll_fn(|cx| std::task::Poll::Ready(waiting.as_mut().poll(cx)))
            .await
            .is_pending()
    );
    cancel.cancel();
    tx.send(Ok(Some(Bytes::from_static(b"late")))).unwrap();
    assert_eq!(waiting.await, Err(Error::Cancelled));
}

#[tokio::test]
async fn live_delivery_and_previously_observed_eof_are_preserved() {
    let mut lease = queued(Instant::now() + Duration::from_secs(1));
    assert_eq!(
        lease.next_chunk().await.unwrap().unwrap(),
        b"queued-before-revoke"[..]
    );
    lease.eof_seen = true;
    lease.cancel();
    assert_eq!(lease.next_chunk().await, Ok(None));
}
