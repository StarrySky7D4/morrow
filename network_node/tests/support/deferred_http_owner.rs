//! Real HTTP barriers; no synthetic sleep stands in for transport progress.
use super::*;
use tokio::sync::oneshot;

struct GatedServer {
    origin: String,
    handle: JoinHandle<()>,
    entered: oneshot::Receiver<()>,
    release: Option<oneshot::Sender<()>>,
}
impl GatedServer {
    async fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let (entered_tx, entered) = oneshot::channel();
        let (release, release_rx) = oneshot::channel();
        let handle = tokio::spawn(async move {
            tokio::time::timeout(WAIT, async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut raw = Vec::new();
                let mut buffer = [0; 1024];
                let end = loop {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    raw.extend_from_slice(&buffer[..n]);
                    assert!(raw.len() <= 128 * 1024);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        break end + 4;
                    }
                };
                // Body is binary; decode headers only.
                let text = std::str::from_utf8(&raw[..end]).unwrap();
                let length: usize = text
                    .lines()
                    .filter_map(|l| l.split_once(':'))
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .map_or(0, |(_, v)| v.trim().parse().unwrap());
                assert!(end + length <= 128 * 1024);
                while raw.len() < end + length {
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert_ne!(n, 0);
                    raw.extend_from_slice(&buffer[..n]);
                    assert!(raw.len() <= 128 * 1024);
                }
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
                let _ = socket.write_all(&raw_response("200 OK", b"ok")).await;
                let _ = socket.shutdown().await;
            })
            .await
            .expect("bounded gated server");
        });
        Self {
            origin,
            handle,
            entered,
            release: Some(release),
        }
    }
    async fn entered(&mut self) {
        tokio::time::timeout(WAIT, &mut self.entered)
            .await
            .unwrap()
            .unwrap();
    }
    async fn finish(&mut self) {
        self.release.take().unwrap().send(()).unwrap();
        tokio::time::timeout(WAIT, &mut self.handle)
            .await
            .unwrap()
            .unwrap();
    }
}
impl Drop for GatedServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn original_owner_commits_during_real_http_wait_and_reopens_same_database() {
    let mut server = GatedServer::spawn().await;
    let mut approved = approval(&server.origin);
    approved.limits.timeout = WAIT;
    let mut run = Running::new(approved, false);
    let operation = "owner-during-http";
    let (request, mut job) = run.submit(&submission(&run.endpoint, operation));
    server.entered().await;
    let mut command = run
        .worker
        .submit_owner_command(b"saved".to_vec(), 64)
        .unwrap();
    let reply = tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(reply) = command.read().unwrap() {
                break reply;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("original owner must run before HTTP is released");
    assert_eq!(reply, b"saved");
    assert_eq!(job.poll(), Poll::Pending);
    // The original job reservation also remains occupied during the wait.
    let competing = run.worker.submit_brokered(
        request.bytes().to_vec(),
        Box::new(run.endpoint.router(tokio::runtime::Handle::current())),
        WAIT,
    );
    assert!(matches!(competing, Err(JobError::Busy)));
    server.finish().await;
    ready(&mut job).await;
    // Ready, but unread, still owns the original job reservation.
    let competing = run.worker.submit_brokered(
        request.bytes().to_vec(),
        Box::new(run.endpoint.router(tokio::runtime::Handle::current())),
        WAIT,
    );
    assert!(matches!(competing, Err(JobError::Busy)));
    let report = consume(&mut job).await;
    assert_eq!(report.task.execution.outcome, Ok(0));
    assert_eq!(report.calls, 1);
    assert!(!report.unknown);
    let host = run.finish().await;
    assert_observed(&host, &request, operation, &report);
    assert_eq!(
        host.store_local().card_ids_local("", 10).unwrap(),
        ["during-http"]
    );
    drop(host);
    let reopened = HostRuntime::new(
        Store::open_existing(&run._dir.path().join("db"), EventBudget::default()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        reopened.store_local().card_ids_local("", 10).unwrap(),
        ["during-http"]
    );
    assert_observed(&reopened, &request, operation, &report);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn owner_panic_joins_transport_and_preserves_committed_content_and_unknown() {
    let mut server = GatedServer::spawn().await;
    let mut approved = approval(&server.origin);
    approved.limits.timeout = WAIT;
    let mut run = Running::new(approved, false);
    let operation = "owner-panic-http";
    let (_, mut job) = run.submit(&submission(&run.endpoint, operation));
    server.entered().await;
    let _command = run
        .worker
        .submit_owner_command(b"panic".to_vec(), 64)
        .unwrap();
    let exit = tokio::time::timeout(WAIT, async {
        loop {
            if let Some(exit) = run.worker.try_reclaim().unwrap() {
                break exit;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("panic must cancel and join pending transport");
    assert!(exit.result.is_err());
    assert_eq!(exit.owner.host.binding(), exit.owner.original);
    assert_eq!(
        exit.owner
            .host
            .store_local()
            .card_ids_local("", 10)
            .unwrap(),
        ["during-http"]
    );
    assert_unknown(&exit.owner.host, operation);
    assert!(job.read(256 * 1024).is_err());
    server.finish().await;
}
