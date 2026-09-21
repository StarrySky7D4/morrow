use super::gated_http::GatedServer;
use super::*;
use morrow_network_node::{
    Error,
    managed_http::{HttpRouteSet, MAX_SERVICE_ENDPOINTS},
};

fn submit(run: &Running, routes: &HttpRouteSet, input: &HttpSubmission) -> (Request, JobHandle) {
    let request = Request::encode_http_submit(1, input).unwrap();
    let handle = run
        .worker
        .submit_brokered(request.bytes().to_vec(), Box::new(routes.clone()), WAIT)
        .unwrap();
    (request, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_live_endpoints_route_exactly_and_keep_deferred_owner_lane() {
    let mut first = GatedServer::spawn().await;
    let mut second =
        Server::new(Some(raw_response("201 Created", b"second")), Duration::ZERO).await;
    let mut approved = approval(&first.origin);
    approved.limits.timeout = WAIT;
    let mut run =
        Running::build(approved, false, false, None, Some(approval(&second.origin))).unwrap();
    let other = run.secondary.as_ref().unwrap().clone();
    let routes = HttpRouteSet::new(
        vec![run.endpoint.clone(), other.clone()],
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    let references: BTreeSet<_> = routes.references().collect();
    let expected = [
        run.endpoint.endpoint_reference(),
        other.endpoint_reference(),
    ];
    assert_eq!(references, expected.iter().map(String::as_str).collect());
    let (request1, mut job1) = submit(&run, &routes, &submission(&run.endpoint, "routes-first"));
    first.entered().await;
    let mut command = run
        .worker
        .submit_owner_command(b"routes".to_vec(), 64)
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if let Some(reply) = command.read().unwrap() {
                assert_eq!(reply, b"routes");
                break;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("route set must preserve deferred dispatch");
    assert_eq!(job1.poll(), Poll::Pending);
    first.finish().await;
    let report1 = consume(&mut job1).await;
    assert_eq!(report1.task.execution.outcome, Ok(0));
    let (request2, mut job2) = submit(&run, &routes, &submission(&other, "routes-second"));
    second.request().await;
    let report2 = consume(&mut job2).await;
    let result = report2.http_response.as_ref().unwrap();
    assert_eq!(result.http_status, 201);
    assert_eq!(result.body, b"second");
    assert_eq!(first.calls.load(Ordering::SeqCst), 1);
    assert_eq!(second.calls.load(Ordering::SeqCst), 1);
    let host = run.finish().await;
    assert_observed(&host, &request1, "routes-first", &report1);
    assert_observed(&host, &request2, "routes-second", &report2);
}

#[tokio::test]
async fn route_set_rejects_empty_duplicate_over_bound_and_unselected_reference() {
    let server = Server::new(Some(raw_response("200 OK", b"unexpected")), Duration::ZERO).await;
    let mut run = Running::new(approval(&server.origin), false);
    let runtime = tokio::runtime::Handle::current();
    assert!(matches!(
        HttpRouteSet::new(vec![], runtime.clone()),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        HttpRouteSet::new(vec![run.endpoint.clone(); 2], runtime.clone()),
        Err(Error::Invalid)
    ));
    assert!(matches!(
        HttpRouteSet::new(
            vec![run.endpoint.clone(); MAX_SERVICE_ENDPOINTS + 1],
            runtime.clone()
        ),
        Err(Error::Limit)
    ));
    let routes = HttpRouteSet::new(vec![run.endpoint.clone()], runtime).unwrap();
    let mut input = submission(&run.endpoint, "routes-unselected");
    input.endpoint = b"not-selected".to_vec();
    let (_, mut job) = submit(&run, &routes, &input);
    let report = consume(&mut job).await;
    assert!(report.task.execution.outcome.is_err());
    assert!(!report.unknown);
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    let host = run.finish().await;
    assert!(
        host.store_local()
            .lookup_io_intent(ID, "routes-unselected")
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn foreign_or_revoked_endpoint_does_not_gain_authority_from_membership() {
    let server = Server::new(Some(raw_response("200 OK", b"unexpected")), Duration::ZERO).await;
    let mut run = Running::new(approval(&server.origin), false);
    let mut foreign = Running::new(approval(&server.origin), false);
    let routes = HttpRouteSet::new(
        vec![run.endpoint.clone(), foreign.endpoint.clone()],
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    let (_, mut job) = submit(
        &run,
        &routes,
        &submission(&foreign.endpoint, "routes-foreign"),
    );
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    run.endpoint.revoke();
    let (_, mut job) = submit(&run, &routes, &submission(&run.endpoint, "routes-revoked"));
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    let host = run.finish().await;
    for id in ["routes-foreign", "routes-revoked"] {
        assert!(
            host.store_local()
                .lookup_io_intent(ID, id)
                .unwrap()
                .is_none()
        );
    }
    foreign.finish().await;
}

#[tokio::test]
async fn retained_route_set_cannot_authorize_reconnected_instance_on_same_host() {
    let server = Server::new(Some(raw_response("200 OK", b"unexpected")), Duration::ZERO).await;
    let mut run = Running::new(approval(&server.origin), false);
    let routes = HttpRouteSet::new(
        vec![run.endpoint.clone()],
        tokio::runtime::Handle::current(),
    )
    .unwrap();
    // Reclaim the real owner and disconnect its original managed instance.
    // The old route set remains alive across this boundary.
    let mut host = run.finish().await;
    let original = host.binding();
    let digest = run._manager.selection(ID).unwrap().digest;
    let instance = run._manager.connect(ID, &mut host).unwrap();
    let binding = run
        ._manager
        .bind_io(
            &host,
            &instance,
            digest,
            run._manager.revision(),
            &BTreeSet::from([IoCapability::HttpRequest]),
            100,
            2,
        )
        .unwrap();
    run.worker = IoWorker::spawn_managed_owned(
        &run._manager,
        HttpOwner { original, host },
        instance,
        binding,
        || 2,
        1,
        JobLimits::new(1, 1024 * 1024, 4 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let (_, mut job) = submit(
        &run,
        &routes,
        &submission(&run.endpoint, "routes-reconnected"),
    );
    assert!(consume(&mut job).await.task.execution.outcome.is_err());
    assert_eq!(server.calls.load(Ordering::SeqCst), 0);
    let host = run.finish().await;
    assert_eq!(host.binding(), original);
    assert!(
        host.store_local()
            .lookup_io_intent(ID, "routes-reconnected")
            .unwrap()
            .is_none()
    );
}
