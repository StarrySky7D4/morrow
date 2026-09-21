//! Real HTTP barriers; no synthetic sleep stands in for transport progress.
use super::gated_http::GatedServer;
use super::*;

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
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
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
    assert_eq!(server.calls.load(Ordering::SeqCst), 1);
    server.finish().await;
}
